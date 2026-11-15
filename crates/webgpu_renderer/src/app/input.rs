//! Window events reach egui first and the orbit camera second; the editor reads its keys inside the egui pass.

use winit::event::WindowEvent;
use winit::window::Window;

use super::StudioApp;

/// Orbit degrees per logical pixel of drag, so a HiDPI screen or a phone turns as far per finger travel as 100 %.
const ORBIT_DEG_PER_POINT: f32 = 0.25;

impl StudioApp {
    /// Routes one window event; true when egui consumed it.
    pub fn window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let consumed = self.overlay.on_event(window, event);
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
