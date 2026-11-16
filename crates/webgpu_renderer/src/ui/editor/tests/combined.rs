//! The rows the three renderer features share once merged: one Render menu, one Renderer and Sky tab, both eyes.

use egui::{vec2, MouseWheelSource, MouseWheelUnit, TouchPhase};

use super::settings::switch_of;
use super::*;
use crate::render::forward::RasterPath;
use crate::render::gpu_timing::TimedPass;
use crate::render::tracer::RenderMode;

/// Where a pass paints `text`, and the clip rect it is painted inside.
fn text_and_clip(studio: &mut Harness, text: &str) -> Option<(Rect, Rect)> {
    let (_, full) = studio.painted_frame(Vec::new());
    let found = full.shapes.iter().find_map(|clipped| match &clipped.shape {
        egui::Shape::Text(shape) if shape.galley.text() == text => Some((
            shape.galley.rect.translate(shape.pos.to_vec2()),
            clipped.clip_rect,
        )),
        _ => None,
    });
    full.drop_without_applying_deltas();
    found
}

#[test]
fn the_render_menu_holds_the_raster_path_radios_and_the_frustum_culling_switch() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    click_text(&mut studio, "Render");
    for item in ["Forward", "Deferred", "Frustum culling"] {
        assert!(
            studio.text_rect(item).is_some(),
            "the Render menu lacks {item}"
        );
    }
    click_text(&mut studio, "Frustum culling");
    assert!(
        !studio.settings.frustum_culling,
        "the menu's switch turns culling off"
    );
    if studio.text_rect("Deferred").is_none() {
        click_text(&mut studio, "Render");
    }
    click_text(&mut studio, "Deferred");
    assert_eq!(studio.settings.raster_path, RasterPath::Deferred);
    assert!(
        !studio.settings.frustum_culling,
        "the radio left culling alone"
    );
}

#[test]
fn the_renderer_tab_switches_the_raster_path_and_frustum_culling_together() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Renderer);
    studio.frame(Vec::new());
    click_text(&mut studio, "Deferred");
    let knob = switch_of(&mut studio, "FRUSTUM CULLING");
    studio.frame(click(knob.x, knob.y));
    assert_eq!(studio.settings.raster_path, RasterPath::Deferred);
    assert!(!studio.settings.frustum_culling);
    assert!(
        studio.text_rect("OCCLUSION CULLING").is_some(),
        "the occlusion rows stay beside the switch"
    );
}

/// FXAA stays live under a traced frame: Always filters traced frames too.
#[test]
fn the_renderer_tab_picks_fxaa_in_every_mode() {
    use crate::render::fxaa::FxaaMode;
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Renderer);
    studio.stats.drawn_mode = RenderMode::PathTraced;
    studio.frame(Vec::new());
    assert_eq!(studio.settings.fxaa, FxaaMode::Deferred);
    click_text(&mut studio, "Always");
    assert_eq!(studio.settings.fxaa, FxaaMode::Always);
    click_text(&mut studio, "Deferred only");
    assert_eq!(studio.settings.fxaa, FxaaMode::Deferred);
}

#[test]
fn the_sky_tab_switches_the_skybox_and_the_clouds_apart() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.editor.select(SceneObject::Skybox);
    studio.frame(Vec::new());
    for section in ["SKYBOX", "ENVIRONMENT", "CLOUDS"] {
        assert!(
            studio.text_rect(section).is_some(),
            "the Sky tab lacks {section}"
        );
    }
    let clouds = switch_of(&mut studio, "CLOUDS");
    studio.frame(click(clouds.x, clouds.y));
    assert!(studio.settings.clouds.enabled && studio.settings.skybox_enabled);
    let skybox = switch_of(&mut studio, "SKYBOX");
    studio.frame(click(skybox.x, skybox.y));
    assert!(studio.settings.clouds.enabled && !studio.settings.skybox_enabled);
}

