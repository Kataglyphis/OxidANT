//! Window events reach egui first and the orbit camera second; the editor reads its keys inside the egui pass.

use winit::event::WindowEvent;
use winit::window::Window;

use super::StudioApp;

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
