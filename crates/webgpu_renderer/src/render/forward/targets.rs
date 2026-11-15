//! The size-dependent render targets and the pass that resolves MSAA depth to single-sample.

use super::{ForwardRenderer, DEPTH_FORMAT, MSAA_SAMPLE_COUNT};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::pipeline_desc;
use crate::render::texture::{create_depth_texture, create_hdr_texture};

/// The forward pass's MSAA targets and the single-sample ones the later passes read.
pub(super) struct RenderTargets {
    pub(super) depth: wgpu::TextureView,
    pub(super) hdr_view: wgpu::TextureView,
    /// MSAA depth of the forward pass, resolved into `depth` by the depth-resolve pass.
    pub(super) depth_msaa: wgpu::TextureView,
    /// MSAA HDR color of the forward pass, auto-resolved into `hdr_view`.
    pub(super) hdr_msaa: wgpu::TextureView,
    pub(super) size: (u32, u32),
}

impl RenderTargets {
    pub(super) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            depth: create_depth_texture(device, width, height, 1),
            hdr_view: create_hdr_texture(device, width, height, 1),
            depth_msaa: create_depth_texture(device, width, height, MSAA_SAMPLE_COUNT),
            hdr_msaa: create_hdr_texture(device, width, height, MSAA_SAMPLE_COUNT),
            size: (width.max(1), height.max(1)),
        }
    }
}

/// Pipeline and bind group for the MSAA depth → single-sample resolve.
pub(super) struct DepthResolve {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl DepthResolve {
    pub(super) fn new(device: &wgpu::Device, depth_msaa: &wgpu::TextureView) -> Self {
        let (pipeline, bind_group_layout) = create_depth_resolve_pipeline(device);
        Self {
            pipeline,
            bind_group: create_depth_resolve_bind_group(device, &bind_group_layout, depth_msaa),
            bind_group_layout,
        }
    }
}

impl ForwardRenderer {
    /// Recreates the size-dependent targets when the output size changed.
    pub(super) fn resize_targets(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if self.targets.size == (width, height) {
            return;
        }
        self.targets = RenderTargets::new(&gpu.device, width, height);
        // Recreate depth resolve bind group with new MSAA depth view.
        self.depth_resolve.bind_group = create_depth_resolve_bind_group(
            &gpu.device,
            &self.depth_resolve.bind_group_layout,
            &self.targets.depth_msaa,
        );
        // A stale view here reads a destroyed texture.
        self.post.histogram.set_input(gpu, &self.targets.hdr_view);
        self.post.rebound_needed = true;
    }

    /// Depth resolve: MSAA depth to single-sample (min over samples) for SSAO and occlusion.
    pub(super) fn encode_depth_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth_resolve"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        resolve.set_pipeline(&self.depth_resolve.pipeline);
        resolve.set_bind_group(0, &self.depth_resolve.bind_group, &[]);
        resolve.draw(0..3, 0..1);
    }
}

fn create_depth_resolve_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    depth_msaa: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("depth_resolve_bind_group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(depth_msaa),
        }],
    })
}

fn create_depth_resolve_pipeline(
    device: &wgpu::Device,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth_resolve_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/depth_resolve.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("depth_resolve_bind_group_layout"),
        entries: &[bind_layout::texture(
            0,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Depth,
            wgpu::TextureViewDimension::D2,
            true,
        )],
    });

    let pipeline_layout =
        pipeline_desc::single_layout(device, "depth_resolve_pipeline_layout", &bind_group_layout);

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("depth_resolve_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}
