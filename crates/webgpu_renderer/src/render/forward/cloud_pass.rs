//! The volumetric clouds: a noise volume generated once, then per frame a full-screen march to the scene depth.
//! The tonemap composites the march's premultiplied output, in linear HDR, over sky and geometry alike;
//! bloom's bright pass does the same first, so only what shows through the clouds glows.

use glam::Vec3;

use super::frame::FrameView;
use super::readback::{read_texture, TextureRegion};
use super::{ForwardRenderer, RasterPath};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::clouds::{
    CloudParams, NOISE_VOLUME_EXTENT, NOISE_VOLUME_FORMAT, NOISE_WORKGROUP_SIZE,
};
use crate::render::gpu_timing::TimedPass;
use crate::render::pipeline_desc::{self, FullscreenPipeline};
use crate::render::texture::create_2d_texture;

/// Premultiplied cloud colour and 1 - transmittance; the C++ engine's `cloudOutputTexture` format.
const CLOUD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudFrameUniforms {
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    to_sun: [f32; 4],
    sun_radiance: [f32; 4],
    params: CloudParams,
    /// x: 1 while binding 4 holds this frame's MSAA depth spread; y: 1 for a half-resolution march.
    depth_spread: [f32; 4],
    /// The drawn sky's mean radiance over its upper and its lower hemisphere.
    ambient_top: [f32; 4],
    ambient_bottom: [f32; 4],
}

/// The march, upsample and noise pipelines with their layouts, from one pair of WGSL modules.
pub(super) struct CloudPipelines {
    march: wgpu::RenderPipeline,
    march_layout: wgpu::BindGroupLayout,
    upsample: wgpu::RenderPipeline,
    upsample_layout: wgpu::BindGroupLayout,
    noise: wgpu::ComputePipeline,
    noise_layout: wgpu::BindGroupLayout,
}

/// The noise volume, made the first frame clouds are on and kept, as the C++ engine keeps it.
struct NoiseVolume {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The write-only storage binding the noise pipeline fills the volume through.
    bind_group: wgpu::BindGroup,
    /// Until a frame has recorded the fill; a shader reload sets it again.
    pending: bool,
}

/// The clouds the tonemap composites, at the frame's size, and the march's bind group over its depths.
struct CloudTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
    /// Compared by identity: resized targets can come back at an old size with a new depth texture.
    depths: SceneDepths,
    bind_group: wgpu::BindGroup,
    /// Where the march writes instead when it runs at half resolution.
    half: Option<HalfTarget>,
}

/// The half-resolution march, and the upsample's bind group that reads it into the full-size target.
struct HalfTarget {
    view: wgpu::TextureView,
    upsample: wgpu::BindGroup,
}

pub(super) struct CloudPass {
    /// Made the first frame clouds are on, so a renderer that never shows them compiles neither shader.
    march: Option<March>,
    target: Option<CloudTarget>,
    /// What the tonemap binds until clouds first run: one transparent texel.
    placeholder: wgpu::TextureView,
    /// A shader reload's pipelines from before clouds first ran, which that first frame then builds on.
    reloaded: Option<CloudPipelines>,
}

/// The single-sample depth and the forward path's MSAA spread beside it, which the march reads.
#[derive(Clone, PartialEq)]
struct SceneDepths {
    depth: wgpu::TextureView,
    spread: wgpu::TextureView,
}

/// The pipelines, the march's inputs and the noise volume it samples.
struct March {
    pipelines: CloudPipelines,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    noise: NoiseVolume,
}

impl CloudPass {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            march: None,
            target: None,
            placeholder: create_2d_texture(
                device,
                Some("cloud_placeholder"),
                1,
                1,
                CLOUD_FORMAT,
                wgpu::TextureUsages::TEXTURE_BINDING,
                1,
                1,
            )
            .create_view(&wgpu::TextureViewDescriptor::default()),
            reloaded: None,
        }
    }

    /// The texture the tonemap composites: the last march, or one transparent texel before the first.
    pub(super) fn composite_view(&self) -> &wgpu::TextureView {
        self.target
            .as_ref()
            .map_or(&self.placeholder, |target| &target.view)
    }

    /// A reload's pipelines: the target rebinds and the volume refills from them on the next cloud frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn replace_pipelines(&mut self, device: &wgpu::Device, pipelines: CloudPipelines) {
        let Some(march) = self.march.as_mut() else {
            self.reloaded = Some(pipelines);
            return;
        };
        march.noise.bind_group =
            noise_bind_group(device, &pipelines.noise_layout, &march.noise.view);
        march.noise.pending = true;
        march.pipelines = pipelines;
        if let Some(target) = self.target.as_mut() {
            target.bind_group = march_bind_group(device, march, &target.depths);
            if let Some(half) = target.half.as_mut() {
                half.upsample = upsample_bind_group(device, march, &target.depths, &half.view);
            }
        }
    }
}

