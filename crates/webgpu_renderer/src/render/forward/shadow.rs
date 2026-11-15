//! Cascaded shadow maps: the depth array, the caster pipelines, and one pass per active cascade.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::primitive::GpuPrimitive;
use super::readback::{read_texture, TextureRegion};
use super::shadow_casters::CasterBundles;
use super::{ForwardRenderer, CASCADE_COUNT, DEPTH_FORMAT, SHADOW_MAP_SIZE};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::cascades::{fit_cascades, CascadeLens};
use crate::render::gpu_timing::TimedPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::{InstanceRaw, Vertex};

/// The caster pipelines: back-culled, unculled for double-sided materials, alpha-tested for MASK.
pub(super) struct ShadowPipelines {
    opaque: wgpu::RenderPipeline,
    /// `opaque` with clockwise front faces, for mirrored transforms; built when first needed.
    opaque_mirrored: Option<wgpu::RenderPipeline>,
    double_sided: wgpu::RenderPipeline,
    masked: wgpu::RenderPipeline,
}

impl ShadowPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        masked_layout: &wgpu::PipelineLayout,
    ) -> Self {
        Self {
            opaque: create_shadow_pipeline(
                device,
                shader,
                layout,
                "shadow_pipeline",
                back_culled(wgpu::FrontFace::Ccw),
            ),
            opaque_mirrored: None,
            // Unculled: a double-sided sheet must shadow whichever side faces the light.
            double_sided: create_shadow_pipeline(
                device,
                shader,
                layout,
                "shadow_pipeline_double_sided",
                wgpu::PrimitiveState::default(),
            ),
            masked: create_masked_shadow_pipeline(device, shader, masked_layout),
        }
    }

    /// Builds the clockwise-front opaque pipeline once a mirrored caster exists.
    pub(super) fn ensure_mirrored(
        &mut self,
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
    ) {
        if self.opaque_mirrored.is_none() {
            self.opaque_mirrored = Some(create_shadow_pipeline(
                device,
                shader,
                layout,
                "shadow_pipeline_mirrored",
                back_culled(wgpu::FrontFace::Cw),
            ));
        }
    }

    /// The opaque caster pipeline whose front faces wind clockwise when `clockwise`.
    fn opaque_for(&self, clockwise: bool) -> &wgpu::RenderPipeline {
        if !clockwise {
            return &self.opaque;
        }
        debug_assert!(
            self.opaque_mirrored.is_some(),
            "ensure_mirrored must run first"
        );
        self.opaque_mirrored.as_ref().unwrap_or(&self.opaque)
    }
}

fn back_culled(front_face: wgpu::FrontFace) -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        front_face,
        cull_mode: Some(wgpu::Face::Back),
        ..Default::default()
    }
}

pub(super) struct ShadowResources {
    pub(super) pipelines: ShadowPipelines,
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    // Read only by the native shader hot-reload path, so unread on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) masked_pipeline_layout: wgpu::PipelineLayout,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) masked_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
    /// The cascade depth array, one layer per cascade the settings ask for; read back only by tests.
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) layer_views: Vec<wgpu::TextureView>,
    /// Texels per side of `texture`.
    resolution: u32,
    pub(super) cascade_index_bind_groups: Vec<wgpu::BindGroup>,
    pub(super) cascade_matrices: [Mat4; CASCADE_COUNT],
    pub(super) cascade_splits: [f32; 4],
    pub(super) bias_scales: [f32; CASCADE_COUNT],
    /// The camera's forward axis the splits are measured on.
    pub(super) view_dir: Vec3,
    /// One caster bundle per cascade over the casters inside it, replayed while that set holds; `None` re-records all.
    pub(super) caster_bundles: Option<CasterBundles>,
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
        let (texture, view, layer_views) =
            create_shadow_map(device, SHADOW_MAP_SIZE, CASCADE_COUNT as u32);
        Self {
            pipelines: ShadowPipelines::new(
                device,
                shader,
                &pipeline_layout,
                &masked_pipeline_layout,
            ),
            pipeline_layout,
            masked_pipeline_layout,
            bind_group_layout,
            masked_bind_group_layout,
            sampler: create_shadow_sampler(device),
            texture,
            view,
            layer_views,
            resolution: SHADOW_MAP_SIZE,
            cascade_index_bind_groups,
            cascade_matrices: [Mat4::IDENTITY; CASCADE_COUNT],
            // z, w (tile counts) are written per frame by `write_frame_uniforms`.
            cascade_splits: [10.0, 30.0, 0.0, 0.0],
            bias_scales: [1.0; CASCADE_COUNT],
            view_dir: Vec3::NEG_Z,
            caster_bundles: None,
            casters_drawn: 0,
            casters_considered: 0,
        }
    }

    /// One caster's draws: MASK casters alpha-test, double-sided ones are not culled.
    pub(super) fn record_caster<'a>(
        &'a self,
        bundle: &mut wgpu::RenderBundleEncoder<'a>,
        prim: &'a GpuPrimitive,
    ) {
        let pipelines = &self.pipelines;
        let masked_group = prim
            .shadow_masked_bind_group
            .as_ref()
            .filter(|_| prim.alpha_masked);
        bundle.set_bind_group(0, masked_group.unwrap_or(&prim.shadow_bind_group), &[]);
        bundle.set_vertex_buffer(0, prim.vertex_buffer.slice(..));
        bundle.set_vertex_buffer(1, prim.instance_buffer.slice(..));
        bundle.set_index_buffer(prim.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (instances, clockwise) in prim.instance_runs() {
            bundle.set_pipeline(match (masked_group.is_some(), prim.double_sided) {
                (true, _) => &pipelines.masked,
                (false, true) => &pipelines.double_sided,
                (false, false) => pipelines.opaque_for(clockwise),
            });
            bundle.draw_indexed(0..prim.index_count, 0, instances);
        }
    }
}

