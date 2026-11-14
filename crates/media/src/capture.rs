//! Webcam → RGBA frame capture.
//! Frames overwrite a single [`FrameSlot`], so slow inference never back-pressures capture.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::ensure_gst_initialized;

/// Which video source element feeds the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraSource {
    /// First device of the best source: `mfvideosrc`, else `ksvideosrc`, else `autovideosrc`.
    Auto,
    /// Webcam by enumeration index (`device-index` property).
    Device(u32),
    /// `videotestsrc` — for containers/CI where no camera device exists.
    Test,
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureConfig {
    pub source: CameraSource,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            source: CameraSource::Auto,
            width: 1280,
            height: 720,
            framerate: 30,
        }
    }
}

/// One RGBA video frame, tightly packed (`width * height * 4` bytes).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Pipeline timestamp in milliseconds, if the buffer carried one.
    pub pts_ms: Option<u64>,
    /// Monotonic frame counter for this session (starts at 1).
    pub sequence: u64,
}

struct SlotInner {
    latest: Mutex<Option<VideoFrame>>,
    signal: Condvar,
    closed: AtomicBool,
    produced: AtomicU64,
    error: Mutex<Option<String>>,
}

/// Runs on the capture thread for every frame, before a consumer can take it.
pub type FrameObserver = Box<dyn FnMut(&VideoFrame) + Send>;

/// Latest-frame slot shared between the sink thread and consumers.
#[derive(Clone)]
pub struct FrameSlot {
    inner: Arc<SlotInner>,
}

impl FrameSlot {
    fn new() -> Self {
        Self {
            inner: Arc::new(SlotInner {
                latest: Mutex::new(None),
                signal: Condvar::new(),
                closed: AtomicBool::new(false),
                produced: AtomicU64::new(0),
                error: Mutex::new(None),
            }),
        }
    }

    /// The buffer of a frame nobody took, emptied: a slow consumer's capture reuses one allocation.
    fn recycled_buffer(&self) -> Vec<u8> {
        let stale = self
            .inner
            .latest
            .lock()
            .expect("frame slot poisoned")
            .take();
        let mut buffer = stale.map(|frame| frame.rgba).unwrap_or_default();
        buffer.clear();
        buffer
    }

    fn publish(&self, frame: VideoFrame) {
        let mut slot = self.inner.latest.lock().expect("frame slot poisoned");
        *slot = Some(frame);
        self.inner.produced.fetch_add(1, Ordering::Relaxed);
        drop(slot);
        self.inner.signal.notify_all();
    }

    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
        self.inner.signal.notify_all();
    }

    /// Closes the slot with the reason the capture ended; the first reason stays.
    fn fail(&self, reason: String) {
        self.inner
            .error
            .lock()
            .expect("frame slot poisoned")
            .get_or_insert(reason);
        self.close();
    }

    /// Why the capture ended by itself (camera unplugged, caps refused); `None` while it runs or after a stop.
    pub fn error(&self) -> Option<String> {
        self.inner
            .error
            .lock()
            .expect("frame slot poisoned")
            .clone()
    }

    /// Takes the newest frame, waiting up to `timeout`; `None` on timeout or once closed.
    pub fn take_latest(&self, timeout: Duration) -> Option<VideoFrame> {
        let mut slot = self.inner.latest.lock().expect("frame slot poisoned");
        loop {
            if let Some(frame) = slot.take() {
                return Some(frame);
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return None;
            }
            let (guard, wait) = self
                .inner
                .signal
                .wait_timeout(slot, timeout)
                .expect("frame slot poisoned");
            slot = guard;
            if wait.timed_out() {
                return slot.take();
            }
        }
    }

    /// Total frames produced by the pipeline (including overwritten ones).
    pub fn produced(&self) -> u64 {
        self.inner.produced.load(Ordering::Relaxed)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
}

/// Picks the best available webcam source element for this platform/registry.
fn webcam_source_factory() -> Result<&'static str> {
    for name in ["mfvideosrc", "ksvideosrc", "autovideosrc", "v4l2src"] {
        if gst::ElementFactory::find(name).is_some() {
            return Ok(name);
        }
    }
    anyhow::bail!("no webcam source element found in the GStreamer registry")
}

