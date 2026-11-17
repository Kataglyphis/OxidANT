//! The flattened glTF BeschleunigerBallett's engine reads through the cxx bridge.

use std::path::{Path, PathBuf};

use kataglyphis_assets::flat::{load_flat, FlatModel};

fn renderer_asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../webgpu_renderer/tests/assets")
        .join(name)
}

fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

/// One unindexed triangle in the xy plane, front face +Z, with the given nodes, materials and extras.
fn triangle_gltf(dir: &Path, name: &str, nodes: &str, extra: &str) -> PathBuf {
    let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let uvs: [f32; 6] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let bytes: Vec<u8> = positions
        .iter()
        .chain(&uvs)
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{nodes}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}}}}]}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
                     {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}],
        "buffers":[{{"byteLength":60,"uri":"data:application/octet-stream;base64,{}"}}]{extra}}}"#,
        base64(&bytes)
    );
    let path = dir.join(name);
    std::fs::write(&path, json).unwrap();
    path
}

fn assert_consistent(model: &FlatModel) {
    assert_eq!(model.indices.len() % 3, 0);
    assert_eq!(model.material_index.len(), model.indices.len() / 3);
    assert!(model
        .indices
        .iter()
        .all(|&i| (i as usize) < model.vertices.len()));
    assert!(model
        .material_index
        .iter()
        .all(|&m| (m as usize) < model.materials.len()));
    let (mut vertices, mut indices, mut triangles) = (0, 0, 0);
    for range in &model.ranges {
        assert_eq!(
            (range.vertex_base, range.index_start, range.tri_start),
            (vertices, indices, triangles)
        );
        vertices += range.vertex_count;
        indices += range.index_count;
        triangles += range.tri_count;
    }
    assert_eq!(
        (vertices, indices, triangles),
        (
            model.vertices.len(),
            model.indices.len(),
            model.material_index.len()
        )
    );
}

#[test]
fn the_cube_flattens_into_consistent_arrays() {
    let model = load_flat(renderer_asset("cube.gltf")).unwrap();
    assert_consistent(&model);
    assert_eq!(model.ranges.len(), 1);
    assert_eq!(model.indices.len(), 36);
    let fallback = model.materials.len() as u32 - 1;
    assert!(model.material_index.iter().all(|&m| m < fallback));
    for v in &model.vertices {
        let n = glam::Vec3::from_array(v.normal);
        assert!((n.length() - 1.0).abs() < 1e-4, "normal {n:?}");
        assert!(v.tangent[3] == 1.0 || v.tangent[3] == -1.0);
    }
}

#[test]
fn a_textured_material_gets_the_encoded_image_and_its_sampler() {
    let model = load_flat(renderer_asset("cube_textured.gltf")).unwrap();
    assert_eq!(model.textures.len(), 1);
    let texture = &model.textures[0];
    assert!(texture.encoded.starts_with(b"\x89PNG"), "bytes as stored");
    assert!(texture.srgb);
    assert!(texture.sampler.mag_nearest && texture.sampler.min_nearest);
    assert!(!texture.sampler.mipmaps, "bare NEAREST reads level 0");
    assert_eq!(model.materials[0].base_texture, 0);
    assert_eq!(model.materials[0].normal_texture, -1);
}

#[test]
fn a_basisu_only_texture_has_no_slot_and_a_fallback_uses_its_core_image() {
    let only = load_flat(renderer_asset("cube_basisu.gltf")).unwrap();
    assert!(
        only.textures.is_empty(),
        "stb cannot decode KTX2, so no slot"
    );
    assert_eq!(only.materials[0].base_texture, -1);
    let fallback = load_flat(renderer_asset("cube_basisu_fallback.gltf")).unwrap();
    assert_eq!(fallback.textures.len(), 1);
    assert!(fallback.textures[0].encoded.starts_with(b"\x89PNG"));
}

