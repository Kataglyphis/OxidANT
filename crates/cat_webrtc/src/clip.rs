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

/// Clips recorded at once at most; overlapping ones share their frames, as a buffer is refcounted.
pub const MAX_RECORDINGS: usize = 3;

/// Collects frames: one in every `every` into a ring of the last `before`, and after each
/// [`Collector::start`] `after` more, then hands back that clip with its tag. Generic, so its tests
/// need no GStreamer.
pub struct Collector<T, A> {
    every: u32,
    tick: u32,
    before: usize,
    after: usize,
    ring: VecDeque<T>,
    /// Each clip being recorded: what it belongs to, its frames so far, how many came from the ring.
    recordings: Vec<(A, Vec<T>, usize)>,
}

impl<T: Clone, A> Collector<T, A> {
    pub fn new(every: u32, before: usize, after: usize) -> Self {
        Self {
            every: every.max(1),
            tick: 0,
            before,
            after,
            ring: VecDeque::with_capacity(before + 1),
            recordings: Vec::new(),
        }
    }

    /// Starts a clip for `tag` with the frames in the ring; `false` while [`MAX_RECORDINGS`] run.
    pub fn start(&mut self, tag: A) -> bool {
        if self.recordings.len() >= MAX_RECORDINGS {
            return false;
        }
        let pre: Vec<T> = self.ring.iter().cloned().collect();
        let taken = pre.len();
        self.recordings.push((tag, pre, taken));
        true
    }

