//! The C++ status bar: the device on the left, the frame size, build and version on the right.

use egui::{pos2, Align2, Margin, TextStyle, Ui};

use super::docks::{self, Side};
use super::EditorContext;
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{frame_height, galley, muted, status_dot};

/// The build the C++ status bar and About name.
pub(super) const BUILD: &str = if cfg!(debug_assertions) {
    "Debug"
} else {
    "Release"
};

/// `Vulkan  ·  llvmpipe (...)`: the API and the adapter.
pub(super) fn device_text(cx: &EditorContext<'_>) -> String {
    format!("{}  ·  {}", cx.device.api, cx.device.name)
}

/// `1600 × 900  ·  Release  ·  v3.0.0  ·  F1 hides the editor`.
pub(super) fn frame_text(cx: &EditorContext<'_>) -> String {
    let [width, height] = cx.surface_px;
    let version = env!("CARGO_PKG_VERSION");
    format!("{width} × {height}  ·  {BUILD}  ·  v{version}  ·  F1 hides the editor")
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
            let left_end = ui
                .horizontal_centered(|ui| {
                    // Amber for a CPU rasterizer, whose frame rates say nothing about the renderer.
                    let colour = if cx.device.cpu {
                        pal::WARNING
                    } else {
                        pal::ACCENT
                    };
                    status_dot(ui, colour);
                    let device = muted(ui, device_text(cx));
                    if cx.device.cpu {
                        let tip = format!(
                            "CPU device: software rendering, expect low frame rates\n{}",
                            cx.device.driver
                        );
                        device.on_hover_text(tip);
                    }
                    ui.min_rect().right()
                })
                .inner;
            let text = frame_text(cx);
            let font = TextStyle::Body.resolve(ui.style());
            let width = galley(ui, &text, font.clone(), pal::TEXT_MUTED).size().x;
            if bar.right() - width > left_end + ui.spacing().item_spacing.x {
                let pos = pos2(bar.right(), bar.center().y);
                ui.painter()
                    .text(pos, Align2::RIGHT_CENTER, text, font, pal::TEXT_MUTED);
            }
        },
    );
}
