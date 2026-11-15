//! Minimal glTF viewer: `cargo run -p kataglyphis_webgpu_renderer --example viewer [model.gltf]`
//! Drag orbits, wheel zooms, Esc quits, S screenshots, R reloads shaders, drag-and-drop loads.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use kataglyphis_webgpu_renderer::{
    load_gltf, AppSignal, GpuContext, HostCaps, OrbitCamera, StudioApp,
};

/// The C++ editor's start size, in logical pixels.
const WINDOW_SIZE: winit::dpi::LogicalSize<u32> = winit::dpi::LogicalSize::new(1600, 900);

struct Viewer {
    model_path: PathBuf,
    window: Option<Arc<Window>>,
    app: Option<StudioApp>,
    started: Instant,
    frame: u64,
    shader_mtimes: Vec<(PathBuf, std::time::SystemTime)>,
}

fn shader_paths() -> Vec<PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    vec![dir.join("forward.wgsl"), dir.join("sky.wgsl")]
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
    fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            window: None,
            app: None,
            started: Instant::now(),
            frame: 0,
            shader_mtimes: shader_mtimes(),
        }
    }

    /// Loads a different model at runtime (drag & drop).
    fn load_model(&mut self, path: PathBuf) {
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match load_gltf(&path) {
            Ok(scene) => {
                log::info!(
                    "Loaded {}: {} primitives, {} triangles",
                    path.display(),
                    scene.primitives.len(),
                    scene.triangle_count()
                );
                app.load_scene(&scene);
                self.model_path = path;
            }
            Err(err) => log::error!("Failed to load {}: {err:#}", path.display()),
        }
    }

    /// Hot shader reload; invalid WGSL keeps the previous pipelines running.
    fn reload_shaders(&mut self) {
        let Some(app) = self.app.as_mut() else {
            return;
        };
        let paths = shader_paths();
        let (Ok(forward_src), Ok(sky_src)) = (
            std::fs::read_to_string(&paths[0]),
            std::fs::read_to_string(&paths[1]),
        ) else {
            log::error!("Failed to read shader sources for reload");
            return;
        };
        match app
            .renderer
            .reload_shaders(&app.gpu, &forward_src, &sky_src)
        {
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

    /// S key: renders one frame offscreen and writes screenshots/NNN.png.
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
                        .with_title("Kataglyphis WebGPU glTF viewer (drop a .gltf/.glb to load)")
                        .with_inner_size(WINDOW_SIZE),
                )
                .expect("failed to create window"),
        );
        let gpu = GpuContext::new_windowed(Arc::clone(&window)).expect("failed to init wgpu");
        let scene = load_gltf(&self.model_path)
            .unwrap_or_else(|err| panic!("failed to load {}: {err:#}", self.model_path.display()));
        log::info!(
            "Loaded {}: {} primitives, {} triangles",
            self.model_path.display(),
            scene.primitives.len(),
            scene.triangle_count()
        );
        let caps = HostCaps {
            quit: true,
            screenshot: true,
            hot_reload: true,
        };
        let app = StudioApp::new(&window, gpu, &scene, OrbitCamera::default(), caps)
            .expect("failed to set up the renderer");
        self.app = Some(app);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
            app.window_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::DroppedFile(path) => self.load_model(path),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let model_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf")
        });

    let event_loop = EventLoop::new()?;
    let mut viewer = Viewer::new(model_path);
    event_loop.run_app(&mut viewer)?;
    Ok(())
}
