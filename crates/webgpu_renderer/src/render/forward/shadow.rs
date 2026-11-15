//! Cascaded shadow maps: the depth array, the caster pipelines and bundle, and one pass per cascade.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::primitive::GpuPrimitive;
use super::{ForwardRenderer, CASCADE_COUNT, DEPTH_FORMAT, SHADOW_MAP_SIZE};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::gpu_timing::TimedPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::{InstanceRaw, Vertex};

pub(super) struct ShadowResources {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) masked_pipeline: wgpu::RenderPipeline,
    // Read only by the native shader hot-reload path, so unread on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) masked_pipeline_layout: wgpu::PipelineLayout,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) masked_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    pub(super) view: wgpu::TextureView,
    layer_views: Vec<wgpu::TextureView>,
    cascade_index_bind_groups: Vec<wgpu::BindGroup>,
    pub(super) cascade_matrices: [Mat4; CASCADE_COUNT],
    pub(super) cascade_splits: [f32; 4],
    /// Shadow-caster draw list, recorded once and replayed per cascade; `None` re-records it.
    /// Only a new draw set or buffer identity invalidates it, not buffer writes or LOD switches.
    pub(super) caster_bundle: Option<wgpu::RenderBundle>,
    /// Caster count baked into the bundle, so the stats stay right on frames that reuse it.
    caster_count: u32,
    /// Caster draws submitted / considered last frame, summed over cascades.
    pub(super) casters_drawn: u32,
    pub(super) casters_considered: u32,
}

impl ShadowResources {
    /// The shadow entry points share the forward module, so `frame_layout` is their group 2 too.
    pub(super) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        frame_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_bind_group_layout"),
            entries: &[
                bind_layout::uniform(0, wgpu::ShaderStages::VERTEX),
                bind_layout::storage_buffer(13, wgpu::ShaderStages::VERTEX, true),
            ],
        });
        let masked_bind_group_layout = create_masked_bind_group_layout(device);
        let (cascade_index_layout, cascade_index_bind_groups) = create_cascade_index_groups(device);
        let pipeline_layout_for = |label: &str, group0: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[
                    Some(group0),
                    Some(&cascade_index_layout),
                    Some(frame_layout),
                ],
                immediate_size: 0,
            })
        };
        let pipeline_layout = pipeline_layout_for("shadow_pipeline_layout", &bind_group_layout);
        let masked_pipeline_layout =
            pipeline_layout_for("shadow_masked_pipeline_layout", &masked_bind_group_layout);
        let (view, layer_views) = create_shadow_map(device);
        Self {
            pipeline: create_shadow_pipeline(device, shader, &pipeline_layout),
            masked_pipeline: create_masked_shadow_pipeline(device, shader, &masked_pipeline_layout),
            pipeline_layout,
            masked_pipeline_layout,
            bind_group_layout,
            masked_bind_group_layout,
            sampler: create_shadow_sampler(device),
            view,
            layer_views,
            cascade_index_bind_groups,
            cascade_matrices: [Mat4::IDENTITY; CASCADE_COUNT],
            // z, w (tile counts) are written per frame by `write_frame_uniforms`.
            cascade_splits: [10.0, 30.0, 0.0, 0.0],
            caster_bundle: None,
            caster_count: 0,
            casters_drawn: 0,
            casters_considered: 0,
        }
    }

    /// One caster's draw; textured MASK casters go through the alpha-testing pipeline.
    fn record_caster<'a>(
        &'a self,
        bundle: &mut wgpu::RenderBundleEncoder<'a>,
        prim: &'a GpuPrimitive,
    ) {
        if let (true, Some(masked_group)) =
            (prim.alpha_masked, prim.shadow_masked_bind_group.as_ref())
        {
            bundle.set_pipeline(&self.masked_pipeline);
            bundle.set_bind_group(0, masked_group, &[]);
        } else {
            bundle.set_pipeline(&self.pipeline);
            bundle.set_bind_group(0, &prim.shadow_bind_group, &[]);
        }
        bundle.set_vertex_buffer(0, prim.vertex_buffer.slice(..));
        bundle.set_vertex_buffer(1, prim.instance_buffer.slice(..));
        bundle.set_index_buffer(prim.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        bundle.draw_indexed(0..prim.index_count, 0, 0..prim.instance_count);
    }
}

