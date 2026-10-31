//! An explicit GPU request must not be hidden by the automatic tract fallback.
#![cfg(any(target_os = "linux", windows))]

use kataglyphis_inference::person_detection::PersonDetector;

#[test]
fn requested_gpu_reports_missing_ort_instead_of_falling_back() {
    let directory =
        std::env::temp_dir().join(format!("oxidant-gpu-request-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let model = directory.join("model.onnx");
    std::fs::write(&model, b"not a model").unwrap();
    std::env::set_var("KATAGLYPHIS_ORT_DEVICE", "cuda");
    std::env::set_var("ORT_DYLIB_PATH", directory.join("missing-ort"));
    let result = PersonDetector::with_backend(model.to_str().unwrap(), None);
    let error = match result {
        Ok(_) => panic!("missing ONNX Runtime must fail an explicit GPU request"),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.contains("ORT_DYLIB_PATH names no file"), "{error}");
    assert!(!error.contains("tract could not load"), "{error}");
    let tract_error = match PersonDetector::with_backend(model.to_str().unwrap(), Some("tract")) {
        Ok(_) => panic!("tract must not ignore an explicit GPU request"),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        tract_error.contains("tract conflicts with the requested ORT GPU device"),
        "{tract_error}"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
