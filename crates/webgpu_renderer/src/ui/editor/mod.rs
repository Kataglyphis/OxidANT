//! The Studio Pro editor, BeschleunigerBallett's docked C++ editor in egui; one module per dock.

mod chip;
mod docks;
mod host;
mod inspector;
mod keys;
mod layout;
mod library;
mod menu_bar;
mod outliner;
mod profiler;
mod render_mode;
mod status_bar;
mod windows;

use egui::{Context, Rect, Ui};

pub use host::{
    DeviceInfo, FrameHistory, ModelSource, RendererSettings, SceneStats, FRAME_HISTORY,
    FRAME_RATE_WINDOW,
};
pub use keys::{KeyBinding, Shortcut, KEY_BINDINGS};
pub use layout::{DockPlan, DockPrefs, SplitterDrag};

use crate::app::HostCaps;
use crate::scene::camera::OrbitCamera;
use crate::scene::viewport::SceneViewport;
use crate::ui::icons::StudioIcon;

/// The brand the C++ editor writes in its menu bar and About.
pub const BRAND: &str = "BeschleunigerBallett";

/// The outliner's objects; each opens one inspector tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneObject {
    Model,
    Sun,
    Camera,
    Skybox,
    Clouds,
    Renderer,
}

impl SceneObject {
    pub const ALL: [Self; 6] = [
        Self::Model,
        Self::Sun,
        Self::Camera,
        Self::Skybox,
        Self::Clouds,
        Self::Renderer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Sun => "Sun",
            Self::Camera => "Camera",
            Self::Skybox => "Skybox",
            Self::Clouds => "Clouds",
            Self::Renderer => "Renderer",
        }
    }

    pub fn icon(self) -> StudioIcon {
        match self {
            Self::Model => StudioIcon::Cube,
            Self::Sun => StudioIcon::Sun,
            Self::Camera => StudioIcon::Camera,
            Self::Skybox => StudioIcon::Mountain,
            Self::Clouds => StudioIcon::Cloud,
            Self::Renderer => StudioIcon::Layers,
        }
    }

    /// The C++ `inspectorTabFor`: the sky's two objects share one tab.
    pub fn tab(self) -> InspectorTab {
        match self {
            Self::Model => InspectorTab::Model,
            Self::Sun => InspectorTab::Sun,
            Self::Camera => InspectorTab::Camera,
            Self::Skybox | Self::Clouds => InspectorTab::Sky,
            Self::Renderer => InspectorTab::Renderer,
        }
    }
}

/// The inspector's tabs, in the C++ order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InspectorTab {
    Model,
    Sun,
    Camera,
    Sky,
    Renderer,
}

impl InspectorTab {
    pub const ALL: [Self; 5] = [
        Self::Model,
        Self::Sun,
        Self::Camera,
        Self::Sky,
        Self::Renderer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Sun => "Sun",
            Self::Camera => "Camera",
            Self::Sky => "Sky",
            Self::Renderer => "Renderer",
        }
    }

    /// The object a click on the tab selects.
    pub fn primary(self) -> SceneObject {
        match self {
            Self::Model => SceneObject::Model,
            Self::Sun => SceneObject::Sun,
            Self::Camera => SceneObject::Camera,
            Self::Sky => SceneObject::Skybox,
            Self::Renderer => SceneObject::Renderer,
        }
    }
}

/// What the editor asks of the host; the expensive or shell-side work happens there, after the pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorAction {
    Quit,
    Screenshot,
    ReloadShaders,
}

/// One pass's result: where the scene shows through, and what to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorOutput {
    /// The free rect between the docks; the whole frame while the editor is hidden.
    pub viewport: SceneViewport,
    pub actions: Vec<EditorAction>,
}

/// What the editor reads and edits in one pass.
pub struct EditorContext<'a> {
    pub settings: &'a mut RendererSettings,
    pub camera: &'a mut OrbitCamera,
    pub stats: &'a SceneStats,
    pub frames: &'a FrameHistory,
    pub device: &'a DeviceInfo,
    pub model: Option<&'a ModelSource>,
    pub caps: HostCaps,
    /// The output's size in physical pixels.
    pub surface_px: [u32; 2],
}

