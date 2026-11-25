//! The tracer's material table: one record per distinct forward material, its textures as layers of two arrays.

use std::collections::HashMap;
use std::sync::Arc;

use crate::scene::{
    AlphaMode, CpuMaterial, CpuSampler, CpuScene, CpuTexture, CpuTextureRef, CpuWrap,
};

/// A slot without a texture samples its factor alone, as the forward pass's white or flat-normal fallback.
pub const NO_LAYER: u32 = 0xFFFF;
/// The material slots in forward order: base colour, metallic-roughness, normal, emissive, occlusion.
pub const SLOT_COUNT: usize = 5;
/// Slots whose texels are sRGB, so they live in the sRGB array; the rest are data.
pub const SRGB_SLOTS: [bool; SLOT_COUNT] = [true, false, false, true, false];

/// Bits of [`GpuMaterial::flags`]; `bvh_traverse.slang` decodes the same layout.
pub mod flags {
    pub const ALPHA_MODE_MASK: u32 = 0b11;
    pub use kataglyphis_render_core::shader_limits::{ALPHA_BLEND, ALPHA_MASK, ALPHA_OPAQUE};
    pub const DOUBLE_SIDED: u32 = 1 << 2;
    pub const UNLIT: u32 = 1 << 3;
    /// Slot `i` reads TEXCOORD_1 when bit `UV1_SHIFT + i` is set.
    pub const UV1_SHIFT: u32 = 4;
    /// Slot `i`'s wrap codes: U at bit `WRAP_SHIFT + 4 i`, V two bits above.
    pub const WRAP_SHIFT: u32 = 9;
    pub const WRAP_REPEAT: u32 = 0;
    pub const WRAP_MIRROR: u32 = 1;
    pub const WRAP_CLAMP: u32 = 2;
}

/// One material as the traversal and the shading kernels read it: 14 float4 records.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    pub base_color: [f32; 4],
    /// rgb emissive (strength folded in), a the MASK cutoff or -1 for OPAQUE and BLEND.
    pub emissive_cutoff: [f32; 4],
    /// Metallic, roughness, occlusion strength, normal scale.
    pub factors: [f32; 4],
    pub flags: u32,
    /// Layers of base | metallic-roughness << 16, normal | emissive << 16, occlusion | nearest-filter mask << 16.
    pub layers: [u32; 3],
    /// Two KHR_texture_transform rows (xyz) per slot.
    pub uv_rows: [[f32; 4]; 2 * SLOT_COUNT],
}

impl GpuMaterial {
    /// Float4 records one material takes.
    pub const RECORDS: usize = std::mem::size_of::<Self>() / 16;

    pub fn alpha_mode(&self) -> u32 {
        self.flags & flags::ALPHA_MODE_MASK
    }

    /// The layer of `slot`, or [`NO_LAYER`].
    pub fn layer(&self, slot: usize) -> u32 {
        let word = self.layers[slot / 2];
        if slot.is_multiple_of(2) {
            word & 0xFFFF
        } else {
            word >> 16
        }
    }

    pub fn nearest(&self, slot: usize) -> bool {
        (self.layers[2] >> (16 + slot)) & 1 != 0
    }

    /// Slot `slot`'s UV after its set selection and its texture transform.
    pub fn slot_uv(&self, slot: usize, uv0: [f32; 2], uv1: [f32; 2]) -> [f32; 2] {
        let uv = if (self.flags >> (flags::UV1_SHIFT + slot as u32)) & 1 != 0 {
            uv1
        } else {
            uv0
        };
        let [r0, r1] = [self.uv_rows[2 * slot], self.uv_rows[2 * slot + 1]];
        [
            r0[0] * uv[0] + r0[1] * uv[1] + r0[2],
            r1[0] * uv[0] + r1[1] * uv[1] + r1[2],
        ]
    }

    /// Slot `slot`'s wrap codes for U and V.
    pub fn wrap(&self, slot: usize) -> [u32; 2] {
        let shift = flags::WRAP_SHIFT + 4 * slot as u32;
        [(self.flags >> shift) & 3, (self.flags >> (shift + 2)) & 3]
    }
}

/// Every distinct material of a scene, the layer each texture takes and the record each primitive uses.
#[derive(Clone, Debug, Default)]
pub struct MaterialTable {
    pub records: Vec<GpuMaterial>,
    /// Index into `records`, per scene primitive.
    pub primitive_material: Vec<u32>,
    pub srgb_layers: Vec<Arc<CpuTexture>>,
    pub linear_layers: Vec<Arc<CpuTexture>>,
    /// Textures left out: past the layer limit, or ETC2/ASTC, which only the GPU decodes.
    pub dropped_textures: usize,
}

/// Assigns each distinct image a layer in its array, keyed by the image's `Arc`.
struct LayerAssigner {
    max_layers: usize,
    layers: [Vec<Arc<CpuTexture>>; 2],
    index: HashMap<(usize, bool), u32>,
    dropped: usize,
}

