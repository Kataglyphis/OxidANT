//! The detector with the real model on a real photo: both cats found, where they are, and taken for nothing else.

mod common;

use common::{assert_both_cats, model, photo, CAT, PERSON};
use kataglyphis_inference::person_detection::PersonDetector;

#[test]
fn the_photo_is_byte_for_byte_the_one_modelant_holds() {
    let ours: &[u8] = include_bytes!("fixtures/two_cats.jpg");
    let theirs: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../third_party/ModelANT/contracts/fixtures/two_cats.jpg"
    ));
    // Not assert_eq: a failure would print both files byte by byte.
    assert!(
        ours == theirs,
        "tests/fixtures/two_cats.jpg ({} bytes) is not third_party/ModelANT's contracts/fixtures/two_cats.jpg ({} bytes)",
        ours.len(),
        theirs.len()
    );
}

#[test]
fn the_real_model_finds_both_cats_in_the_photo() {
    let Some(path) = model() else { return };
    let (rgba, w, h) = photo();
    let mut detector = PersonDetector::new(&path).unwrap();
    // The chain ORT is the backend wherever the family builds one; tract only steps in when it fails.
    let primary = if cfg!(any(target_os = "linux", windows)) {
        "ort"
    } else {
        "tract"
    };
    if detector.backend_name() != primary {
        let why = PersonDetector::with_backend(&path, Some(primary)).err();
        panic!(
            "backend {} instead of {primary}: {why:#?}",
            detector.backend_name()
        );
    }
    let found = detector.infer_rgba(&rgba, w, h, 0.4, None).unwrap();
    assert_both_cats(&found, h, w);
}

#[test]
fn tract_finds_both_cats_in_the_photo_too() {
    let Some(path) = model() else { return };
    let (rgba, w, h) = photo();
    let mut detector = PersonDetector::with_backend(&path, Some("tract")).unwrap();
    assert_eq!(detector.backend_name(), "tract");
    let found = detector.infer_rgba(&rgba, w, h, 0.4, None).unwrap();
    assert_both_cats(&found, h, w);
}

#[test]
fn a_class_filter_keeps_only_its_classes() {
    let Some(path) = model() else { return };
    let (rgba, w, h) = photo();
    let mut detector = PersonDetector::new(&path).unwrap();
    let people = detector
        .infer_rgba(&rgba, w, h, 0.4, Some(&[PERSON]))
        .unwrap();
    assert!(people.is_empty(), "no person in the photo: {people:?}");
    let cats = detector.infer_rgba(&rgba, w, h, 0.4, Some(&[CAT])).unwrap();
    assert!(
        !cats.is_empty() && cats.iter().all(|d| d.class_id == CAT),
        "{cats:?}"
    );
}
