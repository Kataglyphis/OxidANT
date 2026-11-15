//! The floating windows: the About modal, egui's style editor and its inspection window.

use egui::{vec2, Button, Color32, Context, Id, Modal, RichText, Theme, Ui, Window};

use super::status_bar::BUILD;
use super::{Editor, EditorContext, BRAND};
use crate::ui::fonts;
use crate::ui::icons::{self, StudioIcon};
use crate::ui::palette as pal;
use crate::ui::widgets::{frame_height, line_height, muted};

/// The egui this crate builds against; a test holds it to the manifest.
pub(super) const EGUI_VERSION: &str = "0.37";
const REPOSITORY: &str = "https://github.com/Kataglyphis/BeschleunigerBallett";
/// The C++ ModalWindowDimBg, black at 55 %.
const MODAL_DIM: Color32 = Color32::from_black_alpha(140);

pub(super) fn show(ctx: &Context, editor: &mut Editor, cx: &EditorContext<'_>) {
    let font = line_height_of(ctx);
    Window::new("Style editor")
        .open(&mut editor.windows.style)
        .default_size(vec2(font * 30.0, font * 40.0))
        .show(ctx, |ui| ctx.style_ui(ui, Theme::Dark));
    Window::new("egui inspection")
        .open(&mut editor.windows.inspection)
        .show(ctx, |ui| ctx.inspection_ui(ui));
    if editor.windows.about {
        let id = Id::unique("studio_about");
        // ImGui shows a modal at once; egui would fade it in over a few frames.
        let modal = Modal::new(id)
            .area(Modal::default_area(id).fade_in(false))
            .backdrop_color(MODAL_DIM)
            .show(ctx, |ui| about(ui, cx));
        if modal.inner || modal.should_close() {
            editor.windows.about = false;
        }
    }
}

fn line_height_of(ctx: &Context) -> f32 {
    ctx.fonts_mut(|f| f.row_height(&fonts::body_font()))
}

/// The C++ About: accent cube, name and line, the facts, the link, Close; true when Close was clicked.
fn about(ui: &mut Ui, cx: &EditorContext<'_>) -> bool {
    let font = line_height(ui);
    ui.horizontal(|ui| {
        icons::icon_text(ui, StudioIcon::Cube, font * 2.4, pal::ACCENT);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(BRAND)
                    .font(fonts::title_font())
                    .color(pal::TEXT),
            );
            muted(ui, "A WebGPU graphics engine playground");
        });
    });
    ui.add_space(ui.spacing().item_spacing.y);
    facts(ui, cx, font);
    ui.add_space(ui.spacing().item_spacing.y);
    ui.hyperlink_to("github.com/Kataglyphis/BeschleunigerBallett", REPOSITORY);
    ui.add_space(ui.spacing().item_spacing.y);
    ui.add_sized(vec2(font * 8.0, frame_height(ui)), Button::new("Close"))
        .clicked()
}

/// The C++ About rows: a muted name in a 6.5-line column, then the value, one text line each.
fn facts(ui: &mut Ui, cx: &EditorContext<'_>, font: f32) {
    let rows = [
        ("Version", env!("CARGO_PKG_VERSION").to_owned()),
        ("Build", BUILD.to_owned()),
        ("GPU", cx.device.name.clone()),
        ("API", cx.device.api.clone()),
        ("egui", EGUI_VERSION.to_owned()),
        (
            "Fonts",
            "Roboto Medium (Apache-2.0), Cousine (OFL-1.1)".to_owned(),
        ),
    ];
    let body = egui::TextStyle::Body.resolve(ui.style());
    ui.scope(|ui| {
        // Text rows, not frames: the C++ rows are a line apart.
        ui.spacing_mut().interact_size.y = font;
        for (name, value) in rows {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(font * 6.5, font), egui::Sense::hover());
                let anchor = egui::Align2::LEFT_CENTER;
                ui.painter().text(
                    rect.left_center(),
                    anchor,
                    name,
                    body.clone(),
                    pal::TEXT_MUTED,
                );
                ui.label(RichText::new(value).color(pal::TEXT));
            });
        }
    });
}
