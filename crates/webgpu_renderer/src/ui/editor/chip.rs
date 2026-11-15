//! The viewport chip: a pill at the scene's top right naming what is drawn at what size, or the load in flight.

use egui::{pos2, Painter, Rect, Stroke, TextStyle, Ui};

use crate::app::loader::LoadProgress;
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{frame_height, galley, line_height};

/// What the WebGPU renderer draws; the C++ chip also names ray and path tracing.
pub(super) const MODE: &str = "Raster  ·  Forward";
/// One sweep of the bar while a load cannot count, in seconds.
const SWEEP_S: f64 = 1.5;

/// Painted on the root's background layer and never allocated, so the scene under it stays the camera's.
pub(super) fn paint(ui: &Ui, scene: Rect, loading: Option<&LoadProgress>) {
    let pixels = scene.size() * ui.ctx().pixels_per_point();
    let extent = format!("{} × {}", pixels.x.round() as u32, pixels.y.round() as u32);
    let font = TextStyle::Body.resolve(ui.style());
    let (text, dot_colour) = match loading {
        Some(progress) => (progress.text(), pal::WARNING),
        None => (MODE.to_owned(), pal::ACCENT),
    };
    let label = galley(ui, text, font.clone(), pal::TEXT);
    let size = galley(ui, &extent, font, pal::TEXT_MUTED);
    let dot = line_height(ui) * 0.5;
    let spacing = ui.spacing().item_spacing.x;
    let height = frame_height(ui);
    let width = theme::FRAME_PADDING.x * 2.0
        + dot
        + theme::INNER_SPACING.x
        + label.size().x
        + spacing * 1.5
        + size.size().x;
    let padding = theme::WINDOW_PADDING;
    let max = pos2(scene.right() - padding.x, scene.top() + padding.y + height);
    let chip = Rect::from_min_max(pos2(max.x - width, scene.top() + padding.y), max);
    if chip.left() < scene.left() + padding.x || chip.bottom() > scene.bottom() - padding.y {
        return;
    }
    let painter = ui.painter();
    let radius = height * 0.5;
    painter.rect_filled(chip, radius, pal::with_alpha(pal::PANEL, 0.86));
    painter.rect_stroke(
        chip,
        radius,
        Stroke::new(theme::hairline(ui), pal::BORDER),
        egui::StrokeKind::Inside,
    );
    let mut x = chip.left() + theme::FRAME_PADDING.x;
    painter.circle_filled(pos2(x + dot * 0.5, chip.center().y), dot * 0.36, dot_colour);
    x += dot + theme::INNER_SPACING.x;
    let text_top = chip.top() + (height - label.size().y) * 0.5;
    let label_width = label.size().x;
    painter.galley(pos2(x, text_top), label, pal::TEXT);
    x += label_width + spacing * 1.5;
    painter.galley(pos2(x, text_top), size, pal::TEXT_MUTED);
    if let Some(progress) = loading {
        let now = ui.input(|input| input.time);
        progress_bar(
            painter,
            chip.shrink2(egui::vec2(radius, 0.0)),
            progress.fraction,
            now,
        );
    }
}

/// A mint line along the chip's foot: filled to the fraction, or a sweeping quarter while nothing is counted.
fn progress_bar(painter: &Painter, span: Rect, fraction: Option<f32>, now_s: f64) {
    let thickness = 2.0;
    let (from, to) = match fraction {
        Some(fraction) => (0.0, fraction.clamp(0.0, 1.0)),
        None => {
            let phase = (now_s / SWEEP_S).fract() as f32;
            ((phase * 1.25 - 0.25).max(0.0), (phase * 1.25).min(1.0))
        }
    };
    let y = span.bottom() - thickness;
    let track = Rect::from_min_max(pos2(span.left(), y), pos2(span.right(), span.bottom()));
    painter.rect_filled(track, 0.0, pal::with_alpha(pal::ACCENT, 0.2));
    let fill = Rect::from_min_max(
        pos2(span.left() + span.width() * from, y),
        pos2(span.left() + span.width() * to, span.bottom()),
    );
    painter.rect_filled(fill, 0.0, pal::ACCENT);
}
