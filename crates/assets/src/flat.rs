//! A glTF (or, through `obj::load_obj_flat`, an OBJ) flattened into the arrays BeschleunigerBallett's C++ engine draws: world-space vertices in one list,
//! a material id per triangle, a range per primitive, and each texture's image still encoded.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use glam::{Mat3, Mat4, Vec3};

use crate::gltf_images::{self, TextureSource};
use crate::gltf_loader::{self, Geometry, Imported, LoadOptions};
use crate::scene::{self, AlphaMode, CpuLight, CpuLightKind, CpuMaterial, CpuSampler, CpuScene};
use crate::skin;

/// One vertex in the C++ `Vertex`'s field order, in world space.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct FlatVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv0: [f32; 2],
    /// xyz: tangent, w: bitangent handedness, flipped with a mirroring node.
    pub tangent: [f32; 4],
    pub uv1: [f32; 2],
}

/// The top two rows of a KHR_texture_transform T*R*S matrix.
pub type UvRows = [[f32; 3]; 2];

/// A glTF material as the C++ `ObjMaterial` reads it; a texture id indexes `FlatModel::textures`, -1 for none.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatMaterial {
    pub diffuse: [f32; 3],
    /// baseColorFactor's alpha.
    pub dissolve: f32,
    /// emissiveFactor times KHR_materials_emissive_strength.
    pub emission: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    /// MASK's cutoff, -1 for OPAQUE and BLEND.
    pub alpha_cutoff: f32,
    pub normal_scale: f32,
    pub base_uv: UvRows,
    pub metallic_roughness_uv: UvRows,
    pub normal_uv: UvRows,
    pub emissive_uv: UvRows,
    pub base_texture: i32,
    pub metallic_roughness_texture: i32,
    pub normal_texture: i32,
    pub emissive_texture: i32,
    /// An OBJ `map_d` cut-out mask, sampled `.r` at the base colour's UV; glTF has no such slot.
    pub alpha_texture: i32,
    /// Slots sampling TEXCOORD_1, one bit each: 0 base colour, 1 metallic-roughness, 2 normal, 3 emissive.
    pub uv1_slot_mask: u32,
    pub unlit: bool,
    pub alpha_blend: bool,
    pub double_sided: bool,
}

/// One texture slot: an image's bytes as the file stores them, with its sampler and colour space.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatTexture {
    pub encoded: Vec<u8>,
    pub sampler: CpuSampler,
    pub srgb: bool,
}

/// One primitive's slice of the flat arrays.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FlatRange {
    pub vertex_base: usize,
    pub vertex_count: usize,
    pub index_start: usize,
    pub index_count: usize,
    pub tri_start: usize,
    pub tri_count: usize,
    pub double_sided: bool,
    pub alpha_blended: bool,
}

/// A KHR_lights_punctual light in world space; `kind` is 1 point, 2 spot, 3 directional, as the shaders count.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlatLight {
    pub kind: u32,
    pub color: [f32; 3],
    pub intensity: f32,
    /// 0 for unbounded.
    pub range: f32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    /// 1 for every kind but spot.
    pub cos_inner: f32,
    pub cos_outer: f32,
}

/// The whole file: the last material is the neutral one a primitive without a material draws with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatModel {
    pub vertices: Vec<FlatVertex>,
    pub indices: Vec<u32>,
    /// One per triangle.
    pub material_index: Vec<u32>,
    pub materials: Vec<FlatMaterial>,
    pub textures: Vec<FlatTexture>,
    pub ranges: Vec<FlatRange>,
    /// Every punctual light but the Sun.
    pub lights: Vec<FlatLight>,
    /// The first directional light, which the engine adopts as its Sun: [`crate::CpuScene::take_sun`]'s rule.
    pub sun: Option<FlatLight>,
}

/// Loads `path` through the shared loader with `options` and flattens it; an error when nothing draws.
pub fn load_flat(path: impl AsRef<Path>, options: &LoadOptions) -> anyhow::Result<FlatModel> {
    let path = path.as_ref();
    let imported = gltf_loader::import_document(path)
        .with_context(|| format!("Failed to import glTF file: {}", path.display()))?;
    let mut textures = Vec::new();
    let materials = read_materials(&imported, &mut textures);
    let mut model = FlatModel {
        materials,
        textures,
        ..FlatModel::default()
    };
    let source = Source {
        imported: &imported,
        options,
        joints: rest_joint_matrices(&imported),
    };
    let mut lights = Vec::new();
    gltf_loader::walk_nodes(&imported.document, |node, world| {
        visit_node(&mut model, &mut lights, &source, node, world)
    })?;
    anyhow::ensure!(
        !model.vertices.is_empty(),
        "glTF file contains no triangle primitives"
    );
    model.sun = scene::take_sun(&mut lights).map(|sun| flat_light(&sun));
    model.lights = lights.iter().map(flat_light).collect();
    Ok(model)
}

