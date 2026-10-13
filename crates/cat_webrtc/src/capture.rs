//! The capture worker: probe, capture, draw the latest boxes, push; when a capture ends it probes again,
//! so an unplugged camera, a crashed `rpicam-vid` or a webcam plugged in later never needs a restart.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use kataglyphis_core::Detection;
use kataglyphis_inference::person_detection::PersonDetector;

use crate::camera::{self, Choice, Found, Plan};
use crate::clip::{Arrival, Clip};
use crate::config::Config;
use crate::frame;
use crate::motion::{still_period, Pacer};
use crate::notify::Notifier;
use crate::presence::{best_score, Change, Presence};
use crate::status::Status;

/// COCO class id of "cat" in the 80-class YOLO models shipped in this tree.
pub const COCO_CAT: i64 = 15;

/// A stand-in test pattern runs this long before the camera is probed again.
const REPROBE_EVERY: Duration = Duration::from_secs(30);
/// A camera that delivers no frame for this long counts as gone.
const STALL_AFTER: Duration = Duration::from_secs(10);
/// One pull's wait, which bounds how fast shutdown, errors and stalls are noticed.
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_mseconds(500);

type Boxes = Arc<Mutex<Vec<Detection>>>;

/// Everything the worker thread owns.
pub struct Worker {
    pub config: Arc<Config>,
    pub choice: Choice,
    pub appsrc: gst_app::AppSrc,
    pub detector: Option<PersonDetector>,
    pub status: Arc<Status>,
    pub shutdown: Arc<AtomicBool>,
    pub notifier: Option<Notifier>,
    pub clip: Option<Arc<Clip>>,
}

impl Worker {
    pub fn spawn(self) -> std::io::Result<thread::JoinHandle<()>> {
        thread::Builder::new()
            .name("cat-capture".into())
            .spawn(move || self.run())
    }

    fn run(mut self) {
        let boxes: Boxes = Arc::new(Mutex::new(Vec::new()));
        let notifier = self.notifier.take();
        let inference = self.detector.take().and_then(|detector| {
            spawn_inference(
                detector,
                &self.config,
                boxes.clone(),
                self.status.clone(),
                notifier,
                self.clip.clone(),
            )
        });
        let frames = inference.as_ref().map(|(tx, _)| tx);
        let mut failures: u32 = 0;
        while !self.shutdown.load(Ordering::Relaxed) {
            let found = camera::probe(&self.choice);
            let plan = camera::choose(&self.choice, &found.probe);
            let label = plan.describe(&found.probe);
            self.status.set_camera(&label, plan.is_stand_in());
            log::info!("camera: {label}");
            let started = Instant::now();
            let deadline = plan.is_stand_in().then(|| started + REPROBE_EVERY);
            let result = self.capture(&plan, &found, &boxes, frames, deadline);
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            match &result {
                Err(err) => log::warn!("capture from {label} stopped: {err:#}"),
                Ok(()) if !plan.is_stand_in() => log::info!("capture from {label} ended"),
                Ok(()) => {}
            }
            if started.elapsed() > Duration::from_secs(60) {
                failures = 0;
            }
            // A stand-in that ran its course re-probes at once; anything else backs off 1 s .. 32 s.
            if result.is_err() || !plan.is_stand_in() {
                let pause = Duration::from_secs(1 << failures.min(5));
                failures = failures.saturating_add(1);
                self.sleep(pause);
            }
        }
        // Finish the frame in flight: an ORT session still running at process exit errors out.
        if let Some((tx, handle)) = inference {
            drop(tx);
            let _ = handle.join();
        }
        log::info!("capture worker stopped");
    }

