//! What the real-model tests share: the model, the photo and what must be found in it.
#![allow(dead_code)]

use kataglyphis_core::detection::Detection;
use kataglyphis_inference::person_detection::resolve_model_path;

pub const CAT: i64 = 15;
pub const PERSON: i64 = 0;
pub const DOG: i64 = 16;

/// The model `scripts/linux/fetch-models.sh` puts in the checkout; `None` skips, unless CI demands it.
pub fn model() -> Option<String> {
    let path = resolve_model_path(None);
    if std::path::Path::new(&path).is_file() {
        return Some(path);
    }
    let required = std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() == Ok("1");
    assert!(
        !required,
        "no model at {path}: scripts/linux/fetch-models.sh fetches it"
    );
    eprintln!("SKIP: no model at {path} (scripts/linux/fetch-models.sh fetches it)");
    None
}

pub fn photo() -> (Vec<u8>, u32, u32) {
    let photo = image::load_from_memory(include_bytes!("../fixtures/two_cats.jpg"))
        .unwrap()
        .to_rgba8();
    let (w, h) = photo.dimensions();
    (photo.into_raw(), w, h)
}

/// Both cats found in the photo's own pixels, and neither taken for a dog or a person.
pub fn assert_both_cats(found: &[Detection], h: u32, w: u32) {
    let cats: Vec<_> = found.iter().filter(|d| d.class_id == CAT).collect();
    assert!(cats.len() >= 2, "both cats: {found:?}");
    // The one lying in front fills the lower half, and the model is sure of it.
    assert!(
        cats.iter()
            .any(|d| d.score >= 0.8 && (d.y1 + d.y2) / 2.0 > h as f32 * 0.55),
        "{found:?}"
    );
    let (wf, hf) = (w as f32, h as f32);
    for d in found {
        assert!(d.x1 < d.x2 && d.y1 < d.y2, "{d:?}");
        // In the photo's own pixels, give or take the model's few pixels past the edge.
        assert!(d.x1 > -0.05 * wf && d.x2 < 1.05 * wf, "{d:?}");
        assert!(d.y1 > -0.05 * hf && d.y2 < 1.05 * hf, "{d:?}");
    }
    // Scenery may count (the green behind them reads as a couch, 0.57); the cats must not read as a dog or a person.
    assert!(
        found
            .iter()
            .all(|d| ![PERSON, DOG].contains(&d.class_id) || d.score < 0.5),
        "a cat taken for something else: {found:?}"
    );
}
