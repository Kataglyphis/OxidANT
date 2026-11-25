//! Group 1 of the forward pipeline: the IBL maps, baked from an environment or the 1x1 fallbacks.

use wgpu::util::DeviceExt as _;

use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::ibl::{BrdfLut, IblEnvironment, IblFallback};

/// Group 1 uniforms, mirroring `IblParams` in forward.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct IblUniforms {
    // x: enabled flag, y: highest prefiltered mip, z: intensity, w: unused
    enabled_maxmip_intensity: [f32; 4],
}

impl IblUniforms {
    pub(super) fn disabled() -> Self {
        Self {
            enabled_maxmip_intensity: [0.0, 0.0, 1.0, 0.0],
        }
    }

    pub(super) fn enabled(max_mip: f32, intensity: f32) -> Self {
        Self {
            enabled_maxmip_intensity: [1.0, max_mip, intensity, 0.0],
        }
    }
}

pub(super) struct IblBinding {
    /// Group 1 of the forward pipeline: the IBL maps, bound once per pass.
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
    /// 1x1 stand-ins so group 1 is always bindable, environment or not.
    pub(super) fallback: IblFallback,
    /// `None` until `ForwardRenderer::set_environment`; the analytic fallback runs until then.
    pub(super) environment: Option<IblEnvironment>,
    /// Environment-independent, baked lazily once so a renderer without an environment never pays.
    pub(super) brdf_lut: Option<BrdfLut>,
    /// Bumped by every set or clear, so the path tracer's mean starts over.
    pub(super) generation: u64,
}

impl IblBinding {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = create_ibl_bind_group_layout(device);
        let fallback = IblFallback::new(device);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibl_params"),
            contents: bytemuck::bytes_of(&IblUniforms::disabled()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = create_ibl_bind_group(
            device,
            &layout,
            &uniform_buffer,
            &fallback.irradiance,
            &fallback.prefiltered,
            &fallback.brdf_lut,
            &fallback.sampler,
        );
        Self {
            layout,
            uniform_buffer,
            bind_group,
            fallback,
            environment: None,
            brdf_lut: None,
            generation: 0,
        }
    }

    /// Group 1 over the 1x1 fallback maps, for the analytic path.
    pub(super) fn fallback_bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        create_ibl_bind_group(
            device,
            &self.layout,
            &self.uniform_buffer,
            &self.fallback.irradiance,
            &self.fallback.prefiltered,
            &self.fallback.brdf_lut,
            &self.fallback.sampler,
        )
    }
}

impl ForwardRenderer {
    /// The IBL uniforms for the bound environment; per frame, since `ibl_intensity` is a live knob.
    pub(super) fn write_ibl_uniforms(&self, gpu: &GpuContext) {
        let uniforms = match &self.ibl.environment {
            Some(environment) => {
                IblUniforms::enabled(environment.max_prefiltered_mip(), self.ibl_intensity)
            }
            None => IblUniforms::disabled(),
        };
        gpu.queue
            .write_buffer(&self.ibl.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }
}

fn create_ibl_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding: u32, dimension: wgpu::TextureViewDimension| {
        bind_layout::texture(
            binding,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Float { filterable: true },
            dimension,
            false,
        )
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ibl_forward_bind_group_layout"),
        entries: &[
            bind_layout::uniform(0, wgpu::ShaderStages::FRAGMENT),
            texture(1, wgpu::TextureViewDimension::Cube),
            texture(2, wgpu::TextureViewDimension::Cube),
            texture(3, wgpu::TextureViewDimension::D2),
            bind_layout::sampler(
                4,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Filtering,
            ),
        ],
    })
}

pub(super) fn create_ibl_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    irradiance: &wgpu::TextureView,
    prefiltered: &wgpu::TextureView,
    brdf_lut: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ibl_forward_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(irradiance),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(prefiltered),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(brdf_lut),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
mod wgsl_layout {
    use super::*;
    use crate::render::pod_layout::{assert_matches_wgsl, pod_layout, wgsl};

    #[test]
    fn ibl_uniforms_matches_its_wgsl() {
        assert_matches_wgsl(
            &pod_layout!(IblUniforms {
                enabled_maxmip_intensity
            }),
            &wgsl("forward.wgsl"),
            "IblParams_std140_0",
        );
    }
}
