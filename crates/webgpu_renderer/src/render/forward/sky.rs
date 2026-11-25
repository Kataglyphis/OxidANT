//! The sky, a fullscreen triangle at far depth in the forward or deferred lighting pass: cube map, procedural or navy.

use super::frame::FrameView;
use super::{ForwardRenderer, DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
use crate::asset::skybox::CubemapFaces;
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::clouds::CloudAmbient;
use crate::render::pipeline_desc;
use crate::render::tracer::RenderMode;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyUniforms {
    inv_view_proj: [[f32; 4]; 4],
    light_dir_intensity: [f32; 4],
    /// x: 0 procedural, 1 cube map, 2 switched off.
    mode: [f32; 4],
}

/// What the sky pass draws, `sky.slang`'s `mode.x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyMode {
    Procedural = 0,
    Cubemap = 1,
    Off = 2,
}

pub(super) struct SkyResources {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    bind_group_layout: wgpu::BindGroupLayout,
    /// The module `pipeline` was built from, which the deferred path's single-sample twin is built from too.
    pub(super) shader: wgpu::ShaderModule,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    /// A black cube bound while no faces are loaded, so the layout never changes.
    placeholder: wgpu::TextureView,
    /// The loaded skybox.
    cubemap: Option<wgpu::TextureView>,
    /// The loaded skybox's mean radiance per hemisphere, the clouds' ambient while it shows.
    cubemap_ambient: CloudAmbient,
}

impl SkyResources {
    pub(super) fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;
        // Sky: fullscreen triangle at far depth, drawn only where depth is still the cleared 1.0.
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/sky.wgsl").into()),
        });
        let bind_group_layout = create_sky_bind_group_layout(device);
        let pipeline_layout =
            pipeline_desc::single_layout(device, "sky_pipeline_layout", &bind_group_layout);
        let pipeline = create_sky_pipeline(device, &sky_shader, &pipeline_layout);
        let uniform_buffer = buffer_desc::uniform(
            device,
            "sky_uniforms",
            std::mem::size_of::<SkyUniforms>() as wgpu::BufferAddress,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("skybox_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let placeholder = create_cube_view(gpu, &CubemapFaces::solid([[0, 0, 0, 255]; 6]));
        let bind_group = create_sky_bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            &placeholder,
            &sampler,
        );
        Self {
            pipeline,
            pipeline_layout,
            bind_group_layout,
            shader: sky_shader,
            uniform_buffer,
            bind_group,
            sampler,
            placeholder,
            cubemap: None,
            cubemap_ambient: CloudAmbient::sky_off(),
        }
    }

    /// Binds `cubemap`, or the placeholder for none.
    fn bind(&mut self, device: &wgpu::Device, cubemap: Option<wgpu::TextureView>) {
        self.cubemap = cubemap;
        let view = self.cubemap.as_ref().unwrap_or(&self.placeholder);
        self.bind_group = create_sky_bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buffer,
            view,
            &self.sampler,
        );
    }

    /// Sky fills every pixel geometry left untouched.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_with(pass, &self.pipeline);
    }

    /// As [`Self::draw`] with `pipeline`, whose sample count matches the pass.
    pub(super) fn draw_with(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl ForwardRenderer {
    /// Shows `faces` as the skybox from the next frame on, the C++ engine's cube map.
    pub fn set_skybox(&mut self, gpu: &GpuContext, faces: &CubemapFaces) {
        let view = create_cube_view(gpu, faces);
        self.sky.cubemap_ambient = CloudAmbient::of_cubemap(faces);
        self.sky.bind(&gpu.device, Some(view));
    }

    /// Back to the procedural sky.
    pub fn clear_skybox(&mut self, gpu: &GpuContext) {
        self.sky.bind(&gpu.device, None);
    }

    /// A cube map is loaded; without one the skybox is the procedural sky.
    pub fn skybox_loaded(&self) -> bool {
        self.sky.cubemap.is_some()
    }

    /// What the sky pass draws this frame.
    pub fn sky_mode(&self) -> SkyMode {
        match (self.skybox_enabled, self.sky.cubemap.is_some()) {
            (false, _) => SkyMode::Off,
            (true, true) => SkyMode::Cubemap,
            (true, false) => SkyMode::Procedural,
        }
    }

    /// What the clouds scatter as ambient: the mean radiance of the sky this frame draws.
    pub(super) fn cloud_ambient(&self) -> CloudAmbient {
        // The path tracer's camera misses keep the procedural sky whatever the sky pass would draw.
        if self.drawn_mode() == RenderMode::PathTraced {
            return CloudAmbient::analytic_sky();
        }
        match self.sky_mode() {
            SkyMode::Procedural => CloudAmbient::analytic_sky(),
            SkyMode::Cubemap => self.sky.cubemap_ambient,
            SkyMode::Off => CloudAmbient::sky_off(),
        }
    }

    /// The sky's inverse view-projection, its sun direction and intensity, and what it draws.
    pub(super) fn write_sky_uniforms(&self, gpu: &GpuContext, view: &FrameView) {
        let sky_uniforms = SkyUniforms {
            inv_view_proj: view.view_proj.inverse().to_cols_array_2d(),
            light_dir_intensity: [
                self.light_dir_ambient.x,
                self.light_dir_ambient.y,
                self.light_dir_ambient.z,
                self.light_color_intensity.w,
            ],
            mode: [self.sky_mode() as u32 as f32, 0.0, 0.0, 0.0],
        };
        gpu.queue.write_buffer(
            &self.sky.uniform_buffer,
            0,
            bytemuck::bytes_of(&sky_uniforms),
        );
    }
}

fn create_sky_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky_bind_group_layout"),
        entries: &[
            bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            bind_layout::texture(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::Cube,
                false,
            ),
            bind_layout::sampler(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Filtering,
            ),
        ],
    })
}

fn create_sky_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    cubemap: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(cubemap),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Uploads six sRGB faces as one cube, a single mip level as the C++ uploads them.
fn create_cube_view(gpu: &GpuContext, faces: &CubemapFaces) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: faces.size,
        height: faces.size,
        depth_or_array_layers: 6,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        // TEXTURE_2D_SHAPE_OK: the skybox cube, six layers
        label: Some("skybox_cube"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        texture.as_image_copy(),
        &faces.rgba8,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * faces.size),
            rows_per_image: Some(faces.size),
        },
        size,
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("skybox_cube_view"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

pub(super) fn create_sky_pipeline(
    device: &wgpu::Device,
    sky_shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    sky_pipeline(device, sky_shader, layout, MSAA_SAMPLE_COUNT)
}

/// The sky pipeline for a pass of `samples` samples.
pub(super) fn sky_pipeline(
    device: &wgpu::Device,
    sky_shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    samples: u32,
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
            count: samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod wgsl_layout {
    use super::*;
    use crate::render::pod_layout::{assert_matches_wgsl, pod_layout, wgsl};

    #[test]
    fn sky_uniforms_matches_its_wgsl() {
        assert_matches_wgsl(
            &pod_layout!(SkyUniforms {
                inv_view_proj,
                light_dir_intensity,
                mode
            }),
            &wgsl("sky.wgsl"),
            "SkyUniforms_std140_0",
        );
    }
}
