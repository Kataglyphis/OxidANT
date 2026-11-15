//! The app both shells run, the native viewer example and the browser demo: one frame order and one input path.

mod frame;
mod input;

use anyhow::Context as _;
use winit::window::Window;

use crate::context::GpuContext;
use crate::render::forward::ForwardRenderer;
use crate::render::frame_clock::FrameClock;
use crate::render::ibl::EquirectImage;
use crate::render::overlay::{Overlay, OverlayControls};
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::controller::OrbitController;
use crate::scene::viewport::SceneViewport;
use crate::scene::CpuScene;

/// What the shell can do for the app; each is off where the target cannot do it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCaps {
    /// Esc may close the app: a native window, never a browser tab.
    pub quit: bool,
    /// S asks the shell for a screenshot.
    pub screenshot: bool,
    /// R asks the shell to reload the shaders.
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
    tonemap: TonemapPass,
    overlay: Overlay,
    controls: OverlayControls,
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
        let (format, width, height) = (config.format, config.width, config.height);
        let mut renderer = ForwardRenderer::new(&gpu, width, height);
        renderer.enable_gpu_timing(&gpu);
        renderer.upload_scene(&gpu, scene);
        // IBL baked from the background's own sky; 256x128 suffices for low-frequency lighting.
        renderer.set_environment(&gpu, &EquirectImage::sky(256, 128));
        let tonemap = TonemapPass::new(&gpu, format);
        let overlay = Overlay::new(window, &gpu.device, format);
        Ok(Self {
            controls: OverlayControls::from_renderer(&renderer),
            controller: OrbitController::default(),
            camera: home.clone(),
            home,
            caps,
            clock: FrameClock::new(),
            epoch_s: None,
            gpu,
            renderer,
            tonemap,
            overlay,
        })
    }

    /// Swaps in `scene` and starts its camera and animation clock over.
    pub fn load_scene(&mut self, scene: &CpuScene) {
        self.renderer.upload_scene(&self.gpu, scene);
        self.camera = self.home.clone();
        self.controller = OrbitController::default();
        self.epoch_s = None;
    }

    /// The camera over the whole frame, for an offscreen render that no dock covers.
    pub fn full_frame_camera(&self) -> OrbitCamera {
        OrbitCamera {
            viewport: SceneViewport::FULL,
            ..self.camera.clone()
        }
    }
}
