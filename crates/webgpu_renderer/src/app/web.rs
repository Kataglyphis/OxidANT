//! What the browser shell shares with its page, kept where a native test can still check it.

/// The page's hook for a running demo: enable the scene picker, show the controls.
pub(crate) const PAGE_READY: &str = "webgpuDemoReady";
/// The page's hook for a browser with no usable adapter or device; its argument says why.
pub(crate) const PAGE_FAILED: &str = "webgpuDemoFailed";
/// The page's hook for a dropped file, which no picker entry names.
pub(crate) const PAGE_DROPPED: &str = "webgpuDemoDropped";

/// Keys the browser acts on that winit would cancel: Tab leaves the canvas, F5 and Ctrl+R reload, Ctrl +/-/0 zoom.
pub(crate) fn is_browser_key(key: &str, command: bool) -> bool {
    match key {
        "Tab" | "F5" => true,
        "r" | "R" | "+" | "=" | "-" | "_" | "0" => command,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use dom::{leave_browser_keys, notify_page};

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
