//! Browser (wasm32/WebGPU) demo entry point; the page is crates/webgpu_renderer/web/index.html.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::prelude::*;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowExtWebSys};
use winit::window::{Window, WindowId};

use crate::app::library::{ModelLibrary, EMBEDDED_SCENES};
use crate::app::{web, HostCaps, StudioApp};
use crate::asset::gltf_loader::load_gltf_slice;
use crate::asset::skybox::{CubemapFaces, SKYBOX_FACES, SKYBOX_NAME};
use crate::context::GpuContext;
use crate::scene::camera::OrbitCamera;
use crate::ui::editor::{DockPrefs, ModelSource};

/// What the page asked to load, parked until the render loop takes it: a dropped file, or a library row.
enum PageRequest {
    Bytes(String, Vec<u8>),
    Row(usize),
}

/// The slot the drop zone and the page's scene picker fill.
type DroppedScene = Rc<RefCell<Option<PageRequest>>>;

/// The app once the device resolves, shared with the render loop.
type AppSlot = Rc<RefCell<Option<StudioApp>>>;

/// The skybox faces once their fetch finishes, taken by the render loop.
type SkyboxSlot = Rc<RefCell<Option<CubemapFaces>>>;

thread_local! {
    /// The drag-drop slot, shared so [`select_demo_scene`] reuses the dropped-file path.
    static SCENE_SLOT: RefCell<Option<DroppedScene>> = const { RefCell::new(None) };
    /// The app, for [`set_demo_view`] and [`demo_view`]; a page call runs between frames, never inside one.
    static APP_SLOT: RefCell<Option<AppSlot>> = const { RefCell::new(None) };
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
            web::notify_page(web::PAGE_DROPPED, &name);
            let slot = Rc::clone(&slot);
            wasm_bindgen_futures::spawn_local(async move {
                match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buffer) => {
                        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                        slot.borrow_mut().replace(PageRequest::Bytes(name, bytes));
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

/// The canvas's CSS size x devicePixelRatio; configuring the surface sizes its backing store, within the device limit.
fn canvas_device_size(canvas: &web_sys::HtmlCanvasElement) -> (u32, u32) {
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let width = ((canvas.client_width().max(1) as f64) * dpr) as u32;
    let height = ((canvas.client_height().max(1) as f64) * dpr) as u32;
    (width.max(1), height.max(1))
}

/// The showcase framing: a little further out and higher than the native viewer's.
fn home_camera() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 35.0,
        ..OrbitCamera::default()
    }
}

/// A canvas-backed window, mounted in `#demo` (or the body), its CSS size driving the layout.
fn create_canvas_window(
    event_loop: &ActiveEventLoop,
    egui_wants_keys: Rc<Cell<bool>>,
) -> Arc<Window> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_inner_size(winit::dpi::LogicalSize::new(1024, 640)),
            )
            .expect("failed to create window"),
    );
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
    web::leave_browser_keys(&mount, egui_wants_keys);
    window
}

/// Where the browser fetches the Colosseum from: `?scene-base=<folder URL>`, else beside the page.
fn scene_base() -> String {
    let search = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    let value = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("scene-base="))
        .and_then(|value| js_sys::decode_uri_component(value).ok())
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    if value.is_empty() || value.ends_with('/') {
        value
    } else {
        format!("{value}/")
    }
}

/// One file's bytes; any failure is a message, since a missing face only keeps the procedural sky.
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response: web_sys::Response =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
            .await
            .and_then(JsCast::dyn_into)
            .map_err(|error| format!("{url}: {error:?}"))?;
    if !response.ok() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let buffer = response
        .array_buffer()
        .map_err(|error| format!("{url}: {error:?}"))?;
    let buffer = wasm_bindgen_futures::JsFuture::from(buffer)
        .await
        .map_err(|error| format!("{url}: {error:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// The C++ engine's sky, which the docs build stages in a folder beside the page; without it the procedural sky stays.
fn spawn_skybox_fetch(slot: SkyboxSlot) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut files = Vec::with_capacity(SKYBOX_FACES.len());
        for face in SKYBOX_FACES {
            match fetch_bytes(&format!("{}{SKYBOX_NAME}/{face}", scene_base())).await {
                Ok(bytes) => files.push(bytes),
                Err(error) => return log::warn!("Keeping the procedural sky: {error}"),
            }
        }
        match CubemapFaces::decode(std::array::from_fn(|face| files[face].as_slice())) {
            Ok(faces) => {
                slot.borrow_mut().replace(faces);
            }
            Err(error) => log::warn!("Keeping the procedural sky: {error:#}"),
        }
    });
}

/// WebGPU init is async-only in browsers: the app lands in `slot` once the device resolves.
fn spawn_app_init(slot: AppSlot, window: Arc<Window>, size: (u32, u32)) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut gpu = match GpuContext::new_windowed_async(Arc::clone(&window)).await {
            Ok(gpu) => gpu,
            // navigator.gpu without an adapter (a blocklisted GPU, Linux without the flag): the page's fallback, no panic.
            Err(err) => return web::notify_page(web::PAGE_FAILED, &format!("{err:#}")),
        };
        // The context read inner_size before the canvas size propagated through winit.
        gpu.resize(size.0, size.1);
        let (name, bytes) = EMBEDDED_SCENES[0];
        let scene = load_gltf_slice(bytes).expect("embedded demo scene must load");
        // No quit, screenshot or shader files in a browser tab.
        let mut app = StudioApp::new(&window, gpu, &scene, home_camera(), HostCaps::default())
            .expect("failed to set up the renderer");
        app.editor_mut().set_layout(DockPrefs::web());
        app.set_library(ModelLibrary::web(&scene_base()), Some(0));
        app.model = Some(ModelSource {
            name: name.to_owned(),
            path: format!("{name}.gltf (embedded)"),
        });
        slot.borrow_mut().replace(app);
        web::notify_page(web::PAGE_READY, "");
        window.request_redraw();
    });
}

