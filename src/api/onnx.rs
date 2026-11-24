use log::error;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
struct ModelCache<T> {
    current: Mutex<Option<(String, Arc<Mutex<T>>)>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> ModelCache<T> {
    const fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    fn get_or_load<E>(
        &self,
        path: &str,
        load: impl FnOnce() -> Result<T, E>,
    ) -> Result<Arc<Mutex<T>>, E> {
        let lock = || self.current.lock().unwrap_or_else(|e| e.into_inner());
        {
            let guard = lock();
            if let Some((cached_path, detector)) = guard.as_ref() {
                if cached_path == path {
                    return Ok(Arc::clone(detector));
                }
            }
        }
        let detector = Arc::new(Mutex::new(load()?));
        let mut guard = lock();
        if let Some((cached_path, cached)) = guard.as_ref() {
            if cached_path == path {
                return Ok(Arc::clone(cached));
            }
        }
        *guard = Some((path.to_owned(), Arc::clone(&detector)));
        Ok(detector)
    }
}

pub use crate::Detection;

use crate::api::webcam::DetectionBox;

/// The lowest score [`detect_rgba`] returns; callers raise their own bar over it.
pub const DETECT_SCORE_FLOOR: f32 = 0.25;

/// The detector's class names, at their class ids.
#[flutter_rust_bridge::frb(sync)]
pub fn detector_labels() -> Vec<String> {
    kataglyphis_core::coco::NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
}

/// Detect `classes` (class ids, see [`detector_labels`]; empty keeps every class) in an RGBA frame with the default model.
#[flutter_rust_bridge::frb(sync)]
pub fn detect_rgba(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    classes: Vec<i64>,
) -> Result<Vec<DetectionBox>, String> {
    let filter = (!classes.is_empty()).then_some(classes.as_slice());
    detect_rgba_impl("", &rgba, width, height, DETECT_SCORE_FLOOR, filter)
        .map(|found| found.into_iter().map(Into::into).collect())
        .map_err(|e| {
            error!("detect_rgba failed: {e:#}");
            format!("detect_rgba failed: {e:#}")
        })
}

/// Times `runs` every-class [`detect_rgba`] calls on one frame as the `{"p50_ms":..,"p95_ms":..}` line of `--perf-host`.
#[flutter_rust_bridge::frb(sync)]
pub fn detector_latency_json(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    runs: u32,
) -> Result<String, String> {
    if runs == 0 {
        return Err("detector_latency_json needs at least one timed run".into());
    }
    let samples = detector_latency_samples(&rgba, width, height, runs).map_err(|e| {
        error!("detector_latency_json failed: {e:#}");
        format!("detector_latency_json failed: {e:#}")
    })?;
    Ok(latency_json(samples))
}

#[cfg(target_arch = "wasm32")]
fn detector_latency_samples(_: &[u8], _: u32, _: u32, _: u32) -> anyhow::Result<Vec<f64>> {
    // Before the clock: wasm32-unknown-unknown has no Instant, and no model to time.
    detect_rgba_impl("", &[], 0, 0, DETECT_SCORE_FLOOR, None).map(|_| Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
fn detector_latency_samples(
    rgba: &[u8],
    width: u32,
    height: u32,
    runs: u32,
) -> anyhow::Result<Vec<f64>> {
    // Untimed first, so the model load and the first allocations stay out of the figures.
    const WARM_RUNS: u32 = 5;
    let mut samples = Vec::with_capacity(runs as usize);
    for run in 0..WARM_RUNS.saturating_add(runs) {
        let start = std::time::Instant::now();
        detect_rgba_impl("", rgba, width, height, DETECT_SCORE_FLOOR, None)?;
        if run >= WARM_RUNS {
            samples.push(start.elapsed().as_secs_f64() * 1e3);
        }
    }
    Ok(samples)
}

/// The p50 and p95 of `samples` (milliseconds) by nearest rank, as the app's Dart loop ranked them before.
fn latency_json(mut samples: Vec<f64>) -> String {
    samples.sort_by(f64::total_cmp);
    let rank = |fraction: f64| {
        let at = (fraction * samples.len() as f64).floor() as usize;
        samples[at.min(samples.len() - 1)]
    };
    format!("{{\"p50_ms\":{:?},\"p95_ms\":{:?}}}", rank(0.5), rank(0.95))
}

/// Run the packaged detector twice in one profiled session on a real image.
#[flutter_rust_bridge::frb(sync)]
pub fn profile_detector_image(
    image_path: String,
    output: String,
    repeat_output: String,
    profile: String,
) -> Result<(), String> {
    #[cfg(any(target_os = "linux", windows))]
    {
        use std::path::Path;
        crate::detector_profile::run(
            Path::new(&image_path),
            Path::new(&output),
            Path::new(&profile),
            Some(Path::new(&repeat_output)),
        )
        .map_err(|error| format!("profile_detector_image failed: {error:#}"))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (image_path, output, repeat_output, profile);
        Err("Profiled image inference requires Linux or Windows".into())
    }
}

/// Run the desktop detector on a blank frame and return its completed ORT profile path.
#[flutter_rust_bridge::frb(sync)]
pub fn profile_person_detection(
    model_path: String,
    profile_prefix: String,
) -> Result<String, String> {
    profile_person_detection_impl(&model_path, &profile_prefix)
        .map_err(|error| format!("{error:#}"))
}

#[cfg(any(target_os = "linux", windows))]
fn profile_person_detection_impl(model_path: &str, profile_prefix: &str) -> anyhow::Result<String> {
    use crate::person_detection::{resolve_model_path, PersonDetector};
    let model_path = resolve_model_path(Some(model_path));
    let mut detector =
        PersonDetector::with_ort_profiling(&model_path, std::path::Path::new(profile_prefix))?;
    detector.infer_persons_rgba(&[0; 64 * 64 * 4], 64, 64, 0.5)?;
    detector.end_ort_profiling()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn profile_person_detection_impl(
    _model_path: &str,
    _profile_prefix: &str,
) -> anyhow::Result<String> {
    anyhow::bail!("ONNX Runtime profiling requires Linux or Windows")
}

#[flutter_rust_bridge::frb(sync)]
pub fn detect_persons_rgba(
    model_path: String,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    score_threshold: f32,
) -> Result<Vec<Detection>, String> {
    detect_rgba_impl(
        &model_path,
        &rgba,
        width,
        height,
        score_threshold,
        Some(&[0]),
    )
    .map_err(|e| {
        error!("detect_persons_rgba failed: {e:#}");
        format!("detect_persons_rgba failed: {e:#}")
    })
}

// The detector loads its model from a file, and wasm32-unknown-unknown has no file system.
#[cfg(target_arch = "wasm32")]
fn detect_rgba_impl(
    _model_path: &str,
    _rgba: &[u8],
    _width: u32,
    _height: u32,
    _score_threshold: f32,
    _classes: Option<&[i64]>,
) -> anyhow::Result<Vec<Detection>> {
    anyhow::bail!("ONNX inference needs a file system for its model, which the web build lacks")
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_rgba_impl(
    model_path: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
    score_threshold: f32,
    classes: Option<&[i64]>,
) -> anyhow::Result<Vec<Detection>> {
    use crate::person_detection::PersonDetector;

    let resolved = crate::person_detection::resolve_model_path(Some(model_path));
    let model_path = resolved.as_str();

    static DETECTOR: OnceLock<ModelCache<PersonDetector>> = OnceLock::new();
    // Pin this request's detector even if another request replaces the one-entry cache.
    let detector = DETECTOR
        .get_or_init(ModelCache::new)
        .get_or_load(model_path, || PersonDetector::new(model_path))?;
    let mut guard = detector.lock().unwrap_or_else(|e| e.into_inner());
    guard.infer_rgba(rgba, width, height, score_threshold, classes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn profiling_reports_a_missing_model_without_creating_a_profile() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.onnx");
        let prefix = dir.path().join("profile");
        let error = profile_person_detection(
            missing.to_string_lossy().into_owned(),
            prefix.to_string_lossy().into_owned(),
        )
        .unwrap_err();
        assert!(error.contains("missing.onnx"), "{error}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_request_keeps_its_model_when_another_thread_replaces_the_cache() {
        let cache = Arc::new(ModelCache::new());
        let first = cache.get_or_load("first", || Ok::<_, ()>(1)).unwrap();
        let other = Arc::clone(&cache);
        std::thread::spawn(move || {
            let second = other.get_or_load("second", || Ok::<_, ()>(2)).unwrap();
            assert_eq!(*second.lock().unwrap(), 2);
        })
        .join()
        .unwrap();
        assert_eq!(*first.lock().unwrap(), 1);
        let second = cache.get_or_load("second", || Err::<i32, _>(())).unwrap();
        assert_eq!(*second.lock().unwrap(), 2);
    }

    #[test]
    fn a_missing_model_is_named_in_the_error() {
        let err = detect_persons_rgba("/nonexistent/model.onnx".into(), vec![0; 4], 1, 1, 0.5)
            .unwrap_err();
        assert_eq!(
            err,
            "detect_persons_rgba failed: no ONNX model at /nonexistent/model.onnx"
        );
    }

    #[test]
    fn the_latency_line_ranks_its_samples_as_perf_host_reads_them() {
        let samples = (1..=20).rev().map(f64::from).collect();
        assert_eq!(latency_json(samples), r#"{"p50_ms":11.0,"p95_ms":20.0}"#);
        assert_eq!(latency_json(vec![2.5]), r#"{"p50_ms":2.5,"p95_ms":2.5}"#);
    }

    #[test]
    fn latency_needs_a_timed_run_and_names_a_failed_detection() {
        assert!(detector_latency_json(vec![0; 4], 1, 1, 0)
            .unwrap_err()
            .contains("at least one"));
        // A short frame fails at the model or its preprocessing, before any sample is taken.
        let _turn = crate::model_turn();
        assert!(detector_latency_json(vec![0; 3], 1, 1, 1)
            .unwrap_err()
            .starts_with("detector_latency_json failed: "));
    }

    #[test]
    fn labels_are_the_detector_classes_by_id() {
        let labels = detector_labels();
        assert_eq!(labels.len(), 80);
        assert_eq!((labels[0].as_str(), labels[15].as_str()), ("person", "cat"));
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn the_class_list_picks_the_cats_and_not_a_person() {
        let path = crate::person_detection::resolve_model_path(None);
        if !std::path::Path::new(&path).is_file() {
            let required = std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() == Ok("1");
            assert!(!required, "no model at {path}");
            eprintln!("SKIP: no model at {path}");
            return;
        }
        let photo = image::load_from_memory(include_bytes!(
            "../../third_party/ModelANT/contracts/fixtures/two_cats.jpg"
        ))
        .unwrap()
        .to_rgba8();
        let (w, h) = photo.dimensions();
        let rgba = photo.into_raw();
        let _turn = crate::model_turn();
        let labels = detector_labels();
        let cats = detect_rgba(rgba.clone(), w, h, vec![15]).unwrap();
        assert!(
            cats.iter().all(|d| labels[d.class_id as usize] == "cat"),
            "{:?}",
            cats.iter().map(|d| d.class_id).collect::<Vec<_>>()
        );
        assert!(cats.iter().filter(|d| d.score >= 0.25).count() >= 2);
        assert!(detect_rgba(rgba, w, h, vec![0]).unwrap().is_empty());
    }

    #[test]
    fn the_real_model_sees_no_person_in_a_black_frame() {
        let path = crate::person_detection::resolve_model_path(None);
        if !std::path::Path::new(&path).is_file() {
            let required = std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() == Ok("1");
            assert!(!required, "no model at {path}");
            eprintln!("SKIP: no model at {path}");
            return;
        }
        let _turn = crate::model_turn();
        // The second call takes the cached detector.
        for _ in 0..2 {
            let found =
                detect_persons_rgba(path.clone(), vec![0; 64 * 64 * 4], 64, 64, 0.5).unwrap();
            assert!(found.is_empty(), "{found:?}");
        }
    }
}
