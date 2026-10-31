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
    detect_persons_rgba_impl(&model_path, &rgba, width, height, score_threshold).map_err(|e| {
        error!("detect_persons_rgba failed: {e:#}");
        format!("detect_persons_rgba failed: {e:#}")
    })
}

// The detector loads its model from a file, and wasm32-unknown-unknown has no file system.
#[cfg(target_arch = "wasm32")]
fn detect_persons_rgba_impl(
    _model_path: &str,
    _rgba: &[u8],
    _width: u32,
    _height: u32,
    _score_threshold: f32,
) -> anyhow::Result<Vec<Detection>> {
    anyhow::bail!("ONNX inference needs a file system for its model, which the web build lacks")
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_persons_rgba_impl(
    model_path: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
    score_threshold: f32,
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
    guard.infer_persons_rgba(rgba, width, height, score_threshold)
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
