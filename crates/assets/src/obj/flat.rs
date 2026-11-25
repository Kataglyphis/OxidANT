//! An OBJ flattened for the C++ engine: the arrays `flat::load_flat` gives a glTF, one range per `usemtl` run, and each
//! texture's RGBA8 mip chain, decoded on every core.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use glam::Mat4;

use super::material::{self, finite_or};
use super::parse::{read_obj, ObjMaterial, ObjMesh};
use super::paths::texture_source;
use crate::flat::{
    append_primitive, flat_material, renumber_slots, FlatMaterial, FlatModel, FlatTexture,
    DECODE_BUDGET,
};
use crate::gltf_images::StoredImage;
use crate::gltf_loader::{finish_with_generated_tangents, Geometry, LoadOptions};
use crate::parallel::{self, Budget};
use crate::scene::{CpuMaterial, CpuSampler, Vertex};

/// Loads the OBJ at `path` with its materials and textures into the engine's arrays; an error when nothing draws.
pub fn load_obj_flat(path: impl AsRef<Path>, options: &LoadOptions) -> anyhow::Result<FlatModel> {
    let path = path.as_ref();
    let mesh =
        read_obj(path).with_context(|| format!("Failed to load OBJ file: {}", path.display()))?;
    let mut slots = Slots {
        obj_path: path,
        keys: HashMap::new(),
        pending: Vec::new(),
    };
    let mut materials: Vec<FlatMaterial> = mesh
        .materials
        .iter()
        .map(|material| obj_flat_material(material, &mut slots))
        .collect();
    // The neutral fallback every FlatModel ends with; no OBJ range uses it.
    materials.push(flat_material(&CpuMaterial::default(), [-1; 5]));
    let budget = Budget::new(DECODE_BUDGET);
    let decoded = parallel::map(&slots.pending, |(file, stored, srgb)| {
        FlatTexture::from_stored(
            stored,
            CpuSampler::default(),
            *srgb,
            &file.display(),
            &budget,
        )
    });
    let textures = renumber_slots(&mut materials, decoded);
    let mut model = FlatModel {
        materials,
        textures,
        // OBJ has no copyright field: only the folder's attribution credits it.
        credit: crate::flat::scene_meta_credit(path, None),
        ..FlatModel::default()
    };
    append_groups(&mut model, &mesh, options);
    Ok(model)
}

/// One range per `usemtl` run, each with only the vertices its triangles use and tangents generated from UV0.
fn append_groups(model: &mut FlatModel, mesh: &ObjMesh, options: &LoadOptions) {
    let pool: Vec<Vertex> = (0..mesh.positions.len())
        .map(|i| Vertex {
            position: mesh.positions[i],
            normal: mesh.normals[i],
            uv: mesh.uvs[i],
            tangent: [0.0; 4],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: mesh.colors[i],
            // OBJ has one UV set; the engine's second one copies it, as a glTF without TEXCOORD_1 does.
            uv1: mesh.uvs[i],
        })
        .collect();
    let mut local = vec![u32::MAX; pool.len()];
    for &(first, count, material) in &mesh.submeshes {
        let indices = &mesh.indices[first as usize..(first + count) as usize];
        let mut geometry = Geometry {
            vertices: Vec::new(),
            indices: Vec::with_capacity(indices.len()),
            morph_targets: Vec::new(),
        };
        for &index in indices {
            let slot = &mut local[index as usize];
            if *slot == u32::MAX {
                *slot = geometry.vertices.len() as u32;
                geometry.vertices.push(pool[index as usize]);
            }
            geometry.indices.push(*slot);
        }
        for &index in indices {
            local[index as usize] = u32::MAX;
        }
        let geometry = finish_with_generated_tangents(geometry, options);
        append_primitive(model, &geometry, Mat4::IDENTITY, material as u32);
    }
}

/// The engine's material for one OBJ material, by the rules the WebGPU renderer's glTF gets too.
fn obj_flat_material(material: &ObjMaterial, slots: &mut Slots) -> FlatMaterial {
    let rules = CpuMaterial {
        base_color: material::base_color(material),
        alpha_mode: material::alpha_mode(material),
        metallic_factor: material::metallic(material),
        roughness_factor: material::roughness(material),
        emissive_factor: material::emissive(material),
        normal_scale: finite_or(material.normal_scale, 1.0),
        ..CpuMaterial::default()
    };
    let ids = [
        slots.slot(material.base_color_texture.as_deref(), true),
        -1,
        slots.slot(material.normal_texture.as_deref(), false),
        slots.slot(material.emissive_texture.as_deref(), true),
        -1,
    ];
    FlatMaterial {
        alpha_texture: slots.slot(material.alpha_texture.as_deref(), false),
        ..flat_material(&rules, ids)
    }
}

/// Dedups slots on (file, colour space): two spellings of one file share a slot, one file in sRGB and linear does not.
struct Slots<'a> {
    obj_path: &'a Path,
    keys: HashMap<(PathBuf, bool), i32>,
    /// Each slot's file, its bytes and its colour space, decoded once every material has its slots.
    pending: Vec<(PathBuf, StoredImage, bool)>,
}

impl Slots<'_> {
    /// -1, with one warning, for a file that cannot be read, so the material draws untextured.
    fn slot(&mut self, uri: Option<&str>, srgb: bool) -> i32 {
        let Some(uri) = uri else {
            return -1;
        };
        // A file that cannot be found is keyed by its name, so it warns once.
        let file = texture_source(uri, self.obj_path).unwrap_or_else(|| PathBuf::from(uri));
        if let Some(&slot) = self.keys.get(&(file.clone(), srgb)) {
            return slot;
        }
        let encoded = match crate::files::read_regular_file(&file) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            unreadable => {
                if let Some(error) = unreadable.err().filter(|_| file.is_absolute()) {
                    log::warn!(
                        "{}: {error:#}; the material draws without it",
                        file.display()
                    );
                }
                self.keys.insert((file, srgb), -1);
                return -1;
            }
        };
        let slot = self.pending.len() as i32;
        self.keys.insert((file.clone(), srgb), slot);
        let stored = StoredImage {
            bytes: encoded,
            mime_type: None,
            uri: Some(file.to_string_lossy().into_owned()),
        };
        self.pending.push((file, stored, srgb));
        slot
    }
}
