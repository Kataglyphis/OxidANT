//! What `/healthz` reports (which camera is live, how often the model ran, when a cat was last seen)
//! and the latest annotated frame `/snapshot.jpg` encodes.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gstreamer as gst;
use serde::{Deserialize, Serialize};

use crate::config::Privacy;
use crate::presence::{Change, Zone};
use crate::stats::Stats;

/// The cat's visits, kept across restarts in the state directory.
const VISITS_FILE: &str = "visits.json";

/// The visits file's name before ObservANT (2026-10-14), read when [`VISITS_FILE`] is not there yet.
const LEGACY_VISITS_FILE: &str = "cat.json";

/// The frame of the last arrival as an earlier version kept it; taken over as that arrival's.
const LEGACY_ARRIVAL_FILE: &str = "arrival.jpg";
/// What the settings page set; each value in it wins over `catcam.toml`, which the service cannot write.
const SETTINGS_FILE: &str = "settings.json";

/// The angles `videoflip` turns a picture by, clockwise.
pub const ROTATIONS: [u32; 4] = [0, 90, 180, 270];

/// The detection score the page may set: lower counts noise as cats, higher misses them.
pub const SCORES: std::ops::RangeInclusive<f32> = 0.05..=0.95;

/// The longest name the page may give the camera.
const NAME_MAX: usize = 64;

/// The most spots the page may mark "not a cat".
const IGNORE_MAX: usize = 16;

/// The build: the package's version, stamped by the package script (`OBSERVANT_VERSION`, before
/// ObservANT `CATCAM_VERSION`), else the crate's.
pub const VERSION: &str = match option_env!("OBSERVANT_VERSION") {
    Some(version) => version,
    None => match option_env!("CATCAM_VERSION") {
        Some(version) => version,
        None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
    },
};

/// The visits file in `dir`, or the one from before ObservANT when only that exists.
fn read_visits(dir: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(dir.join(VISITS_FILE)) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::read_to_string(dir.join(LEGACY_VISITS_FILE))
        }
        other => other,
    }
}

/// One browser tab connected to the stream; dropping it frees its place.
pub struct Viewer(Arc<AtomicUsize>);

impl Drop for Viewer {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The settings the page can change while the service runs; a value left out stays as it is.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Spots marked "not a cat", as fractions of the picture; a detection over one is dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<Zone>>,
    /// The COCO classes that count, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classes: Option<Vec<String>>,
}

/// `classes` is one to all 80 COCO names, each once.
pub fn check_classes(classes: &[String]) -> anyhow::Result<()> {
    if classes.is_empty() {
        anyhow::bail!("classes must name at least one COCO class");
    }
    if let Some(unknown) = classes.iter().find(|c| crate::coco::id(c).is_none()) {
        anyhow::bail!(
            "{unknown:?} is no COCO class; known: {}",
            crate::coco::NAMES.join(", ")
        );
    }
    let mut seen = std::collections::HashSet::new();
    if let Some(twice) = classes.iter().find(|c| !seen.insert(c.as_str())) {
        anyhow::bail!("classes names {twice:?} twice");
    }
    Ok(())
}

impl Settings {
    /// Every value it carries, checked before any is applied, so a bad one changes nothing.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(degrees) = self.rotate {
            if !ROTATIONS.contains(&degrees) {
                anyhow::bail!("rotate must be 0, 90, 180 or 270 (got {degrees})");
            }
        }
        if let Some(score) = self.score {
            if !SCORES.contains(&score) {
                anyhow::bail!("score must lie in 0.05..=0.95 (got {score})");
            }
        }
        if let Some(name) = &self.name {
            let name = name.trim();
            if name.is_empty()
                || name.chars().count() > NAME_MAX
                || name.chars().any(char::is_control)
            {
                anyhow::bail!("name must be 1 to {NAME_MAX} printable characters");
            }
        }
        if let Some(classes) = &self.classes {
            check_classes(classes)?;
        }
        if let Some(zones) = &self.ignore {
            if zones.len() > IGNORE_MAX {
                anyhow::bail!("at most {IGNORE_MAX} ignored spots (got {})", zones.len());
            }
            let fraction = |v: f32| (0.0..=1.0).contains(&v);
            if let Some(bad) = zones
                .iter()
                .find(|z| !z.iter().all(|&v| fraction(v)) || z[0] >= z[2] || z[1] >= z[3])
            {
                anyhow::bail!("an ignored spot is x1, y1, x2, y2 as fractions 0 to 1, x1 < x2 and y1 < y2 (got {bad:?})");
            }
        }
        Ok(())
    }
}
/// How many arrivals' clips and pictures stay addressable: a false visit must not replace the ones a
/// notification links.
const CLIPS_KEPT: usize = 5;
/// How far a requested arrival time may be from a kept clip's or picture's and still name it.
const CLIP_TIME_SLACK_S: u64 = 2;

/// With no frame for this long `/healthz` says the camera is not ok; the capture restarts after 10 s.
const STALE_FRAME_S: u64 = 20;

/// Model runs failing in a row before `/healthz` says so; one odd frame is no outage.
const MODEL_FAILURES_REPORTED: u32 = 3;

/// A kind of file kept per arrival in the state directory, `<prefix><arrival time><suffix>`.
#[derive(Clone, Copy)]
struct Kept {
    prefix: &'static str,
    suffix: &'static str,
}

const CLIPS: Kept = Kept {
    prefix: "clip-",
    suffix: ".mp4",
};
const PHOTOS: Kept = Kept {
    prefix: "arrival-",
    suffix: ".jpg",
};

impl Kept {
    fn file(self, arrived: u64) -> String {
        format!("{}{arrived}{}", self.prefix, self.suffix)
    }
}

/// How many visits `/healthz` lists and the state directory keeps.
const RECENT_VISITS: usize = 20;

/// One visit: when the cat was confirmed, how long it stayed once it has left, and where its boxes
/// were, which the Camera page offers to mark "not a cat" after the fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Visit {
    arrived: u64,
    stayed_s: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_rounded"
    )]
    boxes: Vec<Zone>,
    /// The COCO class it arrived as; visits from before classes keep none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    class: Option<String>,
}

/// What a restart should not forget about the cat.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Remembered {
    last_seen: Option<u64>,
    visits: u64,
    last_score: Option<f32>,
    /// Newest first; a cat.json from before the log has none.
    #[serde(default)]
    recent: Vec<Visit>,
}

#[derive(Default)]
struct Detected {
    in_view: bool,
    last_seen: Option<u64>,
    visits: u64,
    best_score: f32,
    runs: u64,
    /// The last run saw a cat, confirmed or not.
    hit: bool,
    /// Newest first, at most [`RECENT_VISITS`].
    recent: VecDeque<Visit>,
    /// The arrival the current clip shows.
    clip: Option<u64>,
    /// The boxes drawn into the picture now, as fractions of it.
    boxes: Vec<Zone>,
}

