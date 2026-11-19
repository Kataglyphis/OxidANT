//! Converts Wavefront OBJ to glTF 2.0, so the C++ engine's `Resources/Models` load in this renderer.
//! Unknown OBJ input fails loudly, so a converted asset never silently differs from its source.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

/// A material as OBJ describes it, reduced to what glTF can represent.
#[derive(Debug, Clone)]
pub struct ObjMaterial {
    pub name: String,
    /// `Kd` plus `d`/`Tr` as alpha.
    pub base_color: [f32; 4],
    /// `map_Kd`, as written in the .mtl (relative to it).
    pub base_color_texture: Option<String>,
    /// `Ke`.
    pub emissive: [f32; 3],
    /// `norm`/`map_Bump`/`map_bump`/`bump`, as written in the .mtl (relative to it).
    pub normal_texture: Option<String>,
    /// The bump directive's `-bm` option (glTF `normalTexture.scale`).
    pub normal_scale: f32,
    /// `map_Ke`, as written in the .mtl (relative to it).
    pub emissive_texture: Option<String>,
    /// `map_d`, as written in the .mtl (relative to it): a cut-out mask in its first channel, as the C++ engine reads it.
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
            // glTF's own default emissiveFactor.
            emissive: [0.0, 0.0, 0.0],
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
    pub indices: Vec<u32>,
    /// Materials referenced by the file, in declaration order.
    pub materials: Vec<ObjMaterial>,
    /// `(first_index, index_count, material_index)` per `usemtl` run, all into one index array.
    /// Splitting vertex data per material would duplicate shared vertices and change the geometry.
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

    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        let values: Vec<&str> = parts.collect();

        if keyword == "newmtl" {
            materials.push(ObjMaterial {
                name: values.first().copied().unwrap_or("unnamed").to_string(),
                ..ObjMaterial::default()
            });
        } else if let Some(material) = materials.last_mut() {
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
        "Ke" if values.len() >= 3 => parse_rgb_into(&mut material.emissive, values),
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
        // `norm` wins whatever the order; bump directives only fill an empty slot.
        "norm" | "map_Bump" | "map_bump" | "bump"
            if !values.is_empty() && (keyword == "norm" || material.normal_texture.is_none()) =>
        {
            material.normal_texture = path();
            if let Some(scale) = bump_scale_option(values) {
                material.normal_scale = scale;
            }
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

/// Parses the OBJ subset this converter supports.
/// Each distinct `position/uv/normal` triple becomes one vertex, so more vertices than `v` lines is expected.
pub fn parse_obj(source: &str) -> Result<ObjMesh> {
    parse_obj_with_materials(source, Vec::new())
}

/// As [`parse_obj`], with materials already loaded from the companion `.mtl`.
pub fn parse_obj_with_materials(source: &str, materials: Vec<ObjMaterial>) -> Result<ObjMesh> {
    let mut parser = ObjParser::new(materials);
    for (line_number, raw_line) in source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        let values: Vec<&str> = parts.collect();
        let at = line_number + 1;

        match keyword {
            "v" => parser.vertex(&values, at)?,
            "vn" => parser.normal(&values, at)?,
            "vt" => parser.texcoord(&values, at)?,
            "f" => parser.face(&values, at)?,
            "usemtl" => parser.use_material(values.first().copied().unwrap_or("")),
            // Ignored rather than fatal: almost every real OBJ carries them.
            "mtllib" | "o" | "g" | "s" => {}
            other => {
                bail!("line {at}: unsupported OBJ directive '{other}'");
            }
        }
    }
    parser.finish()
}

/// The state of one OBJ parse: the file's own attribute lists and the mesh its faces build.
struct ObjParser {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    mesh: ObjMesh,
    // Without `vn`, corners at one position share a vertex and one flat normal, as in the C++ loaders.
    seen: HashMap<(i64, i64, i64), u32>,
    active_material: usize,
    run_start: u32,
}

impl ObjParser {
    fn new(materials: Vec<ObjMaterial>) -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            mesh: ObjMesh {
                materials,
                ..ObjMesh::default()
            },
            seen: HashMap::new(),
            active_material: 0,
            run_start: 0,
        }
    }

