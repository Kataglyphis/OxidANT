//! The capture worker's logic that needs no camera: when to capture again, how long boxes count,
//! what a model run makes of a cat, and the frame geometry; a test drives each with set values.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::bail;
use kataglyphis_core::Detection;

use crate::camera::{Found, Plan};
use crate::clip::{Arrival, Clip};
use crate::config::{Config, Privacy};
use crate::frame;
use crate::motion::{still_period, Moving};
use crate::notify::Notifier;
use crate::presence::{best_score, outside, smaller_than, zone_of, Change, Presence};
use crate::scenario::Scenario;
use crate::status::Status;

/// How long the worker waits on a camera; a test shortens both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// A camera that delivers no frame for this long counts as gone.
    pub stall_after: Duration,
    /// A stand-in test pattern runs this long before the camera is probed again.
    pub reprobe_every: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            stall_after: Duration::from_secs(10),
            reprobe_every: Duration::from_secs(30),
        }
    }
}
/// A capture that ran this long was a working camera: the next failure backs off from 1 s again.
const STABLE_AFTER: Duration = Duration::from_secs(60);

/// How long to wait before capturing again: 1 s, doubling to 32 s while captures keep failing.
#[derive(Debug, Default)]
pub(super) struct Backoff {
    failures: u32,
}

impl Backoff {
    /// The pause after a capture that ran `ran_for`; `None` for a stand-in that ran its course,
    /// which probes for the camera again at once.
    pub(super) fn after(
        &mut self,
        ran_for: Duration,
        failed: bool,
        stand_in: bool,
    ) -> Option<Duration> {
        if ran_for > STABLE_AFTER {
            self.failures = 0;
        }
        if stand_in && !failed {
            return None;
        }
        let pause = Duration::from_secs(1 << self.failures.min(5));
        self.failures = self.failures.saturating_add(1);
        Some(pause)
    }
}

/// Waits `total`, or less once `shutdown` is set: it looks every 100 ms, so a stop never waits long.
pub(super) fn sleep_unless(shutdown: &AtomicBool, total: Duration) {
    let until = Instant::now() + total;
    while Instant::now() < until && !shutdown.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }
}

pub(super) type Boxes = Arc<Mutex<Vec<Detection>>>;

/// Boxes count at least this long, whatever the rates say: a fast model must not make them flicker.
const BOX_LIFETIME_FLOOR: Duration = Duration::from_secs(2);

/// How long a run's boxes count: two runs at the slowest rate a cat in view is looked at, each with the model's time.
pub(super) fn box_lifetime(still: Duration, model_ms: Option<f64>) -> Duration {
    let run = Duration::from_secs_f64(model_ms.unwrap_or(0.0).max(0.0) / 1000.0);
    ((still + run) * 2).max(BOX_LIFETIME_FLOOR)
}

/// Whether boxes of this age still count; a run that hangs must not leave their area sharp under the privacy filter.
pub(super) fn fresh(age: Option<Duration>, lifetime: Duration) -> bool {
    age.is_some_and(|age| age <= lifetime)
}

/// What the model's runs make of a cat: presence, the visit log, the arrival picture, the webhook
/// event and the clip. Apart from the model and the clock, so a test drives it with set times.
pub(super) struct Visits {
    presence: Presence,
    /// The open visit's arrival, which its departure event names.
    visit: Option<Arrival>,
    max_box_area: f32,
    width: u32,
    height: u32,
    privacy: Privacy,
    boxes: Boxes,
    status: Arc<Status>,
    notifier: Option<Notifier>,
    clip: Option<Arc<Clip>>,
}

impl Visits {
    /// Nothing seen yet, with the limits `config` sets.
    pub(super) fn new(
        config: &Config,
        boxes: Boxes,
        status: Arc<Status>,
        notifier: Option<Notifier>,
        clip: Option<Arc<Clip>>,
    ) -> Self {
        Self {
            // The slowest rate the model runs at with a cat in view, so a still one's visit never lapses.
            presence: Presence::new(still_period(config.inference_fps)),
            visit: None,
            max_box_area: config.max_box_area,
            width: config.width,
            height: config.height,
            privacy: config.privacy,
            boxes,
            status,
            notifier,
            clip,
        }
    }

    /// A failed run: no stale box stays drawn, unblurred under the privacy filter, and `/healthz` learns of it.
    pub(super) fn model_failed(&mut self, error: String) {
        let failures = self.status.record_model_failure(error.clone());
        // The first, then one a minute at two runs a second: a broken model would otherwise fill the journal.
        if failures == 1 || failures.is_multiple_of(120) {
            log::warn!("inference failed ({failures} in a row): {error}");
        }
        if let Ok(mut boxes) = self.boxes.lock() {
            boxes.clear();
        }
        self.status.set_boxes(Vec::new());
    }

