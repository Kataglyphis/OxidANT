//! Forward PBR pass into an HDR target, ACES-tonemapped to the output, plus headless readback.

use anyhow::Context as _;
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt as _;

use crate::context::GpuContext;
use crate::render::animation::{
    keyframe_lerp_indices, sample_morph_weights, sample_quat, sample_vec3,
};
use crate::render::bind_layout;
use crate::render::bloom::BloomPass;
use crate::render::buffer_desc;
use crate::render::gpu_occlusion::GpuCulling;
use crate::render::gpu_timing::{GpuTiming, TimedPass};
use crate::render::ibl::{BrdfLut, EquirectImage, IblEnvironment, IblFallback};
use crate::render::lights::pack_punctual_lights;
pub use crate::render::lights::MAX_PUNCTUAL_LIGHTS;
use crate::render::occlusion::OcclusionQueries;
use crate::render::pipeline_desc;
use crate::render::ssao::SsaoPass;
use crate::render::tile_grid::{
    build_tile_light_grid, TileLightGridScratch, MAX_LIGHTS_PER_TILE, TILE_SIZE,
};
use crate::render::tonemap::TonemapPass;
use crate::scene::camera::OrbitCamera;
use crate::scene::{
    AlphaMode, ChannelValues, CpuAnimation, CpuNode, CpuSampler, CpuScene, CpuSkin, CpuTexture,
    InstanceRaw, MorphTarget, Vertex,
};

