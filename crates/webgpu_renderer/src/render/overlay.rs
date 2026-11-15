//! egui over the tonemapped output, native and wasm32: one pass per frame, painted over the finished scene.

use std::num::NonZeroUsize;

use winit::window::Window;

use crate::context::GpuContext;
use crate::render::forward::ForwardRenderer;

pub struct Overlay {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl Overlay {
    pub fn new(window: &Window, device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let ctx = studio_context();
        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            Some(winit::window::Theme::Dark),
            Some(max_texture_side),
        );
        let renderer = egui_wgpu::Renderer::new(device, output_format, Default::default());
        Self {
            ctx,
            state,
            renderer,
        }
    }

    /// The egui context, to read focus and popups before a pass.
    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Feeds a window event; true when egui consumed it, so camera controls should skip it.
    pub fn on_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    /// Runs one pass of `ui` over the window's input; paint its output with [`Self::paint`].
    pub fn run<R: Default>(
        &mut self,
        window: &Window,
        ui: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (egui::FullOutput, R) {
        let raw_input = self.state.take_egui_input(window);
        let (mut output, result) = run_pass(&self.ctx, raw_input, ui);
        let platform_output = std::mem::take(&mut output.platform_output);
        self.state.handle_platform_output(window, platform_output);
        (output, result)
    }

    /// Paints a pass over the scene already in `view` (loads, never clears).
    pub fn paint(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
        output: egui::FullOutput,
    ) {
        let screen = screen_for(&output, size_in_pixels);
        let egui::FullOutput {
            textures_delta,
            shapes,
            ..
        } = output;
        let clipped = self.ctx.tessellate(shapes, screen.pixels_per_point);
        let target = PaintTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder,
            view,
            screen,
        };
        paint(&mut self.renderer, target, &clipped, textures_delta);
    }
}

/// A context with the studio look that runs exactly one pass per frame, so the UI's actions happen once.
pub(crate) fn studio_context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::ui::install(&ctx);
    ctx.options_mut(|options| options.max_passes = NonZeroUsize::MIN);
    ctx
}

/// One pass of `ui`; `run_ui` rather than `begin_pass`/`end_pass`, which would skip egui's own plugins.
pub(crate) fn run_pass<R: Default>(
    ctx: &egui::Context,
    raw_input: egui::RawInput,
    ui: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::FullOutput, R) {
    let mut ui = Some(ui);
    let mut result = None;
    let output = ctx.run_ui(raw_input, |root| {
        if let Some(ui) = ui.take() {
            result = Some(ui(root));
        }
    });
    (output, result.unwrap_or_default())
}

/// The pass's own pixels per point: egui lays out at zoom times the window's scale factor.
fn screen_for(output: &egui::FullOutput, size_in_pixels: [u32; 2]) -> egui_wgpu::ScreenDescriptor {
    egui_wgpu::ScreenDescriptor {
        size_in_pixels,
        pixels_per_point: output.pixels_per_point,
    }
}

/// Where one overlay pass paints.
struct PaintTarget<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    screen: egui_wgpu::ScreenDescriptor,
}

/// Applies the frame's texture deltas around one egui pass, which loads and never clears.
fn paint(
    renderer: &mut egui_wgpu::Renderer,
    target: PaintTarget<'_>,
    clipped: &[egui::ClippedPrimitive],
    mut textures: egui::TexturesDelta,
) {
    let PaintTarget {
        device,
        queue,
        encoder,
        view,
        screen,
    } = target;
    // Drained, because epaint's drop check counts what stays in the delta as unapplied; one id can carry several, in order.
    for (id, image_deltas) in textures.set.drain() {
        for image_delta in &image_deltas {
            renderer.update_texture(device, queue, id, image_delta);
        }
    }
    renderer.update_buffers(device, queue, encoder, clipped, &screen);

    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, clipped, &screen);
    }

    for id in textures.free.drain() {
        renderer.free_texture(&id);
    }
}