    fn sleep(&self, total: Duration) {
        let until = Instant::now() + total;
        while Instant::now() < until && !self.shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn capture(
        &self,
        plan: &Plan,
        found: &Found,
        boxes: &Boxes,
        inference: Option<&mpsc::SyncSender<Vec<u8>>>,
        deadline: Option<Instant>,
    ) -> anyhow::Result<()> {
        let config = &self.config;
        let mut source = Source::build(plan, found, config)?;
        let pipeline = gst::Pipeline::new();
        let mut elements = std::mem::take(&mut source.elements);
        elements.push(make("videoconvert")?);
        let mut turned = self.status.rotation();
        let flip = videoflip(turned)?;
        elements.push(flip.clone());
        elements.push(make("videoscale")?);
        elements.push(capsfilter(frame::caps(config))?);
        let sink = gst::ElementFactory::make("appsink")
            .property("max-buffers", 1u32)
            .property("drop", true)
            // Only the non-live still-image loop needs the sink to pace it.
            .property("sync", matches!(plan, Plan::Image(_)))
            .build()
            .context("appsink")?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| anyhow!("appsink is not an AppSink"))?;
        elements.push(sink.clone().upcast());
        pipeline
            .add_many(&elements)
            .context("add capture elements")?;
        gst::Element::link_many(&elements).context("link capture pipeline")?;
        let _running = Running {
            pipeline: pipeline.clone(),
            source,
        };
        pipeline
            .set_state(gst::State::Playing)
            .context("capture pipeline to PLAYING")?;

        let bus = pipeline.bus().context("capture pipeline has no bus")?;
        let mut last_frame = Instant::now();
        let mut pacer = Pacer::new(config.inference_fps, config.idle_inference_fps, last_frame);
        loop {
            if self.shutdown.load(Ordering::Relaxed)
                || deadline.is_some_and(|d| Instant::now() >= d)
            {
                return Ok(());
            }
            if bus_says_done(&bus)? {
                return Ok(());
            }
            let Some(sample) = sink.try_pull_sample(PULL_TIMEOUT) else {
                // A failing source posts its error and then sends EOS: report the error, not a clean end.
                if sink.is_eos() {
                    bus_says_done(&bus)?;
                    return Ok(());
                }
                if last_frame.elapsed() > STALL_AFTER {
                    bail!("no frame for {} s", STALL_AFTER.as_secs());
                }
                continue;
            };
            last_frame = Instant::now();
            // The page turned the picture: videoflip follows at once, and videoscale keeps the size.
            let wanted = self.status.rotation();
            if wanted != turned {
                flip.set_property_from_str("method", flip_method(wanted)?);
                turned = wanted;
            }
            if !self.push(&sample, boxes, inference, &mut pacer)? {
                return Ok(());
            }
        }
    }

    /// Draws the latest boxes into the frame and pushes it; `false` once the output pipeline is gone.
    fn push(
        &self,
        sample: &gst::Sample,
        boxes: &Boxes,
        inference: Option<&mpsc::SyncSender<Vec<u8>>>,
        pacer: &mut Pacer,
    ) -> anyhow::Result<bool> {
        let (width, height) = (self.config.width, self.config.height);
        let buffer = sample.buffer().context("sample without buffer")?;
        let map = buffer.map_readable().context("map frame")?;
        let Some(i420) = map.as_slice().get(..frame::size(width, height)) else {
            return Ok(true);
        };
        let mut annotated = i420.to_vec();
        if let Ok(current) = boxes.lock() {
            for detection in current.iter() {
                let rect = [detection.x1, detection.y1, detection.x2, detection.y2];
                frame::draw_rect(&mut annotated, width, height, rect, frame::GREEN, 4);
            }
        }
        // Only while the inference thread is idle: a frame of lag is fine, a backlog is not.
        if let Some(tx) = inference {
            let now = Instant::now();
            let luma = &i420[..(width * height) as usize];
            if pacer.wants(now, luma, width, height, self.status.cat_seen())
                && tx.try_send(i420.to_vec()).is_ok()
            {
                pacer.ran(now);
            }
        }
        let pushed = gst::Buffer::from_mut_slice(annotated);
        self.status.set_frame(&pushed);
        if let Some(clip) = &self.clip {
            clip.frame(&pushed);
        }
        match self.appsrc.push_buffer(pushed) {
            Ok(_) => Ok(true),
            Err(gst::FlowError::Flushing) => Ok(false),
            Err(err) => Err(anyhow!("appsrc push: {err:?}")),
        }
    }
}