/// The floating windows' open flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Windows {
    about: bool,
    style: bool,
    inspection: bool,
}

/// The editor's own state; everything it edits lives in the [`EditorContext`].
#[derive(Debug)]
pub struct Editor {
    visible: bool,
    pub prefs: DockPrefs,
    /// The scene, inspector and profiler splitters.
    drags: [SplitterDrag; 3],
    selection: SceneObject,
    windows: Windows,
    profiler_tab: usize,
    library_filter: String,
    /// What this pass asks of the host, handed out with its output.
    actions: Vec<EditorAction>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            visible: true,
            prefs: DockPrefs::default(),
            drags: [SplitterDrag::default(); 3],
            selection: SceneObject::Model,
            windows: Windows::default(),
            profiler_tab: 0,
            library_filter: String::new(),
            actions: Vec::new(),
        }
    }
}

impl Editor {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn selection(&self) -> SceneObject {
        self.selection
    }

    /// Selects `object` in the outliner, which opens its inspector tab.
    pub fn select(&mut self, object: SceneObject) {
        self.selection = object;
        self.prefs.show_inspector = true;
    }

    pub fn show_about(&mut self) {
        self.windows.about = true;
    }

    pub fn hide_about(&mut self) {
        self.windows.about = false;
    }

    pub fn about_open(&self) -> bool {
        self.windows.about
    }

    /// Read before the pass: Esc quits only when nothing had focus and no menu or modal was open.
    pub fn esc_armed(&self, ctx: &Context) -> bool {
        !ctx.egui_wants_keyboard_input() && !egui::Popup::is_any_open(ctx) && !self.windows.about
    }

    /// The default dock sizes, every dock shown, the UI at 100 %.
    pub fn reset_layout(&mut self, ctx: &Context) {
        self.prefs = DockPrefs::default();
        ctx.set_zoom_factor(1.0);
    }

    /// One pass: keys, bars, docks, splitters, the chip and the floating windows; the root's leftover rect is the scene.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        cx: &mut EditorContext<'_>,
        esc_armed: bool,
    ) -> EditorOutput {
        self.actions = keys::shortcuts(ui.ctx(), esc_armed, cx.caps, &mut self.visible);
        let frame = ui.ctx().viewport_rect();
        if !self.visible || ui.available_rect_before_wrap().area() <= 0.0 {
            self.hidden(ui.ctx());
            return EditorOutput {
                viewport: SceneViewport::FULL,
                actions: std::mem::take(&mut self.actions),
            };
        }
        menu_bar::show(ui, self, cx);
        status_bar::show(ui, cx);
        let scene = self.docks(ui, cx);
        chip::paint(ui, scene, cx);
        windows::show(ui.ctx(), self, cx);
        EditorOutput {
            viewport: layout::viewport_of(scene, frame),
            actions: std::mem::take(&mut self.actions),
        }
    }

    /// A hidden editor holds no focus and no drag, so neither resumes when it comes back.
    fn hidden(&mut self, ctx: &Context) {
        self.drags = [SplitterDrag::default(); 3];
        self.windows.about = false;
        ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }

    /// The docks in the C++ order, then their splitters; returns the scene rect they leave.
    fn docks(&mut self, ui: &mut Ui, cx: &mut EditorContext<'_>) -> Rect {
        let inner = ui.available_rect_before_wrap();
        let plan = DockPlan::new(inner.size(), &self.prefs, ui.ctx().pixels_per_point());
        if let Some(width) = plan.scene {
            docks::dock(ui, docks::Side::Left, width, |ui| {
                outliner::show(ui, self, cx)
            });
        }
        if let Some(width) = plan.inspector {
            docks::dock(ui, docks::Side::Right, width, |ui| {
                inspector::show(ui, self, cx)
            });
        }
        if let Some(height) = plan.profiler {
            docks::dock(ui, docks::Side::Bottom, height, |ui| {
                profiler::show(ui, self, cx)
            });
        }
        let scene = ui.available_rect_before_wrap();
        docks::splitters(ui.ctx(), self, &plan, scene);
        scene
    }
}

#[cfg(test)]
mod tests;
