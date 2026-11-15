//! One frame: acquire, egui pass, its actions, the scene with the camera on the free rect, egui paint, present.

use winit::window::Window;

use super::{AppSignal, StudioApp};
use crate::render::forward::readback;
use crate::render::texture::create_2d_texture;
use crate::ui::editor::{EditorAction, EditorContext, RendererSettings, SceneStats};

/// Auto-orbit turn rate until the first drag.
const AUTO_ORBIT_DEG_PER_S: f32 = 30.0;

/// One frame without a window: its size in pixels, its scale, its time and the input it receives.
#[derive(Clone, Debug)]
pub struct OffscreenFrame {
    pub size_px: [u32; 2],
    pub pixels_per_point: f32,
    pub now_s: f64,
    pub events: Vec<egui::Event>,
}

impl OffscreenFrame {
    /// What egui-winit would hand egui for a window of this size and scale.
    fn raw_input(&self) -> egui::RawInput {
        let [width, height] = self.size_px;
        let points = egui::vec2(width as f32, height as f32) / self.pixels_per_point;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, points)),
            time: Some(self.now_s),
            focused: true,
            events: self.events.clone(),
            ..Default::default()
        };
        let root = raw.viewports.entry(egui::ViewportId::ROOT).or_default();
        root.native_pixels_per_point = Some(self.pixels_per_point);
        raw
    }
}

impl StudioApp {
    /// Draws one frame and returns what the shell must do; a frame without a surface texture is skipped whole.
    pub fn frame(&mut self, window: &Window, now_s: f64) -> Vec<AppSignal> {
        // Acquired before the egui pass, so a skipped frame never strands a font atlas delta.
        let Some(frame) = self.acquire(window) else {
            return Vec::new();
        };
        let size = [frame.texture.width(), frame.texture.height()];
        let raw = self.overlay.take_input(window);
        let (mut output, signals) = self.pass(raw, now_s, size);
        let platform = std::mem::take(&mut output.platform_output);
        self.overlay.handle_platform_output(window, platform);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(&view, size, output);
        window.pre_present_notify();
        self.gpu.queue.present(frame);
        window.request_redraw();
        signals
    }

