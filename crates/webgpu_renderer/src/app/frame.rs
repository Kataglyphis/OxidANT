//! One frame: acquire, egui pass, its actions, the scene with the camera on the free rect, egui paint, present.

use winit::window::Window;

use super::{AppSignal, StudioApp};
use crate::context::AcquireRetry;
use crate::render::forward::readback;
use crate::render::texture::create_2d_texture;
use crate::render::tracer::RenderMode;
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
        let frame = match self.gpu.acquire() {
            Ok(frame) => frame,
            Err(retry) => {
                // An occluded window asks again when it shows (`Occluded(false)`); every other failure next frame.
                if retry == AcquireRetry::NextFrame {
                    window.request_redraw();
                }
                return Vec::new();
            }
        };
        let size = [frame.texture.width(), frame.texture.height()];
        let raw = self.overlay.take_input(window);
        let (mut output, signals) = self.pass(raw, now_s, size);
        let platform = std::mem::take(&mut output.platform_output);
        self.overlay.handle_platform_output(window, platform);
        // A surface the texture limit shrank is stretched over the window, so egui paints at the same shrink.
        output.pixels_per_point *= self.gpu.surface_scale();
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(&view, size, output);
        window.pre_present_notify();
        self.gpu.present(frame);
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
            fly_speed: &mut self.fly.speed,
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
                // A frame the user asked for is an interaction: frame_scene ends the idle spin.
                EditorAction::FrameScene => self.frame_scene(),
                EditorAction::Quit => signals.push(AppSignal::Quit),
                EditorAction::Screenshot => signals.push(AppSignal::Screenshot),
                EditorAction::ReloadShaders => signals.push(AppSignal::ReloadShaders),
            }
        }
        signals
    }

    /// The frame's time step: the fly camera's held keys, the idle spin and the animation clock.
    fn advance(&mut self, now_s: f64) {
        let elapsed_s = now_s - *self.epoch_s.get_or_insert(now_s);
        let step_s = self.clock.tick_at(now_s);
        self.renderer.frame_delta_seconds = step_s;
        // A spinning camera starts the path tracer's mean over every frame, so picking it stops the spin as a drag does.
        if self.fly.is_engaged() || self.renderer.render_mode == RenderMode::PathTraced {
            self.controller.auto_orbit = false;
        }
        self.fly.advance(&mut self.camera, step_s);
        if self.controller.auto_orbit {
            self.camera.yaw_deg = auto_orbit_yaw(self.home.yaw_deg, elapsed_s);
        }
        if self.renderer.has_animations() {
            self.renderer.set_animation_time(elapsed_s);
        }
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

/// The idle spin's yaw `elapsed_s` after it began, wrapped in f64: f32 degrees step visibly after a few hours.
fn auto_orbit_yaw(home_yaw_deg: f32, elapsed_s: f64) -> f32 {
    let yaw = f64::from(home_yaw_deg) + elapsed_s * f64::from(AUTO_ORBIT_DEG_PER_S);
    yaw.rem_euclid(360.0) as f32
}

#[cfg(test)]
pub(in crate::app) mod tests {
    use super::*;
    use crate::app::HostCaps;
    use crate::scene::camera::OrbitCamera;
    use crate::scene::viewport::SceneViewport;
    use crate::{load_gltf, GpuContext};

    pub(in crate::app) fn studio() -> Option<StudioApp> {
        studio_of("cube_on_plane.gltf")
    }

    fn studio_of(asset: &str) -> Option<StudioApp> {
        let gpu = GpuContext::headless_or_skip()?;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets")
            .join(asset);
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
    fn a_path_traced_pick_stops_the_idle_spin() {
        let Some(mut app) = studio() else {
            return;
        };
        app.render_offscreen(&frame(0.0, Vec::new()))
            .expect("renders");
        let spinning = app.camera.yaw_deg;
        app.render_offscreen(&frame(0.5, Vec::new()))
            .expect("renders");
        assert_ne!(app.camera.yaw_deg, spinning, "the camera spins while idle");
        app.renderer.render_mode = RenderMode::PathTraced;
        let stopped = app.camera.yaw_deg;
        app.render_offscreen(&frame(1.0, Vec::new()))
            .expect("renders");
        assert_eq!(
            app.camera.yaw_deg, stopped,
            "the spin restarts the mean every frame"
        );
    }

    /// The profiler's pass rows scroll, so the scene rect stays put while a traced frame times fewer passes than raster:
    /// a rect that moved as rows left would restart the path tracer's mean every frame until the docks settled.
    #[test]
    fn a_mode_switch_keeps_the_scene_rect_so_the_path_tracers_mean_keeps_growing() {
        let Some(mut app) = studio() else {
            return;
        };
        let mut now_s = 0.0;
        let mut step = |app: &mut StudioApp| {
            let shot = OffscreenFrame {
                size_px: [1600, 900],
                ..frame(now_s, Vec::new())
            };
            now_s += 1.0 / 60.0;
            app.render_offscreen(&shot).expect("renders");
            (
                app.camera.viewport,
                SceneStats::of(&app.renderer).gpu_passes.len(),
            )
        };
        let mut raster = (SceneViewport::FULL, 0);
        for _ in 0..8 {
            raster = step(&mut app);
        }
        app.renderer.render_mode = RenderMode::PathTraced;
        app.renderer.trace_settings.samples_per_pixel = 1;
        app.renderer.trace_settings.max_bounces = 1;
        let built = app
            .renderer
            .tracer_mut()
            .finish_build(&app.gpu, std::time::Duration::from_secs(120));
        assert!(built, "{:?}", app.trace_status());
        let mut traced = raster;
        // Past the timing window, so the raster passes' rows have left the profiler.
        for frame in 1..=40u64 {
            traced = step(&mut app);
            assert_eq!(traced.0, raster.0, "frame {frame} moved the scene rect");
            assert_eq!(
                app.renderer.accumulated_samples(),
                frame,
                "the mean restarted"
            );
        }
        assert!(
            raster.1 > traced.1,
            "the profiler lists {} passes in both modes",
            raster.1
        );
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

    #[test]
    fn the_idle_spin_turns_smoothly_after_days_of_uptime() {
        // Twelve days in, f32 seconds are 1/8 s apart: a 144 Hz spin would stall, then jump 3.75 degrees.
        let base = f64::from(1u32 << 20);
        let yaws: Vec<f32> = (0..8)
            .map(|k| auto_orbit_yaw(45.0, base + f64::from(k) / 144.0))
            .collect();
        for pair in yaws.windows(2) {
            let step = (pair[1] - pair[0]).rem_euclid(360.0);
            assert!(
                (step - AUTO_ORBIT_DEG_PER_S / 144.0).abs() < 1e-3,
                "{yaws:?}"
            );
        }
        assert_eq!(auto_orbit_yaw(45.0, 0.0), 45.0);
        assert_eq!(auto_orbit_yaw(350.0, 1.0), 20.0);
        // Negative control: the old f32 seconds and degrees repeat a frame's yaw.
        let old: Vec<f32> = (0..8)
            .map(|k| 45.0 + (base + f64::from(k) / 144.0) as f32 * AUTO_ORBIT_DEG_PER_S)
            .collect();
        assert!(old.windows(2).any(|pair| pair[1] == pair[0]), "{old:?}");
    }

    #[test]
    fn the_animation_clock_keeps_its_fraction_after_days_of_uptime() {
        let Some(mut app) = studio_of("cube_animated.gltf") else {
            return;
        };
        // Twelve days in, f32 seconds are 1/8 s apart: frames at 144 Hz posed the slide's cube alike.
        app.epoch_s = Some(0.0);
        let base = f64::from(1u32 << 20);
        let lefts: Vec<f32> = (0..4)
            .map(|k| {
                app.advance(base + f64::from(k) / 144.0);
                app.renderer
                    .scene_bounds()
                    .expect("the cube has bounds")
                    .0
                    .x
            })
            .collect();
        for pair in lefts.windows(2) {
            // The cube slides 1.5 m/s, about 10 mm a frame.
            let step = pair[1] - pair[0];
            assert!((step - 1.5 / 144.0).abs() < 1e-3, "{lefts:?}");
        }
    }

    #[test]
    fn held_keys_fly_the_camera_and_end_the_idle_spin() {
        use crate::scene::fly::{FlyInput, FlyKey};
        let Some(mut app) = studio() else {
            return;
        };
        app.render_offscreen(&frame(0.0, Vec::new()))
            .expect("renders");
        let target = app.camera.target;
        let w = FlyInput::Key {
            key: FlyKey::Forward,
            pressed: true,
        };
        app.fly.handle(w, false, &mut app.camera);
        app.render_offscreen(&frame(0.05, Vec::new()))
            .expect("renders");
        assert!(!app.controller.auto_orbit);
        let flown = app.camera.target.distance(target);
        assert!(
            (flown - app.fly_speed() * 0.05).abs() < 1e-4,
            "flew {flown} at {} units/s",
            app.fly_speed()
        );
    }

    #[test]
    fn framing_brings_the_camera_home_and_fits_the_fly_speed_to_the_scene() {
        let Some(mut app) = studio() else {
            return;
        };
        let framing = app.scene_framing().cloned().expect("the cube is framed");
        let expected = framing.fly_speed;
        assert_eq!(
            app.fly_speed(),
            expected,
            "the speed fits the scene from the start"
        );
        assert_eq!(
            expected,
            crate::scene::fly::speed_for_bounds(app.renderer.scene_bounds()),
            "a bounds fit flies a quarter of the diagonal per second"
        );
        assert_ne!(expected, crate::scene::fly::DEFAULT_SPEED);
        app.fly.speed = 123.0;
        app.camera.target = glam::Vec3::splat(50.0);
        app.camera.viewport = SceneViewport {
            x0: 0.2,
            ..SceneViewport::FULL
        };
        let signals = app.act(vec![
            EditorAction::FrameScene,
            EditorAction::Quit,
            EditorAction::Screenshot,
            EditorAction::ReloadShaders,
        ]);
        let shell = [
            AppSignal::Quit,
            AppSignal::Screenshot,
            AppSignal::ReloadShaders,
        ];
        assert_eq!(signals, shell);
        assert_eq!(app.camera.target, framing.camera.target);
        assert_eq!(
            app.camera.viewport.x0, 0.2,
            "the docks still frame the view"
        );
        assert_eq!(app.fly_speed(), expected);
        assert_eq!(app.controller.radius_range, framing.radius_range);
        assert!(
            !app.controller.auto_orbit,
            "a frame asked for ends the idle spin"
        );
    }
}
