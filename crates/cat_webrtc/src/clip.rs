//! A short H.264 clip of each arrival, from a few seconds before it to a few after, so a phone shows
//! the cat coming in and not only the frame it was confirmed on.

use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context as _};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::notify::Notifier;
use crate::status::Status;

/// Frames a second in a clip; a third of the stream's still follows a cat.
pub const CLIP_FPS: u32 = 10;
/// Seconds of a clip before the arrival: the cat walking in.
pub const PRE_ROLL_S: u32 = 3;
/// The encoder's bitrate: about 100 kB a second of 640x480.
const BITRATE: u32 = 800_000;
/// How long encoding one clip may take before it is given up.
const ENCODE_TIMEOUT: Duration = Duration::from_secs(60);

/// Collects frames: one in every `every` into a ring of the last `before`, and after [`Collector::start`]
/// `after` more, then hands back the clip. Generic, so its tests need no GStreamer.
pub struct Collector<T> {
    every: u32,
    tick: u32,
    before: usize,
    after: usize,
    ring: VecDeque<T>,
    /// The clip so far, and how many of its frames came from the ring.
    recording: Option<(Vec<T>, usize)>,
}

impl<T> Collector<T> {
    pub fn new(every: u32, before: usize, after: usize) -> Self {
        Self {
            every: every.max(1),
            tick: 0,
            before,
            after,
            ring: VecDeque::with_capacity(before + 1),
            recording: None,
        }
    }

    /// Starts a clip with the frames already in the ring; `false` while one is being recorded.
    pub fn start(&mut self) -> bool {
        if self.recording.is_some() {
            return false;
        }
        let pre: Vec<T> = self.ring.drain(..).collect();
        let taken = pre.len();
        self.recording = Some((pre, taken));
        true
    }

    /// Takes one stream frame; returns the finished clip when this frame completed one.
    pub fn push(&mut self, frame: T) -> Option<Vec<T>> {
        let keep = self.tick == 0;
        self.tick = (self.tick + 1) % self.every;
        if !keep {
            return None;
        }
        match &mut self.recording {
            Some((clip, pre)) => {
                clip.push(frame);
                if clip.len() - *pre < self.after {
                    return None;
                }
                self.recording.take().map(|(clip, _)| clip)
            }
            None => {
                self.ring.push_back(frame);
                if self.ring.len() > self.before {
                    self.ring.pop_front();
                }
                None
            }
        }
    }
}

/// The arrival a clip belongs to, as `/healthz` and the webhook name it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrival {
    pub time: u64,
    pub best_score: f32,
}

/// What the capture and inference threads share: frames go in, an arrival starts a clip, and a thread
/// of its own encodes it to `/arrival.mp4` and tells the webhook.
pub struct Clip {
    collector: Mutex<Collector<gst::Buffer>>,
    arrival: Mutex<Option<Arrival>>,
    tx: mpsc::SyncSender<(Arrival, Vec<gst::Buffer>)>,
}