    /// One frame of scene and editor into an offscreen target, read back as tightly packed RGBA8 rows.
    pub fn render_offscreen(
        &mut self,
        frame: &OffscreenFrame,
    ) -> anyhow::Result<(Vec<u8>, Vec<AppSignal>)> {
        let (output, signals) = self.pass(frame.raw_input(), frame.now_s, frame.size_px);
        let [width, height] = frame.size_px;
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        let texture = create_2d_texture(
            &self.gpu.device,
            Some("studio_offscreen"),
            width,
            height,
            self.format,
            usage,
            1,
            1,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(&view, frame.size_px, output);
        let pixels = readback::read_rgba8(&self.gpu, &texture, width, height)?;
        Ok((pixels, signals))
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

    /// The editor pass between the renderer's settings read and written back; its rect becomes the camera's.
    fn pass(
        &mut self,
        raw: egui::RawInput,
        now_s: f64,
        surface_px: [u32; 2],
    ) -> (egui::FullOutput, Vec<AppSignal>) {
        // Recorded before the pass, so the editor is a pure function of what it is handed.
        self.frames.tick(now_s);
        self.poll_scene();
        let ctx = self.overlay.context();
        let delay = self.frames.double_click_delay_s();
        // A click's down, up and down reach egui a frame apart, so at software-rendering rates 0.3 s is never a double-click.
        ctx.options_mut(|options| options.input_options.max_double_click_delay = delay);
        let esc_armed = self.editor.esc_armed(ctx);
        let mut settings = RendererSettings::of(&self.renderer);
        let stats = SceneStats::of(&self.renderer);
        let mut cx = EditorContext {
            settings: &mut settings,
            camera: &mut self.camera,
            stats: &stats,
            frames: &self.frames,
            device: &self.device,
            model: self.model.as_ref(),
            scene: self.scene.status(),
            caps: self.caps,
            surface_px,
        };
        let editor = &mut self.editor;
        let (output, editor_output) = self
            .overlay
            .run(raw, |ui| editor.show(ui, &mut cx, esc_armed));
        settings.apply(&mut self.renderer);
        self.camera.viewport = editor_output.viewport;
        let signals = self.act(editor_output.actions);
        self.advance(now_s);
        (output, signals)
    }

    /// Does what the app does itself and hands the rest to the shell.
    pub(super) fn act(&mut self, actions: Vec<EditorAction>) -> Vec<AppSignal> {
        let mut signals = Vec::new();
        for action in actions {
            match action {
                EditorAction::LoadModel(row) => self.open_entry(row),
                EditorAction::FrameScene => self.frame_scene(),
                EditorAction::Quit => signals.push(AppSignal::Quit),
                EditorAction::Screenshot => signals.push(AppSignal::Screenshot),
                EditorAction::ReloadShaders => signals.push(AppSignal::ReloadShaders),
            }
        }
        signals
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

    fn draw(
        &mut self,
        view: &wgpu::TextureView,
        [width, height]: [u32; 2],
        output: egui::FullOutput,
    ) {
        self.renderer.render_tonemapped(
            &self.gpu,
            &mut self.tonemap,
            view,
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
            .paint(&self.gpu, &mut encoder, view, [width, height], output);
        self.gpu.queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::HostCaps;
    use crate::scene::camera::OrbitCamera;
    use crate::scene::viewport::SceneViewport;
    use crate::{load_gltf, GpuContext};

    fn studio() -> Option<StudioApp> {
        let gpu = GpuContext::headless_or_skip()?;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets/cube_on_plane.gltf");
        let scene = load_gltf(&path).expect("the test scene loads");
        let caps = HostCaps {
            quit: true,
            screenshot: true,
            hot_reload: true,
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        Some(StudioApp::headless(
            gpu,
            &scene,
            OrbitCamera::default(),
            caps,
            format,
        ))
    }

    fn frame(now_s: f64, events: Vec<egui::Event>) -> OffscreenFrame {
        OffscreenFrame {
            size_px: [800, 450],
            pixels_per_point: 1.0,
            now_s,
            events,
        }
    }

    fn f1() -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key: egui::Key::F1,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .to_vec()
    }

    #[test]
    fn an_offscreen_frame_paints_the_editor_and_centres_the_camera_on_the_scene() {
        let Some(mut app) = studio() else {
            return;
        };
        for step in 0..2 {
            app.render_offscreen(&frame(f64::from(step) / 60.0, Vec::new()))
                .expect("renders");
        }
        let (pixels, signals) = app
            .render_offscreen(&frame(2.0 / 60.0, Vec::new()))
            .expect("renders");
        assert!(signals.is_empty());
        assert_eq!(pixels.len(), 800 * 450 * 4);
        // 800 points drop the scene dock; menu bar 29, status bar 23, the inspector at its 320-point cap, profiler 172.
        let expected = SceneViewport {
            x0: 0.0,
            y0: 29.0 / 450.0,
            x1: 480.0 / 800.0,
            y1: (450.0 - 23.0 - 172.0) / 450.0,
        };
        assert_eq!(app.camera.viewport, expected);
        let pixel = |x: usize, y: usize| {
            let at = (y * 800 + x) * 4;
            [pixels[at], pixels[at + 1], pixels[at + 2]]
        };
        let panel = crate::ui::palette::PANEL.to_array();
        let near = |rgb: [u8; 3]| rgb.iter().zip(panel).all(|(a, b)| a.abs_diff(b) <= 1);
        assert!(
            near(pixel(4, 14)),
            "the menu bar is the panel colour: {:?}",
            pixel(4, 14)
        );
        assert!(!near(pixel(200, 200)), "the scene shows through");
    }

    #[test]
    fn f1_gives_the_camera_the_whole_frame_and_back() {
        let Some(mut app) = studio() else {
            return;
        };
        app.render_offscreen(&frame(0.0, Vec::new()))
            .expect("renders");
        app.render_offscreen(&frame(0.1, f1())).expect("renders");
        assert_eq!(app.camera.viewport, SceneViewport::FULL);
        assert!(!app.editor().is_visible());
        app.render_offscreen(&frame(0.2, f1())).expect("renders");
        assert_ne!(app.camera.viewport, SceneViewport::FULL);
    }
}