/// State backing the standard demo control panel.
pub struct OverlayControls {
    pub light_azimuth_deg: f32,
    pub light_elevation_deg: f32,
    pub intensity: f32,
    pub ambient: f32,
    pub bloom: f32,
    pub ssao: f32,
    pub exposure_ev: f32,
    pub occlusion_culling: bool,
    /// Last frame's (drawn, considered) opaque-primitive counts, set by the caller; `None` hides it.
    pub occlusion_stats: Option<(u32, u32)>,
}

impl OverlayControls {
    pub fn from_light(light_dir_ambient: glam::Vec4, light_color_intensity: glam::Vec4) -> Self {
        let dir = light_dir_ambient.truncate();
        Self {
            light_azimuth_deg: dir.z.atan2(dir.x).to_degrees(),
            light_elevation_deg: dir
                .y
                .atan2((dir.x * dir.x + dir.z * dir.z).sqrt())
                .to_degrees(),
            intensity: light_color_intensity.w,
            ambient: light_dir_ambient.w,
            bloom: 0.6,
            ssao: 0.7,
            exposure_ev: 0.0,
            occlusion_culling: false,
            occlusion_stats: None,
        }
    }

    /// Starts from the renderer's own values, so the first change does not move the other settings.
    pub fn from_renderer(renderer: &ForwardRenderer) -> Self {
        Self {
            bloom: renderer.bloom_strength,
            ssao: renderer.ssao_strength,
            exposure_ev: renderer.exposure_ev,
            occlusion_culling: renderer.occlusion_queries_enabled,
            ..Self::from_light(renderer.light_dir_ambient, renderer.light_color_intensity)
        }
    }

    /// Writes every control into the renderer.
    pub fn apply(&self, renderer: &mut ForwardRenderer) {
        let dir = self.light_direction();
        renderer.light_dir_ambient = glam::Vec4::new(dir.x, dir.y, dir.z, self.ambient);
        renderer.light_color_intensity.w = self.intensity;
        renderer.bloom_strength = self.bloom;
        renderer.ssao_strength = self.ssao;
        renderer.exposure_ev = self.exposure_ev;
        renderer.occlusion_queries_enabled = self.occlusion_culling;
    }

