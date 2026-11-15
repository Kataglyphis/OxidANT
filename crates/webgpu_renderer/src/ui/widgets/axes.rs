//! The C++ axis drags: three drag fields in one row, each with a palette-toned axis marker on its left edge.

use std::ops::RangeInclusive;

use egui::{pos2, vec2, CornerRadius, DragValue, Rect, Response, Ui};

use super::frame_height;
use crate::ui::palette as pal;
use crate::ui::theme;

/// ImGui's ColorMarkerSize.
const MARKER_WIDTH: f32 = 3.0;
const AXES: [egui::Color32; 3] = [pal::AXIS_X, pal::AXIS_Y, pal::AXIS_Z];

/// X, Y and Z of `values` as equal drag fields across the available width; `range` clamps every drag.
pub fn drag_axes(
    ui: &mut Ui,
    values: &mut [f32; 3],
    speed: f64,
    range: RangeInclusive<f32>,
    decimals: usize,
    suffix: &str,
) -> Response {
    axes_row(ui, values, &|_| speed, range, decimals, suffix)
}

/// [`drag_axes`] as ImGui's logarithmic drag: `step` per pixel crosses the log of `range` as it would the range.
pub fn drag_axes_logarithmic(
    ui: &mut Ui,
    values: &mut [f32; 3],
    step: f64,
    range: RangeInclusive<f32>,
    decimals: usize,
) -> Response {
    let rate = logarithmic_rate(step, &range);
    let speed = move |value: f32| f64::from(value.abs()) * rate;
    axes_row(ui, values, &speed, range, decimals, "")
}

/// The share of its value a logarithmic field moves per pixel; needs `0 < start < end`.
fn logarithmic_rate(step: f64, range: &RangeInclusive<f32>) -> f64 {
    let (lo, hi) = (f64::from(*range.start()), f64::from(*range.end()));
    (step / (hi - lo) * (hi / lo).ln()).exp_m1()
}

fn axes_row(
    ui: &mut Ui,
    values: &mut [f32; 3],
    speed: &dyn Fn(f32) -> f64,
    range: RangeInclusive<f32>,
    decimals: usize,
    suffix: &str,
) -> Response {
    let gap = theme::INNER_SPACING.x;
    let width = ((ui.available_width() - 2.0 * gap) / 3.0).floor().max(1.0);
    let size = vec2(width, frame_height(ui));
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let mut row: Option<Response> = None;
        for (axis, value) in values.iter_mut().enumerate() {
            let speed = speed(*value);
            let field = DragValue::new(value)
                .speed(speed)
                .range(range.clone())
                .fixed_decimals(decimals)
                .suffix(suffix.to_owned());
            let response = ui.add_sized(size, field);
            marker(ui, response.rect, AXES[axis]);
            row = Some(match row {
                Some(row) => row | response,
                None => response,
            });
        }
        row
    });
    inner.inner.unwrap_or(inner.response)
}

/// A strip in the field's rounding along its left edge, as ImGui's colour marker.
fn marker(ui: &Ui, field: Rect, colour: egui::Color32) {
    let strip = Rect::from_min_max(field.min, pos2(field.left() + MARKER_WIDTH, field.bottom()));
    let corners = CornerRadius {
        nw: theme::FRAME_RADIUS.min(2),
        sw: theme::FRAME_RADIUS.min(2),
        ne: 0,
        se: 0,
    };
    ui.painter().rect_filled(strip, corners, colour);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{button, context, frame, moved};
    use egui::PointerButton;

    #[test]
    fn three_equal_fields_fill_the_row() {
        let ctx = context();
        let mut direction = [-0.55, -1.0, -0.35];
        let (row, available) = frame(&ctx, Vec::new(), |ui| {
            let available = ui.available_width();
            (
                drag_axes(ui, &mut direction, 0.005, -1.0..=1.0, 2, "").rect,
                available,
            )
        });
        assert_eq!(row.height(), theme::FRAME_HEIGHT);
        assert!(
            (row.width() - available).abs() <= 3.0,
            "{row:?} of {available}"
        );
    }

    #[test]
    fn a_logarithmic_field_crosses_the_cpp_cloud_scale_range_in_400_pixels() {
        let rate = logarithmic_rate(2.5, &(0.001..=1000.0));
        let pixels = (1000.0_f64 / 0.001).ln() / rate.ln_1p();
        assert!((pixels - 400.0).abs() < 1.0, "{pixels} px");
    }

    #[test]
    fn a_drag_moves_one_axis_inside_its_range() {
        let ctx = context();
        let mut direction = [0.0_f32, 0.9, 0.0];
        let mut row = Rect::NOTHING;
        frame(&ctx, Vec::new(), |ui| {
            row = drag_axes(ui, &mut direction, 0.01, -1.0..=1.0, 2, "").rect
        });
        let y_field = pos2(row.center().x, row.center().y);
        let mut drag = |events| {
            frame(&ctx, events, |ui| {
                drag_axes(ui, &mut direction, 0.01, -1.0..=1.0, 2, "").changed()
            })
        };
        drag(vec![
            moved(y_field.x, y_field.y),
            button(y_field.x, y_field.y, PointerButton::Primary, true),
        ]);
        let changed = drag(vec![moved(y_field.x + 40.0, y_field.y)]);
        drag(vec![button(
            y_field.x + 40.0,
            y_field.y,
            PointerButton::Primary,
            false,
        )]);
        assert!(changed, "dragging the Y field must report a change");
        assert_eq!(direction[1], 1.0, "the drag clamps at the range's end");
        assert_eq!((direction[0], direction[2]), (0.0, 0.0));
    }
}
