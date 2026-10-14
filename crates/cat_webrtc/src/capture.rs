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
use crate::config::{Config, Privacy};
use crate::frame;
use crate::motion::{still_period, Moving, Pacer};
use crate::notify::Notifier;
use crate::presence::{best_score, outside, zone_of, Change, Presence};
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
                if last_frame.elapsed() > STALL_AFTER {
                    bail!("no frame for {} s", STALL_AFTER.as_secs());
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
        let Some(i420) = map.as_slice().get(..frame::size(width, height)) else {
            return Ok(true);
        };
        let mut annotated = i420.to_vec();
        let rects: Vec<[f32; 4]> = boxes
            .lock()
            .map(|current| current.iter().map(|d| [d.x1, d.y1, d.x2, d.y2]).collect())
            .unwrap_or_default();
        // Before the boxes, and on the copy only: the model keeps seeing the whole picture.
        if self.config.privacy == Privacy::Pixelate {
            frame::pixelate_outside(&mut annotated, width, height, &rects);
        }
        for rect in rects {
            frame::draw_rect(&mut annotated, width, height, rect, frame::GREEN, 4);
        }
        // Only while the inference thread is idle: a frame of lag is fine, a backlog is not.
        if let Some(tx) = inference {
            let now = Instant::now();
            let luma = &i420[..(width * height) as usize];
            if pacer.wants(now, luma, width, height, self.status.cat_seen())
                && tx.try_send((i420.to_vec(), pacer.moving(now))).is_ok()
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
    let wanted = (!config.all_classes).then(|| vec![COCO_CAT]);
    let mut visits = Visits {
        // The slowest rate the model runs at with a cat in view, so a still one's visit never lapses.
        presence: Presence::new(still_period(config.inference_fps)),
        visit: None,
        width,
        height,
        privacy: config.privacy,
        boxes,
        status: status.clone(),
        notifier,
        clip,
    };
    let spawned = thread::Builder::new()
        .name("cat-infer".into())
        .spawn(move || {
            while let Ok((i420, moving)) = rx.recv() {
                let rgba = frame::to_rgba(&i420, width, height);
                // The settings page may have moved it since the last run.
                let score = status.score();
                let started = Instant::now();
                match detector.infer_rgba(&rgba, width, height, score, wanted.as_deref()) {
                    Ok(detections) => {
                        status.record_model_time(started.elapsed());
                        visits.record(Instant::now(), &i420, detections, moving.as_ref());
                    }
                    Err(err) => log::warn!("inference failed: {err:#}"),
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

/// What the model's runs make of a cat: presence, the visit log, the arrival picture, the webhook
/// event and the clip. Apart from the model and the clock, so a test drives it with set times.
struct Visits {
    presence: Presence,
    /// The open visit's arrival, which its departure event names.
    visit: Option<Arrival>,
    width: u32,
    height: u32,
    privacy: Privacy,
    boxes: Boxes,
    status: Arc<Status>,
    notifier: Option<Notifier>,
    clip: Option<Arc<Clip>>,
}

impl Visits {
    /// Takes one run's `detections` on the frame `i420`, seen at `now`.
    fn record(
        &mut self,
        now: Instant,
        i420: &[u8],
        detections: Vec<Detection>,
        moving: Option<&Moving>,
    ) {
        let (width, height) = (self.width, self.height);
        let status = &self.status;
        let detections = outside(detections, &status.ignored(), width, height);
        let counted = counted(&detections, self.presence.in_view(), moving);
        let change = self.presence.update(now, &counted);
        match &change {
            Some(Change::Arrived { count, best }) => {
                log::info!("cat in view: {count} detection(s), best score {best:.2}");
            }
            Some(Change::Left { stayed }) => {
                log::info!("no cat in view (stayed {} s)", stayed.as_secs());
            }
            None => {}
        }
        let in_view = self.presence.in_view();
        // Before set_cat, which keeps an arrival's boxes with its visit.
        let zones = if in_view {
            detections
                .iter()
                .map(|d| zone_of(d, width, height))
                .collect()
        } else {
            Vec::new()
        };
        status.set_boxes(zones);
        status.set_cat(in_view, best_score(&counted), change.as_ref());
        // The visit log's own arrival time, so ?t= finds the picture and clip an event names.
        let unix = crate::notify::unix_now();
        let time = match change {
            Some(Change::Arrived { .. }) => status.latest_arrival().unwrap_or(unix),
            _ => unix,
        };
        if let Some(Change::Arrived { best, .. }) = &change {
            // Before the event goes out, so a receiver that fetches /arrival.jpg?t= gets this one.
            match arrival_jpeg(i420, width, height, &detections, self.privacy) {
                Ok(jpeg) => status.set_arrival(jpeg, time),
                Err(err) => log::warn!("arrival snapshot: {err:#}"),
            }
            self.visit = Some(Arrival {
                time,
                best_score: *best,
            });
        }
        if let (Some(notifier), Some(change)) = (&self.notifier, &change) {
            notifier.send(change, time, self.visit);
        }
        if let (Some(clip), Some(Change::Arrived { .. }), Some(arrival)) =
            (&self.clip, &change, self.visit)
        {
            clip.start(arrival);
        }
        if matches!(change, Some(Change::Left { .. })) {
            self.visit = None;
        }
        if change.is_some() {
            status.persist();
        }
        let shown = if in_view { detections } else { Vec::new() };
        if let Ok(mut current) = self.boxes.lock() {
            *current = shown;
        }
    }
}

/// The detections that count toward a visit: all while a cat is in view, and to start one only
/// those where something moved lately, so a still object the model mistakes for a cat never arrives.
fn counted(detections: &[Detection], in_view: bool, moving: Option<&Moving>) -> Vec<Detection> {
    match moving {
        Some(moving) if !in_view => detections
            .iter()
            .filter(|d| moving.near([d.x1, d.y1, d.x2, d.y2]))
            .copied()
            .collect(),
        _ => detections.to_vec(),
    }
}

/// The frame a cat was confirmed on, boxes drawn, as JPEG.
fn arrival_jpeg(
    i420: &[u8],
    width: u32,
    height: u32,
    detections: &[Detection],
    privacy: Privacy,
) -> anyhow::Result<Vec<u8>> {
    let mut boxed = i420.to_vec();
    if privacy == Privacy::Pixelate {
        let rects: Vec<[f32; 4]> = detections
            .iter()
            .map(|d| [d.x1, d.y1, d.x2, d.y2])
            .collect();
        frame::pixelate_outside(&mut boxed, width, height, &rects);
    }
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

/// What the source delivers at `rotate`. A Pi camera's ISP scales for free, so a quarter turn
/// captures the picture already as small as it ends up turned (480x360 for 640x480), and
/// videoscale only adds the side borders: a Pi 5 spent 20 % of a core resampling it, 3 % now.
fn capture_size(isp: bool, config: &Config, rotate: u32) -> (u32, u32) {
    let (w, h) = (config.width, config.height);
    // Only a width whose rows pad alike at 64 and 128 bytes, so the stride holds on any Pi's ISP.
    let pads_alike = h.next_multiple_of(64) == h.next_multiple_of(128);
    if isp && matches!(rotate, 90 | 270) && w > 0 && pads_alike {
        let side = (u64::from(h) * u64::from(h) / u64::from(w)) as u32 & !1;
        (h, side.max(2))
    } else {
        (w, h)
    }
}

/// Where `rpicam-vid --codec yuv420` puts the planes of one frame: each row padded to 128 bytes,
/// measured on a Pi (480 wide writes 512-byte rows, 640 none).
#[cfg(unix)]
#[derive(Debug, PartialEq)]
struct I420Layout {
    luma_stride: u32,
    chroma_stride: u32,
    height: u32,
    frame_size: u32,
}

#[cfg(unix)]
impl I420Layout {
    fn rpicam(width: u32, height: u32) -> Self {
        let luma_stride = width.next_multiple_of(128);
        let chroma_stride = luma_stride / 2;
        Self {
            luma_stride,
            chroma_stride,
            height,
            frame_size: luma_stride * height + 2 * chroma_stride * height.div_ceil(2),
        }
    }

    fn strides(&self) -> String {
        format!(
            "<{}, {}, {}>",
            self.luma_stride, self.chroma_stride, self.chroma_stride
        )
    }

    fn offsets(&self) -> String {
        let chroma = self.luma_stride * self.height;
        let second = chroma + self.chroma_stride * self.height.div_ceil(2);
        format!("<0, {chroma}, {second}>")
    }
}

/// A source whose camera pipeline scales to any size it is asked for: a Pi camera's ISP.
fn isp_scales(plan: &Plan, found: &Found) -> bool {
    match plan {
        Plan::Rpicam | Plan::Libcamera => true,
        Plan::Device { index, .. } => found
            .probe
            .devices
            .get(*index)
            .is_some_and(|d| d.factory == "libcamerasrc"),
        _ => false,
    }
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

    #[test]
    fn a_still_detection_cannot_start_a_visit_but_keeps_one_going() {
        let (width, height) = (64, 48);
        let grey = vec![50u8; (width * height) as usize];
        let t = Instant::now();
        let mut pacer = Pacer::new(2.0, 0.2, t);
        pacer.wants(t, &grey, width, height, false);
        pacer.ran(t);
        let at_start = pacer.moving(t).unwrap();
        let later = t + Duration::from_secs(11);
        pacer.wants(later, &grey, width, height, false);
        pacer.ran(later);
        let still = pacer.moving(later).unwrap();
        // A drying rack along the bottom edge that the model scores as a cat.
        let rack = Detection {
            x1: 8.0,
            y1: 32.0,
            x2: 56.0,
            y2: 47.0,
            score: 0.4,
            class_id: COCO_CAT,
        };
        // Seen as the camera starts it counts, since anything there has just come into view.
        assert_eq!(counted(&[rack], false, Some(&at_start)), vec![rack]);
        // Ten seconds of stillness on, it cannot start a visit, but it keeps an open one going.
        assert!(counted(&[rack], false, Some(&still)).is_empty());
        assert_eq!(counted(&[rack], true, Some(&still)), vec![rack]);
        // With the motion gate off nothing is known about motion, and every detection counts.
        assert_eq!(counted(&[rack], false, None), vec![rack]);
    }

    #[test]
    fn a_pi_camera_turned_a_quarter_captures_the_picture_as_small_as_it_ends_up() {
        let config = Config::default();
        assert_eq!(capture_size(true, &config, 0), (640, 480));
        assert_eq!(capture_size(true, &config, 180), (640, 480));
        assert_eq!(capture_size(true, &config, 90), (480, 360));
        assert_eq!(capture_size(true, &config, 270), (480, 360));
        // A webcam or a test source has no free scaler: it keeps its size, and videoscale scales.
        assert_eq!(capture_size(false, &config, 270), (640, 480));
        // Odd results round down to even, which I420 needs.
        let wide = Config {
            width: 1280,
            height: 722,
            ..Config::default()
        };
        assert_eq!(capture_size(true, &wide, 90), (722, 406));
        // A height whose rows a 64-byte and a 128-byte ISP pad differently keeps the full size.
        let odd = Config {
            width: 768,
            height: 576,
            ..Config::default()
        };
        assert_eq!(capture_size(true, &odd, 90), (768, 576));
    }

    #[cfg(unix)]
    #[test]
    fn rpicam_frames_are_read_with_their_padded_rows() {
        // Measured on tabblscatcam: 480x360 is 276480 bytes a frame, 640x480 460800.
        let turned = I420Layout::rpicam(480, 360);
        assert_eq!(turned.frame_size, 276_480);
        assert_eq!(turned.strides(), "<512, 256, 256>");
        assert_eq!(turned.offsets(), "<0, 184320, 230400>");
        assert_eq!(I420Layout::rpicam(640, 480).frame_size, 460_800);
        assert_eq!(I420Layout::rpicam(448, 336).frame_size, 258_048);
        // rawvideoparse takes them as written.
        gst::init().unwrap();
        let parse = gst::ElementFactory::make("rawvideoparse")
            .property_from_str("format", "i420")
            .property("width", 480i32)
            .property("height", 360i32)
            .property_from_str("plane-strides", &turned.strides())
            .property_from_str("plane-offsets", &turned.offsets())
            .property("frame-size", turned.frame_size)
            .build()
            .unwrap();
        assert_eq!(parse.property::<u32>("frame-size"), 276_480);
    }

    #[test]
    fn only_a_pi_camera_counts_as_scaling_for_free() {
        let device = |factory: &str| crate::camera::DeviceInfo {
            factory: factory.into(),
            ..Default::default()
        };
        let found = Found {
            probe: crate::camera::Probe {
                devices: vec![device("libcamerasrc"), device("v4l2src")],
                ..Default::default()
            },
            devices: Vec::new(),
        };
        assert!(isp_scales(&Plan::Rpicam, &found));
        assert!(isp_scales(&Plan::Libcamera, &found));
        assert!(isp_scales(
            &Plan::Device {
                index: 0,
                mjpeg: false
            },
            &found
        ));
        assert!(!isp_scales(
            &Plan::Device {
                index: 1,
                mjpeg: false
            },
            &found
        ));
        assert!(!isp_scales(
            &Plan::Device {
                index: 9,
                mjpeg: false
            },
            &found
        ));
        assert!(!isp_scales(&Plan::V4l2Path("/dev/video0".into()), &found));
        assert!(!isp_scales(&Plan::Test(None), &found));
    }

    fn visits(status: &Arc<Status>) -> Visits {
        Visits {
            presence: Presence::new(Duration::from_millis(500)),
            visit: None,
            width: 64,
            height: 48,
            privacy: Privacy::Off,
            boxes: Arc::new(Mutex::new(Vec::new())),
            status: status.clone(),
            notifier: None,
            clip: None,
        }
    }

    /// A cat over the middle quarter of the 64x48 frame.
    fn middle_cat() -> Vec<Detection> {
        vec![Detection {
            x1: 16.0,
            y1: 12.0,
            x2: 48.0,
            y2: 36.0,
            score: 0.6,
            class_id: 15,
        }]
    }

    fn health(status: &Status) -> serde_json::Value {
        serde_json::from_str(&status.to_json()).unwrap()
    }

    #[test]
    fn a_visit_opens_on_the_second_hit_and_ends_a_minute_after_the_last() {
        let dir = tempfile::tempdir().unwrap();
        let status = Arc::new(Status::new("Cam", true, 64, 48).remember_in(dir.path().into()));
        let mut v = visits(&status);
        let frame = vec![128u8; frame::size(64, 48)];
        let t = Instant::now();
        let at = |s: f32| t + Duration::from_secs_f32(s);

        v.record(at(0.0), &frame, middle_cat(), None);
        assert_eq!(health(&status)["cat"]["visits"], 0);
        assert!(v.boxes.lock().unwrap().is_empty());

        v.record(at(0.5), &frame, middle_cat(), None);
        let h = health(&status);
        assert_eq!(h["cat"]["visits"], 1);
        assert_eq!(h["cat"]["in_view"], true);
        assert_eq!(
            h["cat"]["recent"][0]["boxes"],
            serde_json::json!([[0.25, 0.25, 0.75, 0.75]])
        );
        assert!(status.arrival_jpeg(None).is_some());
        assert_eq!(v.boxes.lock().unwrap().len(), 1);
        assert!(v.visit.is_some());
        assert!(dir.path().join("cat.json").is_file());

        // Behind the sofa: out of view after the linger, the visit still open.
        v.record(at(4.0), &frame, Vec::new(), None);
        let h = health(&status);
        assert_eq!(h["cat"]["in_view"], false);
        assert!(h["cat"]["recent"][0]["stayed_s"].is_null());
        assert!(v.boxes.lock().unwrap().is_empty());

        v.record(at(30.0), &frame, middle_cat(), None);
        v.record(at(30.5), &frame, middle_cat(), None);
        assert_eq!(health(&status)["cat"]["in_view"], true);
        assert_eq!(health(&status)["cat"]["visits"], 1);

        v.record(at(90.0), &frame, Vec::new(), None);
        assert!(health(&status)["cat"]["recent"][0]["stayed_s"].is_null());
        v.record(at(91.0), &frame, Vec::new(), None);
        assert_eq!(health(&status)["cat"]["recent"][0]["stayed_s"], 30);
        assert!(v.visit.is_none());
    }

    #[test]
    fn a_cat_over_a_spot_marked_not_a_cat_starts_no_visit() {
        let status = Arc::new(Status::new("Cam", true, 64, 48));
        status
            .apply(crate::status::Settings {
                ignore: Some(vec![[0.25, 0.25, 0.75, 0.75]]),
                ..Default::default()
            })
            .unwrap();
        let mut v = visits(&status);
        let frame = vec![128u8; frame::size(64, 48)];
        let t = Instant::now();
        v.record(t, &frame, middle_cat(), None);
        v.record(t + Duration::from_millis(500), &frame, middle_cat(), None);
        assert_eq!(health(&status)["cat"]["visits"], 0);
        assert!(status.arrival_jpeg(None).is_none());
        assert!(v.boxes.lock().unwrap().is_empty());
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
    fn the_arrival_photo_is_filtered_too() {
        let frame: Vec<u8> = (0..frame::size(64, 48))
            .map(|i| (i * 7 % 251) as u8)
            .collect();
        let open = arrival_jpeg(&frame, 64, 48, &middle_cat(), Privacy::Off).unwrap();
        let filtered = arrival_jpeg(&frame, 64, 48, &middle_cat(), Privacy::Pixelate).unwrap();
        assert_ne!(open, filtered);
    }
}
