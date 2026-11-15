//! The Camera tab: the field of view, then the CONTROLS list from the one key table.

use egui::{Align, Layout, Ui};

use super::super::{EditorContext, KEY_BINDINGS};
use crate::ui::theme;
use crate::ui::widgets::{
    keycap, keycap_size, muted_wrapped, property_row, section_header, FillSlider,
};

/// The C++ table's cell padding on both sides of the column edge.
const COLUMN_GAP: f32 = 12.0;

pub(super) fn show(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    property_row(ui, "Field of view", Some("Vertical, in degrees"), |ui| {
        ui.add(
            FillSlider::new(&mut cx.camera.fov_y_deg, 20.0..=110.0)
                .decimals(0)
                .suffix("°"),
        )
    });
    section_header(ui, "CONTROLS", None);
    controls(ui, cx);
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
