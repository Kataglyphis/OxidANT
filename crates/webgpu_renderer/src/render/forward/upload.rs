//! `upload_scene`'s per-primitive work: buffers, material textures, bind groups and LOD chains.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::animate::JOINT_MATRIX_BYTES;
use super::primitive::{is_mirrored, GpuPrimitive, LodLevel, PrimMaterial, PrimUniforms};
use super::streaming::MaterialTextures;
use super::{ForwardRenderer, MAX_PUNCTUAL_LIGHTS};
use crate::context::GpuContext;
use crate::render::bounds::{
    compute_world_bounds, has_unweighted_vertex, normal_matrix_of, primitive_local_aabb,
    primitive_world_aabb, widen_bounds_for_skin,
};
use crate::render::buffer_desc;
use crate::render::lights::pack_punctual_lights;
use crate::render::model_transform::ModelTransform;
use crate::render::texture::create_material_texture;
use crate::scene::{AlphaMode, CpuMaterial, CpuPrimitive, CpuScene, CpuTexture, InstanceRaw};

/// 1x1 stand-ins for the material slots a primitive leaves empty.
pub(super) struct MaterialDefaults {
    pub(super) white: wgpu::TextureView,
    pub(super) flat_normal: wgpu::TextureView,
}

impl MaterialDefaults {
    pub(super) fn new(gpu: &GpuContext) -> Self {
        let solid = |rgba8: [u8; 4], label: &str| {
            create_material_texture(
                gpu,
                &CpuTexture {
                    width: 1,
                    height: 1,
                    rgba8: rgba8.to_vec(),
                    compressed: None,
                },
                false,
                Some(label),
            )
        };
        Self {
            white: solid([255, 255, 255, 255], "white_fallback"),
            // Flat tangent-space normal (0, 0, 1) encoded as RGBA8.
            flat_normal: solid([128, 128, 255, 255], "flat_normal_fallback"),
        }
    }
}

/// The five material slots' views and samplers, in binding order.
pub(super) struct MaterialSlots {
    views: Vec<wgpu::TextureView>,
    samplers: Vec<wgpu::Sampler>,
}

impl MaterialSlots {
    pub(super) fn with_capacity(slots: usize) -> Self {
        Self {
            views: Vec::with_capacity(slots),
            samplers: Vec::with_capacity(slots),
        }
    }

    pub(super) fn push(&mut self, view: wgpu::TextureView, sampler: wgpu::Sampler) {
        self.views.push(view);
        self.samplers.push(sampler);
    }

    /// A streamed texture's new view; the bind groups that bound the old one need rebuilding.
    pub(super) fn set_view(&mut self, slot: usize, view: wgpu::TextureView) {
        self.views[slot] = view;
    }
}

/// The vertex, index and uniform buffers every primitive owns.
struct PrimitiveBuffers {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    uniform: wgpu::Buffer,
}

impl PrimitiveBuffers {
    fn new(device: &wgpu::Device, prim: &CpuPrimitive, i: usize) -> Self {
        // Morphed vertex buffers are re-uploaded per frame, so they need COPY_DST.
        let has_morph = !prim.morph_targets.is_empty();
        let vertex_usage = if has_morph {
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
        } else {
            wgpu::BufferUsages::VERTEX
        };
        Self {
            vertex: init_buffer(
                device,
                &format!("vertices_{i}"),
                &non_empty_contents(&prim.vertices),
                vertex_usage,
            ),
            index: init_buffer(
                device,
                &format!("indices_{i}"),
                &non_empty_contents(&prim.indices),
                wgpu::BufferUsages::INDEX,
            ),
            uniform: buffer_desc::uniform(
                device,
                &format!("uniforms_{i}"),
                std::mem::size_of::<PrimUniforms>() as wgpu::BufferAddress,
            ),
        }
    }
}

/// The forward, shadow and masked-shadow bind groups of one primitive.
struct PrimitiveBindGroups {
    main: wgpu::BindGroup,
    shadow: wgpu::BindGroup,
    alpha_masked: bool,
    shadow_masked: Option<wgpu::BindGroup>,
}

