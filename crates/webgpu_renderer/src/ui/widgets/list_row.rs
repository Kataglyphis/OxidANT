//! The C++ list row of the outliner and the library: icon, a name that keeps priority, a muted detail, a trailing button.

use egui::{pos2, vec2, Rect, Response, Sense, TextStyle, Ui};

use super::{ellipsized, frame_height, galley, line_height};
use crate::ui::icons::{self, StudioIcon};
use crate::ui::palette as pal;
use crate::ui::theme;

/// The marked row's mint bar, the C++ `max(2, TabBarOverlineSize)`.
const MARK_WIDTH: f32 = 2.0;

/// What sits at a row's right end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowTrailing<'a> {
    #[default]
    None,
    /// An eye that shows whether the object is drawn; disabled where the renderer cannot hide it.
    Eye {
        visible: bool,
        tooltip: &'a str,
        enabled: bool,
    },
    /// A row-height button, such as the library's `Load`.
    Action(&'a str),
}

/// One row's content and state.
#[derive(Clone, Copy, Debug, Default)]
pub struct RowSpec<'a> {
    pub icon: Option<StudioIcon>,
    pub name: &'a str,
    pub detail: &'a str,
    /// The row fill: the selection, or the library's cursor.
    pub highlighted: bool,
    /// The mint bar: the selection, or the loaded model.
    pub marked: bool,
    pub trailing: RowTrailing<'a>,
    pub tooltip: Option<&'a str>,
}

/// What a click on a row did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowResult {
    pub clicked: bool,
    pub double_clicked: bool,
    /// The eye or the action button.
    pub trailing_clicked: bool,
}

/// A frame-height row over the available width.
pub fn list_row(ui: &mut Ui, spec: &RowSpec<'_>) -> RowResult {
    let size = vec2(ui.available_width(), frame_height(ui));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = match spec.tooltip {
        Some(tooltip) => response.on_hover_text(tooltip),
        None => response,
    };
    paint_row(ui, rect, spec, &response);
    let (trailing_clicked, content_right) = trailing(ui, rect, spec, &response);
    let shown = !matches!(spec.trailing, RowTrailing::Eye { visible: false, .. });
    let text_left = paint_icon(ui, rect, spec, shown);
    row_text(ui, rect, (text_left, content_right), spec, shown);
    RowResult {
        clicked: response.clicked(),
        double_clicked: response.double_clicked(),
        trailing_clicked,
    }
}

fn paint_row(ui: &Ui, rect: Rect, spec: &RowSpec<'_>, response: &Response) {
    let painter = ui.painter();
    let fill = if response.is_pointer_button_down_on() {
        Some(pal::RAISED_ACTIVE)
    } else if response.hovered() {
        Some(pal::RAISED_HOVER)
    } else {
        spec.highlighted.then_some(pal::RAISED)
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect, 0.0, fill);
    }
    if spec.marked {
        let bar = Rect::from_min_size(rect.min, vec2(MARK_WIDTH, rect.height()));
        painter.rect_filled(bar, 0.0, pal::ACCENT);
    }
}

/// The icon at the frame padding; returns where the text starts.
fn paint_icon(ui: &Ui, rect: Rect, spec: &RowSpec<'_>, shown: bool) -> f32 {
    let left = rect.left() + theme::FRAME_PADDING.x;
    let Some(icon) = spec.icon else {
        return left;
    };
    let size = line_height(ui);
    let colour = match (shown, spec.marked) {
        (false, _) => pal::TEXT_DISABLED,
        (true, true) => pal::TEXT,
        (true, false) => pal::TEXT_MUTED,
    };
    let top = rect.top() + ((rect.height() - size) * 0.5).round();
    icons::paint(
        ui.painter(),
        icon,
        Rect::from_min_size(pos2(left, top), vec2(size, size)),
        colour,
    );
    left + size + theme::INNER_SPACING.x
}

