//! The capture worker: probe, capture, draw the latest boxes, push; when a capture ends it probes again,
//! so an unplugged camera, a crashed `rpicam-vid` or a webcam plugged in later never needs a restart.

mod logic;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

use anyhow::{anyhow, bail, Context as _};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use kataglyphis_inference::person_detection::PersonDetector;

#[cfg(unix)]
use self::logic::I420Layout;
pub use self::logic::Timing;
use self::logic::{
    box_lifetime, capture_size, exact_frame, flip_method, fresh, isp_scales, sleep_unless, Backoff,
    Boxes, Visits,
};
use crate::camera::{self, Choice, Found, Plan};
use crate::clip::Clip;
use crate::config::{Config, Privacy};
use crate::frame;
use crate::motion::{still_period, Moving, Pacer};
use crate::notify::Notifier;
use crate::status::Status;

/// One pull's wait, which bounds how fast shutdown, errors and stalls are noticed.
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_mseconds(500);

/// A frame for the model, and where something moved lately; `None` with the motion gate off.
type Look = (Vec<u8>, Option<Moving>);

/// Why a capture ended without an error.
#[derive(Debug, PartialEq)]
enum Ended {
    /// The source finished, the stand-in's time ran out, or the service stops.
    Done,
    /// The page turned the picture between landscape and portrait: capture again at the new size.
    Turned,
}

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
    pub timing: Timing,
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
        let mut backoff = Backoff::default();
        while !self.shutdown.load(Ordering::Relaxed) {
            let found = camera::probe(&self.choice);
            let plan = camera::choose(&self.choice, &found.probe);
            let label = plan.describe(&found.probe);
            self.status.set_camera(&label, plan.is_stand_in());
            log::info!("camera: {label}");
            let started = Instant::now();
            let deadline = self.reprobe_at(&plan, started);
            let result = self.capture(&plan, &found, &boxes, frames, deadline);
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            if matches!(result, Ok(Ended::Turned)) {
                log::info!(
                    "the picture turned to {} degrees: capturing again at its size",
                    self.status.rotation()
                );
                continue;
            }
            match &result {
                Err(err) => log::warn!("capture from {label} stopped: {err:#}"),
                Ok(_) if !plan.is_stand_in() => log::info!("capture from {label} ended"),
                Ok(_) => {}
            }
            if let Some(pause) =
                backoff.after(started.elapsed(), result.is_err(), plan.is_stand_in())
            {
                sleep_unless(&self.shutdown, pause);
            }
        }
        // Finish the frame in flight: an ORT session still running at process exit errors out.
        if let Some((tx, handle)) = inference {
            drop(tx);
            let _ = handle.join();
        }
        log::info!("capture worker stopped");
    }

    /// When a stand-in started at `started` ends so the camera is probed again; a real camera runs until it fails.
    fn reprobe_at(&self, plan: &Plan, started: Instant) -> Option<Instant> {
        plan.is_stand_in()
            .then(|| started + self.timing.reprobe_every)
    }

    fn capture(
        &self,
        plan: &Plan,
        found: &Found,
        boxes: &Boxes,
        inference: Option<&mpsc::SyncSender<Look>>,
        deadline: Option<Instant>,
    ) -> anyhow::Result<Ended> {
        let config = &self.config;
        let mut turned = self.status.rotation();
        let isp = isp_scales(plan, found);
        let size = capture_size(isp, config, turned);
        let mut source = Source::build(plan, found, config, size)?;
        let pipeline = gst::Pipeline::new();
        let mut elements = std::mem::take(&mut source.elements);
        elements.push(make("videoconvert")?);
        let flip = videoflip(turned)?;
        elements.push(flip.clone());
        elements.push(make("videoscale")?);
        elements.push(capsfilter(frame::caps(config))?);
        // Only the non-live still-image loop needs the sink to pace it.
        let sink = kataglyphis_media::frame_sink(1, matches!(plan, Plan::Image(_)))?;
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
                return Ok(Ended::Done);
            }
            if bus_says_done(&bus)? {
                return Ok(Ended::Done);
            }
            let Some(sample) = sink.try_pull_sample(PULL_TIMEOUT) else {
                // A failing source posts its error and then sends EOS: report the error, not a clean end.
                if sink.is_eos() {
                    bus_says_done(&bus)?;
                    return Ok(Ended::Done);
                }
                if last_frame.elapsed() > self.timing.stall_after {
                    bail!(
                        "no frame for {:.1} s",
                        self.timing.stall_after.as_secs_f64()
                    );
                }
                continue;
            };
            last_frame = Instant::now();
            // The page turned the picture: videoflip follows at once, and videoscale keeps the size.
            let wanted = self.status.rotation();
            if wanted != turned {
                if capture_size(isp, config, wanted) != size {
                    return Ok(Ended::Turned);
                }
                flip.set_property_from_str("method", flip_method(wanted)?);
                turned = wanted;
            }
            if !self.push(&sample, boxes, inference, &mut pacer)? {
                return Ok(Ended::Done);
            }
        }
    }

    /// The frame with the privacy filter and the latest boxes; boxes past their lifetime are dropped first.
    fn annotate(&self, i420: &[u8], boxes: &Boxes) -> Vec<u8> {
        let (width, height) = (self.config.width, self.config.height);
        let lifetime = box_lifetime(
            still_period(self.config.inference_fps),
            self.status.model_ms(),
        );
        let mut dropped = false;
        let rects: Vec<[f32; 4]> = boxes
            .lock()
            .map(|mut current| {
                // Under the lock: a run that ends now sets its time before it sets its boxes.
                if !current.is_empty() && !fresh(self.status.run_age(), lifetime) {
                    current.clear();
                    dropped = true;
                }
                current.iter().map(|d| [d.x1, d.y1, d.x2, d.y2]).collect()
            })
            .unwrap_or_default();
        if dropped {
            self.status.set_boxes(Vec::new());
            log::warn!(
                "no model run for over {} s: the last boxes are dropped",
                lifetime.as_secs()
            );
        }
        let mut annotated = i420.to_vec();
        // Before the boxes, and on the copy only: the model keeps seeing the whole picture.
        if self.config.privacy == Privacy::Pixelate {
            frame::pixelate_outside(&mut annotated, width, height, &rects);
        }
        for rect in rects {
            frame::draw_rect(&mut annotated, width, height, rect, frame::GREEN, 4);
        }
        annotated
    }

    /// Draws the latest boxes into the frame and pushes it; `false` once the output pipeline is gone.
    fn push(
        &self,
        sample: &gst::Sample,
        boxes: &Boxes,
        inference: Option<&mpsc::SyncSender<Look>>,
        pacer: &mut Pacer,
    ) -> anyhow::Result<bool> {
        let (width, height) = (self.config.width, self.config.height);
        let buffer = sample.buffer().context("sample without buffer")?;
        let map = buffer.map_readable().context("map frame")?;
        let i420 = exact_frame(map.as_slice(), width, height)?;
        let annotated = self.annotate(i420, boxes);
        // Only while the inference thread is idle: a frame of lag is fine, a backlog is not.
        if let Some(tx) = inference {
            let now = Instant::now();
            let luma = &i420[..(width * height) as usize];
            if pacer.wants(now, luma, width, height, self.status.anything_seen())
                && tx.try_send((i420.to_vec(), pacer.moving(now))).is_ok()
            {
                pacer.ran(now);
            }
            self.status.set_dark(pacer.is_dark());
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
    /// `size` is what the source delivers; [`capture_size`] picks it.
    fn build(
        plan: &Plan,
        found: &Found,
        config: &Config,
        size: (u32, u32),
    ) -> anyhow::Result<Self> {
        let elements = match plan {
            Plan::Rpicam => return Self::rpicam(config, size),
            Plan::Device { index, mjpeg } => {
                let info = &found.probe.devices[*index];
                let source = found.devices[*index]
                    .create_element(Some("camera"))
                    .with_context(|| format!("create {} for {}", info.factory, info.name))?;
                let mut elements = vec![source];
                match info.factory.as_str() {
                    "libcamerasrc" => elements.push(capsfilter(libcamera_caps(config, size))?),
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
                capsfilter(libcamera_caps(config, size))?,
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
    fn rpicam(config: &Config, (w, h): (u32, u32)) -> anyhow::Result<Self> {
        use std::os::fd::AsRawFd as _;
        use std::process::{Command, Stdio};

        let fps = config.fps;
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
        let layout = I420Layout::rpicam(w, h);
        let fdsrc = gst::ElementFactory::make("fdsrc")
            .property("fd", fd)
            .property("blocksize", layout.frame_size)
            .build()
            .context("fdsrc")?;
        let parse = gst::ElementFactory::make("rawvideoparse")
            .property_from_str("format", "i420")
            .property("width", w as i32)
            .property("height", h as i32)
            .property("framerate", gst::Fraction::new(fps as i32, 1))
            .property_from_str("plane-strides", &layout.strides())
            .property_from_str("plane-offsets", &layout.offsets())
            .property("frame-size", layout.frame_size)
            .build()
            .context("rawvideoparse (is the rawparse plugin installed?)")?;
        Ok(Self {
            elements: vec![fdsrc, parse],
            rpicam: Some(process),
        })
    }

    #[cfg(not(unix))]
    fn rpicam(_config: &Config, _size: (u32, u32)) -> anyhow::Result<Self> {
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
) -> Option<(mpsc::SyncSender<Look>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel::<Look>(1);
    let (width, height) = (config.width, config.height);
    let mut visits = Visits::new(config, boxes, status.clone(), notifier, clip);
    let spawned = thread::Builder::new()
        .name("observant-infer".into())
        .spawn(move || {
            while let Ok((i420, moving)) = rx.recv() {
                let rgba = frame::to_rgba(&i420, width, height);
                // The settings page may have moved them since the last run.
                let score = status.score();
                let wanted = status.wanted_classes();
                let started = Instant::now();
                status.run_starts();
                match detector.infer_rgba(&rgba, width, height, score, wanted.as_deref()) {
                    Ok(detections) => {
                        status.record_model_time(started.elapsed());
                        visits.record(Instant::now(), &i420, detections, moving.as_ref());
                    }
                    Err(err) => visits.model_failed(format!("{err:#}")),
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
fn libcamera_caps(config: &Config, (width, height): (u32, u32)) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "RGB")
        .field("width", width as i32)
        .field("height", height as i32)
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::logic::tests::{health, middle_cat};
    use super::*;
    use crate::presence::zone_of;

    /// A running capture worker, its output's sink and what stops it.
    struct Capturing {
        sink: gst_app::AppSink,
        output: gst::Pipeline,
        status: Arc<Status>,
        shutdown: Arc<AtomicBool>,
        worker: thread::JoinHandle<()>,
    }

    /// A worker on `config.camera`, not started, and the output it pushes into, already playing.
    fn unstarted(
        config: Config,
        detector: Option<PersonDetector>,
        timing: Timing,
    ) -> (Worker, gst_app::AppSink, gst::Pipeline) {
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
        let sink = kataglyphis_media::frame_sink(0, false).unwrap();
        let output = gst::Pipeline::new();
        output
            .add_many([appsrc.upcast_ref::<gst::Element>(), sink.upcast_ref()])
            .unwrap();
        appsrc.link(&sink).unwrap();
        output.set_state(gst::State::Playing).unwrap();
        let status = Arc::new(Status::new("test", false, config.width, config.height));
        let choice = Choice::parse(&config.camera).unwrap();
        let worker = Worker {
            config: Arc::new(config),
            choice,
            appsrc,
            detector,
            status,
            shutdown: Arc::new(AtomicBool::new(false)),
            notifier: None,
            clip: None,
            timing,
        };
        (worker, sink, output)
    }

    impl Capturing {
        /// The worker on `config.camera`, pushing into an appsrc whose frames a test pulls from `sink`.
        fn start(config: Config, detector: Option<PersonDetector>) -> Self {
            let (worker, sink, output) = unstarted(config, detector, Timing::default());
            let (status, shutdown) = (worker.status.clone(), worker.shutdown.clone());
            Self {
                sink,
                output,
                status,
                shutdown,
                worker: worker.spawn().unwrap(),
            }
        }

        /// The next frame within `wait`, with its caps.
        fn frame(&self, wait: Duration) -> Option<(gst::Caps, Vec<u8>)> {
            let sample = self
                .sink
                .try_pull_sample(gst::ClockTime::from_mseconds(wait.as_millis() as u64))?;
            let data = sample.buffer().unwrap().map_readable().unwrap().to_vec();
            Some((sample.caps().unwrap().to_owned(), data))
        }

        /// Stops the worker the way the service does and hands back its status.
        fn stop(self) -> Arc<Status> {
            self.shutdown.store(true, Ordering::Relaxed);
            self.worker.join().unwrap();
            self.output.set_state(gst::State::Null).unwrap();
            self.status
        }
    }

    /// Runs the capture worker on `config.camera` into an appsrc and a frame sink and returns `wanted` frames.
    fn capture_frames(config: Config, wanted: usize) -> (Vec<(gst::Caps, Vec<u8>)>, Arc<Status>) {
        capture_frames_turning(config, wanted, None)
    }

    /// As [`capture_frames`], turning the picture to `degrees` once `after` frames have come out.
    fn capture_frames_turning(
        config: Config,
        wanted: usize,
        turn: Option<(usize, u32)>,
    ) -> (Vec<(gst::Caps, Vec<u8>)>, Arc<Status>) {
        let running = Capturing::start(config, None);
        let mut frames = Vec::new();
        while frames.len() < wanted {
            frames.push(
                running
                    .frame(Duration::from_secs(20))
                    .expect("a frame within 20 s"),
            );
            if let Some((after, degrees)) = turn {
                if frames.len() == after {
                    running.status.set_rotation(degrees).unwrap();
                }
            }
        }
        (frames, running.stop())
    }

    /// The photo, from inside the test binary: a cross lane runs it on a machine without the source tree.
    fn two_cats_in(dir: &std::path::Path) -> String {
        let photo = dir.join("two_cats.jpg");
        std::fs::write(
            &photo,
            include_bytes!("../../inference/tests/fixtures/two_cats.jpg"),
        )
        .unwrap();
        photo.display().to_string()
    }

    /// The model `scripts/linux/fetch-models.sh` puts in the checkout; `None` skips, unless CI demands it.
    fn real_model() -> Option<String> {
        let path = kataglyphis_inference::person_detection::resolve_model_path(None);
        if std::path::Path::new(&path).is_file() {
            return Some(path);
        }
        assert!(
            std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() != Ok("1"),
            "no model at {path}: scripts/linux/fetch-models.sh fetches it"
        );
        eprintln!("SKIP: no model at {path}");
        None
    }

    /// The whole loop on the photo: capture, pacer, the inference thread with the real model, and the visit it opens.
    #[test]
    fn the_real_model_on_the_photo_opens_a_visit_through_the_capture_loop() {
        let Some(model) = real_model() else { return };
        let dir = tempfile::tempdir().unwrap();
        let photo = two_cats_in(dir.path());
        let config = Config {
            camera: format!("image:{photo}"),
            ..Config::default()
        };
        let running = Capturing::start(config, Some(PersonDetector::new(&model).unwrap()));
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut health = serde_json::Value::Null;
        while Instant::now() < deadline {
            let _ = running.frame(Duration::from_millis(100));
            health = serde_json::from_str(&running.status.to_json()).unwrap();
            if health["detection"]["visits"].as_u64() >= Some(1) {
                break;
            }
        }
        running.stop();
        let detection = &health["detection"];
        assert_eq!(detection["visits"], 1, "{health}");
        assert_eq!(detection["in_view"], true, "{health}");
        assert_eq!(
            detection["boxes"].as_array().map(Vec::len),
            Some(2),
            "both cats: {health}"
        );
        assert!(health["model_runs"].as_u64() >= Some(1), "{health}");
    }

    /// A camera slower than `stall_after` counts as gone: the capture ends with the stall, and the worker restarts it.
    #[test]
    fn a_camera_that_stops_sending_is_a_stall_after_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let photo = two_cats_in(dir.path());
        let config = Config {
            camera: format!("image:{photo}"),
            width: 64,
            height: 48,
            // One frame a second, so the 300 ms stall comes between two of them.
            fps: 1,
            ..Config::default()
        };
        let timing = Timing {
            stall_after: Duration::from_millis(300),
            ..Timing::default()
        };
        let (worker, _sink, output) = unstarted(config, None, timing);
        let found = camera::probe(&worker.choice);
        let plan = camera::choose(&worker.choice, &found.probe);
        let started = Instant::now();
        let ended = worker.capture(&plan, &found, &Boxes::default(), None, None);
        output.set_state(gst::State::Null).unwrap();
        let err = format!("{:#}", ended.expect_err("a stall, not a clean end"));
        assert_eq!(err, "no frame for 0.3 s");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    /// A stand-in pattern ends at `reprobe_every` so the camera is looked for again; a real source has no such end.
    #[test]
    fn a_stand_in_ends_at_its_reprobe_time_and_a_real_camera_never() {
        let timing = Timing {
            reprobe_every: Duration::from_millis(300),
            ..Timing::default()
        };
        let config = Config {
            camera: "test".into(),
            width: 64,
            height: 48,
            ..Config::default()
        };
        let (worker, _sink, output) = unstarted(config, None, timing);
        let at = Instant::now();
        assert_eq!(worker.reprobe_at(&Plan::Test(None), at), None);
        assert_eq!(worker.reprobe_at(&Plan::Image("cat.jpg".into()), at), None);
        let stand_in = Plan::Test(Some("no USB webcam found".into()));
        let deadline = worker.reprobe_at(&stand_in, at);
        assert_eq!(deadline, Some(at + Duration::from_millis(300)));
        let found = camera::probe(&Choice::Test);
        let ended = worker.capture(&stand_in, &found, &Boxes::default(), None, deadline);
        output.set_state(gst::State::Null).unwrap();
        assert!(matches!(ended, Ok(Ended::Done)), "{ended:?}");
        assert!(at.elapsed() < Duration::from_secs(5), "{:?}", at.elapsed());
    }

    /// A camera path that is missing at start is tried again after the backoff, and its frames come once it exists.
    #[test]
    fn a_missing_image_is_retried_after_the_backoff_and_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("later.jpg");
        let config = Config {
            camera: format!("image:{}", image.display()),
            width: 64,
            height: 48,
            ..Config::default()
        };
        let running = Capturing::start(config, None);
        let label = format!("still image {}", image.display());
        let deadline = Instant::now() + Duration::from_secs(10);
        let camera = |status: &Status| {
            serde_json::from_str::<serde_json::Value>(&status.to_json()).unwrap()["camera"].clone()
        };
        while camera(&running.status) != label.as_str() {
            assert!(Instant::now() < deadline, "the worker never tried {label}");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            running.frame(Duration::from_millis(500)).is_none(),
            "a frame from a file that is not there"
        );
        std::fs::write(
            &image,
            crate::status::encode_jpeg(&[200u8; 64 * 48 * 4], 64, 48).unwrap(),
        )
        .unwrap();
        let (caps, data) = running
            .frame(Duration::from_secs(10))
            .expect("the capture recovered once the file existed");
        running.stop();
        let s = caps.structure(0).unwrap();
        assert_eq!(s.get::<i32>("width").unwrap(), 64);
        assert_eq!(data.len(), frame::size(64, 48));
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

    /// A worker that never captures, for the steps a frame goes through.
    fn idle_worker(privacy: Privacy, status: &Arc<Status>) -> Worker {
        gst::init().unwrap();
        let config = Config {
            camera: "test".into(),
            width: 64,
            height: 48,
            privacy,
            ..Config::default()
        };
        let appsrc = gst::ElementFactory::make("appsrc")
            .build()
            .unwrap()
            .downcast::<gst_app::AppSrc>()
            .unwrap();
        Worker {
            choice: Choice::parse(&config.camera).unwrap(),
            config: Arc::new(config),
            appsrc,
            detector: None,
            status: status.clone(),
            shutdown: Arc::new(AtomicBool::new(false)),
            notifier: None,
            clip: None,
            timing: Timing::default(),
        }
    }

    #[test]
    fn boxes_of_a_run_long_ago_are_dropped_and_the_filter_covers_their_area() {
        let frame: Vec<u8> = (0..frame::size(64, 48))
            .map(|i| (i * 7 % 251) as u8)
            .collect();
        // One of the filter's 8 x 8 luma blocks inside the cat: sharp it varies, filtered it holds one value.
        let sharp_in_the_cat = |annotated: &[u8]| {
            let block: Vec<u8> = (16..24)
                .flat_map(|y| annotated[y * 64 + 24..y * 64 + 32].to_vec())
                .collect();
            block.iter().any(|&p| p != block[0])
        };
        for privacy in [Privacy::Pixelate, Privacy::Off] {
            let status = Arc::new(Status::new("Cam", true, 64, 48));
            let worker = idle_worker(privacy, &status);
            let boxes: Boxes = Arc::new(Mutex::new(middle_cat()));
            status.set_boxes(vec![zone_of(&middle_cat()[0], 64, 48)]);
            // No run on record: boxes nobody timed are none.
            assert_eq!(
                worker.annotate(&frame, &boxes) != frame,
                privacy == Privacy::Pixelate
            );
            assert!(boxes.lock().unwrap().is_empty());

            *boxes.lock().unwrap() = middle_cat();
            status.set_boxes(vec![zone_of(&middle_cat()[0], 64, 48)]);
            status.set_detection(true, Some(0.6), None);
            let drawn = worker.annotate(&frame, &boxes);
            assert_ne!(drawn, frame, "the box is drawn");
            assert!(sharp_in_the_cat(&drawn));
            assert_eq!(boxes.lock().unwrap().len(), 1);
            assert_eq!(
                health(&status)["detection"]["boxes"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );

            let lifetime = box_lifetime(still_period(worker.config.inference_fps), None);
            status.age_last_run(lifetime + Duration::from_secs(1));
            let stale = worker.annotate(&frame, &boxes);
            assert!(boxes.lock().unwrap().is_empty(), "the stale box is dropped");
            assert!(health(&status)["detection"]["boxes"]
                .as_array()
                .unwrap()
                .is_empty());
            if privacy == Privacy::Pixelate {
                assert!(!sharp_in_the_cat(&stale), "its area would stay sharp");
            } else {
                assert_eq!(stale, frame, "no box is drawn");
            }
        }
    }
    #[test]
    fn with_the_privacy_filter_the_test_pattern_leaves_the_camera_as_blocks() {
        let config = Config {
            camera: "test".into(),
            width: 64,
            height: 48,
            privacy: Privacy::Pixelate,
            ..Config::default()
        };
        let (frames, status) = capture_frames(config, 3);
        // No box: every 8 x 8 luma block (32 pixels at 640 wide, scaled) holds one value.
        for (_, data) in &frames {
            for by in (0..48).step_by(8) {
                for bx in (0..64).step_by(8) {
                    let first = data[by * 64 + bx];
                    for y in by..by + 8 {
                        assert!(data[y * 64 + bx..y * 64 + bx + 8]
                            .iter()
                            .all(|&p| p == first));
                    }
                }
            }
        }
        let first = &frames[0].1;
        assert!(
            first[..64].windows(2).any(|w| w[0] != w[1]),
            "the bars still differ"
        );
        assert!(status.snapshot_jpeg().unwrap().is_some());
    }

    #[test]
    fn the_bus_reports_an_error_with_its_text_and_an_end_of_stream_as_done() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let bus = pipeline.bus().unwrap();
        assert!(!bus_says_done(&bus).unwrap());
        bus.post(gst::message::Eos::new()).unwrap();
        assert!(bus_says_done(&bus).unwrap());
        let error = gst::message::Error::builder(gst::CoreError::Failed, "the camera went away")
            .debug("v4l2 says ENODEV")
            .build();
        bus.post(error).unwrap();
        let err = bus_says_done(&bus).unwrap_err().to_string();
        assert!(
            err.contains("the camera went away") && err.contains("ENODEV"),
            "{err}"
        );
    }
}
