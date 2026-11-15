//! The Sky tab: the procedural sky's image-based light, then the C++ CLOUDS section; the skybox switch follows.

use egui::Ui;

use super::super::EditorContext;
use super::clouds;
use crate::ui::widgets::{caption_line, property_row, FillSlider};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    caption_line(ui, "ENVIRONMENT", None);
    let ambient = &mut cx.settings.light_dir_ambient.w;
    property_row(
        ui,
        "Ambient",
        Some("How strongly the sky lights the scene"),
        |ui| ui.add(FillSlider::new(ambient, 0.0..=2.0).decimals(2)),
    );
    clouds::show(ui, &mut cx.settings.clouds);
}