/// Upper bound on joints per skin (storage buffer is sized to the skin).
pub const MAX_JOINTS: usize = 256;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// MSAA sample count of the forward HDR and depth targets (1 disables MSAA).
const MSAA_SAMPLE_COUNT: u32 = 4;
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// Cascaded shadow map layers (split by view distance).
pub const CASCADE_COUNT: usize = 3;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyUniforms {
    inv_view_proj: [[f32; 4]; 4],
    light_dir_intensity: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    light_space: [[f32; 4]; 4],
    light_space_1: [[f32; 4]; 4],
    light_space_2: [[f32; 4]; 4],
    light_dir_ambient: [f32; 4],
    light_color_intensity: [f32; 4],
    camera_position: [f32; 4],
    cascade_splits: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PrimUniforms {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    base_color: [f32; 4],
    material_factors: [f32; 4],
    emissive_factor: [f32; 4],
    base_uv_row0: [f32; 4],
    base_uv_row1: [f32; 4],
    mr_uv_row0: [f32; 4],
    mr_uv_row1: [f32; 4],
    normal_uv_row0: [f32; 4],
    normal_uv_row1: [f32; 4],
    emissive_uv_row0: [f32; 4],
    emissive_uv_row1: [f32; 4],
    occlusion_uv_row0: [f32; 4],
    occlusion_uv_row1: [f32; 4],
    /// x: 1.0 when KHR_materials_unlit, else 0.0. y: per-slot UV1 mask bits.
    material_flags: [f32; 4],
}

/// One LOD level, uploaded at `upload_scene` so frame cost never depends on camera motion.
struct LodLevel {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

struct GpuPrimitive {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Uniforms-only group: the full group samples the shadow map the shadow pass writes.
    shadow_bind_group: wgpu::BindGroup,
    /// MASK material with a base-color texture: shadows through the alpha-testing pipeline.
    alpha_masked: bool,
    shadow_masked_bind_group: Option<wgpu::BindGroup>,
    /// Per-instance transforms; always at least one, so every draw binds slot 1.
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
    /// Last-set instance transforms (empty = identity); bounds recompose when either side moves.
    instance_transforms: Vec<Mat4>,
    /// World bounds before instancing, so recomposition never re-derives the skinned/morphed pose.
    pre_instance_aabb: (Vec3, Vec3),
    model: Mat4,
    /// Cached `normal_matrix_of(model)`, recomputed only where `model` changes.
    normal_matrix: Mat4,
    base_color: [f32; 4],
    material_factors: [f32; 4],
    emissive_factor: [f32; 4],
    base_uv_transform: [[f32; 3]; 2],
    mr_uv_transform: [[f32; 3]; 2],
    normal_uv_transform: [[f32; 3]; 2],
    emissive_uv_transform: [[f32; 3]; 2],
    occlusion_uv_transform: [[f32; 3]; 2],
    /// Per-slot UV1 selector bits, packed into `material_flags.y`.
    uv_set_mask: u32,
    double_sided: bool,
    /// KHR_materials_unlit: skip lighting entirely and emit the base color.
    unlit: bool,
    alpha_blend: bool,
    casts_shadow: bool,
    world_center: Vec3,
    aabb_min: Vec3,
    aabb_max: Vec3,
    node_index: Option<usize>,
    skin_index: Option<usize>,
    joint_buffer: wgpu::Buffer,
    local_aabb_min: Vec3,
    local_aabb_max: Vec3,
    /// Simplified levels, coarsest last; empty when LOD is off, so that draw path is unchanged.
    lod_levels: Vec<LodLevel>,
    /// Switch distance per `lod_levels` entry, kept contiguous so selection never allocates.
    lod_min_distances: Vec<f32>,
    /// Neutral pose, kept only for morphed primitives so each frame re-blends, not accumulates.
    base_vertices: Vec<Vertex>,
    /// POSITION/NORMAL deltas, one entry per morph target.
    morph_targets: Vec<MorphTarget>,
    /// Current per-target weights (animation-driven or the mesh defaults).
    morph_weights: Vec<f32>,
    /// Vertex buffer needs re-blending; starts true so non-zero default weights apply once.
    morph_dirty: bool,
    /// `PrimUniforms` need re-upload; instance changes don't count, they have their own buffer.
    uniforms_dirty: bool,
}

impl GpuPrimitive {
    /// Buffers and index count to draw from `eye`, by `world_center` distance like the blend sort.
    fn geometry_for(&self, eye: Vec3) -> (&wgpu::Buffer, &wgpu::Buffer, u32) {
        let distance = self.world_center.distance(eye);
        match crate::scene::lod::select_lod_by_distance(&self.lod_min_distances, distance) {
            Some(level) => {
                let lod = &self.lod_levels[level];
                (&lod.vertex_buffer, &lod.index_buffer, lod.index_count)
            }
            None => (&self.vertex_buffer, &self.index_buffer, self.index_count),
        }
    }
}

pub struct ForwardRenderer {
    pipeline: wgpu::RenderPipeline,
    pipeline_double_sided: wgpu::RenderPipeline,
    pipeline_blend: wgpu::RenderPipeline,
    pipeline_blend_double_sided: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    // Read only by the native shader hot-reload path, so unread on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pipeline_layout: wgpu::PipelineLayout,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shadow_pipeline_layout: wgpu::PipelineLayout,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    sky_pipeline_layout: wgpu::PipelineLayout,
    sky_uniform_buffer: wgpu::Buffer,
    sky_bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    shadow_bind_group_layout: wgpu::BindGroupLayout,
    shadow_masked_bind_group_layout: wgpu::BindGroupLayout,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shadow_masked_pipeline_layout: wgpu::PipelineLayout,
    shadow_masked_pipeline: wgpu::RenderPipeline,
    /// Group 1 of the forward pipeline: the IBL maps, bound once per pass.
    ibl_bind_group_layout: wgpu::BindGroupLayout,
    ibl_uniform_buffer: wgpu::Buffer,
    ibl_bind_group: wgpu::BindGroup,
    /// Group 2 (per-frame data) of the forward and shadow layouts; only read to build them.
    #[allow(dead_code)]
    frame_bind_group_layout: wgpu::BindGroupLayout,
    frame_uniform_buffer: wgpu::Buffer,
    /// Punctual lights (up to `MAX_PUNCTUAL_LIGHTS`), read by the forward shader at group 3.
    light_storage_buffer: wgpu::Buffer,
    /// Like [`Self::frame_bind_group_layout`]: only read to build the pipeline layouts.
    #[allow(dead_code)]
    light_bind_group_layout: wgpu::BindGroupLayout,
    light_bind_group: wgpu::BindGroup,
    /// Tile-based light grid: offsets and counts for each screen tile.
    tile_light_grid_buffer: wgpu::Buffer,
    tile_light_indices_buffer: wgpu::Buffer,
    tile_bind_group_layout: wgpu::BindGroupLayout,
    tile_bind_group: wgpu::BindGroup,
    tile_counts: (u32, u32),
    /// Reusable scratch for [`build_tile_light_grid`] — see its doc comment.
    tile_light_grid_scratch: TileLightGridScratch,
    frame_bind_group: wgpu::BindGroup,
    /// 1x1 stand-ins so group 1 is always bindable, environment or not.
    ibl_fallback: IblFallback,
    /// `None` until [`Self::set_environment`]; the analytic fallback runs until then.
    ibl_environment: Option<IblEnvironment>,
    /// Environment-independent, baked lazily once so a renderer without an environment never pays.
    brdf_lut: Option<BrdfLut>,
    shadow_sampler: wgpu::Sampler,
    white_texture_view: wgpu::TextureView,
    flat_normal_view: wgpu::TextureView,
    shadow_view: wgpu::TextureView,
    shadow_layer_views: Vec<wgpu::TextureView>,
    cascade_index_bind_groups: Vec<wgpu::BindGroup>,
    cascade_matrices: [Mat4; CASCADE_COUNT],
    cascade_splits: [f32; 4],
    /// Opaque primitives drawn / considered by the camera pass last frame, after all culling.
    occlusion_drawn: u32,
    occlusion_considered: u32,
    /// Caster draws submitted / considered last frame, summed over cascades.
    shadow_casters_drawn: u32,
    shadow_casters_considered: u32,
    /// Shadow-caster draw list, recorded once and replayed per cascade; `None` re-records it.
    /// Only a new draw set or buffer identity invalidates it, not buffer writes or LOD switches.
    shadow_caster_bundle: Option<wgpu::RenderBundle>,
    /// Caster count baked into the bundle, so the stats stay right on frames that reuse it.
    shadow_caster_count: u32,
    primitives: Vec<GpuPrimitive>,
    scene_bounds: Option<(Vec3, Vec3)>,
    /// GPU textures created by the last `upload_scene`, so tests can prove the dedup.
    uploaded_texture_count: usize,
    depth: wgpu::TextureView,
    hdr_view: wgpu::TextureView,
    /// MSAA depth of the forward pass, resolved into `depth` by the depth-resolve pass.
    depth_msaa: wgpu::TextureView,
    /// MSAA HDR color of the forward pass, auto-resolved into `hdr_view`.
    hdr_msaa: wgpu::TextureView,
    /// Pipeline and bind group for the MSAA depth → single-sample resolve.
    depth_resolve_pipeline: wgpu::RenderPipeline,
    depth_resolve_bind_group: wgpu::BindGroup,
    depth_resolve_bind_group_layout: wgpu::BindGroupLayout,
    target_size: (u32, u32),
    hdr_rebound_needed: bool,
    bloom: BloomPass,
    ssao: SsaoPass,
    /// Bloom contribution mixed in by the tonemap pass.
    pub bloom_strength: f32,
    /// SSAO strength applied by the tonemap pass (0 = off).
    pub ssao_strength: f32,
    /// Exposure in EV stops (0 = neutral).
    pub exposure_ev: f32,
    /// Drive exposure from the scene histogram instead of `exposure_ev`.
    pub auto_exposure: bool,
    /// Adaptation rate; higher settles faster.
    pub auto_exposure_speed: f32,
    /// Seconds since the previous frame, for exposure adaptation (defaults to 60 Hz).
    pub frame_delta_seconds: f32,
    histogram: crate::render::histogram::HistogramPass,
    punctual_lights: [[f32; 4]; MAX_PUNCTUAL_LIGHTS * 4],
    punctual_light_count: u32,
    nodes: Vec<CpuNode>,
    animations: Vec<CpuAnimation>,
    skins: Vec<CpuSkin>,
    pending_joint_world: Option<Vec<Mat4>>,
    /// Direction towards the light (world space) + ambient strength.
    pub light_dir_ambient: Vec4,
    /// Light color (rgb) + intensity (w); HDR, so values above 1 are expected.
    pub light_color_intensity: Vec4,
    /// Build and draw per-primitive LOD chains; off by default.
    /// Read only by `upload_scene`, so set it (and `lod_switch_distances`) before uploading.
    pub lod_enabled: bool,
    /// Multiplier on the environment's radiance; the ambient slider is `light_dir_ambient.w`.
    pub ibl_intensity: f32,
    /// Per-pass GPU timestamps. Inert until [`Self::enable_gpu_timing`].
    gpu_timing: GpuTiming,
    /// Hardware occlusion queries over the forward depth; records only when enabled.
    occlusion: OcclusionQueries,
    /// Compute-shader occlusion culling, created on first use when enabled.
    gpu_culling: Option<GpuCulling>,
    /// Record occlusion detection after the forward pass; off by default since it adds a pass.
    /// Results land in [`Self::occlusion_visibility`].
    pub occlusion_queries_enabled: bool,
    /// Use compute-shader culling ([`GpuCulling`]) instead of hardware occlusion queries.
    pub gpu_culling_enabled: bool,
    /// Camera distances at which successive LOD levels take over, ascending.
    /// Defaults suit unit-scale assets; a scene in metres wants larger values.
    pub lod_switch_distances: Vec<f32>,
}

impl ForwardRenderer {
    pub fn new(gpu: &GpuContext, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/forward.wgsl").into()),
        });

        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = vec![
            bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            bind_layout::texture(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Depth,
                wgpu::TextureViewDimension::D2Array,
                false,
            ),
            bind_layout::sampler(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Comparison,
            ),
        ];
        // Bindings 3..=12: base color, metallic-roughness, normal, emissive, occlusion pairs.
        for slot in 0..5u32 {
            entries.push(bind_layout::texture_2d(
                3 + slot * 2,
                wgpu::ShaderStages::FRAGMENT,
                true,
            ));
            entries.push(bind_layout::sampler(
                4 + slot * 2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::SamplerBindingType::Filtering,
            ));
        }

        entries.push(bind_layout::storage_buffer(
            13,
            wgpu::ShaderStages::VERTEX,
            true,
        ));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("forward_bind_group_layout"),
            entries: &entries,
        });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow_bind_group_layout"),
                entries: &[
                    bind_layout::uniform(0, wgpu::ShaderStages::VERTEX),
                    bind_layout::storage_buffer(13, wgpu::ShaderStages::VERTEX, true),
                ],
            });

        let ibl_bind_group_layout = create_ibl_bind_group_layout(device);

        let frame_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("frame_bind_group_layout"),
                entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let frame_uniform_buffer = buffer_desc::uniform(
            device,
            "frame_uniforms",
            std::mem::size_of::<FrameUniforms>() as wgpu::BufferAddress,
        );
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame_bind_group"),
            layout: &frame_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_uniform_buffer.as_entire_binding(),
            }],
        });

        // Group 3: punctual lights, forward pipelines only.
        let light_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_bind_group_layout"),
                entries: &[bind_layout::storage_buffer(
                    0,
                    wgpu::ShaderStages::FRAGMENT,
                    true,
                )],
            });
        let light_storage_buffer = buffer_desc::storage_dst(
            device,
            "light_storage",
            (MAX_PUNCTUAL_LIGHTS as u64) * 16 * 4, // 4 vec4 per light
        );
        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_bind_group"),
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_storage_buffer.as_entire_binding(),
            }],
        });

        // Group 4: per-tile offset + count into the light index list, forward pipelines only.
        let tile_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("tile_light_bgl"),
                entries: &[
                    bind_layout::storage_buffer(0, wgpu::ShaderStages::FRAGMENT, true),
                    bind_layout::storage_buffer(1, wgpu::ShaderStages::FRAGMENT, true),
                ],
            });
        // Initial sizes: will be rebuilt on first render if larger needed.
        let max_tiles = 192 * 128; // enough for 3072×2048 ÷ 16²
        let tile_grid_buf = buffer_desc::storage_dst(
            device,
            "tile_light_grid",
            (max_tiles as u64) * 8, // vec2<u32> per tile
        );
        let tile_idx_buf = buffer_desc::storage_dst(
            device,
            "tile_light_indices",
            (max_tiles as u64) * (MAX_LIGHTS_PER_TILE as u64) * 4,
        );
        let tile_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile_light_bg"),
            layout: &tile_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: tile_grid_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tile_idx_buf.as_entire_binding(),
                },
            ],
        });

        // The shadow entry points share this module, so their layouts need group 2 as well.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("forward_pipeline_layout"),
            bind_group_layouts: &[
                Some(&bind_group_layout),
                Some(&ibl_bind_group_layout),
                Some(&frame_bind_group_layout),
                Some(&light_bind_group_layout),
                Some(&tile_bind_group_layout),
            ],
            immediate_size: 0,
        });

        // Per cascade: write_buffer lands before any pass, so a shared buffer keeps the last index.
        let cascade_index_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow_pipeline_layout"),
                bind_group_layouts: &[
                    Some(&shadow_bind_group_layout),
                    Some(&cascade_index_layout),
                    Some(&frame_bind_group_layout),
                ],
                immediate_size: 0,
            });

        // Slots 3/4 match the main pass so WGSL globals are shared; the alpha test reads binding 0.
        let shadow_masked_bind_group_layout =
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
            });
        let shadow_masked_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow_masked_pipeline_layout"),
                bind_group_layouts: &[
                    Some(&shadow_masked_bind_group_layout),
                    Some(&cascade_index_layout),
                    Some(&frame_bind_group_layout),
                ],
                immediate_size: 0,
            });

        let (pipeline, pipeline_double_sided, pipeline_blend, pipeline_blend_double_sided) =
            create_forward_pipeline_set(device, &shader, &pipeline_layout);

        // Sky: fullscreen triangle at far depth, drawn only where depth is still the cleared 1.0.
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sky.wgsl").into()),
        });
        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_bind_group_layout"),
                entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let sky_pipeline_layout =
            pipeline_desc::single_layout(device, "sky_pipeline_layout", &sky_bind_group_layout);
        let sky_pipeline = create_sky_pipeline(device, &sky_shader, &sky_pipeline_layout);
        let sky_uniform_buffer = buffer_desc::uniform(
            device,
            "sky_uniforms",
            std::mem::size_of::<SkyUniforms>() as wgpu::BufferAddress,
        );
        let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_bind_group"),
            layout: &sky_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_uniform_buffer.as_entire_binding(),
            }],
        });

        let shadow_pipeline = create_shadow_pipeline(device, &shader, &shadow_pipeline_layout);
        let shadow_masked_pipeline =
            create_masked_shadow_pipeline(device, &shader, &shadow_masked_pipeline_layout);

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let white_texture_view = create_material_texture(
            gpu,
            &CpuTexture {
                width: 1,
                height: 1,
                rgba8: vec![255, 255, 255, 255],
                compressed: None,
            },
            false,
            Some("white_fallback"),
        );
        // Flat tangent-space normal (0, 0, 1) encoded as RGBA8.
        let flat_normal_view = create_material_texture(
            gpu,
            &CpuTexture {
                width: 1,
                height: 1,
                rgba8: vec![128, 128, 255, 255],
                compressed: None,
            },
            false,
            Some("flat_normal_fallback"),
        );

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

        let depth = create_depth_texture(device, width, height, 1);
        let hdr_view = create_hdr_texture(device, width, height, 1);
        let depth_msaa = create_depth_texture(device, width, height, MSAA_SAMPLE_COUNT);
        let hdr_msaa = create_hdr_texture(device, width, height, MSAA_SAMPLE_COUNT);
        let (depth_resolve_pipeline, depth_resolve_bgl) = create_depth_resolve_pipeline(device);
        let depth_resolve_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth_resolve_bind_group"),
            layout: &depth_resolve_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth_msaa),
            }],
        });
        let mut histogram = crate::render::histogram::HistogramPass::new(gpu);
        histogram.set_input(gpu, &hdr_view);

        let ibl_fallback = IblFallback::new(device);
        let ibl_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibl_params"),
            contents: bytemuck::bytes_of(&IblUniforms::disabled()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ibl_bind_group = create_ibl_bind_group(
            device,
            &ibl_bind_group_layout,
            &ibl_uniform_buffer,
            &ibl_fallback.irradiance,
            &ibl_fallback.prefiltered,
            &ibl_fallback.brdf_lut,
            &ibl_fallback.sampler,
        );

        Self {
            pipeline,
            pipeline_double_sided,
            pipeline_blend,
            pipeline_blend_double_sided,
            shadow_pipeline,
            sky_pipeline,
            pipeline_layout,
            shadow_pipeline_layout,
            shadow_masked_bind_group_layout,
            shadow_masked_pipeline_layout,
            shadow_masked_pipeline,
            sky_pipeline_layout,
            sky_uniform_buffer,
            sky_bind_group,
            bind_group_layout,
            shadow_bind_group_layout,
            ibl_bind_group_layout,
            ibl_uniform_buffer,
            ibl_bind_group,
            frame_bind_group_layout,
            frame_uniform_buffer,
            light_storage_buffer,
            light_bind_group_layout,
            light_bind_group,
            tile_light_grid_buffer: tile_grid_buf,
            tile_light_indices_buffer: tile_idx_buf,
            tile_bind_group_layout,
            tile_bind_group,
            tile_counts: (0, 0),
            tile_light_grid_scratch: TileLightGridScratch::default(),
            frame_bind_group,
            ibl_fallback,
            ibl_environment: None,
            brdf_lut: None,
            ibl_intensity: 1.0,
            shadow_sampler,
            white_texture_view,
            flat_normal_view,
            shadow_view,
            shadow_layer_views,
            cascade_index_bind_groups,
            cascade_matrices: [Mat4::IDENTITY; CASCADE_COUNT],
            occlusion_drawn: 0,
            occlusion_considered: 0,
            shadow_casters_drawn: 0,
            shadow_casters_considered: 0,
            shadow_caster_bundle: None,
            shadow_caster_count: 0,
            // z, w (tile counts) are written per frame in `render_tonemapped`.
            cascade_splits: [10.0, 30.0, 0.0, 0.0],
            primitives: Vec::new(),
            gpu_timing: GpuTiming::unavailable(),
            occlusion: OcclusionQueries::new(device),
            gpu_culling: None,
            occlusion_queries_enabled: false,
            gpu_culling_enabled: false,
            lod_enabled: false,
            lod_switch_distances: vec![8.0, 24.0],
            scene_bounds: None,
            uploaded_texture_count: 0,
            depth,
            hdr_view,
            depth_msaa,
            hdr_msaa,
            depth_resolve_pipeline,
            depth_resolve_bind_group,
            depth_resolve_bind_group_layout: depth_resolve_bgl,
            target_size: (width.max(1), height.max(1)),
            hdr_rebound_needed: true,
            bloom: BloomPass::new(gpu),
            ssao: SsaoPass::new(gpu),
            bloom_strength: 0.6,
            ssao_strength: 0.7,
            exposure_ev: 0.0,
            auto_exposure: false,
            auto_exposure_speed: 3.0,
            frame_delta_seconds: 1.0 / 60.0,
            histogram,
            punctual_lights: [[0.0; 4]; MAX_PUNCTUAL_LIGHTS * 4],
            punctual_light_count: 0,
            nodes: Vec::new(),
            animations: Vec::new(),
            skins: Vec::new(),
            pending_joint_world: None,
            light_dir_ambient: Vec4::new(0.5, 0.8, 0.3, 0.35),
            // ~5 because the BRDF divides diffuse by PI.
            light_color_intensity: Vec4::new(1.0, 0.97, 0.92, 5.0),
        }
    }

    /// Replaces one primitive's instance transforms; an empty slice restores the identity instance.
    pub fn set_instances(&mut self, gpu: &GpuContext, primitive_index: usize, transforms: &[Mat4]) {
        let Some(primitive) = self.primitives.get_mut(primitive_index) else {
            return;
        };

        if transforms.is_empty() {
            gpu.queue.write_buffer(
                &primitive.instance_buffer,
                0,
                bytemuck::bytes_of(&InstanceRaw::IDENTITY),
            );
            primitive.instance_count = 1;
            // Single identity instance again: bounds collapse to the posed box.
            primitive.instance_transforms.clear();
            let (min, max) = primitive.pre_instance_aabb;
            primitive.aabb_min = min;
            primitive.aabb_max = max;
            primitive.world_center = (min + max) * 0.5;
            self.recompute_scene_bounds();
            return;
        }

        let raw: Vec<InstanceRaw> = transforms
            .iter()
            .map(|m| InstanceRaw {
                model: m.to_cols_array_2d(),
            })
            .collect();
        let bytes = bytemuck::cast_slice(&raw);

        if (bytes.len() as u64) <= primitive.instance_buffer.size() {
            gpu.queue.write_buffer(&primitive.instance_buffer, 0, bytes);
        } else {
            primitive.instance_buffer =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("instances"),
                        contents: bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            // The bundle captured the old buffer by reference, and this replaced it.
            self.shadow_caster_bundle = None;
        }
        primitive.instance_count = raw.len() as u32;
        // Culling bounds must span every instance, not just the base position.
        primitive.instance_transforms = transforms.to_vec();
        let (min, max) = instanced_bounds(primitive.pre_instance_aabb, transforms);
        primitive.aabb_min = min;
        primitive.aabb_max = max;
        primitive.world_center = (min + max) * 0.5;
        self.recompute_scene_bounds();
    }

    /// Instance count of a primitive, for tests and diagnostics.
    pub fn instance_count(&self, primitive_index: usize) -> u32 {
        self.primitives
            .get(primitive_index)
            .map(|p| p.instance_count)
            .unwrap_or(0)
    }

    /// GPU textures created by the last [`Self::upload_scene`]; shared images count once.
    pub fn uploaded_texture_count(&self) -> usize {
        self.uploaded_texture_count
    }

    /// Whole-scene world bounds as cascade fitting reads them; `None` before any upload.
    pub fn scene_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.scene_bounds
    }

    /// A primitive's world culling bounds, exactly as the frustum test reads them.
    pub fn primitive_world_aabb(&self, primitive_index: usize) -> Option<(Vec3, Vec3)> {
        self.primitives
            .get(primitive_index)
            .map(|p| (p.aabb_min, p.aabb_max))
    }

    /// Number of pre-uploaded LOD levels for a primitive (0 when LOD is off).
    pub fn lod_level_count(&self, primitive_index: usize) -> usize {
        self.primitives
            .get(primitive_index)
            .map(|p| p.lod_levels.len())
            .unwrap_or(0)
    }

    /// Index count of one pre-uploaded level, `None` if it does not exist.
    pub fn lod_level_index_count(&self, primitive_index: usize, level: usize) -> Option<u32> {
        self.primitives
            .get(primitive_index)?
            .lod_levels
            .get(level)
            .map(|l| l.index_count)
    }

    /// Index count the draw path issues for a primitive from `eye` (the pass's own `geometry_for`).
    pub fn selected_index_count(&self, primitive_index: usize, eye: Vec3) -> Option<u32> {
        let prim = self.primitives.get(primitive_index)?;
        Some(prim.geometry_for(eye).2)
    }

    /// Starts collecting per-pass GPU timings (off by default: it is not free).
    /// Returns `false` when the adapter lacks `TIMESTAMP_QUERY`; the frame then renders unchanged.
    pub fn enable_gpu_timing(&mut self, gpu: &GpuContext) -> bool {
        self.gpu_timing = GpuTiming::new(&gpu.device, &gpu.queue);
        self.gpu_timing.is_available()
    }

    /// Shadow caster draws submitted / considered last frame, summed over cascades.
    pub fn shadow_caster_stats(&self) -> (u32, u32) {
        (self.shadow_casters_drawn, self.shadow_casters_considered)
    }

    /// Whether the shadow-caster bundle is cached, so tests can pin the cache/invalidate contract.
    pub fn shadow_caster_bundle_is_cached(&self) -> bool {
        self.shadow_caster_bundle.is_some()
    }

    /// Opaque primitives drawn / considered by the camera pass last frame (equal with culling off).
    /// Fed by `gpu_culling_enabled` (takes priority) or `occlusion_queries_enabled`.
    pub fn occlusion_cull_stats(&self) -> (u32, u32) {
        (self.occlusion_drawn, self.occlusion_considered)
    }

    /// Averaged per-pass GPU durations in ms, in record order; empty until the first readback.
    /// A pass absent from the list has not reported yet, which is not zero.
    pub fn gpu_timings_ms(&self) -> Vec<(&'static str, f32)> {
        self.gpu_timing.timings_ms()
    }

    /// Per-primitive occlusion visibility from the latest readback, index-aligned to primitives.
    /// Empty until the first async readback lands; a missing entry is unmeasured, not occluded.
    pub fn occlusion_visibility(&self) -> &[bool] {
        self.occlusion.visibility()
    }

    /// Like [`Self::occlusion_visibility`], but fed by the compute-shader culling path.
    pub fn gpu_culling_visibility(&self) -> &[bool] {
        self.gpu_culling
            .as_ref()
            .map(GpuCulling::visibility)
            .unwrap_or(&[])
    }

    /// Raw fragment counts behind [`Self::occlusion_visibility`].
    pub fn occlusion_samples(&self) -> &[u64] {
        self.occlusion.samples()
    }

    /// True while per-pass GPU timings are being collected.
    pub fn gpu_timing_available(&self) -> bool {
        self.gpu_timing.is_available()
    }

    /// Bakes `equirect` into IBL maps that light the scene from the next frame on.
    /// Blocks for the bake (one submit): a load-time call, not a per-frame one.
    pub fn set_environment(&mut self, gpu: &GpuContext, equirect: &EquirectImage) {
        // The BRDF table never touches the environment, so it is baked once and reused.
        let brdf_lut = self.brdf_lut.get_or_insert_with(|| BrdfLut::new(gpu));
        let environment = IblEnvironment::bake(gpu, equirect);

        gpu.queue.write_buffer(
            &self.ibl_uniform_buffer,
            0,
            bytemuck::bytes_of(&IblUniforms::enabled(
                environment.max_prefiltered_mip(),
                self.ibl_intensity,
            )),
        );
        self.ibl_bind_group = create_ibl_bind_group(
            &gpu.device,
            &self.ibl_bind_group_layout,
            &self.ibl_uniform_buffer,
            environment.irradiance_view(),
            environment.prefiltered_view(),
            brdf_lut.view(),
            &self.ibl_fallback.sampler,
        );
        self.ibl_environment = Some(environment);
    }

    /// Drops the environment and returns to the analytic hemisphere path.
    pub fn clear_environment(&mut self, gpu: &GpuContext) {
        self.ibl_environment = None;
        gpu.queue.write_buffer(
            &self.ibl_uniform_buffer,
            0,
            bytemuck::bytes_of(&IblUniforms::disabled()),
        );
        self.ibl_bind_group = create_ibl_bind_group(
            &gpu.device,
            &self.ibl_bind_group_layout,
            &self.ibl_uniform_buffer,
            &self.ibl_fallback.irradiance,
            &self.ibl_fallback.prefiltered,
            &self.ibl_fallback.brdf_lut,
            &self.ibl_fallback.sampler,
        );
    }

    /// True when a baked environment is lighting the scene.
    pub fn environment_enabled(&self) -> bool {
        self.ibl_environment.is_some()
    }

    /// The baked environment the frame is sampling, for tests and diagnostics.
    pub fn environment(&self) -> Option<&IblEnvironment> {
        self.ibl_environment.as_ref()
    }

    /// The shared split-sum BRDF table, once an environment has been set.
    pub fn brdf_lut(&self) -> Option<&BrdfLut> {
        self.brdf_lut.as_ref()
    }

    /// Uploads a CPU scene, replacing any previously uploaded one.
    pub fn upload_scene(&mut self, gpu: &GpuContext, scene: &CpuScene) {
        let device = &gpu.device;
        self.primitives.clear();
        // A cached bundle references buffers that are about to be dropped.
        self.shadow_caster_bundle = None;
        // Visibility is per primitive index, so a new scene must not inherit stale culls.
        self.occlusion.reset();
        if let Some(gpu_cull) = self.gpu_culling.as_mut() {
            gpu_cull.reset();
        }
        self.scene_bounds = compute_world_bounds(scene);
        let (packed, count) = pack_punctual_lights(&scene.lights);
        self.punctual_lights = packed;
        self.punctual_light_count = count;
        self.nodes = scene.nodes.clone();
        self.animations = scene.animations.clone();
        self.skins = scene.skins.clone();
        if scene.lights.len() > MAX_PUNCTUAL_LIGHTS {
            log::warn!(
                "Scene has {} punctual lights; only the first {} are used.",
                scene.lights.len(),
                MAX_PUNCTUAL_LIGHTS
            );
        }

        // Skinned primitives need the joint nodes' world matrices to size their bounds.
        let upload_node_world = CpuScene::compute_world_transforms(&scene.nodes);

        // One upload and mip build per (image `Arc` pointer, sRGB), not per referencing primitive.
        let mut texture_cache: std::collections::HashMap<(usize, bool), wgpu::TextureView> =
            std::collections::HashMap::new();
        let mut sampler_cache: std::collections::HashMap<CpuSampler, wgpu::Sampler> =
            std::collections::HashMap::new();
        let mut uploaded_textures = 0usize;

        for (i, prim) in scene.primitives.iter().enumerate() {
            // Morphed vertex buffers are re-uploaded per frame, so they need COPY_DST.
            let has_morph = !prim.morph_targets.is_empty();
            let vertex_usage = if has_morph {
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
            } else {
                wgpu::BufferUsages::VERTEX
            };
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("vertices_{i}")),
                contents: bytemuck::cast_slice(&prim.vertices),
                usage: vertex_usage,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("indices_{i}")),
                contents: bytemuck::cast_slice(&prim.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let uniform_buffer = buffer_desc::uniform(
                device,
                &format!("uniforms_{i}"),
                std::mem::size_of::<PrimUniforms>() as wgpu::BufferAddress,
            );

            let local_bounds = primitive_local_aabb(prim);
            // Widen now: joints may start posed, and set_animation_time bails without animations.
            let mut prim_bounds = primitive_world_aabb(prim);
            if let Some(skin) = prim.skin_index.and_then(|s| scene.skins.get(s)) {
                prim_bounds = widen_bounds_for_skin(
                    prim_bounds,
                    local_bounds.0,
                    local_bounds.1,
                    skin,
                    &upload_node_world,
                );
            }
            let material = &prim.material;
            let slots = [
                (&material.base_color_texture, &self.white_texture_view),
                (
                    &material.metallic_roughness_texture,
                    &self.white_texture_view,
                ),
                (&material.normal_texture, &self.flat_normal_view),
                (&material.emissive_texture, &self.white_texture_view),
                (&material.occlusion_texture, &self.white_texture_view),
            ];

            let mut views: Vec<wgpu::TextureView> = Vec::with_capacity(5);
            let mut samplers: Vec<wgpu::Sampler> = Vec::with_capacity(5);
            for (slot_index, (texture_ref, fallback)) in slots.iter().enumerate() {
                match texture_ref {
                    Some(tex_ref) => {
                        let key = (
                            std::sync::Arc::as_ptr(&tex_ref.texture) as usize,
                            tex_ref.srgb,
                        );
                        let view = match texture_cache.get(&key) {
                            Some(v) => v.clone(),
                            None => {
                                let v = create_material_texture(
                                    gpu,
                                    &tex_ref.texture,
                                    tex_ref.srgb,
                                    Some(&format!("material_{i}_slot_{slot_index}")),
                                );
                                uploaded_textures += 1;
                                texture_cache.insert(key, v.clone());
                                v
                            }
                        };
                        views.push(view);
                        samplers.push(
                            sampler_cache
                                .entry(tex_ref.sampler)
                                .or_insert_with(|| create_sampler(device, &tex_ref.sampler))
                                .clone(),
                        );
                    }
                    None => {
                        views.push((*fallback).clone());
                        let d = CpuSampler::default();
                        samplers.push(
                            sampler_cache
                                .entry(d)
                                .or_insert_with(|| create_sampler(device, &d))
                                .clone(),
                        );
                    }
                }
            }

            // Joint matrices: sized to the skin (identity when unskinned).
            let joint_count = prim
                .skin_index
                .and_then(|s| scene.skins.get(s))
                .map(|s| s.joints.len().clamp(1, MAX_JOINTS))
                .unwrap_or(1);
            let identity = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count];
            let joint_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("joints_{i}")),
                contents: bytemuck::cast_slice(&identity),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

            let mut bind_entries: Vec<wgpu::BindGroupEntry> = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ];
            for slot in 0..5usize {
                bind_entries.push(wgpu::BindGroupEntry {
                    binding: 3 + slot as u32 * 2,
                    resource: wgpu::BindingResource::TextureView(&views[slot]),
                });
                bind_entries.push(wgpu::BindGroupEntry {
                    binding: 4 + slot as u32 * 2,
                    resource: wgpu::BindingResource::Sampler(&samplers[slot]),
                });
            }

            bind_entries.push(wgpu::BindGroupEntry {
                binding: 13,
                resource: joint_buffer.as_entire_binding(),
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("bind_group_{i}")),
                layout: &self.bind_group_layout,
                entries: &bind_entries,
            });

            let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("shadow_bind_group_{i}")),
                layout: &self.shadow_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: joint_buffer.as_entire_binding(),
                    },
                ],
            });

            // Textured MASK casters alpha-test with the forward pass's own slot-0 view/sampler.
            let alpha_masked = matches!(material.alpha_mode, AlphaMode::Mask(_))
                && material.base_color_texture.is_some();
            let shadow_masked_bind_group = if alpha_masked {
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("shadow_masked_bind_group_{i}")),
                    layout: &self.shadow_masked_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&views[0]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&samplers[0]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: joint_buffer.as_entire_binding(),
                        },
                    ],
                }))
            } else {
                None
            };

            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("instances_{i}")),
                contents: bytemuck::bytes_of(&InstanceRaw::IDENTITY),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

            // Morphed primitives skip LOD: only `vertex_buffer` is re-blended, so levels would pop.
            let (lod_levels, lod_min_distances) = if self.lod_enabled && !has_morph {
                let chain = crate::scene::lod::build_lod_chain_with(
                    prim,
                    &self.lod_switch_distances,
                    // QEM's ratio is a triangle budget; clustering is a no-op on non-dense meshes.
                    crate::scene::lod::Simplifier::Quadric,
                );
                let mut levels = Vec::with_capacity(chain.len());
                let mut distances = Vec::with_capacity(chain.len());
                for (level, lod) in chain.iter().enumerate() {
                    // A level decimated to nothing ends the chain (wgpu rejects empty buffers).
                    if lod.primitive.indices.is_empty() || lod.primitive.vertices.is_empty() {
                        break;
                    }
                    levels.push(LodLevel {
                        vertex_buffer: device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some(&format!("vertices_{i}_lod_{level}")),
                                contents: bytemuck::cast_slice(&lod.primitive.vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        ),
                        index_buffer: device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some(&format!("indices_{i}_lod_{level}")),
                                contents: bytemuck::cast_slice(&lod.primitive.indices),
                                usage: wgpu::BufferUsages::INDEX,
                            },
                        ),
                        index_count: lod.primitive.indices.len() as u32,
                    });
                    distances.push(lod.min_distance);
                }
                (levels, distances)
            } else {
                (Vec::new(), Vec::new())
            };

            self.primitives.push(GpuPrimitive {
                vertex_buffer,
                index_buffer,
                index_count: prim.indices.len() as u32,
                uniform_buffer,
                bind_group,
                shadow_bind_group,
                alpha_masked,
                shadow_masked_bind_group,
                instance_buffer,
                instance_count: 1,
                instance_transforms: Vec::new(),
                pre_instance_aabb: prim_bounds,
                model: prim.transform,
                normal_matrix: normal_matrix_of(prim.transform),
                base_color: material.base_color,
                material_factors: [
                    material.metallic_factor,
                    material.roughness_factor,
                    material.occlusion_strength,
                    material.normal_scale,
                ],
                emissive_factor: [
                    material.emissive_factor[0],
                    material.emissive_factor[1],
                    material.emissive_factor[2],
                    // w carries the MASK alpha cutoff (0 = never discard).
                    match material.alpha_mode {
                        AlphaMode::Mask(cutoff) => cutoff,
                        _ => 0.0,
                    },
                ],
                base_uv_transform: material.base_uv_transform,
                mr_uv_transform: material.mr_uv_transform,
                normal_uv_transform: material.normal_uv_transform,
                emissive_uv_transform: material.emissive_uv_transform,
                occlusion_uv_transform: material.occlusion_uv_transform,
                uv_set_mask: material.uv_set_mask,
                node_index: prim.node_index,
                skin_index: prim.skin_index,
                joint_buffer,
                local_aabb_min: local_bounds.0,
                local_aabb_max: local_bounds.1,
                double_sided: material.double_sided,
                unlit: material.unlit,
                alpha_blend: material.alpha_mode == AlphaMode::Blend,
                // Blend casts no shadow; a MASK whose base alpha is below the cutoff is invisible.
                casts_shadow: match material.alpha_mode {
                    AlphaMode::Blend => false,
                    AlphaMode::Mask(cutoff) => material.base_color[3] >= cutoff,
                    AlphaMode::Opaque => true,
                },
                // AABB centre, not vertex centroid: `world_center` must stay one metric everywhere.
                world_center: (prim_bounds.0 + prim_bounds.1) * 0.5,
                aabb_min: prim_bounds.0,
                aabb_max: prim_bounds.1,
                lod_levels,
                lod_min_distances,
                // Neutral pose only for morphed primitives; the render path re-blends from it.
                base_vertices: if has_morph {
                    prim.vertices.clone()
                } else {
                    Vec::new()
                },
                morph_targets: prim.morph_targets.clone(),
                morph_weights: prim.morph_weights.clone(),
                // Non-zero default weights must show before any animation drives them.
                morph_dirty: has_morph && prim.morph_weights.iter().any(|w| *w != 0.0),
                uniforms_dirty: true,
            });
        }
        self.uploaded_texture_count = uploaded_textures;
    }

    /// Renders the scene HDR->tonemap into `output_view`, which must be `width` x `height`.
    pub fn render_tonemapped(
        &mut self,
        gpu: &GpuContext,
        tonemap: &mut TonemapPass,
        output_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
    ) {
        let (width, height) = (width.max(1), height.max(1));
        if self.target_size != (width, height) {
            self.depth = create_depth_texture(&gpu.device, width, height, 1);
            self.hdr_view = create_hdr_texture(&gpu.device, width, height, 1);
            self.depth_msaa = create_depth_texture(&gpu.device, width, height, MSAA_SAMPLE_COUNT);
            self.hdr_msaa = create_hdr_texture(&gpu.device, width, height, MSAA_SAMPLE_COUNT);
            // Recreate depth resolve bind group with new MSAA depth view.
            self.depth_resolve_bind_group =
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("depth_resolve_bind_group"),
                    layout: &self.depth_resolve_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.depth_msaa),
                    }],
                });
            // A stale view here reads a destroyed texture.
            self.histogram.set_input(gpu, &self.hdr_view);
            self.target_size = (width, height);
            self.hdr_rebound_needed = true;
        }
        if self.hdr_rebound_needed {
            self.bloom.rebuild(
                gpu,
                width,
                height,
                &self.hdr_view,
                self.histogram.exposure_buffer(),
            );
            self.ssao.rebuild(gpu, width, height, &self.depth);
            let bloom_out = self
                .bloom
                .output()
                .expect("bloom output exists after rebuild")
                .clone();
            let ao_out = self
                .ssao
                .output()
                .expect("ssao output exists after rebuild")
                .clone();
            tonemap.set_input(
                gpu,
                &self.hdr_view,
                &bloom_out,
                &ao_out,
                self.histogram.exposure_buffer(),
            );
            self.hdr_rebound_needed = false;
        }
        tonemap.set_params(&gpu.queue, self.bloom_strength, self.ssao_strength);
        self.histogram.set_exposure_settings(
            &gpu.queue,
            crate::render::histogram::ExposureSettings {
                delta_time_seconds: self.frame_delta_seconds.max(0.0),
                speed: self.auto_exposure_speed,
                auto_enabled: self.auto_exposure,
                manual_ev: self.exposure_ev,
            },
        );

        self.update_joint_matrices(gpu);
        self.apply_morph_targets(gpu);

        let aspect = width as f32 / height as f32;
        let view_proj = camera.view_projection(aspect);
        let frustum = Frustum::from_view_proj(&view_proj);
        self.ssao
            .write_uniforms(&gpu.queue, camera.projection(aspect), 0.6, 0.02, 1.0);
        self.update_cascades(camera);
        let light_space = self.cascade_matrices[0];
        let eye = camera.eye();

        let sky_uniforms = SkyUniforms {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            light_dir_intensity: [
                self.light_dir_ambient.x,
                self.light_dir_ambient.y,
                self.light_dir_ambient.z,
                self.light_color_intensity.w,
            ],
        };
        gpu.queue.write_buffer(
            &self.sky_uniform_buffer,
            0,
            bytemuck::bytes_of(&sky_uniforms),
        );

        // Before the frame uniforms, so this frame's tile counts land in cascade_splits.zw.
        let tx = width.div_ceil(TILE_SIZE);
        let ty = height.div_ceil(TILE_SIZE);
        self.tile_counts = (tx, ty);

        // Per-frame data: write ONCE, shared by every primitive via @group(2).
        let frame_uniforms = FrameUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_space: light_space.to_cols_array_2d(),
            light_space_1: self.cascade_matrices[1].to_cols_array_2d(),
            light_space_2: self.cascade_matrices[2].to_cols_array_2d(),
            light_dir_ambient: self.light_dir_ambient.to_array(),
            light_color_intensity: self.light_color_intensity.to_array(),
            camera_position: [eye.x, eye.y, eye.z, self.punctual_light_count as f32],
            cascade_splits: [
                self.cascade_splits[0],
                self.cascade_splits[1],
                self.tile_counts.0 as f32,
                self.tile_counts.1 as f32,
            ],
        };
        gpu.queue.write_buffer(
            &self.frame_uniform_buffer,
            0,
            bytemuck::bytes_of(&frame_uniforms),
        );
        // Write punctual lights to storage buffer (group 3, forward-only).
        gpu.queue.write_buffer(
            &self.light_storage_buffer,
            0,
            bytemuck::cast_slice(&self.punctual_lights[..]),
        );
        // Build and upload tile light grid (group 4).
        {
            build_tile_light_grid(
                &mut self.tile_light_grid_scratch,
                &self.punctual_lights,
                self.punctual_light_count,
                width,
                height,
                &view_proj,
            );
            let total_tiles = (tx * ty) as usize;

            // A bind group captures buffer identity, so rebuild it if either buffer is recreated.
            let mut bind_group_dirty = false;
            let needed_grid = (total_tiles as u64) * 8; // vec2<u32> × total_tiles
            if needed_grid > self.tile_light_grid_buffer.size() {
                self.tile_light_grid_buffer =
                    buffer_desc::storage_dst(&gpu.device, "tile_light_grid", needed_grid);
                bind_group_dirty = true;
            }
            let needed_idx = (self.tile_light_grid_scratch.indices.len() as u64) * 4;
            if needed_idx > self.tile_light_indices_buffer.size() {
                self.tile_light_indices_buffer =
                    buffer_desc::storage_dst(&gpu.device, "tile_light_indices", needed_idx.max(64));
                bind_group_dirty = true;
            }
            if bind_group_dirty {
                self.tile_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tile_light_bg"),
                    layout: &self.tile_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.tile_light_grid_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.tile_light_indices_buffer.as_entire_binding(),
                        },
                    ],
                });
            }
            gpu.queue.write_buffer(
                &self.tile_light_grid_buffer,
                0,
                bytemuck::cast_slice(&self.tile_light_grid_scratch.grid),
            );
            gpu.queue.write_buffer(
                &self.tile_light_indices_buffer,
                0,
                bytemuck::cast_slice(&self.tile_light_grid_scratch.indices),
            );
        }

        // Pads a KHR_texture_transform's two affine rows to the [f32; 4] pair PrimUniforms packs.
        fn uv_rows(t: [[f32; 3]; 2]) -> ([f32; 4], [f32; 4]) {
            (
                [t[0][0], t[0][1], t[0][2], 0.0],
                [t[1][0], t[1][1], t[1][2], 0.0],
            )
        }

        for prim in &mut self.primitives {
            if !prim.uniforms_dirty {
                continue;
            }
            let (base_uv_row0, base_uv_row1) = uv_rows(prim.base_uv_transform);
            let (mr_uv_row0, mr_uv_row1) = uv_rows(prim.mr_uv_transform);
            let (normal_uv_row0, normal_uv_row1) = uv_rows(prim.normal_uv_transform);
            let (emissive_uv_row0, emissive_uv_row1) = uv_rows(prim.emissive_uv_transform);
            let (occlusion_uv_row0, occlusion_uv_row1) = uv_rows(prim.occlusion_uv_transform);
            let prim_uniforms = PrimUniforms {
                model: prim.model.to_cols_array_2d(),
                normal_matrix: prim.normal_matrix.to_cols_array_2d(),
                base_color: prim.base_color,
                material_factors: prim.material_factors,
                emissive_factor: prim.emissive_factor,
                base_uv_row0,
                base_uv_row1,
                mr_uv_row0,
                mr_uv_row1,
                normal_uv_row0,
                normal_uv_row1,
                emissive_uv_row0,
                emissive_uv_row1,
                occlusion_uv_row0,
                occlusion_uv_row1,
                material_flags: [
                    if prim.unlit { 1.0 } else { 0.0 },
                    prim.uv_set_mask as f32,
                    0.0,
                    0.0,
                ],
            };
            gpu.queue
                .write_buffer(&prim.uniform_buffer, 0, bytemuck::bytes_of(&prim_uniforms));
            prim.uniforms_dirty = false;
        }

        // Pass wiring is validated once by `graph::tests::forward_graph_is_valid`, not per frame.

        // Before any scope: begin_frame picks the ring slot every scope below writes into.
        self.gpu_timing.begin_frame();

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("forward_encoder"),
            });
        let shadow_scope = self.gpu_timing.scope(TimedPass::ShadowCascades);

        // A cached bundle replayed per cascade; the baked draw list rules out per-cascade culling.
        if self.shadow_caster_bundle.is_none() {
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
            bundle_encoder.set_bind_group(1, &self.cascade_index_bind_groups[0], &[]);
            bundle_encoder.set_bind_group(2, &self.frame_bind_group, &[]);
            let mut caster_count = 0u32;
            for prim in self.primitives.iter().filter(|p| p.casts_shadow) {
                caster_count += 1;
                if let (true, Some(masked_group)) =
                    (prim.alpha_masked, prim.shadow_masked_bind_group.as_ref())
                {
                    bundle_encoder.set_pipeline(&self.shadow_masked_pipeline);
                    bundle_encoder.set_bind_group(0, masked_group, &[]);
                } else {
                    bundle_encoder.set_pipeline(&self.shadow_pipeline);
                    bundle_encoder.set_bind_group(0, &prim.shadow_bind_group, &[]);
                }
                bundle_encoder.set_vertex_buffer(0, prim.vertex_buffer.slice(..));
                bundle_encoder.set_vertex_buffer(1, prim.instance_buffer.slice(..));
                bundle_encoder
                    .set_index_buffer(prim.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                bundle_encoder.draw_indexed(0..prim.index_count, 0, 0..prim.instance_count);
            }
            self.shadow_caster_bundle =
                Some(bundle_encoder.finish(&wgpu::RenderBundleDescriptor {
                    label: Some("shadow_caster_bundle"),
                }));
            self.shadow_caster_count = caster_count;
        }
        let caster_bundle = self
            .shadow_caster_bundle
            .as_ref()
            .expect("just built above when absent");

        for cascade in 0..CASCADE_COUNT {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_layer_views[cascade],
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
            pass.set_bind_group(1, &self.cascade_index_bind_groups[cascade], &[]);
            pass.set_bind_group(2, &self.frame_bind_group, &[]);
            pass.execute_bundles(std::iter::once(caster_bundle));
        }
        // No per-cascade culling, so drawn == considered by construction.
        self.shadow_casters_drawn = self.shadow_caster_count;
        self.shadow_casters_considered = self.shadow_caster_count;
        // Copied to the fields once the pass, which borrows `self`, has ended.
        let mut occ_drawn = 0u32;
        let mut occ_considered = 0u32;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("forward_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_msaa,
                    resolve_target: Some(&self.hdr_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_msaa,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        // Not Discard: the depth resolve is a later, separate pass that reads this.
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self
                    .gpu_timing
                    .scope(TimedPass::Forward)
                    .render_writes(0, 1),
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Per-frame groups 1-4; rebound after the sky, whose own layout invalidates them.
            pass.set_bind_group(1, &self.ibl_bind_group, &[]);
            pass.set_bind_group(2, &self.frame_bind_group, &[]);
            pass.set_bind_group(3, &self.light_bind_group, &[]);
            pass.set_bind_group(4, &self.tile_bind_group, &[]);
            for (i, prim) in self.primitives.iter().enumerate() {
                // Frustum first (cheap, no dependency on last frame).
                if prim.alpha_blend || !frustum.intersects_aabb(prim.aabb_min, prim.aabb_max) {
                    continue;
                }
                occ_considered += 1;
                // Last frame's result; unmeasured primitives count as visible, so nothing pops.
                let occluded = if self.gpu_culling_enabled {
                    self.gpu_culling.as_ref().is_some_and(|c| !c.visible(i))
                } else if self.occlusion_queries_enabled {
                    !self.occlusion.visible(i)
                } else {
                    false
                };
                // Eye inside the box: only back faces rasterise and fail depth, so never skip.
                if occluded && !aabb_contains_point(prim.aabb_min, prim.aabb_max, eye) {
                    continue;
                }
                occ_drawn += 1;
                let pipeline = if prim.double_sided {
                    &self.pipeline_double_sided
                } else {
                    &self.pipeline
                };
                let (vertex_buffer, index_buffer, index_count) = prim.geometry_for(eye);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &prim.bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, prim.instance_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..prim.instance_count);
            }

            // Sky fills every pixel geometry left untouched.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // Transparents last, farthest first, no depth writes.
            let mut blended: Vec<&GpuPrimitive> = self
                .primitives
                .iter()
                .filter(|p| p.alpha_blend && frustum.intersects_aabb(p.aabb_min, p.aabb_max))
                .collect();
            blended.sort_by(|a, b| {
                let da = a.world_center.distance_squared(eye);
                let db = b.world_center.distance_squared(eye);
                db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
            });
            if !blended.is_empty() {
                pass.set_bind_group(1, &self.ibl_bind_group, &[]);
                pass.set_bind_group(2, &self.frame_bind_group, &[]);
                pass.set_bind_group(3, &self.light_bind_group, &[]);
                pass.set_bind_group(4, &self.tile_bind_group, &[]);
            }
            for prim in blended {
                let pipeline = if prim.double_sided {
                    &self.pipeline_blend_double_sided
                } else {
                    &self.pipeline_blend
                };
                let (vertex_buffer, index_buffer, index_count) = prim.geometry_for(eye);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &prim.bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, prim.instance_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..prim.instance_count);
            }
        }

        // Depth resolve: MSAA depth to single-sample (min over samples) for SSAO and occlusion.
        {
            let mut resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth_resolve"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
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
            resolve.set_pipeline(&self.depth_resolve_pipeline);
            resolve.set_bind_group(0, &self.depth_resolve_bind_group, &[]);
            resolve.draw(0..3, 0..1);
        }

        self.occlusion_drawn = occ_drawn;
        self.occlusion_considered = occ_considered;

        // Occlusion detection: AABB proxies vs this frame's depth, by compute or hardware queries.
        if !self.primitives.is_empty() {
            let aabbs: Vec<(Vec3, Vec3)> = self
                .primitives
                .iter()
                .map(|p| (p.aabb_min, p.aabb_max))
                .collect();

            if self.gpu_culling_enabled {
                // GPU compute-shader culling
                let gpu_cull = self
                    .gpu_culling
                    .get_or_insert_with(|| GpuCulling::new(gpu, self.primitives.len().max(1024)));
                let (width, height) = self.target_size;
                let inv_view_proj = view_proj.inverse();
                // Use the resolved single-sample depth for culling
                gpu_cull.cull(
                    gpu,
                    &mut encoder,
                    &self.depth,
                    width,
                    height,
                    bytemuck::cast_ref(&view_proj),
                    bytemuck::cast_ref(&inv_view_proj),
                    &aabbs,
                );
            } else if self.occlusion_queries_enabled {
                // Hardware occlusion query path
                self.occlusion.record(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &self.depth,
                    view_proj,
                    &aabbs,
                    eye,
                    self.gpu_timing.scope(TimedPass::OcclusionCull),
                );
            }
        }

        // At zero strength the tonemap provably ignores these outputs, so skip the passes.
        if self.bloom_strength > 0.0 {
            self.bloom
                .encode(&mut encoder, self.gpu_timing.scope(TimedPass::Bloom));
        }
        if self.ssao_strength > 0.0 {
            self.ssao
                .encode(&mut encoder, self.gpu_timing.scope(TimedPass::Ssao));
        }
        // Manual exposure never reads the bins, so the full-res histogram build is auto-mode only.
        if self.auto_exposure {
            self.histogram.encode(
                &mut encoder,
                width,
                height,
                self.gpu_timing.scope(TimedPass::Histogram),
            );
        }
        // Always reduce: manual mode copies its EV into the one buffer the tonemap reads.
        self.histogram.encode_reduce(
            &mut encoder,
            self.gpu_timing.scope(TimedPass::ExposureReduce),
        );
        tonemap.render(
            &mut encoder,
            output_view,
            self.gpu_timing.scope(TimedPass::Tonemap),
        );
        // In the frame's encoder: a second submit would serialise against the frame it measures.
        self.gpu_timing.resolve(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        self.gpu_timing.end_frame(&gpu.device);
        // Non-blocking: results lag a frame or more, which advisory visibility tolerates.
        self.occlusion.end_frame(&gpu.device);
        // The culling paths are exclusive, so only the enabled one has readbacks in flight.
        if self.gpu_culling_enabled {
            if let Some(gpu_cull) = self.gpu_culling.as_mut() {
                gpu_cull.end_frame(&gpu.device);
            }
        }
    }

    /// Renders one tonemapped frame headless and returns tightly packed row-major RGBA8 bytes.
    pub fn render_to_pixels(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
    ) -> anyhow::Result<Vec<u8>> {
        self.render_to_pixels_with_format(
            gpu,
            width,
            height,
            camera,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    }

    /// As [`Self::render_to_pixels`] with an explicit format, for the non-sRGB path browsers use.
    pub fn render_to_pixels_with_format(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        camera: &OrbitCamera,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<Vec<u8>> {
        let mut tonemap = TonemapPass::new(gpu, format);

        let texture = create_2d_texture(
            &gpu.device,
            Some("offscreen_color"),
            width,
            height,
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            1,
            1,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Force a rebind: this pass instance has never seen the HDR view.
        self.hdr_rebound_needed = true;
        self.render_tonemapped(gpu, &mut tonemap, &view, width, height, camera);

        let bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
        let readback = buffer_desc::readback(&gpu.device, "readback", buffer_size);

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback_encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("Device poll failed while mapping readback buffer")?;
        rx.recv()
            .context("Readback mapping callback dropped")?
            .context("Failed to map readback buffer")?;

        // Fallible since wgpu 30; this function returns Result, so propagate rather than panic.
        let data = slice
            .get_mapped_range()
            .context("Failed to view the mapped readback buffer")?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * bytes_per_row) as usize;
            pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        Ok(pixels)
    }

    /// Rebuilds the scene/shadow/sky pipelines from new WGSL; invalid shaders keep the old ones.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_shaders(
        &mut self,
        gpu: &GpuContext,
        forward_wgsl: &str,
        sky_wgsl: &str,
    ) -> anyhow::Result<()> {
        let device = &gpu.device;
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(forward_wgsl.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sky_wgsl.into()),
        });
        let set = create_forward_pipeline_set(device, &shader, &self.pipeline_layout);
        let shadow = create_shadow_pipeline(device, &shader, &self.shadow_pipeline_layout);
        let shadow_masked =
            create_masked_shadow_pipeline(device, &shader, &self.shadow_masked_pipeline_layout);
        let sky = create_sky_pipeline(device, &sky_shader, &self.sky_pipeline_layout);
        if let Some(err) = pollster::block_on(error_scope.pop()) {
            anyhow::bail!("shader reload rejected: {err}");
        }
        (
            self.pipeline,
            self.pipeline_double_sided,
            self.pipeline_blend,
            self.pipeline_blend_double_sided,
        ) = set;
        self.shadow_pipeline = shadow;
        self.shadow_masked_pipeline = shadow_masked;
        self.sky_pipeline = sky;
        Ok(())
    }

    /// Uploads skin joint matrices (joint world * inverse bind) once per frame.
    fn update_joint_matrices(&mut self, gpu: &GpuContext) {
        let Some(world) = self.pending_joint_world.take() else {
            return;
        };
        if self.skins.is_empty() {
            return;
        }
        for prim in &self.primitives {
            let Some(skin_index) = prim.skin_index else {
                continue;
            };
            let Some(skin) = self.skins.get(skin_index) else {
                continue;
            };
            let matrices: Vec<[[f32; 4]; 4]> = skin
                .joints
                .iter()
                .take(MAX_JOINTS)
                .enumerate()
                .map(|(i, &node)| {
                    let joint_world = world.get(node).copied().unwrap_or(Mat4::IDENTITY);
                    let inverse_bind = skin
                        .inverse_bind_matrices
                        .get(i)
                        .copied()
                        .unwrap_or(Mat4::IDENTITY);
                    (joint_world * inverse_bind).to_cols_array_2d()
                })
                .collect();
            if !matrices.is_empty() {
                gpu.queue
                    .write_buffer(&prim.joint_buffer, 0, bytemuck::cast_slice(&matrices));
            }
        }
    }

    /// Re-blends and re-uploads the vertex buffer of every morphed primitive whose weights moved.
    fn apply_morph_targets(&mut self, gpu: &GpuContext) {
        for prim in &mut self.primitives {
            if !prim.morph_dirty || prim.base_vertices.is_empty() {
                continue;
            }
            let blended = crate::scene::blend_morph_targets(
                &prim.base_vertices,
                &prim.morph_targets,
                &prim.morph_weights,
            );
            gpu.queue
                .write_buffer(&prim.vertex_buffer, 0, bytemuck::cast_slice(&blended));
            prim.morph_dirty = false;
        }
    }

    /// True when the uploaded scene carries animations.
    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }

    /// Samples every animation at `time` (s) and retargets transforms, bounds and blend centres.
    pub fn set_animation_time(&mut self, time: f32) {
        if self.animations.is_empty() || self.nodes.is_empty() {
            return;
        }
        for animation in &self.animations {
            for channel in &animation.channels {
                let Some(node) = self.nodes.get_mut(channel.node) else {
                    continue;
                };
                let t = if animation.duration > 0.0 {
                    time % animation.duration
                } else {
                    0.0
                };
                let (i0, i1, frac, dt) = keyframe_lerp_indices(&channel.times, t);
                match &channel.values {
                    ChannelValues::Translation(values) => {
                        if let Some(v) =
                            sample_vec3(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.translation = v;
                        }
                    }
                    ChannelValues::Rotation(values) => {
                        if let Some(q) =
                            sample_quat(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.rotation = q;
                        }
                    }
                    ChannelValues::Scale(values) => {
                        if let Some(v) =
                            sample_vec3(values, channel.interpolation, i0, i1, frac, dt)
                        {
                            node.scale = v;
                        }
                    }
                    // Sampled in the pass below, which avoids fighting the `nodes` borrow.
                    ChannelValues::MorphWeights(_) => {}
                }
            }
        }

        // Morph weights; the target count comes from each primitive to stride the flat channel.
        for animation in &self.animations {
            let t = if animation.duration > 0.0 {
                time % animation.duration
            } else {
                0.0
            };
            for channel in &animation.channels {
                let ChannelValues::MorphWeights(values) = &channel.values else {
                    continue;
                };
                let (i0, i1, frac, dt) = keyframe_lerp_indices(&channel.times, t);
                for prim in &mut self.primitives {
                    if prim.node_index != Some(channel.node) || prim.morph_targets.is_empty() {
                        continue;
                    }
                    let n = prim.morph_targets.len();
                    let w =
                        sample_morph_weights(values, n, channel.interpolation, i0, i1, frac, dt);
                    if w != prim.morph_weights {
                        prim.morph_weights = w;
                        prim.morph_dirty = true;
                    }
                }
            }
        }

        let world = CpuScene::compute_world_transforms(&self.nodes);
        self.pending_joint_world = Some(world.clone());
        for prim in &mut self.primitives {
            if let Some(node) = prim.node_index {
                if let Some(m) = world.get(node) {
                    prim.model = *m;
                    prim.normal_matrix = normal_matrix_of(*m);
                    prim.uniforms_dirty = true;
                    let mut bounds = transform_aabb(*m, prim.local_aabb_min, prim.local_aabb_max);
                    if let Some(skin) = prim.skin_index.and_then(|s| self.skins.get(s)) {
                        bounds = widen_bounds_for_skin(
                            bounds,
                            prim.local_aabb_min,
                            prim.local_aabb_max,
                            skin,
                            &world,
                        );
                    }
                    // The posed box moved, so re-apply the instances on top.
                    prim.pre_instance_aabb = bounds;
                    let (min, max) = instanced_bounds(bounds, &prim.instance_transforms);
                    prim.aabb_min = min;
                    prim.aabb_max = max;
                    prim.world_center = (min + max) * 0.5;
                }
            }
        }
        self.recompute_scene_bounds();
    }

    /// Re-derives `scene_bounds`, the sole cascade-fitting input; every box mutator must call it.
    /// See `../../docs/renderer-bounds-invariant.md` § Maintainers — everything that must update bounds
    fn recompute_scene_bounds(&mut self) {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for prim in &self.primitives {
            min = min.min(prim.aabb_min);
            max = max.max(prim.aabb_max);
        }
        if min.x <= max.x {
            self.scene_bounds = Some((min, max));
        }
    }

    /// Fits one light matrix per cascade; see `render::cascades` for why splits track the camera.
    fn update_cascades(&mut self, camera: &OrbitCamera) {
        let (min, max) = self
            .scene_bounds
            .unwrap_or((Vec3::splat(-1.0), Vec3::splat(1.0)));
        let light_dir = self.light_dir_ambient.truncate().normalize_or_zero();

        let fit = crate::render::cascades::fit_cascades(camera, min, max, light_dir);
        // z, w (tile counts) are written per frame in `render_tonemapped`.
        self.cascade_splits = [fit.splits[0], fit.splits[1], 0.0, 0.0];
        self.cascade_matrices = fit.matrices;
    }
}