pub struct CaptureSession {
    pipeline: gst::Pipeline,
    frames: FrameSlot,
}

/// RGBA at the asked size with square pixels, at the asked rate where the source has it, else at any.
fn frame_caps(config: &CaptureConfig) -> gst::Caps {
    // Without the pixel aspect ratio `videoscale` keeps a 4:3 camera's shape with non-square pixels, which no texture honours.
    let any_rate = gst::Structure::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build();
    let mut caps = gst::Caps::builder_full();
    if config.framerate > 0 {
        let mut exact = any_rate.clone();
        exact.set("framerate", gst::Fraction::new(config.framerate as i32, 1));
        caps = caps.structure(exact);
    }
    caps.structure(any_rate).build()
}

impl CaptureSession {
    /// Builds and starts the capture pipeline.
    pub fn start(config: CaptureConfig) -> Result<Self> {
        Self::start_observed(config, None)
    }

    /// [`Self::start`], with `observer` called for every frame the camera delivers.
    pub fn start_observed(config: CaptureConfig, observer: Option<FrameObserver>) -> Result<Self> {
        ensure_gst_initialized()?;

        let source = match config.source {
            CameraSource::Test => gst::ElementFactory::make("videotestsrc")
                .property_from_str("pattern", "smpte")
                .property("is-live", true)
                .build()
                .context("failed to create videotestsrc")?,
            CameraSource::Auto | CameraSource::Device(_) => {
                let factory = webcam_source_factory()?;
                log::info!("using webcam source element `{factory}`");
                match config.source {
                    // Only these two have `device-index`; setting it on a fallback would panic.
                    CameraSource::Device(index)
                        if matches!(factory, "mfvideosrc" | "ksvideosrc") =>
                    {
                        gst::ElementFactory::make(factory)
                            .property("device-index", index as i32)
                            .build()
                    }
                    // Elsewhere the listed device builds its own source, so the index picks the camera it names.
                    CameraSource::Device(index) => {
                        return Self::from_source(
                            crate::devices::camera_element(index)?,
                            &config,
                            observer,
                        )
                    }
                    _ => gst::ElementFactory::make(factory).build(),
                }
                .with_context(|| format!("failed to create `{factory}`"))?
            }
        };
        Self::from_source(source, &config, observer)
    }

