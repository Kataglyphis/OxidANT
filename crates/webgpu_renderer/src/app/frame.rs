//! One frame: acquire, egui pass, its actions, the scene with the camera on the free rect, egui paint, present.

use glam::Vec2;
use winit::window::Window;

use super::{input, AppSignal, HostCaps, StudioApp};
use crate::render::overlay::OverlayControls;
use crate::scene::viewport::SceneViewport;

/// Auto-orbit turn rate until the first drag.
const AUTO_ORBIT_DEG_PER_S: f32 = 30.0;

/// What one egui pass hands back to its frame.
#[derive(Debug, Default)]
pub(crate) struct PassOutput {
    pub(crate) signals: Vec<AppSignal>,
    pub(crate) controls_changed: bool,
    pub(crate) viewport: SceneViewport,
}

impl StudioApp {
    /// Draws one frame and returns what the shell must do; a frame without a surface texture is skipped whole.
    pub fn frame(&mut self, window: &Window, now_s: f64) -> Vec<AppSignal> {
        // Acquired before the egui pass, so a skipped frame never strands a font atlas delta.
        let Some(frame) = self.acquire(window) else {
            return Vec::new();
        };
        let (output, pass) = self.run_ui(window);
        if pass.controls_changed {
            self.controls.apply(&mut self.renderer);
        }
        self.camera.viewport = pass.viewport;
        self.advance(now_s);
        self.draw(&frame, output);
        window.pre_present_notify();
        self.gpu.queue.present(frame);
        window.request_redraw();
        pass.signals
    }

    fn acquire(&mut self, window: &Window) -> Option<wgpu::SurfaceTexture> {
        match self.gpu.surface.as_ref()?.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // Never abort on Outdated/Lost: reconfigure and retry next frame.
                self.gpu.reconfigure();
                window.request_redraw();
                None
            }
            other => {
                log::error!("Surface acquire failed: {other:?}");
                None
            }
        }
    }

    fn run_ui(&mut self, window: &Window) -> (egui::FullOutput, PassOutput) {
        // Read before the pass: the pass's own Esc handling clears focus and closes popups.
        let esc_armed = input::esc_armed(self.overlay.context());
        let caps = self.caps;
        let controls = &mut self.controls;
        controls.occlusion_stats = Some(self.renderer.occlusion_cull_stats());
        self.overlay
            .run(window, |ui| studio_pass(ui, esc_armed, caps, controls))
    }

    fn advance(&mut self, now_s: f64) {
        let elapsed = (now_s - *self.epoch_s.get_or_insert(now_s)) as f32;
        if self.controller.auto_orbit {
            self.camera.yaw_deg = self.home.yaw_deg + elapsed * AUTO_ORBIT_DEG_PER_S;
        }
        if self.renderer.has_animations() {
            self.renderer.set_animation_time(elapsed);
        }
        self.renderer.frame_delta_seconds = self.clock.tick_at(now_s);
    }

    fn draw(&mut self, frame: &wgpu::SurfaceTexture, output: egui::FullOutput) {
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Size from the acquired texture: mid-resize the window already differs from the surface.
        let (width, height) = (frame.texture.width(), frame.texture.height());
        self.renderer.render_tonemapped(
            &self.gpu,
            &mut self.tonemap,
            &view,
            width,
            height,
            &self.camera,
        );
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("overlay_encoder"),
            });
        self.overlay
            .paint(&self.gpu, &mut encoder, &view, [width, height], output);
        self.gpu.queue.submit(Some(encoder.finish()));
    }
}

/// One pass's UI: the shortcuts, then the controls; what the root keeps free is the scene's rect.
pub(crate) fn studio_pass(
    ui: &mut egui::Ui,
    esc_armed: bool,
    caps: HostCaps,
    controls: &mut OverlayControls,
) -> PassOutput {
    let signals = input::shortcuts(ui.ctx(), esc_armed, caps);
    let controls_changed = controls.ui(ui.ctx());
    let point = |p: egui::Pos2| Vec2::new(p.x, p.y);
    let free = ui.available_rect_before_wrap();
    let frame = ui.ctx().viewport_rect();
    PassOutput {
        signals,
        controls_changed,
        viewport: SceneViewport::of_rect(
            point(free.min),
            point(free.max),
            point(frame.min),
            point(frame.max),
        ),
    }
}
