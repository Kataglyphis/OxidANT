//! The profiler dock, for now the C++ layout: Performance (frame figure, frame-time graph, GPU passes) and Visibility.

use egui::{pos2, vec2, Align, Layout, Rect, RichText, Sense, Shape, Stroke, Ui};

use super::{Editor, EditorContext, FRAME_RATE_WINDOW};
use crate::ui::palette as pal;
use crate::ui::widgets::{
    caption, frame_height, galley, metric_row, muted, tab_bar, toggle, MetricRow,
};
use crate::ui::{fonts, theme};

const TABS: [&str; 2] = ["Performance", "Visibility"];

pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    tab_bar(ui, &mut editor.profiler_tab, &TABS);
    match editor.profiler_tab {
        0 => performance(ui, cx),
        _ => visibility(ui, cx),
    }
}

/// The C++ two-column table, 40/60 with a rule between: FRAME on the left, GPU PASSES on the right.
fn performance(ui: &mut Ui, cx: &EditorContext<'_>) {
    let body = ui.available_rect_before_wrap();
    let split = body.left() + (body.width() * 0.4).round();
    let rule = theme::hairline(ui);
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(split, body.top()), pos2(split + rule, body.bottom())),
        0.0,
        pal::BORDER,
    );
    let gap = theme::INNER_SPACING.x * 2.0;
    let left = Rect::from_min_max(body.min, pos2(split - gap * 0.5, body.bottom()));
    let right = Rect::from_min_max(pos2(split + gap, body.top()), body.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| frame(ui, cx));
    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
        gpu_passes(ui, cx)
    });
}

fn frame(ui: &mut Ui, cx: &EditorContext<'_>) {
    caption(ui, "FRAME", 0.0);
    let frame_ms = cx.frames.mean_ms(FRAME_RATE_WINDOW).unwrap_or(0.0);
    let decimals = if frame_ms >= 100.0 {
        0
    } else if frame_ms >= 10.0 {
        1
    } else {
        2
    };
    let figure = fonts::figure_font();
    let height = ui.ctx().fonts_mut(|f| f.row_height(&figure));
    // Bottom-aligned in a row of the figure's own height, so `ms` sits on the figure's baseline.
    let row = vec2(ui.available_width(), height);
    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Max), |ui| {
        ui.spacing_mut().item_spacing.x = theme::INNER_SPACING.x;
        let text = RichText::new(format!("{frame_ms:.decimals$}")).font(figure);
        ui.label(text.color(pal::TEXT));
        muted(ui, "ms");
    });
    let fps = if frame_ms > 0.0 {
        1000.0 / frame_ms
    } else {
        0.0
    };
    muted(ui, format!("{fps:.0} FPS"));
    graph(ui, cx);
}

/// The last 120 frame times as one mint line, scaled to 1.25 times the peak.
fn graph(ui: &mut Ui, cx: &EditorContext<'_>) {
    let height = ui.available_height().max(frame_height(ui) * 1.5);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme::FRAME_RADIUS, pal::FRAME);
    let samples: Vec<f32> = cx.frames.samples_ms().collect();
    if samples.len() < 2 {
        return;
    }
    let top = samples.iter().copied().fold(1.0, f32::max) * 1.25;
    let step = rect.width() / (super::FRAME_HISTORY - 1) as f32;
    let points = samples
        .iter()
        .enumerate()
        .map(|(i, ms)| {
            pos2(
                rect.left() + step * i as f32,
                rect.bottom() - rect.height() * ms / top,
            )
        })
        .collect();
    ui.painter()
        .add(Shape::line(points, Stroke::new(1.0, pal::ACCENT)));
}

fn gpu_passes(ui: &mut Ui, cx: &EditorContext<'_>) {
    caption(ui, "GPU PASSES", 0.0);
    if !cx.stats.gpu_timing {
        muted(ui, "unavailable (no timestamp queries on this adapter)");
        return;
    }
    let frame_ms = cx.frames.mean_ms(FRAME_RATE_WINDOW).unwrap_or(0.0);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let name_width = cx
        .stats
        .gpu_passes
        .iter()
        .map(|(name, _)| galley(ui, *name, font.clone(), pal::TEXT).size().x)
        .fold(0.0, f32::max);
    let value_width = galley(ui, "0000.00 ms", font, pal::TEXT).size().x;
    ui.spacing_mut().item_spacing.y = 2.0;
    for (name, ms) in &cx.stats.gpu_passes {
        let value = format!("{ms:.2} ms");
        let fraction = if frame_ms > 0.0 { ms / frame_ms } else { 0.0 };
        let row = MetricRow {
            name,
            fraction,
            value: &value,
            active: true,
        };
        metric_row(ui, (name_width, value_width), &row);
    }
}

/// The C++ Visibility tab: the frustum-culling switch, what the camera pass and the cascades drew, and what they culled.
fn visibility(ui: &mut Ui, cx: &mut EditorContext<'_>) {
    ui.horizontal(|ui| {
        ui.label("Frustum culling");
        toggle(ui, &mut cx.settings.frustum_culling);
    });
    let font = egui::TextStyle::Body.resolve(ui.style());
    let names = ["Primitives drawn", "Shadow casters drawn"];
    let name_width = names
        .iter()
        .map(|name| galley(ui, *name, font.clone(), pal::TEXT).size().x)
        .fold(0.0, f32::max);
    let value_width = galley(ui, "00000 / 00000", font, pal::TEXT).size().x;
    let counts = [cx.stats.primitives, cx.stats.shadow_casters];
    for (name, (drawn, total)) in names.into_iter().zip(counts) {
        let value = format!("{drawn} / {total}");
        let fraction = if total > 0 {
            drawn as f32 / total as f32
        } else {
            0.0
        };
        let row = MetricRow {
            name,
            fraction,
            value: &value,
            active: total > 0,
        };
        metric_row(ui, (name_width, value_width), &row);
    }
    let [(primitives, primitive_share), (casters, caster_share)] = counts.map(culled);
    muted(
        ui,
        format!(
            "Culled: {primitives} primitives ({primitive_share:.1} %), \
             {casters} shadow casters ({caster_share:.1} %)"
        ),
    );
}

/// How many of `total` were not drawn, and their share in percent.
fn culled((drawn, total): (u32, u32)) -> (u32, f32) {
    let culled = total.saturating_sub(drawn);
    let share = if total > 0 {
        100.0 * culled as f32 / total as f32
    } else {
        0.0
    };
    (culled, share)
}
