//! The OBJ and MTL text, read as tinyobjloader reads it: a bad line is skipped and reported, never fatal.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;

use anyhow::{bail, Result};

use super::paths;
use super::triangulate::triangulate;

/// A material as OBJ describes it, reduced to what glTF can represent.
#[derive(Debug, Clone)]
pub struct ObjMaterial {
    pub name: String,
    /// `Kd` plus `d`/`Tr` as alpha.
    pub base_color: [f32; 4],
    /// `map_Kd`; relative to the .mtl as `parse_mtl` reads it, to the OBJ's folder once `read_obj` has placed it.
    pub base_color_texture: Option<String>,
    /// `Ke`; `None` when absent, so an authored `Ke 0 0 0` stays dark beside a `map_Ke`.
    pub emissive: Option<[f32; 3]>,
    /// `norm`/`map_Bump`/`map_bump`/`bump`, relative like `base_color_texture`.
    pub normal_texture: Option<String>,
    /// The bump directive's `-bm` option (glTF `normalTexture.scale`).
    pub normal_scale: f32,
    /// `map_Ke`, relative like `base_color_texture`.
    pub emissive_texture: Option<String>,
    /// `map_d`, relative like `base_color_texture`: a cut-out mask in its first channel, as the C++ engine reads it.
    pub alpha_texture: Option<String>,
    /// `Pm`; `None` when absent, distinct from an authored `Pm 0.0`.
    pub metallic: Option<f32>,
    /// `Pr`. `None` when absent, same reasoning as `metallic`.
    pub roughness: Option<f32>,
    /// `Ns`; derives `roughnessFactor` when `Pr` is absent, as the C++ `material_roughness()` does.
    pub shininess: Option<f32>,
}

impl Default for ObjMaterial {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            // glTF's own default, so an OBJ without materials converts untinted rather than black.
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: None,
            emissive: None,
            normal_texture: None,
            // glTF's own default normalTexture.scale.
            normal_scale: 1.0,
            emissive_texture: None,
            alpha_texture: None,
            metallic: None,
            roughness: None,
            shininess: None,
        }
    }
}

/// One triangulated mesh extracted from an OBJ file.
#[derive(Debug, Default, Clone)]
pub struct ObjMesh {
    /// Interleaved-free parallel arrays, one entry per unique vertex.
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// `[1,1,1,1]` per vertex when the OBJ had no vertex colour; see [`ObjMesh::has_vertex_colors`].
    pub colors: Vec<[f32; 4]>,
    /// Whether any `v` line carried a colour; this, not `colors.is_empty()`, gates `COLOR_0`.
    pub has_vertex_colors: bool,
    /// Whether any `vn` line appeared; informational, since missing normals are filled flat.
    pub has_normals: bool,
    /// Lines skipped as malformed or unsupported; each was logged as a warning, the first few with their number.
    pub skipped_lines: usize,
    pub indices: Vec<u32>,
    /// Materials referenced by the file, in declaration order.
    pub materials: Vec<ObjMaterial>,
    /// `(first_index, index_count, material_index)` per `usemtl` run, all into one index array: a run ends where its
    /// group (`o`/`g`) or its material does, so each mesh culls as the file grouped it (Sponza: 385 runs).
    pub submeshes: Vec<(u32, u32, usize)>,
}

impl ObjMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Axis-aligned bounds; glTF requires min/max on POSITION and loaders cull with them.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &self.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        (min, max)
    }
}

/// Parses the `.mtl` subset glTF can represent; unknown Phong-era directives are ignored, not rejected.
pub fn parse_mtl(source: &str) -> Vec<ObjMaterial> {
    let mut materials: Vec<ObjMaterial> = Vec::new();
    // tinyobjloader's rule: a `d` wins over a `Tr` of the same material, whichever comes first.
    let mut has_d = false;
    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        let values: Vec<&str> = parts.collect();

        if keyword == "newmtl" {
            has_d = false;
            materials.push(ObjMaterial {
                name: values.first().copied().unwrap_or("unnamed").to_string(),
                ..ObjMaterial::default()
            });
        } else if let Some(material) = materials.last_mut() {
            if keyword == "Tr" && has_d {
                continue;
            }
            has_d |= keyword == "d";
            apply_mtl_directive(material, keyword, &values);
        }
    }
    materials
}