impl CloudPipelines {
    /// Every pipeline from `march_wgsl` and `noise_wgsl`; a shader reload builds them inside its error scope.
    pub(super) fn new(device: &wgpu::Device, march_wgsl: &str, noise_wgsl: &str) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cloud_march_shader"),
            source: wgpu::ShaderSource::Wgsl(march_wgsl.into()),
        });
        let (march, march_layout) = march_pipeline(device, &module);
        let (upsample, upsample_layout) = upsample_pipeline(device, &module);
        let (noise, noise_layout) = noise_pipeline(device, noise_wgsl);
        Self {
            march,
            march_layout,
            upsample,
            upsample_layout,
            noise,
            noise_layout,
        }
    }

    /// From the WGSL this crate was built with.
    fn shipped(device: &wgpu::Device) -> Self {
        Self::new(
            device,
            include_str!("../../shaders/cloud_march.wgsl"),
            include_str!("../../shaders/cloud_noise.wgsl"),
        )
    }
}

impl March {
    fn new(device: &wgpu::Device, pipelines: CloudPipelines) -> Self {
        let noise = NoiseVolume::new(device, &pipelines.noise_layout);
        Self {
            pipelines,
            uniform_buffer: buffer_desc::uniform(
                device,
                "cloud_frame_uniforms",
                std::mem::size_of::<CloudFrameUniforms>() as wgpu::BufferAddress,
            ),
            sampler: noise_sampler(device),
            noise,
        }
    }
}

/// `fs_main` over the uniforms, the noise volume and its sampler, and the scene depth.
fn march_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cloud_march_bind_group_layout"),
        entries: &[
            bind_layout::uniform(0, fragment),
            bind_layout::texture(
                1,
                fragment,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::D3,
                false,
            ),
            bind_layout::sampler(2, fragment, wgpu::SamplerBindingType::Filtering),
            // Unfilterable float: a depth texture binds so without a depth-texture patch of the WGSL.
            bind_layout::texture_2d(3, fragment, false),
            bind_layout::texture_2d(4, fragment, false),
        ],
    });
    let pipeline_layout =
        pipeline_desc::single_layout(device, "cloud_march_pipeline_layout", &layout);
    let pipeline = pipeline_desc::create_fullscreen_pipeline(
        device,
        FullscreenPipeline {
            label: "cloud_march_pipeline",
            layout: &pipeline_layout,
            module: shader,
            vs_entry: "vs_main",
            fs_entry: "fs_main",
            format: CLOUD_FORMAT,
            blend: None,
        },
    );
    (pipeline, layout)
}

/// `fs_upsample` over the uniforms, the scene depth and the half-resolution march.
fn upsample_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let fragment = wgpu::ShaderStages::FRAGMENT;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cloud_upsample_bind_group_layout"),
        entries: &[
            bind_layout::uniform(0, fragment),
            bind_layout::texture_2d(3, fragment, false),
            bind_layout::texture_2d(5, fragment, false),
        ],
    });
    let pipeline_layout =
        pipeline_desc::single_layout(device, "cloud_upsample_pipeline_layout", &layout);
    let pipeline = pipeline_desc::create_fullscreen_pipeline(
        device,
        FullscreenPipeline {
            label: "cloud_upsample_pipeline",
            layout: &pipeline_layout,
            module: shader,
            vs_entry: "vs_main",
            fs_entry: "fs_upsample",
            format: CLOUD_FORMAT,
            blend: None,
        },
    );
    (pipeline, layout)
}

/// Trilinear and wrapping, as the C++ engine samples its volume: the march wraps positions with `frac`.
fn noise_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("cloud_noise_sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

impl NoiseVolume {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let extent = NOISE_VOLUME_EXTENT;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            // TEXTURE_2D_SHAPE_OK: the 3D noise volume the march samples.
            label: Some("cloud_noise_volume"),
            size: wgpu::Extent3d {
                width: extent,
                height: extent,
                depth_or_array_layers: extent,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: NOISE_VOLUME_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = noise_bind_group(device, layout, &view);
        Self {
            texture,
            view,
            bind_group,
            pending: true,
        }
    }

    /// Records the dispatch that fills the volume, once after it is made and once after each reload.
    fn generate(&mut self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::ComputePipeline) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cloud_noise_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        let [x, y, z] = NOISE_WORKGROUP_SIZE.map(|size| NOISE_VOLUME_EXTENT / size);
        pass.dispatch_workgroups(x, y, z);
    }
}

