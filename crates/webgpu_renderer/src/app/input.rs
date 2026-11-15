//! Window events reach egui first and the orbit camera second; the keyboard shortcuts are read inside the egui pass.

use egui::{Context, InputState, Key};
use winit::event::WindowEvent;
use winit::window::Window;

use super::{AppSignal, HostCaps, StudioApp};

impl StudioApp {
    /// Routes one window event; true when egui consumed it.
    pub fn window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let consumed = self.overlay.on_event(window, event);
        if consumed {
            self.controller.note_consumed_event(event);
        } else {
            self.controller.handle_event(event, &mut self.camera);
        }
        if let WindowEvent::Resized(size) = event {
            self.gpu.resize(size.width, size.height);
        }
        consumed
    }
}

/// Esc may quit only if, before the pass, nothing had focus and no popup was open: the first Esc releases those.
pub(crate) fn esc_armed(ctx: &Context) -> bool {
    !ctx.egui_wants_keyboard_input() && !egui::Popup::is_any_open(ctx)
}

/// S, R and Esc, read inside the pass so that typing them into a text field fires none of them.
pub(crate) fn shortcuts(ctx: &Context, esc_armed: bool, caps: HostCaps) -> Vec<AppSignal> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    let bindings = [
        (Key::Escape, caps.quit && esc_armed, AppSignal::Quit),
        (Key::S, caps.screenshot, AppSignal::Screenshot),
        (Key::R, caps.hot_reload, AppSignal::ReloadShaders),
    ];
    ctx.input(|input| {
        bindings
            .into_iter()
            .filter(|&(key, enabled, _)| enabled && pressed_once(input, key))
            .map(|(_, _, signal)| signal)
            .collect()
    })
}

/// A first press of `key` without modifiers; key repeats do not count.
fn pressed_once(input: &InputState, key: Key) -> bool {
    input.events.iter().any(|event| {
        matches!(
            event,
            egui::Event::Key { key: event_key, pressed: true, repeat: false, modifiers, .. }
                if *event_key == key && modifiers.is_none()
        )
    })
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2, Event, Modifiers, Rect};

    use super::super::frame::studio_pass;
    use super::*;
    use crate::render::overlay::{run_pass, studio_context, OverlayControls};

    const NATIVE: HostCaps = HostCaps {
        quit: true,
        screenshot: true,
        hot_reload: true,
    };
    const BROWSER: HostCaps = HostCaps {
        quit: false,
        screenshot: false,
        hot_reload: false,
    };

    fn key_event(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    /// A press and its release in one frame; egui counts a press of a key it still holds as a repeat.
    fn tap(key: Key) -> Vec<Event> {
        vec![
            key_event(key, true, Modifiers::NONE),
            key_event(key, false, Modifiers::NONE),
        ]
    }

    fn input(events: Vec<Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            events,
            ..Default::default()
        }
    }

    /// One frame in the app's order: Esc armed before the pass, the shortcuts read first inside it, then `ui`.
    fn frame(
        ctx: &Context,
        events: Vec<Event>,
        caps: HostCaps,
        ui: &mut dyn FnMut(&mut egui::Ui),
    ) -> Vec<AppSignal> {
        let armed = esc_armed(ctx);
        let (output, signals) = run_pass(ctx, input(events), |root| {
            let signals = shortcuts(root.ctx(), armed, caps);
            ui(root);
            signals
        });
        output.drop_without_applying_deltas();
        signals
    }

    #[test]
    fn s_and_r_reach_the_shell_while_nothing_has_focus() {
        let ctx = studio_context();
        let mut nothing = |_: &mut egui::Ui| {};
        let mut signals = |key| frame(&ctx, tap(key), NATIVE, &mut nothing);
        assert_eq!(signals(Key::S), [AppSignal::Screenshot]);
        assert_eq!(signals(Key::R), [AppSignal::ReloadShaders]);
        assert_eq!(signals(Key::Escape), [AppSignal::Quit]);
    }

    #[test]
    fn typing_into_a_field_fires_nothing_and_esc_first_leaves_the_field() {
        let ctx = studio_context();
        let (mut text, mut focus) = (String::new(), true);
        let mut field = |ui: &mut egui::Ui| {
            let response = ui.text_edit_singleline(&mut text);
            if std::mem::take(&mut focus) {
                response.request_focus();
            }
        };
        assert!(frame(&ctx, Vec::new(), NATIVE, &mut field).is_empty());
        assert!(
            ctx.text_edit_focused(),
            "the field must hold focus, or this test proves nothing"
        );
        for key in [Key::S, Key::R, Key::Escape] {
            assert!(
                frame(&ctx, tap(key), NATIVE, &mut field).is_empty(),
                "{key:?}"
            );
        }
        assert!(!ctx.egui_wants_keyboard_input());
        assert_eq!(
            frame(&ctx, tap(Key::Escape), NATIVE, &mut field),
            [AppSignal::Quit]
        );
    }

    #[test]
    fn the_browser_never_quits_and_has_no_file_shortcuts() {
        let ctx = studio_context();
        let keys = [tap(Key::Escape), tap(Key::S), tap(Key::R)].concat();
        assert!(frame(&ctx, keys, BROWSER, &mut |_| {}).is_empty());
    }

    #[test]
    fn key_repeats_and_modified_presses_are_not_shortcuts() {
        let ctx = studio_context();
        let mut nothing = |_: &mut egui::Ui| {};
        let ctrl_r = vec![key_event(Key::R, true, Modifiers::COMMAND)];
        assert!(frame(&ctx, ctrl_r, NATIVE, &mut nothing).is_empty());
        let hold_s = || vec![key_event(Key::S, true, Modifiers::NONE)];
        assert_eq!(
            frame(&ctx, hold_s(), NATIVE, &mut nothing),
            [AppSignal::Screenshot]
        );
        assert!(frame(&ctx, hold_s(), NATIVE, &mut nothing).is_empty());
    }

    #[test]
    fn the_studio_pass_reads_the_keys_and_leaves_the_whole_frame_to_the_scene() {
        let ctx = studio_context();
        let mut controls =
            OverlayControls::from_light(glam::Vec4::new(0.5, 0.8, 0.3, 0.35), glam::Vec4::ONE);
        let (output, pass) = run_pass(&ctx, input(tap(Key::S)), |ui| {
            studio_pass(ui, esc_armed(ui.ctx()), NATIVE, &mut controls)
        });
        output.drop_without_applying_deltas();
        assert_eq!(pass.signals, [AppSignal::Screenshot]);
        assert!(!pass.controls_changed);
        assert!(
            pass.viewport.is_full(),
            "a floating panel docks nothing: {:?}",
            pass.viewport
        );
    }
}