    /// Takes one run's `detections` on the frame `i420`, seen at `now`; what it changed.
    pub(super) fn record(
        &mut self,
        now: Instant,
        i420: &[u8],
        detections: Vec<Detection>,
        moving: Option<&Moving>,
    ) -> Option<Change> {
        let (width, height) = (self.width, self.height);
        let status = &self.status;
        let detections = smaller_than(
            outside(detections, &status.ignored(), width, height),
            self.max_box_area,
            width,
            height,
        );
        let counted = counted(&detections, self.presence.in_view(), moving);
        let change = self.presence.update(now, &counted);
        match &change {
            Some(Change::Arrived { count, best, class }) => {
                let class = crate::coco::name(*class).unwrap_or("?");
                log::info!("{class} in view: {count} detection(s), best score {best:.2}");
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
        status.set_detection(in_view, best_score(&counted), change.as_ref());
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
        change
    }
}

/// One visit a replay found, in seconds since its scenario started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Visit {
    pub arrived: f64,
    /// From arrival to the last sighting; `None` while the visit is still open at the scenario's end.
    pub stayed: Option<f64>,
}

/// The visits `scenario`'s runs make under this build's rules: [`Config::default`]'s score and box
/// limit, [`Presence`], [`counted`]; the camera's size, rate and ignored spots come from its header.
pub fn replay(scenario: &Scenario) -> anyhow::Result<Vec<Visit>> {
    let h = &scenario.header;
    let defaults = Config::default();
    anyhow::ensure!(
        h.floor <= defaults.score,
        "recorded down to {} only, so a threshold of {} cannot be judged",
        h.floor,
        defaults.score
    );
    let config = Config {
        width: h.width,
        height: h.height,
        inference_fps: h.inference_fps,
        ..defaults
    };
    let status = Arc::new(Status::new("replay", true, h.width, h.height));
    status.apply(crate::status::Settings {
        ignore: Some(h.ignore.clone()),
        ..Default::default()
    })?;
    let boxes: Boxes = Arc::new(Mutex::new(Vec::new()));
    let mut visits = Visits::new(&config, boxes, status, None, None);
    let i420 = vec![128u8; frame::size(h.width, h.height)];
    let start = Instant::now();
    let mut found: Vec<Visit> = Vec::new();
    for f in &scenario.frames {
        let detections = f
            .boxes
            .iter()
            .filter(|b| b.score >= config.score)
            .map(Detection::from)
            .collect();
        let moving = f.moving.as_ref().map(Moving::from);
        let now = start + Duration::from_secs_f64(f.t);
        match visits.record(now, &i420, detections, moving.as_ref()) {
            Some(Change::Arrived { .. }) => found.push(Visit {
                arrived: f.t,
                stayed: None,
            }),
            Some(Change::Left { stayed }) => {
                if let Some(open) = found.last_mut() {
                    open.stayed = Some(stayed.as_secs_f64());
                }
            }
            None => {}
        }
    }
    Ok(found)
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

/// What the source delivers at `rotate`. A Pi camera's ISP scales for free, so a quarter turn
/// captures the picture already as small as it ends up turned (480x360 for 640x480), and
/// videoscale only adds the side borders: a Pi 5 spent 20 % of a core resampling it, 3 % now.
pub(super) fn capture_size(isp: bool, config: &Config, rotate: u32) -> (u32, u32) {
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
pub(super) struct I420Layout {
    luma_stride: u32,
    chroma_stride: u32,
    height: u32,
    pub(super) frame_size: u32,
}

#[cfg(unix)]
impl I420Layout {
    pub(super) fn rpicam(width: u32, height: u32) -> Self {
        let luma_stride = width.next_multiple_of(128);
        let chroma_stride = luma_stride / 2;
        Self {
            luma_stride,
            chroma_stride,
            height,
            frame_size: luma_stride * height + 2 * chroma_stride * height.div_ceil(2),
        }
    }

    pub(super) fn strides(&self) -> String {
        format!(
            "<{}, {}, {}>",
            self.luma_stride, self.chroma_stride, self.chroma_stride
        )
    }

    pub(super) fn offsets(&self) -> String {
        let chroma = self.luma_stride * self.height;
        let second = chroma + self.chroma_stride * self.height.div_ceil(2);
        format!("<0, {chroma}, {second}>")
    }
}

/// A source whose camera pipeline scales to any size it is asked for: a Pi camera's ISP.
pub(super) fn isp_scales(plan: &Plan, found: &Found) -> bool {
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

pub(super) fn flip_method(rotate: u32) -> anyhow::Result<&'static str> {
    Ok(match rotate {
        0 => "none",
        90 => "clockwise",
        180 => "rotate-180",
        270 => "counterclockwise",
        other => bail!("rotate must be 0, 90, 180 or 270 (got {other})"),
    })
}

/// The sample's bytes when they are exactly one I420 frame of the configured size.
pub(super) fn exact_frame(data: &[u8], width: u32, height: u32) -> anyhow::Result<&[u8]> {
    let expected = frame::size(width, height);
    // A longer buffer used to be cut and a shorter one skipped, without a word; both mean caps and bytes disagree.
    anyhow::ensure!(
        data.len() == expected,
        "the camera sent a frame of {} bytes where {width}x{height} I420 needs {expected}; \
         row padding (a stride wider than the width) is the likely cause",
        data.len()
    );
    Ok(data)
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::mpsc;

    use gstreamer as gst;

    use super::*;
    use crate::motion::{rate_period, Pacer};

    #[test]
    fn a_frame_of_exactly_its_size_passes_and_a_padded_or_short_one_names_both_sizes() {
        let (w, h) = (64, 48);
        let size = frame::size(w, h);
        assert_eq!(exact_frame(&vec![0; size], w, h).unwrap().len(), size);
        for wrong in [size + 4, size - 4] {
            let err = exact_frame(&vec![0; wrong], w, h).unwrap_err().to_string();
            assert!(err.contains(&format!("{wrong} bytes")), "{err}");
            assert!(err.contains(&format!("64x48 I420 needs {size}")), "{err}");
            assert!(err.contains("row padding"), "{err}");
        }
    }

    #[test]
    fn failures_back_off_from_1_s_doubling_to_32() {
        let mut backoff = Backoff::default();
        let pauses: Vec<u64> = (0..7)
            .map(|_| {
                backoff
                    .after(Duration::from_secs(2), true, false)
                    .unwrap()
                    .as_secs()
            })
            .collect();
        assert_eq!(pauses, [1, 2, 4, 8, 16, 32, 32]);
    }

    #[test]
    fn a_capture_that_ran_over_a_minute_starts_the_backoff_again() {
        let mut backoff = Backoff::default();
        for _ in 0..4 {
            backoff.after(Duration::from_secs(1), true, false);
        }
        // Exactly a minute is not over it.
        assert_eq!(
            backoff.after(STABLE_AFTER, true, false),
            Some(Duration::from_secs(16))
        );
        assert_eq!(
            backoff.after(STABLE_AFTER + Duration::from_secs(1), true, false),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn a_stand_in_that_ran_its_course_probes_at_once_and_counts_no_failure() {
        let mut backoff = Backoff::default();
        let reprobe_every = Timing::default().reprobe_every;
        assert_eq!(backoff.after(reprobe_every, false, true), None);
        assert_eq!(backoff.after(reprobe_every, false, true), None);
        // A real camera's source that ended, or a stand-in that failed, waits; still from 1 s.
        assert_eq!(
            backoff.after(Duration::from_secs(5), false, false),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            backoff.after(Duration::from_secs(5), true, true),
            Some(Duration::from_secs(2))
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
            class_id: 15,
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
        use gstreamer::prelude::*;

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
    fn a_zero_width_is_left_as_it_is_rather_than_divided_by() {
        let config = Config {
            width: 0,
            height: 480,
            ..Config::default()
        };
        assert_eq!(capture_size(true, &config, 90), (0, 480));
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
            max_box_area: 0.9,
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
    pub(in crate::capture) fn middle_cat() -> Vec<Detection> {
        vec![Detection {
            x1: 16.0,
            y1: 12.0,
            x2: 48.0,
            y2: 36.0,
            score: 0.6,
            class_id: 15,
        }]
    }

    pub(in crate::capture) fn health(status: &Status) -> serde_json::Value {
        serde_json::from_str(&status.to_json()).unwrap()
    }

    #[test]
    fn boxes_count_for_two_slow_runs_and_never_for_less_than_the_floor() {
        let ms = Duration::from_millis;
        assert_eq!(box_lifetime(ms(1000), Some(300.0)), ms(2600));
        assert_eq!(box_lifetime(ms(1000), None), ms(2000));
        assert_eq!(box_lifetime(ms(2000), None), ms(4000));
        // An uncapped fast model: two runs are 60 ms, less than a frame at a low rate.
        assert_eq!(box_lifetime(ms(0), Some(30.0)), BOX_LIFETIME_FLOOR);
        assert_eq!(box_lifetime(ms(0), Some(-1.0)), BOX_LIFETIME_FLOOR);
        assert!(!fresh(None, ms(2000)), "no run, no boxes");
        assert!(fresh(Some(ms(0)), ms(2000)));
        assert!(fresh(Some(ms(2000)), ms(2000)));
        assert!(!fresh(Some(ms(2001)), ms(2000)));
    }

    #[test]
    fn a_failing_model_clears_the_boxes_and_healthz_says_so_until_a_run_succeeds() {
        gst::init().unwrap();
        let status = Arc::new(Status::new("Cam", true, 64, 48));
        status.set_frame(&gst::Buffer::from_mut_slice(vec![
            128u8;
            frame::size(64, 48)
        ]));
        let mut v = visits(&status);
        let frame = vec![128u8; frame::size(64, 48)];
        let t = Instant::now();
        v.record(t, &frame, middle_cat(), None);
        v.record(t + Duration::from_millis(500), &frame, middle_cat(), None);
        assert_eq!(v.boxes.lock().unwrap().len(), 1);

        v.model_failed("ort: out of memory".into());
        assert!(
            v.boxes.lock().unwrap().is_empty(),
            "a stale box would stay sharp"
        );
        assert!(health(&status)["detection"]["boxes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(health(&status)["ok"], true, "one failed run is no outage");
        v.model_failed("ort: out of memory".into());
        v.model_failed("ort: out of memory".into());
        let h = health(&status);
        assert_eq!(h["ok"], false);
        assert_eq!(h["problem"], "the model fails: ort: out of memory");

        status.record_model_time(Duration::from_millis(250));
        assert_eq!(health(&status)["ok"], true);
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
        assert_eq!(health(&status)["detection"]["visits"], 0);
        assert!(v.boxes.lock().unwrap().is_empty());

        v.record(at(0.5), &frame, middle_cat(), None);
        let h = health(&status);
        assert_eq!(h["detection"]["visits"], 1);
        assert_eq!(h["detection"]["in_view"], true);
        assert_eq!(
            h["detection"]["recent"][0]["boxes"],
            serde_json::json!([[0.25, 0.25, 0.75, 0.75]])
        );
        assert!(status.arrival_jpeg(None).is_some());
        assert_eq!(v.boxes.lock().unwrap().len(), 1);
        assert!(v.visit.is_some());
        assert!(dir.path().join("visits.json").is_file());

        // Behind the sofa: out of view after the linger, the visit still open.
        v.record(at(4.0), &frame, Vec::new(), None);
        let h = health(&status);
        assert_eq!(h["detection"]["in_view"], false);
        assert!(h["detection"]["recent"][0]["stayed_s"].is_null());
        assert!(v.boxes.lock().unwrap().is_empty());

        v.record(at(30.0), &frame, middle_cat(), None);
        v.record(at(30.5), &frame, middle_cat(), None);
        assert_eq!(health(&status)["detection"]["in_view"], true);
        assert_eq!(health(&status)["detection"]["visits"], 1);

        v.record(at(90.0), &frame, Vec::new(), None);
        assert!(health(&status)["detection"]["recent"][0]["stayed_s"].is_null());
        v.record(at(91.0), &frame, Vec::new(), None);
        assert_eq!(health(&status)["detection"]["recent"][0]["stayed_s"], 30);
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
        assert_eq!(health(&status)["detection"]["visits"], 0);
        assert!(status.arrival_jpeg(None).is_none());
        assert!(v.boxes.lock().unwrap().is_empty());
    }

    #[test]
    fn the_arrival_photo_is_filtered_outside_the_cat_and_only_with_the_filter_on() {
        let frame: Vec<u8> = (0..frame::size(64, 48))
            .map(|i| (i * 7 % 251) as u8)
            .collect();
        // The decoded photo's top-left 8 x 8 corner, far from the cat: brightest less darkest.
        let spread = |privacy| {
            let jpeg = arrival_jpeg(&frame, 64, 48, &middle_cat(), privacy).unwrap();
            let gray = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
                .unwrap()
                .to_luma8();
            let corner: Vec<u8> = (0..8u32)
                .flat_map(|y| (0..8u32).map(move |x| (x, y)))
                .map(|(x, y)| gray.get_pixel(x, y)[0])
                .collect();
            corner.iter().max().unwrap() - corner.iter().min().unwrap()
        };
        let (open, filtered) = (spread(Privacy::Off), spread(Privacy::Pixelate));
        assert!(
            open > 100 && filtered < 30,
            "open {open}, filtered {filtered}"
        );
    }

    /// How long [`sleep_unless`] took on a thread of its own; `None` while it still waits after 5 s.
    fn timed_pause(stopping: bool, total: Duration) -> Option<Duration> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let started = Instant::now();
            sleep_unless(&AtomicBool::new(stopping), total);
            let _ = tx.send(started.elapsed());
        });
        rx.recv_timeout(Duration::from_secs(5)).ok()
    }

    #[test]
    fn a_pause_lasts_its_time_unless_the_service_stops() {
        let full = timed_pause(false, Duration::from_millis(250)).expect("a 250 ms pause ended");
        assert!(full >= Duration::from_millis(250), "{full:?}");
        let cut = timed_pause(true, Duration::from_secs(30)).expect("a stop cut the pause short");
        assert!(cut < Duration::from_secs(1), "{cut:?}");
    }
}