impl Clip {
    /// `None` when `seconds` is 0, or when this GStreamer cannot encode one (named in the log).
    pub fn spawn(
        seconds: u32,
        (width, height, fps): (u32, u32, u32),
        status: Arc<Status>,
        notifier: Option<Notifier>,
    ) -> Option<Arc<Self>> {
        if seconds == 0 {
            return None;
        }
        if let Some(missing) = ["appsrc", "openh264enc", "h264parse", "mp4mux", "filesink"]
            .into_iter()
            .find(|e| gst::ElementFactory::find(e).is_none())
        {
            log::warn!("arrival clips off: GStreamer has no {missing}");
            return None;
        }
        let every = (fps as f32 / CLIP_FPS as f32).round().max(1.0) as u32;
        let rate = gst::Fraction::new(fps as i32, every as i32);
        let per_second = fps as f32 / every as f32;
        let before = (PRE_ROLL_S as f32 * per_second).round() as usize;
        let after =
            (seconds.saturating_sub(PRE_ROLL_S).max(1) as f32 * per_second).round() as usize;
        // One clip in flight; a second arrival during an encode keeps the first clip.
        let (tx, rx) = mpsc::sync_channel::<(Arrival, Vec<gst::Buffer>)>(1);
        let spawned = thread::Builder::new()
            .name("cat-clip".into())
            .spawn(move || {
                for (arrival, frames) in rx {
                    match encode_mp4(&frames, width, height, rate) {
                        Ok(mp4) => {
                            log::info!(
                                "arrival clip: {} frames, {} kB",
                                frames.len(),
                                mp4.len() / 1000
                            );
                            status.set_clip(mp4, arrival.time);
                            if let Some(notifier) = &notifier {
                                notifier.send_clip(arrival);
                            }
                        }
                        Err(err) => log::warn!("arrival clip: {err:#}"),
                    }
                }
            });
        if let Err(err) = spawned {
            log::warn!("arrival clips off: no encoder thread ({err})");
            return None;
        }
        Some(Arc::new(Self {
            collector: Mutex::new(Collector::new(every, before, after)),
            arrival: Mutex::new(None),
            tx,
        }))
    }

    /// The frame just pushed to the stream; a buffer is refcounted, so keeping it copies nothing.
    pub fn frame(&self, frame: &gst::Buffer) {
        let finished = self
            .collector
            .lock()
            .ok()
            .and_then(|mut c| c.push(frame.clone()));
        if let Some(frames) = finished {
            let arrival = self.arrival.lock().ok().and_then(|mut a| a.take());
            if let Some(arrival) = arrival {
                if self.tx.try_send((arrival, frames)).is_err() {
                    log::warn!("arrival clip dropped: the last one is still encoding");
                }
            }
        }
    }

    /// A cat arrived: the clip takes the ring's frames and the next few seconds.
    pub fn start(&self, arrival: Arrival) {
        let started = self.collector.lock().is_ok_and(|mut c| c.start());
        if started {
            if let Ok(mut current) = self.arrival.lock() {
                *current = Some(arrival);
            }
        }
    }
}

/// I420 frames at `rate` to an MP4 with its index first, so a browser plays it before the end arrives.
pub fn encode_mp4(
    frames: &[gst::Buffer],
    width: u32,
    height: u32,
    rate: gst::Fraction,
) -> anyhow::Result<Vec<u8>> {
    if frames.is_empty() {
        bail!("no frames");
    }
    let dir = tempfile_dir()?;
    let out = dir.join("clip.mp4");
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("appsrc")
        .build()
        .context("appsrc")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| anyhow!("element 'appsrc' is not an AppSrc"))?;
    src.set_format(gst::Format::Time);
    src.set_caps(Some(
        &gst::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("width", width as i32)
            .field("height", height as i32)
            .field("framerate", rate)
            .build(),
    ));
    let encoder = gst::ElementFactory::make("openh264enc")
        .property("bitrate", BITRATE)
        .property("gop-size", CLIP_FPS)
        .build()
        .context("openh264enc")?;
    let parse = gst::ElementFactory::make("h264parse")
        .build()
        .context("h264parse")?;
    let mux = gst::ElementFactory::make("mp4mux")
        .property("faststart", true)
        .property(
            "faststart-file",
            dir.join("moov.tmp").to_string_lossy().as_ref(),
        )
        .build()
        .context("mp4mux")?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", out.to_string_lossy().as_ref())
        .build()
        .context("filesink")?;
    pipeline.add_many([src.upcast_ref(), &encoder, &parse, &mux, &sink])?;
    gst::Element::link_many([src.upcast_ref(), &encoder, &parse, &mux, &sink])?;
    let result = (|| {
        pipeline
            .set_state(gst::State::Playing)
            .context("clip pipeline to PLAYING")?;
        let step = gst::ClockTime::SECOND
            .mul_div_floor(rate.denom() as u64, rate.numer() as u64)
            .context("frame duration")?;
        for (i, frame) in frames.iter().enumerate() {
            let mut buffer = frame.copy();
            {
                let buffer = buffer.get_mut().context("clip buffer")?;
                buffer.set_pts(step * i as u64);
                buffer.set_duration(step);
            }
            src.push_buffer(buffer)
                .map_err(|err| anyhow!("clip push: {err:?}"))?;
        }
        src.end_of_stream()
            .map_err(|err| anyhow!("clip end: {err:?}"))?;
        let bus = pipeline.bus().context("clip pipeline has no bus")?;
        match bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(ENCODE_TIMEOUT.as_secs()),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) {
            Some(msg) => match msg.view() {
                gst::MessageView::Error(err) => bail!("{} ({:?})", err.error(), err.debug()),
                _ => Ok(()),
            },
            None => bail!("encoding took over {} s", ENCODE_TIMEOUT.as_secs()),
        }
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result?;
    let mp4 = std::fs::read(&out).with_context(|| format!("read {}", out.display()))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(mp4)
}