/// Drains the capture bus: `Err` on an error message, `Ok(true)` on EOS.
fn bus_says_done(bus: &gst::Bus) -> anyhow::Result<bool> {
    let mut eos = false;
    while let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos]) {
        match msg.view() {
            gst::MessageView::Error(err) => bail!(
                "{} ({})",
                err.error(),
                err.debug().map(|d| d.to_string()).unwrap_or_default()
            ),
            gst::MessageView::Eos(..) => eos = true,
            _ => {}
        }
    }
    Ok(eos)
}

/// The source half of a capture pipeline, plus a child process it may read from.
struct Source {
    elements: Vec<gst::Element>,
    #[cfg(unix)]
    #[allow(dead_code)]
    rpicam: Option<RpicamProcess>,
}

/// `rpicam-vid` and the pipe it writes into; dropping it stops and reaps the process.
#[cfg(unix)]
struct RpicamProcess {
    child: std::process::Child,
    stdout: Option<std::process::ChildStdout>,
}

#[cfg(unix)]
impl Drop for RpicamProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Source {
    fn build(plan: &Plan, found: &Found, config: &Config) -> anyhow::Result<Self> {
        let elements = match plan {
            Plan::Rpicam => return Self::rpicam(config),
            Plan::Device { index, mjpeg } => {
                let info = &found.probe.devices[*index];
                let source = found.devices[*index]
                    .create_element(Some("camera"))
                    .with_context(|| format!("create {} for {}", info.factory, info.name))?;
                let mut elements = vec![source];
                match info.factory.as_str() {
                    "libcamerasrc" => elements.push(capsfilter(libcamera_caps(config))?),
                    "v4l2src" if *mjpeg => {
                        elements.push(capsfilter(gst::Caps::builder("image/jpeg").build())?);
                        elements.push(make("jpegdec")?);
                    }
                    "v4l2src" => elements.push(capsfilter(raw_caps())?),
                    _ => {}
                }
                elements
            }
            Plan::Libcamera => vec![
                make("libcamerasrc")
                    .context("libcamerasrc (is the libcamera plugin installed?)")?,
                capsfilter(libcamera_caps(config))?,
            ],
            Plan::V4l2Path(path) => vec![
                gst::ElementFactory::make("v4l2src")
                    .property("device", path.as_str())
                    .build()
                    .with_context(|| format!("v4l2src {path}"))?,
                // Raw caps: a C920 otherwise negotiates MJPG, which videoconvert cannot decode.
                capsfilter(raw_caps())?,
            ],
            Plan::Test(_) => vec![gst::ElementFactory::make("videotestsrc")
                .property("is-live", true)
                .property_from_str("pattern", "ball")
                .build()
                .context("videotestsrc")?],
            Plan::Image(path) => {
                let png = path.to_ascii_lowercase().ends_with(".png");
                let (media_type, decoder) = if png {
                    ("image/png", "pngdec")
                } else {
                    ("image/jpeg", "jpegdec")
                };
                vec![
                    gst::ElementFactory::make("multifilesrc")
                        .property("location", path.as_str())
                        .property("loop", true)
                        .property(
                            "caps",
                            gst::Caps::builder(media_type)
                                .field("framerate", gst::Fraction::new(config.fps as i32, 1))
                                .build(),
                        )
                        .build()
                        .with_context(|| format!("multifilesrc for {path}"))?,
                    make(decoder)?,
                ]
            }
        };
        Ok(Self {
            elements,
            #[cfg(unix)]
            rpicam: None,
        })
    }

