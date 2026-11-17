//! What the browser shell shares with its page, kept where a native test can still check it.

use super::StudioApp;
use crate::render::forward::RasterPath;
use crate::render::tracer::{RenderMode, TraceStatus};
use crate::scene::viewport::SceneViewport;

/// The page's hook for a running demo: enable the scene picker, show the controls.
pub(crate) const PAGE_READY: &str = "webgpuDemoReady";
/// The page's hook for a browser with no usable adapter or device; its argument says why.
pub(crate) const PAGE_FAILED: &str = "webgpuDemoFailed";
/// The page's hook for a dropped file, which no picker entry names.
pub(crate) const PAGE_DROPPED: &str = "webgpuDemoDropped";

/// `set_demo_view`'s mode names, in the editor's order.
const MODE_NAMES: [(&str, RenderMode); 3] = [
    ("raster", RenderMode::Raster),
    ("ray", RenderMode::RayTraced),
    ("path", RenderMode::PathTraced),
];
/// `set_demo_view`'s raster path names.
const PATH_NAMES: [(&str, RasterPath); 2] = [
    ("forward", RasterPath::Forward),
    ("deferred", RasterPath::Deferred),
];

/// What a browser test asks the demo to draw through `set_demo_view(mode, path, clouds)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DemoView {
    pub(crate) mode: RenderMode,
    pub(crate) path: RasterPath,
    pub(crate) clouds: bool,
}

impl DemoView {
    /// `mode` is raster, ray or path; `path` forward or deferred.
    pub(crate) fn parse(mode: &str, path: &str, clouds: bool) -> Result<Self, String> {
        Ok(Self {
            mode: named(&MODE_NAMES, mode)
                .ok_or_else(|| format!("unknown render mode '{mode}': raster, ray or path"))?,
            path: named(&PATH_NAMES, path)
                .ok_or_else(|| format!("unknown raster path '{path}': forward or deferred"))?,
            clouds,
        })
    }
}