/// Everything `upload_primitive` created, for `assemble_primitive` to move into place.
struct PrimitiveParts {
    buffers: PrimitiveBuffers,
    joint_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    groups: PrimitiveBindGroups,
    slots: MaterialSlots,
    lod: (Vec<LodLevel>, Vec<f32>),
    local_bounds: (Vec3, Vec3),
    prim_bounds: (Vec3, Vec3),
}

impl ForwardRenderer {
    /// Forgets the previous scene and takes over `scene`'s bounds, lights and animation data.
    pub(super) fn reset_scene_state(&mut self, scene: &CpuScene) {
        self.primitives.clear();
        // A cached bundle references buffers that are about to be dropped.
        self.shadow.caster_bundles = None;
        // Visibility is per primitive index, so a new scene must not inherit stale culls.
        self.culling.reset_occlusion();
        self.scene_bounds = compute_world_bounds(scene);
        // A load places the scene at the origin, as the C++ zeroes the inspector's transform.
        self.model_transform = ModelTransform::default();
        self.placed_root = Mat4::IDENTITY;
        let (packed, count) = pack_punctual_lights(&scene.lights);
        self.lights.punctual = packed;
        self.lights.punctual_count = count;
        // Also drops a pose sampled for the previous scene, which would land on this scene's joints.
        self.animation = super::animate::SceneAnimation::for_scene(scene);
        if scene.lights.len() > MAX_PUNCTUAL_LIGHTS {
            log::warn!(
                "Scene has {} punctual lights; only the first {} are used.",
                scene.lights.len(),
                MAX_PUNCTUAL_LIGHTS
            );
        }
    }

    /// One CPU primitive on the GPU: buffers, material slots, bind groups and LOD chain.
    pub(super) fn upload_primitive(
        &self,
        gpu: &GpuContext,
        scene: &CpuScene,
        i: usize,
        prim: &CpuPrimitive,
        node_world: &[Mat4],
        textures: &mut MaterialTextures,
    ) -> GpuPrimitive {
        let device = &gpu.device;
        let buffers = PrimitiveBuffers::new(device, prim, i);
        let local_bounds = primitive_local_aabb(prim);
        let prim_bounds = posed_world_bounds(scene, prim, local_bounds, node_world);
        let slots =
            textures.material_slots(gpu, &self.materials, (prim, i), &self.texture_streaming);
        let joint_buffer = create_joint_buffer(device, scene, prim, i);
        let groups = self.create_primitive_bind_groups(
            device,
            i,
            &prim.material,
            &buffers.uniform,
            &joint_buffer,
            &slots,
        );
        let instance_buffer = init_buffer(
            device,
            &format!("instances_{i}"),
            bytemuck::bytes_of(&InstanceRaw::IDENTITY),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let lod = self.upload_lod_chain(device, prim, i);
        assemble_primitive(
            prim,
            PrimitiveParts {
                buffers,
                joint_buffer,
                instance_buffer,
                groups,
                slots,
                lod,
                local_bounds,
                prim_bounds,
            },
        )
    }

    /// Every primitive's group 0 again, around the shadow map `sync_shadow_map` just reallocated.
    pub(super) fn rebuild_material_bind_groups(&mut self, device: &wgpu::Device) {
        let groups: Vec<wgpu::BindGroup> = self
            .primitives
            .iter()
            .enumerate()
            .map(|(i, prim)| {
                self.create_material_bind_group(
                    device,
                    i,
                    &prim.uniform_buffer,
                    &prim.joint_buffer,
                    &prim.material_slots,
                )
            })
            .collect();
        for (prim, group) in self.primitives.iter_mut().zip(groups) {
            prim.bind_group = group;
        }
    }

    /// Forward group 0, the shadow group and, for MASK casters, the alpha-testing one.
    fn create_primitive_bind_groups(
        &self,
        device: &wgpu::Device,
        i: usize,
        material: &CpuMaterial,
        uniform_buffer: &wgpu::Buffer,
        joint_buffer: &wgpu::Buffer,
        slots: &MaterialSlots,
    ) -> PrimitiveBindGroups {
        let main = self.create_material_bind_group(device, i, uniform_buffer, joint_buffer, slots);
        let shadow = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("shadow_bind_group_{i}")),
            layout: &self.shadow.bind_group_layout,
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
        // Every MASK caster alpha-tests with slot 0's view, white when untextured: vertex alpha cuts too.
        let alpha_masked = matches!(material.alpha_mode, AlphaMode::Mask(_));
        let shadow_masked = if alpha_masked {
            Some(self.create_masked_shadow_bind_group(
                device,
                i,
                uniform_buffer,
                joint_buffer,
                slots,
            ))
        } else {
            None
        };
        PrimitiveBindGroups {
            main,
            shadow,
            alpha_masked,
            shadow_masked,
        }
    }

