//! The Renderer tab: the C++ mode switch and raster path, then the WebGPU renderer's post and culling settings.

use egui::{vec2, Button, Ui};

use super::super::menu_bar::{RENDER_MODES, RENDER_MODES_ENABLED};
use super::super::{EditorAction, EditorContext};
use crate::render::forward::RasterPath;
use crate::ui::widgets::{
    enabled_ui, frame_height, muted_wrapped, property_row, section_header, toggle, FillSlider,
    Segmented,
};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>, actions: &mut Vec<EditorAction>) {
    let mut mode = 0;
    Segmented::new(&RENDER_MODES)
        .enabled(&RENDER_MODES_ENABLED)
        .show(ui, &mut mode);
    muted_wrapped(ui, "WebGPU has no hardware ray tracing.");
    let settings = &mut *cx.settings;
    let mut path = RasterPath::ALL
        .iter()
        .position(|path| *path == settings.raster_path)
        .unwrap_or(0);
    property_row(
        ui,
        "Raster path",
        Some("Forward shades while drawing; deferred shades a G-buffer afterwards"),
        |ui| Segmented::new(&RasterPath::ALL.map(RasterPath::label)).show(ui, &mut path),
    );
    settings.raster_path = RasterPath::ALL[path];
    post(ui, cx);
    culling(ui, cx);
    if shaders(ui, cx) {
        actions.push(EditorAction::ReloadShaders);
    }
}

fn post(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    section_header(ui, "POST", None);
    let settings = &mut *cx.settings;
    property_row(ui, "Bloom", None, |ui| {
        ui.add(FillSlider::new(&mut settings.bloom_strength, 0.0..=2.0).decimals(2))
    });
    property_row(
        ui,
        "SSAO",
        Some("Ambient occlusion strength; 0 turns it off"),
        |ui| ui.add(FillSlider::new(&mut settings.ssao_strength, 0.0..=1.0).decimals(2)),
    );
    let auto = settings.auto_exposure;
    enabled_ui(ui, !auto, |ui| {
        property_row(
            ui,
            "Exposure",
            Some("In stops; auto exposure overrides it"),
            |ui| {
                ui.add(
                    FillSlider::new(&mut settings.exposure_ev, -4.0..=4.0)
                        .decimals(1)
                        .suffix(" EV"),
                )
            },
        );
    });
    property_row(
        ui,
        "Auto exposure",
        Some("Meters the frame's histogram"),
        |ui| toggle(ui, &mut settings.auto_exposure),
    );
    enabled_ui(ui, settings.auto_exposure, |ui| {
        property_row(ui, "Adaptation", Some("Higher settles faster"), |ui| {
            ui.add(FillSlider::new(&mut settings.auto_exposure_speed, 0.1..=10.0).decimals(1))
        });
    });
}

fn culling(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    let settings = &mut *cx.settings;
    section_header(ui, "FRUSTUM CULLING", Some(&mut settings.frustum_culling));
    muted_wrapped(
        ui,
        "Off draws every mesh and shadow caster. The profiler's Visibility tab counts both.",
    );
    section_header(ui, "OCCLUSION CULLING", None);
    property_row(
        ui,
        "Occlusion",
        Some("Skips what the depth of the last frame hides"),
        |ui| toggle(ui, &mut settings.occlusion_queries_enabled),
    );
    enabled_ui(ui, settings.occlusion_queries_enabled, |ui| {
        property_row(
            ui,
            "GPU culling",
            Some("A compute pass instead of occlusion queries"),
            |ui| toggle(ui, &mut settings.gpu_culling_enabled),
        );
    });
    let (drawn, considered) = cx.stats.occlusion;
    let culled = considered.saturating_sub(drawn);
    let text = if settings.occlusion_queries_enabled {
        format!("Drew {drawn} of {considered} primitives in the frustum, {culled} occluded.")
    } else {
        "Off: every primitive in the frustum is drawn.".to_owned()
    };
    muted_wrapped(ui, text);
}

/// The reload button, live only where the shell can read shader files; true on a click.
fn shaders(ui: &mut Ui, cx: &EditorContext<'_>) -> bool {
    section_header(ui, "SHADERS", None);
    let size = vec2(ui.available_width(), frame_height(ui));
    let reload = enabled_ui(ui, cx.caps.hot_reload, |ui| {
        ui.add_sized(size, Button::new("Reload all shaders"))
    })
    .inner;
    let reload = if cx.caps.hot_reload {
        reload.on_hover_text("F5")
    } else {
        reload.on_disabled_hover_text("Needs the native viewer")
    };
    reload.clicked()
}
