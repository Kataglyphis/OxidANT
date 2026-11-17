//! The C++ status bar, device left and frame size right, plus a load or notice and the scene's credit between them.

use egui::{pos2, Align2, Color32, FontId, Margin, Rect, Sense, TextStyle, Ui};

use super::docks::{self, Side};
use super::EditorContext;
use crate::app::Notice;
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{ellipsized, frame_height, galley, muted, status_dot};

/// The build the C++ status bar and About name.
pub(super) const BUILD: &str = if cfg!(debug_assertions) {
    "Debug"
} else {
    "Release"
};

/// `Vulkan  ·  llvmpipe (...)`, `WebGPU  ·  intel gen-12lp`: the API and the adapter.
pub(super) fn device_text(cx: &EditorContext<'_>) -> String {
    format!("{}  ·  {}", cx.device.api, cx.device.name)
}

/// `1600 × 900  ·  Release  ·  v3.0.0  ·  F1 hides the editor`.
pub(super) fn frame_text(cx: &EditorContext<'_>) -> String {
    frame_text_of(cx.surface_px)
}

pub(super) fn frame_text_of([width, height]: [u32; 2]) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!("{width} × {height}  ·  {BUILD}  ·  v{version}  ·  F1 hides the editor")
}

/// The load in flight, else the notice, in its colour.
pub(super) fn notice_text(cx: &EditorContext<'_>) -> Option<(String, Color32)> {
    if let Some(progress) = cx.scene.loading {
        return Some((progress.text(), pal::TEXT_MUTED));
    }
    match cx.scene.notice? {
        Notice::Hint(text) => Some((text.clone(), pal::WARNING)),
        Notice::Error(text) => Some((text.clone(), pal::DANGER)),
    }
}

pub(super) fn show(ui: &mut Ui, cx: &EditorContext<'_>) {
    let margin = Margin::symmetric(theme::WINDOW_PADDING.x as i8, 0);
    docks::panel(
        ui,
        "studio_status",
        Side::Bottom,
        frame_height(ui),
        margin,
        |ui| {
            let bar = ui.max_rect();
            let font = TextStyle::Body.resolve(ui.style());
            let gap = ui.spacing().item_spacing.x * 2.0;
            let width = |text: &str| galley(ui, text, font.clone(), pal::TEXT_MUTED).size().x;
            let credit_width = cx.scene.credit.map_or(0.0, |credit| width(credit) + gap);
            // The device yields its name before the credit, which CC BY asks for, is cut.
            let short = credit_width + width(&device_text(cx)) + frame_height(ui) > bar.width();
            let left = device(ui, cx, short) + ui.spacing().item_spacing.x;
            let mut right = bar.right();
            if let Some(credit) = cx.scene.credit {
                right = right_aligned(ui, bar, (left, right), credit, &font) - gap;
            }
            let notice = notice_text(cx);
            let frame = frame_text(cx);
            let frame_width = galley(ui, &frame, font.clone(), pal::TEXT_MUTED).size().x;
            // The frame text goes first when the bar runs short, before a credit or a notice is cut.
            let notice_room = notice.as_ref().map_or(0.0, |_| gap * 8.0);
            if right - frame_width > left + notice_room {
                let pos = pos2(right, bar.center().y);
                ui.painter().text(
                    pos,
                    Align2::RIGHT_CENTER,
                    frame,
                    font.clone(),
                    pal::TEXT_MUTED,
                );
                right -= frame_width + gap;
            }
            if let Some((text, colour)) = notice {
                let top = bar.center().y - ui.text_style_height(&TextStyle::Body) * 0.5;
                ellipsized(ui.painter(), pos2(left, top), right, &text, font, colour);
            }
        },
    );
}

/// The dot, the device and its hint, `short` the API alone; returns where they end.
fn device(ui: &mut Ui, cx: &EditorContext<'_>, short: bool) -> f32 {
    ui.horizontal_centered(|ui| {
        // Amber for a CPU rasterizer, whose frame rates say nothing about the renderer.
        let colour = if cx.device.cpu {
            pal::WARNING
        } else {
            pal::ACCENT
        };
        status_dot(ui, colour);
        let text = if short {
            cx.device.api.clone()
        } else {
            device_text(cx)
        };
        let device = muted(ui, text);
        let mut tip = Vec::new();
        if short {
            tip.push(device_text(cx));
        }
        if cx.device.cpu {
            tip.push(format!(
                "CPU device: software rendering, expect low frame rates\n{}",
                cx.device.driver
            ));
        }
        if !tip.is_empty() {
            device.on_hover_text(tip.join("\n"));
        }
        if let Some(hint) = cx.device.hint.filter(|_| !short) {
            let text = egui::RichText::new(hint.text).color(pal::WARNING);
            ui.label(text).on_hover_text(hint.tooltip);
        }
        ui.min_rect().right()
    })
    .inner
}

/// `text` against the bar's right end, cut where it would cross `left`, its whole in a tooltip; returns where it starts.
fn right_aligned(
    ui: &mut Ui,
    bar: Rect,
    (left, right): (f32, f32),
    text: &str,
    font: &FontId,
) -> f32 {
    let width = galley(ui, text, font.clone(), pal::TEXT_MUTED).size().x;
    let start = (right - width).max(left);
    let top = bar.center().y - ui.text_style_height(&TextStyle::Body) * 0.5;
    let painted = ellipsized(
        ui.painter(),
        pos2(start, top),
        right,
        text,
        font.clone(),
        pal::TEXT_MUTED,
    );
    let area = Rect::from_min_max(pos2(start, bar.top()), pos2(start + painted, bar.bottom()));
    ui.interact(
        area,
        egui::Id::unique("studio_status_credit"),
        Sense::hover(),
    )
    .on_hover_text(text);
    start
}