/// `cs_noise` over a write-only storage view of the volume.
fn noise_pipeline(
    device: &wgpu::Device,
    wgsl: &str,
) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cloud_noise_shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cloud_noise_bind_group_layout"),
        entries: &[bind_layout::storage_texture(
            0,
            wgpu::ShaderStages::COMPUTE,
            wgpu::StorageTextureAccess::WriteOnly,
            NOISE_VOLUME_FORMAT,
            wgpu::TextureViewDimension::D3,
        )],
    });
    let pipeline_layout =
        pipeline_desc::single_layout(device, "cloud_noise_pipeline_layout", &layout);
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cloud_noise_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_noise"),
        compilation_options: Default::default(),
        cache: None,
    });
    (pipeline, layout)
}

fn noise_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud_noise_bind_group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

impl CloudTarget {
    /// At `size`, and with a half-resolution march beneath it when `half`.
    fn new(
        device: &wgpu::Device,
        march: &March,
        (size, half): ((u32, u32), bool),
        depths: &SceneDepths,
    ) -> Self {
        let texture = cloud_texture(device, "cloud_target", size);
        let half = half.then(|| {
            let half_size = (size.0.div_ceil(2), size.1.div_ceil(2));
            let view = cloud_texture(device, "cloud_half_target", half_size)
                .create_view(&wgpu::TextureViewDescriptor::default());
            HalfTarget {
                upsample: upsample_bind_group(device, march, depths, &view),
                view,
            }
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            size,
            depths: depths.clone(),
            bind_group: march_bind_group(device, march, depths),
            half,
        }
    }

    /// Where the march writes: the half-resolution texture when there is one.
    fn march_view(&self) -> &wgpu::TextureView {
        self.half.as_ref().map_or(&self.view, |half| &half.view)
    }
}

/// A target the march or the upsample renders into and the next pass samples or reads back.
fn cloud_texture(device: &wgpu::Device, label: &str, (width, height): (u32, u32)) -> wgpu::Texture {
    create_2d_texture(
        device,
        Some(label),
        width,
        height,
        CLOUD_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        1,
        1,
    )
}

/// The upsample's group 0: the march's uniforms, the scene depth and the half-resolution march.
fn upsample_bind_group(
    device: &wgpu::Device,
    march: &March,
    depths: &SceneDepths,
    half: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud_upsample_bind_group"),
        layout: &march.pipelines.upsample_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: march.uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&depths.depth),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(half),
            },
        ],
    })
}

/// The march's group 0: its uniforms, the noise volume and its sampler, the scene depth and its spread.
fn march_bind_group(device: &wgpu::Device, march: &March, depths: &SceneDepths) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud_march_bind_group"),
        layout: &march.pipelines.march_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: march.uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&march.noise.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&march.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&depths.depth),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&depths.spread),
            },
        ],
    })
}

impl ForwardRenderer {
    /// Before the tonemap binds its inputs: clouds that are on need the noise and a target over this frame's depth.
    pub(super) fn prepare_clouds(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if !self.clouds.enabled {
            return;
        }
        let pass = &mut self.cloud_pass;
        let march = pass.march.get_or_insert_with(|| {
            let shipped = || CloudPipelines::shipped(&gpu.device);
            March::new(&gpu.device, pass.reloaded.take().unwrap_or_else(shipped))
        });
        let depths = SceneDepths {
            depth: self.targets.depth.clone(),
            spread: self.targets.depth_spread.clone(),
        };
        let half = self.clouds.half_resolution;
        let current = pass.target.as_ref().is_some_and(|target| {
            target.size == (width, height)
                && target.depths == depths
                && target.half.is_some() == half
        });
        if current {
            return;
        }
        pass.target = Some(CloudTarget::new(
            &gpu.device,
            march,
            ((width, height), half),
            &depths,
        ));
        // Bloom's bright pass and every tonemap sample the new target.
        self.post.rebuild_needed = true;
    }

