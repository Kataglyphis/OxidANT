//! The Sun tab: radiance, colour and the direction from the sun into the scene, then the C++ SHADOWS section.

use egui::{vec2, Align2, Sense, TextStyle, Ui};
use glam::Vec3;

use super::super::{EditorContext, RendererSettings};
use crate::render::forward::CASCADE_COUNT;
use crate::render::shadow_settings::{
    nearest_resolution, ShadowSettings, MAX_PCF_RADIUS, SHADOW_DISTANCE_RANGE,
    SHADOW_MAP_RESOLUTIONS,
};
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{
    drag_axes, enabled_ui, frame_fill, frame_height, muted_wrapped, property_row, section_header,
    FillSlider, Segmented,
};

/// The C++ segment labels: `kShadowMapResolutions`, then the digits of the cascade and radius rows.
const RESOLUTION_LABELS: [&str; 4] = ["512", "1024", "2048", "4096"];
const DIGITS: [&str; 6] = ["0", "1", "2", "3", "4", "5"];

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
    shadows(ui, &mut settings.shadows);
    section_header(ui, "LIGHT SHADOWS", Some(&mut settings.shadows.punctual));
    muted_wrapped(
        ui,
        "A 512 px cube for each of the scene's first four point and spot lights, redrawn every frame.",
    );
}

/// The SHADOWS header's switch, then the grid it disables.
fn shadows(ui: &mut Ui, shadows: &mut ShadowSettings) {
    section_header(ui, "SHADOWS", Some(&mut shadows.enabled));
    enabled_ui(ui, shadows.enabled, |ui| {
        let tooltip = "Texels per side of each cascade's map";
        property_row(ui, "Resolution", Some(tooltip), |ui| {
            // The size the renderer draws; an unlisted value is written over only by a click.
            let drawn = nearest_resolution(shadows.resolution);
            let mut index = SHADOW_MAP_RESOLUTIONS
                .iter()
                .position(|texels| *texels == drawn)
                .unwrap_or(2);
            let response = Segmented::new(&RESOLUTION_LABELS).show(ui, &mut index);
            if response.changed() {
                shadows.resolution = SHADOW_MAP_RESOLUTIONS[index];
            }
            response
        });
        let tooltip = "Shadow maps along the view; more keep near shadows sharp";
        property_row(ui, "Cascades", Some(tooltip), |ui| {
            let mut index = (shadows.cascade_count as usize).clamp(1, CASCADE_COUNT) - 1;
            let response = Segmented::new(&DIGITS[1..=CASCADE_COUNT]).show(ui, &mut index);
            shadows.cascade_count = index as u32 + 1;
            response
        });
        let tooltip = "Filter taps on each side; 0 gives hard edges";
        property_row(ui, "PCF radius", Some(tooltip), |ui| {
            let mut index = shadows.pcf_radius.min(MAX_PCF_RADIUS) as usize;
            let labels = &DIGITS[..=MAX_PCF_RADIUS as usize];
            let response = Segmented::new(labels).show(ui, &mut index);
            shadows.pcf_radius = index as u32;
            response
        });
        filtering(ui, shadows);
    });
}

/// Strength, the logarithmic distance and the split lambda, the C++ sliders.
fn filtering(ui: &mut Ui, shadows: &mut ShadowSettings) {
    let tooltip = "How dark shadowed areas get: 0 leaves them lit, 1 makes them black";
    property_row(ui, "Strength", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut shadows.strength, 0.0..=1.0).decimals(2))
    });
    let tooltip = "How far from the camera the cascades reach";
    property_row(ui, "Distance", Some(tooltip), |ui| {
        let slider = FillSlider::new(&mut shadows.distance, SHADOW_DISTANCE_RANGE)
            .logarithmic(true)
            .decimals(0);
        ui.add(slider)
    });
    let tooltip = "0 splits the cascades evenly, 1 logarithmically";
    property_row(ui, "Split lambda", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut shadows.split_lambda, 0.0..=1.0).decimals(2))
    });
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
