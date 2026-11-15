//! The C++ property grid row: a muted label carrying the tooltip, then the control across the value column.

use egui::{pos2, vec2, Align, Layout, Rect, Sense, TextStyle, Ui, UiBuilder};

use super::{ellipsized, frame_height, line_height};
use crate::ui::palette as pal;

/// The C++ CellPadding.x, on both sides of the column edge.
const CELL_PADDING_X: f32 = 6.0;
/// Half the C++ row's 8 points of cell padding; item spacing makes up the rest, so the pitch stays 31.
const ROW_PADDING_Y: f32 = 1.0;

/// The label column: 38 % of the row, held between 5.5 and 9 lines of text, as in the C++.
pub fn label_width(ui: &Ui) -> f32 {
    let font = line_height(ui);
    (ui.available_width() * 0.38)
        .clamp(font * 5.5, font * 9.0)
        .round()
}

/// One row: `label` (with `tooltip` on hover) and whatever `add` puts in the value column.
pub fn property_row<R>(
    ui: &mut Ui,
    label: &str,
    tooltip: Option<&str>,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let label_width = label_width(ui);
    let height = frame_height(ui);
    let size = vec2(ui.available_width(), height + 2.0 * ROW_PADDING_Y);
    let (row, _) = ui.allocate_exact_size(size, Sense::hover());
    let row = row.shrink2(vec2(0.0, ROW_PADDING_Y));
    let label_rect = Rect::from_min_size(row.min, vec2(label_width, height));
    let label_response = ui.interact(
        label_rect,
        ui.scope_id().with(("property", label)),
        Sense::hover(),
    );
    let font = TextStyle::Body.resolve(ui.style());
    let text_top = row.top() + ((height - line_height(ui)) * 0.5).round();
    ellipsized(
        ui.painter(),
        pos2(row.left(), text_top),
        label_rect.right(),
        label,
        font,
        pal::TEXT_MUTED,
    );
    if let Some(tooltip) = tooltip {
        label_response.on_hover_text(tooltip);
    }
    let value = Rect::from_min_max(
        pos2(row.left() + label_width + 2.0 * CELL_PADDING_X, row.top()),
        row.max,
    );
    let mut cell = ui.new_child(
        UiBuilder::new()
            .max_rect(value)
            .layout(Layout::left_to_right(Align::Center))
            .id_salt(("value", label)),
    );
    add(&mut cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{context, frame};
    use crate::ui::widgets::FillSlider;

    #[test]
    fn the_label_column_follows_the_cpp_clamp() {
        let ctx = context();
        let widths = frame(&ctx, Vec::new(), |ui| {
            [200.0, 312.0, 600.0].map(|width| {
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_size(ui.cursor().min, vec2(width, 50.0))),
                    |ui| label_width(ui),
                )
                .inner
            })
        });
        // 5.5 and 9 lines of 15 points bound it; the 336-point inspector's 312-point body takes 38 %.
        assert_eq!(widths, [83.0, 119.0, 135.0]);
    }

    #[test]
    fn rows_keep_the_cpp_31_point_pitch_and_the_control_fills_the_value_column() {
        let ctx = context();
        let mut value = 0.5;
        let (pitch, control, row, label_w) = frame(&ctx, Vec::new(), |ui| {
            let label_w = label_width(ui);
            let first = ui.cursor().top();
            let control = property_row(ui, "Strength", Some("tip"), |ui| {
                ui.add(FillSlider::new(&mut value, 0.0..=1.0))
            });
            property_row(ui, "Split lambda", None, |_| ());
            let pitch = (ui.cursor().top() - first) / 2.0;
            (pitch, control.rect, ui.max_rect().x_range(), label_w)
        });
        assert_eq!(pitch, 31.0);
        assert_eq!(control.height(), crate::ui::theme::FRAME_HEIGHT);
        assert_eq!(control.right(), row.max);
        assert_eq!(control.left(), row.min + label_w + 2.0 * CELL_PADDING_X);
    }
}
