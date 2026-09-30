//! Browser (wasm32/WebGPU) demo entry point; the page is crates/webgpu_renderer/web/index.html.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::prelude::*;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowExtWebSys};
use winit::window::{Window, WindowId};

use crate::asset::gltf_loader::load_gltf_slice;
use crate::context::GpuContext;
use crate::render::forward::ForwardRenderer;
use crate::render::frame_clock::FrameClock;
use crate::render::overlay::{Overlay, OverlayControls};
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::controller::OrbitController;

const DEMO_SCENE: &[u8] = include_bytes!("../tests/assets/cube_on_plane.gltf");

/// Self-contained showcase scenes, indexed like web/index.html's `<option>`s; 0 is the startup scene.
const DEMO_SCENES: &[(&str, &[u8])] = &[
    ("cube_on_plane (shadows)", DEMO_SCENE),
    (
        "cube_textured (PBR textures)",
        include_bytes!("../tests/assets/cube_textured.gltf"),
    ),
    (
        "cube_animated (animation)",
        include_bytes!("../tests/assets/cube_animated.gltf"),
    ),
    (
        "cube_morph (morph targets)",
        include_bytes!("../tests/assets/cube_morph.gltf"),
    ),
    (
        "cube_unlit (KHR_materials_unlit)",
        include_bytes!("../tests/assets/cube_unlit.gltf"),
    ),
];

/// A dropped model parked until the render loop takes it; the read is async and GPU state may lag.
type DroppedScene = Rc<RefCell<Option<(String, Vec<u8>)>>>;

thread_local! {
    /// The drag-drop slot, shared so [`select_demo_scene`] reuses the dropped-file path.
    static SCENE_SLOT: RefCell<Option<DroppedScene>> = const { RefCell::new(None) };
}

/// Drag-and-drop via the DOM File API, since winit's web backend never sends `DroppedFile`.
fn install_drop_zone(document: &web_sys::Document, slot: DroppedScene) {
    // An uncancelled dragover lets the browser navigate away to the file.
    let on_dragover = Closure::<dyn FnMut(web_sys::DragEvent)>::new(|event: web_sys::DragEvent| {
        event.prevent_default();
    });
    let _ =
        document.add_event_listener_with_callback("dragover", on_dragover.as_ref().unchecked_ref());
    on_dragover.forget();

    let on_drop =
        Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |event: web_sys::DragEvent| {
            event.prevent_default();
            let Some(file) = event
                .data_transfer()
                .and_then(|transfer| transfer.files())
                .and_then(|files| files.get(0))
            else {
                return;
            };
            let name = file.name();
            let slot = Rc::clone(&slot);
            wasm_bindgen_futures::spawn_local(async move {
                match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buffer) => {
                        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                        slot.borrow_mut().replace((name, bytes));
                    }
                    Err(err) => log::error!("Reading the dropped file failed: {err:?}"),
                }
            });
        });
    let _ = document.add_event_listener_with_callback("drop", on_drop.as_ref().unchecked_ref());
    on_drop.forget();
}

/// Monotonic seconds for [`FrameClock::tick_at`]; `std::time::Instant` panics on wasm32.
fn performance_now_seconds() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now() / 1000.0)
        .unwrap_or(0.0)
}

/// Sizes the canvas backing store to CSS size x devicePixelRatio, which winit skips on web.
fn sync_canvas_backing_size(canvas: &web_sys::HtmlCanvasElement) -> (u32, u32) {
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let width = ((canvas.client_width().max(1) as f64) * dpr) as u32;
    let height = ((canvas.client_height().max(1) as f64) * dpr) as u32;
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    (width.max(1), height.max(1))
}

struct GpuState {
    gpu: GpuContext,
    renderer: ForwardRenderer,
    tonemap: TonemapPass,
    overlay: Overlay,
    controls: OverlayControls,
}

