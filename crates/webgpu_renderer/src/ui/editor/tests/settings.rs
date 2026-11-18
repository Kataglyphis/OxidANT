//! The C++ rows the renderer now backs: the Sun's shadows, the skybox switch, the model transform and frustum culling.

use egui::Pos2;
use glam::Vec3;

use super::*;
use crate::render::model_transform::ModelTransform;

/// The knob of the pill switch at the right end of `caption`'s line.
pub(super) fn switch_of(studio: &mut Harness, caption: &str) -> Pos2 {
    let line = studio
        .text_rect(caption)
        .unwrap_or_else(|| panic!("{caption} is painted"));
    let (_, full) = studio.painted_frame(Vec::new());
    let mut knobs = Vec::new();
    for clipped in &full.shapes {
        collect_circles(&clipped.shape, &mut knobs);
    }
    full.drop_without_applying_deltas();
    knobs
        .into_iter()
        .rfind(|knob| (knob.y - line.center().y).abs() < 4.0 && knob.x > line.right())
        .unwrap_or_else(|| panic!("no switch on the {caption} line"))
}

fn collect_circles(shape: &egui::Shape, knobs: &mut Vec<Pos2>) {
    match shape {
        egui::Shape::Circle(circle) => knobs.push(circle.center),
        egui::Shape::Vec(shapes) => shapes
            .iter()
            .for_each(|shape| collect_circles(shape, knobs)),
        _ => {}
    }
}

/// Showing the tab leaves an unlisted resolution alone; the renderer draws its nearest listed size, 512.
#[test]
fn the_sun_tab_does_not_rewrite_an_unlisted_shadow_resolution() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.settings.shadows.resolution = 600;
    studio.editor.select(SceneObject::Sun);
    studio.frame(Vec::new());
    studio.frame(Vec::new());
    assert_eq!(studio.settings.shadows.resolution, 600);
    click_text(&mut studio, "1024");
    assert_eq!(studio.settings.shadows.resolution, 1024);
}

#[test]
fn the_shadows_section_writes_every_cpp_control_and_its_switch_disables_them() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Sun);
    studio.frame(Vec::new());
    assert!(
        studio.text_rect("3 cascades").is_some(),
        "the outliner names them"
    );
    click_text(&mut studio, "4096");
    // The cascade row paints its 1 before the radius row does.
    click_text(&mut studio, "1");
    click_text(&mut studio, "5");
    let shadows = studio.settings.shadows;
    assert_eq!(
        (
            shadows.resolution,
            shadows.cascade_count,
            shadows.pcf_radius
        ),
        (4096, 1, 5)
    );
    assert!(studio.text_rect("1 cascades").is_some());
    let knob = switch_of(&mut studio, "SHADOWS");
    studio.frame(click(knob.x, knob.y));
    assert!(!studio.settings.shadows.enabled);
    click_text(&mut studio, "512");
    assert_eq!(
        studio.settings.shadows.resolution, 4096,
        "the grid is disabled"
    );
    assert!(studio.text_rect("no shadows").is_some());
}

#[test]
fn the_light_shadows_switch_is_its_own_and_outlives_the_suns() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Sun);
    studio.frame(Vec::new());
    let knob = switch_of(&mut studio, "SHADOWS");
    studio.frame(click(knob.x, knob.y));
    assert!(!studio.settings.shadows.enabled);
    assert!(
        studio.settings.shadows.punctual,
        "the sun's switch leaves the lights' alone"
    );
    let knob = switch_of(&mut studio, "LIGHT SHADOWS");
    studio.frame(click(knob.x, knob.y));
    assert!(!studio.settings.shadows.punctual);
}

#[test]
fn the_skybox_eye_and_the_sky_tab_switch_both_toggle_the_skybox() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    let row = studio.text_rect("Skybox").expect("the outliner row");
    // The eye's ink ends where a detail ends, such as the Sun row's.
    let detail = studio
        .text_rect("3 cascades")
        .expect("the Sun row's detail");
    studio.frame(click(detail.right() - 6.0, row.center().y));
    assert!(!studio.settings.skybox_enabled);
    assert_eq!(
        studio.editor.selection(),
        SceneObject::Model,
        "the eye selects nothing"
    );
    studio.editor.select(SceneObject::Skybox);
    studio.frame(Vec::new());
    assert!(
        studio.text_rect("Procedural").is_some(),
        "no cube map is loaded"
    );
    let knob = switch_of(&mut studio, "SKYBOX");
    studio.frame(click(knob.x, knob.y));
    assert!(studio.settings.skybox_enabled);
    studio.stats.cubemap = true;
    studio.frame(Vec::new());
    assert!(studio.text_rect("Cube map").is_some());
}

#[test]
fn reset_transform_zeroes_what_the_model_tab_shows() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.settings.model_transform = ModelTransform {
        position: Vec3::new(1.5, 0.0, 0.0),
        rotation_deg: Vec3::new(0.0, 30.0, 0.0),
    };
    studio.editor.select(SceneObject::Model);
    studio.frame(Vec::new());
    assert!(studio.text_rect("1.50").is_some(), "the position shows");
    // egui paints a drag field's suffix apart from its digits.
    assert!(studio.text_rect("30.0").is_some(), "the rotation shows");
    click_text(&mut studio, "Reset transform");
    assert_eq!(studio.settings.model_transform, ModelTransform::default());
}

#[test]
fn frustum_culling_is_one_switch_in_the_renderer_tab_and_the_visibility_tab() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.stats.primitives = (3, 7);
    studio.editor.select(SceneObject::Renderer);
    studio.frame(Vec::new());
    let knob = switch_of(&mut studio, "FRUSTUM CULLING");
    studio.frame(click(knob.x, knob.y));
    assert!(!studio.settings.frustum_culling);
    studio.editor.profiler_tab = 1;
    studio.frame(Vec::new());
    assert!(
        studio
            .text_rect("Culled: 4 primitives (57.1 %), 4 shadow casters (44.4 %)")
            .is_some(),
        "the Visibility tab counts what the camera pass and the cascades culled"
    );
    let knob = switch_of(&mut studio, "Frustum culling");
    studio.frame(click(knob.x, knob.y));
    assert!(studio.settings.frustum_culling);
}
