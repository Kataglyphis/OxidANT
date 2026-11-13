//! A model that cannot load says why: a missing file by its path, a broken one by each backend's own reason.

use kataglyphis_inference::person_detection::PersonDetector;

fn load_error(path: &str, backend: Option<&str>) -> String {
    match PersonDetector::with_backend(path, backend) {
        Ok(_) => panic!("{path} loaded with {backend:?}"),
        Err(e) => format!("{e:#}"),
    }
}

/// A file that is not an ONNX protobuf, removed when dropped.
struct BrokenModel(std::path::PathBuf);

impl BrokenModel {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{name}-{}.onnx", std::process::id()));
        std::fs::write(&path, b"this is not an ONNX protobuf").unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for BrokenModel {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

#[test]
fn a_missing_model_is_named_as_missing_by_every_backend() {
    for backend in [None, Some("ort"), Some("tract")] {
        let err = load_error("/nonexistent/deim.onnx", backend);
        assert_eq!(
            err, "no ONNX model at /nonexistent/deim.onnx",
            "{backend:?}"
        );
    }
}

#[test]
fn a_broken_model_keeps_the_runtime_reason() {
    let model = BrokenModel::new("broken-ort");
    let err = load_error(model.path(), Some("ort"));
    let prefix = format!("ORT could not load {}: ", model.path());
    assert!(err.starts_with(&prefix), "{err}");
    assert!(err.len() > prefix.len(), "{err}");
    assert!(
        !err.contains("tract could not load"),
        "an explicit ORT has no fallback: {err}"
    );
}

#[test]
fn a_broken_model_tract_cannot_read_keeps_its_reason() {
    let model = BrokenModel::new("broken-tract");
    let err = load_error(model.path(), Some("tract"));
    assert!(
        err.starts_with(&format!("tract could not load {}: ", model.path())),
        "{err}"
    );
    assert!(!err.contains("ORT could not load"), "{err}");
}

// ORT's reason comes first, then the fallback's, so neither hides the other.
#[cfg(any(target_os = "linux", windows))]
#[test]
fn a_model_neither_backend_loads_names_both_reasons() {
    let model = BrokenModel::new("broken-both");
    let err = load_error(model.path(), None);
    assert!(
        err.starts_with(&format!("ORT could not load {}: ", model.path())),
        "{err}"
    );
    assert!(
        err.contains(&format!("; tract could not load {}: ", model.path())),
        "{err}"
    );
}

#[test]
fn an_unknown_backend_is_refused() {
    let model = BrokenModel::new("broken-backend");
    let err = load_error(model.path(), Some("tensorrt"));
    assert_eq!(
        err,
        "Unsupported KATAGLYPHIS_ONNX_BACKEND='tensorrt'. Use 'tract' or 'ort'."
    );
}

/// An ONNX file that holds only the metadata entry `key = value`: enough for the detector's check, no model behind it.
fn says(key: &str, value: &str) -> Vec<u8> {
    let text = |tag: u8, text: &str| [&[tag, text.len() as u8], text.as_bytes()].concat();
    let entry = [text(0x0A, key), text(0x12, value)].concat();
    [&[0x72, entry.len() as u8], entry.as_slice()].concat()
}

#[test]
fn a_model_that_names_another_preprocessing_or_layout_is_refused_before_any_backend_loads() {
    let model = BrokenModel::new("says-letterbox");
    for (key, value, expected) in [
        (
            "kataglyphis.preprocess",
            "letterbox",
            "the model names its preprocessing letterbox (kataglyphis.preprocess), the detector is set to stretch: set KATAGLYPHIS_PREPROCESS=letterbox",
        ),
        (
            "kataglyphis.output",
            "cxcywh_objectness_scores",
            "the model names its output layout cxcywh_objectness_scores (kataglyphis.output), this detector decodes xyxy_score_class only",
        ),
    ] {
        std::fs::write(&model.0, says(key, value)).unwrap();
        for backend in [None, Some("ort"), Some("tract")] {
            assert_eq!(load_error(model.path(), backend), expected, "{backend:?}");
        }
    }
    // What the detector does anyway is no reason to refuse: the load goes on to the backend, which finds no graph.
    std::fs::write(&model.0, says("kataglyphis.preprocess", "stretch")).unwrap();
    let err = load_error(model.path(), Some("tract"));
    assert!(!err.contains("kataglyphis."), "{err}");
}
