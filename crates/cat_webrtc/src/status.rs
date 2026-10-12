//! What `/healthz` reports (which camera is live, how often the model ran, when a cat was last seen)
//! and the latest annotated frame `/snapshot.jpg` encodes.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use gstreamer as gst;

#[derive(Default)]
struct Cat {
    in_view: bool,
    last_seen: Option<u64>,
    visits: u64,
    best_score: f32,
    runs: u64,
    /// The last run saw a cat, confirmed or not.
    hit: bool,
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
        }
    }

    pub fn set_camera(&self, camera: &str) {
        if let Ok(mut current) = self.camera.lock() {
            camera.clone_into(&mut current);
        }
    }

    /// Records one inference run's view of the cat, after presence smoothing.
    pub fn set_cat(&self, in_view: bool, best_score: Option<f32>, arrived: bool) {
        if let Ok(mut cat) = self.cat.lock() {
            cat.runs += 1;
            cat.hit = best_score.is_some();
            cat.in_view = in_view;
            if in_view {
                cat.last_seen = Some(unix_now());
                cat.best_score = best_score.unwrap_or(cat.best_score);
            }
            if arrived {
                cat.visits += 1;
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
        if let Ok(mut current) = self.arrival.lock() {
            *current = Some(jpeg);
        }
    }

    pub fn arrival_jpeg(&self) -> Option<Vec<u8>> {
        self.arrival.lock().ok().and_then(|a| a.clone())
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
        let (in_view, last_seen, visits, best, runs) = cat
            .as_deref()
            .map(|c| (c.in_view, c.last_seen, c.visits, c.best_score, c.runs))
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
            },
        })
        .to_string()
    }
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
        status.set_cat(true, Some(0.8), true);
        status.set_cat(true, Some(0.6), false);
        status.set_cat(false, None, false);
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["cat"]["in_view"], false);
        assert_eq!(json["cat"]["visits"], 1);
        assert_eq!(json["model_runs"], 3);
        assert!(!status.cat_seen());
        status.set_cat(false, Some(0.3), false);
        assert!(status.cat_seen());
        assert!(json["cat"]["last_seen"].as_u64().unwrap() > 0);
        assert!((json["cat"]["last_score"].as_f64().unwrap() - 0.6).abs() < 1e-6);
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