#[derive(Default)]
struct DemoApp {
    window: Option<Arc<Window>>,
    /// Filled asynchronously once the WebGPU adapter/device resolve.
    state: Rc<RefCell<Option<GpuState>>>,
    /// Filled asynchronously by the drop-zone listeners.
    dropped_scene: DroppedScene,
    controller: OrbitController,
    camera: OrbitCamera,
    frame: u64,
    frame_clock: FrameClock,
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(winit::dpi::LogicalSize::new(1024, 640)),
                )
                .expect("failed to create window"),
        );

        // CSS drives the canvas layout size; the backing store follows it.
        let canvas = window.canvas().expect("winit window must expose a canvas");
        let style = canvas.style();
        let _ = style.set_property("width", "100%");
        let _ = style.set_property("height", "100%");
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("no document");
        let mount = document
            .get_element_by_id("demo")
            .unwrap_or_else(|| document.body().expect("no body").into());
        mount
            .append_child(&canvas)
            .expect("failed to append canvas");
        install_drop_zone(&document, Rc::clone(&self.dropped_scene));
        // The scene picker and drag-drop share one slot.
        SCENE_SLOT.with(|slot| {
            slot.borrow_mut().replace(Rc::clone(&self.dropped_scene));
        });
        let (initial_width, initial_height) = sync_canvas_backing_size(&canvas);

        // WebGPU init is async-only in browsers.
        let state_slot = Rc::clone(&self.state);
        let init_window = Arc::clone(&window);
        wasm_bindgen_futures::spawn_local(async move {
            let mut gpu = GpuContext::new_windowed_async(Arc::clone(&init_window))
                .await
                .expect("WebGPU init failed (does this browser support WebGPU?)");
            let format = gpu.surface_format().expect("windowed context has a format");
            // The context read inner_size before the canvas size propagated through winit.
            gpu.resize(initial_width, initial_height);
            let mut renderer = ForwardRenderer::new(&gpu, initial_width, initial_height);
            let tonemap = TonemapPass::new(&gpu, format);

            let scene = load_gltf_slice(DEMO_SCENE).expect("embedded demo scene must load");
            renderer.upload_scene(&gpu, &scene);

            // IBL from the procedural sky, so the showcase has real environment reflections.
            let sky_env = crate::render::ibl::EquirectImage::sky(256, 128);
            renderer.set_environment(&gpu, &sky_env);

            let overlay = Overlay::new(&init_window, &gpu.device, format);
            let controls = OverlayControls::from_light(
                renderer.light_dir_ambient,
                renderer.light_color_intensity,
            );

            state_slot.borrow_mut().replace(GpuState {
                gpu,
                renderer,
                tonemap,
                overlay,
                controls,
            });
            init_window.request_redraw();
        });

        self.window = Some(window);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let consumed = match (self.state.borrow_mut().as_mut(), self.window.as_ref()) {
            (Some(state), Some(window)) => state.overlay.on_event(window, &event),
            _ => false,
        };
        if consumed {
            self.controller.note_consumed_event(&event);
        } else {
            self.controller.handle_event(&event, &mut self.camera);
        }

        match event {
            WindowEvent::Resized(size) => {
                if let Some(state) = self.state.borrow_mut().as_mut() {
                    state.gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(window) = self.window.as_ref() else {
                    return;
                };
                let mut state_ref = self.state.borrow_mut();
                let Some(state) = state_ref.as_mut() else {
                    return;
                };

                // As the native viewer: swap and re-frame, keeping the scene on a parse error.
                if let Some((name, bytes)) = self.dropped_scene.borrow_mut().take() {
                    match load_gltf_slice(&bytes) {
                        Ok(scene) => {
                            log::info!(
                                "Loaded {name}: {} primitives, {} triangles",
                                scene.primitives.len(),
                                scene.triangle_count()
                            );
                            state.renderer.upload_scene(&state.gpu, &scene);
                            self.camera = OrbitCamera::default();
                            self.controller = OrbitController::default();
                            self.frame = 0;
                        }
                        Err(err) => log::error!(
                            "Failed to load {name}: {err:#} (external .bin/textures cannot \
                             be resolved from a drop - use a self-contained .glb)"
                        ),
                    }
                }

                // Follow the CSS layout size every frame.
                if let Some(canvas) = window.canvas() {
                    let (width, height) = sync_canvas_backing_size(&canvas);
                    let configured = state
                        .gpu
                        .surface_config
                        .as_ref()
                        .map(|c| (c.width, c.height));
                    if configured != Some((width, height)) {
                        state.gpu.resize(width, height);
                    }
                }

                let Some(surface) = state.gpu.surface.as_ref() else {
                    return;
                };

                // std::time::Instant is unavailable on wasm32: animate by frame.
                self.frame += 1;
                if state.renderer.has_animations() {
                    state.renderer.set_animation_time(self.frame as f32 / 60.0);
                }
                if self.controller.auto_orbit {
                    self.camera.yaw_deg = 45.0 + self.frame as f32 * 0.5;
                    self.camera.radius = 6.0;
                    self.camera.pitch_deg = 35.0;
                }
                // rAF runs at the display rate, so adaptation needs real time, not frames.
                state.renderer.frame_delta_seconds =
                    self.frame_clock.tick_at(performance_now_seconds());

                // Keep this acquire identical to the native viewer's.
                let frame = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        // Never abort on Outdated/Lost: reconfigure and retry.
                        state.gpu.reconfigure();
                        window.request_redraw();
                        return;
                    }
                    other => {
                        log::error!("Surface acquire failed: {other:?}");
                        return;
                    }
                };
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let (width, height) = (frame.texture.width(), frame.texture.height());
                let GpuState {
                    gpu,
                    renderer,
                    tonemap,
                    overlay,
                    controls,
                } = state;
                renderer.render_tonemapped(gpu, tonemap, &view, width, height, &self.camera);

                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("overlay_encoder"),
                        });
                // Cull counts for the overlay's occlusion toggle.
                controls.occlusion_stats = Some(renderer.occlusion_cull_stats());
                let mut changed = false;
                overlay.render(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    window,
                    &view,
                    width,
                    height,
                    |ctx| {
                        changed |= controls.ui(ctx);
                    },
                );
                gpu.queue.submit(Some(encoder.finish()));
                if changed {
                    let dir = controls.light_direction();
                    renderer.light_dir_ambient =
                        glam::Vec4::new(dir.x, dir.y, dir.z, controls.ambient);
                    renderer.light_color_intensity.w = controls.intensity;
                    renderer.bloom_strength = controls.bloom;
                    renderer.ssao_strength = controls.ssao;
                    renderer.exposure_ev = controls.exposure_ev;
                    renderer.occlusion_queries_enabled = controls.occlusion_culling;
                }

                window.pre_present_notify();
                // wgpu 30: presentation moved to the queue.
                gpu.queue.present(frame);
                window.request_redraw();
            }
            _ => {}
        }
    }
}

/// Switches the demo to a bundled scene through the drag-drop slot; bad or early calls are ignored.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn select_demo_scene(index: u32) {
    let Some(&(name, bytes)) = DEMO_SCENES.get(index as usize) else {
        log::warn!("select_demo_scene: scene index {index} out of range - ignored");
        return;
    };
    SCENE_SLOT.with(|slot| match slot.borrow().as_ref() {
        Some(scene_slot) => {
            scene_slot
                .borrow_mut()
                .replace((name.to_string(), bytes.to_vec()));
        }
        None => log::warn!("select_demo_scene: called before the demo initialized - ignored"),
    });
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.spawn_app(DemoApp::default());
}
