//! egui overlay (stats, light and tonemap controls) over the tonemapped output, native and wasm32.

use winit::window::Window;

pub struct Overlay {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl Overlay {
    pub fn new(window: &Window, device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(device, output_format, Default::default());
        Self {
            ctx,
            state,
            renderer,
        }
    }

    /// Feeds a window event; true when egui consumed it, so camera controls should skip it.
    pub fn on_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    /// Runs the UI closure and paints over the scene already in `view` (loads, never clears).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        window: &Window,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        mut run_ui: impl FnMut(&egui::Context),
    ) {
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: window.scale_factor() as f32,
        };

        let raw_input = self.state.take_egui_input(window);
        // egui 0.35 replaced Context::run with begin_pass/end_pass.
        self.ctx.begin_pass(raw_input);
        run_ui(&self.ctx);
        let output = self.ctx.end_pass();
        self.state
            .handle_platform_output(window, output.platform_output);

        let clipped = self.ctx.tessellate(output.shapes, screen.pixels_per_point);
        paint(
            &mut self.renderer,
            PaintTarget {
                device,
                queue,
                encoder,
                view,
                screen,
            },
            &clipped,
            output.textures_delta,
        );
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
}