pub struct Status {
    name: Mutex<String>,
    inference: bool,
    privacy: Privacy,
    /// The sampler's latest look, for `/healthz`'s `stats`.
    stats: Mutex<Stats>,
    /// The model's time per run in ms, smoothed; `None` before the first run.
    model_ms: Mutex<Option<f64>>,
    /// Model runs failed in a row, and the last one's error.
    model_failures: AtomicU32,
    model_error: Mutex<Option<String>>,
    /// The COCO classes that count, and whether every class does.
    classes: Mutex<Vec<String>>,
    all_classes: bool,
    width: u32,
    height: u32,
    fps: u32,
    camera: Mutex<String>,
    /// A test pattern streams because no camera was found.
    stand_in: AtomicBool,
    detected: Mutex<Detected>,
    frame: Mutex<Option<gst::Buffer>>,
    /// Counts the frames pushed, so a snapshot is encoded once per frame however often it is asked for.
    frame_seq: AtomicU64,
    /// The last snapshot and the frame it shows; its lock lets one encode run at a time.
    snapshot: Mutex<Option<(u64, bytes::Bytes)>>,
    /// When the capture last delivered a frame, and the model last ran.
    last_frame: Mutex<Option<Instant>>,
    last_run: Mutex<Option<Instant>>,
    /// The frames the last [`CLIPS_KEPT`] arrivals were confirmed on, as JPEG, newest first.
    photos: Mutex<VecDeque<(u64, bytes::Bytes)>>,
    /// The last [`CLIPS_KEPT`] arrivals as MP4, newest first, by arrival time.
    clips: Mutex<VecDeque<(u64, bytes::Bytes)>>,
    /// Where [`VISITS_FILE`], the photos and the clips live; `None` forgets everything at exit.
    state_dir: Option<PathBuf>,
    /// Clockwise degrees the capture turns the picture by; the capture thread follows a change.
    rotate: AtomicU32,
    /// The detection score threshold as `f32` bits; the inference thread reads it each run.
    score: AtomicU32,
    /// What the page set so far, the content of [`SETTINGS_FILE`].
    page: Mutex<Settings>,
    /// Spots marked "not a cat"; the inference thread reads them each run.
    ignore: Mutex<Vec<Zone>>,
    /// Browser tabs connected to the stream now.
    viewers: Arc<AtomicUsize>,
    /// The other ObservANT cameras' origins, which the page lists.
    peers: Vec<String>,
}

impl Status {
    pub fn new(name: &str, inference: bool, width: u32, height: u32) -> Self {
        Self {
            name: Mutex::new(name.to_owned()),
            inference,
            privacy: Privacy::Off,
            stats: Mutex::new(Stats::default()),
            model_ms: Mutex::new(None),
            model_failures: AtomicU32::new(0),
            model_error: Mutex::new(None),
            classes: Mutex::new(vec!["cat".into()]),
            all_classes: false,
            width,
            height,
            fps: 0,
            camera: Mutex::new("starting".to_owned()),
            stand_in: AtomicBool::new(false),
            detected: Mutex::new(Detected::default()),
            frame: Mutex::new(None),
            frame_seq: AtomicU64::new(0),
            snapshot: Mutex::new(None),
            last_frame: Mutex::new(None),
            last_run: Mutex::new(None),
            photos: Mutex::new(VecDeque::new()),
            clips: Mutex::new(VecDeque::new()),
            state_dir: None,
            rotate: AtomicU32::new(0),
            score: AtomicU32::new(0.25f32.to_bits()),
            page: Mutex::new(Settings::default()),
            ignore: Mutex::new(Vec::new()),
            viewers: Arc::new(AtomicUsize::new(0)),
            peers: Vec::new(),
        }
    }

    /// The other ObservANT cameras, which `/healthz` names for the page's overview.
    pub fn peered(mut self, peers: Vec<String>) -> Self {
        self.peers = peers;
        self
    }

    /// A place for one more browser tab on the stream, unless `max` (0: no limit) are connected.
    pub fn viewer(&self, max: usize) -> Option<Viewer> {
        let before = self.viewers.fetch_add(1, Ordering::AcqRel);
        if max != 0 && before >= max {
            self.viewers.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Viewer(self.viewers.clone()))
    }

    /// The spots marked "not a cat".
    pub fn ignored(&self) -> Vec<Zone> {
        self.ignore.lock().map(|z| z.clone()).unwrap_or_default()
    }

    /// The boxes drawn into the picture now, for `/healthz` and the page's "not a cat".
    pub fn set_boxes(&self, boxes: Vec<Zone>) {
        if let Ok(mut cat) = self.detected.lock() {
            cat.boxes = boxes;
        }
    }

    /// The rotation `catcam.toml` asks for; one the page saved overrides it in [`Self::remember_in`].
    pub fn rotated(self, degrees: u32) -> Self {
        self.rotate.store(degrees, Ordering::Relaxed);
        self
    }

    /// The detection score `catcam.toml` asks for; one the page saved overrides it.
    pub fn scored(self, score: f32) -> Self {
        self.score.store(score.to_bits(), Ordering::Relaxed);
        self
    }

    /// Frames pushed and model runs so far, for the stats sampler.
    pub fn counters(&self) -> (u64, u64) {
        let runs = self.detected.lock().map(|c| c.runs).unwrap_or(0);
        (self.frame_seq.load(Ordering::Relaxed), runs)
    }

    pub fn set_stats(&self, stats: Stats) {
        if let Ok(mut current) = self.stats.lock() {
            *current = stats;
        }
    }

    /// One model run's time; the stats show a mean that weighs the last runs most.
    pub fn record_model_time(&self, took: std::time::Duration) {
        self.model_failures.store(0, Ordering::Relaxed);
        let ms = took.as_secs_f64() * 1000.0;
        if let Ok(mut mean) = self.model_ms.lock() {
            *mean = Some(mean.map_or(ms, |m| m * 0.8 + ms * 0.2));
        }
    }

    pub fn model_ms(&self) -> Option<f64> {
        self.model_ms.lock().ok().and_then(|m| *m)
    }