fn named<T: Copy>(names: &[(&str, T)], name: &str) -> Option<T> {
    names.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn name_of<T: PartialEq + Copy>(names: &[(&'static str, T)], value: T) -> &'static str {
    names
        .iter()
        .find(|(_, v)| *v == value)
        .map_or("?", |(n, _)| n)
}

/// What the demo draws now, in `set_demo_view`'s names, for a browser test to check against what it asked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DemoReport {
    pub(crate) drawn: &'static str,
    pub(crate) picked: &'static str,
    pub(crate) path: &'static str,
    pub(crate) clouds: bool,
    /// Whether the drawn frame runs FXAA.
    pub(crate) fxaa: bool,
    /// idle, building, ready or failed.
    pub(crate) trace: &'static str,
    /// The tracer's status line, or why a traced pick rasters.
    pub(crate) detail: String,
    pub(crate) samples: u64,
    pub(crate) viewport: SceneViewport,
}

impl StudioApp {
    /// Sets what the editor's mode, path and clouds controls set, and stops the idle spin so frames compare.
    pub(crate) fn apply_demo_view(&mut self, view: DemoView) {
        self.renderer.render_mode = view.mode;
        self.renderer.raster_path = view.path;
        self.renderer.clouds.enabled = view.clouds;
        self.controller.auto_orbit = false;
    }

    /// Turns the orbit camera to `yaw_deg` and `pitch_deg` and stops the idle spin, so a test frames the sky too.
    pub(crate) fn apply_demo_camera(&mut self, yaw_deg: f32, pitch_deg: f32) {
        self.camera.yaw_deg = yaw_deg;
        self.camera.pitch_deg = pitch_deg;
        self.controller.auto_orbit = false;
    }

    pub(crate) fn demo_report(&self) -> DemoReport {
        let status = self.renderer.trace_status();
        let trace = match status {
            TraceStatus::Idle => "idle",
            TraceStatus::Building(_) => "building",
            TraceStatus::Ready(_) => "ready",
            TraceStatus::Failed(_) => "failed",
        };
        DemoReport {
            drawn: name_of(&MODE_NAMES, self.renderer.drawn_mode()),
            picked: name_of(&MODE_NAMES, self.renderer.render_mode),
            path: name_of(&PATH_NAMES, self.renderer.raster_path),
            clouds: self.renderer.clouds.enabled,
            fxaa: self.renderer.fxaa_runs(),
            trace,
            detail: self
                .renderer
                .traced_fallback()
                .unwrap_or_else(|| status.text()),
            samples: self.renderer.accumulated_samples(),
            viewport: self.camera.viewport,
        }
    }
}

/// Keys the browser acts on that winit would cancel: Tab leaves the canvas, F5 and Ctrl+R reload, Ctrl +/-/0 zoom.
pub(crate) fn is_browser_key(key: &str, command: bool) -> bool {
    match key {
        "Tab" | "F5" => true,
        "r" | "R" | "+" | "=" | "-" | "_" | "0" => command,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use dom::{adapter_info, leave_browser_keys, notify_page, report_to_js};

#[cfg(target_arch = "wasm32")]
mod dom {
    use std::cell::Cell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;

    /// Calls the page's `name(argument)` hook when it defines one: the page owns its DOM, the demo only reports.
    pub(crate) fn notify_page(name: &str, argument: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let hook = js_sys::Reflect::get(&window, &JsValue::from_str(name)).ok();
        if let Some(hook) = hook.and_then(|hook| hook.dyn_into::<js_sys::Function>().ok()) {
            if let Err(err) = hook.call1(&JsValue::NULL, &JsValue::from_str(argument)) {
                log::error!("The page's {name} hook failed: {err:?}");
            }
        }
    }

    /// `GPUAdapterInfo`'s vendor and architecture, which wgpu drops; a second `requestAdapter` returns the same GPU.
    pub(crate) async fn adapter_info() -> Option<crate::adapter::WebAdapterInfo> {
        let get = |target: &JsValue, key: &str| {
            js_sys::Reflect::get(target, &JsValue::from_str(key)).ok()
        };
        let gpu = get(&get(&js_sys::global(), "navigator")?, "gpu")?;
        let request = get(&gpu, "requestAdapter")?
            .dyn_into::<js_sys::Function>()
            .ok()?;
        let options = js_sys::Object::new();
        let preference = JsValue::from_str("high-performance");
        js_sys::Reflect::set(&options, &JsValue::from_str("powerPreference"), &preference).ok()?;
        let promise = request
            .call1(&gpu, &options)
            .ok()?
            .dyn_into::<js_sys::Promise>()
            .ok()?;
        let adapter = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?;
        let info = get(&adapter, "info").filter(|info| info.is_object())?;
        let field = |key: &str| {
            get(&info, key)
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        };
        Some(crate::adapter::WebAdapterInfo {
            vendor: field("vendor"),
            architecture: field("architecture"),
            device: field("device"),
            description: field("description"),
        })
    }

    /// A [`super::DemoReport`] as a plain object; `viewport` is `[x0, y0, x1, y1]` in fractions of the canvas.
    pub(crate) fn report_to_js(report: &super::DemoReport) -> JsValue {
        let object = js_sys::Object::new();
        let v = report.viewport;
        let viewport: js_sys::Array = [v.x0, v.y0, v.x1, v.y1]
            .into_iter()
            .map(|f| JsValue::from_f64(f64::from(f)))
            .collect();
        let fields: [(&str, JsValue); 9] = [
            ("drawn", report.drawn.into()),
            ("picked", report.picked.into()),
            ("path", report.path.into()),
            ("clouds", report.clouds.into()),
            ("fxaa", report.fxaa.into()),
            ("trace", report.trace.into()),
            ("detail", report.detail.as_str().into()),
            ("samples", JsValue::from_f64(report.samples as f64)),
            ("viewport", viewport.into()),
        ];
        for (key, value) in fields {
            let _ = js_sys::Reflect::set(&object, &JsValue::from_str(key), &value);
        }
        object.into()
    }

    /// Stops the browser's keys before winit's canvas listener cancels them, unless egui has a field to type into.
    pub(crate) fn leave_browser_keys(mount: &web_sys::Element, egui_wants_keys: Rc<Cell<bool>>) {
        let listener = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |event: web_sys::KeyboardEvent| {
                let command = event.ctrl_key() || event.meta_key();
                if !egui_wants_keys.get() && super::is_browser_key(&event.key(), command) {
                    event.stop_immediate_propagation();
                }
            },
        );
        // The capture phase on the canvas's parent runs before the canvas's own listeners.
        let _ = mount.add_event_listener_with_callback_and_bool(
            "keydown",
            listener.as_ref().unchecked_ref(),
            true,
        );
        listener.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../web/index.html");

    #[test]
    fn the_page_defines_every_hook_the_demo_calls() {
        for hook in [PAGE_READY, PAGE_FAILED, PAGE_DROPPED] {
            assert!(PAGE.contains(&format!("window.{hook} = ")), "{hook}");
        }
        assert!(
            PAGE.contains("selectedIndex = -1"),
            "a drop must clear the picker, or its scene cannot be picked again"
        );
    }

    #[test]
    fn a_demo_view_parses_every_mode_and_path_and_names_a_wrong_one() {
        for (mode, want) in MODE_NAMES {
            for (path, want_path) in PATH_NAMES {
                let view = DemoView::parse(mode, path, true).expect("parses");
                assert_eq!((view.mode, view.path, view.clouds), (want, want_path, true));
                assert_eq!(name_of(&MODE_NAMES, view.mode), mode);
                assert_eq!(name_of(&PATH_NAMES, view.path), path);
            }
        }
        let err = DemoView::parse("traced", "forward", false).unwrap_err();
        assert!(err.contains("'traced'"), "{err}");
        let err = DemoView::parse("ray", "Deferred", false).unwrap_err();
        assert!(err.contains("'Deferred'"), "{err}");
    }

    #[test]
    fn a_demo_view_switches_the_renderer_and_the_camera_and_reports_what_it_draws() {
        let Some(mut app) = super::super::frame::tests::studio() else {
            return;
        };
        let raster = app.demo_report();
        assert_eq!(
            (raster.drawn, raster.picked, raster.path),
            ("raster", "raster", "forward")
        );
        assert_eq!(
            (raster.clouds, raster.fxaa, raster.trace, raster.samples),
            (false, false, "idle", 0)
        );
        assert!(
            app.controller.auto_orbit,
            "the demo spins until a view is set"
        );
        let view = DemoView::parse("path", "deferred", true).expect("parses");
        app.apply_demo_view(view);
        assert!(!app.controller.auto_orbit);
        let waiting = app.demo_report();
        assert_eq!(
            (waiting.drawn, waiting.picked, waiting.path, waiting.clouds),
            ("raster", "path", "deferred", true),
            "a traced pick rasters until its scene is built"
        );
        assert!(waiting.fxaa, "the deferred raster frame runs FXAA");
        let built = app
            .renderer
            .tracer_mut()
            .finish_build(&app.gpu, std::time::Duration::from_secs(120));
        assert!(built, "{:?}", app.trace_status());
        app.apply_demo_camera(10.0, 5.0);
        assert_eq!((app.camera.yaw_deg, app.camera.pitch_deg), (10.0, 5.0));
        let ready = app.demo_report();
        assert_eq!(
            (ready.drawn, ready.trace, ready.fxaa),
            ("path", "ready", false)
        );
        assert!(ready.detail.starts_with("Tracer ready"), "{}", ready.detail);
    }

    #[test]
    fn the_browser_keeps_tab_reload_and_zoom_and_egui_every_other_key() {
        assert!(is_browser_key("Tab", false));
        assert!(is_browser_key("F5", false));
        for key in ["r", "R", "+", "=", "-", "_", "0"] {
            assert!(is_browser_key(key, true), "Ctrl {key}");
            assert!(
                !is_browser_key(key, false),
                "{key} alone is text or a key egui reads"
            );
        }
        for key in [
            "w", "a", "s", "d", "q", "e", "F1", "Escape", "Enter", "ArrowUp",
        ] {
            assert!(!is_browser_key(key, false), "{key}");
            assert!(!is_browser_key(key, true), "Ctrl {key}");
        }
    }
}