    /// The noise on the first cloud frame, then the march of every pixel to its scene depth; `traced` when no resolve ran.
    /// A half-resolution march marches a quarter of them, and the upsample fills the full-size target from it.
    pub(super) fn encode_clouds(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        (view, traced): (&FrameView, bool),
    ) {
        if !self.clouds.enabled {
            return;
        }
        // Only the forward path resolves MSAA, so only its full-resolution frames have a spread to march to.
        let spread =
            !traced && self.raster_path == RasterPath::Forward && !self.clouds.half_resolution;
        let uniforms = self.cloud_uniforms(view, spread);
        let pass = &mut self.cloud_pass;
        let (Some(march), Some(target)) = (pass.march.as_mut(), pass.target.as_ref()) else {
            return;
        };
        gpu.queue
            .write_buffer(&march.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        march.noise.generate(encoder, &march.pipelines.noise);
        let scope = self.gpu_timing.scope(TimedPass::Clouds);
        let passes = 1 + usize::from(target.half.is_some());
        fullscreen_pass(
            encoder,
            ("cloud_march_pass", target.march_view()),
            (&march.pipelines.march, &target.bind_group),
            scope.render_writes(0, passes),
        );
        if let Some(half) = target.half.as_ref() {
            fullscreen_pass(
                encoder,
                ("cloud_upsample_pass", &target.view),
                (&march.pipelines.upsample, &half.upsample),
                scope.render_writes(1, passes),
            );
        }
    }

    /// The camera, the sun and the cloud settings as `cloud_march.slang` reads them.
    fn cloud_uniforms(&self, view: &FrameView, spread: bool) -> CloudFrameUniforms {
        let to_sun = self
            .light_dir_ambient
            .truncate()
            .try_normalize()
            .unwrap_or(Vec3::Y);
        let colour = self.light_color_intensity;
        let ambient = self.cloud_ambient();
        CloudFrameUniforms {
            inv_view_proj: view.view_proj.inverse().to_cols_array_2d(),
            eye: view.eye.extend(1.0).to_array(),
            to_sun: to_sun.extend(0.0).to_array(),
            sun_radiance: (colour.truncate() * colour.w).extend(0.0).to_array(),
            params: self.clouds.params_at(self.cloud_time_seconds),
            depth_spread: [
                f32::from(u8::from(spread)),
                f32::from(u8::from(self.clouds.half_resolution)),
                0.0,
                0.0,
            ],
            ambient_top: ambient.top.extend(0.0).to_array(),
            ambient_bottom: ambient.bottom.extend(0.0).to_array(),
        }
    }

    /// The last march as raw half-float bits, RGBA and row-major; `None` before clouds first ran.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_cloud_target(&self, gpu: &GpuContext) -> anyhow::Result<Option<Vec<u16>>> {
        let Some(target) = self.cloud_pass.target.as_ref() else {
            return Ok(None);
        };
        let (width, height) = target.size;
        let region = half_float_region(width, height, 1);
        let bytes = read_texture(gpu, &target.texture, &region)?;
        Ok(Some(bytemuck::pod_collect_to_vec(&bytes)))
    }

    /// The noise volume as raw half-float bits, RGBA, x fastest, then y, then z; `None` before clouds first ran.
    /// Stalls on a readback of 16 MiB, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_noise_volume(&self, gpu: &GpuContext) -> anyhow::Result<Option<Vec<u16>>> {
        let Some(march) = self.cloud_pass.march.as_ref() else {
            return Ok(None);
        };
        let extent = NOISE_VOLUME_EXTENT;
        let bytes = read_texture(
            gpu,
            &march.noise.texture,
            &half_float_region(extent, extent, extent),
        )?;
        Ok(Some(bytemuck::pod_collect_to_vec(&bytes)))
    }
}

/// One full-screen triangle of `pipeline` into `target`, cleared to transparent first.
fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    (label, target): (&str, &wgpu::TextureView),
    (pipeline, bind_group): (&wgpu::RenderPipeline, &wgpu::BindGroup),
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
) {
    let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    render.set_pipeline(pipeline);
    render.set_bind_group(0, bind_group, &[]);
    render.draw(0..3, 0..1);
}

/// `layers` slices of RGBA half floats.
fn half_float_region(width: u32, height: u32, layers: u32) -> TextureRegion {
    TextureRegion {
        aspect: wgpu::TextureAspect::All,
        width,
        height,
        layers,
        texel_bytes: 8,
    }
}
