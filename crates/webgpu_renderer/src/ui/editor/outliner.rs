//! The scene dock's outliner: the C++ six objects, a click selecting one and opening its inspector tab.

use egui::Ui;

use super::{library, Editor, EditorContext, SceneObject};
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
            trailing: trailing(object, cx),
            tooltip: None,
        };
        let row = list_row(ui, &spec);
        if row.trailing_clicked {
            toggle_shown(object, cx);
        } else if row.clicked {
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
        SceneObject::Sun if cx.settings.shadows.enabled => {
            format!("{} cascades", cx.settings.shadows.sanitized().cascade_count)
        }
        SceneObject::Sun => "no shadows".to_owned(),
        SceneObject::Camera => format!("{:.0}°", cx.camera.fov_y_deg),
        SceneObject::Renderer => "Raster".to_owned(),
        SceneObject::Skybox | SceneObject::Clouds => String::new(),
    }
}

/// The sky and the clouds carry the C++ eye, which shows and hides what they draw.
fn trailing(object: SceneObject, cx: &EditorContext<'_>) -> RowTrailing<'static> {
    match object {
        SceneObject::Skybox => RowTrailing::Eye {
            visible: cx.settings.skybox_enabled,
            tooltip: "Show or hide the skybox",
            enabled: true,
        },
        SceneObject::Clouds => RowTrailing::Eye {
            visible: cx.settings.clouds.enabled,
            tooltip: "Show or hide the clouds",
            enabled: true,
        },
        _ => RowTrailing::None,
    }
}

/// A click on an eye: the skybox switches to the C++ navy and back, the clouds off and on.
fn toggle_shown(object: SceneObject, cx: &mut EditorContext<'_>) {
    match object {
        SceneObject::Skybox => cx.settings.skybox_enabled = !cx.settings.skybox_enabled,
        SceneObject::Clouds => cx.settings.clouds.enabled = !cx.settings.clouds.enabled,
        _ => {}
    }
}
