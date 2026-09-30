// Every `KATAGLYPHIS_*` env var is read here, so the configuration surface lives in one place.

use std::sync::OnceLock;

use log::warn;

// Helper macro

/// Define an env-var accessor that parses once and caches in a `OnceLock`.
/// Variants: `fn` returns by copy, `$vis fn` sets the visibility, `ref fn` returns `&'static`.
macro_rules! env_cached {
    // Copy variant — returns `T` by value (requires `T: Copy`).
    ($(#[$attr:meta])* fn $name:ident -> $ty:ty, $init:expr) => {
        $(#[$attr])*
        pub fn $name() -> $ty {
            static CACHE: OnceLock<$ty> = OnceLock::new();
            *CACHE.get_or_init($init)
        }
    };
    // Copy variant with explicit visibility.
    ($(#[$attr:meta])* $vis:vis fn $name:ident -> $ty:ty, $init:expr) => {
        $(#[$attr])*
        $vis fn $name() -> $ty {
            static CACHE: OnceLock<$ty> = OnceLock::new();
            *CACHE.get_or_init($init)
        }
    };
    // Reference variant — returns `&'static T` to avoid cloning.
    ($(#[$attr:meta])* ref fn $name:ident -> $ty:ty, $init:expr) => {
        $(#[$attr])*
        pub fn $name() -> &'static $ty {
            static CACHE: OnceLock<$ty> = OnceLock::new();
            CACHE.get_or_init($init)
        }
    };
}

// Preprocessing

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PreprocessMode {
    Letterbox,
    #[default]
    Stretch,
}

env_cached!(
    /// `KATAGLYPHIS_PREPROCESS`: `"letterbox"` | `"stretch"` (default `"stretch"`).
    #[allow(dead_code)]
    pub fn preprocess_mode -> PreprocessMode, || {
        let raw = std::env::var("KATAGLYPHIS_PREPROCESS").unwrap_or_else(|_| "stretch".to_string());
        match raw.trim().to_ascii_lowercase().as_str() {
            "letterbox" | "boxed" | "pad" => PreprocessMode::Letterbox,
            "stretch" | "resize" => PreprocessMode::Stretch,
            other => {
                warn!("Unknown preprocess mode '{other}', defaulting to stretch");
                PreprocessMode::Stretch
            }
        }
    }
);

env_cached!(
    /// `KATAGLYPHIS_SWAP_XY`: set to `"1"` to swap X/Y in YOLO output coords.
    fn swap_xy_enabled -> bool, || {
        std::env::var("KATAGLYPHIS_SWAP_XY").ok().as_deref() == Some("1")
    }
);

// ONNX backend selection

env_cached!(
    /// `KATAGLYPHIS_ONNX_BACKEND`: `"ort"` / `"onnxruntime"` | `"tract"` | unset (auto, `None`).
    ref fn onnx_backend -> Option<String>, || {
        std::env::var("KATAGLYPHIS_ONNX_BACKEND")
            .ok()
            .map(|v| v.to_lowercase())
    }
);

env_cached!(
    /// `KATAGLYPHIS_ORT_DEVICE`: `"cpu"` (default) | `"cuda"` | `"auto"`.
    ref fn ort_device -> String, || {
        std::env::var("KATAGLYPHIS_ORT_DEVICE")
            .unwrap_or_else(|_| "cpu".to_string())
            .to_ascii_lowercase()
    }
);

// Inference / GUI

env_cached!(
    /// `KATAGLYPHIS_ONNX_MODEL`: override path to the ONNX model file.
    ref fn onnx_model_override -> Option<String>, || {
        std::env::var("KATAGLYPHIS_ONNX_MODEL").ok()
    }
);

env_cached!(
    /// `KATAGLYPHIS_SCORE_THRESHOLD`: detection confidence threshold (default `0.5`).
    fn score_threshold -> f32, || {
        std::env::var("KATAGLYPHIS_SCORE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.5)
    }
);

env_cached!(
    /// `KATAGLYPHIS_INFER_EVERY_MS`: minimum interval between inference requests (default `0`).
    fn infer_every_ms -> u64, || {
        std::env::var("KATAGLYPHIS_INFER_EVERY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    }
);

// Logging

env_cached!(
    /// `KATAGLYPHIS_LOG_LEVEL`: `error` | `warn` | `info` (default) | `debug` | `trace`.
    fn log_level -> log::LevelFilter, || {
        std::env::var("KATAGLYPHIS_LOG_LEVEL")
            .ok()
            .and_then(|v| match v.to_ascii_lowercase().as_str() {
                "error" => Some(log::LevelFilter::Error),
                "warn" | "warning" => Some(log::LevelFilter::Warn),
                "info" => Some(log::LevelFilter::Info),
                "debug" => Some(log::LevelFilter::Debug),
                "trace" => Some(log::LevelFilter::Trace),
                _ => None,
            })
            .unwrap_or(log::LevelFilter::Info)
    }
);
