//! The C++ dock geometry and splitters: sizes from preferences, capped by the window, dropped when the scene would starve.

use egui::{pos2, Context, CursorIcon, Id, Order, Rect, Sense, Vec2};

use crate::scene::viewport::SceneViewport;
use crate::ui::palette as pal;

pub const DEFAULT_SCENE_WIDTH: f32 = 248.0;
pub const DEFAULT_INSPECTOR_WIDTH: f32 = 336.0;
pub const DEFAULT_PROFILER_HEIGHT: f32 = 172.0;
pub const MIN_SCENE_WIDTH: f32 = 180.0;
pub const MIN_INSPECTOR_WIDTH: f32 = 260.0;
pub const MIN_PROFILER_HEIGHT: f32 = 116.0;
const MAX_SCENE_SHARE: f32 = 0.3;
const MAX_INSPECTOR_SHARE: f32 = 0.4;
const MAX_PROFILER_SHARE: f32 = 0.45;
/// The least of the scene a dock may leave; a smaller window drops docks instead of overlapping them.
pub const MIN_SCENE_VIEW: Vec2 = Vec2::new(320.0, 180.0);
pub const SPLITTER_THICKNESS: f32 = 6.0;

/// What the user asked for: each dock's size and whether it is shown; the window decides what it gets.
#[derive(Clone, Debug, PartialEq)]
pub struct DockPrefs {
    pub scene_width: f32,
    pub inspector_width: f32,
    pub profiler_height: f32,
    pub show_scene: bool,
    pub show_inspector: bool,
    pub show_profiler: bool,
}

impl Default for DockPrefs {
    fn default() -> Self {
        Self {
            scene_width: DEFAULT_SCENE_WIDTH,
            inspector_width: DEFAULT_INSPECTOR_WIDTH,
            profiler_height: DEFAULT_PROFILER_HEIGHT,
            show_scene: true,
            show_inspector: true,
            show_profiler: true,
        }
    }
}

/// One frame's docks between the bars: each shown dock's extent and how far its splitter may take it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockPlan {
    pub scene: Option<f32>,
    pub inspector: Option<f32>,
    pub profiler: Option<f32>,
    pub scene_max: f32,
    pub inspector_max: f32,
    pub profiler_max: f32,
}

/// Whole physical pixels, so a dock's edge and its hairline stay crisp.
fn snap(points: f32, pixels_per_point: f32) -> f32 {
    (points * pixels_per_point).round() / pixels_per_point
}

/// The C++ `dockExtent`: the preference held between the minimum and the window's share.
fn extent(preferred: f32, minimum: f32, maximum: f32, pixels_per_point: f32) -> f32 {
    snap(
        preferred.clamp(minimum, minimum.max(maximum)),
        pixels_per_point,
    )
}

impl DockPlan {
    /// The plan for `inner`, the room between the bars; a narrow window drops the scene dock, then the inspector.
    pub fn new(inner: Vec2, prefs: &DockPrefs, pixels_per_point: f32) -> Self {
        let scene = extent(
            prefs.scene_width,
            MIN_SCENE_WIDTH,
            inner.x * MAX_SCENE_SHARE,
            pixels_per_point,
        );
        let inspector = extent(
            prefs.inspector_width,
            MIN_INSPECTOR_WIDTH,
            inner.x * MAX_INSPECTOR_SHARE,
            pixels_per_point,
        );
        let profiler = extent(
            prefs.profiler_height,
            MIN_PROFILER_HEIGHT,
            inner.y * MAX_PROFILER_SHARE,
            pixels_per_point,
        );
        // Extents are whole pixels, so a dock dragged to its stop may round half a pixel past the limit.
        let room = inner + Vec2::splat(0.5);
        let show_inspector = prefs.show_inspector && inspector + MIN_SCENE_VIEW.x <= room.x;
        let inspector_share = if show_inspector { inspector } else { 0.0 };
        let show_scene = prefs.show_scene && scene + inspector_share + MIN_SCENE_VIEW.x <= room.x;
        let scene_share = if show_scene { scene } else { 0.0 };
        let show_profiler = prefs.show_profiler && profiler + MIN_SCENE_VIEW.y <= room.y;
        Self {
            scene: show_scene.then_some(scene),
            inspector: show_inspector.then_some(inspector),
            profiler: show_profiler.then_some(profiler),
            scene_max: (inner.x * MAX_SCENE_SHARE)
                .min(inner.x - inspector_share - MIN_SCENE_VIEW.x),
            inspector_max: (inner.x * MAX_INSPECTOR_SHARE)
                .min(inner.x - scene_share - MIN_SCENE_VIEW.x),
            profiler_max: (inner.y * MAX_PROFILER_SHARE).min(inner.y - MIN_SCENE_VIEW.y),
        }
    }
}

