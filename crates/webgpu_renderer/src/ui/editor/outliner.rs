//! The scene dock's outliner: the C++ six objects, a click selecting one and opening its inspector tab.

use egui::Ui;

use super::{library, Editor, EditorContext, SceneObject};
use crate::render::forward::CASCADE_COUNT;
use crate::ui::widgets::{caption_line, list_row, section_header, RowSpec, RowTrailing};

/// The scene dock: SCENE, its rows, then the LIBRARY.
pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    caption_line(ui, "SCENE", None);
    let spacing = ui.spacing().item_spacing.y;
    // A tighter pitch than the property grid, so the highlights of adjacent rows nearly touch.
    ui.spacing_mut().item_spacing.y = (spacing / 3.0).round();
    for object in SceneObject::ALL {
        let detail = detail(object, cx);
        let spec = RowSpec {
            icon: Some(object.icon()),
            name: object.name(),
            detail: &detail,
            highlighted: editor.selection == object,
            marked: editor.selection == object,
            trailing: trailing(object),
            tooltip: None,
        };
        if list_row(ui, &spec).clicked {
            editor.select(object);
        }
    }
    ui.spacing_mut().item_spacing.y = spacing;
    section_header(ui, "LIBRARY", None);
    library::show(ui, editor, cx);
}

/// The muted text at a row's right end.
pub(super) fn detail(object: SceneObject, cx: &EditorContext<'_>) -> String {
    match object {
        SceneObject::Model => cx
            .model
            .map_or_else(|| "none".to_owned(), |m| m.name.clone()),
        SceneObject::Sun => format!("{CASCADE_COUNT} cascades"),
        SceneObject::Camera => format!("{:.0}°", cx.camera.fov_y_deg),
        SceneObject::Renderer => "Raster".to_owned(),
        SceneObject::Skybox | SceneObject::Clouds => String::new(),
    }
}

/// The sky and the clouds carry the C++ eye; the renderer always draws the one and has not got the other.
fn trailing(object: SceneObject) -> RowTrailing<'static> {
    match object {
        SceneObject::Skybox => RowTrailing::Eye {
            visible: true,
            tooltip: "Show or hide the skybox",
            enabled: false,
        },
        SceneObject::Clouds => RowTrailing::Eye {
            visible: false,
            tooltip: "Show or hide the clouds",
            enabled: false,
        },
        _ => RowTrailing::None,
    }
}