/// Hands what the page asked for to the app's loader; the scene on screen stays until the new one is ready.
fn take_page_request(app: &mut StudioApp, dropped: &DroppedScene) {
    match dropped.borrow_mut().take() {
        Some(PageRequest::Bytes(name, bytes)) => app.open_bytes(&name, bytes),
        Some(PageRequest::Row(row)) => app.open_entry(row),
        None => {}
    }
}

/// Follows the canvas's CSS layout size every frame; against the size asked, so a clamped surface stays put.
fn follow_canvas_size(window: &Window, gpu: &mut GpuContext) {
    let Some(canvas) = window.canvas() else {
        return;
    };
    let size = canvas_device_size(&canvas);
    if gpu.requested_size() != Some(size) {
        gpu.resize(size.0, size.1);
    }
}

#[derive(Default)]
struct DemoApp {
    window: Option<Arc<Window>>,
    /// Filled asynchronously once the WebGPU adapter/device resolve.
    app: AppSlot,
    /// Filled asynchronously by the drop-zone listeners.
    dropped_scene: DroppedScene,
    /// egui had focus last frame, so the browser-key listener lets winit hand it Tab and friends.
    egui_wants_keys: Rc<Cell<bool>>,
    /// Filled once the skybox fetch decodes all six faces.
    skybox: SkyboxSlot,
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = create_canvas_window(event_loop, Rc::clone(&self.egui_wants_keys));
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            install_drop_zone(&document, Rc::clone(&self.dropped_scene));
        }
        // The scene picker and drag-drop share one slot.
        SCENE_SLOT.with(|slot| {
            slot.borrow_mut().replace(Rc::clone(&self.dropped_scene));
        });
        APP_SLOT.with(|slot| slot.borrow_mut().replace(Rc::clone(&self.app)));
        let canvas = window.canvas().expect("winit window must expose a canvas");
        let size = canvas_device_size(&canvas);
        spawn_app_init(Rc::clone(&self.app), Arc::clone(&window), size);
        spawn_skybox_fetch(Rc::clone(&self.skybox));
        self.window = Some(window);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let mut slot = self.app.borrow_mut();
        let Some(app) = slot.as_mut() else {
            return;
        };
        app.window_event(window, &event);
        if matches!(event, WindowEvent::RedrawRequested) {
            take_page_request(app, &self.dropped_scene);
            if let Some(faces) = self.skybox.borrow_mut().take() {
                app.set_skybox_faces(&faces);
            }
            follow_canvas_size(window, &mut app.gpu);
            // A browser tab never quits and has no file shortcuts, so a frame asks nothing of this shell.
            let _ = app.frame(window, performance_now_seconds());
            self.egui_wants_keys.set(app.wants_keyboard());
        }
    }

    /// Raw pointer motion, which turns a right-drag look while the Pointer Lock API holds the cursor.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let Some(app) = self.app.borrow_mut().as_mut() {
            app.device_event(&event);
        }
    }
}

/// Loads row `index` of the web library, as the page's `<select>` orders it: the showcase scenes, then the Colosseum.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn select_demo_scene(index: u32) {
    if index as usize > EMBEDDED_SCENES.len() {
        log::warn!("select_demo_scene: scene index {index} out of range - ignored");
        return;
    }
    SCENE_SLOT.with(|slot| match slot.borrow().as_ref() {
        Some(scene_slot) => {
            scene_slot
                .borrow_mut()
                .replace(PageRequest::Row(index as usize));
        }
        None => log::warn!("select_demo_scene: called before the demo initialized - ignored"),
    });
}

/// Runs `f` on the app once it is up; a page call between frames finds it free.
fn with_app<T>(f: impl FnOnce(&mut StudioApp) -> T) -> Result<T, JsValue> {
    APP_SLOT.with(|slot| {
        let slot = slot.borrow();
        let app = slot.as_ref().ok_or("the demo has no window yet")?;
        let mut app = app.try_borrow_mut().map_err(|_| "the demo is mid-frame")?;
        let app = app
            .as_mut()
            .ok_or("the WebGPU device has not resolved yet")?;
        Ok(f(app))
    })
}

/// The browser test's switch: `mode` raster, ray or path, `path` forward or deferred, clouds on or off; the spin stops.
#[wasm_bindgen]
pub fn set_demo_view(mode: &str, path: &str, clouds: bool) -> Result<(), JsValue> {
    let view = web::DemoView::parse(mode, path, clouds).map_err(|e| JsValue::from_str(&e))?;
    with_app(|app| app.apply_demo_view(view))
}

/// The browser test's camera: the orbit's yaw and pitch in degrees; the spin stops.
#[wasm_bindgen]
pub fn set_demo_camera(yaw_deg: f32, pitch_deg: f32) -> Result<(), JsValue> {
    with_app(|app| app.apply_demo_camera(yaw_deg, pitch_deg))
}

/// What the demo draws now: `{drawn, picked, path, clouds, fxaa, trace, detail, samples, viewport}`.
#[wasm_bindgen]
pub fn demo_view() -> Result<JsValue, JsValue> {
    with_app(|app| web::report_to_js(&app.demo_report()))
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.spawn_app(DemoApp::default());
}