impl LayerAssigner {
    fn layer(&mut self, texture: &Arc<CpuTexture>, srgb: bool) -> u32 {
        if !super::layers::traceable(texture) {
            self.dropped += 1;
            return NO_LAYER;
        }
        let key = (Arc::as_ptr(texture) as usize, srgb);
        if let Some(&layer) = self.index.get(&key) {
            return layer;
        }
        let layers = &mut self.layers[usize::from(srgb)];
        if layers.len() >= self.max_layers {
            self.dropped += 1;
            return NO_LAYER;
        }
        let layer = layers.len() as u32;
        layers.push(Arc::clone(texture));
        self.index.insert(key, layer);
        layer
    }
}

impl MaterialTable {
    /// Builds the table for `scene`; each array holds at most `max_layers` images.
    pub fn build(scene: &CpuScene, max_layers: usize) -> Self {
        let mut assigner = LayerAssigner {
            max_layers: max_layers.min(NO_LAYER as usize),
            layers: [Vec::new(), Vec::new()],
            index: HashMap::new(),
            dropped: 0,
        };
        let mut records = Vec::new();
        let mut known: HashMap<Vec<u8>, u32> = HashMap::new();
        let mut primitive_material = Vec::with_capacity(scene.primitives.len());
        for prim in &scene.primitives {
            let record = gpu_material(&prim.material, &mut assigner);
            let index = *known
                .entry(bytemuck::bytes_of(&record).to_vec())
                .or_insert_with(|| {
                    records.push(record);
                    records.len() as u32 - 1
                });
            primitive_material.push(index);
        }
        if assigner.dropped > 0 {
            log::warn!(
                "tracer: {} material texture reference(s) render with their factor alone \
                 (ETC2 or ASTC, or past {max_layers} layers per array)",
                assigner.dropped
            );
        }
        let [linear_layers, srgb_layers] = assigner.layers;
        Self {
            records,
            primitive_material,
            srgb_layers,
            linear_layers,
            dropped_textures: assigner.dropped,
        }
    }

    /// True when record `material` alpha-tests (MASK).
    pub fn masked(&self, material: u32) -> bool {
        self.records[material as usize].alpha_mode() == flags::ALPHA_MASK
    }
}

fn wrap_code(wrap: CpuWrap) -> u32 {
    match wrap {
        CpuWrap::Repeat => flags::WRAP_REPEAT,
        CpuWrap::MirroredRepeat => flags::WRAP_MIRROR,
        CpuWrap::ClampToEdge => flags::WRAP_CLAMP,
    }
}

fn row(r: [f32; 3]) -> [f32; 4] {
    [r[0], r[1], r[2], 0.0]
}

/// One forward material as its tracer record, taking layers for its textures.
fn gpu_material(material: &CpuMaterial, assigner: &mut LayerAssigner) -> GpuMaterial {
    let slots: [&Option<CpuTextureRef>; SLOT_COUNT] = [
        &material.base_color_texture,
        &material.metallic_roughness_texture,
        &material.normal_texture,
        &material.emissive_texture,
        &material.occlusion_texture,
    ];
    let transforms = [
        material.base_uv_transform,
        material.mr_uv_transform,
        material.normal_uv_transform,
        material.emissive_uv_transform,
        material.occlusion_uv_transform,
    ];
    let mut layer = [NO_LAYER; SLOT_COUNT];
    let mut nearest = 0u32;
    let mut flags = alpha_flags(material) | ((material.uv_set_mask & 0x1F) << flags::UV1_SHIFT);
    let mut uv_rows = [[0.0; 4]; 2 * SLOT_COUNT];
    for (slot, texture) in slots.iter().enumerate() {
        let sampler = texture
            .as_ref()
            .map_or(CpuSampler::default(), |t| t.sampler);
        if let Some(texture) = texture {
            layer[slot] = assigner.layer(&texture.texture, SRGB_SLOTS[slot]);
        }
        let shift = flags::WRAP_SHIFT + 4 * slot as u32;
        flags |= wrap_code(sampler.wrap_u) << shift | wrap_code(sampler.wrap_v) << (shift + 2);
        nearest |= u32::from(sampler.mag_nearest) << slot;
        uv_rows[2 * slot] = row(transforms[slot][0]);
        uv_rows[2 * slot + 1] = row(transforms[slot][1]);
    }
    let cutoff = match material.alpha_mode {
        AlphaMode::Mask(cutoff) => cutoff,
        AlphaMode::Opaque | AlphaMode::Blend => -1.0,
    };
    let [er, eg, eb] = material.emissive_factor;
    GpuMaterial {
        base_color: material.base_color,
        emissive_cutoff: [er, eg, eb, cutoff],
        factors: [
            material.metallic_factor,
            material.roughness_factor,
            material.occlusion_strength,
            material.normal_scale,
        ],
        flags,
        layers: [
            layer[0] | layer[1] << 16,
            layer[2] | layer[3] << 16,
            layer[4] | nearest << 16,
        ],
        uv_rows,
    }
}

