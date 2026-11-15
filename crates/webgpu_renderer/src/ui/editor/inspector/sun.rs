//! The Sun tab: radiance, colour and the direction from the sun into the scene, bound to the renderer's light.

use egui::{vec2, Align2, Sense, TextStyle, Ui};
use glam::Vec3;

use super::super::{EditorContext, RendererSettings};
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{drag_axes, frame_fill, frame_height, property_row, FillSlider};

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    let settings = &mut *cx.settings;
    property_row(ui, "Radiance", None, |ui| {
        ui.add(FillSlider::new(&mut settings.light_color_intensity.w, 0.0..=50.0).decimals(1))
    });
    property_row(ui, "Color", Some("Click the swatch for the picker"), |ui| {
        colour(ui, settings)
    });
    let tooltip = "Points from the sun into the scene; normalized before use";
    property_row(ui, "Direction", Some(tooltip), |ui| direction(ui, settings));
}

/// The colour as hex in a field, and the swatch that opens egui's picker.
fn colour(ui: &mut Ui, settings: &mut RendererSettings) -> egui::Response {
    let mut rgb = settings.light_color_intensity.truncate().to_array();
    let side = frame_height(ui);
    let field = vec2(
        (ui.available_width() - side - ui.spacing().item_spacing.x).max(0.0),
        side,
    );
    let (rect, _) = ui.allocate_exact_size(field, Sense::hover());
    ui.painter()
        .rect_filled(rect, theme::FRAME_RADIUS, frame_fill(ui));
    let hex = format!(
        "#{:02X}{:02X}{:02X}",
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2])
    );
    let text_pos = egui::pos2(rect.left() + theme::FRAME_PADDING.x, rect.center().y);
    let font = TextStyle::Body.resolve(ui.style());
    ui.painter()
        .text(text_pos, Align2::LEFT_CENTER, hex, font, pal::TEXT);
    ui.spacing_mut().interact_size = vec2(side, side);
    let response = egui::color_picker::color_edit_button_rgb(ui, &mut rgb);
    if response.changed() {
        let w = settings.light_color_intensity.w;
        settings.light_color_intensity = Vec3::from_array(rgb).extend(w);
    }
    response
}

/// ImGui's hex: the float scaled to a byte, no transfer curve.
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The renderer stores the way towards the light; the C++ field shows the way the light travels.
fn direction(ui: &mut Ui, settings: &mut RendererSettings) -> egui::Response {
    let towards = settings.light_dir_ambient.truncate();
    let mut from_sun = (-towards).to_array();
    let response = drag_axes(ui, &mut from_sun, 0.005, -1.0..=1.0, 2, "");
    if response.changed() {
        let travel = Vec3::from_array(from_sun);
        // A zero vector has no direction; the C++ engine falls back to straight down.
        let travel = if travel.length_squared() < 1e-12 {
            Vec3::NEG_Y
        } else {
            travel
        };
        settings.light_dir_ambient = (-travel).extend(settings.light_dir_ambient.w);
    }
    response
}
