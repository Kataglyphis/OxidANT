//! The dock and bar frames: exact-size panels on the root `Ui` with one hairline on the scene side, and the splitters.

use egui::{pos2, Context, Frame, Id, InnerResponse, Margin, Panel, Rect, Ui};

use super::layout::{
    self, DockPlan, Splitter, MIN_INSPECTOR_WIDTH, MIN_PROFILER_HEIGHT, MIN_SCENE_WIDTH,
};
use super::Editor;
use crate::ui::palette as pal;
use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The panel fill and padding every dock and bar shares; the C++ WindowPadding inside.
fn frame(margin: Margin) -> Frame {
    Frame::NONE.fill(pal::PANEL).inner_margin(margin)
}

/// One side panel of exactly `size` points, egui's own separator and resize handle off.
pub(super) fn panel<R>(
    ui: &mut Ui,
    id: &'static str,
    side: Side,
    size: f32,
    margin: Margin,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let panel = match side {
        Side::Left => Panel::left(id),
        Side::Right => Panel::right(id),
        Side::Top => Panel::top(id),
        Side::Bottom => Panel::bottom(id),
    };
    let response = panel
        .exact_size(size)
        .resizable(false)
        .show_separator_line(false)
        .frame(frame(margin))
        .show(ui, add);
    hairline(ui, response.response.rect, side);
    response
}

/// A dock: a panel with the C++ window padding.
pub(super) fn dock<R>(ui: &mut Ui, side: Side, size: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let id = match side {
        Side::Left => "studio_scene",
        Side::Right => "studio_inspector",
        Side::Top | Side::Bottom => "studio_profiler",
    };
    let padding = theme::WINDOW_PADDING;
    let margin = Margin::symmetric(padding.x as i8, padding.y as i8);
    panel(ui, id, side, size, margin, add).inner
}

/// One physical pixel or more along the panel's edge that faces the scene, inside the panel.
fn hairline(ui: &Ui, rect: Rect, side: Side) {
    let width = theme::hairline(ui);
    let line = match side {
        Side::Left => Rect::from_min_max(pos2(rect.right() - width, rect.top()), rect.max),
        Side::Right => Rect::from_min_max(rect.min, pos2(rect.left() + width, rect.bottom())),
        Side::Top => Rect::from_min_max(pos2(rect.left(), rect.bottom() - width), rect.max),
        Side::Bottom => Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + width)),
    };
    ui.painter().rect_filled(line, 0.0, pal::BORDER);
}

/// The splitters inside the scene's dock edges; a drag stores its size only when it really resized.
pub(super) fn splitters(ctx: &Context, editor: &mut Editor, plan: &DockPlan, scene: Rect) {
    let thickness =
        (layout::SPLITTER_THICKNESS * ctx.pixels_per_point()).round() / ctx.pixels_per_point();
    let [left, right, bottom] = layout::handles(scene, thickness);
    let prefs = &mut editor.prefs;
    let docks = [
        (
            plan.scene,
            left,
            0,
            1.0,
            MIN_SCENE_WIDTH,
            plan.scene_max,
            &mut prefs.scene_width,
        ),
        (
            plan.inspector,
            right,
            0,
            -1.0,
            MIN_INSPECTOR_WIDTH,
            plan.inspector_max,
            &mut prefs.inspector_width,
        ),
        (
            plan.profiler,
            bottom,
            1,
            -1.0,
            MIN_PROFILER_HEIGHT,
            plan.profiler_max,
            &mut prefs.profiler_height,
        ),
    ];
    for (index, (extent, handle, axis, growth, min, max, preference)) in
        docks.into_iter().enumerate()
    {
        let drag = &mut editor.drags[index];
        // A dock dropped mid-drag never sees the release, so its drag would resume with another's.
        let Some(mut size) = extent else {
            *drag = Default::default();
            continue;
        };
        let splitter = Splitter {
            id: Id::unique(("studio_splitter", index)),
            handle,
            axis,
            growth,
            min,
            max,
        };
        if splitter.show(ctx, &mut size, drag) {
            *preference = size;
        }
    }
}
