//! Fullscreen ACES tonemap pass: HDR (Rgba16Float) input -> display target.

use std::sync::Arc;

use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::fxaa::FxaaPass;
use crate::render::gpu_timing::PassScope;
use crate::render::pipeline_desc::{self, FullscreenPipeline};

/// The tonemap and FXAA WGSL a pass builds from; a reload in the renderer hands every pass new ones.
#[derive(Clone, Debug)]
pub struct TonemapShaders {
    pub tonemap: Arc<str>,
    pub fxaa: Arc<str>,
    /// 0 for the shipped pair, a fresh number for each accepted reload.
    pub id: u64,
}

impl TonemapShaders {
    /// The WGSL this crate was built with.
    pub fn shipped() -> Self {
        Self {
            tonemap: include_str!("../shaders/tonemap.wgsl").into(),
            fxaa: include_str!("../shaders/fxaa.wgsl").into(),
            id: 0,
        }
    }
}

pub struct TonemapPass {
    pipeline: wgpu::RenderPipeline,
    /// What `pipeline` and the FXAA pass are built from.
    shaders: TonemapShaders,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
    /// The `ForwardRenderer` input set `bind_group` samples, so a rebuild there rebinds this pass.
    pub(crate) bound_inputs: Option<u64>,
    /// Whether the shader must gamma-encode itself: WebGPU canvases have no sRGB surface format.
    encode_srgb: bool,
    output_format: wgpu::TextureFormat,
    /// Built the first frame FXAA filters, and kept.
    fxaa: Option<FxaaPass>,
}

impl TonemapPass {
    /// `output_format` is the format of the view later passed to [`Self::render`].
    pub fn new(gpu: &GpuContext, output_format: wgpu::TextureFormat) -> Self {
        Self::with_shaders(gpu, output_format, TonemapShaders::shipped())
    }

    /// As [`Self::new`], built from `shaders`.
    pub fn with_shaders(
        gpu: &GpuContext,
        output_format: wgpu::TextureFormat,
        shaders: TonemapShaders,
    ) -> Self {
        let device = &gpu.device;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap_bind_group_layout"),
            entries: &[
                bind_layout::texture_2d(0, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::sampler(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::SamplerBindingType::Filtering,
                ),
                bind_layout::texture_2d(2, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::uniform(3, wgpu::ShaderStages::FRAGMENT),
                bind_layout::texture_2d(4, wgpu::ShaderStages::FRAGMENT, true),
                // Exposure stays on the GPU, so no frame waits on a readback.
                bind_layout::storage_buffer(5, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::texture_2d(6, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });

        let pipeline =
            tonemap_pipeline(device, &bind_group_layout, output_format, &shaders.tonemap);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tonemap_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = buffer_desc::uniform(device, "tonemap_uniforms", 16);

        Self {
            pipeline,
            shaders,
            bind_group_layout,
            sampler,
            uniform_buffer,
            bind_group: None,
            bound_inputs: None,
            encode_srgb: !output_format.is_srgb(),
            output_format,
            fxaa: None,
        }
    }

    /// Rebuilds from `shaders` unless built from them already; the bind group and its inputs stay.
    pub fn use_shaders(&mut self, gpu: &GpuContext, shaders: &TonemapShaders) {
        if self.shaders.id == shaders.id {
            return;
        }
        let device = &gpu.device;
        let format = self.output_format;
        self.pipeline = tonemap_pipeline(device, &self.bind_group_layout, format, &shaders.tonemap);
        // Rebuilt from the new WGSL the next frame FXAA filters.
        self.fxaa = None;
        self.shaders = shaders.clone();
    }

    /// The id of the shaders this pass is built from, [`TonemapShaders::id`].
    pub fn shaders_id(&self) -> u64 {
        self.shaders.id
    }

    /// The format of the view [`Self::render`] draws into, which FXAA's intermediate shares.
    pub fn output_format(&self) -> wgpu::TextureFormat {
        self.output_format
    }

    /// Readies FXAA to filter `input`, the intermediate of a `width` x `height` output, building it on first use.
    pub fn prepare_fxaa(
        &mut self,
        gpu: &GpuContext,
        input: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let format = self.output_format;
        self.fxaa
            .get_or_insert_with(|| FxaaPass::new(gpu, format, &self.shaders.fxaa))
            .prepare(gpu, input, width, height);
    }

    /// Lets FXAA's intermediate go, on a frame that does not filter.
    pub fn release_fxaa(&mut self) {
        if let Some(fxaa) = self.fxaa.as_mut() {
            fxaa.release();
        }
    }

    /// Filters FXAA's intermediate into `output_view`; nothing before [`Self::prepare_fxaa`].
    pub fn render_fxaa(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        scope: PassScope<'_>,
    ) {
        match self.fxaa.as_ref() {
            Some(fxaa) => fxaa.render(encoder, output_view, scope),
            None => log::error!("FXAA ran before prepare_fxaa"),
        }
    }

    /// Per-frame parameters; `clouds` composites the cloud input over the scene.
    pub fn set_params(
        &self,
        queue: &wgpu::Queue,
        bloom_strength: f32,
        ssao_strength: f32,
        clouds: bool,
    ) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&[
                bloom_strength,
                ssao_strength,
                if clouds { 1.0 } else { 0.0 },
                // params.w: `encode_srgb`, fixed at creation but carried in the spare slot.
                if self.encode_srgb { 1.0 } else { 0.0 },
            ]),
        );
    }

    /// (Re)binds the HDR, bloom, AO, exposure and cloud inputs; call whenever they are recreated.
    pub fn set_input(
        &mut self,
        gpu: &GpuContext,
        hdr_view: &wgpu::TextureView,
        bloom_view: &wgpu::TextureView,
        ao_view: &wgpu::TextureView,
        exposure_buffer: &wgpu::Buffer,
        cloud_view: &wgpu::TextureView,
    ) {
        self.bound_inputs = None;
        self.bind_group = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: exposure_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(cloud_view),
                },
            ],
        }));
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        scope: PassScope<'_>,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            log::error!("TonemapPass::render called before set_input");
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap_pass"),
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
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// `fs_main` of `wgsl` over the tonemap's bind group into `format`.
fn tonemap_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    wgsl: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tonemap_shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let layout = pipeline_desc::single_layout(device, "tonemap_pipeline_layout", bind_group_layout);
    pipeline_desc::create_fullscreen_pipeline(
        device,
        FullscreenPipeline {
            label: "tonemap_pipeline",
            layout: &layout,
            module: &shader,
            vs_entry: "vs_main",
            fs_entry: "fs_main",
            format,
            blend: None,
        },
    )
}