/// What every node reads: the file, the load options and each skin's joint matrices at rest.
struct Source<'a> {
    imported: &'a Imported,
    options: &'a LoadOptions,
    joints: Vec<Vec<Mat4>>,
}

/// Each skin's joint matrices at the file's pose: the engine draws no animation, so its skins keep that pose.
fn rest_joint_matrices(imported: &Imported) -> Vec<Vec<Mat4>> {
    let world = CpuScene::compute_world_transforms(&gltf_loader::read_nodes(&imported.document));
    gltf_loader::read_skins(&imported.document, &imported.buffers)
        .iter()
        .map(|skin| skin::joint_matrices(skin, &world))
        .collect()
}

/// Every material in document order, then the neutral fallback; textures get their slots on the way.
fn read_materials(imported: &Imported, textures: &mut Vec<FlatTexture>) -> Vec<FlatMaterial> {
    let mut slots = Slots {
        imported,
        keys: HashMap::new(),
        textures,
    };
    let mut materials: Vec<FlatMaterial> = imported
        .document
        .materials()
        .map(|material| {
            let ids = slots.material_slots(&material);
            flat_material(&gltf_loader::load_material(&material, &[]), ids)
        })
        .collect();
    materials.push(flat_material(&CpuMaterial::default(), [-1; 4]));
    materials
}

/// Dedups slots on (image, sampler settings, colour space): an image sampled two ways gets one slot per way.
struct Slots<'a> {
    imported: &'a Imported,
    /// The settings, not the sampler's index: two equal sampler objects, or none and a default one, share a slot.
    keys: HashMap<(usize, CpuSampler, bool), i32>,
    textures: &'a mut Vec<FlatTexture>,
}

impl Slots<'_> {
    /// Base colour, metallic-roughness, normal and emissive, in the C++ material's order.
    fn material_slots(&mut self, material: &gltf::Material) -> [i32; 4] {
        let pbr = material.pbr_metallic_roughness();
        [
            self.slot(pbr.base_color_texture().map(|i| i.texture()), true),
            self.slot(pbr.metallic_roughness_texture().map(|i| i.texture()), false),
            self.slot(material.normal_texture().map(|i| i.texture()), false),
            self.slot(material.emissive_texture().map(|i| i.texture()), true),
        ]
    }

    /// The core `source` only: the C++ engine decodes with stb, which reads no KTX2.
    fn slot(&mut self, texture: Option<gltf::Texture>, srgb: bool) -> i32 {
        let Some(texture) = texture else {
            return -1;
        };
        let source = self.imported.sources.get(texture.index());
        let Some(&TextureSource {
            fallback: Some(image),
            ..
        }) = source
        else {
            return -1;
        };
        let sampler = gltf_loader::to_cpu_sampler(&texture.sampler());
        let key = (image, sampler, srgb);
        if let Some(&slot) = self.keys.get(&key) {
            return slot;
        }
        let Some(encoded) = self.encoded(image) else {
            return -1;
        };
        let slot = self.textures.len() as i32;
        self.keys.insert(key, slot);
        self.textures.push(FlatTexture {
            encoded,
            sampler,
            srgb,
        });
        slot
    }

    /// An image's stored bytes; `None`, with a warning, when they cannot be read, as the C++ loader skips them.
    fn encoded(&self, image: usize) -> Option<Vec<u8>> {
        let imported = self.imported;
        let image = imported.document.images().nth(image)?;
        match gltf_images::encoded_bytes(&image, &imported.base, &imported.buffers) {
            Ok(bytes) if !bytes.is_empty() => Some(bytes),
            Ok(_) => None,
            Err(err) => {
                log::warn!("glTF image {}: {err:#}", image.index());
                None
            }
        }
    }
}

/// The C++ `ObjMaterial` fields of a loaded material; factors clamp to [0, 1] as the C++ loader's do.
pub(crate) fn flat_material(material: &CpuMaterial, ids: [i32; 4]) -> FlatMaterial {
    let [r, g, b, a] = material.base_color;
    FlatMaterial {
        diffuse: [r, g, b],
        dissolve: a,
        emission: material.emissive_factor,
        metallic: material.metallic_factor.clamp(0.0, 1.0),
        roughness: material.roughness_factor.clamp(0.0, 1.0),
        alpha_cutoff: match material.alpha_mode {
            AlphaMode::Mask(cutoff) => cutoff,
            AlphaMode::Opaque | AlphaMode::Blend => -1.0,
        },
        normal_scale: material.normal_scale,
        base_uv: material.base_uv_transform,
        metallic_roughness_uv: material.mr_uv_transform,
        normal_uv: material.normal_uv_transform,
        emissive_uv: material.emissive_uv_transform,
        base_texture: ids[0],
        metallic_roughness_texture: ids[1],
        normal_texture: ids[2],
        emissive_texture: ids[3],
        alpha_texture: -1,
        // Bit 4 is occlusion, which the C++ material has no slot for.
        uv1_slot_mask: material.uv_set_mask & 0b1111,
        unlit: material.unlit,
        alpha_blend: material.alpha_mode == AlphaMode::Blend,
        double_sided: material.double_sided,
    }
}

