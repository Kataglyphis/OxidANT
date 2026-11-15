//! Window events reach egui first, then the fly and orbit cameras; the editor reads its keys inside the egui pass.

use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, WindowEvent};
use winit::window::{CursorGrabMode, Window};

use super::StudioApp;
use crate::scene::fly::{FlyInput, PointerGrab};

impl StudioApp {
    /// Routes one window event; true when egui consumed it.
    pub fn window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        // While looking, the hidden pointer is the camera's: egui sees no moves, so no panel lights up under it.
        let looking_move =
            self.fly.is_looking() && matches!(event, WindowEvent::CursorMoved { .. });
        let consumed = !looking_move && self.overlay.on_event(window, event);
        if let Some(input) = FlyInput::of(event) {
            let grab = self.fly.handle(input, consumed, &mut self.camera);
            apply_grab(window, grab);
            // At the event, not the frame: a look shorter than a frame must not be spun away.
            if self.fly.is_engaged() {
                self.controller.auto_orbit = false;
            }
        }
        // The only scale the orbit controller sees: its drag, finger and wheel deltas are physical pixels.
        self.controller.set_scale_factor(window.scale_factor());
        if consumed {
            self.controller.note_consumed_event(event);
        } else {
            self.controller.handle_event(event, &mut self.camera);
        }
        match event {
            WindowEvent::Resized(size) => self.gpu.resize(size.width, size.height),
            // An occluded frame was skipped without asking for another; this restarts the loop.
            WindowEvent::Occluded(false) => window.request_redraw(),
            _ => {}
        }
        consumed
    }

    /// Raw pointer motion, which keeps a look turning where the platform holds the cursor still.
    pub fn device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.fly.raw_motion(&mut self.camera, *delta);
        }
    }
}

/// Hides and holds the pointer for a look, Locked where the platform has it and Confined elsewhere, or gives it back.
fn apply_grab(window: &Window, grab: PointerGrab) {
    match grab {
        PointerGrab::Keep => {}
        PointerGrab::Hide => {
            let held = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            if let Err(err) = held {
                log::debug!("The pointer stays free during the look: {err}");
            }
            window.set_cursor_visible(false);
        }
        PointerGrab::Show { at } => {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            if let Some((x, y)) = at {
                // Wayland and browsers cannot place the pointer; it then reappears where the platform left it.
                let _ = window.set_cursor_position(PhysicalPosition::new(x, y));
            }
        }
    }
}
