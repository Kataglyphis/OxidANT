//! A model that cannot load says why: a missing file by its path, a broken one by ORT's own reason.
#![cfg(all(feature = "onnxruntime", not(feature = "onnx_tract")))]

use kataglyphis_inference::person_detection::PersonDetector;

fn load_error(path: &str) -> String {
    match PersonDetector::new(path) {
        Ok(_) => panic!("{path} loaded"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn a_missing_model_is_named_as_missing() {
    let err = load_error("/nonexistent/deim.onnx");
    assert_eq!(err, "no ONNX model at /nonexistent/deim.onnx");
}

#[test]
fn a_broken_model_keeps_the_runtime_reason() {
    let path = std::env::temp_dir().join(format!("broken-model-{}.onnx", std::process::id()));
    std::fs::write(&path, b"this is not an ONNX protobuf").unwrap();
    let err = load_error(path.to_str().unwrap());
    std::fs::remove_file(&path).ok();
    assert!(
        err.starts_with(&format!("ORT could not load {}: ", path.display())),
        "{err}"
    );
    assert!(!err.contains("No ONNX backend available"), "{err}");
}
