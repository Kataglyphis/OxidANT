//! The Studio editor: `cargo run -p kataglyphis_webgpu_renderer --example viewer [model]`; without one the fetched Colosseum.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use kataglyphis_webgpu_renderer::app::discovery::{native_startup, skybox_dir};
use kataglyphis_webgpu_renderer::app::library::EMBEDDED_SCENES;
use kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf_slice;
use kataglyphis_webgpu_renderer::render::forward::ShaderSources;
use kataglyphis_webgpu_renderer::ui::ModelSource;
use kataglyphis_webgpu_renderer::{
    AppSignal, GpuContext, HostCaps, Notice, OrbitCamera, StudioApp,
};

/// The C++ editor's start size, in logical pixels.
const WINDOW_SIZE: winit::dpi::LogicalSize<u32> = winit::dpi::LogicalSize::new(1600, 900);

struct Viewer {
    /// The command line's model, first in the startup chain.
    model_path: Option<PathBuf>,
    window: Option<Arc<Window>>,
    app: Option<StudioApp>,
    started: Instant,
    frame: u64,
    shader_mtimes: Vec<(PathBuf, std::time::SystemTime)>,
}

fn shader_paths() -> Vec<PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    ShaderSources::FILES.map(|file| dir.join(file)).to_vec()
}

fn shader_mtimes() -> Vec<(PathBuf, std::time::SystemTime)> {
    shader_paths()
        .into_iter()
        .filter_map(|p| {
            let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((p, mtime))
        })
        .collect()
}

impl Viewer {
    fn new(model_path: Option<PathBuf>) -> Self {
        Self {
            model_path,
            window: None,
            app: None,
            started: Instant::now(),
            frame: 0,
            shader_mtimes: shader_mtimes(),
        }
    }

    /// Hot shader reload; invalid WGSL keeps the previous pipelines running.
    fn reload_shaders(&mut self) {
        let Some(app) = self.app.as_mut() else {
            return;
        };
        let read: Result<Vec<String>, _> =
            shader_paths().iter().map(std::fs::read_to_string).collect();
        let files = read
            .ok()
            .and_then(|files| <[String; 4]>::try_from(files).ok());
        let Some([forward, sky, cloud_march, cloud_noise]) = files else {
            log::error!("Failed to read shader sources for reload");
            return;
        };
        let sources = ShaderSources {
            forward: &forward,
            sky: &sky,
            cloud_march: &cloud_march,
            cloud_noise: &cloud_noise,
        };
        match app.renderer.reload_shaders(&app.gpu, &sources) {
            Ok(()) => log::info!("Shaders reloaded"),
            Err(err) => log::error!("{err:#}"),
        }
        self.shader_mtimes = shader_mtimes();
    }

    /// Polls shader file mtimes (~every 30 frames) and reloads on change.
    fn poll_shader_changes(&mut self) {
        self.frame += 1;
        if !self.frame.is_multiple_of(30) {
            return;
        }
        let current = shader_mtimes();
        if current != self.shader_mtimes {
            log::info!("Shader change detected - reloading");
            self.reload_shaders();
        }
    }

    /// F12: renders one frame offscreen and writes screenshots/NNN.png.
    fn save_screenshot(&mut self) {
        let Some(app) = self.app.as_mut() else {
            return;
        };
        let (width, height) = (1920u32, 1080u32);
        // The whole frame: no dock covers an offscreen shot, so the camera must not centre on the free rect.
        let camera = app.full_frame_camera();
        match app
            .renderer
            .render_to_pixels(&app.gpu, width, height, &camera)
        {
            Ok(pixels) => {
                let dir = std::path::Path::new("screenshots");
                let _ = std::fs::create_dir_all(dir);
                let mut index = 0;
                let path = loop {
                    let candidate = dir.join(format!("screenshot-{index:03}.png"));
                    if !candidate.exists() {
                        break candidate;
                    }
                    index += 1;
                };
                match image::save_buffer(&path, &pixels, width, height, image::ColorType::Rgba8) {
                    Ok(()) => log::info!("Saved {}", path.display()),
                    Err(err) => log::error!("Failed to save screenshot: {err}"),
                }
            }
            Err(err) => log::error!("Screenshot render failed: {err:#}"),
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_shader_changes();
        let (Some(window), Some(app)) = (self.window.as_ref(), self.app.as_mut()) else {
            return;
        };
        for signal in app.frame(window, self.started.elapsed().as_secs_f64()) {
            match signal {
                AppSignal::Quit => event_loop.exit(),
                AppSignal::Screenshot => self.save_screenshot(),
                AppSignal::ReloadShaders => self.reload_shaders(),
            }
        }
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(
                            "BeschleunigerBallett WebGPU (drop a .gltf, .glb or .obj to load)",
                        )
                        .with_inner_size(WINDOW_SIZE),
                )
                .expect("failed to create window"),
        );
        let gpu = GpuContext::new_windowed(Arc::clone(&window)).expect("failed to init wgpu");
        let caps = HostCaps {
            quit: true,
            screenshot: true,
            hot_reload: true,
        };
        // The embedded cube shows at once; the startup scene replaces it when its load is done.
        let (name, cube) = EMBEDDED_SCENES[0];
        let scene = load_gltf_slice(cube).unwrap_or_default();
        let mut app = StudioApp::new(&window, gpu, &scene, OrbitCamera::default(), caps)
            .expect("failed to set up the renderer");
        app.model = Some(ModelSource {
            name: name.to_owned(),
            path: format!("{name}.gltf (embedded)"),
        });
        start_scene(&mut app, self.model_path.as_deref());
        self.app = Some(app);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
            app.window_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::DroppedFile(path) => {
                if let Some(app) = self.app.as_mut() {
                    app.open_path(&path);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    /// Raw mouse motion turns a right-drag look where the platform holds the cursor still.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let Some(app) = self.app.as_mut() {
            app.device_event(&event);
        }
    }
}

/// The library, the C++ skybox and the startup chain: the command line, `KATAGLYPHIS_MODEL_OVERRIDE`, the fetched Colosseum.
fn start_scene(app: &mut StudioApp, model_path: Option<&Path>) {
    let (library, startup) = native_startup(model_path);
    app.set_library(library, None);
    if let Some(dir) = skybox_dir(&std::env::current_dir().unwrap_or_default()) {
        app.load_skybox(&dir);
    }
    if let Some(path) = &startup.path {
        app.open_path(path);
    }
    if !startup.notes.is_empty() {
        app.set_notice(Some(Notice::Hint(startup.notes.join("  ·  "))));
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let model_path = std::env::args().nth(1).map(PathBuf::from);

    let event_loop = EventLoop::new()?;
    let mut viewer = Viewer::new(model_path);
    event_loop.run_app(&mut viewer)?;
    Ok(())
}
