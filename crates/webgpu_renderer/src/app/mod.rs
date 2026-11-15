//! The app both shells run, the native viewer example and the browser demo: one frame order and one input path.

mod frame;
mod input;
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod web;

use anyhow::Context as _;
use winit::window::Window;

pub use frame::OffscreenFrame;

use crate::context::GpuContext;
use crate::render::forward::ForwardRenderer;
use crate::render::frame_clock::FrameClock;
use crate::render::ibl::EquirectImage;
use crate::render::overlay::Overlay;
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::controller::OrbitController;
use crate::scene::viewport::SceneViewport;
use crate::scene::CpuScene;
use crate::ui::editor::{DeviceInfo, Editor, FrameHistory, ModelSource};

/// What the shell can do for the app; each is off where the target cannot do it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCaps {
    /// Esc may close the app: a native window, never a browser tab.
    pub quit: bool,
    /// S asks the shell for a screenshot.
    pub screenshot: bool,
    /// F5 asks the shell to reload the shaders.
    pub hot_reload: bool,
}

/// What a frame asks its shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppSignal {
    Quit,
    Screenshot,
    ReloadShaders,
}

pub struct StudioApp {
    pub gpu: GpuContext,
    pub renderer: ForwardRenderer,
    pub camera: OrbitCamera,
    /// The loaded model as the editor names it; the shell sets it with each load.
    pub model: Option<ModelSource>,
    editor: Editor,
    frames: FrameHistory,
    device: DeviceInfo,
    format: wgpu::TextureFormat,
    tonemap: TonemapPass,
    overlay: Overlay,
    controller: OrbitController,
    home: OrbitCamera,
    caps: HostCaps,
    clock: FrameClock,
    /// When the scene's animation and auto-orbit started, in the shell's seconds.
    epoch_s: Option<f64>,
}

impl StudioApp {
    /// Builds the renderer for `gpu`'s surface and uploads `scene`; GPU timing starts here, so the profiler has data.
    pub fn new(
        window: &Window,
        gpu: GpuContext,
        scene: &CpuScene,
        home: OrbitCamera,
        caps: HostCaps,
    ) -> anyhow::Result<Self> {
        let config = gpu
            .surface_config
            .as_ref()
            .context("StudioApp needs a windowed GpuContext")?;
        let (format, size) = (config.format, [config.width, config.height]);
        let overlay = Overlay::new(window, &gpu.device, format);
        Ok(Self::with_overlay(
            gpu,
            scene,
            (home, caps),
            (format, size),
            overlay,
        ))
    }

    /// Without a window: frames go to [`Self::render_offscreen`] in `format`, for snapshots and tests.
    pub fn headless(
        gpu: GpuContext,
        scene: &CpuScene,
        home: OrbitCamera,
        caps: HostCaps,
        format: wgpu::TextureFormat,
    ) -> Self {
        let overlay = Overlay::headless(&gpu.device, format);
        Self::with_overlay(gpu, scene, (home, caps), (format, [1, 1]), overlay)
    }

    fn with_overlay(
        gpu: GpuContext,
        scene: &CpuScene,
        (home, caps): (OrbitCamera, HostCaps),
        (format, size): (wgpu::TextureFormat, [u32; 2]),
        overlay: Overlay,
    ) -> Self {
        let mut renderer = ForwardRenderer::new(&gpu, size[0], size[1]);
        renderer.enable_gpu_timing(&gpu);
        renderer.upload_scene(&gpu, scene);
        // IBL baked from the background's own sky; 256x128 suffices for low-frequency lighting.
        renderer.set_environment(&gpu, &EquirectImage::sky(256, 128));
        Self {
            tonemap: TonemapPass::new(&gpu, format),
            device: DeviceInfo::of(&gpu.adapter_info),
            controller: OrbitController::default(),
            camera: home.clone(),
            model: None,
            editor: Editor::default(),
            frames: FrameHistory::default(),
            home,
            caps,
            format,
            clock: FrameClock::new(),
            epoch_s: None,
            gpu,
            renderer,
            overlay,
        }
    }

    /// Swaps in `scene` and starts its camera and animation clock over.
    pub fn load_scene(&mut self, scene: &CpuScene) {
        self.renderer.upload_scene(&self.gpu, scene);
        self.camera = self.home.clone();
        self.controller = OrbitController::default();
        self.epoch_s = None;
    }

    /// egui wants the keys: a field or a widget has focus, so a browser key belongs to egui, not the page.
    pub fn wants_keyboard(&self) -> bool {
        self.overlay.context().egui_wants_keyboard_input()
    }

    /// The camera over the whole frame, for an offscreen render that no dock covers.
    pub fn full_frame_camera(&self) -> OrbitCamera {
        OrbitCamera {
            viewport: SceneViewport::FULL,
            ..self.camera.clone()
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// The editor's own state: its docks, selection and windows.
    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }
}