/// One directive after `newmtl`, applied to the material it belongs to.
fn apply_mtl_directive(material: &mut ObjMaterial, keyword: &str, values: &[&str]) {
    let first = || values.first().and_then(|value| value.parse::<f32>().ok());
    // Last token, past any options; `\` becomes `/` so Windows-authored paths resolve on Linux.
    let path = || values.last().map(|name| name.replace('\\', "/"));
    match keyword {
        "Kd" if values.len() >= 3 => parse_rgb_into(&mut material.base_color, values),
        // Stored unclamped: `to_gltf` carries values above 1 via KHR_materials_emissive_strength.
        "Ke" if values.len() >= 3 => {
            let mut rgb = material.emissive.unwrap_or([0.0; 3]);
            parse_rgb_into(&mut rgb, values);
            material.emissive = Some(rgb);
        }
        // `d` is opacity and `Tr` its inverse; mixing them up swaps opaque and transparent.
        "d" => {
            if let Some(opacity) = first() {
                material.base_color[3] = opacity.clamp(0.0, 1.0);
            }
        }
        "Tr" => {
            if let Some(transparency) = first() {
                material.base_color[3] = (1.0 - transparency).clamp(0.0, 1.0);
            }
        }
        "map_Kd" if !values.is_empty() => material.base_color_texture = path(),
        "map_Ke" if !values.is_empty() => material.emissive_texture = path(),
        "map_d" if !values.is_empty() => material.alpha_texture = path(),
        "Pm" => material.metallic = first().or(material.metallic),
        "Pr" => material.roughness = first().or(material.roughness),
        "Ns" => material.shininess = first().or(material.shininess),
        // `norm` wins whatever the order; bump directives only fill an empty slot, and the winner's `-bm` counts.
        "norm" | "map_Bump" | "map_bump" | "bump"
            if !values.is_empty() && (keyword == "norm" || material.normal_texture.is_none()) =>
        {
            material.normal_texture = path();
            material.normal_scale = bump_scale_option(values).unwrap_or(1.0);
        }
        _ => {}
    }
}

/// The first three values into `rgb`'s first three channels; a component that does not parse is kept.
fn parse_rgb_into(rgb: &mut [f32], values: &[&str]) {
    for (axis, value) in values.iter().take(3).enumerate() {
        if let Ok(component) = value.parse::<f32>() {
            rgb[axis] = component;
        }
    }
}

/// Finds a bump directive's `-bm <factor>` option, e.g. `bump -bm 0.5 rock_normal.png`.
fn bump_scale_option(values: &[&str]) -> Option<f32> {
    values
        .iter()
        .position(|token| *token == "-bm")
        .and_then(|index| values.get(index + 1))
        .and_then(|value| value.parse::<f32>().ok())
}

/// Parses an OBJ without its materials; see [`parse_obj_with_materials`].
pub fn parse_obj(source: &str) -> Result<ObjMesh> {
    parse_obj_with_materials(source, Vec::new())
}

/// Each distinct `position/uv/normal` triple becomes one vertex, so more vertices than `v` lines is expected.
/// Only a file without a single drawable face is an error; every bad line is skipped and reported.
pub fn parse_obj_with_materials(source: &str, materials: Vec<ObjMaterial>) -> Result<ObjMesh> {
    let mut parser = ObjParser::new(materials);
    for (line_number, raw_line) in source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        parser.values.clear();
        parser.values.extend(parts);
        let at = line_number + 1;
        match keyword {
            "v" => parser.vertex(at),
            "vn" => parser.normal(at),
            "vt" => parser.texcoord(at),
            "f" => parser.face(at),
            "usemtl" => parser.use_material(at),
            // A group shares no vertex with the one before, so a corner without `vn` takes its own group's face normal.
            "o" | "g" => parser.start_group(),
            // Smoothing changes nothing drawn; lines and points draw nothing in a triangle mesh.
            "mtllib" | "s" | "l" | "p" => {}
            other => parser.skip(at, &format!("unsupported directive '{other}'")),
        }
    }
    parser.finish()
}

/// The first few skipped lines are logged one by one; the rest only count, so a broken export cannot flood the log.
const REPORTED_LINES: usize = 8;

/// The ear clipper costs up to n^3 on one face; the largest face in any scene this repo knows has 1429 corners.
pub(super) const MAX_FACE_CORNERS: usize = 2048;

/// A number as tinyobjloader reads one: `nan` and `inf` parse as floats but would poison bounds and the BLEND sort.
fn finite(text: &str) -> Option<f32> {
    text.parse::<f32>().ok().filter(|value| value.is_finite())
}