/// `shaders/common/alpha_mode.slang`'s code for `mode`, which both passes hand `alpha_masked_out`.
pub(crate) fn alpha_mode_code(mode: AlphaMode) -> u32 {
    match mode {
        AlphaMode::Opaque => flags::ALPHA_OPAQUE,
        AlphaMode::Mask(_) => flags::ALPHA_MASK,
        AlphaMode::Blend => flags::ALPHA_BLEND,
    }
}

fn alpha_flags(material: &CpuMaterial) -> u32 {
    alpha_mode_code(material.alpha_mode)
        | if material.double_sided {
            flags::DOUBLE_SIDED
        } else {
            0
        }
        | if material.unlit { flags::UNLIT } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CpuPrimitive, CpuTexture};
    use glam::Mat4;

    fn texture(width: u32) -> Arc<CpuTexture> {
        Arc::new(CpuTexture {
            width,
            height: width,
            rgba8: vec![255; (width * width * 4) as usize],
            compressed: None,
        })
    }

    fn primitive(material: CpuMaterial) -> CpuPrimitive {
        CpuPrimitive {
            vertices: Vec::new(),
            indices: Vec::new(),
            transform: Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material,
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }
    }

    fn reference(texture: &Arc<CpuTexture>, srgb: bool) -> Option<CpuTextureRef> {
        Some(CpuTextureRef {
            texture: Arc::clone(texture),
            sampler: CpuSampler {
                wrap_u: CpuWrap::ClampToEdge,
                wrap_v: CpuWrap::MirroredRepeat,
                mag_nearest: true,
                ..CpuSampler::default()
            },
            srgb,
        })
    }

    #[test]
    fn equal_materials_share_a_record_and_shared_images_share_a_layer() {
        let albedo = texture(4);
        let data = texture(2);
        let textured = CpuMaterial {
            base_color_texture: reference(&albedo, true),
            metallic_roughness_texture: reference(&data, false),
            occlusion_texture: reference(&data, false),
            emissive_texture: reference(&albedo, true),
            metallic_factor: 0.75,
            alpha_mode: AlphaMode::Mask(0.4),
            uv_set_mask: 0b10000,
            ..CpuMaterial::default()
        };
        let scene = CpuScene {
            primitives: vec![
                primitive(textured.clone()),
                primitive(CpuMaterial::default()),
                primitive(textured),
            ],
            ..CpuScene::default()
        };
        let table = MaterialTable::build(&scene, 256);
        assert_eq!(table.primitive_material, [0, 1, 0]);
        assert_eq!((table.srgb_layers.len(), table.linear_layers.len()), (1, 1));
        let record = table.records[0];
        assert_eq!(
            (0..SLOT_COUNT).map(|s| record.layer(s)).collect::<Vec<_>>(),
            [0, 0, NO_LAYER, 0, 0]
        );
        assert!(table.masked(0) && !table.masked(1));
        assert_eq!(record.emissive_cutoff[3], 0.4);
        assert_eq!(
            record.factors[0], 0.75,
            "the untextured metallic factor is kept"
        );
        assert_eq!(record.wrap(0), [flags::WRAP_CLAMP, flags::WRAP_MIRROR]);
        assert_eq!(record.wrap(2), [flags::WRAP_REPEAT, flags::WRAP_REPEAT]);
        assert!(record.nearest(0) && !record.nearest(2));
        assert_eq!(record.slot_uv(4, [0.1, 0.2], [0.7, 0.8]), [0.7, 0.8]);
        assert_eq!(record.slot_uv(0, [0.1, 0.2], [0.7, 0.8]), [0.1, 0.2]);
        assert_eq!(table.records[1].emissive_cutoff[3], -1.0);
    }

    #[test]
    fn textures_past_the_layer_limit_fall_back_to_the_factor() {
        let materials: Vec<CpuPrimitive> = (0..3)
            .map(|i| {
                primitive(CpuMaterial {
                    base_color_texture: reference(&texture(i + 1), true),
                    ..CpuMaterial::default()
                })
            })
            .collect();
        let scene = CpuScene {
            primitives: materials,
            ..CpuScene::default()
        };
        let table = MaterialTable::build(&scene, 2);
        assert_eq!(table.srgb_layers.len(), 2);
        assert_eq!(table.records[2].layer(0), NO_LAYER);
        assert_eq!(table.dropped_textures, 1);
    }

    #[test]
    fn a_material_is_fourteen_float4_records() {
        assert_eq!(GpuMaterial::RECORDS, 14);
        assert_eq!(std::mem::size_of::<GpuMaterial>() % 16, 0);
    }
}