/// One node: its light into `lights`, then each drawable primitive in its default morph pose, baked by its world
/// transform, or by its skin at rest.
fn visit_node(
    model: &mut FlatModel,
    lights: &mut Vec<CpuLight>,
    source: &Source,
    node: &gltf::Node,
    world: Mat4,
) -> anyhow::Result<()> {
    lights.extend(gltf_loader::read_light(node, world));
    let Some(mesh) = node.mesh() else {
        return Ok(());
    };
    // glTF ignores a skinned mesh's node transform: its joints place it.
    let joints = node.skin().map(|skin| {
        source
            .joints
            .get(skin.index())
            .map_or(&[][..], Vec::as_slice)
    });
    let world = if joints.is_some() {
        Mat4::IDENTITY
    } else {
        world
    };
    let fallback = (model.materials.len() - 1) as u32;
    for primitive in mesh.primitives().filter(gltf_loader::draws_triangles) {
        let Some(mut geometry) =
            gltf_loader::primitive_geometry(&primitive, &source.imported.buffers, source.options)?
        else {
            continue;
        };
        // The C++ engine draws no morphs, so it gets the default pose the Rust renderer starts from.
        if let Some(weights) =
            gltf_loader::default_morph_weights(node, geometry.morph_targets.len())
        {
            geometry.vertices =
                scene::blend_morph_targets(&geometry.vertices, &geometry.morph_targets, weights);
        }
        // Nor skins: it gets the pose the Rust renderer uploads, the joints at the file's transforms.
        if let Some(joints) = joints {
            let vertices = &mut geometry.vertices;
            gltf_loader::bind_influences(vertices, Some(joints.len()), primitive.index());
            skin::rewind_mirrored_triangles(
                vertices,
                &mut geometry.indices,
                joints,
                Mat4::IDENTITY,
            );
            for vertex in vertices.iter_mut() {
                *vertex = skin::skin_vertex(vertex, joints, Mat4::IDENTITY);
            }
        }
        let material = primitive.material().index().map_or(fallback, |i| i as u32);
        append_primitive(model, &geometry, world, material);
    }
    Ok(())
}

/// Appends one primitive in world space; a mirroring transform reverses the winding so the front face stays front.
pub(crate) fn append_primitive(
    model: &mut FlatModel,
    geometry: &Geometry,
    world: Mat4,
    material: u32,
) {
    let linear = Mat3::from_mat4(world);
    let normal_matrix = linear.inverse().transpose();
    let mirrored = linear.determinant() < 0.0;
    let handedness = if mirrored { -1.0 } else { 1.0 };
    let vertex_base = model.vertices.len();
    model.vertices.extend(geometry.vertices.iter().map(|v| {
        let tangent = (linear * Vec3::from_slice(&v.tangent[..3])).normalize_or_zero();
        FlatVertex {
            position: world
                .transform_point3(Vec3::from_array(v.position))
                .to_array(),
            normal: (normal_matrix * Vec3::from_array(v.normal))
                .normalize_or_zero()
                .to_array(),
            color: v.color,
            uv0: v.uv,
            tangent: [tangent.x, tangent.y, tangent.z, v.tangent[3] * handedness],
            uv1: v.uv1,
        }
    }));
    let base = vertex_base as u32;
    let index_start = model.indices.len();
    for &[a, b, c] in geometry.indices.as_chunks::<3>().0 {
        let triangle = if mirrored { [a, c, b] } else { [a, b, c] };
        model.indices.extend(triangle.map(|i| base + i));
    }
    let tri_start = model.material_index.len();
    let tri_count = (model.indices.len() - index_start) / 3;
    model
        .material_index
        .extend(std::iter::repeat_n(material, tri_count));
    let flat = &model.materials[material as usize];
    model.ranges.push(FlatRange {
        vertex_base,
        vertex_count: geometry.vertices.len(),
        index_start,
        index_count: tri_count * 3,
        tri_start,
        tri_count,
        double_sided: flat.double_sided,
        alpha_blended: flat.alpha_blend,
    });
}

fn flat_light(light: &CpuLight) -> FlatLight {
    let (kind, cos_inner, cos_outer) = match light.kind {
        CpuLightKind::Point => (1, 1.0, 1.0),
        CpuLightKind::Spot {
            cos_inner,
            cos_outer,
        } => (2, cos_inner, cos_outer),
        CpuLightKind::Directional => (3, 1.0, 1.0),
    };
    // A node that scales its light to nothing has no -Z; the C++ light then points down -Z.
    let direction = Vec3::from_array(light.direction)
        .try_normalize()
        .unwrap_or(Vec3::NEG_Z);
    FlatLight {
        kind,
        color: light.color,
        intensity: light.intensity,
        range: light.range,
        position: light.position,
        direction: direction.to_array(),
        cos_inner,
        cos_outer,
    }
}
