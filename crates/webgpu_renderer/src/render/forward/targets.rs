//! The size-dependent render targets, backed by the frame graph's transient pool, and the pass that resolves
//! MSAA depth to single-sample: the nearest sample as the depth, and the farthest with its share for the cloud march.

use super::frame_graph::{self, CloudShape, FrameConfig, FramePass, FramePlan};
use super::readback::{read_texture, TextureRegion};
use super::{ForwardRenderer, DEPTH_FORMAT};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::graph::{Resource, TransientPool};
use crate::render::pipeline_desc;

/// The farthest MSAA depth and the share of samples at it; float32, as depth near 1 needs its precision.
pub(super) const DEPTH_SPREAD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// The frame's targets: the textures the pool backs this frame's transients with, at the scene size.
pub(super) struct RenderTargets {
    pool: TransientPool,
    /// The scene targets' size, the output's at render scale.
    pub(super) size: (u32, u32),
    pub(super) output_size: (u32, u32),
}

impl RenderTargets {
    pub(super) fn new(width: u32, height: u32) -> Self {
        Self {
            pool: TransientPool::default(),
            size: (width.max(1), height.max(1)),
            output_size: (width.max(1), height.max(1)),
        }
    }

    /// `resource`'s target this frame; only a pass the frame's graph declares it for asks, so the pool backs it.
    pub(super) fn view(&self, resource: Resource) -> &wgpu::TextureView {
        self.pool
            .view(resource)
            .unwrap_or_else(|| panic!("this frame's graph allocates no {resource:?}"))
    }

    /// `resource`'s target this frame, if the frame's graph allocates one.
    pub(super) fn get(&self, resource: Resource) -> Option<&wgpu::TextureView> {
        self.pool.view(resource)
    }

    /// The last frame's `resource` texture, for the readbacks.
    pub(super) fn texture(&self, resource: Resource) -> anyhow::Result<&wgpu::Texture> {
        self.pool
            .texture(resource)
            .ok_or_else(|| anyhow::anyhow!("the last frame allocated no {resource:?}"))
    }
}

/// Pipeline and bind group for the MSAA depth → single-sample resolve.
pub(super) struct DepthResolve {
    pipeline: wgpu::RenderPipeline,
    /// The bind group over this frame's MSAA depth, compared by identity; `None` while no frame resolves.
    bound: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl DepthResolve {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = create_depth_resolve_layout(device);
        let shipped = include_str!("../../shaders/depth_resolve.wgsl");
        let pipeline = create_depth_resolve_pipeline(device, &bind_group_layout, shipped);
        Self {
            pipeline,
            bound: None,
            bind_group_layout,
        }
    }

    /// Binds `depth_msaa`, or drops the bind group, and with it the texture, on a frame without one.
    fn bind(&mut self, device: &wgpu::Device, depth_msaa: Option<&wgpu::TextureView>) {
        let Some(depth_msaa) = depth_msaa else {
            self.bound = None;
            return;
        };
        if self
            .bound
            .as_ref()
            .is_some_and(|(view, _)| view == depth_msaa)
        {
            return;
        }
        let bind_group =
            create_depth_resolve_bind_group(device, &self.bind_group_layout, depth_msaa);
        self.bound = Some((depth_msaa.clone(), bind_group));
    }
}

impl DepthResolve {
    /// The pipeline `wgsl` builds against this pass's layout; a shader reload builds it inside its error scope.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn compile(&self, device: &wgpu::Device, wgsl: &str) -> wgpu::RenderPipeline {
        create_depth_resolve_pipeline(device, &self.bind_group_layout, wgsl)
    }

    /// Swaps in a reload's pipeline; the bind group stays.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn replace_pipeline(&mut self, pipeline: wgpu::RenderPipeline) {
        self.pipeline = pipeline;
    }
}

impl ForwardRenderer {
    /// Compiles this frame's graph and backs its targets; a moved target rebinds every group that samples it.
    pub(super) fn plan_frame(
        &mut self,
        gpu: &GpuContext,
        output_format: wgpu::TextureFormat,
        (size, output_size): ((u32, u32), (u32, u32)),
    ) -> FramePlan {
        self.targets.size = size;
        self.targets.output_size = output_size;
        let config = self.frame_config(output_format);
        let plan = frame_graph::plan(&config)
            .unwrap_or_else(|error| panic!("frame graph of {config:?}: {error}"));
        if self.targets.pool.acquire(&gpu.device, &plan.graph) {
            self.rebind_targets(gpu, &plan);
        }
        plan
    }