#[test]
fn the_outliner_shows_the_cascades_and_each_eye_switches_its_own_row() {
    let mut studio = Harness::new((1600.0, 900.0), 1.0);
    studio.frame(Vec::new());
    let detail = studio
        .text_rect("3 cascades")
        .expect("the Sun row counts its cascades");
    // The eye's ink ends where a row's detail ends.
    let eye = |studio: &mut Harness, row: &str| {
        let row = studio
            .text_rect(row)
            .unwrap_or_else(|| panic!("the {row} row"));
        egui::pos2(detail.right() - 6.0, row.center().y)
    };
    let clouds = eye(&mut studio, "Clouds");
    studio.frame(click(clouds.x, clouds.y));
    assert!(studio.settings.clouds.enabled && studio.settings.skybox_enabled);
    let skybox = eye(&mut studio, "Skybox");
    studio.frame(click(skybox.x, skybox.y));
    assert!(studio.settings.clouds.enabled && !studio.settings.skybox_enabled);
    assert_eq!(
        studio.editor.selection(),
        SceneObject::Model,
        "an eye selects nothing"
    );
}

#[test]
fn the_profiler_scrolls_every_timed_pass_and_keeps_the_culling_counts() {
    let mut studio = Harness::new((1600.0, 700.0), 1.0);
    studio.stats.gpu_passes = TimedPass::ALL
        .iter()
        .map(|pass| (pass.label(), 0.5))
        .collect();
    studio.stats.primitives = (3, 7);
    studio.frame(Vec::new());
    let last = TimedPass::ALL[TimedPass::ALL.len() - 1].label();
    let (first, first_clip) =
        text_and_clip(&mut studio, "Shadow cascades").expect("the first pass");
    let (row, clip) = text_and_clip(&mut studio, last).expect("the last pass is laid out");
    assert!(first_clip.contains_rect(first), "the first pass shows");
    assert!(
        row.bottom() > clip.bottom(),
        "eleven passes fit the dock, so nothing scrolls: {row:?} in {clip:?}"
    );
    let wheel = Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -400.0),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    };
    studio.frame(vec![moved(first.center().x, first.center().y), wheel]);
    for _ in 0..30 {
        studio.frame(vec![moved(first.center().x, first.center().y)]);
    }
    let (row, clip) = text_and_clip(&mut studio, last).expect("the last pass");
    assert!(
        clip.contains_rect(row),
        "the wheel scrolled the last pass into view: {row:?} in {clip:?}"
    );
    studio.editor.profiler_tab = 1;
    studio.frame(Vec::new());
    assert!(
        studio.text_rect("Frustum culling").is_some(),
        "the Visibility tab keeps its switch"
    );
    assert!(
        studio
            .text_rect("Culled: 4 primitives (57.1 %), 4 shadow casters (44.4 %)")
            .is_some(),
        "the Visibility tab keeps the culled counts"
    );
}

/// A traced frame shades neither way, so the raster path's rows are off in the Renderer tab and the Render menu.
#[test]
fn a_traced_frame_disables_the_raster_path_rows() {
    let drawing = |drawn: RenderMode, object: SceneObject| {
        let mut studio = Harness::new((1600.0, 900.0), 1.0);
        studio.settings.render_mode = drawn;
        studio.stats.drawn_mode = drawn;
        studio.editor.select(object);
        studio.frame(Vec::new());
        studio
    };
    for drawn in [RenderMode::Raster, RenderMode::PathTraced] {
        let mut tab = drawing(drawn, SceneObject::Renderer);
        click_text(&mut tab, "Deferred");
        // The Model tab has no "Deferred" of its own, so the click lands on the menu's radio.
        let mut menu = drawing(drawn, SceneObject::Model);
        click_text(&mut menu, "Render");
        click_text(&mut menu, "Deferred");
        let expected = if drawn.is_traced() {
            RasterPath::Forward
        } else {
            RasterPath::Deferred
        };
        let picked = (tab.settings.raster_path, menu.settings.raster_path);
        assert_eq!(picked, (expected, expected), "drawing {drawn:?}");
    }
}