/// The scene's rect in fractions of the frame, both in points.
pub fn viewport_of(scene: Rect, frame: Rect) -> SceneViewport {
    let point = |p: egui::Pos2| glam::Vec2::new(p.x, p.y);
    SceneViewport::of_rect(
        point(scene.min),
        point(scene.max),
        point(frame.min),
        point(frame.max),
    )
}

/// A press on a handle, remembered across frames: where in the handle the pointer caught it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SplitterDrag {
    pub active: bool,
    grab: f32,
}

/// One dock edge's handle strip inside the scene rect.
#[derive(Clone, Copy, Debug)]
pub struct Splitter {
    pub id: Id,
    pub handle: Rect,
    /// 0 drags along x, 1 along y.
    pub axis: usize,
    /// +1 if dragging along the axis grows the dock, -1 if it shrinks it.
    pub growth: f32,
    pub min: f32,
    pub max: f32,
}

/// The pointer as the C++ splitter reads it.
struct PointerState {
    pos: Option<egui::Pos2>,
    primary_down: bool,
    primary_pressed: bool,
    other_down: bool,
}

fn pointer_state(ctx: &Context) -> PointerState {
    ctx.input(|input| {
        let pointer = &input.pointer;
        PointerState {
            pos: pointer.latest_pos(),
            primary_down: pointer.primary_down(),
            primary_pressed: pointer.primary_pressed(),
            other_down: pointer.secondary_down() || pointer.middle_down(),
        }
    })
}

impl Splitter {
    /// Drags `size`; true only on a real resize, so a click never rewrites a clamped preference.
    pub fn show(&self, ctx: &Context, size: &mut f32, drag: &mut SplitterDrag) -> bool {
        let layer = self.claim_handle(ctx);
        let pointer = pointer_state(ctx);
        let edge = self.handle.min[self.axis];
        let mut resized = false;
        if let (true, Some(pos)) = (drag.active, pointer.pos) {
            // From where the handle is now, so its edge stays under the pointer past a stop.
            let target = *size + (pos[self.axis] - drag.grab - edge) * self.growth;
            let next = target.clamp(self.min, self.min.max(self.max)).round();
            resized = next != *size;
            *size = next;
        }
        if !pointer.primary_down {
            drag.active = false;
        }
        // Only a free pointer hovers: a press held from the scene or a look drag must not light or grab one.
        let free = (!pointer.primary_down || pointer.primary_pressed) && !pointer.other_down;
        let hovered = pointer.pos.is_some_and(|pos| {
            !drag.active
                && free
                && !egui::Popup::is_any_open(ctx)
                && ctx.layer_id_at(pos) == Some(layer)
                && self.handle.contains(pos)
        });
        if hovered && pointer.primary_pressed {
            let pos = pointer.pos.unwrap_or_default();
            *drag = SplitterDrag {
                active: true,
                grab: pos[self.axis] - edge,
            };
        }
        if hovered || drag.active {
            self.highlight(ctx, layer, drag.active);
        }
        resized
    }

    /// An area over the handle, so a press there is egui's and never reaches the camera.
    fn claim_handle(&self, ctx: &Context) -> egui::LayerId {
        let area = egui::Area::new(self.id)
            .order(Order::Middle)
            .fixed_pos(self.handle.min)
            .constrain(false)
            .interactable(true);
        let layer = area.layer();
        area.show(ctx, |ui| {
            ui.allocate_exact_size(self.handle.size(), Sense::drag());
        });
        layer
    }

    fn highlight(&self, ctx: &Context, layer: egui::LayerId, active: bool) {
        let icon = if self.axis == 0 {
            CursorIcon::ResizeHorizontal
        } else {
            CursorIcon::ResizeVertical
        };
        ctx.set_cursor_icon(icon);
        let colour = if active {
            pal::ACCENT
        } else {
            pal::with_alpha(pal::ACCENT, 0.6)
        };
        ctx.layer_painter(layer)
            .rect_filled(self.handle, 0.0, colour);
    }
}