#[test]
fn lights_are_posed_by_their_nodes() {
    let model = load_flat(renderer_asset("point_light.gltf")).unwrap();
    assert_eq!(model.lights.len(), 2);
    let point = &model.lights[0];
    assert_eq!(point.kind, 1);
    assert_eq!(point.position, [-1.2, 1.0, 0.0]);
    assert_eq!(
        (point.cos_inner, point.cos_outer, point.range),
        (1.0, 1.0, 6.0)
    );
    let spot = &model.lights[1];
    assert_eq!(spot.kind, 2);
    assert!((spot.cos_outer - 0.6f32.cos()).abs() < 1e-6);
    // The node pitches -90 degrees about X, so its -Z points down.
    assert!((glam::Vec3::from_array(spot.direction) - glam::Vec3::NEG_Y).length() < 1e-5);
}

#[test]
fn a_mirroring_node_reverses_the_winding_and_flips_the_tangent_handedness() {
    let dir = tempfile::tempdir().unwrap();
    let plain = load_flat(triangle_gltf(dir.path(), "plain.gltf", r#"{"mesh":0}"#, "")).unwrap();
    let mirrored = load_flat(triangle_gltf(
        dir.path(),
        "mirrored.gltf",
        r#"{"mesh":0,"scale":[-1,1,1]}"#,
        "",
    ))
    .unwrap();
    assert_eq!(plain.indices, [0, 1, 2]);
    assert_eq!(mirrored.indices, [0, 2, 1]);
    assert_eq!(mirrored.vertices[1].position, [-1.0, 0.0, 0.0]);
    assert_eq!(
        plain.vertices[0].tangent[3],
        -mirrored.vertices[0].tangent[3]
    );
    // Both faces still face +Z, the flat normal the file left out.
    for model in [&plain, &mirrored] {
        assert!(model.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
    }
}

#[test]
fn a_skinned_node_keeps_its_bind_pose_and_a_plain_one_moves() {
    let dir = tempfile::tempdir().unwrap();
    let skin = r#","skins":[{"joints":[1]}]"#;
    let skinned = load_flat(triangle_gltf(
        dir.path(),
        "skinned.gltf",
        r#"{"mesh":0,"skin":0,"translation":[5,0,0]},{}"#,
        skin,
    ))
    .unwrap();
    assert_eq!(skinned.vertices[0].position, [0.0, 0.0, 0.0]);
    let moved = load_flat(triangle_gltf(
        dir.path(),
        "moved.gltf",
        r#"{"mesh":0,"translation":[5,0,0]}"#,
        "",
    ))
    .unwrap();
    assert_eq!(moved.vertices[0].position, [5.0, 0.0, 0.0]);
}

#[test]
fn a_primitive_without_a_material_draws_with_the_neutral_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let model = load_flat(triangle_gltf(dir.path(), "bare.gltf", r#"{"mesh":0}"#, "")).unwrap();
    assert_eq!(model.materials.len(), 1, "only the fallback");
    assert_eq!(model.material_index, [0]);
    assert_consistent(&model);
}

#[test]
fn slots_dedup_on_image_sampler_and_colour_space() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.png"), b"\x89PNG a").unwrap();
    std::fs::write(dir.path().join("b.png"), b"\x89PNG b").unwrap();
    let extra = r#","images":[{"uri":"a.png"},{"uri":"b.png"}],"samplers":[{},{"wrapS":33071}],
        "textures":[{"source":0},{"source":0},{"source":0,"sampler":1},{"source":1}],
        "materials":[
          {"pbrMetallicRoughness":{"baseColorTexture":{"index":0}},"normalTexture":{"index":1}},
          {"pbrMetallicRoughness":{"baseColorTexture":{"index":1}},"emissiveTexture":{"index":0}},
          {"pbrMetallicRoughness":{"baseColorTexture":{"index":2},"metallicRoughnessTexture":{"index":3}}}]"#;
    let model = load_flat(triangle_gltf(
        dir.path(),
        "slots.gltf",
        r#"{"mesh":0}"#,
        extra,
    ))
    .unwrap();
    let ids: Vec<[i32; 4]> = model
        .materials
        .iter()
        .map(|m| {
            [
                m.base_texture,
                m.metallic_roughness_texture,
                m.normal_texture,
                m.emissive_texture,
            ]
        })
        .collect();
    // One image and sampler share a slot across textures; a linear use or another sampler gets its own.
    assert_eq!(ids[0], [0, -1, 1, -1]);
    assert_eq!(ids[1], [0, -1, -1, 0]);
    assert_eq!(ids[2], [2, 3, -1, -1]);
    assert_eq!(model.textures.len(), 4);
    assert_eq!(model.textures[3].encoded, b"\x89PNG b");
    assert!(!model.textures[1].srgb && !model.textures[3].srgb);
}

#[test]
fn an_image_outside_the_document_directory_gets_no_slot() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("model");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(root.path().join("secret.png"), b"\x89PNG secret").unwrap();
    let extra = r#","images":[{"uri":"../secret.png"},{"uri":"https://example.com/a.png"},{"uri":"missing.png"}],
        "textures":[{"source":0},{"source":1},{"source":2}],
        "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}},"normalTexture":{"index":1},"emissiveTexture":{"index":2}}]"#;
    let model = load_flat(triangle_gltf(&dir, "escape.gltf", r#"{"mesh":0}"#, extra)).unwrap();
    assert!(model.textures.is_empty());
    assert_eq!(model.materials[0].base_texture, -1);
}

#[test]
fn a_file_without_triangles_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = triangle_gltf(dir.path(), "empty.gltf", r#"{}"#, "");
    let err = load_flat(path).unwrap_err();
    assert!(
        format!("{err:#}").contains("no triangle primitives"),
        "{err:#}"
    );
}

/// The PNG data URI `cube_textured.gltf` embeds, so a test material has a decodable image.
fn embedded_png_uri() -> String {
    let json = std::fs::read_to_string(renderer_asset("cube_textured.gltf")).unwrap();
    let start = json.find("data:image/png;base64,").unwrap();
    let end = start + json[start..].find('"').unwrap();
    json[start..end].to_owned()
}

#[test]
fn a_texture_transform_tex_coord_overrides_the_slots_uv_set() {
    let dir = tempfile::tempdir().unwrap();
    let transform = r#""extensions":{"KHR_texture_transform":{"texCoord":1}}"#;
    let extra = format!(
        r#","images":[{{"uri":"{}"}}],"textures":[{{"source":0}}],"extensionsUsed":["KHR_texture_transform"],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0,{transform}}}}},
                       "normalTexture":{{"index":0,{transform}}},"occlusionTexture":{{"index":0,"texCoord":1}}}}]"#,
        embedded_png_uri()
    );
    let nodes = r#"{"mesh":0}"#;
    let path = triangle_gltf(dir.path(), "uv1.gltf", nodes, &extra);
    let json = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        json.replace(r#""attributes":{"#, r#""material":0,"attributes":{"#),
    )
    .unwrap();
    let flat = load_flat(&path).unwrap();
    assert_eq!(
        flat.materials[0].uv1_slot_mask, 0b0101,
        "base colour and normal"
    );
    let scene = kataglyphis_assets::load_gltf(&path).unwrap();
    assert_eq!(
        scene.primitives[0].material.uv_set_mask, 0b1_0101,
        "plus occlusion"
    );
}

#[test]
fn a_document_without_scenes_loads_every_node_posed_by_its_parents() {
    let dir = tempfile::tempdir().unwrap();
    let nodes = r#"{"translation":[0,0,3],"children":[1]},{"mesh":0,"translation":[1,0,0]}"#;
    let path = triangle_gltf(dir.path(), "sceneless.gltf", nodes, "");
    let json = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        json.replace(r#""scene":0,"scenes":[{"nodes":[0]}],"#, ""),
    )
    .unwrap();
    let flat = load_flat(&path).unwrap();
    assert_eq!(flat.vertices[0].position, [1.0, 0.0, 3.0]);
    assert_eq!(flat.ranges.len(), 1, "the child is not visited twice");
    let scene = kataglyphis_assets::load_gltf(&path).unwrap();
    assert_eq!(scene.primitives.len(), 1);
}

#[test]
fn a_truncated_base64_image_gets_no_slot() {
    let dir = tempfile::tempdir().unwrap();
    let extra = r#","images":[{"uri":"data:image/png;base64,QQ"}],"textures":[{"source":0}],
        "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#;
    let model = load_flat(triangle_gltf(
        dir.path(),
        "short.gltf",
        r#"{"mesh":0}"#,
        extra,
    ))
    .unwrap();
    assert!(model.textures.is_empty());
}
