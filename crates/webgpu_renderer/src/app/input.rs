//! Window events reach egui first, then the fly and orbit cameras; the editor reads its keys inside the egui pass.

use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, WindowEvent};
use winit::window::{CursorGrabMode, Window};

use super::StudioApp;
use crate::scene::fly::{FlyInput, PointerGrab};

/// Orbit degrees per logical pixel of drag, so a HiDPI screen or a phone turns as far per finger travel as 100 %.
const ORBIT_DEG_PER_POINT: f32 = 0.25;

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
        self.controller.rotate_speed_deg_per_px = orbit_deg_per_px(window.scale_factor());
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

/// The orbit controller's rate for a window at `scale_factor`: pointer and touch deltas arrive in physical pixels.
fn orbit_deg_per_px(scale_factor: f64) -> f32 {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor as f32
    } else {
        1.0
    };
    ORBIT_DEG_PER_POINT / scale
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::camera::OrbitCamera;
    use crate::scene::controller::OrbitController;

    #[test]
    fn an_orbit_drag_turns_as_far_per_logical_pixel_at_every_scale() {
        let turn = |scale: f64, physical_px: f32| {
            let mut controller = OrbitController::default();
            controller.rotate_speed_deg_per_px = orbit_deg_per_px(scale);
            let mut camera = OrbitCamera::default();
            let yaw = camera.yaw_deg;
            controller.apply_drag(&mut camera, physical_px, 0.0);
            camera.yaw_deg - yaw
        };
        assert_eq!(turn(1.0, 40.0), 10.0);
        assert_eq!(turn(2.0, 80.0), 10.0);
        assert!((turn(1.5, 60.0) - 10.0).abs() < 1e-4);
        assert_eq!(turn(0.0, 40.0), 10.0, "a bogus scale falls back to 100 %");
        assert_eq!(turn(f64::NAN, 40.0), 10.0);
    }
}
