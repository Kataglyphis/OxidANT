//! The Model tab: the C++ transform rows over the loaded scene, then its source.

use egui::{vec2, Button, Label, RichText, Ui};
use glam::Vec3;

use super::super::EditorContext;
use crate::render::model_transform::ModelTransform;
use crate::ui::palette as pal;
use crate::ui::widgets::{
    drag_axes, enabled_ui, frame_height, muted_wrapped, property_row, section_header,
};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    let transform = &mut cx.settings.model_transform;
    enabled_ui(ui, cx.model.is_some(), |ui| {
        let unbounded = f32::NEG_INFINITY..=f32::INFINITY;
        let mut position = transform.position.to_array();
        property_row(ui, "Position", None, |ui| {
            drag_axes(ui, &mut position, 0.1, unbounded.clone(), 2, "")
        });
        transform.position = Vec3::from_array(position);
        let mut rotation = transform.rotation_deg.to_array();
        property_row(ui, "Rotation", Some("Degrees about X, Y and Z"), |ui| {
            drag_axes(ui, &mut rotation, 0.5, unbounded, 1, "°")
        });
        transform.rotation_deg = Vec3::from_array(rotation);
        let size = vec2(ui.available_width(), frame_height(ui));
        if ui.add_sized(size, Button::new("Reset transform")).clicked() {
            *transform = ModelTransform::default();
        }
    });
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
