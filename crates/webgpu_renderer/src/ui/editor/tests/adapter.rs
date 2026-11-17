//! The Renderer tab's adapter list and the status bar's adapter hint.

use super::*;
use crate::adapter::{hint, AdapterClass};

#[test]
fn use_asks_the_host_to_restart_on_that_adapter_and_the_one_in_use_has_no_button() {
    // Tall enough that the inspector shows the ADAPTER section without a scroll.
    let mut studio = Harness::new((1600.0, 3000.0), 1.0);
    studio.editor.select(SceneObject::Renderer);
    studio.frame(Vec::new());
    assert!(studio.text_rect("In use").is_some());
    let button = studio
        .text_rect("Use")
        .expect("the other adapter has a Use button");
    let output = studio.frame(click(button.center().x, button.center().y));
    assert_eq!(output.actions, [EditorAction::UseAdapter(1)]);
}

#[test]
fn the_status_bar_hints_at_an_integrated_browser_gpu() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    assert!(studio.text_rect("Integrated GPU").is_none());
    studio.device.hint = hint(AdapterClass::Integrated, true, false);
    studio.device.api = "WebGPU".into();
    studio.device.name = "intel gen-12lp".into();
    assert!(studio.text_rect("WebGPU  ·  intel gen-12lp").is_some());
    assert!(studio.text_rect("Integrated GPU").is_some());
}
