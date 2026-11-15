//! The C++ readouts: the status dot, keycaps and the metric row of name, bar and value.

use egui::{pos2, vec2, Align2, Color32, Rect, Response, Sense, TextStyle, Ui};

use super::{galley, line_height};
use crate::ui::fonts;
use crate::ui::palette as pal;

/// A dot that sits on the text line like a glyph, 0.6 lines wide.
pub fn status_dot(ui: &mut Ui, colour: Color32) -> Response {
    let line = line_height(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(line * 0.6, line), Sense::hover());
    ui.painter().circle_filled(
        pos2(rect.left() + line * 0.3, rect.center().y),
        line * 0.22,
        colour,
    );
    response
}

/// A keycap's size: a line and a margin tall, at least square, wide enough for its label.
pub fn keycap_size(ui: &Ui, key: &str) -> egui::Vec2 {
    let line = line_height(ui);
    let height = line + 2.0 * (line * 0.14).round().max(1.0);
    let text = galley(ui, key, fonts::caption_font(), pal::TEXT);
    vec2(
        height.max(text.size().x + 2.0 * (height * 0.3).round()),
        height,
    )
}

/// A key as a small raised cap with a darker lip, caption-size text on its face.
pub fn keycap(ui: &mut Ui, key: &str) -> Response {
    let size = keycap_size(ui, key);
    let (width, height) = (size.x, size.y);
    let lip = (height * 0.06).round().max(1.0);
    let text = galley(ui, key, fonts::caption_font(), pal::TEXT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let radius = (height * 0.22).round();
    let painter = ui.painter();
    painter.rect_filled(rect, radius, pal::BASE);
    let face = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - lip));
    painter.rect_filled(face, radius, pal::RAISED);
    let text_pos = pos2(
        (rect.left() + (width - text.size().x) * 0.5).round(),
        (rect.top() + (height - lip - text.size().y) * 0.5).round(),
    );
    painter.galley(text_pos, text, pal::TEXT);
    response
}

/// One row of a metric table; `columns` are the name and value widths every row shares.
#[derive(Clone, Copy, Debug)]
pub struct MetricRow<'a> {
    pub name: &'a str,
    /// The bar's fill, clamped to 0..1.
    pub fraction: f32,
    pub value: &'a str,
    /// Off: disabled text and no fill, as for a pass without a sample.
    pub active: bool,
}

/// The name, a pill bar across the middle and the value right-aligned, on one line.
pub fn metric_row(ui: &mut Ui, columns: (f32, f32), row: &MetricRow<'_>) -> Response {
    let line = line_height(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), line), Sense::hover());
    let gap = ui.spacing().item_spacing.x;
    let colour = if row.active {
        pal::TEXT
    } else {
        pal::TEXT_DISABLED
    };
    let font = TextStyle::Body.resolve(ui.style());
    let painter = ui.painter();
    painter.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        row.name,
        font.clone(),
        colour,
    );
    painter.text(
        rect.right_center(),
        Align2::RIGHT_CENTER,
        row.value,
        font,
        colour,
    );
    let left = rect.left() + columns.0 + gap;
    let right = (rect.right() - columns.1 - gap).max(left);
    let bar_height = (line * 0.32).round().max(3.0);
    let top = rect.center().y - bar_height * 0.5;
    let track = Rect::from_min_max(pos2(left, top), pos2(right, top + bar_height));
    painter.rect_filled(track, bar_height * 0.5, pal::FRAME);
    let fraction = row.fraction.clamp(0.0, 1.0);
    if row.active && fraction > 0.0 {
        let filled = (track.width() * fraction)
            .max(bar_height)
            .min(track.width());
        let fill = Rect::from_min_size(track.min, vec2(filled, bar_height));
        painter.rect_filled(fill, bar_height * 0.5, pal::ACCENT);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{context, frame};

    #[test]
    fn a_keycap_is_at_least_square_and_grows_with_its_label() {
        let ctx = context();
        let (w, right_mouse) = frame(&ctx, Vec::new(), |ui| {
            (
                keycap(ui, "W").rect.size(),
                keycap(ui, "Right mouse").rect.size(),
            )
        });
        // A 15-point line: 19 tall with a 2-point margin above and below; W and its 6-point sides are wider.
        assert_eq!(w.y, 19.0);
        assert!((21.0..22.0).contains(&w.x), "{w:?}");
        assert_eq!(right_mouse.y, 19.0);
        assert!(right_mouse.x > 60.0, "{right_mouse:?}");
    }

    #[test]
    fn the_status_dot_and_a_metric_row_take_one_line() {
        let ctx = context();
        let (dot, metric) = frame(&ctx, Vec::new(), |ui| {
            let dot = status_dot(ui, pal::ACCENT).rect.size();
            let row = MetricRow {
                name: "Forward + sky",
                fraction: 2.0,
                value: "1.25 ms",
                active: true,
            };
            (dot, metric_row(ui, (90.0, 70.0), &row).rect.height())
        });
        assert_eq!(dot, vec2(9.0, 15.0));
        assert_eq!(metric, 15.0);
    }
}