    fn vertex(&mut self, values: &[&str], at: usize) -> Result<()> {
        if values.len() != 3 && values.len() != 6 && values.len() != 7 {
            bail!(
                "line {at}: 'v' needs 3, 6 or 7 components, got {}",
                values.len()
            );
        }
        self.positions.push([
            parse_f32(values[0], at)?,
            parse_f32(values[1], at)?,
            parse_f32(values[2], at)?,
        ]);
        if values.len() >= 6 {
            let alpha = if values.len() == 7 {
                parse_f32(values[6], at)?
            } else {
                1.0
            };
            self.colors.push([
                parse_f32(values[3], at)?,
                parse_f32(values[4], at)?,
                parse_f32(values[5], at)?,
                alpha,
            ]);
            self.mesh.has_vertex_colors = true;
        } else {
            self.colors.push([1.0, 1.0, 1.0, 1.0]);
        }
        Ok(())
    }

    fn normal(&mut self, values: &[&str], at: usize) -> Result<()> {
        if values.len() < 3 {
            bail!("line {at}: 'vn' needs 3 components, got {}", values.len());
        }
        self.normals.push([
            parse_f32(values[0], at)?,
            parse_f32(values[1], at)?,
            parse_f32(values[2], at)?,
        ]);
        self.mesh.has_normals = true;
        Ok(())
    }

    fn texcoord(&mut self, values: &[&str], at: usize) -> Result<()> {
        if values.len() < 2 {
            bail!("line {at}: 'vt' needs 2 components, got {}", values.len());
        }
        // OBJ's V points up and glTF's down; flipping here keeps the asset right for any consumer.
        self.uvs
            .push([parse_f32(values[0], at)?, 1.0 - parse_f32(values[1], at)?]);
        Ok(())
    }

    fn face(&mut self, values: &[&str], at: usize) -> Result<()> {
        if values.len() < 3 {
            bail!(
                "line {at}: 'f' needs at least 3 vertices, got {}",
                values.len()
            );
        }
        let mut face: Vec<u32> = Vec::with_capacity(values.len());
        for value in values {
            face.push(self.face_vertex(value, at)?);
        }
        // Fan triangulation: correct for the convex faces OBJ exporters emit.
        for i in 1..face.len() - 1 {
            self.mesh
                .indices
                .extend_from_slice(&[face[0], face[i], face[i + 1]]);
        }
        Ok(())
    }

    /// The mesh vertex for one `position/uv/normal` triple, appended the first time it appears.
    fn face_vertex(&mut self, text: &str, at: usize) -> Result<u32> {
        let key = parse_face_vertex(text, at)?;
        if let Some(&existing) = self.seen.get(&key) {
            return Ok(existing);
        }
        let mesh = &mut self.mesh;
        let position = resolve(key.0, self.positions.len(), at, "position")?;
        mesh.positions.push(self.positions[position]);
        mesh.colors.push(self.colors[position]);

        if key.1 > 0 {
            let uv = resolve(key.1, self.uvs.len(), at, "texcoord")?;
            mesh.uvs.push(self.uvs[uv]);
        } else {
            mesh.uvs.push([0.0, 0.0]);
        }

        if key.2 > 0 {
            let normal = resolve(key.2, self.normals.len(), at, "normal")?;
            mesh.normals.push(self.normals[normal]);
        } else {
            // Zero marks the corner for `fill_missing_flat_normals`.
            mesh.normals.push([0.0, 0.0, 0.0]);
        }

        let index = (mesh.positions.len() - 1) as u32;
        self.seen.insert(key, index);
        Ok(index)
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

    fn use_material(&mut self, name: &str) {
        self.close_run();
        let materials = &mut self.mesh.materials;
        self.active_material = materials
            .iter()
            .position(|material| material.name == name)
            .unwrap_or_else(|| {
                // Undeclared: keep the name so the mismatch shows instead of collapsing onto material 0.
                materials.push(ObjMaterial {
                    name: name.to_string(),
                    ..ObjMaterial::default()
                });
                materials.len() - 1
            });
    }

    fn finish(mut self) -> Result<ObjMesh> {
        self.close_run();
        let mut mesh = self.mesh;
        fill_missing_flat_normals(&mut mesh);

        if mesh.positions.is_empty() {
            bail!("the OBJ contained no geometry");
        }
        if mesh.materials.is_empty() {
            mesh.materials.push(ObjMaterial::default());
        }
        if mesh.submeshes.is_empty() {
            mesh.submeshes.push((0, mesh.indices.len() as u32, 0));
        }
        Ok(mesh)
    }
}

/// Fills zero corner normals (no `vn` index) with their triangle's flat face normal.
/// Degenerate triangles are skipped rather than normalized into NaN, as C++ `fillMissingFlatNormals` does.
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
        let normal = [
            face_normal[0] * inv_len,
            face_normal[1] * inv_len,
            face_normal[2] * inv_len,
        ];
        for i in [i0, i1, i2] {
            let n = mesh.normals[i];
            if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] <= 1e-12 {
                mesh.normals[i] = normal;
            }
        }
    }
}