/// The trailing button, if any; returns its click and where the text must end.
fn trailing(ui: &mut Ui, rect: Rect, spec: &RowSpec<'_>, row: &Response) -> (bool, f32) {
    let right = rect.right() - theme::FRAME_PADDING.x;
    let font_size = line_height(ui);
    match spec.trailing {
        RowTrailing::None => (false, right),
        RowTrailing::Eye {
            visible,
            tooltip,
            enabled,
        } => {
            // Placed so the eye's ink ends where a detail would end.
            let x = right - ((rect.height() + font_size) * 0.5).round();
            let button =
                Rect::from_min_size(pos2(x, rect.top()), vec2(rect.height(), rect.height()));
            let icon = if visible {
                StudioIcon::Eye
            } else {
                StudioIcon::EyeOff
            };
            let response =
                icons::icon_button_at(ui, row.id.with("eye"), button, icon, enabled, tooltip);
            let text_right =
                x + ((rect.height() - font_size) * 0.5).round() - theme::INNER_SPACING.x;
            (response.clicked(), text_right)
        }
        RowTrailing::Action(label) => {
            let font = TextStyle::Body.resolve(ui.style());
            let width = galley(ui, label, font, pal::TEXT).size().x + theme::FRAME_PADDING.x * 2.0;
            let button = Rect::from_min_max(pos2(rect.right() - width, rect.top()), rect.max);
            let mut slot = ui.new_child(egui::UiBuilder::new().max_rect(button).id_salt(row.id));
            let clicked = slot
                .add_sized(button.size(), egui::Button::new(label))
                .clicked();
            (clicked, button.left() - theme::INNER_SPACING.x)
        }
    }
}

/// The name keeps priority; the detail takes what is left and ellipsizes first.
fn row_text(ui: &Ui, rect: Rect, (left, right): (f32, f32), spec: &RowSpec<'_>, shown: bool) {
    let font = TextStyle::Body.resolve(ui.style());
    let available = (right - left).max(0.0);
    let gap = ui.spacing().item_spacing.x;
    let name_width = galley(ui, spec.name, font.clone(), pal::TEXT).size().x;
    let detail_space = if spec.detail.is_empty() {
        0.0
    } else {
        let detail_width = galley(ui, spec.detail, font.clone(), pal::TEXT).size().x;
        detail_width.min((available - name_width.min(available * 0.6) - gap).max(0.0))
    };
    let name_max = left
        + (available
            - if detail_space > 0.0 {
                detail_space + gap
            } else {
                0.0
            })
        .max(0.0);
    let top = rect.top() + ((rect.height() - line_height(ui)) * 0.5).round();
    let painter = ui.painter();
    let colour = if shown { pal::TEXT } else { pal::TEXT_DISABLED };
    ellipsized(
        painter,
        pos2(left, top),
        name_max,
        spec.name,
        font.clone(),
        colour,
    );
    if detail_space > 0.0 {
        let detail_left = right - detail_space;
        let detail = galley(ui, spec.detail, font.clone(), pal::TEXT_MUTED);
        if detail.size().x <= detail_space {
            painter.galley(pos2(right - detail.size().x, top), detail, pal::TEXT_MUTED);
        } else {
            ellipsized(
                painter,
                pos2(detail_left, top),
                right,
                spec.detail,
                font,
                pal::TEXT_MUTED,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};

    fn rows(ui: &mut Ui, sky: bool) -> [RowResult; 2] {
        let model = RowSpec {
            icon: Some(StudioIcon::Cube),
            name: "Model",
            detail: "Dinosaurs/dinosaurs",
            highlighted: true,
            marked: true,
            ..Default::default()
        };
        let skybox = RowSpec {
            icon: Some(StudioIcon::Mountain),
            name: "Skybox",
            trailing: RowTrailing::Eye {
                visible: sky,
                tooltip: "Show or hide the skybox",
                enabled: true,
            },
            ..Default::default()
        };
        [list_row(ui, &model), list_row(ui, &skybox)]
    }

    #[test]
    fn a_row_is_a_frame_tall_and_its_eye_takes_the_click_at_the_right_end() {
        let ctx = context();
        let (mut top, mut right, mut height) = (0.0, 0.0, 0.0);
        frame(&ctx, Vec::new(), |ui| {
            top = ui.cursor().top();
            right = ui.max_rect().right();
            rows(ui, true);
            height = ui.cursor().top() - top;
        });
        let pitch = theme::FRAME_HEIGHT + 6.0;
        assert_eq!(height, 2.0 * pitch);
        let second_row = top + pitch + theme::FRAME_HEIGHT * 0.5;
        let eye = frame(&ctx, click(right - 20.0, second_row), |ui| rows(ui, true));
        assert_eq!(
            eye[1],
            RowResult {
                clicked: false,
                double_clicked: false,
                trailing_clicked: true
            }
        );
        let body = frame(&ctx, click(right - 120.0, second_row), |ui| rows(ui, true));
        assert!(body[1].clicked && !body[1].trailing_clicked);
        assert_eq!(body[0], RowResult::default());
    }
}
