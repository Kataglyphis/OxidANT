use std::collections::VecDeque;

pub(crate) fn draw_cpu_history(ui: &mut egui::Ui, history: &VecDeque<f32>) {
    let desired = egui::vec2(160.0, 48.0);
    let (rect, _response) = ui.allocate_exact_size(desired, egui::Sense::hover());

    let painter = ui.painter();
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::GRAY),
        egui::StrokeKind::Inside,
    );

    if history.len() < 2 {
        return;
    }

    let stroke = egui::Stroke::new(1.5, egui::Color32::LIGHT_BLUE);
    painter.add(egui::Shape::line(history_points(rect, history), stroke));
}

/// Each sample as a point in `rect`; the scale tops out at 100 % or the peak, as a busy process sums its cores.
fn history_points(rect: egui::Rect, history: &VecDeque<f32>) -> Vec<egui::Pos2> {
    let ceiling = history.iter().copied().fold(100.0, f32::max);
    let step_x = rect.width() / (history.len().max(2) as f32 - 1.0);
    history
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            let y_norm = (value / ceiling).clamp(0.0, 1.0);
            egui::pos2(
                rect.left() + (i as f32) * step_x,
                rect.bottom() - y_norm * rect.height(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_core_history_keeps_its_trend_inside_the_graph() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(160.0, 48.0));
        let points = history_points(rect, &VecDeque::from([350.0, 300.0, 250.0]));
        assert_eq!(points[0].y, rect.top(), "the peak touches the top");
        assert!(
            points[0].y < points[1].y && points[1].y < points[2].y,
            "{points:?}"
        );
        assert!(points.iter().all(|p| rect.contains(*p)), "{points:?}");
        let single = history_points(rect, &VecDeque::from([50.0, 25.0]));
        assert_eq!(
            single[0].y,
            rect.bottom() - 24.0,
            "one core still reads against 100 %"
        );
        assert_eq!(single[1].y, rect.bottom() - 12.0);
        assert_eq!(single[1].x, rect.right());
    }
}
