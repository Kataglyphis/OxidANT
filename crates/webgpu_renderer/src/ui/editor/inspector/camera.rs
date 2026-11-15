//! The Camera tab: the lens, the fly speed and the frame button, then the CONTROLS list from the one key table.

use std::ops::RangeInclusive;

use egui::{vec2, Align, Button, Layout, Ui};

use super::super::{EditorAction, EditorContext, KEY_BINDINGS};
use crate::scene::camera::OrbitCamera;
use crate::scene::fly::SPEED_RANGE;
use crate::ui::theme;
use crate::ui::widgets::{
    frame_height, keycap, keycap_size, muted_wrapped, property_row, section_header, FillSlider,
};

/// The C++ table's cell padding on both sides of the column edge.
const COLUMN_GAP: f32 = 12.0;
/// Clip planes on log scales wide enough for a 1 m cube and an 8 km scene.
const NEAR_RANGE: RangeInclusive<f32> = 0.001..=1000.0;
const FAR_RANGE: RangeInclusive<f32> = 1.0..=1_000_000.0;
/// Far stays this far beyond near, so the depth range can never collapse or invert.
const MIN_DEPTH_RATIO: f32 = 2.0;

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>, actions: &mut Vec<EditorAction>) {
    lens(ui, cx.camera);
    let decimals = decimals_for(*cx.fly_speed);
    let tooltip = "World units per second for W A S D";
    property_row(ui, "Fly speed", Some(tooltip), |ui| {
        let slider = FillSlider::new(&mut *cx.fly_speed, SPEED_RANGE)
            .logarithmic(true)
            .decimals(decimals)
            .suffix(" /s");
        ui.add(slider)
    });
    let size = vec2(ui.available_width(), frame_height(ui));
    let frame = ui
        .add_sized(size, Button::new("Frame the scene"))
        .on_hover_text("Back to the scene's view, the fly speed fitted to its size");
    if frame.clicked() {
        actions.push(EditorAction::FrameScene);
    }
    section_header(ui, "CONTROLS", None);
    controls(ui, cx);
}

/// The field of view as in C++, then the clip planes the C++ editor does not show.
fn lens(ui: &mut Ui, camera: &mut OrbitCamera) {
    property_row(ui, "Field of view", Some("Vertical, in degrees"), |ui| {
        ui.add(
            FillSlider::new(&mut camera.fov_y_deg, 20.0..=110.0)
                .decimals(0)
                .suffix("°"),
        )
    });
    let near = camera.near;
    property_row(
        ui,
        "Near plane",
        Some("Closer than this is clipped"),
        |ui| {
            let slider = FillSlider::new(&mut camera.near, NEAR_RANGE)
                .logarithmic(true)
                .decimals(decimals_for(near));
            ui.add(slider)
        },
    );
    let far = camera.far;
    property_row(
        ui,
        "Far plane",
        Some("Farther than this is clipped"),
        |ui| {
            let slider = FillSlider::new(&mut camera.far, FAR_RANGE)
                .logarithmic(true)
                .decimals(decimals_for(far));
            ui.add(slider)
        },
    );
    camera.far = camera.far.max(camera.near * MIN_DEPTH_RATIO);
}

/// Three significant digits, so a small value on a log scale never reads as zero.
fn decimals_for(value: f32) -> usize {
    match value.abs() {
        v if v >= 100.0 => 0,
        v if v >= 10.0 => 1,
        v if v >= 1.0 => 2,
        v if v >= 0.1 => 3,
        _ => 4,
    }
}

/// Keycaps in a column as wide as the widest row, the action wrapped beside them, as the C++ table.
fn controls(ui: &mut Ui, cx: &EditorContext<'_>) {
    let gap = (theme::INNER_SPACING.x * 0.75).round().max(2.0);
    let rows: Vec<_> = KEY_BINDINGS
        .iter()
        .filter_map(|binding| Some((binding.keys, binding.action_on(cx.caps)?)))
        .collect();
    let widths: Vec<f32> = rows
        .iter()
        .map(|(keys, _)| {
            keys.iter().map(|key| keycap_size(ui, key).x).sum::<f32>()
                + gap * keys.len().saturating_sub(1) as f32
        })
        .collect();
    let column = widths.iter().copied().fold(0.0, f32::max);
    for ((keys, action), width) in rows.into_iter().zip(widths) {
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for key in keys {
                keycap(ui, key);
            }
            ui.add_space(column - width + COLUMN_GAP - gap);
            ui.with_layout(Layout::top_down(Align::Min), |ui| muted_wrapped(ui, action));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_keep_three_significant_digits() {
        let shown = |value: f32| format!("{value:.*}", decimals_for(value));
        assert_eq!(shown(0.001), "0.0010");
        assert_eq!(shown(0.1), "0.100");
        assert_eq!(shown(0.433), "0.433");
        assert_eq!(shown(3.54), "3.54");
        assert_eq!(shown(12.5), "12.5");
        assert_eq!(shown(2829.4), "2829");
    }
}
