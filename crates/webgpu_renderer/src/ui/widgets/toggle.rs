//! The C++ pill switch: a knob that eases along a track, mint when on.

use egui::{lerp, pos2, vec2, Color32, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo};

use super::frame_height;
use crate::ui::palette as pal;

/// The C++ knob ease, end to end.
const KNOB_SECONDS: f32 = 0.12;
/// The C++ off track under the pointer, a step lighter than RAISED_ACTIVE.
const OFF_HOVER: Color32 = Color32::from_rgb(60, 68, 82);

/// The track's width by the frame height: the C++ `round(round(frame * 0.72) * 1.8)`.
pub fn toggle_size(ui: &Ui) -> Vec2 {
    let height = frame_height(ui);
    vec2(((height * 0.72).round() * 1.8).round(), height)
}

/// A switch for `on`: a click flips it and marks the response changed.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(toggle_size(ui), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let (enabled, value) = (ui.is_enabled(), *on);
    response.widget_info(|| WidgetInfo::selected(Role::Switch, enabled, value, ""));
    if ui.is_rect_visible(rect) {
        let t = ui
            .ctx()
            .animate_bool_with_time(response.id, value, KNOB_SECONDS);
        paint(ui, rect, t, response.hovered());
    }
    response
}

/// The track lerps from raised to mint, the knob from muted to the panel, so on reads dark on mint.
fn paint(ui: &Ui, rect: Rect, t: f32, hovered: bool) {
    let track_height = (rect.height() * 0.72).round();
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), track_height));
    let (off, on) = if hovered {
        (OFF_HOVER, pal::ACCENT_HOVER)
    } else {
        (pal::RAISED_ACTIVE, pal::ACCENT)
    };
    let painter = ui.painter();
    painter.rect_filled(track, track_height * 0.5, off.lerp_to_gamma(on, t));
    let half = track_height * 0.5;
    let radius = half - (track_height * 0.12).round().max(2.0);
    let x = lerp(track.left() + half..=track.right() - half, t);
    let idle = if hovered { pal::TEXT } else { pal::TEXT_MUTED };
    painter.circle_filled(
        pos2(x, track.center().y),
        radius,
        idle.lerp_to_gamma(pal::PANEL, t),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};

    #[test]
    fn the_track_has_the_cpp_proportions() {
        let ctx = context();
        let size = frame(&ctx, Vec::new(), |ui| toggle_size(ui));
        // Frame height 23: a 17-point track, 31 wide.
        assert_eq!(size, vec2(31.0, 23.0));
    }

    #[test]
    fn a_click_flips_the_value_and_reports_a_change() {
        let ctx = context();
        let mut on = false;
        let mut rect = Rect::NOTHING;
        frame(&ctx, Vec::new(), |ui| rect = toggle(ui, &mut on).rect);
        let centre = rect.center();
        let changed = frame(&ctx, click(centre.x, centre.y), |ui| {
            toggle(ui, &mut on).changed()
        });
        assert!(changed && on);
        let disabled = frame(&ctx, click(centre.x, centre.y), |ui| {
            ui.add_enabled_ui(false, |ui| toggle(ui, &mut on).changed())
                .inner
        });
        assert!(!disabled && on, "a disabled switch must not flip");
    }
}
