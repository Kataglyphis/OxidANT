//! The C++ segmented control: equal segments in one frame, the selected one raised and underlined in mint.

use egui::{pos2, vec2, Align2, Rect, Response, Role, Sense, TextStyle, Ui, WidgetInfo};

use super::{frame_height, galley};
use crate::ui::palette as pal;
use crate::ui::theme;

/// A row of `labels`, any of which `enabled` may switch off.
#[derive(Clone, Copy, Debug)]
pub struct Segmented<'a> {
    labels: &'a [&'a str],
    enabled: &'a [bool],
    width: Option<f32>,
}

impl<'a> Segmented<'a> {
    /// Fills the available width, every segment enabled.
    pub fn new(labels: &'a [&'a str]) -> Self {
        Self {
            labels,
            enabled: &[],
            width: None,
        }
    }

    /// One flag per label; a missing flag counts as enabled.
    pub fn enabled(mut self, enabled: &'a [bool]) -> Self {
        self.enabled = enabled;
        self
    }

    /// A fixed width instead of the available one.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The width that fits the widest label in every segment, the C++ `width == 0`.
    pub fn fitted_width(&self, ui: &Ui) -> f32 {
        let font = TextStyle::Body.resolve(ui.style());
        let widest = self
            .labels
            .iter()
            .map(|label| galley(ui, *label, font.clone(), pal::TEXT).size().x)
            .fold(0.0, f32::max);
        (widest + theme::FRAME_PADDING.x * 2.0) * self.labels.len() as f32 + inset(ui) * 2.0
    }

    fn is_enabled(&self, index: usize) -> bool {
        self.enabled.get(index).copied().unwrap_or(true)
    }

    /// Shows the control; the response is changed when a click picked another segment.
    pub fn show(self, ui: &mut Ui, selected: &mut usize) -> Response {
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, frame_height(ui)), Sense::hover());
        ui.painter()
            .rect_filled(rect, theme::FRAME_RADIUS, super::frame_fill(ui));
        for index in 0..self.labels.len() {
            let segment = self.segment_rect(ui, rect, index);
            let clicked = self.segment(ui, response.id.with(index), segment, index, *selected);
            if clicked && index != *selected {
                *selected = index;
                response.mark_changed();
            }
        }
        response
    }

    fn segment_rect(&self, ui: &Ui, frame: Rect, index: usize) -> Rect {
        let inset = inset(ui);
        let width = (frame.width() - inset * 2.0) / self.labels.len().max(1) as f32;
        let left = frame.left() + inset + width * index as f32;
        Rect::from_min_max(
            pos2(left, frame.top() + inset),
            pos2(left + width, frame.bottom() - inset),
        )
    }

    /// One segment's interaction and paint; true on a click.
    fn segment(&self, ui: &Ui, id: egui::Id, rect: Rect, index: usize, selected: usize) -> bool {
        let enabled = self.is_enabled(index) && ui.is_enabled();
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, id, sense);
        let is_selected = index == selected;
        let label = self.labels[index];
        response
            .widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, is_selected, label));
        let painter = ui.painter();
        let radius = (f32::from(theme::FRAME_RADIUS) - inset(ui)).max(0.0);
        if is_selected || (enabled && response.hovered()) {
            let fill = if is_selected {
                pal::RAISED
            } else {
                pal::FRAME_HOVER
            };
            painter.rect_filled(rect, radius, fill);
        }
        if is_selected {
            underline(ui, rect);
        }
        let colour = match (is_selected, enabled) {
            (true, _) => pal::TEXT,
            (false, true) => pal::TEXT_MUTED,
            (false, false) => pal::TEXT_MUTED.gamma_multiply(ui.visuals().disabled_alpha()),
        };
        let font = TextStyle::Body.resolve(ui.style());
        painter.text(rect.center(), Align2::CENTER_CENTER, label, font, colour);
        enabled && response.clicked()
    }
}

/// The C++ `max(2, round(FramePadding.y / 2))` between the frame and its segments.
fn inset(_ui: &Ui) -> f32 {
    (theme::FRAME_PADDING.y * 0.5).round().max(2.0)
}

/// The selected mark: a 2-point mint bar along the bottom, inset 22 % from each side.
pub(super) fn underline(ui: &Ui, rect: Rect) {
    let thickness = 2.0;
    let margin = (rect.width() * 0.22).round();
    let bar = Rect::from_min_max(
        pos2(rect.left() + margin, rect.bottom() - thickness),
        pos2(rect.right() - margin, rect.bottom()),
    );
    ui.painter().rect_filled(bar, thickness * 0.5, pal::ACCENT);
}

/// [`Segmented`] over the available width with every segment enabled.
pub fn segmented(ui: &mut Ui, selected: &mut usize, labels: &[&str]) -> Response {
    Segmented::new(labels).show(ui, selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};

    const MODES: [&str; 3] = ["Raster", "Ray traced", "Path traced"];

    #[test]
    fn a_fitted_control_sizes_every_segment_for_the_widest_label() {
        let ctx = context();
        let (fitted, filled, available) = frame(&ctx, Vec::new(), |ui| {
            let fitted = Segmented::new(&MODES).fitted_width(ui);
            let available = ui.available_width();
            let mut selected = 0;
            let filled = segmented(ui, &mut selected, &MODES).rect.width();
            (fitted, filled, available)
        });
        // "Path traced" is about 66 points wide in the body face.
        assert!(
            (fitted - ((66.1 + 16.0) * 3.0 + 4.0)).abs() < 3.0,
            "{fitted}"
        );
        assert_eq!(filled, available);
    }

    #[test]
    fn a_click_picks_an_enabled_segment_and_never_a_disabled_one() {
        let ctx = context();
        let mut selected = 0;
        let enabled = [true, false, true];
        let show = |ui: &mut Ui, selected: &mut usize| {
            Segmented::new(&MODES)
                .enabled(&enabled)
                .width(300.0)
                .show(ui, selected)
        };
        let rect = frame(&ctx, Vec::new(), |ui| show(ui, &mut selected).rect);
        let show = |ui: &mut Ui, selected: &mut usize| show(ui, selected).changed();
        let third = rect.left() + rect.width() * 5.0 / 6.0;
        let second = rect.left() + rect.width() * 0.5;
        assert!(frame(&ctx, click(third, rect.center().y), |ui| show(
            ui,
            &mut selected
        )));
        assert_eq!(selected, 2);
        assert!(!frame(&ctx, click(second, rect.center().y), |ui| show(
            ui,
            &mut selected
        )));
        assert_eq!(selected, 2);
        assert!(!frame(&ctx, click(third, rect.center().y), |ui| show(
            ui,
            &mut selected
        )));
    }
}
