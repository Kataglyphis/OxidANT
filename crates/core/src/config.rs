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
    /// `KATAGLYPHIS_SWAP_XY`: set to `"1"` to swap X/Y in the detector's output coords.
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
    /// `KATAGLYPHIS_ORT_DEVICE`: `"cpu"` (default) | `"auto"` | `"cuda"` | `"migraphx"` (Linux) | `"directml"` (Windows).
    ref fn ort_device -> String, || {
        std::env::var("KATAGLYPHIS_ORT_DEVICE")
            .unwrap_or_else(|_| "cpu".to_string())
            .to_ascii_lowercase()
    }
);

env_cached!(
    /// `KATAGLYPHIS_ORT_THREADS`: ORT intra-op threads; unset takes [`family_threads`], `0` ORT's own one per core.
    fn ort_intra_threads -> usize, || {
        threads_from(std::env::var("KATAGLYPHIS_ORT_THREADS").ok().as_deref(), usable_cpus())
    }
);

/// The CPUs the thread rules count (owner D16): the smaller of the affinity mask and the cgroup CPU quota.
pub fn usable_cpus() -> usize {
    let affinity = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let quota = std::fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|membership| {
            let path = cgroup_v2_path(&membership)?;
            quota_along(path, |dir| {
                std::fs::read_to_string(
                    std::path::Path::new("/sys/fs/cgroup")
                        .join(dir)
                        .join("cpu.max"),
                )
                .ok()
            })
        });
    usable_of(affinity, quota)
}

/// `affinity` capped by a `quota` in whole CPUs, at least 1.
pub fn usable_of(affinity: usize, quota: Option<usize>) -> usize {
    quota.map_or(affinity, |quota| affinity.min(quota)).max(1)
}

/// The cgroup v2 path of `/proc/self/cgroup`'s `0::<path>` line; a v1 hierarchy has none.
pub fn cgroup_v2_path(membership: &str) -> Option<&str> {
    membership.lines().find_map(|line| line.strip_prefix("0::"))
}

/// The tightest quota of the cgroup at `path` and its ancestors up to the root, `read` giving each one's `cpu.max`.
pub fn quota_along(path: &str, read: impl Fn(&str) -> Option<String>) -> Option<usize> {
    let mut dir = path.trim_matches('/');
    let mut tightest: Option<usize> = None;
    loop {
        if let Some(quota) = read(dir).as_deref().and_then(cpu_max_quota) {
            tightest = Some(tightest.map_or(quota, |held| held.min(quota)));
        }
        if dir.is_empty() {
            return tightest;
        }
        dir = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
}

/// Whole CPUs, rounded down, of a `cpu.max` line `<quota> <period>` in microseconds; `max` sets no limit.
pub fn cpu_max_quota(cpu_max: &str) -> Option<usize> {
    let mut fields = cpu_max.split_whitespace();
    let quota: u64 = fields.next()?.parse().ok()?;
    let period: u64 = fields.next()?.parse().ok()?;
    quota
        .checked_div(period)
        .and_then(|cpus| usize::try_from(cpus).ok())
}

/// The family's ORT pool: the usable CPUs but one, 1 to 8 (owner decision); ORT's default ignores the mask.
pub fn family_threads(usable: usize) -> usize {
    usable.saturating_sub(1).clamp(1, 8)
}

/// A GenAI decoder's pool: half of [`family_threads`], 1 to 4, as decoding is memory-bound (owner decision, hub CON218).
pub fn text_threads(usable: usize) -> usize {
    (family_threads(usable) / 2).clamp(1, 4)
}

/// A set count wins, `0` included; unset or unreadable takes [`family_threads`] of `usable`.
fn threads_from(raw: Option<&str>, usable: usize) -> usize {
    raw.and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| family_threads(usable))
}

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

/// The family's one default score threshold, measured at the cameras; ModelANT's contract states it.
pub const DEFAULT_SCORE_THRESHOLD: f32 = 0.25;

env_cached!(
    /// `KATAGLYPHIS_SCORE_THRESHOLD`: detection confidence threshold (default `0.25`).
    fn score_threshold -> f32, || {
        parse_or(
            std::env::var("KATAGLYPHIS_SCORE_THRESHOLD").ok().as_deref(),
            DEFAULT_SCORE_THRESHOLD,
        )
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
    fn an_unset_thread_count_is_the_usable_cpus_but_one_from_one_to_eight() {
        assert_eq!(
            [1, 2, 4, 8, 16].map(family_threads),
            [1, 1, 3, 7, 8],
            "the usable CPUs but one, at least 1 and at most 8"
        );
        assert_eq!(threads_from(None, 4), 3);
        assert_eq!(threads_from(Some("many"), 4), 3);
        assert_eq!(threads_from(Some(" 2 "), 4), 2);
        assert_eq!(threads_from(Some("0"), 4), 0, "0 keeps ORT's own default");
    }

    /// Owner D16: a container's `--cpus 3` on a Pi 5 leaves 3 usable CPUs, not the mask's 4.
    #[test]
    fn a_usable_cpu_is_the_smaller_of_the_mask_and_the_quota() {
        for (cpu_max, quota) in [
            ("max 100000", None),
            ("300000 100000\n", Some(3)),
            ("150000 100000", Some(1)),
            ("50000 100000", Some(0)),
            ("100000 0", None),
            ("", None),
            ("max", None),
        ] {
            assert_eq!(cpu_max_quota(cpu_max), quota, "{cpu_max:?}");
        }
        assert_eq!(usable_of(4, Some(3)), 3);
        assert_eq!(usable_of(2, Some(3)), 2);
        assert_eq!(usable_of(4, None), 4);
        assert_eq!(usable_of(4, Some(0)), 1, "half a CPU still runs one thread");
        assert!(usable_cpus() >= 1);
    }

    #[test]
    fn the_quota_is_the_tightest_from_the_cgroup_up_to_the_root() {
        assert_eq!(
            cgroup_v2_path("12:cpu:/x\n0::/user.slice/app\n"),
            Some("/user.slice/app")
        );
        assert_eq!(cgroup_v2_path("4:cpu,cpuacct:/docker/1\n"), None);
        let limits = |dir: &str| match dir {
            "" => Some("400000 100000".to_owned()),
            "user.slice" => Some("200000 100000".to_owned()),
            "user.slice/app" => Some("max 100000".to_owned()),
            _ => None,
        };
        assert_eq!(quota_along("/user.slice/app", limits), Some(2));
        assert_eq!(
            quota_along("/", limits),
            Some(4),
            "inside a container its own root"
        );
        assert_eq!(quota_along("/other/deep", |_| None), None);
    }

    #[test]
    fn a_text_decoder_takes_half_the_family_pool_from_one_to_four() {
        assert_eq!(
            [1, 2, 4, 8, 16, 32].map(text_threads),
            [1, 1, 1, 3, 4, 4],
            "a Pi 5 decodes on one thread, a host pinned to 8 CPUs on three"
        );
    }

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
        // Unset or unreadable, the threshold is the family's 0.25, not the 0.5 this variable once had.
        assert_eq!(parse_or(None, DEFAULT_SCORE_THRESHOLD), 0.25);
        assert_eq!(parse_or(Some("high"), DEFAULT_SCORE_THRESHOLD), 0.25);
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