    /// Direction TOWARDS the light from azimuth/elevation.
    pub fn light_direction(&self) -> glam::Vec3 {
        let az = self.light_azimuth_deg.to_radians();
        let el = self.light_elevation_deg.to_radians();
        glam::Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin())
    }

    /// Draws the panel and returns true when a value changed.
    pub fn ui(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        egui::Window::new("Renderer")
            .default_open(true)
            .resizable(false)
            .show(ctx, |ui| {
                let dt = ctx.input(|i| i.stable_dt).max(1e-6);
                ui.label(format!("{:.1} ms/frame ({:.0} FPS)", dt * 1000.0, 1.0 / dt));
                ui.separator();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.light_azimuth_deg, -180.0..=180.0)
                            .text("light azimuth"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.light_elevation_deg, 5.0..=89.0)
                            .text("light elevation"),
                    )
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut self.intensity, 0.0..=20.0).text("intensity"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut self.ambient, 0.0..=2.0).text("IBL strength"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut self.bloom, 0.0..=2.0).text("bloom"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut self.ssao, 0.0..=1.0).text("SSAO"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut self.exposure_ev, -4.0..=4.0).text("exposure EV"))
                    .changed();
                changed |= ui
                    .checkbox(&mut self.occlusion_culling, "occlusion culling")
                    .changed();
                if self.occlusion_culling {
                    if let Some((drawn, considered)) = self.occlusion_stats {
                        let culled = considered.saturating_sub(drawn);
                        ui.label(format!("  drew {drawn}/{considered} ({culled} culled)"));
                    }
                }
                ui.separator();
                ui.label("drag: orbit - wheel: zoom");
            });
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GpuContext;

    #[test]
    fn painting_takes_the_first_frames_font_texture_out_of_its_delta() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        let (device, queue) = (&gpu.device, &gpu.queue);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let size = wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("overlay_test_target"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut renderer = egui_wgpu::Renderer::new(device, format, Default::default());

        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("overlay");
        });
        assert!(
            !output.textures_delta.is_empty(),
            "a first frame must upload the font atlas, or this test proves nothing"
        );
        let font = egui::TextureId::default();
        assert!(renderer.texture(&font).is_none());
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point: 1.0,
        };
        let clipped = ctx.tessellate(output.shapes, screen.pixels_per_point);
        // A delta left in place panics in its Drop on this debug test profile.
        paint(
            &mut renderer,
            PaintTarget {
                device,
                queue,
                encoder: &mut encoder,
                view: &view,
                screen,
            },
            &clipped,
            output.textures_delta,
        );

        assert!(renderer.texture(&font).is_some());
        queue.submit(Some(encoder.finish()));
    }

    /// Input for one frame of a 1600x900-point window at `native_ppp` pixels per point.
    fn window_input(native_ppp: f32, events: Vec<egui::Event>) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let root = input.viewports.entry(egui::ViewportId::ROOT).or_default();
        root.native_pixels_per_point = Some(native_ppp);
        input
    }

    #[test]
    fn the_paint_scale_is_the_zoomed_one_not_the_windows() {
        let ctx = studio_context();
        ctx.set_zoom_factor(2.0);
        // A new zoom factor takes effect at the start of the next pass.
        for _ in 0..2 {
            let (output, ()) = run_pass(&ctx, window_input(1.5, Vec::new()), |ui| {
                ui.label("zoomed");
            });
            let screen = screen_for(&output, [2400, 1350]);
            output.drop_without_applying_deltas();
            assert_eq!(screen.size_in_pixels, [2400, 1350]);
            if ctx.zoom_factor() == 2.0 {
                assert_eq!(screen.pixels_per_point, 3.0);
                return;
            }
        }
        panic!("the zoom factor never took effect");
    }

    #[test]
    fn ctrl_plus_does_not_zoom_the_ui() {
        let ctx = studio_context();
        let plus = egui::Event::Key {
            key: egui::Key::Plus,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        for events in [vec![plus], Vec::new()] {
            let (output, ()) = run_pass(&ctx, window_input(1.0, events), |_| {});
            output.drop_without_applying_deltas();
        }
        assert_eq!(ctx.zoom_factor(), 1.0);
        assert_eq!(ctx.pixels_per_point(), 1.0);
    }

    #[test]
    fn a_pass_runs_the_ui_once_even_when_a_widget_asks_for_another() {
        let ctx = studio_context();
        let mut calls = 0;
        let (output, value) = run_pass(&ctx, window_input(1.0, Vec::new()), |ui| {
            calls += 1;
            ui.ctx()
                .request_discard("a second pass would rerun the editor's actions");
            7
        });
        output.drop_without_applying_deltas();
        assert_eq!((calls, value), (1, 7));
    }

    #[test]
    fn the_controls_start_from_the_renderer_and_write_back_what_they_show() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };
        let mut renderer = ForwardRenderer::new(&gpu, 8, 8);
        renderer.light_dir_ambient = glam::Vec4::new(0.5, 0.8, 0.3, 0.35);
        renderer.bloom_strength = 0.25;
        renderer.occlusion_queries_enabled = true;
        let mut controls = OverlayControls::from_renderer(&renderer);
        assert_eq!(controls.bloom, 0.25);
        assert!(controls.occlusion_culling);
        controls.ssao = 0.1;
        controls.apply(&mut renderer);
        assert_eq!(
            (renderer.ssao_strength, renderer.bloom_strength),
            (0.1, 0.25)
        );
        let dir = renderer.light_dir_ambient.truncate().normalize();
        let expected = glam::Vec3::new(0.5, 0.8, 0.3).normalize();
        assert!(dir.abs_diff_eq(expected, 1e-4), "{dir} vs {expected}");
    }
}
