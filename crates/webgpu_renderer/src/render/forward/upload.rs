//! `upload_scene`'s per-primitive work: buffers, material textures, bind groups and LOD chains.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt as _;

use super::primitive::{is_mirrored, GpuPrimitive, LodLevel, PrimMaterial, PrimUniforms};
use super::{ForwardRenderer, MAX_JOINTS, MAX_PUNCTUAL_LIGHTS};
use crate::context::GpuContext;
use crate::render::bounds::{
    compute_world_bounds, normal_matrix_of, primitive_local_aabb, primitive_world_aabb,
    widen_bounds_for_skin,
};
use crate::render::buffer_desc;
use crate::render::lights::pack_punctual_lights;
use crate::render::texture::{create_material_texture, create_sampler};
use crate::scene::{
    AlphaMode, CpuMaterial, CpuPrimitive, CpuSampler, CpuScene, CpuTexture, CpuTextureRef,
    InstanceRaw,
};

/// 1x1 stand-ins for the material slots a primitive leaves empty.
pub(super) struct MaterialDefaults {
    white: wgpu::TextureView,
    flat_normal: wgpu::TextureView,
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

/// One upload and mip build per (image `Arc` pointer, sRGB), not per referencing primitive.
#[derive(Default)]
pub(super) struct MaterialTextureCache {
    textures: HashMap<(usize, bool), wgpu::TextureView>,
    samplers: HashMap<CpuSampler, wgpu::Sampler>,
    /// GPU textures created so far; the cache hits do not count.
    pub(super) uploaded: usize,
}

/// The five material slots' views and samplers, in binding order.
struct MaterialSlots {
    views: Vec<wgpu::TextureView>,
    samplers: Vec<wgpu::Sampler>,
}

impl MaterialTextureCache {
    /// Base color, metallic-roughness, normal, emissive and occlusion, each falling back alone.
    fn material_slots(
        &mut self,
        gpu: &GpuContext,
        defaults: &MaterialDefaults,
        material: &CpuMaterial,
        i: usize,
    ) -> MaterialSlots {
        let slots = [
            (&material.base_color_texture, &defaults.white),
            (&material.metallic_roughness_texture, &defaults.white),
            (&material.normal_texture, &defaults.flat_normal),
            (&material.emissive_texture, &defaults.white),
            (&material.occlusion_texture, &defaults.white),
        ];
        let mut out = MaterialSlots {
            views: Vec::with_capacity(5),
            samplers: Vec::with_capacity(5),
        };
        for (slot_index, (texture_ref, fallback)) in slots.iter().enumerate() {
            match texture_ref {
                Some(tex_ref) => {
                    out.views
                        .push(self.texture_view(gpu, tex_ref, i, slot_index));
                    out.samplers
                        .push(self.sampler(&gpu.device, tex_ref.sampler));
                }
                None => {
                    out.views.push((*fallback).clone());
                    out.samplers
                        .push(self.sampler(&gpu.device, CpuSampler::default()));
                }
            }
        }
        out
    }

    fn texture_view(
        &mut self,
        gpu: &GpuContext,
        tex_ref: &CpuTextureRef,
        i: usize,
        slot_index: usize,
    ) -> wgpu::TextureView {
        let key = (
            std::sync::Arc::as_ptr(&tex_ref.texture) as usize,
            tex_ref.srgb,
        );
        if let Some(view) = self.textures.get(&key) {
            return view.clone();
        }
        let view = create_material_texture(
            gpu,
            &tex_ref.texture,
            tex_ref.srgb,
            Some(&format!("material_{i}_slot_{slot_index}")),
        );
        self.uploaded += 1;
        self.textures.insert(key, view.clone());
        view
    }

    fn sampler(&mut self, device: &wgpu::Device, desc: CpuSampler) -> wgpu::Sampler {
        self.samplers
            .entry(desc)
            .or_insert_with(|| create_sampler(device, &desc))
            .clone()
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
                bytemuck::cast_slice(&prim.vertices),
                vertex_usage,
            ),
            index: init_buffer(
                device,
                &format!("indices_{i}"),
                bytemuck::cast_slice(&prim.indices),
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
        self.culling.occlusion.reset();
        if let Some(gpu_cull) = self.culling.gpu_culling.as_mut() {
            gpu_cull.reset();
        }
        self.scene_bounds = compute_world_bounds(scene);
        let (packed, count) = pack_punctual_lights(&scene.lights);
        self.lights.punctual = packed;
        self.lights.punctual_count = count;
        self.animation.nodes = scene.nodes.clone();
        self.animation.animations = scene.animations.clone();
        self.animation.skins = scene.skins.clone();
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
        textures: &mut MaterialTextureCache,
    ) -> GpuPrimitive {
        let device = &gpu.device;
        let buffers = PrimitiveBuffers::new(device, prim, i);
        let local_bounds = primitive_local_aabb(prim);
        let prim_bounds = posed_world_bounds(scene, prim, local_bounds, node_world);
        let slots = textures.material_slots(gpu, &self.materials, &prim.material, i);
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
                lod,
                local_bounds,
                prim_bounds,
            },
        )
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
    fn create_material_bind_group(
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

    fn create_masked_shadow_bind_group(
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
        shadow_bind_group: parts.groups.shadow,
        alpha_masked: parts.groups.alpha_masked,
        shadow_masked_bind_group: parts.groups.shadow_masked,
        instance_buffer: parts.instance_buffer,
        instance_count: 1,
        mirrored_instances_from: 1,
        mirrored: is_mirrored(&prim.transform),
        instance_transforms: Vec::new(),
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
            local_bounds.0,
            local_bounds.1,
            skin,
            node_world,
        );
    }
    prim_bounds
}

/// Joint matrices: sized to the skin (identity when unskinned).
fn create_joint_buffer(
    device: &wgpu::Device,
    scene: &CpuScene,
    prim: &CpuPrimitive,
    i: usize,
) -> wgpu::Buffer {
    let joint_count = prim
        .skin_index
        .and_then(|s| scene.skins.get(s))
        .map(|s| s.joints.len().clamp(1, MAX_JOINTS))
        .unwrap_or(1);
    let identity = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count];
    init_buffer(
        device,
        &format!("joints_{i}"),
        bytemuck::cast_slice(&identity),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    )
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
