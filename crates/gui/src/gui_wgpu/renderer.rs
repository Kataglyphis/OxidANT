use std::sync::Arc;
use std::time::Duration;

use log::info;
use winit::window::Window;

use super::inference::InferenceState;
use super::inference_bridge::spawn_inference_thread;
use super::wgpu_init::{create_quad_buffers, create_render_pipeline, init_wgpu};

struct FrameTexture {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

pub(crate) struct WgpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    sampler: wgpu::Sampler,
    texture: Option<FrameTexture>,
    egui: super::egui_overlay::EguiOverlay,
    inference: InferenceState,
    current_frame_id: Option<u64>,
    first_frame_logged: bool,
}

/// Draws the camera frame; none has arrived before the first sample, and a draw without its bind group fails wgpu validation.
fn draw_camera_quad(
    rpass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    camera_frame: Option<&wgpu::BindGroup>,
    (vertices, indices): (&wgpu::Buffer, &wgpu::Buffer),
    num_indices: u32,
) {
    let Some(bind_group) = camera_frame else {
        return;
    };
    rpass.set_pipeline(pipeline);
    rpass.set_vertex_buffer(0, vertices.slice(..));
    rpass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint16);
    rpass.set_bind_group(0, bind_group, &[]);
    rpass.draw_indexed(0..num_indices, 0, 0..1);
}

impl WgpuState {
    pub(crate) async fn new(
        window: Arc<Window>,
        display_target: &dyn wgpu::rwh::HasDisplayHandle,
        backends: wgpu::Backends,
    ) -> anyhow::Result<Self> {
        let (surface, device, queue, config) = init_wgpu(&window, backends).await?;

        let (vertex_buffer, index_buffer, num_indices) = create_quad_buffers(&device);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let render_pipeline = create_render_pipeline(&device, config.format);

        let egui = super::egui_overlay::EguiOverlay::new(
            &window,
            display_target,
            &device,
            config.format,
            &config,
        );

        let (infer_tx, infer_rx, detector_error, model_path) = spawn_inference_thread();

        let score_threshold = kataglyphis_core::config::score_threshold();

        let infer_every_ms = kataglyphis_core::config::infer_every_ms();

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            sampler,
            texture: None,
            egui,
            inference: InferenceState {
                detector_error,
                model_path,
                score_threshold,
                infer_every: Duration::from_millis(infer_every_ms),
                last_infer: std::time::Instant::now() - Duration::from_secs(1),
                last_detections: Vec::new(),
                last_detections_frame_id: None,
                infer_tx,
                infer_rx,
                infer_in_flight: false,
                infer_enabled: true,
            },
            current_frame_id: None,
            first_frame_logged: false,
        })
    }

    pub(crate) fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.egui.resize(new_size.width, new_size.height);
        }
    }

    pub(crate) fn handle_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) {
        if self.egui.handle_window_event(window, event) {
            window.request_redraw();
        }
    }

    pub fn poll_inference(&mut self) {
        self.inference.poll();
    }

    pub(crate) fn upload_frame(&mut self, frame: &crate::gui_wgpu::Frame) {
        self.current_frame_id = Some(frame.id);
        self.poll_inference();

        let needs_recreate = match &self.texture {
            Some(ft) => frame.width != ft.width || frame.height != ft.height,
            None => true,
        };

        if needs_recreate {
            let size = wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            };
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame_texture"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("texture_bind_group"),
                layout: &self.render_pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.texture = Some(FrameTexture {
                texture,
                _view: view,
                bind_group,
                width: frame.width,
                height: frame.height,
            });
        }

        if let Some(ft) = &self.texture {
            let layout = wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            };
            let size = wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            };
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ft.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.data,
                layout,
                size,
            );
        }

        self.inference.maybe_infer(frame);
    }

    pub(crate) fn render(&mut self, window: &Window) -> anyhow::Result<()> {
        self.render_overlay_frame(window)?;
        // After the frame's FullOutput dropped: the GUI smoke test waits for this line.
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            info!("First GUI frame finished");
        }
        Ok(())
    }

    fn render_overlay_frame(&mut self, window: &Window) -> anyhow::Result<()> {
        self.inference.poll();

        self.egui.update_stats();

        let frame_dimensions = self
            .texture
            .as_ref()
            .map(|ft| (ft.width, ft.height))
            .unwrap_or((0, 0));

        let mut full_output = self
            .egui
            .run_overlay(window, frame_dimensions, &mut self.inference);

        let shapes = full_output.shapes.clone();
        let clipped_primitives = self
            .egui
            .ctx
            .tessellate(shapes, self.egui.screen.pixels_per_point);

        // Drained, because epaint's drop check counts what stays in the delta as unapplied; one id can carry several, in order.
        for (id, image_deltas) in full_output.textures_delta.set.drain() {
            for image_delta in &image_deltas {
                self.egui
                    .renderer
                    .update_texture(&self.device, &self.queue, id, image_delta);
            }
        }

        let submitted = self.submit_frame(window, &clipped_primitives, &full_output);

        // Also after a failed frame: egui will not ask to free these textures again.
        for id in full_output.textures_delta.free.drain() {
            self.egui.renderer.free_texture(&id);
        }
        submitted?;
        self.egui
            .state
            .handle_platform_output(window, full_output.platform_output);

        Ok(())
    }

    fn submit_frame(
        &mut self,
        _window: &Window,
        clipped_primitives: &[egui::ClippedPrimitive],
        _full_output: &egui::FullOutput,
    ) -> anyhow::Result<()> {
        // Outdated/Lost reconfigure and skip the frame rather than tearing the app down.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            other => anyhow::bail!("surface acquire failed: {other:?}"),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })];

            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            draw_camera_quad(
                &mut rpass,
                &self.render_pipeline,
                self.texture.as_ref().map(|ft| &ft.bind_group),
                (&self.vertex_buffer, &self.index_buffer),
                self.num_indices,
            );
        }

        self.egui.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            clipped_primitives,
            &self.egui.screen,
        );

        {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })];

            let mut egui_rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_render_pass"),
                    color_attachments: &color_attachments,
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                })
                .forget_lifetime();

            self.egui
                .renderer
                .render(&mut egui_rpass, clipped_primitives, &self.egui.screen);
        }

        self.queue.submit(Some(encoder.finish()));
        // wgpu 30: presentation lives on the queue now.
        self.queue.present(frame);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No dev-dependency on the renderer's context: its wgpu features would bring the GL import into this crate's test exes.
    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let required = std::env::var("KATAGLYPHIS_REQUIRE_GPU").is_ok_and(|v| !v.is_empty());
        let adapter = pollster::block_on(
            wgpu::Instance::default().request_adapter(&wgpu::RequestAdapterOptions::default()),
        );
        match adapter {
            Ok(adapter) => Some(
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                    .expect("request a device"),
            ),
            Err(err) => {
                assert!(!required, "KATAGLYPHIS_REQUIRE_GPU is set: {err}");
                eprintln!("SKIP: no GPU adapter available in this environment");
                None
            }
        }
    }

    #[test]
    fn a_frame_before_the_first_camera_sample_passes_validation() {
        let Some((device, queue)) = headless_device() else {
            return;
        };
        let device = &device;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("gui_test_target"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let pipeline = create_render_pipeline(device, format);
        let (vertices, indices, num_indices) = create_quad_buffers(device);

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            draw_camera_quad(
                &mut rpass,
                &pipeline,
                None,
                (&vertices, &indices),
                num_indices,
            );
        }
        // wgpu's default error handler panics on the validation error.
        queue.submit(Some(encoder.finish()));
    }
}
