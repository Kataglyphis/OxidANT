//! One uploaded primitive: its buffers, bind groups, bounds, material, LOD chain and morph state.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::upload::MaterialSlots;
use crate::context::GpuContext;
use crate::render::bounds::instanced_bounds;
use crate::scene::{AlphaMode, CpuMaterial, InstanceRaw, MorphTarget, Vertex};

/// True when `m` mirrors space (negative determinant), which reverses its triangles' winding.
pub(super) fn is_mirrored(m: &Mat4) -> bool {
    glam::Mat3::from_mat4(*m).determinant() < 0.0
}

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
                // The flat fallback decodes 128 to about 0.004, a 0.32 degree tilt; scale 0 keeps the geometric normal, as C++ does.
                if material.normal_texture.is_some() {
                    material.normal_scale
                } else {
                    0.0
                },
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
    /// The views and samplers `bind_group` binds, kept to rebuild it around a reallocated shadow map.
    pub(super) material_slots: MaterialSlots,
    /// Uniforms-only group: the full group samples the shadow map the shadow pass writes.
    pub(super) shadow_bind_group: wgpu::BindGroup,
    /// MASK material: shadows through the alpha-testing pipeline, which also reads vertex alpha.
    pub(super) alpha_masked: bool,
    pub(super) shadow_masked_bind_group: Option<wgpu::BindGroup>,
    /// Per-instance transforms; always at least one, so every draw binds slot 1.
    pub(super) instance_buffer: wgpu::Buffer,
    pub(super) instance_count: u32,
    /// The buffer holds unmirrored instances first; those from this index on mirror space.
    pub(super) mirrored_instances_from: u32,
    /// `model` mirrors space, so its front faces wind clockwise (glTF 3.7.4).
    pub(super) mirrored: bool,
    /// Last-set instance transforms (empty = identity); bounds recompose when either side moves.
    pub(super) instance_transforms: Vec<Mat4>,
    /// The Model tab's transform, left of every instance.
    pub(super) root: Mat4,
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
    /// A vertex has no skin weight, so the model's box joins a skin's bounds.
    pub(super) unweighted: bool,
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
    /// The instance ranges to draw and whether each one's front faces wind clockwise.
    pub(super) fn instance_runs(&self) -> impl Iterator<Item = (std::ops::Range<u32>, bool)> {
        let split = self.mirrored_instances_from;
        [
            (0..split, self.mirrored),
            (split..self.instance_count, !self.mirrored),
        ]
        .into_iter()
        .filter(|(range, _)| !range.is_empty())
    }

    /// Whether any draw of this primitive needs the clockwise-front pipelines.
    pub(super) fn needs_clockwise(&self) -> bool {
        self.instance_runs().any(|(_, clockwise)| clockwise)
    }

    /// Stores `transforms` (none: one identity), unmirrored first, and re-derives the bounds.
    /// Returns true when the recorded draws went stale: a new buffer, count or winding split.
    pub(super) fn write_instances(&mut self, gpu: &GpuContext, transforms: &[Mat4]) -> bool {
        let before = (self.instance_count, self.mirrored_instances_from);
        let placed = placed(self.root, transforms);
        let (upright, mirrored): (Vec<Mat4>, Vec<Mat4>) =
            placed.iter().copied().partition(|m| !is_mirrored(m));
        let mut raw: Vec<InstanceRaw> = upright
            .iter()
            .chain(&mirrored)
            .map(|m| InstanceRaw {
                model: m.to_cols_array_2d(),
            })
            .collect();
        if raw.is_empty() {
            raw.push(InstanceRaw::IDENTITY);
        }
        let bytes = bytemuck::cast_slice(&raw);
        let replaced = (bytes.len() as u64) > self.instance_buffer.size();
        if replaced {
            self.instance_buffer =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("instances"),
                        contents: bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
        } else {
            gpu.queue.write_buffer(&self.instance_buffer, 0, bytes);
        }
        self.instance_count = raw.len() as u32;
        self.mirrored_instances_from = self.instance_count - mirrored.len() as u32;
        // Culling bounds must span every instance; no instances collapse them to the posed box.
        self.instance_transforms = transforms.to_vec();
        let (min, max) = instanced_bounds(self.pre_instance_aabb, &placed);
        self.aabb_min = min;
        self.aabb_max = max;
        self.world_center = (min + max) * 0.5;
        replaced || before != (self.instance_count, self.mirrored_instances_from)
    }

    /// The instances as drawn: the user's transforms, each placed by the Model tab's root.
    pub(super) fn placed_instances(&self) -> Vec<Mat4> {
        placed(self.root, &self.instance_transforms)
    }

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

/// `transforms` placed by `root`; empty stays empty (one identity instance) only while the root is too.
fn placed(root: Mat4, transforms: &[Mat4]) -> Vec<Mat4> {
    if root == Mat4::IDENTITY {
        transforms.to_vec()
    } else if transforms.is_empty() {
        vec![root]
    } else {
        transforms.iter().map(|m| root * *m).collect()
    }
}

/// Pads a KHR_texture_transform's two affine rows to the [f32; 4] pair PrimUniforms packs.
fn uv_rows(t: [[f32; 3]; 2]) -> ([f32; 4], [f32; 4]) {
    (
        [t[0][0], t[0][1], t[0][2], 0.0],
        [t[1][0], t[1][1], t[1][2], 0.0],
    )
}