/// The three handles along the scene rect's dock edges, each `thickness` deep.
pub fn handles(scene: Rect, thickness: f32) -> [Rect; 3] {
    [
        Rect::from_min_max(scene.min, pos2(scene.left() + thickness, scene.bottom())),
        Rect::from_min_max(pos2(scene.right() - thickness, scene.top()), scene.max),
        Rect::from_min_max(
            pos2(scene.left() + thickness, scene.bottom() - thickness),
            pos2(scene.right() - thickness, scene.bottom()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    /// The room between the C++ bars: a 29-point menu bar and a 23-point status bar.
    fn inner(width: f32, height: f32) -> Vec2 {
        vec2(width, height - 29.0 - 23.0)
    }

    #[test]
    fn a_wide_window_shows_every_dock_at_its_default() {
        let plan = DockPlan::new(inner(1600.0, 900.0), &DockPrefs::default(), 1.0);
        assert_eq!(
            (plan.scene, plan.inspector, plan.profiler),
            (Some(248.0), Some(336.0), Some(172.0))
        );
        // 1600 x 900 pixels at 150 %: 1066.7 points wide, still room for all three.
        let scaled = DockPlan::new(inner(1600.0 / 1.5, 900.0 / 1.5), &DockPrefs::default(), 1.5);
        assert_eq!(scaled.scene, Some(248.0));
        assert_eq!(scaled.inspector, Some(336.0));
        assert_eq!(scaled.profiler, Some(172.0));
    }

    #[test]
    fn narrow_and_low_windows_drop_docks_in_the_cpp_order() {
        // 800 points: the inspector takes its 320-point share cap, the scene dock no longer fits beside it.
        let iframe = DockPlan::new(inner(800.0, 600.0), &DockPrefs::default(), 1.0);
        assert_eq!((iframe.scene, iframe.inspector), (None, Some(320.0)));
        assert_eq!(iframe.profiler, Some(172.0));
        // 800 x 600 pixels at 150 %: in 533 points the inspector's minimum no longer fits, the scene dock's does.
        let scaled = DockPlan::new(inner(800.0 / 1.5, 600.0 / 1.5), &DockPrefs::default(), 1.5);
        assert_eq!(
            (scaled.scene, scaled.inspector),
            (Some(MIN_SCENE_WIDTH), None)
        );
        // 45 % of the 348 points between the bars, on a whole pixel.
        assert_eq!(scaled.profiler, Some(235.0 / 1.5));
        let low = DockPlan::new(inner(1600.0, 360.0), &DockPrefs::default(), 1.0);
        assert_eq!(low.profiler, None, "a low window drops the profiler");
    }

    #[test]
    fn extents_hold_the_preference_between_the_minimum_and_the_share() {
        let prefs = DockPrefs {
            scene_width: 100.0,
            inspector_width: 900.0,
            profiler_height: 1000.0,
            ..DockPrefs::default()
        };
        let plan = DockPlan::new(inner(1600.0, 900.0), &prefs, 1.0);
        assert_eq!(plan.scene, Some(MIN_SCENE_WIDTH));
        assert_eq!(plan.inspector, Some(640.0));
        assert_eq!(plan.profiler, Some((848.0 * 0.45_f32).round()));
        assert_eq!(plan.scene_max, 1600.0 * 0.3);
        assert_eq!(plan.inspector_max, 1600.0 * 0.4);
        assert_eq!(plan.profiler_max, 848.0 * 0.45);
        let hidden = DockPrefs {
            show_inspector: false,
            ..DockPrefs::default()
        };
        let plan = DockPlan::new(inner(800.0, 600.0), &hidden, 1.0);
        assert_eq!((plan.scene, plan.inspector), (Some(240.0), None));
    }

    #[test]
    fn extents_land_on_whole_pixels() {
        let prefs = DockPrefs {
            scene_width: 247.4,
            ..DockPrefs::default()
        };
        let plan = DockPlan::new(inner(1600.0, 900.0), &prefs, 1.5);
        let pixels = plan.scene.unwrap() * 1.5;
        assert!((pixels - 371.0).abs() < 1e-3, "{pixels}");
    }

    #[test]
    fn handles_lie_inside_the_scene_edges() {
        let scene = Rect::from_min_max(pos2(248.0, 29.0), pos2(1264.0, 705.0));
        let [left, right, bottom] = handles(scene, 6.0);
        assert_eq!(
            left,
            Rect::from_min_max(pos2(248.0, 29.0), pos2(254.0, 705.0))
        );
        assert_eq!(right.left(), 1258.0);
        assert_eq!(
            bottom,
            Rect::from_min_max(pos2(254.0, 699.0), pos2(1258.0, 705.0))
        );
        let viewport = viewport_of(
            scene,
            Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0)),
        );
        assert_eq!(viewport.x0, 248.0 / 1600.0);
    }
}