/// One corner of a face: zero-based attribute indices, `u32::MAX` where the corner names none.
type Corner = (u32, u32, u32);
const NONE: u32 = u32::MAX;

/// FNV-1a: the corner map is the parse's hot path, and SipHash's DoS resistance buys nothing for a local file.
#[derive(Default)]
struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u32(&mut self, value: u32) {
        let seeded = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        self.0 = (seeded ^ u64::from(value)).wrapping_mul(0x0100_0000_01b3);
    }
}

/// The state of one OBJ parse: the file's own attribute lists and the mesh its faces build.
struct ObjParser<'a> {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    mesh: ObjMesh,
    // Within a group, corners naming one position/uv/normal triple share a vertex (and, without `vn`, one flat normal).
    seen: HashMap<Corner, u32, BuildHasherDefault<Fnv>>,
    active_material: usize,
    /// Each material's index by name; the first `newmtl` of a name wins, as in tinyobjloader.
    material_index: HashMap<String, usize>,
    run_start: u32,
    /// The current line's values after its keyword, reused so a line allocates nothing.
    values: Vec<&'a str>,
    corners: Vec<Corner>,
}

impl<'a> ObjParser<'a> {
    fn new(materials: Vec<ObjMaterial>) -> Self {
        let mut material_index = HashMap::new();
        for (index, material) in materials.iter().enumerate() {
            material_index.entry(material.name.clone()).or_insert(index);
        }
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            mesh: ObjMesh {
                materials,
                ..ObjMesh::default()
            },
            seen: HashMap::default(),
            active_material: 0,
            material_index,
            run_start: 0,
            values: Vec::new(),
            corners: Vec::new(),
        }
    }

    fn skip(&mut self, at: usize, why: &str) {
        self.mesh.skipped_lines += 1;
        if self.mesh.skipped_lines <= REPORTED_LINES {
            log::warn!("OBJ line {at}: {why}; skipped");
        }
    }

    /// The values as numbers; one that does not parse, or is not finite, reads 0 as in tinyobjloader, so indices keep.
    fn numbers<const N: usize>(&mut self, at: usize, what: &str) -> [f32; N] {
        let mut out = [0.0; N];
        let mut bad = self.values.len() < N;
        for (slot, text) in out.iter_mut().zip(&self.values) {
            match finite(text) {
                Some(value) => *slot = value,
                None => bad = true,
            }
        }
        if bad {
            self.mesh.skipped_lines += 1;
            if self.mesh.skipped_lines <= REPORTED_LINES {
                log::warn!("OBJ line {at}: '{what}' needs {N} numbers; the missing ones read 0");
            }
        }
        out
    }

    /// `v x y z`, an optional `w` ignored, or `v x y z r g b [a]`.
    fn vertex(&mut self, at: usize) {
        let position = self.numbers::<3>(at, "v");
        self.positions.push(position);
        let color = match self.values.len() {
            6.. => {
                let parsed: Vec<f32> = self.values[3..]
                    .iter()
                    .take(4)
                    .map(|text| finite(text).unwrap_or(1.0))
                    .collect();
                self.mesh.has_vertex_colors = true;
                [
                    parsed[0],
                    parsed[1],
                    parsed[2],
                    parsed.get(3).copied().unwrap_or(1.0),
                ]
            }
            _ => [1.0; 4],
        };
        self.colors.push(color);
    }

    fn normal(&mut self, at: usize) {
        let normal = self.numbers::<3>(at, "vn");
        self.normals.push(normal);
        self.mesh.has_normals = true;
    }

    fn texcoord(&mut self, at: usize) {
        // A `vt u` without v is common and legal; only a missing u counts as malformed.
        let [u] = self.numbers::<1>(at, "vt");
        let v = self
            .values
            .get(1)
            .and_then(|text| finite(text))
            .unwrap_or(0.0);
        // OBJ's V points up and glTF's down; flipping here keeps the asset right for any consumer.
        self.uvs.push([u, 1.0 - v]);
    }

    fn face(&mut self, at: usize) {
        if self.values.len() < 3 {
            self.skip(at, "a face needs at least 3 vertices");
            return;
        }
        if self.values.len() > MAX_FACE_CORNERS {
            let why = format!(
                "a face of {} corners is over the {MAX_FACE_CORNERS} this reader splits",
                self.values.len()
            );
            self.skip(at, &why);
            return;
        }
        let mut corners = std::mem::take(&mut self.corners);
        corners.clear();
        let mut bad_attribute = None;
        for index in 0..self.values.len() {
            match self.corner(self.values[index]) {
                Ok((corner, attribute_error)) => {
                    corners.push(corner);
                    bad_attribute = bad_attribute.or(attribute_error);
                }
                Err(why) => {
                    self.skip(at, &why);
                    self.corners = corners;
                    return;
                }
            }
        }
        if let Some(why) = bad_attribute {
            self.skip(at, &format!("{why}, so those corners go without it"));
        }
        // One vertex per corner of this face, so the triangles of one face share it.
        let vertices: Vec<u32> = corners
            .iter()
            .map(|&corner| self.vertex_for(corner))
            .collect();
        if let [a, b, c] = vertices[..] {
            self.mesh.indices.extend_from_slice(&[a, b, c]);
        } else {
            let positions: Vec<[f32; 3]> = corners
                .iter()
                .map(|corner| self.positions[corner.0 as usize])
                .collect();
            for triangle in triangulate(&positions) {
                self.mesh
                    .indices
                    .extend(triangle.map(|local| vertices[local]));
            }
        }
        self.corners = corners;
    }

    /// `v`, `v/vt`, `v//vn` or `v/vt/vn`, resolved; a bad uv or normal index leaves the corner without it.
    fn corner(&self, text: &str) -> std::result::Result<(Corner, Option<String>), String> {
        let mut fields = text.split('/');
        let position = resolve(fields.next(), self.positions.len())
            .map_err(|why| format!("position index '{text}' {why}"))?
            .ok_or_else(|| format!("'{text}' has no position index"))?;
        let mut bad = None;
        let mut optional = |field: Option<&str>, available: usize, what: &str| {
            resolve(field, available).unwrap_or_else(|why| {
                bad = Some(format!("{what} index in '{text}' {why}"));
                None
            })
        };
        let uv = optional(fields.next(), self.uvs.len(), "texcoord");
        let normal = optional(fields.next(), self.normals.len(), "normal");
        Ok(((position, uv.unwrap_or(NONE), normal.unwrap_or(NONE)), bad))
    }

    /// The mesh vertex for one corner, appended the first time it appears.
    /// A corner without `vn` is never shared, so it takes its own face's flat normal, as glTF unindexes such a primitive.
    fn vertex_for(&mut self, key: Corner) -> u32 {
        let shared = key.2 != NONE;
        if let Some(&existing) = self.seen.get(&key).filter(|_| shared) {
            return existing;
        }
        let (position, uv, normal) = key;
        let mesh = &mut self.mesh;
        mesh.positions.push(self.positions[position as usize]);
        mesh.colors.push(self.colors[position as usize]);
        mesh.uvs.push(if uv == NONE {
            [0.0, 0.0]
        } else {
            self.uvs[uv as usize]
        });
        // Zero marks the corner for `fill_missing_flat_normals`.
        mesh.normals.push(if normal == NONE {
            [0.0; 3]
        } else {
            self.normals[normal as usize]
        });
        let index = (mesh.positions.len() - 1) as u32;
        if shared {
            self.seen.insert(key, index);
        }
        index
    }

    /// Ends the current material run; empty runs are skipped, as they would become primitives drawing nothing.
    fn close_run(&mut self) {
        let current = self.mesh.indices.len() as u32;
        if current > self.run_start {
            self.mesh.submeshes.push((
                self.run_start,
                current - self.run_start,
                self.active_material,
            ));
            self.run_start = current;
        }
    }

    /// An `o` or `g`: the run ends, and the group shares no vertex with the one before.
    fn start_group(&mut self) {
        self.close_run();
        self.seen.clear();
    }

    fn use_material(&mut self, at: usize) {
        let Some(&name) = self.values.first() else {
            self.skip(at, "'usemtl' names no material");
            return;
        };
        let materials = &mut self.mesh.materials;
        let next = *self
            .material_index
            .entry(name.to_string())
            .or_insert_with(|| {
                // Undeclared: keep the name so the mismatch shows instead of collapsing onto material 0.
                materials.push(ObjMaterial {
                    name: name.to_string(),
                    ..ObjMaterial::default()
                });
                materials.len() - 1
            });
        if next != self.active_material {
            self.close_run();
            self.active_material = next;
        }
    }

    fn finish(mut self) -> Result<ObjMesh> {
        self.close_run();
        let skipped = self.mesh.skipped_lines;
        if skipped > REPORTED_LINES {
            log::warn!(
                "OBJ: {} more malformed line(s) skipped",
                skipped - REPORTED_LINES
            );
        }
        let mut mesh = self.mesh;
        if mesh.indices.is_empty() {
            bail!("the OBJ contained no drawable face");
        }
        fill_missing_flat_normals(&mut mesh);
        if mesh.materials.is_empty() {
            mesh.materials.push(ObjMaterial::default());
        }
        Ok(mesh)
    }
}

