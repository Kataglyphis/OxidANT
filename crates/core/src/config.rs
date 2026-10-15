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
        parse_preprocess(std::env::var("KATAGLYPHIS_PREPROCESS").ok().as_deref())
    }
);

/// The preprocess mode a `KATAGLYPHIS_PREPROCESS` value names; unset or unknown is stretch.
fn parse_preprocess(raw: Option<&str>) -> PreprocessMode {
    match raw.map(|r| r.trim().to_ascii_lowercase()).as_deref() {
        None | Some("stretch" | "resize") => PreprocessMode::Stretch,
        Some("letterbox" | "boxed" | "pad") => PreprocessMode::Letterbox,
        Some(other) => {
            warn!("Unknown preprocess mode '{other}', defaulting to stretch");
            PreprocessMode::Stretch
        }
    }
}

/// `raw` parsed as `T` after trimming, else `default`: a typo falls back rather than failing a start.
fn parse_or<T: std::str::FromStr>(raw: Option<&str>, default: T) -> T {
    raw.and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

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

env_cached!(
    /// `KATAGLYPHIS_ORT_THREADS`: ORT intra-op threads; unset or `0` keeps ORT's one per core.
    fn ort_intra_threads -> usize, || {
        parse_or(std::env::var("KATAGLYPHIS_ORT_THREADS").ok().as_deref(), 0)
    }
);

env_cached!(
    /// `KATAGLYPHIS_ORT_SPIN`: `"0"` stops idle ORT workers spin-waiting for the next run (default: ORT's, on).
    fn ort_spin_enabled -> bool, || {
        std::env::var("KATAGLYPHIS_ORT_SPIN").ok().as_deref() != Some("0")
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
        parse_or(std::env::var("KATAGLYPHIS_SCORE_THRESHOLD").ok().as_deref(), 0.5)
    }
);

env_cached!(
    /// `KATAGLYPHIS_INFER_EVERY_MS`: minimum interval between inference requests (default `0`).
    fn infer_every_ms -> u64, || {
        parse_or(std::env::var("KATAGLYPHIS_INFER_EVERY_MS").ok().as_deref(), 0)
    }
);

// Logging

env_cached!(
    /// `KATAGLYPHIS_LOG_LEVEL`: `error` | `warn` | `info` (default) | `debug` | `trace`.
    fn log_level -> log::LevelFilter, || {
        parse_log_level(std::env::var("KATAGLYPHIS_LOG_LEVEL").ok().as_deref())
    }
);

/// The level a `KATAGLYPHIS_LOG_LEVEL` value names, any case; unset or unknown is info.
fn parse_log_level(raw: Option<&str>) -> log::LevelFilter {
    match raw.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("error") => log::LevelFilter::Error,
        Some("warn" | "warning") => log::LevelFilter::Warn,
        Some("debug") => log::LevelFilter::Debug,
        Some("trace") => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_takes_its_synonyms_in_any_case_and_falls_back_to_stretch() {
        assert_eq!(parse_preprocess(None), PreprocessMode::Stretch);
        for raw in ["letterbox", " LetterBox ", "boxed", "pad"] {
            assert_eq!(
                parse_preprocess(Some(raw)),
                PreprocessMode::Letterbox,
                "{raw}"
            );
        }
        for raw in ["stretch", "RESIZE", "crop", ""] {
            assert_eq!(
                parse_preprocess(Some(raw)),
                PreprocessMode::Stretch,
                "{raw}"
            );
        }
    }

    #[test]
    fn numbers_are_trimmed_and_a_typo_keeps_the_default() {
        assert_eq!(parse_or(Some(" 4 "), 0usize), 4);
        assert_eq!(parse_or(Some("four"), 0usize), 0);
        assert_eq!(parse_or(None, 7u64), 7);
        assert_eq!(parse_or(Some(" 0.4"), 0.5f32), 0.4);
        assert_eq!(
            parse_or(Some("-1"), 0u64),
            0,
            "a negative interval is not a u64"
        );
    }

    #[test]
    fn log_levels_are_read_in_any_case_and_default_to_info() {
        assert_eq!(parse_log_level(Some("ERROR")), log::LevelFilter::Error);
        assert_eq!(parse_log_level(Some("warning")), log::LevelFilter::Warn);
        assert_eq!(parse_log_level(Some(" debug ")), log::LevelFilter::Debug);
        assert_eq!(parse_log_level(Some("trace")), log::LevelFilter::Trace);
        assert_eq!(parse_log_level(Some("info")), log::LevelFilter::Info);
        assert_eq!(parse_log_level(Some("loud")), log::LevelFilter::Info);
        assert_eq!(parse_log_level(None), log::LevelFilter::Info);
    }
}