impl ForwardRenderer {
    /// One depth pass per cascade, each replaying the cached caster bundle.
    pub(super) fn encode_shadow_cascades(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.ensure_shadow_caster_bundle(gpu);
        let shadow = &self.shadow;
        let caster_bundle = shadow
            .caster_bundle
            .as_ref()
            .expect("just built above when absent");
        let shadow_scope = self.gpu_timing.scope(TimedPass::ShadowCascades);
        for cascade in 0..CASCADE_COUNT {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadow.layer_views[cascade],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: shadow_scope.render_writes(cascade, CASCADE_COUNT),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(1, &shadow.cascade_index_bind_groups[cascade], &[]);
            pass.set_bind_group(2, &self.frame.bind_group, &[]);
            pass.execute_bundles(std::iter::once(caster_bundle));
        }
        // No per-cascade culling, so drawn == considered by construction.
        self.shadow.casters_drawn = self.shadow.caster_count;
        self.shadow.casters_considered = self.shadow.caster_count;
    }

    /// A cached bundle replayed per cascade; the baked draw list rules out per-cascade culling.
    fn ensure_shadow_caster_bundle(&mut self, gpu: &GpuContext) {
        if self.shadow.caster_bundle.is_some() {
            return;
        }
        let mut bundle_encoder =
            gpu.device
                .create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("shadow_casters"),
                    color_formats: &[],
                    depth_stencil: Some(wgpu::RenderBundleDepthStencil {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_read_only: false,
                        stencil_read_only: true,
                    }),
                    sample_count: 1,
                    multiview: None,
                });
        // Groups 1/2 only satisfy layout validation; the parent pass sets the real ones.
        bundle_encoder.set_bind_group(1, &self.shadow.cascade_index_bind_groups[0], &[]);
        bundle_encoder.set_bind_group(2, &self.frame.bind_group, &[]);
        let mut caster_count = 0u32;
        for prim in self.primitives.iter().filter(|p| p.casts_shadow) {
            caster_count += 1;
            self.shadow.record_caster(&mut bundle_encoder, prim);
        }
        self.shadow.caster_bundle = Some(bundle_encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some("shadow_caster_bundle"),
        }));
        self.shadow.caster_count = caster_count;
    }

    /// Fits one light matrix per cascade; see `render::cascades` for why splits track the camera.
    pub(super) fn update_cascades(&mut self, camera: &OrbitCamera) {
        let (min, max) = self
            .scene_bounds
            .unwrap_or((Vec3::splat(-1.0), Vec3::splat(1.0)));
        let light_dir = self.light_dir_ambient.truncate().normalize_or_zero();

        let fit = crate::render::cascades::fit_cascades(camera, min, max, light_dir);
        // z, w (tile counts) are written per frame by `write_frame_uniforms`.
        self.shadow.cascade_splits = [fit.splits[0], fit.splits[1], 0.0, 0.0];
        self.shadow.cascade_matrices = fit.matrices;
    }
}

/// Slots 3/4 match the main pass so WGSL globals are shared; the alpha test reads binding 0.
fn create_masked_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow_masked_bind_group_layout"),
        entries: &[
            bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            bind_layout::texture_2d(3, wgpu::ShaderStages::FRAGMENT, true),
            bind_layout::sampler(
                4,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Filtering,
            ),
            bind_layout::storage_buffer(13, wgpu::ShaderStages::VERTEX, true),
        ],
    })
}

/// Per cascade: write_buffer lands before any pass, so a shared buffer keeps the last index.
fn create_cascade_index_groups(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, Vec<wgpu::BindGroup>) {
    let cascade_index_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow_cascade_index_layout"),
        entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX)],
    });
    let cascade_index_bind_groups: Vec<wgpu::BindGroup> = (0..CASCADE_COUNT as u32)
        .map(|cascade| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("shadow_cascade_index_{cascade}")),
                contents: bytemuck::bytes_of(&[cascade, 0u32, 0u32, 0u32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("shadow_cascade_index_bg_{cascade}")),
                layout: &cascade_index_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        })
        .collect();
    (cascade_index_layout, cascade_index_bind_groups)
}

/// The cascade depth array: its sampled array view and one attachment view per layer.
fn create_shadow_map(device: &wgpu::Device) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        // TEXTURE_2D_SHAPE_OK: depth array, not single-layer 2D
        label: Some("shadow_map_array"),
        size: wgpu::Extent3d {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            depth_or_array_layers: CASCADE_COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow_map_array_view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let shadow_layer_views: Vec<wgpu::TextureView> = (0..CASCADE_COUNT as u32)
        .map(|layer| {
            shadow_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow_map_layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (shadow_view, shadow_layer_views)
}

fn create_shadow_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    })
}

pub(super) fn create_masked_shadow_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow_masked_pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shadow_masked"),
            buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            compilation_options: Default::default(),
        },
        // No color targets: the fragment stage exists only for the alpha-test discard.
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_shadow_masked"),
            targets: &[],
            compilation_options: Default::default(),
        }),
        // Unculled: cut-out cards are single-sided quads the light may see from behind.
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn create_shadow_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow_pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shadow"),
            buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
