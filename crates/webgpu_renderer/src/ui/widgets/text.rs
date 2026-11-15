//! The C++ editor's text pieces: tracked caps captions, caption lines with a rule, muted labels, ellipsized runs.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    pos2, vec2, Color32, FontId, Label, Painter, Pos2, Rect, Response, RichText, Sense, Ui,
    UiBuilder,
};

use super::{frame_height, toggle, toggle_size};
use crate::ui::palette as pal;
use crate::ui::{fonts, theme};

/// The C++ `max(1, round(12 * 0.08))`, plus the half pixel ImGui adds on average by rounding each glyph's advance up.
const CAPTION_TRACKING: f32 = 1.5;

/// Tracked caps at caption size, centred in `line_height` (or its own height, if taller).
pub fn caption(ui: &mut Ui, text: &str, line_height: f32) -> Response {
    let format = TextFormat {
        font_id: fonts::caption_font(),
        color: pal::TEXT_MUTED,
        extra_letter_spacing: CAPTION_TRACKING,
        ..Default::default()
    };
    let job = LayoutJob::single_section(text.to_uppercase(), format);
    let galley = ui.painter().layout_job(job);
    let height = line_height.max(galley.size().y);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x, height), Sense::hover());
    let top = rect.top() + ((height - galley.size().y) * 0.5).round();
    ui.painter()
        .galley(pos2(rect.left(), top), galley, pal::TEXT_MUTED);
    response
}

/// A caption on a frame-height line with a rule under it; with `switch`, a toggle at its right end.
pub fn caption_line(ui: &mut Ui, text: &str, switch: Option<&mut bool>) -> Option<Response> {
    let height = frame_height(ui);
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let mut label = ui.new_child(UiBuilder::new().max_rect(line).id_salt(("caption", text)));
    caption(&mut label, text, height);
    let toggled = switch.map(|value| {
        let size = toggle_size(ui);
        let rect = Rect::from_min_size(pos2(line.right() - size.x, line.top()), size);
        let mut slot = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("switch", text)));
        toggle(&mut slot, value)
    });
    let rule = theme::hairline(ui);
    let bottom = Rect::from_min_max(pos2(line.left(), line.bottom() - rule), line.max);
    ui.painter().rect_filled(bottom, 0.0, pal::BORDER);
    toggled
}

/// [`caption_line`] after half a line of extra space, the C++ `sectionHeader`.
pub fn section_header(ui: &mut Ui, text: &str, switch: Option<&mut bool>) -> Option<Response> {
    // ImGui spaces the Dummy above the line as an item of its own.
    ui.add_space(ui.spacing().item_spacing.y * 1.5);
    caption_line(ui, text, switch)
}

/// One line of muted text.
pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(Label::new(RichText::new(text).color(pal::TEXT_MUTED)).extend())
}

/// Muted text wrapped to the available width.
pub fn muted_wrapped(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(Label::new(RichText::new(text).color(pal::TEXT_MUTED)).wrap())
}

/// Paints `text` from `pos`, cut with an ellipsis at `max_x`; returns the painted width.
pub fn ellipsized(
    painter: &Painter,
    pos: Pos2,
    max_x: f32,
    text: &str,
    font: FontId,
    colour: Color32,
) -> f32 {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, colour);
    job.wrap = TextWrapping::truncate_at_width((max_x - pos.x).max(0.0));
    let galley = painter.layout_job(job);
    let width = galley.size().x;
    painter.galley(pos, galley, colour);
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{context, frame};

    #[test]
    fn a_caption_is_uppercase_tracked_and_centred_in_its_line() {
        let ctx = context();
        let (plain, tracked) = frame(&ctx, Vec::new(), |ui| {
            let plain = super::super::galley(ui, "LIBRARY", fonts::caption_font(), pal::TEXT);
            let response = caption(ui, "library", 23.0);
            (plain.size().x, response.rect.size())
        });
        assert_eq!(tracked.y, 23.0);
        // Seven letters, six gaps of tracking between them.
        assert!(
            (tracked.x - (plain + 6.0 * CAPTION_TRACKING)).abs() < 0.5,
            "{tracked:?} vs {plain}"
        );
    }

    #[test]
    fn a_caption_line_is_a_frame_tall_and_its_switch_sits_at_the_right_end() {
        let ctx = context();
        let mut on = false;
        let (line, switch) = frame(&ctx, Vec::new(), |ui| {
            let top = ui.cursor().top();
            let switch = caption_line(ui, "SHADOWS", Some(&mut on)).map(|r| r.rect);
            let right = ui.max_rect().right();
            ((ui.cursor().top() - top, right), switch)
        });
        let switch = switch.expect("a switch was asked for");
        assert!(line.0 >= theme::FRAME_HEIGHT, "{line:?}");
        assert_eq!(switch.right(), line.1);
        assert_eq!(switch.height(), theme::FRAME_HEIGHT);
    }

    #[test]
    fn ellipsizing_cuts_a_long_name_to_the_room_left() {
        let ctx = context();
        let (long, short) = frame(&ctx, Vec::new(), |ui| {
            let font = fonts::body_font();
            let painter = ui.painter();
            let long = ellipsized(
                painter,
                Pos2::ZERO,
                60.0,
                "Dinosaurs/dinosaurs",
                font.clone(),
                pal::TEXT,
            );
            let short = ellipsized(painter, Pos2::ZERO, 600.0, "Sun", font, pal::TEXT);
            (long, short)
        });
        assert!(long <= 60.0 && long > 40.0, "{long}");
        assert!(short > 15.0 && short < 30.0, "{short}");
    }
}
