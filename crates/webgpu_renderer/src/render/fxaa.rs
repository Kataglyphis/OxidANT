//! FXAA after the tonemap: the tonemap draws into an intermediate of the output's format, this pass filters it out.

use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc::{self, FullscreenPipeline};
use crate::render::texture::create_2d_texture;

/// Which frames FXAA smooths; the deferred path shades one sample, the forward one resolves 4x MSAA.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FxaaMode {
    Off,
    /// Deferred raster frames only, which have no other anti-aliasing.
    #[default]
    Deferred,
    /// Every frame: forward on top of its MSAA, and the traced modes.
    Always,
}

impl FxaaMode {
    /// The editor's order.
    pub const ALL: [Self; 3] = [Self::Off, Self::Deferred, Self::Always];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Deferred => "Deferred only",
            Self::Always => "Always",
        }
    }

    /// Whether a frame filters, `deferred_raster` when it rasters on the deferred path.
    pub fn filters(self, deferred_raster: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Deferred => deferred_raster,
            Self::Always => true,
        }
    }
}

/// The tonemapped frame FXAA reads, sized to the output.
struct Intermediate {
    size: (u32, u32),
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

pub struct FxaaPass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    format: wgpu::TextureFormat,
    intermediate: Option<Intermediate>,
}

impl FxaaPass {
    /// `format` is the output's, which the intermediate shares so one tonemap pipeline draws either; `wgsl` is `fxaa.wgsl`.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat, wgsl: &str) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fxaa_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fxaa_bind_group_layout"),
            entries: &[
                bind_layout::texture_2d(0, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::sampler(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::SamplerBindingType::Filtering,
                ),
                bind_layout::uniform(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let layout =
            pipeline_desc::single_layout(device, "fxaa_pipeline_layout", &bind_group_layout);
        let pipeline = pipeline_desc::create_fullscreen_pipeline(
            device,
            FullscreenPipeline {
                label: "fxaa_pipeline",
                layout: &layout,
                module: &shader,
                vs_entry: "vs_main",
                fs_entry: "fs_main",
                format,
                blend: None,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fxaa_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer: buffer_desc::uniform(device, "fxaa_uniforms", 16),
            format,
            intermediate: None,
        }
    }

    /// (Re)allocates the intermediate for a `width` x `height` output; a no-op at the same size.
    pub fn prepare(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if self.intermediate.as_ref().is_some_and(|i| i.size == size) {
            return;
        }
        let texture = create_2d_texture(
            &gpu.device,
            Some("fxaa_input"),
            size.0,
            size.1,
            self.format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
            1,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fxaa_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let linear = if self.format.is_srgb() { 1.0 } else { 0.0 };
        let params = [1.0 / size.0 as f32, 1.0 / size.1 as f32, linear, 0.0];
        gpu.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&params));
        self.intermediate = Some(Intermediate {
            size,
            view,
            bind_group,
        });
    }

    /// The target the tonemap draws into while FXAA is on; `None` before [`Self::prepare`].
    pub fn input_view(&self) -> Option<&wgpu::TextureView> {
        self.intermediate.as_ref().map(|i| &i.view)
    }

    /// Filters the intermediate into `output_view`, which must be the size [`Self::prepare`] was given.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        scope: PassScope<'_>,
    ) {
        let Some(intermediate) = self.intermediate.as_ref() else {
            log::error!("FxaaPass::render called before prepare");
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fxaa_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: scope.render_writes(0, 1),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &intermediate.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