/// An OBJ index field: 1-based, or negative for "counted back from the latest"; `Ok(None)` when the field is empty.
fn resolve(
    field: Option<&str>,
    available: usize,
) -> std::result::Result<Option<u32>, &'static str> {
    let Some(text) = field.filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    let index: i64 = text.parse().map_err(|_| "is not a number")?;
    let zero_based = match index {
        0 => return Err("is 0, and OBJ counts from 1"),
        1.. => index - 1,
        _ => available as i64 + index,
    };
    if zero_based < 0 || zero_based >= available as i64 {
        return Err("names a vertex the file has not declared yet");
    }
    Ok(Some(zero_based as u32))
}

/// Fills zero corner normals (no `vn` index) with their triangle's flat face normal, in file order.
/// Degenerate triangles are skipped rather than normalized into NaN.
fn fill_missing_flat_normals(mesh: &mut ObjMesh) {
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = tri.map(|i| i as usize);
        let p0 = mesh.positions[i0];
        let p1 = mesh.positions[i1];
        let p2 = mesh.positions[i2];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let face_normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len_sq = face_normal[0] * face_normal[0]
            + face_normal[1] * face_normal[1]
            + face_normal[2] * face_normal[2];
        if len_sq <= 0.0 {
            continue;
        }
        let inv_len = len_sq.sqrt().recip();
        let normal = face_normal.map(|component| component * inv_len);
        for i in [i0, i1, i2] {
            let n = mesh.normals[i];
            if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] <= 1e-12 {
                mesh.normals[i] = normal;
            }
        }
    }
}