/// Escapes `.mtl` names and paths for the hand-written JSON string literals (RFC 8259).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Replaces non-finite floats, which `Display` as `inf`/`NaN` and are not valid JSON.
fn finite_or(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

fn parse_f32(text: &str, line: usize) -> Result<f32> {
    text.parse::<f32>()
        .with_context(|| format!("line {line}: '{text}' is not a number"))
}

/// Parses `v`, `v/vt`, `v//vn` or `v/vt/vn`. Missing components come back 0.
fn parse_face_vertex(text: &str, line: usize) -> Result<(i64, i64, i64)> {
    let mut fields = text.split('/');
    let position = fields
        .next()
        .unwrap_or("")
        .parse::<i64>()
        .with_context(|| format!("line {line}: '{text}' has no position index"))?;
    let uv = fields.next().unwrap_or("").parse::<i64>().unwrap_or(0);
    let normal = fields.next().unwrap_or("").parse::<i64>().unwrap_or(0);
    Ok((position, uv, normal))
}

/// OBJ indices are 1-based, and negative values mean "relative to the end".
fn resolve(index: i64, available: usize, line: usize, what: &str) -> Result<usize> {
    if index < 0 {
        bail!("line {line}: negative (relative) {what} indices are not supported");
    }
    if index == 0 {
        bail!("line {line}: {what} index 0 is invalid - OBJ indices start at 1");
    }
    let zero_based = (index - 1) as usize;
    if zero_based >= available {
        bail!("line {line}: {what} index {index} exceeds the {available} declared");
    }
    Ok(zero_based)
}

/// Serialises a mesh as `(gltf_json, bin)`; the JSON references `bin_uri`, so the caller picks the layout.
pub fn to_gltf(mesh: &ObjMesh, bin_uri: &str) -> (String, Vec<u8>) {
    let (bin, layout) = write_buffer(mesh);
    let (index_accessors, primitives) = primitives_json(mesh);
    let image_uris = image_uris(mesh);
    let texture_arrays = texture_arrays_json(&image_uris);
    let (materials_json, extensions_used_json) = materials_json(&mesh.materials, &image_uris);
    let (color_buffer_view_json, color_accessor_json) = color_json(mesh, layout.colors);

    let vertex_count = mesh.positions.len();
    let buffer_length = bin.len();
    let positions_length = vertex_count * 12;
    let normals_length = vertex_count * 12;
    let uvs_length = vertex_count * 8;
    let indices_length = mesh.indices.len() * 4;
    let BufferLayout {
        positions: positions_offset,
        normals: normals_offset,
        uvs: uvs_offset,
        indices: indices_offset,
        ..
    } = layout;
    let (min, max) = mesh.bounds();
    // An empty mesh's bounds are +/-infinity, which is not valid JSON.
    let [min0, min1, min2] = min.map(|component| finite_or(component, 0.0));
    let [max0, max1, max2] = max.map(|component| finite_or(component, 0.0));

    let json = format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "kataglyphis obj_to_gltf" }},
  "scene": 0,
  "scenes": [{{ "nodes": [0] }}],
  "nodes": [{{ "mesh": 0 }}],
  "meshes": [{{ "primitives": [{primitives}] }}],
  "materials": [{materials_json}],{texture_arrays}
  "buffers": [{{ "uri": "{bin_uri}", "byteLength": {buffer_length} }}],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": {positions_offset}, "byteLength": {positions_length}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {normals_offset}, "byteLength": {normals_length}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {uvs_offset}, "byteLength": {uvs_length}, "target": 34962 }}{color_buffer_view_json},
    {{ "buffer": 0, "byteOffset": {indices_offset}, "byteLength": {indices_length}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": {vertex_count}, "type": "VEC3", "min": [{min0}, {min1}, {min2}], "max": [{max0}, {max1}, {max2}] }},
    {{ "bufferView": 1, "componentType": 5126, "count": {vertex_count}, "type": "VEC3" }},
    {{ "bufferView": 2, "componentType": 5126, "count": {vertex_count}, "type": "VEC2" }}{color_accessor_json},
    {index_accessors}
  ]{extensions_used_json}
}}"#
    );

    (json, bin)
}

