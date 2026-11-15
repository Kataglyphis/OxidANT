//! One uploaded primitive: its buffers, bind groups, bounds, material, LOD chain and morph state.

use glam::{Mat4, Vec3};

use crate::scene::{AlphaMode, CpuMaterial, MorphTarget, Vertex};

/// Group 0 binding 0 of the forward and shadow pipelines.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PrimUniforms {
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
pub(super) struct LodLevel {
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) index_buffer: wgpu::Buffer,
    pub(super) index_count: u32,
}

/// The material parameters `PrimUniforms` carries, fixed at upload.
pub(super) struct PrimMaterial {
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
    /// KHR_materials_unlit: skip lighting entirely and emit the base color.
    unlit: bool,
}

impl PrimMaterial {
    pub(super) fn from_cpu(material: &CpuMaterial) -> Self {
        Self {
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
            unlit: material.unlit,
        }
    }
}

pub(super) struct GpuPrimitive {
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) index_buffer: wgpu::Buffer,
    pub(super) index_count: u32,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
    /// Uniforms-only group: the full group samples the shadow map the shadow pass writes.
    pub(super) shadow_bind_group: wgpu::BindGroup,
    /// MASK material with a base-color texture: shadows through the alpha-testing pipeline.
    pub(super) alpha_masked: bool,
    pub(super) shadow_masked_bind_group: Option<wgpu::BindGroup>,
    /// Per-instance transforms; always at least one, so every draw binds slot 1.
    pub(super) instance_buffer: wgpu::Buffer,
    pub(super) instance_count: u32,
    /// Last-set instance transforms (empty = identity); bounds recompose when either side moves.
    pub(super) instance_transforms: Vec<Mat4>,
    /// World bounds before instancing, so recomposition never re-derives the skinned/morphed pose.
    pub(super) pre_instance_aabb: (Vec3, Vec3),
    pub(super) model: Mat4,
    /// Cached `normal_matrix_of(model)`, recomputed only where `model` changes.
    pub(super) normal_matrix: Mat4,
    pub(super) material: PrimMaterial,
    pub(super) double_sided: bool,
    pub(super) alpha_blend: bool,
    pub(super) casts_shadow: bool,
    pub(super) world_center: Vec3,
    pub(super) aabb_min: Vec3,
    pub(super) aabb_max: Vec3,
    pub(super) node_index: Option<usize>,
    pub(super) skin_index: Option<usize>,
    pub(super) joint_buffer: wgpu::Buffer,
    pub(super) local_aabb_min: Vec3,
    pub(super) local_aabb_max: Vec3,
    /// Simplified levels, coarsest last; empty when LOD is off, so that draw path is unchanged.
    pub(super) lod_levels: Vec<LodLevel>,
    /// Switch distance per `lod_levels` entry, kept contiguous so selection never allocates.
    pub(super) lod_min_distances: Vec<f32>,
    /// Neutral pose, kept only for morphed primitives so each frame re-blends, not accumulates.
    pub(super) base_vertices: Vec<Vertex>,
    /// POSITION/NORMAL deltas, one entry per morph target.
    pub(super) morph_targets: Vec<MorphTarget>,
    /// Current per-target weights (animation-driven or the mesh defaults).
    pub(super) morph_weights: Vec<f32>,
    /// Vertex buffer needs re-blending; starts true so non-zero default weights apply once.
    pub(super) morph_dirty: bool,
    /// `PrimUniforms` need re-upload; instance changes don't count, they have their own buffer.
    pub(super) uniforms_dirty: bool,
}

impl GpuPrimitive {
    /// Buffers and index count to draw from `eye`, by `world_center` distance like the blend sort.
    pub(super) fn geometry_for(&self, eye: Vec3) -> (&wgpu::Buffer, &wgpu::Buffer, u32) {
        let distance = self.world_center.distance(eye);
        match crate::scene::lod::select_lod_by_distance(&self.lod_min_distances, distance) {
            Some(level) => {
                let lod = &self.lod_levels[level];
                (&lod.vertex_buffer, &lod.index_buffer, lod.index_count)
            }
            None => (&self.vertex_buffer, &self.index_buffer, self.index_count),
        }
    }

    /// The uniforms for the current model and material.
    pub(super) fn uniforms(&self) -> PrimUniforms {
        let material = &self.material;
        let (base_uv_row0, base_uv_row1) = uv_rows(material.base_uv_transform);
        let (mr_uv_row0, mr_uv_row1) = uv_rows(material.mr_uv_transform);
        let (normal_uv_row0, normal_uv_row1) = uv_rows(material.normal_uv_transform);
        let (emissive_uv_row0, emissive_uv_row1) = uv_rows(material.emissive_uv_transform);
        let (occlusion_uv_row0, occlusion_uv_row1) = uv_rows(material.occlusion_uv_transform);
        PrimUniforms {
            model: self.model.to_cols_array_2d(),
            normal_matrix: self.normal_matrix.to_cols_array_2d(),
            base_color: material.base_color,
            material_factors: material.material_factors,
            emissive_factor: material.emissive_factor,
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
                if material.unlit { 1.0 } else { 0.0 },
                material.uv_set_mask as f32,
                0.0,
                0.0,
            ],
        }
    }
}

/// Pads a KHR_texture_transform's two affine rows to the [f32; 4] pair PrimUniforms packs.
fn uv_rows(t: [[f32; 3]; 2]) -> ([f32; 4], [f32; 4]) {
    (
        [t[0][0], t[0][1], t[0][2], 0.0],
        [t[1][0], t[1][1], t[1][2], 0.0],
    )
}
