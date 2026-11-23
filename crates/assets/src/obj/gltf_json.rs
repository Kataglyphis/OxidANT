//! An OBJ mesh as glTF 2.0 JSON plus its buffer, which `obj2gltf` writes to disk and `obj_to_glb` packs.

use std::path::Path;

use anyhow::{bail, Context, Result};

use super::material::{self, finite_or};
use super::parse::{read_obj, ObjMaterial, ObjMesh};
use super::paths::{self, texture_source};

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
    let [r, g, b, a] = material::base_color(material);
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
        // A `map_d` material stays BLEND here: only the GLB composites its mask and turns it MASK.
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
    // Debug keeps the ".0" of a whole number, which tests pin.
    format!("{:?}", material::metallic(material))
}

fn roughness_factor_json(material: &ObjMaterial) -> String {
    format!("{:?}", material::roughness(material))
}

/// `(emissiveFactor, extensions, uses_emissive_strength)` for one material.
fn emissive_json(material: &ObjMaterial) -> (String, String, bool) {
    let [er, eg, eb] = material::emissive(material);
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
    let emissive_json = if er != 0.0 || eg != 0.0 || eb != 0.0 {
        format!(r#", "emissiveFactor": [{er}, {eg}, {eb}]"#)
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

/// Copies the texture `uri` names (relative to the OBJ's folder) to the same place relative to `gltf_path`'s folder.
fn copy_texture_beside_gltf(uri: &str, obj_path: &Path, gltf_path: &Path) {
    // `uri` passed `paths::placed`, but it is the destination too: a climb out of the output folder is refused again.
    let Some(relative) = paths::inside(uri) else {
        log::warn!("texture '{uri}' leaves the output folder; not copied");
        return;
    };
    // Warn rather than fail: OBJ files routinely reference textures never shipped with them.
    let Some(source_path) = texture_source(uri, obj_path) else {
        return;
    };
    let destination = gltf_path.with_file_name(&relative);
    if destination
        .canonicalize()
        .is_ok_and(|existing| existing == source_path)
    {
        return;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Err(error) = std::fs::copy(&source_path, &destination) {
        log::warn!(
            "{} -> {}: {error}; the converted glTF references a texture that is not there",
            source_path.display(),
            destination.display()
        );
    }
}