/// Byte offsets of each attribute and of the indices in the buffer `write_buffer` fills.
struct BufferLayout {
    positions: usize,
    normals: usize,
    uvs: usize,
    colors: usize,
    indices: usize,
}

fn push_f32_components<const N: usize>(bin: &mut Vec<u8>, values: &[[f32; N]]) {
    for value in values {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
}

/// The binary buffer: positions, normals, uvs, colours if any, then the indices.
fn write_buffer(mesh: &ObjMesh) -> (Vec<u8>, BufferLayout) {
    let mut bin: Vec<u8> = Vec::new();
    let positions = bin.len();
    push_f32_components(&mut bin, &mesh.positions);
    let normals = bin.len();
    push_f32_components(&mut bin, &mesh.normals);
    let uvs = bin.len();
    push_f32_components(&mut bin, &mesh.uvs);
    // Colours only when the source had them, so a colourless OBJ's buffer layout stays unchanged.
    let colors = bin.len();
    if mesh.has_vertex_colors {
        push_f32_components(&mut bin, &mesh.colors);
    }
    // u32 indices need 4-byte alignment; padded in case a non-float attribute is ever added.
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let indices = bin.len();
    for index in &mesh.indices {
        bin.extend_from_slice(&index.to_le_bytes());
    }
    let layout = BufferLayout {
        positions,
        normals,
        uvs,
        colors,
        indices,
    };
    (bin, layout)
}

/// `(index_accessors, primitives)`, one of each per `usemtl` run.
fn primitives_json(mesh: &ObjMesh) -> (String, String) {
    // Slots 0-2 are POSITION/NORMAL/TEXCOORD_0 and COLOR_0 takes 3; indices follow the computed base.
    let indices_buffer_view = if mesh.has_vertex_colors { 4 } else { 3 };
    let index_accessor_base = indices_buffer_view;

    // Per-run accessors view one index bufferView at different offsets, so shared vertices stay shared.
    let color_attribute = if mesh.has_vertex_colors {
        r#", "COLOR_0": 3"#
    } else {
        ""
    };
    let mut index_accessors = String::new();
    let mut primitives = String::new();
    for (run, &(first_index, count, material)) in mesh.submeshes.iter().enumerate() {
        if run > 0 {
            index_accessors.push_str(",\n    ");
            primitives.push_str(", ");
        }
        index_accessors.push_str(&format!(
            r#"{{ "bufferView": {}, "byteOffset": {}, "componentType": 5125, "count": {}, "type": "SCALAR" }}"#,
            indices_buffer_view,
            first_index as usize * 4,
            count
        ));
        primitives.push_str(&format!(
            r#"{{ "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2{} }}, "indices": {}, "material": {}, "mode": 4 }}"#,
            color_attribute,
            index_accessor_base + run,
            material
        ));
    }
    (index_accessors, primitives)
}

/// The texture files a material names that glTF carries; `map_d` is composited only into the GLB.
fn material_texture_uris(material: &ObjMaterial) -> impl Iterator<Item = &String> {
    [
        &material.base_color_texture,
        &material.normal_texture,
        &material.emissive_texture,
    ]
    .into_iter()
    .flatten()
}

/// Deduplicated by file, or a shared map is decoded and uploaded once per material and per spelling.
fn image_uris(mesh: &ObjMesh) -> Vec<String> {
    let mut image_uris: Vec<String> = Vec::new();
    for uri in mesh.materials.iter().flat_map(material_texture_uris) {
        if !image_uris.iter().any(|existing| same_file(existing, uri)) {
            image_uris.push(uri.clone());
        }
    }
    image_uris
}

/// Whether two .mtl paths name one file once `.` segments and `dir/..` pairs are gone: `tex.png` and `./tex.png` do.
fn same_file(a: &str, b: &str) -> bool {
    normalized(a) == normalized(b)
}

fn normalized(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|last| *last != "..") => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    let relative = segments.join("/");
    if path.starts_with('/') {
        format!("/{relative}")
    } else {
        relative
    }
}