/// A private directory for one encode; the service's sandbox gives it a /tmp of its own.
fn tempfile_dir() -> anyhow::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "catcam-clip-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_last_seconds_at_the_clip_rate() {
        let mut c = Collector::new(3, 4, 2);
        for frame in 0..30 {
            assert!(c.push(frame).is_none());
        }
        // One in three of 0..30 is 0, 3, ..., 27; the ring holds the last four.
        assert!(c.start());
        assert_eq!(c.push(30), None);
        assert_eq!(c.push(31), None);
        assert_eq!(c.push(32), None);
        assert_eq!(c.push(33), Some(vec![18, 21, 24, 27, 30, 33]));
    }

    #[test]
    fn an_arrival_right_after_the_start_has_a_short_pre_roll() {
        let mut c = Collector::new(1, 30, 2);
        c.push(1);
        assert!(c.start());
        assert_eq!(c.push(2), None);
        assert_eq!(c.push(3), Some(vec![1, 2, 3]));
    }

    #[test]
    fn a_second_start_while_recording_keeps_the_first_clip() {
        let mut c = Collector::new(1, 2, 2);
        c.push(1);
        c.push(2);
        assert!(c.start());
        assert!(!c.start());
        c.push(3);
        assert_eq!(c.push(4), Some(vec![1, 2, 3, 4]));
        // Recording is over, so the next arrival starts a new clip from an empty ring.
        assert!(c.start());
        c.push(5);
        assert_eq!(c.push(6), Some(vec![5, 6]));
    }

    #[test]
    fn frames_of_an_mp4_clip_come_out_as_one_with_its_index_first() {
        gst::init().unwrap();
        if ["openh264enc", "h264parse", "mp4mux"]
            .iter()
            .any(|e| gst::ElementFactory::find(e).is_none())
        {
            eprintln!("skipped: this GStreamer cannot encode H.264 to MP4");
            return;
        }
        let (width, height) = (64u32, 48u32);
        let frames: Vec<gst::Buffer> = (0..20u8)
            .map(|i| {
                let mut i420 = vec![16u8; crate::frame::size(width, height)];
                i420[..(width * height) as usize].fill(16 + i * 10);
                gst::Buffer::from_mut_slice(i420)
            })
            .collect();
        let mp4 = encode_mp4(&frames, width, height, gst::Fraction::new(10, 1)).unwrap();
        assert_eq!(&mp4[4..8], b"ftyp");
        // faststart: the index (moov) precedes the media (mdat), so playback starts at once.
        let moov = mp4.windows(4).position(|w| w == b"moov").unwrap();
        let mdat = mp4.windows(4).position(|w| w == b"mdat").unwrap();
        assert!(moov < mdat, "moov at {moov}, mdat at {mdat}");
    }
}
