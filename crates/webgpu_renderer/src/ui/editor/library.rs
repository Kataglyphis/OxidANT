//! The scene dock's model library, for now the C++ layout over the loaded model alone: a filter, then its row.

use egui::{Margin, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, EditorContext};
use crate::ui::palette as pal;
use crate::ui::widgets::{frame_height, list_row, muted_wrapped, RowSpec};

/// The filter field's id, fixed so a test can find the field.
pub(super) fn filter_id() -> egui::Id {
    egui::Id::unique("studio_library_filter")
}

pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    let filter = TextEdit::singleline(&mut editor.library_filter)
        .id(filter_id())
        .hint_text(RichText::new("Filter").color(pal::TEXT_DISABLED))
        .margin(Margin::symmetric(8, 4))
        .min_size(egui::vec2(0.0, frame_height(ui)))
        .desired_width(f32::INFINITY);
    ui.add(filter);
    let Some(model) = cx.model else {
        muted_wrapped(ui, "No model loaded");
        return;
    };
    let wanted = editor.library_filter.trim().to_lowercase();
    ScrollArea::vertical()
        .auto_shrink(false)
        .id_salt("library")
        .show(ui, |ui| {
            if !model.name.to_lowercase().contains(&wanted) {
                return;
            }
            let spec = RowSpec {
                name: &model.name,
                highlighted: true,
                marked: true,
                tooltip: Some(&model.path),
                ..Default::default()
            };
            list_row(ui, &spec);
        });
}