/// The `images`, `samplers` and `textures` arrays, or nothing for an untextured model.
fn texture_arrays_json(image_uris: &[String]) -> String {
    if image_uris.is_empty() {
        return String::new();
    }
    let images_json = images_json(image_uris);
    let textures_json = (0..image_uris.len())
        .map(|index| format!(r#"{{ "source": {index}, "sampler": 0 }}"#))
        .collect::<Vec<_>>()
        .join(", ");
    // Explicit REPEAT (10497), OBJ's tiling convention, rather than relying on loader defaults.
    let samplers_json = r#"{ "wrapS": 10497, "wrapT": 10497 }"#;
    format!(
        "
  \"images\": [{images_json}],
  \"samplers\": [{samplers_json}],
  \"textures\": [{textures_json}],"
    )
}

/// `(materials, extensions_used)`: the joined `materials` entries and the document's `extensionsUsed`.
fn materials_json(materials: &[ObjMaterial], image_uris: &[String]) -> (String, String) {
    let entries: Vec<(String, bool)> = materials
        .iter()
        .map(|material| material_json(material, image_uris))
        .collect();
    let materials_json = entries
        .iter()
        .map(|(json, _)| json.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // Only when a material needs it: tests compare the generated JSON.
    let extensions_used_json = if entries.iter().any(|(_, used)| *used) {
        r#",
  "extensionsUsed": ["KHR_materials_emissive_strength"]"#
            .to_string()
    } else {
        String::new()
    };
    (materials_json, extensions_used_json)
}

/// One `materials` entry, and whether it uses KHR_materials_emissive_strength.
fn material_json(material: &ObjMaterial, image_uris: &[String]) -> (String, bool) {
    // Non-finite colour falls back to glTF's default white.
    let [r, g, b, a] = material
        .base_color
        .map(|component| finite_or(component, 1.0));
    let texture_index = |uri: &String| {
        image_uris
            .iter()
            .position(|existing| same_file(existing, uri))
            .unwrap_or(0)
    };
    let texture_json = match &material.base_color_texture {
        Some(uri) => format!(
            r#", "baseColorTexture": {{ "index": {} }}"#,
            texture_index(uri)
        ),
        None => String::new(),
    };
    let normal_texture_json = match &material.normal_texture {
        Some(uri) => {
            let scale = finite_or(material.normal_scale, 1.0);
            format!(
                r#", "normalTexture": {{ "index": {}, "scale": {scale} }}"#,
                texture_index(uri)
            )
        }
        None => String::new(),
    };
    let emissive_texture_json = match &material.emissive_texture {
        Some(uri) => format!(
            r#", "emissiveTexture": {{ "index": {} }}"#,
            texture_index(uri)
        ),
        None => String::new(),
    };
    let (emissive_json, extensions_json, uses_emissive_strength) = emissive_json(material);
    let json = format!(
        r#"{{ "name": "{}", "pbrMetallicRoughness": {{ "baseColorFactor": [{}, {}, {}, {}]{}, "metallicFactor": {}, "roughnessFactor": {} }}{}{}{}{}{} }}"#,
        json_escape(&material.name),
        r,
        g,
        b,
        a,
        texture_json,
        metallic_factor_json(material),
        roughness_factor_json(material),
        if a < 1.0 {
            r#", "alphaMode": "BLEND""#
        } else {
            ""
        },
        normal_texture_json,
        emissive_texture_json,
        emissive_json,
        extensions_json
    );
    (json, uses_emissive_strength)
}

fn metallic_factor_json(material: &ObjMaterial) -> String {
    // Plain-diffuse fallback as literal "0.0"/"1.0" (`Display` prints "0"/"1"); tests pin the output.
    match material.metallic {
        Some(value) => format!("{}", finite_or(value, 0.0)),
        None => "0.0".to_string(),
    }
}

fn roughness_factor_json(material: &ObjMaterial) -> String {
    // Without Pr, Ns maps through the C++ material_roughness() curve so both renderers agree.
    match material.roughness {
        Some(value) => format!("{}", finite_or(value, 1.0)),
        None => match material.shininess {
            Some(shininess) if shininess.is_finite() && shininess >= 0.0 => {
                format!("{}", (2.0 / (shininess + 2.0)).sqrt().clamp(0.045, 1.0))
            }
            _ => "1.0".to_string(),
        },
    }
}

/// `(emissiveFactor, extensions, uses_emissive_strength)` for one material.
fn emissive_json(material: &ObjMaterial) -> (String, String, bool) {
    let [er, eg, eb] = material.emissive.map(|component| finite_or(component, 0.0));
    // `emissiveFactor` is [0,1], so an HDR `Ke` moves its magnitude into KHR_materials_emissive_strength.
    let strength = er.max(eg).max(eb);
    let uses_emissive_strength = strength > 1.0;
    let (er, eg, eb, extensions_json) = if uses_emissive_strength {
        (
            er / strength,
            eg / strength,
            eb / strength,
            format!(
                r#", "extensions": {{ "KHR_materials_emissive_strength": {{ "emissiveStrength": {strength} }} }}"#
            ),
        )
    } else {
        (er, eg, eb, String::new())
    };
    // glTF's default factor is [0,0,0], so a map_Ke without Ke needs an explicit [1,1,1].
    let emissive_json = if er != 0.0 || eg != 0.0 || eb != 0.0 {
        format!(r#", "emissiveFactor": [{er}, {eg}, {eb}]"#)
    } else if material.emissive_texture.is_some() {
        r#", "emissiveFactor": [1, 1, 1]"#.to_string()
    } else {
        String::new()
    };
    (emissive_json, extensions_json, uses_emissive_strength)
}

/// `(bufferView, accessor)` for COLOR_0, both empty when the OBJ had no vertex colour.
fn color_json(mesh: &ObjMesh, colors_offset: usize) -> (String, String) {
    if !mesh.has_vertex_colors {
        return (String::new(), String::new());
    }
    let vertex_count = mesh.positions.len();
    let colors_length = vertex_count * 16;
    let buffer_view = format!(
        r#",
    {{ "buffer": 0, "byteOffset": {colors_offset}, "byteLength": {colors_length}, "target": 34962 }}"#
    );
    let accessor = format!(
        r#",
    {{ "bufferView": 3, "componentType": 5126, "count": {vertex_count}, "type": "VEC4" }}"#
    );
    (buffer_view, accessor)
}

/// Parses `obj_path` with the materials of every `mtllib` it names.
pub fn read_obj(obj_path: &Path) -> Result<ObjMesh> {
    let source = std::fs::read_to_string(obj_path)
        .with_context(|| format!("reading {}", obj_path.display()))?;

    // A missing .mtl is not fatal: the geometry is still worth converting.
    let mut materials: Vec<ObjMaterial> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("mtllib ") {
            for name in rest.split_whitespace() {
                let mtl_path = obj_path.with_file_name(name);
                match std::fs::read_to_string(&mtl_path) {
                    Ok(mtl) => materials.extend(parse_mtl(&mtl)),
                    Err(error) => {
                        log::warn!("{}: {error}; converting without it", mtl_path.display());
                    }
                }
            }
        }
    }

    parse_obj_with_materials(&source, materials)
        .with_context(|| format!("parsing {}", obj_path.display()))
}

/// Converts `obj_path` to `gltf_path`, writing the binary buffer alongside it.
pub fn convert_file(obj_path: &Path, gltf_path: &Path) -> Result<ObjMesh> {
    let mesh = read_obj(obj_path)?;
    check_texture_formats(&mesh)?;
    for material in mesh.materials.iter().filter(|m| m.alpha_texture.is_some()) {
        log::warn!(
            "material '{}': its map_d mask needs a composited image, which only the in-memory GLB writes; it draws solid",
            material.name
        );
    }
    let bin_name = gltf_path
        .file_stem()
        .map(|stem| format!("{}.bin", stem.to_string_lossy()))
        .unwrap_or_else(|| "buffer.bin".to_string());
    let (json, bin) = to_gltf(&mesh, &bin_name);

    if let Some(parent) = gltf_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(gltf_path, json).with_context(|| format!("writing {}", gltf_path.display()))?;
    let bin_path = gltf_path.with_file_name(bin_name);
    std::fs::write(&bin_path, bin).with_context(|| format!("writing {}", bin_path.display()))?;

    // Copy textures beside the glTF: a URI into the source tree would only load on this machine.
    for uri in image_uris(&mesh) {
        copy_texture_beside_gltf(&uri, obj_path, gltf_path);
    }

    Ok(mesh)
}

/// The `images` entries; the loader matches extensions case-sensitively, so `Wall.JPG` names its type.
fn images_json(image_uris: &[String]) -> String {
    image_uris
        .iter()
        .map(|uri| match image_mime_type(uri) {
            Some(mime) => format!(
                r#"{{ "uri": "{}", "mimeType": "{mime}" }}"#,
                json_escape(uri)
            ),
            None => format!(r#"{{ "uri": "{}" }}"#, json_escape(uri)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The glTF `mimeType` for a texture file name, matched case-insensitively; `None` for formats glTF lacks.
fn image_mime_type(uri: &str) -> Option<&'static str> {
    let extension = Path::new(uri).extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// Refuses a texture glTF cannot carry: the converted file would fail to load as a whole.
fn check_texture_formats(mesh: &ObjMesh) -> Result<()> {
    for material in &mesh.materials {
        for uri in material_texture_uris(material) {
            if image_mime_type(uri).is_none() {
                bail!(
                    "material '{}' uses texture '{uri}', which is neither PNG nor JPEG, the only images glTF carries",
                    material.name
                );
            }
        }
    }
    Ok(())
}

/// Where the texture a .mtl names as `uri` lives: beside the .obj, else in its `textures/` folder.
pub fn texture_source(uri: &str, obj_path: &Path) -> std::path::PathBuf {
    // See https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/model-loading.md
    let beside_mtl = obj_path.with_file_name(uri);
    let under_textures = obj_path.with_file_name(format!("textures/{uri}"));
    if !beside_mtl.exists() && under_textures.exists() {
        under_textures
    } else {
        beside_mtl
    }
}

/// Copies the texture named by `uri` (as written in the .mtl) beside `gltf_path`.
fn copy_texture_beside_gltf(uri: &str, obj_path: &Path, gltf_path: &Path) {
    let source_path = texture_source(uri, obj_path);
    let beside_mtl = obj_path.with_file_name(uri);
    let under_textures = obj_path.with_file_name(format!("textures/{uri}"));
    let destination = gltf_path.with_file_name(uri);
    if source_path == destination {
        return;
    }
    match std::fs::copy(&source_path, &destination) {
        Ok(_) => {}
        Err(error) => {
            // Warn rather than fail: OBJ files routinely reference textures never shipped with them.
            log::warn!(
                "{} (also tried {}): {error}; the converted glTF references a texture that is not there",
                beside_mtl.display(),
                under_textures.display()
            );
        }
    }
}
