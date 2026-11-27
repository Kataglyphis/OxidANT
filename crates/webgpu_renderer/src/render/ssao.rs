//! Half-resolution depth-only SSAO with a 3x3 blur into targets the frame graph's pool backs; the lit passes' ambient takes it.

use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc::{self, FullscreenPipeline};

/// SSAO's raw and blurred occlusion.
pub(crate) const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SsaoUniforms {
    proj: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],
    params: [f32; 4],
}

/// The occlusion and blur pipelines from one WGSL module; a shader reload builds them inside its error scope.
pub struct SsaoPipelines {
    ssao: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
}

impl SsaoPipelines {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, wgsl: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssao_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let layout = pipeline_desc::single_layout(device, "ssao_pipeline_layout", layout);
        let make = |entry: &str, label: &str| {
            pipeline_desc::create_fullscreen_pipeline(
                device,
                FullscreenPipeline {
                    label,
                    layout: &layout,
                    module: &shader,
                    vs_entry: "vs_main",
                    fs_entry: entry,
                    format: AO_FORMAT,
                    blend: None,
                },
            )
        };
        Self {
            ssao: make("fs_ssao", "ssao_pipeline"),
            blur: make("fs_blur", "ssao_blur_pipeline"),
        }
    }
}

/// What SSAO reads and writes this frame: a depth, the raw occlusion and the blurred one the lit passes sample.
#[derive(Clone, Copy)]
pub struct SsaoTargets<'a> {
    pub depth: &'a wgpu::TextureView,
    pub raw: &'a wgpu::TextureView,
    pub blurred: &'a wgpu::TextureView,
}

/// The targets and the groups that read them, held while the frame graph backs them.
struct Bound {
    raw: wgpu::TextureView,
    blurred: wgpu::TextureView,
    ssao: wgpu::BindGroup,
    blur: wgpu::BindGroup,
}

pub struct SsaoPass {
    pipelines: SsaoPipelines,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    bound: Option<Bound>,
}

impl SsaoPass {
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssao_bind_group_layout"),
            entries: &[
                bind_layout::texture(
                    0,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2,
                    false,
                ),
                bind_layout::uniform(1, wgpu::ShaderStages::FRAGMENT),
                bind_layout::texture_2d(2, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let shipped = include_str!("../shaders/ssao.wgsl");
        let pipelines = SsaoPipelines::new(device, &bind_group_layout, shipped);

        let uniform_buffer = buffer_desc::uniform(
            device,
            "ssao_uniforms",
            std::mem::size_of::<SsaoUniforms>() as wgpu::BufferAddress,
        );

        Self {
            pipelines,
            bind_group_layout,
            uniform_buffer,
            bound: None,
        }
    }

    /// The pipelines `wgsl` builds against this pass's layout, for [`Self::replace_pipelines`].
    pub fn compile(&self, device: &wgpu::Device, wgsl: &str) -> SsaoPipelines {
        SsaoPipelines::new(device, &self.bind_group_layout, wgsl)
    }

    /// Swaps in a reload's pipelines; the targets and bind groups stay.
    pub fn replace_pipelines(&mut self, pipelines: SsaoPipelines) {
        self.pipelines = pipelines;
    }

    /// Binds `targets`, the pool's this frame; `None` lets the bound ones go.
    pub fn bind(&mut self, device: &wgpu::Device, targets: Option<SsaoTargets<'_>>) {
        let Some(targets) = targets else {
            self.bound = None;
            return;
        };
        let bind = |ao_input: &wgpu::TextureView, label: &str| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(targets.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(ao_input),
                    },
                ],
            })
        };
        // The ssao pass ignores binding 2; any non-target texture satisfies the layout.
        self.bound = Some(Bound {
            ssao: bind(targets.blurred, "ssao_bg"),
            blur: bind(targets.raw, "ssao_blur_bg"),
            raw: targets.raw.clone(),
            blurred: targets.blurred.clone(),
        });
    }

    /// Per-frame camera matrices + tuning.
    pub fn write_uniforms(
        &self,
        queue: &wgpu::Queue,
        proj: glam::Mat4,
        radius: f32,
        bias: f32,
        intensity: f32,
    ) {
        let uniforms = SsaoUniforms {
            proj: proj.to_cols_array_2d(),
            inv_proj: proj.inverse().to_cols_array_2d(),
            params: [radius, bias, intensity, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Records the occlusion and its blur, sub-passes `first` and `first + 1` of `count`; nothing while unbound.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scope: PassScope<'_>,
        (first, count): (usize, usize),
    ) {
        let Some(bound) = self.bound.as_ref() else {
            log::error!("SSAO ran on a frame that backs no AO targets");
            return;
        };
        let steps: [(&wgpu::RenderPipeline, &wgpu::BindGroup, &wgpu::TextureView); 2] = [
            (&self.pipelines.ssao, &bound.ssao, &bound.raw),
            (&self.pipelines.blur, &bound.blur, &bound.blurred),
        ];
        for (step, (pipeline, bind_group, target)) in steps.into_iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ssao_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: scope.render_writes(first + step, count),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod wgsl_layout {
    use super::*;
    use crate::render::pod_layout::{assert_matches_wgsl, pod_layout, wgsl};

    #[test]
    fn ssao_uniforms_matches_its_wgsl() {
        assert_matches_wgsl(
            &pod_layout!(SsaoUniforms {
                proj,
                inv_proj,
                params
            }),
            &wgsl("ssao.wgsl"),
            "SsaoUniforms_std140_0",
        );
    }
}