    /// One failed model run; returns how many failed in a row, so the caller can keep its log short.
    pub fn record_model_failure(&self, error: String) -> u32 {
        if let Ok(mut last) = self.model_error.lock() {
            *last = Some(error);
        }
        self.model_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The model's last error while it keeps failing.
    fn model_problem(&self) -> Option<String> {
        if self.model_failures.load(Ordering::Relaxed) < MODEL_FAILURES_REPORTED {
            return None;
        }
        let error = self.model_error.lock().ok().and_then(|e| e.clone());
        Some(format!("the model fails: {}", error.unwrap_or_default()))
    }

    /// The COCO classes that count; `all` keeps every class.
    pub fn watching(mut self, classes: Vec<String>, all: bool) -> Self {
        self.classes = Mutex::new(classes);
        self.all_classes = all;
        self
    }

    pub fn classes(&self) -> Vec<String> {
        self.classes.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// The class ids the model's results keep; `None` keeps every class.
    pub fn wanted_classes(&self) -> Option<Vec<i64>> {
        (!self.all_classes).then(|| {
            self.classes()
                .iter()
                .filter_map(|c| crate::coco::id(c))
                .collect()
        })
    }

    /// The privacy filter in effect, for `/healthz` and the page.
    pub fn private(mut self, privacy: Privacy) -> Self {
        self.privacy = privacy;
        self
    }

    /// The camera's frame rate, for `/healthz`.
    pub fn at_fps(mut self, fps: u32) -> Self {
        self.fps = fps;
        self
    }

    /// Clockwise degrees the picture is turned by.
    pub fn rotation(&self) -> u32 {
        self.rotate.load(Ordering::Relaxed)
    }

    /// The score a detection needs to count.
    pub fn score(&self) -> f32 {
        f32::from_bits(self.score.load(Ordering::Relaxed))
    }

    /// The score to two places, so JSON says 0.45 rather than the `f32`'s 0.44999998807907104.
    fn score_rounded(&self) -> f64 {
        (f64::from(self.score()) * 100.0).round() / 100.0
    }

    /// The camera's name, as viewers and webhook events see it.
    pub fn name(&self) -> String {
        self.name.lock().map(|n| n.clone()).unwrap_or_default()
    }

    /// Turns the picture as the settings page does; the tests' shorthand for [`Self::apply`].
    #[cfg(test)]
    pub fn set_rotation(&self, degrees: u32) -> anyhow::Result<()> {
        self.apply(Settings {
            rotate: Some(degrees),
            ..Settings::default()
        })
    }

    /// The values the page can change, as `GET /settings` answers them.
    pub fn settings_json(&self) -> String {
        serde_json::json!({
            "name": self.name(),
            "rotate": self.rotation(),
            "score": self.score_rounded(),
            "ignore": rounded(&self.ignored()),
            "classes": self.classes(),
            "privacy": self.privacy,
        })
        .to_string()
    }

    /// Applies what `update` carries, all or nothing, and keeps it in the state directory.
    pub fn apply(&self, mut update: Settings) -> anyhow::Result<()> {
        update.validate()?;
        if self.privacy == Privacy::Pixelate
            && update
                .classes
                .as_ref()
                .is_some_and(|c| *c != self.classes())
        {
            anyhow::bail!(
                "the privacy filter is on, so only observant.toml sets the classes: a box of another class would show it unblurred"
            );
        }
        let turned = update.rotate.is_some_and(|r| r != self.rotation());
        if turned && update.ignore.is_none() && !self.ignored().is_empty() {
            log::info!("the picture turned, so the spots marked not a cat no longer fit: cleared");
            update.ignore = Some(Vec::new());
        }
        self.take(&update);
        let Ok(mut page) = self.page.lock() else {
            return Ok(());
        };
        if update.rotate.is_some() {
            page.rotate = update.rotate;
        }
        if update.score.is_some() {
            page.score = update.score;
        }
        if let Some(name) = &update.name {
            page.name = Some(name.trim().to_owned());
        }
        if update.ignore.is_some() {
            page.ignore.clone_from(&update.ignore);
        }
        if update.classes.is_some() {
            page.classes.clone_from(&update.classes);
        }
        if let Some(dir) = &self.state_dir {
            write_atomic(&dir.join(SETTINGS_FILE), &serde_json::to_vec(&*page)?)?;
        }
        log::info!(
            "settings from the page: {}",
            serde_json::to_string(&update)?
        );
        Ok(())
    }

    /// Sets the values `settings` carries; [`Settings::validate`] has passed.
    fn take(&self, settings: &Settings) {
        if let Some(degrees) = settings.rotate {
            self.rotate.store(degrees, Ordering::Relaxed);
        }
        if let Some(score) = settings.score {
            self.score.store(score.to_bits(), Ordering::Relaxed);
        }
        if let (Some(name), Ok(mut current)) = (&settings.name, self.name.lock()) {
            *current = name.trim().to_owned();
        }
        if let (Some(zones), Ok(mut current)) = (&settings.ignore, self.ignore.lock()) {
            current.clone_from(zones);
        }
        if let (Some(classes), Ok(mut current)) = (&settings.classes, self.classes.lock()) {
            current.clone_from(classes);
        }
    }

    /// Restores the cat's visits and last arrival frame from `dir` and keeps them there from now on.
    pub fn remember_in(mut self, dir: PathBuf) -> Self {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log::warn!("{}: {err}", dir.display());
        }
        match read_visits(&dir) {
            Ok(text) => match serde_json::from_str::<Remembered>(&text) {
                Ok(r) => {
                    if let Ok(cat) = self.detected.get_mut() {
                        cat.last_seen = r.last_seen;
                        cat.visits = r.visits;
                        cat.best_score = r.last_score.unwrap_or_default();
                        cat.recent = r.recent.into_iter().take(RECENT_VISITS).collect();
                        // The service stopped with the cat in view: it stayed at least until last_seen.
                        if let (Some(open), Some(seen)) = (cat.recent.front_mut(), cat.last_seen) {
                            if open.stayed_s.is_none() {
                                open.stayed_s = Some(seen.saturating_sub(open.arrived));
                            }
                        }
                    }
                }
                Err(err) => log::warn!(
                    "{}: {err}; starting afresh",
                    dir.join(VISITS_FILE).display()
                ),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!("{}: {err}", dir.join(VISITS_FILE).display()),
        }
        adopt_legacy_photo(&dir, self.latest_arrival().unwrap_or(0));
        if let Ok(photos) = self.photos.get_mut() {
            *photos = read_kept(&dir, PHOTOS);
        }
        let restored = read_kept(&dir, CLIPS);
        if let (Ok(cat), Some((newest, _))) = (self.detected.get_mut(), restored.front()) {
            cat.clip = Some(*newest);
        }
        if let Ok(clips) = self.clips.get_mut() {
            *clips = restored;
        }
        if let Some(mut page) = std::fs::read_to_string(dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .filter(|page| page.validate().is_ok())
        {
            if self.privacy == Privacy::Pixelate && page.classes.take().is_some() {
                log::warn!("the privacy filter is on: the classes the page once set give way to observant.toml's");
            }
            log::info!(
                "settings the page set: {}",
                serde_json::to_string(&page).unwrap_or_default()
            );
            self.take(&page);
            if let Ok(saved) = self.page.get_mut() {
                *saved = page;
            }
        }
        self.state_dir = Some(dir);
        self
    }

    /// Writes what a restart should keep; called on every arrival and departure.
    pub fn persist(&self) {
        let Some(dir) = &self.state_dir else {
            return;
        };
        let Ok(remembered) = self.detected.lock().map(|cat| Remembered {
            last_seen: cat.last_seen,
            visits: cat.visits,
            last_score: cat.last_seen.map(|_| cat.best_score),
            recent: cat.recent.iter().cloned().collect(),
        }) else {
            return;
        };
        let json = serde_json::to_vec(&remembered).unwrap_or_default();
        if let Err(err) = write_atomic(&dir.join(VISITS_FILE), &json) {
            log::warn!("{}: {err}", dir.join(VISITS_FILE).display());
        }
    }

    /// What the capture streams; `stand_in` when it is a test pattern for a camera that was not found.
    pub fn set_camera(&self, camera: &str, stand_in: bool) {
        if let Ok(mut current) = self.camera.lock() {
            camera.clone_into(&mut current);
        }
        self.stand_in.store(stand_in, Ordering::Relaxed);
    }

    /// Whether the camera delivers, and if not, why: `/healthz`'s `ok` and `problem`.
    fn health(&self) -> (bool, Option<String>, Option<u64>, Option<u64>) {
        let age = |at: &Mutex<Option<Instant>>| {
            at.lock()
                .ok()
                .and_then(|t| t.map(|t| t.elapsed().as_secs()))
        };
        let (frame_age, model_age) = (age(&self.last_frame), age(&self.last_run));
        let problem = if self.stand_in.load(Ordering::Relaxed) {
            Some("no camera found, streaming a test pattern".to_owned())
        } else {
            match frame_age {
                None => Some("waiting for the first frame".to_owned()),
                Some(s) if s > STALE_FRAME_S => Some(format!("no frame for {s} s")),
                Some(_) => self.model_problem(),
            }
        };
        (problem.is_none(), problem, frame_age, model_age)
    }

    /// Records one inference run's view of the cat, after presence smoothing, and the visit it starts or ends.
    pub fn set_detection(&self, in_view: bool, best_score: Option<f32>, change: Option<&Change>) {
        if let Ok(mut last) = self.last_run.lock() {
            *last = Some(Instant::now());
        }
        if let Ok(mut cat) = self.detected.lock() {
            cat.runs += 1;
            cat.hit = best_score.is_some();
            cat.in_view = in_view;
            if in_view {
                cat.last_seen = Some(unix_now());
                cat.best_score = best_score.unwrap_or(cat.best_score);
            }
            match change {
                Some(Change::Arrived { class, .. }) => {
                    cat.visits += 1;
                    let boxes = cat.boxes.clone();
                    cat.recent.push_front(Visit {
                        arrived: unix_now(),
                        stayed_s: None,
                        boxes,
                        class: crate::coco::name(*class).map(str::to_owned),
                    });
                    cat.recent.truncate(RECENT_VISITS);
                }
                Some(Change::Left { stayed }) => {
                    if let Some(open) = cat.recent.front_mut() {
                        open.stayed_s.get_or_insert(stayed.as_secs());
                    }
                }
                None => {}
            }
        }
    }

    /// A cat is in view, or the last run saw one that is not confirmed yet.
    pub fn anything_seen(&self) -> bool {
        self.detected.lock().is_ok_and(|cat| cat.in_view || cat.hit)
    }

    /// Keeps a reference to the frame just pushed; a buffer is refcounted, so this copies nothing.
    pub fn set_frame(&self, frame: &gst::Buffer) {
        if let Ok(mut current) = self.frame.lock() {
            *current = Some(frame.clone());
            self.frame_seq.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut last) = self.last_frame.lock() {
            *last = Some(Instant::now());
        }
    }

    /// Keeps the frame the arrival at `arrived` was confirmed on with the last few, on disk too.
    pub fn set_arrival(&self, jpeg: Vec<u8>, arrived: u64) {
        self.keep(PHOTOS, &self.photos, arrived, jpeg);
    }

    /// The newest arrival's frame, or with `at` that arrival's own; `None` once it is dropped.
    pub fn arrival_jpeg(&self, at: Option<u64>) -> Option<bytes::Bytes> {
        pick(&self.photos, at)
    }

    /// Keeps the clip of the arrival at `arrived` with the last few, on disk too, and names it in `/healthz`.
    pub fn set_clip(&self, mp4: Vec<u8>, arrived: u64) {
        self.keep(CLIPS, &self.clips, arrived, mp4);
        if let Ok(mut cat) = self.detected.lock() {
            cat.clip = Some(arrived);
        }
    }

    /// The newest clip, or with `at` the one of the arrival at that time; `None` once it is dropped.
    pub fn clip_mp4(&self, at: Option<u64>) -> Option<bytes::Bytes> {
        pick(&self.clips, at)
    }

    /// Puts `bytes` first in `store` and on disk as `kind`, dropping the oldest beyond [`CLIPS_KEPT`].
    fn keep(
        &self,
        kind: Kept,
        store: &Mutex<VecDeque<(u64, bytes::Bytes)>>,
        arrived: u64,
        bytes: Vec<u8>,
    ) {
        if let Some(dir) = &self.state_dir {
            let path = dir.join(kind.file(arrived));
            if let Err(err) = write_atomic(&path, &bytes) {
                log::warn!("{}: {err}", path.display());
            }
        }
        if let Ok(mut kept) = store.lock() {
            kept.retain(|(t, _)| *t != arrived);
            kept.push_front((arrived, bytes.into()));
            while kept.len() > CLIPS_KEPT {
                if let (Some((old, _)), Some(dir)) = (kept.pop_back(), &self.state_dir) {
                    let _ = std::fs::remove_file(dir.join(kind.file(old)));
                }
            }
        }
    }

    /// How long the visit that began at `arrived` lasted; `None` while it goes on or once it left the log.
    pub fn visit_stay(&self, arrived: u64) -> Option<u64> {
        self.detected.lock().ok().and_then(|cat| {
            cat.recent
                .iter()
                .find(|v| v.arrived == arrived)
                .and_then(|v| v.stayed_s)
        })
    }

    /// When the newest visit began, as its log entry says.
    pub fn latest_arrival(&self) -> Option<u64> {
        self.detected
            .lock()
            .ok()
            .and_then(|cat| cat.recent.front().map(|v| v.arrived))
    }

    /// The latest annotated frame as JPEG, or `None` before the first frame; encoded once per frame,
    /// one encode at a time, so a client asking in a loop cannot starve detection.
    pub fn snapshot_jpeg(&self) -> anyhow::Result<Option<bytes::Bytes>> {
        let mut cached = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("snapshot lock"))?;
        let (seq, buffer) = match self.frame.lock() {
            Ok(frame) => (self.frame_seq.load(Ordering::Relaxed), frame.clone()),
            Err(_) => return Ok(None),
        };
        let Some(buffer) = buffer else {
            return Ok(None);
        };
        if let Some((at, jpeg)) = cached.as_ref() {
            if *at == seq {
                return Ok(Some(jpeg.clone()));
            }
        }
        let map = buffer
            .map_readable()
            .map_err(|_| anyhow::anyhow!("map frame"))?;
        let i420 = map
            .as_slice()
            .get(..crate::frame::size(self.width, self.height))
            .ok_or_else(|| anyhow::anyhow!("frame smaller than its caps"))?;
        let rgba = crate::frame::to_rgba(i420, self.width, self.height);
        let jpeg = bytes::Bytes::from(encode_jpeg(&rgba, self.width, self.height)?);
        *cached = Some((seq, jpeg.clone()));
        Ok(Some(jpeg))
    }

    pub fn to_json(&self) -> String {
        let camera = self.camera.lock().map(|c| c.clone()).unwrap_or_default();
        let cat = self.detected.lock();
        let (in_view, last_seen, visits, best, runs, clip) = cat
            .as_deref()
            .map(|c| {
                (
                    c.in_view,
                    c.last_seen,
                    c.visits,
                    c.best_score,
                    c.runs,
                    c.clip,
                )
            })
            .unwrap_or_default();
        let boxes = cat
            .as_deref()
            .map(|c| rounded(&c.boxes))
            .unwrap_or_default();
        // The arrivals whose clips /arrival.mp4?t= still serves, newest first.
        let clips: Vec<u64> = self
            .clips
            .lock()
            .map(|c| c.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default();
        let recent: Vec<Visit> = cat
            .as_deref()
            .map(|c| c.recent.iter().cloned().collect())
            .unwrap_or_default();
        let (ok, problem, frame_age, model_age) = self.health();
        serde_json::json!({
            "ok": ok,
            "problem": problem,
            "version": VERSION,
            "viewers": self.viewers.load(Ordering::Acquire),
            "peers": self.peers,
            "frame_age_s": frame_age,
            "model_age_s": model_age,
            "name": self.name(),
            "rotate": self.rotation(),
            "score": self.score_rounded(),
            "frame": {"width": self.width, "height": self.height, "fps": self.fps},
            "camera": camera,
            "inference": self.inference,
            "privacy": self.privacy,
            "classes": self.classes(),
            "all_classes": self.all_classes,
            "model_runs": runs,
            "stats": self.stats.lock().map(|s| s.clone()).unwrap_or_default(),
            "detection": {
                "in_view": in_view,
                "last_seen": last_seen,
                "visits": visits,
                "last_score": (last_seen.is_some()).then_some(best),
                "recent": recent,
                "clip": clip,
                "clips": clips,
                "boxes": boxes,
            },
        })
        .to_string()
    }
}

/// `zones` to three places, so JSON says 0.25 rather than the `f32`'s 0.25000000372529.
fn rounded(zones: &[Zone]) -> Vec<[f64; 4]> {
    zones
        .iter()
        .map(|z| z.map(|v| (f64::from(v) * 1000.0).round() / 1000.0))
        .collect()
}

fn serialize_rounded<S: serde::Serializer>(
    zones: &[Zone],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    rounded(zones).serialize(serializer)
}

/// The newest of `store`, or with `at` the one kept for the arrival at that time.
fn pick(store: &Mutex<VecDeque<(u64, bytes::Bytes)>>, at: Option<u64>) -> Option<bytes::Bytes> {
    let kept = store.lock().ok()?;
    match at {
        None => kept.front().map(|(_, bytes)| bytes.clone()),
        Some(at) => kept
            .iter()
            .filter(|(t, _)| t.abs_diff(at) <= CLIP_TIME_SLACK_S)
            .min_by_key(|(t, _)| t.abs_diff(at))
            .map(|(_, bytes)| bytes.clone()),
    }
}

/// An earlier version's single `arrival.jpg` becomes the photo of the arrival at `arrived`.
fn adopt_legacy_photo(dir: &Path, arrived: u64) {
    let legacy = dir.join(LEGACY_ARRIVAL_FILE);
    if legacy.is_file() {
        let adopted = dir.join(PHOTOS.file(arrived));
        if let Err(err) = std::fs::rename(&legacy, &adopted) {
            log::warn!("{}: {err}", legacy.display());
        }
    }
}

/// The files of `kind` a previous run left in `dir`, newest first, at most [`CLIPS_KEPT`].
fn read_kept(dir: &Path, kind: Kept) -> VecDeque<(u64, bytes::Bytes)> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let time = name
                .strip_prefix(kind.prefix)?
                .strip_suffix(kind.suffix)?
                .parse()
                .ok()?;
            Some((time, entry.path()))
        })
        .collect();
    found.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    found
        .into_iter()
        .take(CLIPS_KEPT)
        .filter_map(|(time, path)| std::fs::read(path).ok().map(|bytes| (time, bytes.into())))
        .collect()
}

