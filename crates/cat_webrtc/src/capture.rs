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
use crate::config::Config;
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
}

impl Worker {
    pub fn spawn(self) -> std::io::Result<thread::JoinHandle<()>> {
        thread::Builder::new()
            .name("cat-capture".into())
            .spawn(move || self.run())
    }

    fn run(mut self) {
        let boxes: Boxes = Arc::new(Mutex::new(Vec::new()));
        let inference = self
            .detector
            .take()
            .and_then(|detector| spawn_inference(detector, &self.config, boxes.clone()));
        let frames = inference.as_ref().map(|(tx, _)| tx);
        let mut failures: u32 = 0;
        while !self.shutdown.load(Ordering::Relaxed) {
            let found = camera::probe();
            let plan = camera::choose(&self.choice, &found.probe);
            let label = plan.describe(&found.probe);
            self.status.set_camera(&label);
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
        if let Some(flip) = videoflip(config.rotate)? {
            elements.push(flip);
        }
        elements.push(make("videoscale")?);
        elements.push(capsfilter(rgba_caps(config))?);
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
            if !self.push(&sample, boxes, inference)? {
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
    ) -> anyhow::Result<bool> {
        let config = &self.config;
        let buffer = sample.buffer().context("sample without buffer")?;
        let map = buffer.map_readable().context("map frame")?;
        let rgba = map.as_slice();
        if rgba.len() < (config.width * config.height * 4) as usize {
            return Ok(true);
        }
        let mut annotated = rgba.to_vec();
        if let Ok(current) = boxes.lock() {
            for detection in current.iter() {
                draw_rect(
                    &mut annotated,
                    config.width,
                    config.height,
                    [detection.x1, detection.y1, detection.x2, detection.y2],
                    [0, 255, 0, 255],
                    4,
                );
            }
        }
        // Only while the inference thread is idle: a frame of lag is fine, a backlog is not.
        if let Some(tx) = inference {
            let _ = tx.try_send(rgba.to_vec());
        }
        match self
            .appsrc
            .push_buffer(gst::Buffer::from_mut_slice(annotated))
        {
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
    rpicam: Option<(std::process::Child, std::process::ChildStdout)>,
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
        let mut child = Command::new("rpicam-vid")
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
        let stdout = child.stdout.take().context("rpicam-vid stdout")?;
        let fdsrc = gst::ElementFactory::make("fdsrc")
            .property("fd", stdout.as_raw_fd())
            .property("blocksize", w * h * 3 / 2)
            .build()
            .context("fdsrc")?;
        let parse = gst::ElementFactory::make("rawvideoparse")
            .property_from_str("format", "i420")
            .property("width", w as i32)
            .property("height", h as i32)
            .property("framerate", gst::Fraction::new(fps as i32, 1))
            .build()
            .context("rawvideoparse (is the videoparsersbad plugin installed?)")?;
        Ok(Self {
            elements: vec![fdsrc, parse],
            rpicam: Some((child, stdout)),
        })
    }

    #[cfg(not(unix))]
    fn rpicam(_config: &Config) -> anyhow::Result<Self> {
        bail!("rpicam-vid exists on Raspberry Pi OS only")
    }
}

/// Stops the pipeline, then the child process it read from, whichever way the capture ends.
struct Running {
    pipeline: gst::Pipeline,
    #[allow(dead_code)]
    source: Source,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        #[cfg(unix)]
        if let Some((mut child, stdout)) = self.source.rpicam.take() {
            let _ = child.kill();
            let _ = child.wait();
            drop(stdout);
        }
    }
}

/// The inference thread: takes the newest frame, writes the boxes, logs when cats come and go.
/// It ends once its sender is dropped.
fn spawn_inference(
    mut detector: PersonDetector,
    config: &Config,
    boxes: Boxes,
) -> Option<(mpsc::SyncSender<Vec<u8>>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(1);
    let (width, height, score) = (config.width, config.height, config.score);
    let wanted = (!config.all_classes).then(|| vec![COCO_CAT]);
    let spawned = thread::Builder::new()
        .name("cat-infer".into())
        .spawn(move || {
            let mut seen = 0usize;
            while let Ok(rgba) = rx.recv() {
                match detector.infer_rgba(&rgba, width, height, score, wanted.as_deref()) {
                    Ok(detections) => {
                        if detections.is_empty() != (seen == 0) {
                            match detections.first() {
                                Some(first) => log::info!(
                                    "cat in view: {} detection(s), best score {:.2}",
                                    detections.len(),
                                    first.score
                                ),
                                None => log::info!("no cat in view"),
                            }
                        }
                        seen = detections.len();
                        if let Ok(mut current) = boxes.lock() {
                            *current = detections;
                        }
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

/// The frame format the output pipeline's appsrc is fixed to.
pub fn rgba_caps(config: &Config) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("framerate", gst::Fraction::new(config.fps as i32, 1))
        .build()
}

fn videoflip(rotate: u32) -> anyhow::Result<Option<gst::Element>> {
    let method = match rotate {
        0 => return Ok(None),
        90 => "clockwise",
        180 => "rotate-180",
        270 => "counterclockwise",
        other => bail!("rotate must be 0, 90, 180 or 270 (got {other})"),
    };
    Ok(Some(
        gst::ElementFactory::make("videoflip")
            .property_from_str("method", method)
            .build()
            .context("videoflip")?,
    ))
}

fn draw_rect(
    frame: &mut [u8],
    width: u32,
    height: u32,
    rect: [f32; 4],
    color: [u8; 4],
    thickness: i32,
) {
    let w = width as i32;
    let h = height as i32;
    let [x1, y1, x2, y2] = rect;
    let clamp = |v: f32, max: i32| v.round().max(0.0).min((max - 1) as f32) as i32;
    let (x1, y1, x2, y2) = (clamp(x1, w), clamp(y1, h), clamp(x2, w), clamp(y2, h));
    for t in 0..thickness {
        for x in x1..=x2 {
            put(frame, w, x, (y1 + t).min(h - 1), color);
            put(frame, w, x, (y2 - t).max(0), color);
        }
        for y in y1..=y2 {
            put(frame, w, (x1 + t).min(w - 1), y, color);
            put(frame, w, (x2 - t).max(0), y, color);
        }
    }
}

fn put(frame: &mut [u8], width: i32, x: i32, y: i32, color: [u8; 4]) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 4 <= frame.len() {
        frame[idx..idx + 4].copy_from_slice(&color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_rect_marks_the_outline_and_leaves_the_inside_alone() {
        let (w, h) = (8u32, 8u32);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        draw_rect(&mut frame, w, h, [1.0, 1.0, 6.0, 6.0], [9, 9, 9, 9], 1);
        let at = |x: u32, y: u32| frame[((y * w + x) * 4) as usize];
        assert_eq!(at(1, 1), 9);
        assert_eq!(at(6, 6), 9);
        assert_eq!(at(3, 1), 9);
        assert_eq!(at(3, 3), 0);
    }

    #[test]
    fn draw_rect_clamps_boxes_that_leave_the_frame() {
        let (w, h) = (4u32, 4u32);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        draw_rect(
            &mut frame,
            w,
            h,
            [-10.0, -10.0, 40.0, 40.0],
            [7, 7, 7, 7],
            2,
        );
        assert!(frame.contains(&7));
    }
}
