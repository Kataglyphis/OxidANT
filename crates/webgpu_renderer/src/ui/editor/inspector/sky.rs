//! The Sky tab: the C++ SKYBOX switch, the image-based light the sky gives the scene, then the C++ CLOUDS section.

use egui::Ui;
use kataglyphis_render_core::sun::AMBIENT_RANGE;

use super::super::EditorContext;
use super::clouds;
use crate::ui::widgets::{caption_line, muted_wrapped, property_row, section_header, FillSlider};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    caption_line(ui, "SKYBOX", Some(&mut cx.settings.skybox_enabled));
    if !cx.stats.cubemap {
        muted_wrapped(ui, "No cube map loaded, so the procedural sky stands in.");
    }
    section_header(ui, "ENVIRONMENT", None);
    let ambient = &mut cx.settings.light_dir_ambient.w;
    property_row(
        ui,
        "Ambient",
        Some("How strongly the sky lights the scene"),
        |ui| ui.add(FillSlider::new(ambient, AMBIENT_RANGE).decimals(2)),
    );
    clouds::show(ui, &mut cx.settings.clouds);
}