    /// Uniforms, the shadow map and its comparison sampler, five material slots and the joints.
    pub(super) fn create_material_bind_group(
        &self,
        device: &wgpu::Device,
        i: usize,
        uniform_buffer: &wgpu::Buffer,
        joint_buffer: &wgpu::Buffer,
        slots: &MaterialSlots,
    ) -> wgpu::BindGroup {
        let mut bind_entries: Vec<wgpu::BindGroupEntry> = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&self.shadow.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&self.shadow.sampler),
            },
        ];
        for slot in 0..5usize {
            bind_entries.push(wgpu::BindGroupEntry {
                binding: 3 + slot as u32 * 2,
                resource: wgpu::BindingResource::TextureView(&slots.views[slot]),
            });
            bind_entries.push(wgpu::BindGroupEntry {
                binding: 4 + slot as u32 * 2,
                resource: wgpu::BindingResource::Sampler(&slots.samplers[slot]),
            });
        }

        bind_entries.push(wgpu::BindGroupEntry {
            binding: 13,
            resource: joint_buffer.as_entire_binding(),
        });

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("bind_group_{i}")),
            layout: &self.main.material_layout,
            entries: &bind_entries,
        })
    }

    pub(super) fn create_masked_shadow_bind_group(
        &self,
        device: &wgpu::Device,
        i: usize,
        uniform_buffer: &wgpu::Buffer,
        joint_buffer: &wgpu::Buffer,
        slots: &MaterialSlots,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("shadow_masked_bind_group_{i}")),
            layout: &self.shadow.masked_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&slots.views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&slots.samplers[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: joint_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Simplified levels and their switch distances; both empty while LOD is off.
    fn upload_lod_chain(
        &self,
        device: &wgpu::Device,
        prim: &CpuPrimitive,
        i: usize,
    ) -> (Vec<LodLevel>, Vec<f32>) {
        // Morphed primitives skip LOD: only `vertex_buffer` is re-blended, so levels would pop.
        if !self.lod_enabled || !prim.morph_targets.is_empty() {
            return (Vec::new(), Vec::new());
        }
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
                vertex_buffer: init_buffer(
                    device,
                    &format!("vertices_{i}_lod_{level}"),
                    bytemuck::cast_slice(&lod.primitive.vertices),
                    wgpu::BufferUsages::VERTEX,
                ),
                index_buffer: init_buffer(
                    device,
                    &format!("indices_{i}_lod_{level}"),
                    bytemuck::cast_slice(&lod.primitive.indices),
                    wgpu::BufferUsages::INDEX,
                ),
                index_count: lod.primitive.indices.len() as u32,
            });
            distances.push(lod.min_distance);
        }
        (levels, distances)
    }
}

