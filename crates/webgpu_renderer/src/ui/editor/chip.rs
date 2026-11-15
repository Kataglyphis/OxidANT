//! The viewport chip: a pill at the scene's top right naming what is drawn and at what size.

use egui::{pos2, Rect, Stroke, TextStyle, Ui};

use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{frame_height, galley, line_height};

/// What the WebGPU renderer draws; the C++ chip also names ray and path tracing.
pub(super) const MODE: &str = "Raster  ·  Forward";

/// Painted on the root's background layer and never allocated, so the scene under it stays the camera's.
pub(super) fn paint(ui: &Ui, scene: Rect) {
    let pixels = scene.size() * ui.ctx().pixels_per_point();
    let extent = format!("{} × {}", pixels.x.round() as u32, pixels.y.round() as u32);
    let font = TextStyle::Body.resolve(ui.style());
    let label = galley(ui, MODE, font.clone(), pal::TEXT);
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
    painter.circle_filled(
        pos2(x + dot * 0.5, chip.center().y),
        dot * 0.36,
        pal::ACCENT,
    );
    x += dot + theme::INNER_SPACING.x;
    let text_top = chip.top() + (height - label.size().y) * 0.5;
    let label_width = label.size().x;
    painter.galley(pos2(x, text_top), label, pal::TEXT);
    x += label_width + spacing * 1.5;
    painter.galley(pos2(x, text_top), size, pal::TEXT_MUTED);
}
