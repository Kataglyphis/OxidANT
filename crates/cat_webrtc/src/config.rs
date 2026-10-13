//! Service settings: built-in defaults, then a TOML file, then command-line flags.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _};
use serde::{Deserialize, Serialize};

/// Environment variable naming the config file, for installs that keep it elsewhere.
pub const CONFIG_ENV: &str = "KATAGLYPHIS_CATCAM_CONFIG";

/// Where a packaged install keeps its config, read only when it exists: `%ProgramData%` on Windows, `/etc` elsewhere.
pub fn system_config() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("omni-accelerant").join("catcam.toml")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/omni-accelerant/catcam.toml")
    }
}

/// Whether the YOLO model runs: `auto` skips it on a small board or when it does not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Inference {
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `auto`, `rpicam`, `libcamera`, `v4l2[:DEVICE]`, `mf`, `ks`, `test` or `image:FILE`.
    pub camera: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// 0, 90, 180 or 270 degrees.
    pub rotate: u32,
    /// Producer name shown to viewers.
    pub name: String,
    pub inference: Inference,
    /// ONNX model; unset means `KATAGLYPHIS_ONNX_MODEL`, then the one beside the binary.
    pub model: Option<String>,
    pub score: f32,
    /// Detections per second at most; 0 runs the model on every frame it can take.
    pub inference_fps: f32,
    /// The model's rate while nothing moves and no cat is in view; at `inference_fps` or above the motion gate is off.
    pub idle_inference_fps: f32,
    /// ONNX Runtime threads for the model; 0 takes one per core.
    pub inference_threads: u32,
    /// Keep every COCO class instead of only cats.
    pub all_classes: bool,
    pub http_host: String,
    /// 0 turns the built-in web server off.
    pub http_port: u16,
    /// The Flutter web build; unset means `$KATAGLYPHIS_WEB_ROOT`, then `web/` beside the binary.
    pub web_root: Option<PathBuf>,
    /// The built-in signalling server; the web server proxies `/webrtc-ws` to it.
    pub signalling_host: String,
    pub signalling_port: u16,
    /// TLS for the signalling server itself, for clients that connect to it directly.
    pub cert: Option<String>,
    pub key: Option<String>,
    /// `stun://host:port`; empty or unset keeps the stream LAN-only, with no outside lookup.
    pub stun_server: Option<String>,
    /// An http(s) URL POSTed a JSON event whenever a cat arrives or leaves; empty or unset sends nothing.
    pub notify_url: Option<String>,
    /// Seconds of H.264 clip per arrival at `/arrival.mp4`, three of them before it; 0 records none.
    pub clip_seconds: u32,
    /// UDP range for WebRTC media, so a firewall can open exactly it; 0..0 leaves it free.
    pub ice_port_min: u16,
    pub ice_port_max: u16,
    /// Host names besides IPs and local names (`cat`, `*.local`, `*.fritz.box`, ...) under which the
    /// page may open the stream and change settings, such as a reverse proxy's.
    pub allowed_hosts: Vec<String>,
    /// Browser tabs connected to the stream at once; each one the cat cam encodes for costs CPU.
    /// 0 means no limit.
    pub max_viewers: u32,
    /// The other cat cams, as origins (`http://192.168.1.20:8080`): their pages may read this one's
    /// status and pictures, and this one's page lists them.
    pub peers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            camera: "auto".into(),
            width: 640,
            height: 480,
            fps: 30,
            rotate: 0,
            name: "Trouble Tabbls Cat Cam".into(),
            inference: Inference::Auto,
            model: None,
            score: 0.25,
            // Unbounded, YOLO held a Pi 5's four cores at 380 %; two boxes a second still follow a cat.
            inference_fps: 2.0,
            idle_inference_fps: 0.2,
            // At 2 Hz a Pi 5 ran at 183 % with ORT's four spinning threads, 81 % with two that sleep (2026-10-11).
            inference_threads: 2,
            all_classes: false,
            http_host: "0.0.0.0".into(),
            http_port: 8080,
            web_root: None,
            signalling_host: "127.0.0.1".into(),
            signalling_port: 8443,
            cert: None,
            key: None,
            stun_server: None,
            notify_url: None,
            clip_seconds: 10,
            ice_port_min: 40000,
            ice_port_max: 40099,
            allowed_hosts: Vec::new(),
            max_viewers: 8,
            peers: Vec::new(),
        }
    }
}