/// Group 1 uniforms, mirroring `IblParams` in forward.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct IblUniforms {
    // x: enabled flag, y: highest prefiltered mip, z: intensity, w: unused
    enabled_maxmip_intensity: [f32; 4],
}

impl IblUniforms {
    fn disabled() -> Self {
        Self {
            enabled_maxmip_intensity: [0.0, 0.0, 1.0, 0.0],
        }
    }

    fn enabled(max_mip: f32, intensity: f32) -> Self {
        Self {
            enabled_maxmip_intensity: [1.0, max_mip, intensity, 0.0],
        }
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

fn create_ibl_bind_group(
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

type ForwardPipelineSet = (
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
);

fn create_forward_pipeline_set(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
) -> ForwardPipelineSet {
    let make = |cull_mode: Option<wgpu::Face>, blend: bool, label: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(if blend {
                        wgpu::BlendState::ALPHA_BLENDING
                    } else {
                        wgpu::BlendState::REPLACE
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(!blend),
                depth_compare: Some(wgpu::CompareFunction::Less),
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
    };
    (
        make(Some(wgpu::Face::Back), false, "forward_pipeline"),
        make(None, false, "forward_pipeline_double_sided"),
        make(Some(wgpu::Face::Back), true, "forward_pipeline_blend"),
        make(None, true, "forward_pipeline_blend_double_sided"),
    )
}

fn create_depth_resolve_pipeline(
    device: &wgpu::Device,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("depth_resolve_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/depth_resolve.wgsl").into()),
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

fn create_sky_pipeline(
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

use crate::render::bounds::{
    aabb_contains_point, compute_world_bounds, instanced_bounds, normal_matrix_of,
    primitive_local_aabb, primitive_world_aabb, transform_aabb, widen_bounds_for_skin, Frustum,
};
use crate::render::texture::{
    create_2d_texture, create_depth_texture, create_hdr_texture, create_material_texture,
    create_sampler,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_animation_time_recomputes_and_dirties_the_cached_normal_matrix() {
        let Some(gpu) = GpuContext::headless_or_skip() else {
            return;
        };

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets/cube_animated.gltf");
        let scene = crate::load_gltf(&path).expect("cube_animated.gltf must load");
        let mut renderer = ForwardRenderer::new(&gpu, 4, 4);
        renderer.upload_scene(&gpu, &scene);
        assert!(
            renderer.primitives.iter().any(|p| p.node_index.is_some()),
            "cube_animated.gltf must have at least one node-driven primitive"
        );

        // As if uploaded this frame, so only set_animation_time can re-dirty the flag.
        for prim in &mut renderer.primitives {
            prim.uniforms_dirty = false;
        }

        renderer.set_animation_time(1.0);

        for prim in &renderer.primitives {
            if prim.node_index.is_none() {
                continue;
            }
            assert!(
                prim.uniforms_dirty,
                "an animated primitive's model changed, so its uniforms must be re-marked dirty"
            );
            let expected = normal_matrix_of(prim.model);
            assert!(
                prim.normal_matrix.is_finite(),
                "cached normal matrix must stay finite"
            );
            assert!(
                (prim.normal_matrix - expected).abs_diff_eq(Mat4::ZERO, 1e-6),
                "cached normal matrix must match a fresh computation from the current model"
            );
        }
    }
}
