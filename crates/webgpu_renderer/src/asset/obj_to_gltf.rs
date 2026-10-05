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

        match keyword {
            "newmtl" => materials.push(ObjMaterial {
                name: values.first().copied().unwrap_or("unnamed").to_string(),
                ..ObjMaterial::default()
            }),
            "Kd" if values.len() >= 3 => {
                if let Some(material) = materials.last_mut() {
                    for (axis, value) in values.iter().take(3).enumerate() {
                        if let Ok(component) = value.parse::<f32>() {
                            material.base_color[axis] = component;
                        }
                    }
                }
            }
            // Stored unclamped: `to_gltf` carries values above 1 via KHR_materials_emissive_strength.
            "Ke" if values.len() >= 3 => {
                if let Some(material) = materials.last_mut() {
                    for (axis, value) in values.iter().take(3).enumerate() {
                        if let Ok(component) = value.parse::<f32>() {
                            material.emissive[axis] = component;
                        }
                    }
                }
            }
            // `d` is opacity and `Tr` its inverse; mixing them up swaps opaque and transparent.
            "d" if !values.is_empty() => {
                if let (Some(material), Ok(opacity)) =
                    (materials.last_mut(), values[0].parse::<f32>())
                {
                    material.base_color[3] = opacity.clamp(0.0, 1.0);
                }
            }
            "map_Kd" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    // Last token, past any options; `\` becomes `/` so Windows-authored paths resolve on Linux.
                    material.base_color_texture = values.last().map(|name| name.replace('\\', "/"));
                }
            }
            // Same last-token, backslash-normalising rule as `map_Kd`.
            "map_Ke" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    material.emissive_texture = values.last().map(|name| name.replace('\\', "/"));
                }
            }
            "Tr" if !values.is_empty() => {
                if let (Some(material), Ok(transparency)) =
                    (materials.last_mut(), values[0].parse::<f32>())
                {
                    material.base_color[3] = (1.0 - transparency).clamp(0.0, 1.0);
                }
            }
            "Pm" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    if let Ok(metallic) = values[0].parse::<f32>() {
                        material.metallic = Some(metallic);
                    }
                }
            }
            "Pr" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    if let Ok(roughness) = values[0].parse::<f32>() {
                        material.roughness = Some(roughness);
                    }
                }
            }
            "Ns" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    if let Ok(shininess) = values[0].parse::<f32>() {
                        material.shininess = Some(shininess);
                    }
                }
            }
            // `norm` wins whatever the order; bump directives only fill an empty slot.
            "norm" | "map_Bump" | "map_bump" | "bump" if !values.is_empty() => {
                if let Some(material) = materials.last_mut() {
                    if keyword == "norm" || material.normal_texture.is_none() {
                        material.normal_texture = values.last().map(|name| name.replace('\\', "/"));
                        if let Some(scale) = bump_scale_option(&values) {
                            material.normal_scale = scale;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    materials
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
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();

    let mut mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    // Without `vn`, corners at one position share a vertex and one flat normal, as in the C++ loaders.
    let mut seen: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut active_material: usize = 0;
    let mut run_start: u32 = 0;

    for (line_number, raw_line) in source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        let values: Vec<&str> = parts.collect();
        let at = line_number + 1;

        match keyword {
            "v" => {
                if values.len() != 3 && values.len() != 6 && values.len() != 7 {
                    bail!(
                        "line {at}: 'v' needs 3, 6 or 7 components, got {}",
                        values.len()
                    );
                }
                positions.push([
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
                    colors.push([
                        parse_f32(values[3], at)?,
                        parse_f32(values[4], at)?,
                        parse_f32(values[5], at)?,
                        alpha,
                    ]);
                    mesh.has_vertex_colors = true;
                } else {
                    colors.push([1.0, 1.0, 1.0, 1.0]);
                }
            }
            "vn" => {
                if values.len() < 3 {
                    bail!("line {at}: 'vn' needs 3 components, got {}", values.len());
                }
                normals.push([
                    parse_f32(values[0], at)?,
                    parse_f32(values[1], at)?,
                    parse_f32(values[2], at)?,
                ]);
                mesh.has_normals = true;
            }
            "vt" => {
                if values.len() < 2 {
                    bail!("line {at}: 'vt' needs 2 components, got {}", values.len());
                }
                // OBJ's V points up and glTF's down; flipping here keeps the asset right for any consumer.
                uvs.push([parse_f32(values[0], at)?, 1.0 - parse_f32(values[1], at)?]);
            }
            "f" => {
                if values.len() < 3 {
                    bail!(
                        "line {at}: 'f' needs at least 3 vertices, got {}",
                        values.len()
                    );
                }
                let mut face: Vec<u32> = Vec::with_capacity(values.len());
                for value in &values {
                    let key = parse_face_vertex(value, at)?;
                    let index = match seen.get(&key) {
                        Some(&existing) => existing,
                        None => {
                            let position = resolve(key.0, positions.len(), at, "position")?;
                            mesh.positions.push(positions[position]);
                            mesh.colors.push(colors[position]);

                            if key.1 > 0 {
                                let uv = resolve(key.1, uvs.len(), at, "texcoord")?;
                                mesh.uvs.push(uvs[uv]);
                            } else {
                                mesh.uvs.push([0.0, 0.0]);
                            }

                            if key.2 > 0 {
                                let normal = resolve(key.2, normals.len(), at, "normal")?;
                                mesh.normals.push(normals[normal]);
                            } else {
                                // Zero marks the corner for `fill_missing_flat_normals`.
                                mesh.normals.push([0.0, 0.0, 0.0]);
                            }

                            let index = (mesh.positions.len() - 1) as u32;
                            seen.insert(key, index);
                            index
                        }
                    };
                    face.push(index);
                }

                // Fan triangulation: correct for the convex faces OBJ exporters emit.
                for i in 1..face.len() - 1 {
                    mesh.indices
                        .extend_from_slice(&[face[0], face[i], face[i + 1]]);
                }
            }
            "usemtl" => {
                // Skip empty runs, which would become primitives drawing nothing.
                let current = mesh.indices.len() as u32;
                if current > run_start {
                    mesh.submeshes
                        .push((run_start, current - run_start, active_material));
                    run_start = current;
                }
                let name = values.first().copied().unwrap_or("");
                active_material = mesh
                    .materials
                    .iter()
                    .position(|material| material.name == name)
                    .unwrap_or_else(|| {
                        // Undeclared: keep the name so the mismatch shows instead of collapsing onto material 0.
                        mesh.materials.push(ObjMaterial {
                            name: name.to_string(),
                            ..ObjMaterial::default()
                        });
                        mesh.materials.len() - 1
                    });
            }
            // Ignored rather than fatal: almost every real OBJ carries them.
            "mtllib" | "o" | "g" | "s" => {}
            other => {
                bail!("line {at}: unsupported OBJ directive '{other}'");
            }
        }
    }

    // Close the final run.
    let total = mesh.indices.len() as u32;
    if total > run_start {
        mesh.submeshes
            .push((run_start, total - run_start, active_material));
    }

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
    let mut bin: Vec<u8> = Vec::new();

    let positions_offset = bin.len();
    for p in &mesh.positions {
        for component in p {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    let normals_offset = bin.len();
    for n in &mesh.normals {
        for component in n {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    let uvs_offset = bin.len();
    for uv in &mesh.uvs {
        for component in uv {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    // Colours only when the source had them, so a colourless OBJ's buffer layout stays unchanged.
    let colors_offset = bin.len();
    if mesh.has_vertex_colors {
        for c in &mesh.colors {
            for component in c {
                bin.extend_from_slice(&component.to_le_bytes());
            }
        }
    }
    // u32 indices need 4-byte alignment; padded in case a non-float attribute is ever added.
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let indices_offset = bin.len();
    for index in &mesh.indices {
        bin.extend_from_slice(&index.to_le_bytes());
    }

    let (min, max) = mesh.bounds();
    let vertex_count = mesh.positions.len();
    let index_count = mesh.indices.len();

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

    // Deduplicated, or a shared map is decoded and uploaded once per material.
    let mut image_uris: Vec<String> = Vec::new();
    for material in &mesh.materials {
        for uri in [
            &material.base_color_texture,
            &material.normal_texture,
            &material.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            if !image_uris.iter().any(|existing| existing == uri) {
                image_uris.push(uri.clone());
            }
        }
    }

    let images_json = image_uris
        .iter()
        .map(|uri| format!(r#"{{ "uri": "{}" }}"#, json_escape(uri)))
        .collect::<Vec<_>>()
        .join(", ");
    let textures_json = (0..image_uris.len())
        .map(|index| format!(r#"{{ "source": {index}, "sampler": 0 }}"#))
        .collect::<Vec<_>>()
        .join(", ");
    // Explicit REPEAT (10497), OBJ's tiling convention, rather than relying on loader defaults.
    let samplers_json = if image_uris.is_empty() {
        String::new()
    } else {
        r#"{ "wrapS": 10497, "wrapT": 10497 }"#.to_string()
    };

    let material_entries: Vec<(String, bool)> = mesh
        .materials
        .iter()
        .map(|material| {
            // Non-finite colour falls back to glTF's default white.
            let [r, g, b, a] = material.base_color.map(|component| finite_or(component, 1.0));
            // Plain-diffuse fallback as literal "0.0"/"1.0" (`Display` prints "0"/"1"); tests pin the output.
            let metallic_factor = match material.metallic {
                Some(value) => format!("{}", finite_or(value, 0.0)),
                None => "0.0".to_string(),
            };
            // Without Pr, Ns maps through the C++ material_roughness() curve so both renderers agree.
            let roughness_factor = match material.roughness {
                Some(value) => format!("{}", finite_or(value, 1.0)),
                None => match material.shininess {
                    Some(shininess) if shininess.is_finite() && shininess >= 0.0 => {
                        format!("{}", (2.0 / (shininess + 2.0)).sqrt().clamp(0.045, 1.0))
                    }
                    _ => "1.0".to_string(),
                },
            };
            let texture_json = match &material.base_color_texture {
                Some(uri) => {
                    let index = image_uris.iter().position(|existing| existing == uri).unwrap_or(0);
                    format!(r#", "baseColorTexture": {{ "index": {index} }}"#)
                }
                None => String::new(),
            };
            let normal_texture_json = match &material.normal_texture {
                Some(uri) => {
                    let index = image_uris.iter().position(|existing| existing == uri).unwrap_or(0);
                    let scale = finite_or(material.normal_scale, 1.0);
                    format!(r#", "normalTexture": {{ "index": {index}, "scale": {scale} }}"#)
                }
                None => String::new(),
            };
            let emissive_texture_json = match &material.emissive_texture {
                Some(uri) => {
                    let index = image_uris.iter().position(|existing| existing == uri).unwrap_or(0);
                    format!(r#", "emissiveTexture": {{ "index": {index} }}"#)
                }
                None => String::new(),
            };
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
            let json = format!(
                r#"{{ "name": "{}", "pbrMetallicRoughness": {{ "baseColorFactor": [{}, {}, {}, {}]{}, "metallicFactor": {}, "roughnessFactor": {} }}{}{}{}{}{} }}"#,
                json_escape(&material.name),
                r, g, b, a,
                texture_json,
                metallic_factor,
                roughness_factor,
                if a < 1.0 { r#", "alphaMode": "BLEND""# } else { "" },
                normal_texture_json,
                emissive_texture_json,
                emissive_json,
                extensions_json
            );
            (json, uses_emissive_strength)
        })
        .collect();

    let materials_json = material_entries
        .iter()
        .map(|(json, _)| json.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // Only when a material needs it: tests compare the generated JSON.
    let extensions_used_json = if material_entries.iter().any(|(_, used)| *used) {
        r#",
  "extensionsUsed": ["KHR_materials_emissive_strength"]"#
            .to_string()
    } else {
        String::new()
    };

    let color_buffer_view_json = if mesh.has_vertex_colors {
        format!(
            r#",
    {{ "buffer": 0, "byteOffset": {colors_offset}, "byteLength": {colors_length}, "target": 34962 }}"#,
            colors_offset = colors_offset,
            colors_length = vertex_count * 16,
        )
    } else {
        String::new()
    };
    let color_accessor_json = if mesh.has_vertex_colors {
        format!(
            r#",
    {{ "bufferView": 3, "componentType": 5126, "count": {vertex_count}, "type": "VEC4" }}"#,
            vertex_count = vertex_count,
        )
    } else {
        String::new()
    };

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
}}"#,
        bin_uri = bin_uri,
        buffer_length = bin.len(),
        positions_offset = positions_offset,
        positions_length = vertex_count * 12,
        normals_offset = normals_offset,
        normals_length = vertex_count * 12,
        uvs_offset = uvs_offset,
        uvs_length = vertex_count * 8,
        color_buffer_view_json = color_buffer_view_json,
        color_accessor_json = color_accessor_json,
        indices_offset = indices_offset,
        indices_length = index_count * 4,
        vertex_count = vertex_count,
        index_accessors = index_accessors,
        primitives = primitives,
        materials_json = materials_json,
        extensions_used_json = extensions_used_json,
        texture_arrays = if image_uris.is_empty() {
            String::new()
        } else {
            format!(
                "
  \"images\": [{images_json}],
  \"samplers\": [{samplers_json}],
  \"textures\": [{textures_json}],"
            )
        },
        // An empty mesh's bounds are +/-infinity, which is not valid JSON.
        min0 = finite_or(min[0], 0.0),
        min1 = finite_or(min[1], 0.0),
        min2 = finite_or(min[2], 0.0),
        max0 = finite_or(max[0], 0.0),
        max1 = finite_or(max[1], 0.0),
        max2 = finite_or(max[2], 0.0),
    );

    (json, bin)
}

/// Converts `obj_path` to `gltf_path`, writing the binary buffer alongside it.
pub fn convert_file(obj_path: &Path, gltf_path: &Path) -> Result<ObjMesh> {
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

    let mesh = parse_obj_with_materials(&source, materials)
        .with_context(|| format!("parsing {}", obj_path.display()))?;

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
    let mut copied: Vec<&str> = Vec::new();
    for material in &mesh.materials {
        for uri in [
            &material.base_color_texture,
            &material.normal_texture,
            &material.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            if copied.iter().any(|existing| *existing == uri) {
                continue;
            }
            copied.push(uri);
            copy_texture_beside_gltf(uri, obj_path, gltf_path);
        }
    }

    Ok(mesh)
}

/// Copies the texture named by `uri` (as written in the .mtl) beside `gltf_path`.
fn copy_texture_beside_gltf(uri: &str, obj_path: &Path, gltf_path: &Path) {
    // See https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/model-loading.md
    let beside_mtl = obj_path.with_file_name(uri);
    let under_textures = obj_path.with_file_name(format!("textures/{uri}"));
    let source_path = if beside_mtl.exists() {
        beside_mtl.clone()
    } else if under_textures.exists() {
        under_textures.clone()
    } else {
        beside_mtl.clone()
    };
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
