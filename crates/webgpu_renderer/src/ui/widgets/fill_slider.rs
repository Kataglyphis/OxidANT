//! The C++ fill slider: a frame filled up to the value, so no grab ever covers the digits.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{
    vec2, Align2, CornerRadius, Rect, Response, Role, Sense, TextStyle, Ui, Widget, WidgetInfo,
};

use super::frame_height;
use crate::ui::palette as pal;
use crate::ui::theme;

/// ImGui's fixed padding before an int slider's one-unit grab.
const GRAB_PADDING: f64 = 2.0;

/// A slider over `range` whose value shows as a mint fill and centred digits.
pub struct FillSlider<'a> {
    get_set: Box<dyn 'a + FnMut(Option<f64>) -> f64>,
    range: RangeInclusive<f64>,
    integral: bool,
    logarithmic: bool,
    decimals: usize,
    suffix: &'a str,
}

impl<'a> FillSlider<'a> {
    /// Floats show three decimals and integers none, the C++ `%.3f` and `%d`.
    pub fn new<N: Numeric>(value: &'a mut N, range: RangeInclusive<N>) -> Self {
        let range = range.start().to_f64()..=range.end().to_f64();
        Self {
            get_set: Box::new(move |set: Option<f64>| {
                if let Some(set) = set {
                    *value = N::from_f64(set);
                }
                value.to_f64()
            }),
            range,
            integral: N::INTEGRAL,
            logarithmic: false,
            decimals: if N::INTEGRAL { 0 } else { 3 },
            suffix: "",
        }
    }

    /// Fills by `log(v / lo) / log(hi / lo)`; needs `0 < lo < hi`.
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.logarithmic = logarithmic;
        self
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// Written right after the digits, such as `°`.
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    fn text(&self, value: f64) -> String {
        format!("{value:.*}{}", self.decimals, self.suffix)
    }
}

/// Where `value` sits in `range`, 0 to 1.
pub(crate) fn value_to_t(value: f64, range: &RangeInclusive<f64>, logarithmic: bool) -> f64 {
    let (lo, hi) = (*range.start(), *range.end());
    if hi == lo {
        return 0.0;
    }
    let value = value.clamp(lo.min(hi), lo.max(hi));
    let t = if logarithmic {
        (value / lo).ln() / (hi / lo).ln()
    } else {
        (value - lo) / (hi - lo)
    };
    t.clamp(0.0, 1.0)
}

/// The value at `t` in `range`, whole when `integral`.
pub(crate) fn t_to_value(
    t: f64,
    range: &RangeInclusive<f64>,
    logarithmic: bool,
    integral: bool,
) -> f64 {
    let (lo, hi) = (*range.start(), *range.end());
    let t = t.clamp(0.0, 1.0);
    let value = if logarithmic {
        lo * (hi / lo).powf(t)
    } else {
        lo + (hi - lo) * t
    };
    if integral {
        value.round()
    } else {
        value
    }
}

/// An int slider's minimum shows one unit filled, as ImGui maps its mouse after a one-unit grab.
fn lead(width: f64, range: &RangeInclusive<f64>, integral: bool) -> f64 {
    if !integral {
        return 0.0;
    }
    GRAB_PADDING + (width - 2.0 * GRAB_PADDING) / ((range.end() - range.start()).abs() + 1.0)
}

impl Widget for FillSlider<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let size = vec2(ui.available_width(), frame_height(ui));
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let lead = lead(f64::from(rect.width()), &self.range, self.integral);
        let pressed = response.is_pointer_button_down_on() || response.clicked();
        if let (true, Some(pointer)) = (pressed, response.interact_pointer_pos()) {
            let along = f64::from(pointer.x - rect.left()) - lead * 0.5;
            let t = along / (f64::from(rect.width()) - lead).max(1.0);
            let old = (self.get_set)(None);
            let new = t_to_value(t, &self.range, self.logarithmic, self.integral);
            if new != old {
                (self.get_set)(Some(new));
                response.mark_changed();
            }
        }
        let value = (self.get_set)(None);
        let text = self.text(value);
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Slider, enabled, &text));
        let t = value_to_t(value, &self.range, self.logarithmic);
        let active = response.is_pointer_button_down_on();
        paint(
            ui,
            rect,
            (lead as f32, t as f32),
            active,
            response.hovered(),
            &text,
        );
        response
    }
}

/// The frame, the C++ hover tint, the fill up to `t` past the lead, then the digits.
fn paint(ui: &Ui, rect: Rect, (lead, t): (f32, f32), active: bool, hovered: bool, text: &str) {
    let painter = ui.painter();
    let radius = f32::from(theme::FRAME_RADIUS);
    painter.rect_filled(rect, radius, super::frame_fill(ui));
    if hovered || active {
        painter.rect_filled(rect, radius, pal::with_alpha(pal::TEXT, 0.04));
    }
    let right = egui::lerp(rect.left() + lead..=rect.right(), t);
    if right > rect.left() {
        let fill = Rect::from_min_max(rect.min, egui::pos2(right, rect.bottom()));
        let end = if right >= rect.right() - radius {
            radius
        } else {
            0.0
        };
        let corners = CornerRadius {
            nw: radius as u8,
            sw: radius as u8,
            ne: end as u8,
            se: end as u8,
        };
        let alpha = if active { 0.32 } else { 0.2 };
        painter.rect_filled(fill, corners, pal::with_alpha(pal::ACCENT, alpha));
    }
    let font = TextStyle::Body.resolve(ui.style());
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, pal::TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};

    #[test]
    fn linear_and_logarithmic_mappings_invert_each_other() {
        let linear = 0.0..=50.0;
        assert_eq!(value_to_t(10.0, &linear, false), 0.2);
        assert_eq!(t_to_value(0.2, &linear, false, false), 10.0);
        let log = 5.0..=500.0;
        assert!((value_to_t(50.0, &log, true) - 0.5).abs() < 1e-12);
        assert!((t_to_value(0.5, &log, true, false) - 50.0).abs() < 1e-9);
        assert_eq!(value_to_t(9_999.0, &log, true), 1.0, "out of range clamps");
        assert_eq!(t_to_value(0.49, &(1.0..=64.0), false, true), 32.0);
        assert_eq!(value_to_t(1.0, &(1.0..=1.0), false), 0.0);
    }

    #[test]
    fn an_int_slider_shows_one_unit_filled_at_its_minimum() {
        assert_eq!(lead(100.0, &(0.0..=1.0), false), 0.0);
        // Two units over 96 usable points: 48 each, after the 2-point pad.
        assert_eq!(lead(100.0, &(0.0..=1.0), true), 50.0);
    }

    #[test]
    fn a_click_sets_the_value_under_the_pointer_and_formats_it() {
        let ctx = context();
        let mut radiance = 10.0_f32;
        let mut rect = Rect::NOTHING;
        frame(&ctx, Vec::new(), |ui| {
            rect = ui
                .add(FillSlider::new(&mut radiance, 0.0..=50.0).decimals(1))
                .rect;
        });
        assert_eq!(rect.height(), theme::FRAME_HEIGHT);
        let x = rect.left() + rect.width() * 0.75;
        let changed = frame(&ctx, click(x, rect.center().y), |ui| {
            ui.add(FillSlider::new(&mut radiance, 0.0..=50.0)).changed()
        });
        assert!(changed);
        assert!((radiance - 37.5).abs() < 0.1, "{radiance}");
        let fov = FillSlider::new(&mut radiance, 20.0..=110.0)
            .decimals(0)
            .suffix("°");
        assert_eq!(fov.text(45.4), "45°");
    }
}