    /// Takes one stream frame; returns the clips this frame completed.
    pub fn push(&mut self, frame: T) -> Vec<(A, Vec<T>)> {
        let keep = self.tick == 0;
        self.tick = (self.tick + 1) % self.every;
        if !keep {
            return Vec::new();
        }
        for (_, clip, _) in &mut self.recordings {
            clip.push(frame.clone());
        }
        self.ring.push_back(frame);
        if self.ring.len() > self.before {
            self.ring.pop_front();
        }
        let after = self.after;
        let (done, running): (Vec<_>, Vec<_>) = std::mem::take(&mut self.recordings)
            .into_iter()
            .partition(|(_, clip, pre)| clip.len() - pre >= after);
        self.recordings = running;
        done.into_iter().map(|(tag, clip, _)| (tag, clip)).collect()
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
    collector: Mutex<Collector<gst::Buffer, Arrival>>,
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
        // Two clips may wait while one encodes: arrivals close together each keep theirs.
        let (tx, rx) = mpsc::sync_channel::<(Arrival, Vec<gst::Buffer>)>(2);
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
                                notifier.send_clip(arrival, status.visit_stay(arrival.time));
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
            tx,
        }))
    }

    /// The frame just pushed to the stream; a buffer is refcounted, so keeping it copies nothing.
    pub fn frame(&self, frame: &gst::Buffer) {
        let finished = self
            .collector
            .lock()
            .map(|mut c| c.push(frame.clone()))
            .unwrap_or_default();
        for (arrival, frames) in finished {
            if self.tx.try_send((arrival, frames)).is_err() {
                log::warn!(
                    "arrival clip of {} dropped: two are still encoding",
                    arrival.time
                );
            }
        }
    }

    /// A cat arrived: its clip takes the ring's frames and the next few seconds.
    pub fn start(&self, arrival: Arrival) {
        if !self.collector.lock().is_ok_and(|mut c| c.start(arrival)) {
            log::warn!(
                "no clip for the arrival at {}: {MAX_RECORDINGS} are recording",
                arrival.time
            );
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
    // openh264enc reads a frame's full size whatever the buffer holds: a short one crashed the process.
    let want = crate::frame::size(width, height);
    if let Some(short) = frames.iter().find(|f| f.size() != want) {
        bail!(
            "a frame of {} bytes, not the {want} of {width}x{height} I420",
            short.size()
        );
    }
    // Declared before the pipeline, so it drops after it: Windows deletes no file a filesink holds.
    let scratch = Scratch(tempfile_dir()?);
    let dir = &scratch.0;
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
        // mul_div_floor panics on a zero denominator, which would end the encoder thread for good.
        let (num, den) = (rate.numer(), rate.denom());
        if num <= 0 || den <= 0 {
            bail!("frame rate {num}/{den}");
        }
        let step = gst::ClockTime::SECOND
            .mul_div_floor(den as u64, num as u64)
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
    result.and_then(|()| std::fs::read(&out).with_context(|| format!("read {}", out.display())))
}

/// One encode's scratch directory, removed however [`encode_mp4`] returns: a GStreamer without
/// one of its elements returned before the cleanup, and left a directory per arrival.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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

    /// One encode at a time: the scratch-directory count must not see a parallel test's.
    static ENCODES: Mutex<()> = Mutex::new(());

    #[test]
    fn the_ring_keeps_the_last_seconds_at_the_clip_rate() {
        let mut c = Collector::new(3, 4, 2);
        for frame in 0..30 {
            assert!(c.push(frame).is_empty());
        }
        // One in three of 0..30 is 0, 3, ..., 27; the ring holds the last four.
        assert!(c.start('a'));
        assert!(c.push(30).is_empty());
        assert!(c.push(31).is_empty());
        assert!(c.push(32).is_empty());
        assert_eq!(c.push(33), vec![('a', vec![18, 21, 24, 27, 30, 33])]);
    }

    #[test]
    fn an_arrival_right_after_the_start_has_a_short_pre_roll() {
        let mut c = Collector::new(1, 30, 2);
        c.push(1);
        assert!(c.start('a'));
        assert!(c.push(2).is_empty());
        assert_eq!(c.push(3), vec![('a', vec![1, 2, 3])]);
    }

    #[test]
    fn a_second_arrival_while_recording_gets_a_clip_of_its_own() {
        let mut c = Collector::new(1, 2, 2);
        c.push(1);
        c.push(2);
        assert!(c.start('a'));
        c.push(3);
        // Its pre-roll is the ring as it is now, the first clip's frames shared.
        assert!(c.start('b'));
        assert_eq!(c.push(4), vec![('a', vec![1, 2, 3, 4])]);
        assert_eq!(c.push(5), vec![('b', vec![2, 3, 4, 5])]);
        // The ring kept filling while they recorded, so the next pre-roll is full.
        assert!(c.start('c'));
        c.push(6);
        assert_eq!(c.push(7), vec![('c', vec![4, 5, 6, 7])]);
    }

    #[test]
    fn at_most_max_recordings_run_at_once() {
        let mut c = Collector::new(1, 1, 5);
        for tag in 0..MAX_RECORDINGS {
            assert!(c.start(tag));
        }
        assert!(!c.start(MAX_RECORDINGS));
        for frame in 0..4 {
            assert!(c.push(frame).is_empty());
        }
        // All of them finish on the same frame, and a start is possible again.
        assert_eq!(c.push(4).len(), MAX_RECORDINGS);
        assert!(c.start(9));
    }

    #[test]
    fn a_frame_shorter_than_its_caps_is_refused_before_the_encoder_sees_it() {
        let _one = ENCODES.lock().unwrap_or_else(|e| e.into_inner());
        gst::init().unwrap();
        // Without the check this one 16-byte frame took the test process down with SIGSEGV.
        let frames = vec![gst::Buffer::from_mut_slice(vec![0u8; 16])];
        let err = encode_mp4(&frames, 64, 48, gst::Fraction::new(10, 1)).unwrap_err();
        assert!(err.to_string().contains("not the 4608"), "{err:#}");
    }

    #[test]
    fn a_failed_encode_leaves_no_scratch_directory() {
        let _one = ENCODES.lock().unwrap_or_else(|e| e.into_inner());
        gst::init().unwrap();
        let before = scratch_dirs();
        // A rate of 0 fails after the scratch directory exists, while the frames are pushed.
        let frames = vec![gst::Buffer::from_mut_slice(vec![
            16u8;
            crate::frame::size(64, 48)
        ])];
        assert!(encode_mp4(&frames, 64, 48, gst::Fraction::new(0, 1)).is_err());
        assert_eq!(scratch_dirs(), before);
    }

    #[test]
    fn a_gstreamer_without_the_encoder_leaves_no_scratch_directory() {
        let _one = ENCODES.lock().unwrap_or_else(|e| e.into_inner());
        gst::init().unwrap();
        let registry = gst::Registry::get();
        // Windows arm64's GStreamer has no openh264enc; hide it here for one encode.
        let hidden = registry.find_feature("openh264enc", gst::ElementFactory::static_type());
        if let Some(feature) = &hidden {
            registry.remove_feature(feature);
        }
        let before = scratch_dirs();
        let frames = vec![gst::Buffer::from_mut_slice(vec![
            16u8;
            crate::frame::size(64, 48)
        ])];
        let result = encode_mp4(&frames, 64, 48, gst::Fraction::new(10, 1));
        if let Some(feature) = &hidden {
            registry.add_feature(feature).unwrap();
        }
        assert!(format!("{:#}", result.unwrap_err()).contains("openh264enc"));
        assert_eq!(scratch_dirs(), before);
    }

    #[test]
    fn an_arrival_is_encoded_on_its_own_thread_and_kept_for_that_arrival() {
        let _one = ENCODES.lock().unwrap_or_else(|e| e.into_inner());
        gst::init().unwrap();
        if ["openh264enc", "h264parse", "mp4mux"]
            .iter()
            .any(|e| gst::ElementFactory::find(e).is_none())
        {
            eprintln!("skipped: this GStreamer cannot encode H.264 to MP4");
            return;
        }
        let status = Arc::new(Status::new("Cam", true, 64, 48));
        // 4 s at 10 fps: the 3 s pre-roll and 1 s after the arrival.
        let clip = Clip::spawn(4, (64, 48, 10), status.clone(), None).expect("an encoder");
        let frame = gst::Buffer::from_mut_slice(vec![16u8; crate::frame::size(64, 48)]);
        for _ in 0..40 {
            clip.frame(&frame);
        }
        assert!(status.clip_mp4(None).is_none(), "no clip before an arrival");
        let arrival = Arrival {
            time: 1_791_763_273,
            best_score: 0.8,
        };
        clip.start(arrival);
        for _ in 0..10 {
            clip.frame(&frame);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while status.clip_mp4(Some(arrival.time)).is_none() && std::time::Instant::now() < deadline
        {
            thread::sleep(std::time::Duration::from_millis(50));
        }
        let mp4 = status
            .clip_mp4(Some(arrival.time))
            .expect("the arrival's clip");
        assert_eq!(&mp4[4..8], b"ftyp", "an MP4");
        assert!(status.clip_mp4(Some(arrival.time + 600)).is_none());
    }

    #[test]
    fn no_seconds_no_clip_thread() {
        let status = Arc::new(Status::new("Cam", true, 64, 48));
        assert!(Clip::spawn(0, (64, 48, 10), status, None).is_none());
    }

    fn scratch_dirs() -> usize {
        let prefix = format!("catcam-clip-{}-", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count()
    }

    #[test]
    fn frames_of_an_mp4_clip_come_out_as_one_with_its_index_first() {
        let _one = ENCODES.lock().unwrap_or_else(|e| e.into_inner());
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
