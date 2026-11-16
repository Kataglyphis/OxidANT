//! Half-resolution bloom (bright-pass, H blur, V blur) over two ping-ponged Rgba16Float textures.

use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::forward::HDR_FORMAT;
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc::{self, FullscreenPipeline};
use crate::render::texture::create_2d_view;

/// What the bright pass reads: the HDR scene, the exposure and the clouds it composites first.
#[derive(Clone, Copy)]
pub struct BloomInputs<'a> {
    pub hdr: &'a wgpu::TextureView,
    pub exposure: &'a wgpu::Buffer,
    pub cloud: &'a wgpu::TextureView,
}

/// The bright pass and both blurs, from one WGSL module; a shader reload builds them inside its error scope.
pub struct BloomPipelines {
    brightpass: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
}

impl BloomPipelines {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, wgsl: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let layout = pipeline_desc::single_layout(device, "bloom_pipeline_layout", layout);
        let make = |entry: &str, label: &str| {
            pipeline_desc::create_fullscreen_pipeline(
                device,
                FullscreenPipeline {
                    label,
                    layout: &layout,
                    module: &shader,
                    vs_entry: "vs_main",
                    fs_entry: entry,
                    format: HDR_FORMAT,
                    blend: None,
                },
            )
        };
        Self {
            brightpass: make("fs_brightpass", "bloom_brightpass"),
            blur_h: make("fs_blur_h", "bloom_blur_h"),
            blur_v: make("fs_blur_v", "bloom_blur_v"),
        }
    }
}

pub struct BloomPass {
    pipelines: BloomPipelines,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Whether the bright pass composites the clouds first, written every frame.
    uniform_buffer: wgpu::Buffer,
    texture_a: Option<wgpu::TextureView>,
    texture_b: Option<wgpu::TextureView>,
    bg_bright: Option<wgpu::BindGroup>,
    bg_h: Option<wgpu::BindGroup>,
    bg_v: Option<wgpu::BindGroup>,
    size: (u32, u32),
}

impl BloomPass {
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom_bind_group_layout"),
            entries: &[
                bind_layout::texture_2d(0, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::sampler(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::SamplerBindingType::Filtering,
                ),
                // Exposure: only fs_brightpass reads it, but all three pipelines share one layout.
                bind_layout::storage_buffer(2, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::texture_2d(3, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::uniform(4, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let shipped = include_str!("../shaders/bloom.wgsl");
        let pipelines = BloomPipelines::new(device, &bind_group_layout, shipped);

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipelines,
            bind_group_layout,
            sampler,
            uniform_buffer: buffer_desc::uniform(device, "bloom_uniforms", 16),
            texture_a: None,
            texture_b: None,
            bg_bright: None,
            bg_h: None,
            bg_v: None,
            size: (0, 0),
        }
    }

    /// (Re)creates the half-res chain over `inputs`, the views and buffer `TonemapPass::set_input` takes too.
    pub fn rebuild(&mut self, gpu: &GpuContext, width: u32, height: u32, inputs: BloomInputs<'_>) {
        let (w, h) = ((width / 2).max(1), (height / 2).max(1));
        self.size = (w, h);
        let make_tex = |label: &str| {
            create_2d_view(
                &gpu.device,
                Some(label),
                w,
                h,
                HDR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                1,
                1,
            )
        };
        let a = make_tex("bloom_a");
        let b = make_tex("bloom_b");

        let bind = |src: &wgpu::TextureView, label: &str| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: inputs.exposure.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(inputs.cloud),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                ],
            })
        };
        self.bg_bright = Some(bind(inputs.hdr, "bloom_bg_bright"));
        self.bg_h = Some(bind(&a, "bloom_bg_h"));
        self.bg_v = Some(bind(&b, "bloom_bg_v"));
        self.texture_a = Some(a);
        self.texture_b = Some(b);
    }

    /// The pipelines `wgsl` builds against this pass's layout, for [`Self::replace_pipelines`].
    pub fn compile(&self, device: &wgpu::Device, wgsl: &str) -> BloomPipelines {
        BloomPipelines::new(device, &self.bind_group_layout, wgsl)
    }

    /// Swaps in a reload's pipelines; the targets and bind groups stay.
    pub fn replace_pipelines(&mut self, pipelines: BloomPipelines) {
        self.pipelines = pipelines;
    }

    /// Per frame: whether the bright pass composites the cloud input over the HDR first.
    pub fn set_clouds(&self, queue: &wgpu::Queue, clouds: bool) {
        let params = [if clouds { 1.0_f32 } else { 0.0 }, 0.0, 0.0, 0.0];
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&params));
    }

    /// The blurred bloom output (valid after `rebuild` + `encode`).
    pub fn output(&self) -> Option<&wgpu::TextureView> {
        self.texture_a.as_ref()
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, scope: PassScope<'_>) {
        let (Some(a), Some(b), Some(bg_bright), Some(bg_h), Some(bg_v)) = (
            self.texture_a.as_ref(),
            self.texture_b.as_ref(),
            self.bg_bright.as_ref(),
            self.bg_h.as_ref(),
            self.bg_v.as_ref(),
        ) else {
            return;
        };
        let steps: [(&wgpu::RenderPipeline, &wgpu::BindGroup, &wgpu::TextureView); 3] = [
            (&self.pipelines.brightpass, bg_bright, a),
            (&self.pipelines.blur_h, bg_h, b),
            (&self.pipelines.blur_v, bg_v, a),
        ];
        let step_count = steps.len();
        for (step, (pipeline, bind_group, target)) in steps.into_iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: scope.render_writes(step, step_count),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