    /// What decides this frame's passes, from the settings and the drawn mode.
    fn frame_config(&self, output_format: wgpu::TextureFormat) -> FrameConfig {
        let mode = self.drawn_mode();
        let traced = mode.is_traced();
        let culls = self.gpu_culling_enabled || self.occlusion_queries_enabled;
        FrameConfig {
            size: self.targets.size,
            output_size: self.targets.output_size,
            output_format,
            mode,
            path: self.raster_path,
            clouds: self.clouds.enabled.then(|| CloudShape {
                half: self.clouds.half_resolution,
                noise: self.cloud_pass.noise_pending(),
            }),
            bloom: self.bloom_strength > 0.0,
            ssao: self.ssao_strength > 0.0,
            histogram: self.auto_exposure,
            occlusion: !traced && culls && !self.primitives.is_empty(),
            fxaa: self.fxaa_filters(traced),
        }
    }

    /// After the pool moved a target: what samples one rebinds, and what holds one this frame lacks lets it go.
    fn rebind_targets(&mut self, gpu: &GpuContext, plan: &FramePlan) {
        let msaa = self.targets.get(Resource::DepthMsaa);
        self.depth_resolve.bind(&gpu.device, msaa);
        let hdr = self.targets.view(Resource::HdrColor);
        self.post.histogram.set_input(gpu, hdr);
        self.post.rebuild_needed = true;
        if !plan.runs(FramePass::GBuffer) {
            self.drop_gbuffer_bindings();
        }
        if !plan.runs(FramePass::Clouds) {
            self.cloud_pass.drop_target();
        }
    }

    /// Bytes of the textures the transient pool holds after the last frame.
    #[doc(hidden)]
    pub fn transient_texture_bytes(&self) -> u64 {
        self.targets.pool.bytes()
    }

    /// The last frame's resolved HDR colour as raw half-float bits, RGBA and row-major.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_hdr_target(&self, gpu: &GpuContext) -> anyhow::Result<Vec<u16>> {
        let (width, height) = self.targets.size;
        let bytes = read_texture(
            gpu,
            self.targets.texture(Resource::HdrColor)?,
            &TextureRegion {
                aspect: wgpu::TextureAspect::All,
                width,
                height,
                layers: 1,
                texel_bytes: 8,
            },
        )?;
        Ok(bytemuck::pod_collect_to_vec(&bytes))
    }

    /// The last frame's single-sample depth, row-major: what SSAO, the cullers and the clouds read.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_depth_target(&self, gpu: &GpuContext) -> anyhow::Result<Vec<f32>> {
        let (width, height) = self.targets.size;
        let bytes = read_texture(
            gpu,
            self.targets.texture(Resource::Depth)?,
            &TextureRegion {
                aspect: wgpu::TextureAspect::DepthOnly,
                width,
                height,
                layers: 1,
                texel_bytes: 4,
            },
        )?;
        Ok(bytemuck::pod_collect_to_vec(&bytes))
    }

    /// Depth resolve: MSAA depth to single-sample (min over samples) for SSAO and occlusion, and the spread.
    pub(super) fn encode_depth_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some((_, bind_group)) = self.depth_resolve.bound.as_ref() else {
            log::error!("the depth resolve ran on a frame without MSAA depth");
            return;
        };
        let mut resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth_resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.targets.view(Resource::DepthSpread),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.targets.view(Resource::Depth),
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
        resolve.set_bind_group(0, bind_group, &[]);
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

fn create_depth_resolve_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("depth_resolve_bind_group_layout"),
        entries: &[bind_layout::texture(
            0,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Depth,
            wgpu::TextureViewDimension::D2,
            true,
        )],
    })
}

fn create_depth_resolve_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    wgsl: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth_resolve_shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let pipeline_layout =
        pipeline_desc::single_layout(device, "depth_resolve_pipeline_layout", bind_group_layout);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            targets: &[Some(DEPTH_SPREAD_FORMAT.into())],
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
    })
}
