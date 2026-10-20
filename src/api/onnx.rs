use log::error;

pub use crate::Detection;

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
    use std::sync::{Mutex, OnceLock};

    use crate::person_detection::PersonDetector;

    let resolved = crate::person_detection::resolve_model_path(Some(model_path));
    let model_path = resolved.as_str();

    struct Cached {
        model_path: String,
        detector: PersonDetector,
    }

    static DETECTOR: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    let mutex = DETECTOR.get_or_init(|| Mutex::new(None));

    let lock_guard = || {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    };

    // Check whether a reload is needed while holding the lock briefly.
    let needs_reload = {
        let guard = lock_guard();
        guard
            .as_ref()
            .map(|c| c.model_path != model_path)
            .unwrap_or(true)
    };

    // Load outside the lock so concurrent callers are not blocked for a multi-second load.
    if needs_reload {
        let detector = PersonDetector::new(model_path)?;
        let mut guard = lock_guard();
        // Re-check: another thread may have loaded the same model meanwhile.
        let still_needs = guard
            .as_ref()
            .map(|c| c.model_path != model_path)
            .unwrap_or(true);
        if still_needs {
            *guard = Some(Cached {
                model_path: model_path.to_string(),
                detector,
            });
        }
    }

    let mut guard = lock_guard();
    let Some(cached) = guard.as_mut() else {
        return Ok(Vec::new());
    };
    cached
        .detector
        .infer_persons_rgba(rgba, width, height, score_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // The second call takes the cached detector.
        for _ in 0..2 {
            let found =
                detect_persons_rgba(path.clone(), vec![0; 64 * 64 * 4], 64, 64, 0.5).unwrap();
            assert!(found.is_empty(), "{found:?}");
        }
    }
}