/// A rename after the write, so a crash never leaves half a file for the next start.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// RGBA to JPEG; the alpha channel is dropped, as JPEG has none.
pub fn encode_jpeg(rgba: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let pixels = (width as usize) * (height as usize);
    anyhow::ensure!(
        rgba.len() >= pixels * 4,
        "frame smaller than {width}x{height}"
    );
    let (rgba_pixels, _) = rgba[..pixels * 4].as_chunks::<4>();
    let rgb: Vec<u8> = rgba_pixels
        .iter()
        .flat_map(|&[r, g, b, _]| [r, g, b])
        .collect();
    let mut out = Vec::with_capacity(pixels / 4);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode(
        &rgb,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn arrived() -> Change {
        Change::Arrived {
            count: 2,
            best: 0.8,
            class: 15,
        }
    }

    fn left(seconds: u64) -> Change {
        Change::Left {
            stayed: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn reports_the_camera_last_set() {
        let status = Status::new("Detected \"Cam\"", true, 4, 4);
        status.set_camera("v4l2src /dev/video0 (C920)", false);
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["camera"], "v4l2src /dev/video0 (C920)");
        assert_eq!(json["name"], "Detected \"Cam\"");
        assert_eq!(json["inference"], true);
        assert_eq!(json["detection"]["in_view"], false);
        assert!(json["detection"]["last_seen"].is_null());
    }

    #[test]
    fn a_visit_is_counted_once_and_last_seen_stays_after_it_ends() {
        let status = Status::new("Cam", true, 4, 4);
        status.set_detection(true, Some(0.8), Some(&arrived()));
        status.set_detection(true, Some(0.6), None);
        status.set_detection(false, None, None);
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["in_view"], false);
        assert_eq!(json["detection"]["visits"], 1);
        assert_eq!(json["model_runs"], 3);
        assert!(!status.anything_seen());
        status.set_detection(false, Some(0.3), None);
        assert!(status.anything_seen());
        assert!(json["detection"]["last_seen"].as_u64().unwrap() > 0);
        assert!((json["detection"]["last_score"].as_f64().unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_restart_remembers_the_visits_and_the_arrival_frame() {
        let dir = tempfile::tempdir().unwrap();
        let before = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        before.set_detection(true, Some(0.7), Some(&arrived()));
        let arrived = before.latest_arrival().unwrap();
        before.set_arrival(vec![0xFF, 0xD8, 0xFF, 0xD9], arrived);
        before.set_detection(false, None, Some(&left(42)));
        before.persist();

        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["detection"]["visits"], 1);
        assert_eq!(json["detection"]["in_view"], false);
        assert!(json["detection"]["last_seen"].as_u64().unwrap() > 0);
        assert!((json["detection"]["last_score"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(
            after.arrival_jpeg(None).unwrap(),
            [0xFF, 0xD8, 0xFF, 0xD9][..]
        );
        assert_eq!(
            after.arrival_jpeg(Some(arrived)).unwrap(),
            [0xFF, 0xD8, 0xFF, 0xD9][..]
        );
        assert_eq!(json["detection"]["recent"][0]["stayed_s"], 42);
        assert!(!dir.path().join("cat.partial").exists());
    }

    #[test]
    fn a_restart_keeps_the_clip_and_the_arrival_it_shows() {
        let dir = tempfile::tempdir().unwrap();
        let before = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert!(before.clip_mp4(None).is_none());
        before.set_detection(true, Some(0.7), Some(&arrived()));
        // As the inference thread does after every arrival and departure.
        before.persist();
        let arrived_at = before.latest_arrival().unwrap();
        before.set_clip(b"\0\0\0\x18ftypisom".to_vec(), arrived_at);

        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["detection"]["clip"], arrived_at);
        assert_eq!(json["detection"]["recent"][0]["arrived"], arrived_at);
        assert_eq!(&after.clip_mp4(None).unwrap()[4..8], b"ftyp");
        assert!(!dir
            .path()
            .join(format!("clip-{arrived_at}.partial"))
            .exists());
    }

    #[test]
    fn healthz_lists_the_kept_clips_and_a_visit_knows_its_stay() {
        let status = Status::new("Cam", true, 4, 4);
        status.set_detection(true, Some(0.8), Some(&arrived()));
        let at = status.latest_arrival().unwrap();
        assert_eq!(status.visit_stay(at), None);
        status.set_clip(vec![1], at);
        status.set_clip(vec![2], at + 60);
        status.set_detection(false, None, Some(&left(42)));
        assert_eq!(status.visit_stay(at), Some(42));
        assert_eq!(status.visit_stay(at + 1), None);
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["clips"], serde_json::json!([at + 60, at]));
    }

    #[test]
    fn a_turn_from_the_page_outlives_a_restart_and_wins_over_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let status = Status::new("Cam", true, 4, 4)
            .rotated(180)
            .remember_in(dir.path().to_path_buf());
        assert_eq!(
            status.rotation(),
            180,
            "catcam.toml's rotate until the page sets one"
        );
        status.set_rotation(270).unwrap();
        assert!(status.set_rotation(45).is_err());
        assert_eq!(status.rotation(), 270);

        let after = Status::new("Cam", true, 4, 4)
            .rotated(180)
            .remember_in(dir.path().to_path_buf());
        assert_eq!(after.rotation(), 270);
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["rotate"], 270);
    }

    #[test]
    fn the_page_keeps_only_what_it_set_across_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let status = Status::new("From The File", true, 4, 4)
            .scored(0.25)
            .remember_in(dir.path().to_path_buf());
        status
            .apply(Settings {
                name: Some(" Futtertrog ".into()),
                ..Settings::default()
            })
            .unwrap();
        // catcam.toml's score moved meanwhile, and the page never set one: the file's counts.
        let after = Status::new("From The File", true, 4, 4)
            .scored(0.4)
            .remember_in(dir.path().to_path_buf());
        assert_eq!(after.name(), "Futtertrog");
        assert_eq!(after.score(), 0.4);
        // Two places in the JSON, not the f32's 0.4000000059604645.
        let settings: serde_json::Value = serde_json::from_str(&after.settings_json()).unwrap();
        assert_eq!(settings["score"].to_string(), "0.4");
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["name"], "Futtertrog");
    }

    #[test]
    fn a_settings_update_is_checked_whole_before_any_of_it_applies() {
        let status = Status::new("Cam", true, 4, 4).scored(0.25);
        let mixed = Settings {
            rotate: Some(90),
            score: Some(0.99),
            name: Some("New".into()),
            ignore: None,
            classes: None,
        };
        assert!(status.apply(mixed).is_err());
        assert_eq!(
            (status.rotation(), status.score(), status.name()),
            (0, 0.25, "Cam".into())
        );
        let long = Settings {
            name: Some("x".repeat(65)),
            ..Settings::default()
        };
        assert!(status.apply(long).is_err());
    }

    #[test]
    fn each_of_the_last_five_arrivals_keeps_its_own_clip() {
        let dir = tempfile::tempdir().unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        for t in 1..=7u64 {
            status.set_clip(vec![t as u8], 1_000 * t);
        }
        // A later false visit does not take an earlier arrival's clip.
        assert_eq!(status.clip_mp4(Some(4_000)).unwrap()[..], [4]);
        assert_eq!(status.clip_mp4(Some(4_001)).unwrap()[..], [4]);
        assert_eq!(status.clip_mp4(None).unwrap()[..], [7]);
        // Two arrivals ago and older than the five kept: gone, in memory and on disk.
        assert!(status.clip_mp4(Some(2_000)).is_none());
        assert!(status.clip_mp4(Some(4_500)).is_none());
        assert!(!dir.path().join("clip-2000.mp4").exists());
        assert!(dir.path().join("clip-3000.mp4").exists());

        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["detection"]["clip"], 7_000);
        assert_eq!(after.clip_mp4(Some(3_000)).unwrap()[..], [3]);
    }

    #[test]
    fn a_cat_json_from_before_clips_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("visits.json"),
            r#"{"last_seen":5,"visits":3,"last_score":0.5,"recent":[{"arrived":4,"stayed_s":1}]}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["visits"], 3);
        assert!(json["detection"]["clip"].is_null());
    }

    #[test]
    fn the_log_keeps_each_visit_and_its_stay_newest_first() {
        let status = Status::new("Cam", true, 4, 4);
        status.set_detection(true, Some(0.8), Some(&arrived()));
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert!(json["detection"]["recent"][0]["arrived"].as_u64().unwrap() > 0);
        assert!(json["detection"]["recent"][0]["stayed_s"].is_null());
        status.set_detection(false, None, Some(&left(95)));
        for _ in 0..RECENT_VISITS + 4 {
            status.set_detection(true, Some(0.8), Some(&arrived()));
            status.set_detection(false, None, Some(&left(7)));
        }
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        let recent = json["detection"]["recent"].as_array().unwrap();
        assert_eq!(recent.len(), RECENT_VISITS);
        assert!(recent.iter().all(|v| v["stayed_s"] == 7));
        assert_eq!(json["detection"]["visits"], RECENT_VISITS + 5);
    }

    #[test]
    fn a_restart_closes_the_visit_it_cut_short_at_last_seen() {
        let dir = tempfile::tempdir().unwrap();
        let open = Remembered {
            last_seen: Some(1_000_030),
            visits: 2,
            last_score: Some(0.7),
            recent: vec![
                Visit {
                    arrived: 1_000_000,
                    stayed_s: None,
                    boxes: Vec::new(),
                    class: None,
                },
                Visit {
                    arrived: 900_000,
                    stayed_s: Some(60),
                    boxes: Vec::new(),
                    class: None,
                },
            ],
        };
        std::fs::write(
            dir.path().join(VISITS_FILE),
            serde_json::to_vec(&open).unwrap(),
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["recent"][0]["stayed_s"], 30);
        assert_eq!(json["detection"]["recent"][1]["stayed_s"], 60);
    }

    #[test]
    fn a_cat_json_from_before_the_log_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(VISITS_FILE),
            r#"{"last_seen":1791763273,"visits":3,"last_score":0.76}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["visits"], 3);
        assert_eq!(json["detection"]["recent"], serde_json::json!([]));
    }

    #[test]
    fn each_of_the_last_five_arrivals_keeps_its_own_picture_and_an_old_one_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        // An earlier version kept one arrival.jpg: it becomes the newest visit's picture.
        std::fs::write(dir.path().join(LEGACY_ARRIVAL_FILE), [1u8]).unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert_eq!(status.arrival_jpeg(Some(0)).unwrap(), [1u8][..]);
        assert!(!dir.path().join(LEGACY_ARRIVAL_FILE).exists());
        for t in 1..=6u64 {
            status.set_arrival(vec![t as u8 + 10], t * 100);
        }
        assert_eq!(status.arrival_jpeg(None).unwrap(), [16u8][..]);
        assert_eq!(status.arrival_jpeg(Some(301)).unwrap(), [13u8][..]);
        // Six arrivals after the adopted one: the adopted and the first new one are gone, files too.
        assert!(status.arrival_jpeg(Some(0)).is_none());
        assert!(status.arrival_jpeg(Some(100)).is_none());
        assert!(!dir.path().join("arrival-100.jpg").exists());
        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert_eq!(after.arrival_jpeg(Some(200)).unwrap(), [12u8][..]);
    }

    #[test]
    fn healthz_is_ok_only_while_a_real_camera_delivers_frames() {
        gst::init().unwrap();
        let status = Status::new("Cam", true, 4, 4);
        let health = |s: &Status| serde_json::from_str::<serde_json::Value>(&s.to_json()).unwrap();
        let fresh = health(&status);
        assert_eq!(fresh["ok"], false);
        assert_eq!(fresh["problem"], "waiting for the first frame");
        assert!(fresh["frame_age_s"].is_null() && fresh["model_age_s"].is_null());
        status.set_camera("v4l2src /dev/video0 (C920)", false);
        status.set_frame(&gst::Buffer::from_mut_slice(vec![
            0u8;
            crate::frame::size(4, 4)
        ]));
        status.set_detection(false, None, None);
        let running = health(&status);
        assert_eq!(running["ok"], true);
        assert!(running["problem"].is_null());
        assert_eq!(
            (
                running["frame_age_s"].clone(),
                running["model_age_s"].clone()
            ),
            (0.into(), 0.into())
        );
        // The camera went away and a test pattern stands in: still frames, but not ok.
        status.set_camera("test pattern (no camera found)", true);
        let stand_in = health(&status);
        assert_eq!(stand_in["ok"], false);
        assert_eq!(
            stand_in["problem"],
            "no camera found, streaming a test pattern"
        );
    }

    #[test]
    fn spots_marked_not_a_cat_are_checked_kept_and_cleared_when_the_picture_turns() {
        let dir = tempfile::tempdir().unwrap();
        let status = Status::new("Cam", true, 640, 480).remember_in(dir.path().to_path_buf());
        let rack = [0.25, 0.75, 0.75, 1.0];
        let zones = |z: Vec<Zone>| Settings {
            ignore: Some(z),
            ..Settings::default()
        };
        status.apply(zones(vec![rack])).unwrap();
        assert_eq!(status.ignored(), vec![rack]);
        let json: serde_json::Value = serde_json::from_str(&status.settings_json()).unwrap();
        assert_eq!(json["ignore"], serde_json::json!([[0.25, 0.75, 0.75, 1.0]]));
        // Kept across a restart, as every other setting the page makes.
        let again = Status::new("Cam", true, 640, 480).remember_in(dir.path().to_path_buf());
        assert_eq!(again.ignored(), vec![rack]);
        // A bad spot, or too many, changes nothing.
        for bad in [
            vec![[0.5, 0.5, 0.4, 0.9]],
            vec![[0.0, 0.0, 1.5, 1.0]],
            vec![[f32::NAN, 0.0, 1.0, 1.0]],
            vec![rack; 17],
        ] {
            assert!(status.apply(zones(bad)).is_err());
        }
        assert_eq!(status.ignored(), vec![rack]);
        // The same angle again keeps them; a turned picture no longer fits them.
        status.set_rotation(0).unwrap();
        assert_eq!(status.ignored(), vec![rack]);
        status.set_rotation(90).unwrap();
        assert!(status.ignored().is_empty());
        let again = Status::new("Cam", true, 640, 480).remember_in(dir.path().to_path_buf());
        assert!(again.ignored().is_empty());
        assert_eq!(again.rotation(), 90);
    }

    #[test]
    fn viewers_are_counted_and_capped_and_a_closed_tab_frees_its_place() {
        let status = Status::new("Cam", true, 4, 4).peered(vec!["http://10.0.0.2:8080".into()]);
        let health = |s: &Status| serde_json::from_str::<serde_json::Value>(&s.to_json()).unwrap();
        let first = status.viewer(2).unwrap();
        let second = status.viewer(2).unwrap();
        assert!(status.viewer(2).is_none());
        assert_eq!(health(&status)["viewers"], 2);
        drop(first);
        assert_eq!(health(&status)["viewers"], 1);
        let third = status.viewer(2).unwrap();
        // 0 is no limit.
        let more: Vec<Viewer> = (0..20).map(|_| status.viewer(0).unwrap()).collect();
        assert_eq!(health(&status)["viewers"], 22);
        drop((second, third, more));
        let json = health(&status);
        assert_eq!(json["viewers"], 0);
        assert_eq!(json["peers"], serde_json::json!(["http://10.0.0.2:8080"]));
        assert!(json["version"].as_str().is_some_and(|v| !v.is_empty()));
    }

    #[test]
    fn a_visit_keeps_the_boxes_it_arrived_with_across_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let status = Status::new("Cam", true, 640, 480).remember_in(dir.path().to_path_buf());
        let rack = [0.25, 0.75, 0.75, 1.0];
        status.set_boxes(vec![rack]);
        status.set_detection(
            true,
            Some(0.36),
            Some(&Change::Arrived {
                count: 1,
                best: 0.36,
                class: 15,
            }),
        );
        // The boxes move on with the picture; the visit keeps the ones it arrived with.
        status.set_boxes(Vec::new());
        status.set_detection(
            false,
            None,
            Some(&Change::Left {
                stayed: std::time::Duration::from_secs(172),
            }),
        );
        status.persist();
        let recent = |s: &Status| {
            serde_json::from_str::<serde_json::Value>(&s.to_json()).unwrap()["detection"]["recent"]
                [0]
            .clone()
        };
        assert_eq!(
            recent(&status)["boxes"],
            serde_json::json!([[0.25, 0.75, 0.75, 1.0]])
        );
        let again = Status::new("Cam", true, 640, 480).remember_in(dir.path().to_path_buf());
        assert_eq!(
            recent(&again)["boxes"],
            serde_json::json!([[0.25, 0.75, 0.75, 1.0]])
        );
        assert_eq!(recent(&again)["stayed_s"], 172);
        // A fraction no f32 holds exactly is written as the three places it means.
        status.set_boxes(vec![[0.1, 0.2, 0.3, 0.4]]);
        status.set_detection(
            true,
            Some(0.5),
            Some(&Change::Arrived {
                count: 1,
                best: 0.5,
                class: 15,
            }),
        );
        assert_eq!(
            recent(&status)["boxes"],
            serde_json::json!([[0.1, 0.2, 0.3, 0.4]])
        );
        status.persist();
        let file = std::fs::read_to_string(dir.path().join(VISITS_FILE)).unwrap();
        assert!(file.contains("[0.1,0.2,0.3,0.4]"), "{file}");
    }

    #[test]
    fn healthz_lists_the_boxes_drawn_now() {
        let status = Status::new("Cam", true, 640, 480);
        let boxes = |s: &Status| {
            serde_json::from_str::<serde_json::Value>(&s.to_json()).unwrap()["detection"]["boxes"]
                .clone()
        };
        assert_eq!(boxes(&status), serde_json::json!([]));
        status.set_boxes(vec![[0.1, 0.2, 0.3, 0.4]]);
        assert_eq!(boxes(&status), serde_json::json!([[0.1, 0.2, 0.3, 0.4]]));
    }

    #[test]
    fn a_broken_or_missing_state_starts_afresh() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert!(fresh.arrival_jpeg(None).is_none());
        std::fs::write(dir.path().join(VISITS_FILE), "{not json").unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["visits"], 0);
        assert!(json["detection"]["last_seen"].is_null());
    }

    #[test]
    fn with_the_privacy_filter_no_page_changes_the_classes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            r#"{"classes":["person"],"score":0.4}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 16, 8)
            .private(Privacy::Pixelate)
            .remember_in(dir.path().into());
        // A choice the page made before the filter was on must not unblur people now.
        assert_eq!(status.classes(), vec!["cat"]);
        assert!(
            (status.score() - 0.4).abs() < 1e-6,
            "the rest of it still holds"
        );
        let person = Settings {
            classes: Some(vec!["person".into()]),
            ..Settings::default()
        };
        let err = status.apply(person).unwrap_err();
        assert!(format!("{err}").contains("privacy filter"), "{err}");
        assert_eq!(status.classes(), vec!["cat"]);
        // The page sends the classes it has with its other changes; those still go through.
        let same = Settings {
            classes: Some(vec!["cat".into()]),
            score: Some(0.5),
            ..Settings::default()
        };
        status.apply(same).unwrap();
        let json: serde_json::Value = serde_json::from_str(&status.settings_json()).unwrap();
        assert_eq!(json["privacy"], "pixelate");
        assert_eq!(json["score"], 0.5);
    }

    #[test]
    fn a_snapshot_is_a_jpeg_of_the_last_frame() {
        gst::init().unwrap();
        let status = Status::new("Cam", false, 16, 8);
        assert!(status.snapshot_jpeg().unwrap().is_none());
        status.set_frame(&gst::Buffer::from_mut_slice(vec![
            200u8;
            crate::frame::size(16, 8)
        ]));
        let jpeg = status.snapshot_jpeg().unwrap().unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        // Asked again for the same frame: the very same bytes, no second encode.
        let again = status.snapshot_jpeg().unwrap().unwrap();
        assert_eq!(again.as_ptr(), jpeg.as_ptr());
        status.set_frame(&gst::Buffer::from_mut_slice(vec![
            20u8;
            crate::frame::size(16, 8)
        ]));
        let next = status.snapshot_jpeg().unwrap().unwrap();
        assert_ne!(next.as_ptr(), jpeg.as_ptr());
        assert_ne!(next, jpeg);
    }

    #[test]
    fn healthz_names_the_privacy_filter() {
        let off: serde_json::Value =
            serde_json::from_str(&Status::new("Cam", true, 4, 4).to_json()).unwrap();
        assert_eq!(off["privacy"], "off");
        let on = Status::new("Cam", true, 4, 4).private(Privacy::Pixelate);
        let on: serde_json::Value = serde_json::from_str(&on.to_json()).unwrap();
        assert_eq!(on["privacy"], "pixelate");
    }

    #[test]
    fn healthz_carries_the_stats_and_the_model_time_weighs_the_last_runs_most() {
        let status = Status::new("Cam", true, 4, 4);
        assert_eq!(status.model_ms(), None);
        status.record_model_time(std::time::Duration::from_millis(100));
        status.record_model_time(std::time::Duration::from_millis(50));
        assert!((status.model_ms().unwrap() - 90.0).abs() < 1e-9);
        status.set_stats(Stats {
            capture_fps: Some(29.8),
            cores: 4,
            uptime_s: 61,
            ..Stats::default()
        });
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["stats"]["capture_fps"], 29.8);
        assert_eq!(json["stats"]["cores"], 4);
        assert_eq!(json["stats"]["uptime_s"], 61);
        assert!(json["stats"]["temperature_c"].is_null());
        status.set_detection(false, None, None);
        assert_eq!(status.counters().1, 1);
    }

    #[test]
    fn the_watched_classes_come_from_the_settings_and_name_each_visit() {
        let status = Status::new("Cam", true, 4, 4);
        assert_eq!(status.classes(), ["cat"]);
        assert_eq!(status.wanted_classes(), Some(vec![15]));
        status
            .apply(Settings {
                classes: Some(vec!["cat".into(), "dog".into()]),
                ..Settings::default()
            })
            .unwrap();
        assert_eq!(status.wanted_classes(), Some(vec![15, 16]));
        let settings: serde_json::Value = serde_json::from_str(&status.settings_json()).unwrap();
        assert_eq!(settings["classes"], serde_json::json!(["cat", "dog"]));
        // A dog arrives: its visit says so.
        status.set_detection(
            true,
            Some(0.8),
            Some(&Change::Arrived {
                count: 1,
                best: 0.8,
                class: 16,
            }),
        );
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["classes"], serde_json::json!(["cat", "dog"]));
        assert_eq!(json["detection"]["recent"][0]["class"], "dog");
        // Every class: no filter.
        assert_eq!(
            Status::new("Cam", true, 4, 4)
                .watching(vec![], true)
                .wanted_classes(),
            None
        );
    }

    #[test]
    fn a_bad_class_list_changes_nothing() {
        let status = Status::new("Cam", true, 4, 4);
        for bad in [
            vec![],
            vec!["unicorn".to_owned()],
            vec!["cat".to_owned(), "cat".to_owned()],
        ] {
            assert!(status
                .apply(Settings {
                    classes: Some(bad),
                    score: Some(0.5),
                    ..Settings::default()
                })
                .is_err());
        }
        assert_eq!(status.classes(), ["cat"]);
        assert_eq!(status.score(), 0.25);
    }

    #[test]
    fn page_classes_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let before = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        before
            .apply(Settings {
                classes: Some(vec!["bird".into()]),
                ..Settings::default()
            })
            .unwrap();
        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert_eq!(after.classes(), ["bird"]);
    }

    #[test]
    fn the_visits_of_a_cat_cam_from_before_observant_carry_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(LEGACY_VISITS_FILE),
            r#"{"last_seen":5,"visits":3,"last_score":0.5,"recent":[{"arrived":4,"stayed_s":1}]}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["detection"]["visits"], 3);
        // From then on it keeps them under the new name.
        status.persist();
        assert!(dir.path().join(VISITS_FILE).is_file());
    }
}
