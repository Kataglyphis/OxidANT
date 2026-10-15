//! ObservANT's parts, which `main.rs` wires into one service; benches and integration tests reach them here.

pub mod camera;
pub mod capture;
pub mod clip;
pub mod coco;
pub mod config;
pub mod frame;
pub mod install;
pub mod motion;
pub mod notify;
pub mod output;
pub mod precompress;
pub mod presence;
pub mod static_files;
pub mod stats;
pub mod status;
pub mod web;

/// Tests: whether this GStreamer has every element; under `KATAGLYPHIS_REQUIRE_GST=1`, as CI sets it, a missing one fails.
#[cfg(test)]
pub(crate) fn gst_has(elements: &[&str]) -> bool {
    gstreamer::init().unwrap();
    let missing: Vec<&str> = elements
        .iter()
        .copied()
        .filter(|e| gstreamer::ElementFactory::find(e).is_none())
        .collect();
    if missing.is_empty() {
        return true;
    }
    let required = std::env::var("KATAGLYPHIS_REQUIRE_GST").as_deref() == Ok("1");
    assert!(!required, "this GStreamer lacks {missing:?}");
    eprintln!("SKIP: this GStreamer lacks {missing:?}");
    false
}