    /// `rpicam-vid` writing raw I420 to a pipe: the host's camera stack, whatever its libcamera ABI.
    #[cfg(unix)]
    fn rpicam(config: &Config) -> anyhow::Result<Self> {
        use std::os::fd::AsRawFd as _;
        use std::process::{Command, Stdio};

        let (w, h, fps) = (config.width, config.height, config.fps);
        let child = Command::new("rpicam-vid")
            .args([
                "--timeout",
                "0",
                "--nopreview",
                "--codec",
                "yuv420",
                "--output",
                "-",
            ])
            .args(["--width", &w.to_string(), "--height", &h.to_string()])
            .args(["--framerate", &fps.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start rpicam-vid")?;
        // Owned before anything below can fail, so an error cannot leave the process behind.
        let mut process = RpicamProcess {
            child,
            stdout: None,
        };
        process.stdout = process.child.stdout.take();
        let fd = process
            .stdout
            .as_ref()
            .context("rpicam-vid stdout")?
            .as_raw_fd();
        let fdsrc = gst::ElementFactory::make("fdsrc")
            .property("fd", fd)
            .property("blocksize", w * h * 3 / 2)
            .build()
            .context("fdsrc")?;
        let parse = gst::ElementFactory::make("rawvideoparse")
            .property_from_str("format", "i420")
            .property("width", w as i32)
            .property("height", h as i32)
            .property("framerate", gst::Fraction::new(fps as i32, 1))
            .build()
            .context("rawvideoparse (is the rawparse plugin installed?)")?;
        Ok(Self {
            elements: vec![fdsrc, parse],
            rpicam: Some(process),
        })
    }

    #[cfg(not(unix))]
    fn rpicam(_config: &Config) -> anyhow::Result<Self> {
        bail!("rpicam-vid exists on Raspberry Pi OS only")
    }
}

/// Stops the pipeline, then (as `source` drops) the child process it read from, whichever way the capture ends.
struct Running {
    pipeline: gst::Pipeline,
    #[allow(dead_code)]
    source: Source,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// The inference thread: takes the newest frame, boxes a cat once [`Presence`] confirms it, logs visits.
/// It ends once its sender is dropped.
fn spawn_inference(
    mut detector: PersonDetector,
    config: &Config,
    boxes: Boxes,
    status: Arc<Status>,
    notifier: Option<Notifier>,
    clip: Option<Arc<Clip>>,
) -> Option<(mpsc::SyncSender<Vec<u8>>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (width, height) = (config.width, config.height);
    let wanted = (!config.all_classes).then(|| vec![COCO_CAT]);
    // The slowest rate the model runs at with a cat in view, so a still one's visit never lapses.
    let mut presence = Presence::new(still_period(config.inference_fps));
    // The open visit's arrival, which its departure event names.
    let mut visit: Option<Arrival> = None;
    let spawned = thread::Builder::new()
        .name("cat-infer".into())
        .spawn(move || {
            while let Ok(i420) = rx.recv() {
                let rgba = frame::to_rgba(&i420, width, height);
                // The settings page may have moved it since the last run.
                let score = status.score();
                let detections =
                    match detector.infer_rgba(&rgba, width, height, score, wanted.as_deref()) {
                        Ok(detections) => detections,
                        Err(err) => {
                            log::warn!("inference failed: {err:#}");
                            continue;
                        }
                    };
                let change = presence.update(Instant::now(), &detections);
                match &change {
                    Some(Change::Arrived { count, best }) => {
                        log::info!("cat in view: {count} detection(s), best score {best:.2}");
                    }
                    Some(Change::Left { stayed }) => {
                        log::info!("no cat in view (stayed {} s)", stayed.as_secs());
                    }
                    None => {}
                }
                let in_view = presence.in_view();
                status.set_cat(in_view, best_score(&detections), change.as_ref());
                // The visit log's own arrival time, so ?t= finds the picture and clip an event names.
                let now = crate::notify::unix_now();
                let time = match change {
                    Some(Change::Arrived { .. }) => status.latest_arrival().unwrap_or(now),
                    _ => now,
                };
                // Before the event goes out, so a receiver that fetches /arrival.jpg?t= gets this one.
                if matches!(change, Some(Change::Arrived { .. })) {
                    match arrival_jpeg(&i420, width, height, &detections) {
                        Ok(jpeg) => status.set_arrival(jpeg, time),
                        Err(err) => log::warn!("arrival snapshot: {err:#}"),
                    }
                }
                if let Some(Change::Arrived { best, .. }) = &change {
                    visit = Some(Arrival {
                        time,
                        best_score: *best,
                    });
                }
                if let (Some(notifier), Some(change)) = (&notifier, &change) {
                    notifier.send(change, time, visit);
                }
                if let (Some(clip), Some(Change::Arrived { .. }), Some(arrival)) =
                    (&clip, &change, visit)
                {
                    clip.start(arrival);
                }
                if matches!(change, Some(Change::Left { .. })) {
                    visit = None;
                }
                if change.is_some() {
                    status.persist();
                }
                if let Ok(mut current) = boxes.lock() {
                    *current = if in_view { detections } else { Vec::new() };
                }
            }
        });
    match spawned {
        Ok(handle) => Some((tx, handle)),
        Err(err) => {
            log::error!("could not start the inference thread, streaming without boxes: {err}");
            None
        }
    }
}

/// The frame a cat was confirmed on, boxes drawn, as JPEG.
fn arrival_jpeg(
    i420: &[u8],
    width: u32,
    height: u32,
    detections: &[Detection],
) -> anyhow::Result<Vec<u8>> {
    let mut boxed = i420.to_vec();
    for d in detections {
        frame::draw_rect(
            &mut boxed,
            width,
            height,
            [d.x1, d.y1, d.x2, d.y2],
            frame::GREEN,
            4,
        );
    }
    crate::status::encode_jpeg(&frame::to_rgba(&boxed, width, height), width, height)
}

fn make(factory: &str) -> anyhow::Result<gst::Element> {
    gst::ElementFactory::make(factory)
        .build()
        .with_context(|| format!("GStreamer element {factory}"))
}

fn capsfilter(caps: gst::Caps) -> anyhow::Result<gst::Element> {
    gst::ElementFactory::make("capsfilter")
        .property("caps", caps)
        .build()
        .context("capsfilter")
}

fn raw_caps() -> gst::Caps {
    gst::Caps::builder("video/x-raw").build()
}

/// The ISP scales cheaper than videoscale, and without `RGB` libcamera hands back raw Bayer.
fn libcamera_caps(config: &Config) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "RGB")
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("framerate", gst::Fraction::new(config.fps as i32, 1))
        .build()
}

/// Always in the pipeline, as a pass-through at 0, so a turn from the page needs no rebuild.
fn videoflip(rotate: u32) -> anyhow::Result<gst::Element> {
    gst::ElementFactory::make("videoflip")
        .property_from_str("method", flip_method(rotate)?)
        .build()
        .context("videoflip")
}

fn flip_method(rotate: u32) -> anyhow::Result<&'static str> {
    Ok(match rotate {
        0 => "none",
        90 => "clockwise",
        180 => "rotate-180",
        270 => "counterclockwise",
        other => bail!("rotate must be 0, 90, 180 or 270 (got {other})"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::rate_period;

    /// Runs the capture worker on `config.camera` into an appsrc/appsink pair and returns `wanted` frames.
    fn capture_frames(config: Config, wanted: usize) -> (Vec<(gst::Caps, Vec<u8>)>, Arc<Status>) {
        capture_frames_turning(config, wanted, None)
    }

    /// As [`capture_frames`], turning the picture to `degrees` once `after` frames have come out.
    fn capture_frames_turning(
        config: Config,
        wanted: usize,
        turn: Option<(usize, u32)>,
    ) -> (Vec<(gst::Caps, Vec<u8>)>, Arc<Status>) {
        gst::init().unwrap();
        let appsrc = gst::ElementFactory::make("appsrc")
            .build()
            .unwrap()
            .downcast::<gst_app::AppSrc>()
            .unwrap();
        appsrc.set_is_live(true);
        appsrc.set_do_timestamp(true);
        appsrc.set_format(gst::Format::Time);
        appsrc.set_caps(Some(&frame::caps(&config)));
        let sink = gst::ElementFactory::make("appsink")
            .property("sync", false)
            .build()
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();
        let output = gst::Pipeline::new();
        output
            .add_many([appsrc.upcast_ref::<gst::Element>(), sink.upcast_ref()])
            .unwrap();
        appsrc.link(&sink).unwrap();
        output.set_state(gst::State::Playing).unwrap();

        let status = Arc::new(Status::new("test", false, config.width, config.height));
        let shutdown = Arc::new(AtomicBool::new(false));
        let choice = Choice::parse(&config.camera).unwrap();
        let worker = Worker {
            config: Arc::new(config),
            choice,
            appsrc,
            detector: None,
            status: status.clone(),
            shutdown: shutdown.clone(),
            notifier: None,
            clip: None,
        }
        .spawn()
        .unwrap();
        let mut frames = Vec::new();
        while frames.len() < wanted {
            let sample = sink
                .try_pull_sample(gst::ClockTime::from_seconds(20))
                .expect("a frame within 20 s");
            let data = sample.buffer().unwrap().map_readable().unwrap().to_vec();
            frames.push((sample.caps().unwrap().to_owned(), data));
            if let Some((after, degrees)) = turn {
                if frames.len() == after {
                    status.set_rotation(degrees).unwrap();
                }
            }
        }
        shutdown.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        output.set_state(gst::State::Null).unwrap();
        (frames, status)
    }

    #[test]
    fn the_test_pattern_reaches_the_output_as_i420_at_the_configured_size() {
        let config = Config {
            camera: "test".into(),
            width: 64,
            height: 48,
            ..Config::default()
        };
        let (frames, status) = capture_frames(config, 3);
        for (caps, data) in &frames {
            let s = caps.structure(0).unwrap();
            assert_eq!(s.get::<&str>("format").unwrap(), "I420");
            assert_eq!(s.get::<i32>("width").unwrap(), 64);
            assert_eq!(s.get::<i32>("height").unwrap(), 48);
            assert_eq!(data.len(), frame::size(64, 48));
        }
        let health: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(health["camera"], "test pattern");
        assert!(status.snapshot_jpeg().unwrap().is_some());
    }

    #[test]
    fn a_turn_from_the_page_reaches_the_running_capture_without_stretching() {
        // Left half white, right half black: a quarter turn clockwise puts the white on top.
        let (w, h) = (64u32, 48u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 255 } else { 0 };
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("half.jpg");
        std::fs::write(&image, crate::status::encode_jpeg(&rgba, w, h).unwrap()).unwrap();
        let config = Config {
            camera: format!("image:{}", image.display()),
            width: w,
            height: h,
            ..Config::default()
        };
        let (frames, _) = capture_frames_turning(config, 30, Some((5, 90)));
        let luma = |data: &[u8], x: u32, y: u32| data[(y * w + x) as usize];
        let (_, before) = &frames[2];
        assert!(
            luma(before, 6, 24) > 200 && luma(before, 57, 24) < 50,
            "white left, black right"
        );
        let (caps, after) = frames.last().unwrap();
        let s = caps.structure(0).unwrap();
        assert_eq!(
            (
                s.get::<i32>("width").unwrap(),
                s.get::<i32>("height").unwrap()
            ),
            (64, 48)
        );
        assert_eq!(after.len(), frame::size(w, h));
        assert!(
            luma(after, 32, 6) > 200 && luma(after, 32, 41) < 50,
            "white top, black bottom"
        );
        // The 48x64 picture is bordered into 64x48, not stretched: its sides are bars.
        assert!(
            luma(after, 2, 6) < 50 && luma(after, 61, 6) < 50,
            "bars at the sides"
        );
    }

    #[test]
    fn the_rate_period_follows_the_rate_and_zero_means_unbounded() {
        assert_eq!(rate_period(2.0), Duration::from_millis(500));
        assert_eq!(rate_period(0.5), Duration::from_secs(2));
        assert_eq!(rate_period(0.0), Duration::ZERO);
    }
}
