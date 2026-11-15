//! The inspector dock: the C++ tab bar kept in step with the outliner, a title row, then one module per tab.

mod camera;
mod model;
mod renderer;
mod sky;
mod sun;

use egui::{vec2, Align, Layout, RichText, ScrollArea, Sense, TextStyle, Ui};

use super::{Editor, EditorContext, InspectorTab, SceneObject};
use crate::ui::fonts;
use crate::ui::icons;
use crate::ui::palette as pal;
use crate::ui::widgets::{ellipsized, tab_bar};

/// The tab labels in the C++ order.
const LABELS: [&str; 5] = ["Model", "Sun", "Camera", "Sky", "Renderer"];

pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    let mut open = InspectorTab::ALL
        .iter()
        .position(|tab| *tab == editor.selection.tab())
        .unwrap_or(0);
    // A tab click selects the tab's primary object; the selection alone decides the open tab.
    if tab_bar(ui, &mut open, &LABELS) {
        editor.selection = InspectorTab::ALL[open].primary();
    }
    let object = editor.selection;
    title(ui, object, &subtitle(object, cx));
    ScrollArea::vertical()
        .auto_shrink(false)
        .id_salt("inspector_body")
        .show(ui, |ui| match object.tab() {
            InspectorTab::Model => model::show(ui, cx),
            InspectorTab::Sun => sun::show(ui, cx),
            InspectorTab::Camera => camera::show(ui, cx),
            InspectorTab::Sky => sky::show(ui, cx),
            InspectorTab::Renderer => renderer::show(ui, cx, &mut editor.actions),
        });
}

/// What the title row says beside the object's name.
pub(super) fn subtitle(object: SceneObject, cx: &EditorContext<'_>) -> String {
    match object {
        SceneObject::Model => cx
            .model
            .map_or_else(|| "No model".to_owned(), |m| m.name.clone()),
        SceneObject::Sun => "Directional light".to_owned(),
        SceneObject::Camera => "Perspective".to_owned(),
        SceneObject::Skybox => "Procedural".to_owned(),
        SceneObject::Clouds => "Volumetric".to_owned(),
        SceneObject::Renderer => super::render_mode::drawn_label(cx).to_owned(),
    }
}

/// The object's icon and name at title size, then its kind in the muted body face on the same baseline.
fn title(ui: &mut Ui, object: SceneObject, subtitle: &str) {
    ui.add_space(ui.spacing().item_spacing.y * 0.5);
    let title_font = fonts::title_font();
    let height = ui.ctx().fonts_mut(|f| f.row_height(&title_font));
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::left_to_right(Align::Max),
        |ui| {
            icons::icon_text(ui, object.icon(), height, pal::TEXT_MUTED);
            ui.label(
                RichText::new(object.name())
                    .font(title_font)
                    .color(pal::TEXT),
            );
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
            let font = TextStyle::Body.resolve(ui.style());
            let line = ui.text_style_height(&TextStyle::Body);
            let top = rect.bottom() - line;
            ellipsized(
                ui.painter(),
                egui::pos2(rect.left(), top),
                rect.right(),
                subtitle,
                font,
                pal::TEXT_MUTED,
            );
        },
    );
}
