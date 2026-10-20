//! The detector demo on the real model through the chain ORT, with a few adapter steps on its outputs.
#![cfg(any(target_os = "linux", windows))]

use kataglyphis_inference::person_detection::resolve_model_path;

#[test]
fn the_detector_demo_runs_the_real_model() {
    let path = resolve_model_path(None);
    if !std::path::Path::new(&path).is_file() {
        let required = std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() == Ok("1");
        assert!(
            !required,
            "no model at {path}: scripts/linux/fetch-models.sh fetches it"
        );
        eprintln!("SKIP: no model at {path} (scripts/linux/fetch-models.sh fetches it)");
        return;
    }
    let device = kataglyphis_burn_demos::training_device();
    let shape = kataglyphis_burn_demos::onnx_detector::onnx_detector_demo(
        std::path::Path::new(&path),
        0,
        1,
        3,
        2,
        0.001,
        &device,
    )
    .unwrap();
    // The family's detector layout: [1, N, xyxy + score + class].
    assert_eq!((shape.len(), shape[0], shape[2]), (3, 1, 6), "{shape:?}");
}
