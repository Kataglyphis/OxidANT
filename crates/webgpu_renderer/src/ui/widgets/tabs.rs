//! The C++ tab bar: muted labels, the open one bright and underlined in mint, a hairline under the bar.

use egui::{pos2, vec2, Align2, Rect, Role, Sense, TextStyle, Ui, WidgetInfo};

use super::{frame_height, galley, segmented::underline};
use crate::ui::palette as pal;
use crate::ui::theme;

/// The C++ tab rounding.
const TAB_RADIUS: f32 = 4.0;

/// One row of tabs over the available width; true when a click opened another tab.
pub fn tab_bar(ui: &mut Ui, open: &mut usize, labels: &[&str]) -> bool {
    let height = frame_height(ui);
    let (bar, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let font = TextStyle::Body.resolve(ui.style());
    let mut x = bar.left();
    let mut changed = false;
    for (index, label) in labels.iter().enumerate() {
        let text = galley(ui, *label, font.clone(), pal::TEXT);
        let width = text.size().x + theme::FRAME_PADDING.x * 2.0;
        let rect = Rect::from_min_size(pos2(x, bar.top()), vec2(width, height));
        x = rect.right() + theme::INNER_SPACING.x;
        let tab = ui.interact(rect, response.id.with(index), Sense::click());
        let is_open = index == *open;
        tab.widget_info(|| WidgetInfo::selected(Role::Tab, true, is_open, *label));
        if tab.clicked() && !is_open {
            *open = index;
            changed = true;
        }
        paint_tab(ui, rect, label, index == *open, tab.hovered());
    }
    let rule = theme::hairline(ui);
    let line = Rect::from_min_max(pos2(bar.left(), bar.bottom() - rule), bar.max);
    ui.painter().rect_filled(line, 0.0, pal::RAISED);
    changed
}

fn paint_tab(ui: &Ui, rect: Rect, label: &str, open: bool, hovered: bool) {
    let painter = ui.painter();
    let top = egui::CornerRadius {
        nw: TAB_RADIUS as u8,
        ne: TAB_RADIUS as u8,
        sw: 0,
        se: 0,
    };
    if open {
        painter.rect_filled(rect, top, pal::RAISED);
        underline(ui, rect);
    } else if hovered {
        painter.rect_filled(rect, top, pal::FRAME_HOVER);
    }
    let colour = if open { pal::TEXT } else { pal::TEXT_MUTED };
    let font = TextStyle::Body.resolve(ui.style());
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, colour);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};

    #[test]
    fn a_click_opens_a_tab_and_a_click_on_the_open_tab_changes_nothing() {
        let ctx = context();
        let labels = ["Model", "Sun", "Camera", "Sky", "Renderer"];
        let mut open = 0;
        let (mut height, mut sun_x, mut y) = (0.0, 0.0, 0.0);
        frame(&ctx, Vec::new(), |ui| {
            let top = ui.cursor().top();
            let font = TextStyle::Body.resolve(ui.style());
            let model = galley(ui, "Model", font, pal::TEXT).size().x;
            sun_x = ui.cursor().left() + model + 16.0 + theme::INNER_SPACING.x + 8.0;
            y = top + 10.0;
            tab_bar(ui, &mut open, &labels);
            height = ui.cursor().top() - top;
        });
        assert!(height >= theme::FRAME_HEIGHT, "{height}");
        assert!(frame(&ctx, click(sun_x, y), |ui| tab_bar(
            ui, &mut open, &labels
        )));
        assert_eq!(open, 1);
        assert!(!frame(&ctx, click(sun_x, y), |ui| tab_bar(
            ui, &mut open, &labels
        )));
        assert_eq!(open, 1);
    }
}
