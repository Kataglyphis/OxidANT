//! What `/healthz` reports: which camera is live and whether the model runs.

use std::sync::Mutex;

pub struct Status {
    name: String,
    inference: bool,
    camera: Mutex<String>,
}

impl Status {
    pub fn new(name: &str, inference: bool) -> Self {
        Self {
            name: name.to_owned(),
            inference,
            camera: Mutex::new("starting".to_owned()),
        }
    }

    pub fn set_camera(&self, camera: &str) {
        if let Ok(mut current) = self.camera.lock() {
            camera.clone_into(&mut current);
        }
    }

    pub fn to_json(&self) -> String {
        let camera = self.camera.lock().map(|c| c.clone()).unwrap_or_default();
        serde_json::json!({
            "ok": true,
            "name": self.name,
            "camera": camera,
            "inference": self.inference,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_camera_last_set() {
        let status = Status::new("Cat \"Cam\"", true);
        status.set_camera("v4l2src /dev/video0 (C920)");
        let json: serde_json::Value = serde_json::from_str(&status.to_json()).unwrap();
        assert_eq!(json["camera"], "v4l2src /dev/video0 (C920)");
        assert_eq!(json["name"], "Cat \"Cam\"");
        assert_eq!(json["inference"], true);
    }
}