    /// The pipeline behind `source`: convert, scale, and an appsink feeding the frame slot.
    fn from_source(
        source: gst::Element,
        config: &CaptureConfig,
        mut observer: Option<FrameObserver>,
    ) -> Result<Self> {
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .context("failed to create videoconvert")?;
        let videoscale = gst::ElementFactory::make("videoscale")
            .build()
            .context("failed to create videoscale")?;

        let caps = frame_caps(config);

        // Latest-frame semantics at the sink too: never queue stale video.
        let sink = crate::frame_sink(1, false)?;
        sink.set_caps(Some(&caps));

        let pipeline = gst::Pipeline::new();
        pipeline
            .add_many([&source, &videoconvert, &videoscale, sink.upcast_ref()])
            .context("failed to assemble pipeline")?;
        gst::Element::link_many([&source, &videoconvert, &videoscale, sink.upcast_ref()])
            .context("failed to link pipeline (source may not support requested caps)")?;

        let frames = FrameSlot::new();
        let slot = frames.clone();
        let mut sequence: u64 = 0;
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    sequence += 1;
                    // The bus only says "internal data stream error"; the slot keeps what was wrong with the sample.
                    let frame = frame_from_sample(&sample, slot.recycled_buffer(), sequence)
                        .map_err(|reason| {
                            slot.fail(format!("appsink: {reason}"));
                            gst::FlowError::Error
                        })?;
                    if let Some(observer) = observer.as_mut() {
                        observer(&frame);
                    }
                    slot.publish(frame);
                    Ok(gst::FlowSuccess::Ok)
                })
                .eos({
                    let slot = frames.clone();
                    move |_| slot.close()
                })
                .build(),
        );

        // Nothing else reads this bus: an unplugged camera or refused caps would end the frames without a word.
        let bus = pipeline.bus().context("capture pipeline has no bus")?;
        let slot = frames.clone();
        bus.set_sync_handler(move |_, message| {
            match message.view() {
                gst::MessageView::Error(error) => {
                    let source = error.src().map(|s| s.path_string()).unwrap_or_default();
                    slot.fail(format!("{} ({source})", error.error()));
                }
                gst::MessageView::Eos(_) => slot.close(),
                _ => {}
            }
            gst::BusSyncReply::Drop
        });

        if let Err(error) = pipeline.set_state(gst::State::Playing) {
            // A half-started pipeline keeps the camera open until it reaches Null.
            let _ = pipeline.set_state(gst::State::Null);
            bus.unset_sync_handler();
            let reason = frames.error().unwrap_or_else(|| error.to_string());
            anyhow::bail!("failed to set pipeline to Playing: {reason}");
        }

        Ok(Self { pipeline, frames })
    }

    /// Shared handle for pulling frames; stays valid until [`stop`](Self::stop).
    pub fn frames(&self) -> FrameSlot {
        self.frames.clone()
    }

    /// Stops the pipeline and wakes all waiting consumers.
    pub fn stop(&self) {
        self.frames.close();
        // Before the teardown: what a stopping pipeline posts is no capture failure.
        if let Some(bus) = self.pipeline.bus() {
            bus.unset_sync_handler();
        }
        if let Err(e) = self.pipeline.set_state(gst::State::Null) {
            log::warn!("failed to stop capture pipeline: {e}");
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One appsink sample as its frame, or why it is none.
fn frame_from_sample(
    sample: &gst::Sample,
    mut rgba: Vec<u8>,
    sequence: u64,
) -> Result<VideoFrame, String> {
    let buffer = sample.buffer().ok_or("a sample without a buffer")?;
    let caps = sample.caps().ok_or("a sample without caps")?;
    let info = gstreamer_video::VideoInfo::from_caps(caps)
        .map_err(|error| format!("caps it cannot read as video ({caps}): {error}"))?;
    let map = buffer
        .map_readable()
        .map_err(|error| format!("a buffer it cannot map: {error}"))?;
    rgba.extend_from_slice(map.as_slice());
    Ok(VideoFrame {
        rgba,
        width: info.width(),
        height: info.height(),
        pts_ms: buffer.pts().map(|t| t.mseconds()),
        sequence,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_sample_that_is_no_frame_says_why() {
        ensure_gst_initialized().unwrap();
        let buffer = gst::Buffer::with_size(4).unwrap();
        let bare = gst::Sample::builder().buffer(&buffer).build();
        let reason = frame_from_sample(&bare, Vec::new(), 1).unwrap_err();
        assert_eq!(reason, "a sample without caps");
        let audio = gst::Caps::builder("audio/x-raw").build();
        let sound = gst::Sample::builder().buffer(&buffer).caps(&audio).build();
        let reason = frame_from_sample(&sound, Vec::new(), 1).unwrap_err();
        assert!(
            reason.starts_with("caps it cannot read as video (audio/x-raw)"),
            "{reason}"
        );
    }

    fn config(framerate: u32) -> CaptureConfig {
        CaptureConfig {
            source: CameraSource::Test,
            width: 64,
            height: 48,
            framerate,
        }
    }

    /// A source bin from a launch line, so a test can end or break the stream at a chosen frame.
    fn session(description: &str, observer: Option<FrameObserver>) -> Result<CaptureSession> {
        ensure_gst_initialized()?;
        let source = gst::parse::bin_from_description(description, true)?;
        CaptureSession::from_source(source.upcast(), &config(30), observer)
    }

    fn wait_closed(frames: &FrameSlot) {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !frames.is_closed() {
            assert!(
                std::time::Instant::now() < deadline,
                "the slot never closed"
            );
            frames.take_latest(Duration::from_millis(50));
        }
    }

    #[test]
    fn the_caps_ask_for_square_pixels_and_prefer_the_asked_rate() {
        ensure_gst_initialized().unwrap();
        let caps = frame_caps(&config(15));
        assert_eq!(caps.size(), 2);
        for structure in caps.iter() {
            assert_eq!(structure.get::<&str>("format").unwrap(), "RGBA");
            assert_eq!(structure.get::<i32>("width").unwrap(), 64);
            let par = structure
                .get::<gst::Fraction>("pixel-aspect-ratio")
                .unwrap();
            assert_eq!(par, gst::Fraction::new(1, 1));
        }
        let rate = caps.structure(0).unwrap().get::<gst::Fraction>("framerate");
        assert_eq!(rate.unwrap(), gst::Fraction::new(15, 1));
        assert!(!caps.structure(1).unwrap().has_field("framerate"));
        // No rate asked: the camera's own, and nothing to fall back from.
        assert_eq!(frame_caps(&config(0)).size(), 1);
    }

    #[test]
    fn the_observer_sees_every_frame_though_nobody_takes_one() {
        let (tx, rx) = mpsc::channel();
        let observer: FrameObserver = Box::new(move |frame| {
            let _ = tx.send((frame.sequence, frame.rgba.len()));
        });
        let session = session("videotestsrc num-buffers=5", Some(observer)).unwrap();
        let frames = session.frames();
        let seen: Vec<(u64, usize)> = (0..5)
            .map(|_| rx.recv_timeout(Duration::from_secs(20)).expect("a frame"))
            .collect();
        assert_eq!(seen, (1..=5).map(|n| (n, 64 * 48 * 4)).collect::<Vec<_>>());
        // The stream's own end closes the slot, and is no failure.
        wait_closed(&frames);
        assert_eq!(frames.error(), None);
        assert_eq!(frames.produced(), 5);
    }

    #[test]
    fn an_error_mid_stream_closes_the_slot_and_keeps_its_reason() {
        let session = session("videotestsrc is-live=true ! identity error-after=3", None).unwrap();
        let frames = session.frames();
        wait_closed(&frames);
        let reason = frames.error().expect("the pipeline's error");
        assert!(reason.contains("identity"), "{reason}");
        // A stop afterwards keeps the reason the stream ended for.
        session.stop();
        assert_eq!(frames.error(), Some(reason));
    }

    #[test]
    fn a_stop_is_no_error() {
        let session = session("videotestsrc is-live=true", None).unwrap();
        let frames = session.frames();
        assert!(frames.take_latest(Duration::from_secs(20)).is_some());
        session.stop();
        assert!(frames.is_closed());
        assert_eq!(frames.error(), None);
    }

    #[test]
    fn a_source_that_cannot_start_fails_with_its_reason_and_is_torn_down() {
        let Err(error) = session("filesrc location=/nonexistent/clip.raw", None) else {
            panic!("a missing file started");
        };
        let message = format!("{error:#}");
        assert!(
            message.contains("failed to set pipeline to Playing"),
            "{message}"
        );
        // The bus's own words and the element they came from, not just "state change failed".
        assert!(message.contains("GstFileSrc"), "{message}");
    }

    #[test]
    fn a_frame_nobody_took_lends_its_buffer_to_the_next() {
        let slot = FrameSlot::new();
        assert!(slot.recycled_buffer().is_empty());
        let rgba = Vec::with_capacity(4096);
        let address = rgba.as_ptr();
        slot.publish(VideoFrame {
            rgba,
            width: 1,
            height: 1,
            pts_ms: None,
            sequence: 1,
        });
        let reused = slot.recycled_buffer();
        assert!(reused.is_empty());
        assert_eq!((reused.as_ptr(), reused.capacity()), (address, 4096));
        assert!(slot.take_latest(Duration::ZERO).is_none());
    }

    #[test]
    fn a_camera_index_past_the_list_names_how_many_there_are() {
        let listed = crate::list_cameras().unwrap().len();
        let error = crate::devices::camera_element(listed as u32 + 7).unwrap_err();
        assert!(
            format!("{error:#}").contains(&format!("{listed} listed")),
            "{error:#}"
        );
    }
}