impl ForwardRenderer {
    /// One depth pass per active cascade, each replaying that cascade's caster bundle; none with shadows off.
    pub(super) fn encode_shadow_cascades(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let count = self
            .shadows
            .active_cascades()
            .min(self.shadow.layer_views.len());
        if count == 0 {
            self.shadow.casters_drawn = 0;
            self.shadow.casters_considered = 0;
            return;
        }
        self.ensure_shadow_caster_bundles(&gpu.device, count);
        let shadow = &self.shadow;
        let caster_bundles = shadow
            .caster_bundles
            .as_ref()
            .expect("just built above when absent");
        let shadow_scope = self.gpu_timing.scope(TimedPass::ShadowCascades);
        for cascade in 0..count {
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
                timestamp_writes: shadow_scope.render_writes(cascade, count),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(bundle) = caster_bundles.bundle(cascade) {
                pass.execute_bundles(std::iter::once(bundle));
            }
        }
    }

    /// Reallocates the cascade array when the resolution or the cascade count changed, then rebinds every group over it.
    pub(super) fn sync_shadow_map(&mut self, device: &wgpu::Device) {
        let settings = self.shadows.sanitized();
        let layers = settings.cascade_count as usize;
        if self.shadow.resolution == settings.resolution && self.shadow.layer_views.len() == layers
        {
            return;
        }
        let (texture, view, layer_views) =
            create_shadow_map(device, settings.resolution, layers as u32);
        self.shadow.texture = texture;
        self.shadow.view = view;
        self.shadow.layer_views = layer_views;
        self.shadow.resolution = settings.resolution;
        self.rebuild_material_bind_groups(device);
        self.rebind_deferred_shadow_map(device);
    }

    /// Texels per side of each cascade's map as allocated now.
    pub fn shadow_map_resolution(&self) -> u32 {
        self.shadow.resolution
    }

    /// Cascade layers allocated now; the frame renders them all unless shadows are off.
    pub fn shadow_map_layers(&self) -> usize {
        self.shadow.layer_views.len()
    }

    /// Every cascade's depth layer, layer-major then row-major, as the last frame left it.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_shadow_map(&self, gpu: &GpuContext) -> anyhow::Result<Vec<f32>> {
        let bytes = read_texture(
            gpu,
            &self.shadow.texture,
            &TextureRegion {
                aspect: wgpu::TextureAspect::DepthOnly,
                width: self.shadow.resolution,
                height: self.shadow.resolution,
                layers: self.shadow.layer_views.len() as u32,
                texel_bytes: 4,
            },
        )?;
        Ok(bytemuck::pod_collect_to_vec(&bytes))
    }

    /// The light view-projection each cascade's shadow pass used last frame.
    #[doc(hidden)]
    pub fn shadow_cascade_matrices(&self) -> [Mat4; CASCADE_COUNT] {
        self.shadow.cascade_matrices
    }

    /// Fits one light matrix per slice of the camera frustum; see `render::cascades`.
    pub(super) fn update_cascades(&mut self, camera: &OrbitCamera, frame_aspect: f32) {
        let light_dir = self.light_dir_ambient.truncate();
        let lens = CascadeLens::of(camera, frame_aspect);
        let fit = fit_cascades(&lens, light_dir, &self.shadows, self.scene_bounds);
        // z, w (tile counts) are written per frame by `write_frame_uniforms`.
        self.shadow.cascade_splits = [fit.splits[0], fit.splits[1], 0.0, 0.0];
        self.shadow.cascade_matrices = fit.matrices;
        self.shadow.bias_scales = fit.bias_scales;
        self.shadow.view_dir = fit.view_dir;
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

/// The cascade depth array, its sampled array view and one attachment view per layer.
fn create_shadow_map(
    device: &wgpu::Device,
    resolution: u32,
    layers: u32,
) -> (wgpu::Texture, wgpu::TextureView, Vec<wgpu::TextureView>) {
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        // TEXTURE_2D_SHAPE_OK: depth array, not single-layer 2D
        label: Some("shadow_map_array"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        // COPY_SRC only for `read_shadow_map`.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow_map_array_view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let shadow_layer_views: Vec<wgpu::TextureView> = (0..layers)
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
    (shadow_texture, shadow_view, shadow_layer_views)
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

fn create_masked_shadow_pipeline(
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

fn create_shadow_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    label: &str,
    primitive: wgpu::PrimitiveState,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shadow"),
            buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive,
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
