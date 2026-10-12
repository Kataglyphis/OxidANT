//! What `/healthz` reports (which camera is live, how often the model ran, when a cat was last seen)
//! and the latest annotated frame `/snapshot.jpg` encodes.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use gstreamer as gst;
use serde::{Deserialize, Serialize};

use crate::presence::Change;

/// The cat's visits, kept across restarts in the state directory.
const CAT_FILE: &str = "cat.json";

/// The frame of the last arrival, kept across restarts beside it.
const ARRIVAL_FILE: &str = "arrival.jpg";
/// How many arrivals' clips stay addressable: a false visit must not replace the clip a notification links.
const CLIPS_KEPT: usize = 5;
/// How far a requested arrival time may be from a kept clip's and still name it.
const CLIP_TIME_SLACK_S: u64 = 2;

/// How many visits `/healthz` lists and the state directory keeps.
const RECENT_VISITS: usize = 20;

/// One visit: when the cat was confirmed, and how long it stayed once it has left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Visit {
    arrived: u64,
    stayed_s: Option<u64>,
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
struct Cat {
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
}

pub struct Status {
    name: String,
    inference: bool,
    width: u32,
    height: u32,
    camera: Mutex<String>,
    cat: Mutex<Cat>,
    frame: Mutex<Option<gst::Buffer>>,
    /// The JPEG of the frame the last arrival was confirmed on.
    arrival: Mutex<Option<Vec<u8>>>,
    /// The last [`CLIPS_KEPT`] arrivals as MP4, newest first, by arrival time.
    clips: Mutex<VecDeque<(u64, bytes::Bytes)>>,
    /// Where [`CAT_FILE`] and [`ARRIVAL_FILE`] live; `None` forgets everything at exit.
    state_dir: Option<PathBuf>,
}

impl Status {
    pub fn new(name: &str, inference: bool, width: u32, height: u32) -> Self {
        Self {
            name: name.to_owned(),
            inference,
            width,
            height,
            camera: Mutex::new("starting".to_owned()),
            cat: Mutex::new(Cat::default()),
            frame: Mutex::new(None),
            arrival: Mutex::new(None),
            clips: Mutex::new(VecDeque::new()),
            state_dir: None,
        }
    }