/// Moves the uploaded parts into a `GpuPrimitive` and derives its draw state from the material.
fn assemble_primitive(prim: &CpuPrimitive, parts: PrimitiveParts) -> GpuPrimitive {
    let material = &prim.material;
    let has_morph = !prim.morph_targets.is_empty();
    let (min, max) = parts.prim_bounds;
    GpuPrimitive {
        vertex_buffer: parts.buffers.vertex,
        index_buffer: parts.buffers.index,
        index_count: prim.indices.len() as u32,
        uniform_buffer: parts.buffers.uniform,
        bind_group: parts.groups.main,
        material_slots: parts.slots,
        shadow_bind_group: parts.groups.shadow,
        alpha_masked: parts.groups.alpha_masked,
        shadow_masked_bind_group: parts.groups.shadow_masked,
        instance_buffer: parts.instance_buffer,
        instance_count: 1,
        mirrored_instances_from: 1,
        mirrored: is_mirrored(&prim.transform),
        instance_transforms: Vec::new(),
        root: Mat4::IDENTITY,
        pre_instance_aabb: parts.prim_bounds,
        model: prim.transform,
        normal_matrix: normal_matrix_of(prim.transform),
        material: PrimMaterial::from_cpu(material),
        double_sided: material.double_sided,
        alpha_blend: material.alpha_mode == AlphaMode::Blend,
        // Blend casts no shadow; a MASK whose base alpha is below the cutoff is invisible.
        casts_shadow: match material.alpha_mode {
            AlphaMode::Blend => false,
            AlphaMode::Mask(cutoff) => material.base_color[3] >= cutoff,
            AlphaMode::Opaque => true,
        },
        // AABB centre, not vertex centroid: `world_center` must stay one metric everywhere.
        world_center: (min + max) * 0.5,
        aabb_min: min,
        aabb_max: max,
        node_index: prim.node_index,
        skin_index: prim.skin_index,
        unweighted: has_unweighted_vertex(&prim.vertices),
        joint_buffer: parts.joint_buffer,
        local_aabb_min: parts.local_bounds.0,
        local_aabb_max: parts.local_bounds.1,
        lod_levels: parts.lod.0,
        lod_min_distances: parts.lod.1,
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
    }
}

/// World bounds at upload, widened over the skin's joints when it has one.
fn posed_world_bounds(
    scene: &CpuScene,
    prim: &CpuPrimitive,
    local_bounds: (Vec3, Vec3),
    node_world: &[Mat4],
) -> (Vec3, Vec3) {
    // Widen now: joints may start posed, and set_animation_time bails without animations.
    let mut prim_bounds = primitive_world_aabb(prim);
    if let Some(skin) = prim.skin_index.and_then(|s| scene.skins.get(s)) {
        prim_bounds = widen_bounds_for_skin(
            prim_bounds,
            has_unweighted_vertex(&prim.vertices),
            local_bounds,
            skin,
            node_world,
        );
    }
    prim_bounds
}

/// Joint matrices: sized to the skin (identity when unskinned), within the device's binding limit.
fn create_joint_buffer(
    device: &wgpu::Device,
    scene: &CpuScene,
    prim: &CpuPrimitive,
    i: usize,
) -> wgpu::Buffer {
    let skin_joints = prim
        .skin_index
        .and_then(|s| scene.skins.get(s))
        .map_or(1, |s| s.joints.len());
    let max_joints =
        (device.limits().max_storage_buffer_binding_size / JOINT_MATRIX_BYTES) as usize;
    if skin_joints > max_joints {
        log::warn!(
            "primitive {i}: its skin has {skin_joints} joints; the device binds {max_joints}"
        );
    }
    let joint_count = skin_joints.clamp(1, max_joints);
    let identity = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count];
    init_buffer(
        device,
        &format!("joints_{i}"),
        bytemuck::cast_slice(&identity),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    )
}

/// `items` as bytes, or one zeroed item: wgpu panics slicing an empty buffer, and draws use the real count.
fn non_empty_contents<T: bytemuck::Pod>(items: &[T]) -> std::borrow::Cow<'_, [u8]> {
    if items.is_empty() {
        std::borrow::Cow::Owned(vec![0; std::mem::size_of::<T>()])
    } else {
        std::borrow::Cow::Borrowed(bytemuck::cast_slice(items))
    }
}

fn init_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}
