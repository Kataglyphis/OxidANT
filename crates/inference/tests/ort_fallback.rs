//! When the chain ORT cannot load, the detector warns with ORT's reason and tract still finds the cats.
#![cfg(any(target_os = "linux", windows))]

mod common;

use std::sync::Mutex;

use common::{assert_both_cats, model, photo};
use kataglyphis_inference::person_detection::PersonDetector;

/// Keeps every warning, so the test can see the fallback announced.
struct Warnings(Mutex<Vec<String>>);

impl log::Log for Warnings {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static WARNINGS: Warnings = Warnings(Mutex::new(Vec::new()));

#[test]
fn a_missing_onnx_runtime_falls_back_to_tract_and_says_why() {
    // Set before this process loads ORT: an explicit path is the only candidate, so the load fails.
    let missing = std::env::temp_dir()
        .join("no-onnxruntime-here")
        .join("libonnxruntime.so");
    std::env::set_var("ORT_DYLIB_PATH", &missing);
    log::set_logger(&WARNINGS).unwrap();
    log::set_max_level(log::LevelFilter::Warn);

    let Some(path) = model() else { return };
    let (rgba, w, h) = photo();
    let mut detector = PersonDetector::with_backend(&path, None).unwrap();
    assert_eq!(detector.backend_name(), "tract");
    let found = detector.infer_rgba(&rgba, w, h, 0.4, None).unwrap();
    assert_both_cats(&found, h, w);

    let warnings = WARNINGS.0.lock().unwrap();
    assert!(
        warnings.iter().any(
            |w| w.starts_with("Falling back to tract: ORT could not load ")
                && w.contains("ORT_DYLIB_PATH names no file")
        ),
        "{warnings:?}"
    );
}