/// Parses `obj_path` with the materials of every `mtllib` it names; text that is not UTF-8 is read lossily. A `.mtl`
/// or a texture outside the OBJ's folder is refused with a warning, as a glTF's buffers and images are.
pub fn read_obj(obj_path: &Path) -> Result<ObjMesh> {
    use anyhow::Context as _;
    let bytes = crate::files::read_regular_file(obj_path)
        .with_context(|| format!("reading {}", obj_path.display()))?;
    let source = String::from_utf8_lossy(&bytes);

    // A missing .mtl is not fatal: the geometry is still worth loading.
    let mut materials: Vec<ObjMaterial> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        if parts.next() != Some("mtllib") {
            continue;
        }
        for name in parts {
            let mtl = paths::inside(name)
                .ok_or_else(|| anyhow::anyhow!("'{name}' leaves the OBJ's folder"))
                .and_then(|relative| Ok((paths::confined(obj_path, &relative)?, relative)))
                .and_then(|(file, relative)| {
                    Ok((crate::files::read_regular_file(&file)?, relative))
                });
            match mtl {
                Ok((text, relative)) => {
                    let parsed = parse_mtl(&String::from_utf8_lossy(&text));
                    materials.extend(
                        parsed
                            .into_iter()
                            .map(|m| placed(m, paths::folder(&relative))),
                    );
                }
                Err(error) => log::warn!("mtllib '{name}': {error:#}; loading without it"),
            }
        }
    }

    parse_obj_with_materials(&source, materials)
        .with_context(|| format!("parsing {}", obj_path.display()))
}

/// The material with its textures relative to the OBJ's folder instead of its `.mtl`'s; one that leaves it is dropped.
fn placed(mut material: ObjMaterial, mtl_folder: &str) -> ObjMaterial {
    for texture in [
        &mut material.base_color_texture,
        &mut material.normal_texture,
        &mut material.emissive_texture,
        &mut material.alpha_texture,
    ] {
        if let Some(name) = texture.take() {
            *texture = paths::placed(mtl_folder, &name);
            if texture.is_none() {
                log::warn!(
                    "material '{}': texture '{name}' leaves the OBJ's folder; drawn without it",
                    material.name
                );
            }
        }
    }
    material
}