    /// Restores the cat's visits and last arrival frame from `dir` and keeps them there from now on.
    pub fn remember_in(mut self, dir: PathBuf) -> Self {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log::warn!("{}: {err}", dir.display());
        }
        match std::fs::read_to_string(dir.join(CAT_FILE)) {
            Ok(text) => match serde_json::from_str::<Remembered>(&text) {
                Ok(r) => {
                    if let Ok(cat) = self.cat.get_mut() {
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
                Err(err) => log::warn!("{}: {err}; starting afresh", dir.join(CAT_FILE).display()),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!("{}: {err}", dir.join(CAT_FILE).display()),
        }
        if let (Ok(jpeg), Ok(arrival)) = (
            std::fs::read(dir.join(ARRIVAL_FILE)),
            self.arrival.get_mut(),
        ) {
            *arrival = Some(jpeg);
        }
        let restored = read_clips(&dir);
        if let (Ok(cat), Some((newest, _))) = (self.cat.get_mut(), restored.front()) {
            cat.clip = Some(*newest);
        }
        if let Ok(clips) = self.clips.get_mut() {
            *clips = restored;
        }
        self.state_dir = Some(dir);
        self
    }

    /// Writes what a restart should keep; called on every arrival and departure.
    pub fn persist(&self) {
        let Some(dir) = &self.state_dir else {
            return;
        };
        let Ok(remembered) = self.cat.lock().map(|cat| Remembered {
            last_seen: cat.last_seen,
            visits: cat.visits,
            last_score: cat.last_seen.map(|_| cat.best_score),
            recent: cat.recent.iter().copied().collect(),
        }) else {
            return;
        };
        let json = serde_json::to_vec(&remembered).unwrap_or_default();
        if let Err(err) = write_atomic(&dir.join(CAT_FILE), &json) {
            log::warn!("{}: {err}", dir.join(CAT_FILE).display());
        }
    }

    pub fn set_camera(&self, camera: &str) {
        if let Ok(mut current) = self.camera.lock() {
            camera.clone_into(&mut current);
        }
    }

    /// Records one inference run's view of the cat, after presence smoothing, and the visit it starts or ends.
    pub fn set_cat(&self, in_view: bool, best_score: Option<f32>, change: Option<&Change>) {
        if let Ok(mut cat) = self.cat.lock() {
            cat.runs += 1;
            cat.hit = best_score.is_some();
            cat.in_view = in_view;
            if in_view {
                cat.last_seen = Some(unix_now());
                cat.best_score = best_score.unwrap_or(cat.best_score);
            }
            match change {
                Some(Change::Arrived { .. }) => {
                    cat.visits += 1;
                    cat.recent.push_front(Visit {
                        arrived: unix_now(),
                        stayed_s: None,
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
    pub fn cat_seen(&self) -> bool {
        self.cat.lock().is_ok_and(|cat| cat.in_view || cat.hit)
    }

    /// Keeps a reference to the frame just pushed; a buffer is refcounted, so this copies nothing.
    pub fn set_frame(&self, frame: &gst::Buffer) {
        if let Ok(mut current) = self.frame.lock() {
            *current = Some(frame.clone());
        }
    }

    pub fn set_arrival(&self, jpeg: Vec<u8>) {
        if let Some(dir) = &self.state_dir {
            if let Err(err) = write_atomic(&dir.join(ARRIVAL_FILE), &jpeg) {
                log::warn!("{}: {err}", dir.join(ARRIVAL_FILE).display());
            }
        }
        if let Ok(mut current) = self.arrival.lock() {
            *current = Some(jpeg);
        }
    }

    pub fn arrival_jpeg(&self) -> Option<Vec<u8>> {
        self.arrival.lock().ok().and_then(|a| a.clone())
    }

    /// Keeps the clip of the arrival at `arrived` with the last few, on disk too, and names it in `/healthz`.
    pub fn set_clip(&self, mp4: Vec<u8>, arrived: u64) {
        if let Some(dir) = &self.state_dir {
            let path = dir.join(clip_file(arrived));
            if let Err(err) = write_atomic(&path, &mp4) {
                log::warn!("{}: {err}", path.display());
            }
        }
        if let Ok(mut clips) = self.clips.lock() {
            clips.retain(|(t, _)| *t != arrived);
            clips.push_front((arrived, mp4.into()));
            while clips.len() > CLIPS_KEPT {
                if let (Some((old, _)), Some(dir)) = (clips.pop_back(), &self.state_dir) {
                    let _ = std::fs::remove_file(dir.join(clip_file(old)));
                }
            }
        }
        if let Ok(mut cat) = self.cat.lock() {
            cat.clip = Some(arrived);
        }
    }

    /// The newest clip, or with `at` the one of the arrival at that time; `None` once it is dropped.
    pub fn clip_mp4(&self, at: Option<u64>) -> Option<bytes::Bytes> {
        let clips = self.clips.lock().ok()?;
        match at {
            None => clips.front().map(|(_, mp4)| mp4.clone()),
            Some(at) => clips
                .iter()
                .filter(|(t, _)| t.abs_diff(at) <= CLIP_TIME_SLACK_S)
                .min_by_key(|(t, _)| t.abs_diff(at))
                .map(|(_, mp4)| mp4.clone()),
        }
    }

    /// When the newest visit began, as its log entry says.
    pub fn latest_arrival(&self) -> Option<u64> {
        self.cat
            .lock()
            .ok()
            .and_then(|cat| cat.recent.front().map(|v| v.arrived))
    }

    /// The latest annotated frame as JPEG, or `None` before the first frame.
    pub fn snapshot_jpeg(&self) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(buffer) = self.frame.lock().ok().and_then(|f| f.clone()) else {
            return Ok(None);
        };
        let map = buffer
            .map_readable()
            .map_err(|_| anyhow::anyhow!("map frame"))?;
        let i420 = map
            .as_slice()
            .get(..crate::frame::size(self.width, self.height))
            .ok_or_else(|| anyhow::anyhow!("frame smaller than its caps"))?;
        let rgba = crate::frame::to_rgba(i420, self.width, self.height);
        encode_jpeg(&rgba, self.width, self.height).map(Some)
    }

    pub fn to_json(&self) -> String {
        let camera = self.camera.lock().map(|c| c.clone()).unwrap_or_default();
        let cat = self.cat.lock();
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
        let recent: Vec<Visit> = cat
            .as_deref()
            .map(|c| c.recent.iter().copied().collect())
            .unwrap_or_default();
        serde_json::json!({
            "ok": true,
            "name": self.name,
            "camera": camera,
            "inference": self.inference,
            "model_runs": runs,
            "cat": {
                "in_view": in_view,
                "last_seen": last_seen,
                "visits": visits,
                "last_score": (last_seen.is_some()).then_some(best),
                "recent": recent,
                "clip": clip,
            },
        })
        .to_string()
    }
}

fn clip_file(arrived: u64) -> String {
    format!("clip-{arrived}.mp4")
}

/// The clips a previous run left in `dir`, newest first, at most [`CLIPS_KEPT`].
fn read_clips(dir: &Path) -> VecDeque<(u64, bytes::Bytes)> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let time = name
                .strip_prefix("clip-")?
                .strip_suffix(".mp4")?
                .parse()
                .ok()?;
            Some((time, entry.path()))
        })
        .collect();
    found.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    found
        .into_iter()
        .take(CLIPS_KEPT)
        .filter_map(|(time, path)| std::fs::read(path).ok().map(|mp4| (time, mp4.into())))
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
        }
    }

    fn left(seconds: u64) -> Change {
        Change::Left {
            stayed: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn reports_the_camera_last_set() {
        let status = Status::new("Cat \"Cam\"", true, 4, 4);
        status.set_camera("v4l2src /dev/video0 (C920)");
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["camera"], "v4l2src /dev/video0 (C920)");
        assert_eq!(json["name"], "Cat \"Cam\"");
        assert_eq!(json["inference"], true);
        assert_eq!(json["cat"]["in_view"], false);
        assert!(json["cat"]["last_seen"].is_null());
    }

    #[test]
    fn a_visit_is_counted_once_and_last_seen_stays_after_it_ends() {
        let status = Status::new("Cam", true, 4, 4);
        status.set_cat(true, Some(0.8), Some(&arrived()));
        status.set_cat(true, Some(0.6), None);
        status.set_cat(false, None, None);
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["in_view"], false);
        assert_eq!(json["cat"]["visits"], 1);
        assert_eq!(json["model_runs"], 3);
        assert!(!status.cat_seen());
        status.set_cat(false, Some(0.3), None);
        assert!(status.cat_seen());
        assert!(json["cat"]["last_seen"].as_u64().unwrap() > 0);
        assert!((json["cat"]["last_score"].as_f64().unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_restart_remembers_the_visits_and_the_arrival_frame() {
        let dir = tempfile::tempdir().unwrap();
        let before = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        before.set_cat(true, Some(0.7), Some(&arrived()));
        before.set_arrival(vec![0xFF, 0xD8, 0xFF, 0xD9]);
        before.set_cat(false, None, Some(&left(42)));
        before.persist();

        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["cat"]["visits"], 1);
        assert_eq!(json["cat"]["in_view"], false);
        assert!(json["cat"]["last_seen"].as_u64().unwrap() > 0);
        assert!((json["cat"]["last_score"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(after.arrival_jpeg().unwrap(), [0xFF, 0xD8, 0xFF, 0xD9]);
        assert_eq!(json["cat"]["recent"][0]["stayed_s"], 42);
        assert!(!dir.path().join("cat.partial").exists());
    }

    #[test]
    fn a_restart_keeps_the_clip_and_the_arrival_it_shows() {
        let dir = tempfile::tempdir().unwrap();
        let before = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert!(before.clip_mp4(None).is_none());
        before.set_cat(true, Some(0.7), Some(&arrived()));
        // As the inference thread does after every arrival and departure.
        before.persist();
        let arrived_at = before.latest_arrival().unwrap();
        before.set_clip(b"\0\0\0\x18ftypisom".to_vec(), arrived_at);

        let after = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&after.to_json()).unwrap();
        assert_eq!(json["cat"]["clip"], arrived_at);
        assert_eq!(json["cat"]["recent"][0]["arrived"], arrived_at);
        assert_eq!(&after.clip_mp4(None).unwrap()[4..8], b"ftyp");
        assert!(!dir
            .path()
            .join(format!("clip-{arrived_at}.partial"))
            .exists());
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
        assert_eq!(json["cat"]["clip"], 7_000);
        assert_eq!(after.clip_mp4(Some(3_000)).unwrap()[..], [3]);
    }

    #[test]
    fn a_cat_json_from_before_clips_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("cat.json"),
            r#"{"last_seen":5,"visits":3,"last_score":0.5,"recent":[{"arrived":4,"stayed_s":1}]}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["visits"], 3);
        assert!(json["cat"]["clip"].is_null());
    }

    #[test]
    fn the_log_keeps_each_visit_and_its_stay_newest_first() {
        let status = Status::new("Cam", true, 4, 4);
        status.set_cat(true, Some(0.8), Some(&arrived()));
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert!(json["cat"]["recent"][0]["arrived"].as_u64().unwrap() > 0);
        assert!(json["cat"]["recent"][0]["stayed_s"].is_null());
        status.set_cat(false, None, Some(&left(95)));
        for _ in 0..RECENT_VISITS + 4 {
            status.set_cat(true, Some(0.8), Some(&arrived()));
            status.set_cat(false, None, Some(&left(7)));
        }
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        let recent = json["cat"]["recent"].as_array().unwrap();
        assert_eq!(recent.len(), RECENT_VISITS);
        assert!(recent.iter().all(|v| v["stayed_s"] == 7));
        assert_eq!(json["cat"]["visits"], RECENT_VISITS + 5);
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
                },
                Visit {
                    arrived: 900_000,
                    stayed_s: Some(60),
                },
            ],
        };
        std::fs::write(
            dir.path().join(CAT_FILE),
            serde_json::to_vec(&open).unwrap(),
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["recent"][0]["stayed_s"], 30);
        assert_eq!(json["cat"]["recent"][1]["stayed_s"], 60);
    }

    #[test]
    fn a_cat_json_from_before_the_log_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(CAT_FILE),
            r#"{"last_seen":1791763273,"visits":3,"last_score":0.76}"#,
        )
        .unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["visits"], 3);
        assert_eq!(json["cat"]["recent"], serde_json::json!([]));
    }

    #[test]
    fn a_broken_or_missing_state_starts_afresh() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        assert!(fresh.arrival_jpeg().is_none());
        std::fs::write(dir.path().join(CAT_FILE), "{not json").unwrap();
        let status = Status::new("Cam", true, 4, 4).remember_in(dir.path().to_path_buf());
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["visits"], 0);
        assert!(json["cat"]["last_seen"].is_null());
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
    }
}