impl Config {
    /// Parses a config file's text; unknown keys are errors, so a typo cannot silently fall back.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// `explicit`, else `$KATAGLYPHIS_CATCAM_CONFIG`, else [`system_config`] when present, else defaults.
    pub fn load(explicit: Option<&Path>) -> anyhow::Result<(Self, Option<PathBuf>)> {
        let path = explicit
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(CONFIG_ENV).map(PathBuf::from))
            .or_else(|| {
                let system = system_config();
                system.is_file().then_some(system)
            });
        let Some(path) = path else {
            return Ok((Self::default(), None));
        };
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read config {}", path.display()))?;
        let config =
            Self::parse(&text).with_context(|| format!("parse config {}", path.display()))?;
        Ok((config, Some(path)))
    }

    /// The STUN server to hand to webrtcsink, if any.
    pub fn stun(&self) -> Option<&str> {
        self.stun_server
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// The webhook to POST cat events to, if any.
    pub fn notify(&self) -> Option<&str> {
        self.notify_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if ![0, 90, 180, 270].contains(&self.rotate) {
            bail!("rotate must be 0, 90, 180 or 270 (got {})", self.rotate);
        }
        let even = |n: u32| n != 0 && n.is_multiple_of(2);
        if !even(self.width) || !even(self.height) {
            bail!(
                "width and height must be even and non-zero (got {}x{})",
                self.width,
                self.height
            );
        }
        if self.fps == 0 {
            bail!("fps must be non-zero");
        }
        if !(0.0..=1.0).contains(&self.score) {
            bail!("score must lie in 0..=1 (got {})", self.score);
        }
        if !self.inference_fps.is_finite() || self.inference_fps < 0.0 {
            bail!(
                "inference_fps must be 0 or positive (got {})",
                self.inference_fps
            );
        }
        if !self.idle_inference_fps.is_finite() || self.idle_inference_fps <= 0.0 {
            bail!(
                "idle_inference_fps must be positive (got {})",
                self.idle_inference_fps
            );
        }
        if self.signalling_port == 0 {
            bail!("signalling_port must be non-zero");
        }
        // A clip is held in memory until encoded: 60 s at 640x480 is about 280 MB of frames.
        if self.clip_seconds > 60 {
            bail!(
                "clip_seconds must lie in 0..=60 (got {})",
                self.clip_seconds
            );
        }
        let range = (self.ice_port_min, self.ice_port_max);
        if range != (0, 0) && (range.0 == 0 || range.0 > range.1) {
            bail!(
                "ice_port_min..ice_port_max must be an ascending range or 0..0 (got {}..{})",
                range.0,
                range.1
            );
        }
        if self.cert.is_some() != self.key.is_some() {
            bail!("cert and key go together");
        }
        if let Some(url) = self.notify() {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                bail!("notify_url must be an http:// or https:// URL (got {url:?})");
            }
        }
        for peer in &self.peers {
            let rest = peer
                .strip_prefix("http://")
                .or_else(|| peer.strip_prefix("https://"));
            if rest.is_none_or(|r| r.is_empty() || r.trim_end_matches('/').contains('/')) {
                bail!("a peer is an origin such as \"http://192.168.1.20:8080\", no path (got {peer:?})");
            }
        }
        crate::camera::Choice::parse(&self.camera)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn load_reads_the_file_it_is_given_and_names_it_in_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("catcam.toml");
        std::fs::write(&file, "fps = 15\nname = \"Garden\"\n").unwrap();
        let (config, source) = Config::load(Some(&file)).unwrap();
        assert_eq!((config.fps, config.name.as_str()), (15, "Garden"));
        assert_eq!(source.as_deref(), Some(file.as_path()));

        // An unreadable file and a typo are different mistakes; both say which file.
        let missing = dir.path().join("nope.toml");
        let err = format!("{:#}", Config::load(Some(&missing)).unwrap_err());
        assert!(
            err.contains("read config") && err.contains("nope.toml"),
            "{err}"
        );
        std::fs::write(&file, "fsp = 15\n").unwrap();
        let err = format!("{:#}", Config::load(Some(&file)).unwrap_err());
        assert!(
            err.contains("parse config") && err.contains("catcam.toml"),
            "{err}"
        );
    }

    #[test]
    fn a_partial_file_overrides_only_what_it_names() {
        let config = Config::parse("camera = \"v4l2:/dev/video2\"\nrotate = 180\n").unwrap();
        assert_eq!(config.camera, "v4l2:/dev/video2");
        assert_eq!(config.rotate, 180);
        assert_eq!(config.http_port, Config::default().http_port);
    }

    #[test]
    fn inference_threads_reads_from_the_file() {
        assert_eq!(Config::default().inference_threads, 2);
        assert_eq!(
            Config::parse("inference_threads = 0")
                .unwrap()
                .inference_threads,
            0
        );
    }

    #[test]
    fn a_misspelt_key_is_an_error_not_a_default() {
        assert!(Config::parse("camra = \"rpicam\"\n").is_err());
    }

    #[test]
    fn inference_reads_lowercase_modes() {
        assert_eq!(
            Config::parse("inference = \"off\"").unwrap().inference,
            Inference::Off
        );
        assert!(Config::parse("inference = \"sometimes\"").is_err());
    }

    #[test]
    fn a_blank_stun_server_means_none() {
        let mut config = Config::default();
        assert_eq!(config.stun(), None);
        config.stun_server = Some("  ".into());
        assert_eq!(config.stun(), None);
        config.stun_server = Some("stun://stun.example:3478".into());
        assert_eq!(config.stun(), Some("stun://stun.example:3478"));
    }

    #[test]
    fn a_blank_notify_url_sends_nothing() {
        let mut config = Config::default();
        assert_eq!(config.notify(), None);
        config.notify_url = Some(" ".into());
        assert_eq!(config.notify(), None);
        assert!(config.validate().is_ok());
        config.notify_url = Some("http://localhost:8123/api/webhook/cats".into());
        assert_eq!(
            config.notify(),
            Some("http://localhost:8123/api/webhook/cats")
        );
    }

    #[test]
    fn validation_rejects_what_the_pipeline_cannot_take() {
        assert!(Config::default().validate().is_ok());
        let bad = [
            Config {
                rotate: 45,
                ..Config::default()
            },
            Config {
                inference_fps: -1.0,
                ..Config::default()
            },
            Config {
                idle_inference_fps: 0.0,
                ..Config::default()
            },
            Config {
                width: 641,
                ..Config::default()
            },
            Config {
                fps: 0,
                ..Config::default()
            },
            Config {
                ice_port_min: 50000,
                ice_port_max: 40000,
                ..Config::default()
            },
            Config {
                cert: Some("cert.pem".into()),
                ..Config::default()
            },
            Config {
                camera: "webcam".into(),
                ..Config::default()
            },
            Config {
                notify_url: Some("mqtt://broker/cats".into()),
                ..Config::default()
            },
            Config {
                score: 1.5,
                ..Config::default()
            },
            Config {
                signalling_port: 0,
                ..Config::default()
            },
            Config {
                clip_seconds: 61,
                ..Config::default()
            },
        ];
        for config in bad {
            assert!(config.validate().is_err(), "accepted {config:?}");
        }
        let unrestricted = Config {
            ice_port_min: 0,
            ice_port_max: 0,
            ..Config::default()
        };
        assert!(unrestricted.validate().is_ok());
    }

    #[test]
    fn a_peer_is_an_origin_and_nothing_more() {
        let peers = |list: &[&str]| Config {
            peers: list.iter().map(|p| (*p).to_owned()).collect(),
            ..Config::default()
        };
        assert!(
            peers(&["http://192.168.188.151:8080", "https://cat.local/"])
                .validate()
                .is_ok()
        );
        for bad in [
            "192.168.188.151:8080",
            "ftp://cat.local",
            "http://",
            "http://cat.local/stream",
        ] {
            assert!(peers(&[bad]).validate().is_err(), "accepted {bad}");
        }
        // Unset, none are listed and eight tabs may watch.
        let text = toml::to_string(&Config::default()).unwrap();
        let back = Config::parse(&text).unwrap();
        assert!(back.peers.is_empty());
        assert_eq!(back.max_viewers, 8);
    }

    #[test]
    fn the_effective_config_round_trips_through_toml() {
        let config = Config {
            camera: "rpicam".into(),
            model: Some("/opt/models/yolo26n.onnx".into()),
            ..Config::default()
        };
        let text = toml::to_string(&config).unwrap();
        assert_eq!(Config::parse(&text).unwrap(), config);
    }
}
