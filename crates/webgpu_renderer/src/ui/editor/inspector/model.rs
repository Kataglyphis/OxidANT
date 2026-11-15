//! The Model tab: the C++ transform rows, disabled until the renderer has a root transform, and the model's source.

use egui::{vec2, Button, Label, RichText, Ui};

use super::super::EditorContext;
use crate::ui::palette as pal;
use crate::ui::widgets::{
    drag_axes, enabled_ui, frame_height, muted_wrapped, property_row, section_header,
};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    enabled_ui(ui, false, |ui| {
        let unbounded = f32::NEG_INFINITY..=f32::INFINITY;
        let mut position = [0.0; 3];
        property_row(ui, "Position", None, |ui| {
            drag_axes(ui, &mut position, 0.1, unbounded.clone(), 2, "")
        });
        let mut rotation = [0.0; 3];
        property_row(ui, "Rotation", Some("Degrees about X, Y and Z"), |ui| {
            drag_axes(ui, &mut rotation, 0.5, unbounded, 1, "°")
        });
        let size = vec2(ui.available_width(), frame_height(ui));
        ui.add_sized(size, Button::new("Reset transform"));
    })
    .response
    .on_disabled_hover_text("The renderer has no model transform yet");
    section_header(ui, "SOURCE", None);
    match cx.model {
        Some(model) => {
            let path = RichText::new(&model.path)
                .monospace()
                .color(pal::TEXT_MUTED);
            ui.add(Label::new(path).wrap());
            if let Some(credit) = cx.scene.credit {
                muted_wrapped(ui, credit);
            }
        }
        None => {
            muted_wrapped(ui, "Pick a model in the scene library.");
        }
    }
}
