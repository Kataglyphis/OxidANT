//! The C++ editor's custom widgets as egui painter widgets: each allocates its rect, senses it and paints the palette.

mod axes;
mod fill_slider;
mod list_row;
mod metrics;
mod property;
mod segmented;
mod tabs;
mod text;
mod toggle;

pub use axes::{drag_axes, drag_axes_logarithmic};
pub use fill_slider::FillSlider;
pub use list_row::{list_row, RowResult, RowSpec, RowTrailing};
pub use metrics::{keycap, keycap_size, metric_row, status_dot, MetricRow};
pub use property::{label_width, property_row};
pub use segmented::{segmented, Segmented};
pub use tabs::tab_bar;
pub use text::{caption, caption_line, ellipsized, muted, muted_wrapped, section_header};
pub use toggle::{toggle, toggle_size};

use egui::{Color32, FontId, Galley, InnerResponse, TextStyle, Ui};
use std::sync::Arc;

use crate::ui::palette as pal;

/// The C++ GetFrameHeight, from the live style so a style edit carries through.
pub fn frame_height(ui: &Ui) -> f32 {
    ui.spacing().interact_size.y
}

/// The C++ GetFontSize: one line of body text.
pub fn line_height(ui: &Ui) -> f32 {
    ui.text_style_height(&TextStyle::Body)
}

/// FRAME, or under the disabled fade a fill that composites back to FRAME over the panel, as the C++ `studioBeginDisabled`.
pub fn frame_fill(ui: &Ui) -> Color32 {
    if ui.is_enabled() {
        return pal::FRAME;
    }
    let alpha = ui.visuals().disabled_alpha().max(0.01);
    let [panel, frame] = [pal::PANEL.to_array(), pal::FRAME.to_array()];
    let mix = |i: usize| {
        let (p, f) = (f32::from(panel[i]), f32::from(frame[i]));
        (p + (f - p) / alpha).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(0), mix(1), mix(2))
}

/// `add_enabled_ui` whose disabled fields and buttons keep their frames instead of fading into the panel.
pub fn enabled_ui<R>(
    ui: &mut Ui,
    enabled: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    ui.add_enabled_ui(enabled, |ui| {
        let fill = frame_fill(ui);
        let inactive = &mut ui.visuals_mut().widgets.inactive;
        inactive.bg_fill = fill;
        inactive.weak_bg_fill = fill;
        add(ui)
    })
}

/// One line of `text`, laid out but not placed.
pub fn galley(ui: &Ui, text: impl Into<String>, font: FontId, color: Color32) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.into(), font, color)
}

#[cfg(test)]
pub(crate) mod test_support {
    use egui::{pos2, vec2, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    /// A studio-styled context that, like the app's, runs one pass per frame.
    pub(crate) fn context() -> egui::Context {
        let ctx = crate::render::overlay::studio_context();
        ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Panic);
        ctx
    }

    /// One frame of a `width` x `height`-point screen at `ppp` pixels per point.
    pub(crate) fn input(size: (f32, f32), ppp: f32, events: Vec<Event>) -> RawInput {
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        input
    }

    /// Runs `ui` for one 800 x 600 frame and drops its paint output.
    pub(crate) fn frame<R>(
        ctx: &egui::Context,
        events: Vec<Event>,
        ui: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let mut ui = Some(ui);
        let mut result = None;
        let output = ctx.run_ui(input((800.0, 600.0), 1.0, events), |root| {
            if let Some(ui) = ui.take() {
                result = Some(ui(root));
            }
        });
        output.drop_without_applying_deltas();
        result.expect("a pass runs its ui once")
    }

    pub(crate) fn moved(x: f32, y: f32) -> Event {
        Event::PointerMoved(pos2(x, y))
    }

    pub(crate) fn button(x: f32, y: f32, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos: pos2(x, y),
            button,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    /// A left click at a point: move there, press, release.
    pub(crate) fn click(x: f32, y: f32) -> Vec<Event> {
        vec![
            moved(x, y),
            button(x, y, PointerButton::Primary, true),
            button(x, y, PointerButton::Primary, false),
        ]
    }
}
