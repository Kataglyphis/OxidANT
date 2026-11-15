//! The procedural sky: a fullscreen triangle at far depth, drawn inside the forward pass.

use super::frame::FrameView;
use super::{ForwardRenderer, DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::pipeline_desc;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyUniforms {
    inv_view_proj: [[f32; 4]; 4],
    light_dir_intensity: [f32; 4],
}

pub(super) struct SkyResources {
    pub(super) pipeline: wgpu::RenderPipeline,
    // Read only by the native shader hot-reload path, so unread on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl SkyResources {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        // Sky: fullscreen triangle at far depth, drawn only where depth is still the cleared 1.0.
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/sky.wgsl").into()),
        });
        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_bind_group_layout"),
                entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let pipeline_layout =
            pipeline_desc::single_layout(device, "sky_pipeline_layout", &sky_bind_group_layout);
        let pipeline = create_sky_pipeline(device, &sky_shader, &pipeline_layout);
        let uniform_buffer = buffer_desc::uniform(
            device,
            "sky_uniforms",
            std::mem::size_of::<SkyUniforms>() as wgpu::BufferAddress,
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_bind_group"),
            layout: &sky_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            pipeline_layout,
            uniform_buffer,
            bind_group,
        }
    }

    /// Sky fills every pixel geometry left untouched.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl ForwardRenderer {
    /// The sky's inverse view-projection and its sun direction and intensity.
    pub(super) fn write_sky_uniforms(&self, gpu: &GpuContext, view: &FrameView) {
        let sky_uniforms = SkyUniforms {
            inv_view_proj: view.view_proj.inverse().to_cols_array_2d(),
            light_dir_intensity: [
                self.light_dir_ambient.x,
                self.light_dir_ambient.y,
                self.light_dir_ambient.z,
                self.light_color_intensity.w,
            ],
        };
        gpu.queue.write_buffer(
            &self.sky.uniform_buffer,
            0,
            bytemuck::bytes_of(&sky_uniforms),
        );
    }
}

pub(super) fn create_sky_pipeline(
    device: &wgpu::Device,
    sky_shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky_pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: sky_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: sky_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLE_COUNT,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}
