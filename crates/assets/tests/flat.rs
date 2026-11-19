//! The flattened glTF BeschleunigerBallett's engine reads through the cxx bridge.

use std::path::{Path, PathBuf};

use kataglyphis_assets::flat::{load_flat, FlatModel};
use kataglyphis_assets::LoadOptions;

fn renderer_asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../webgpu_renderer/tests/assets")
        .join(name)
}

/// A quad whose second triangle mirrors its UVs: MikkTSpace splits the two shared corners, Lengyel's tangents do not.
fn mirrored_uv_quad() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/mirrored_uv_quad.gltf")
}

const MIKKTSPACE: LoadOptions = LoadOptions {
    mikktspace_tangents: true,
};

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

/// The 60 bytes of `triangle_gltf`'s buffer: three positions, then three uvs.
fn triangle_bytes() -> Vec<u8> {
    let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let uvs: [f32; 6] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    positions
        .iter()
        .chain(&uvs)
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// One unindexed triangle in the xy plane, front face +Z, with the given nodes, materials and extras.
fn triangle_gltf(dir: &Path, name: &str, nodes: &str, extra: &str) -> PathBuf {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{nodes}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}}}}]}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
                     {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}],
        "buffers":[{{"byteLength":60,"uri":"data:application/octet-stream;base64,{}"}}]{extra}}}"#,
        base64(&triangle_bytes())
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
    let model = load_flat(renderer_asset("cube.gltf"), &LoadOptions::default()).unwrap();
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
fn the_load_options_reach_the_tangent_generator() {
    let lengyel = load_flat(mirrored_uv_quad(), &LoadOptions::default()).unwrap();
    let mikktspace = load_flat(mirrored_uv_quad(), &MIKKTSPACE).unwrap();
    assert_consistent(&mikktspace);
    assert_eq!(
        lengyel.vertices.len(),
        4,
        "the default never splits a vertex"
    );
    assert_eq!(
        mikktspace.vertices.len(),
        6,
        "the corners both charts share split"
    );
    let handedness =
        |model: &FlatModel| -> Vec<f32> { model.vertices.iter().map(|v| v.tangent[3]).collect() };
    assert!(handedness(&mikktspace).contains(&-1.0) && handedness(&mikktspace).contains(&1.0));
    let scene = kataglyphis_assets::load_gltf_with(mirrored_uv_quad(), &MIKKTSPACE).unwrap();
    assert_eq!(scene.primitives[0].vertices.len(), 6);
    let bytes = std::fs::read(mirrored_uv_quad()).unwrap();
    let slice = kataglyphis_assets::load_gltf_slice_with(&bytes, &MIKKTSPACE).unwrap();
    assert_eq!(slice.primitives[0].vertices.len(), 6);
    let plain = kataglyphis_assets::load_gltf_slice(&bytes).unwrap();
    assert_eq!(plain.primitives[0].vertices.len(), 4);
}

#[test]
fn a_glb_reads_its_buffer_from_the_binary_chunk() {
    let model = load_flat(renderer_asset("cube.glb"), &LoadOptions::default()).unwrap();
    assert_consistent(&model);
    assert_eq!(model.indices.len(), 36);
}

#[test]
fn a_textured_material_gets_the_encoded_image_and_its_sampler() {
    let model = load_flat(
        renderer_asset("cube_textured.gltf"),
        &LoadOptions::default(),
    )
    .unwrap();
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
    let only = load_flat(renderer_asset("cube_basisu.gltf"), &LoadOptions::default()).unwrap();
    assert!(
        only.textures.is_empty(),
        "stb cannot decode KTX2, so no slot"
    );
    assert_eq!(only.materials[0].base_texture, -1);
    let fallback = load_flat(
        renderer_asset("cube_basisu_fallback.gltf"),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(fallback.textures.len(), 1);
    assert!(fallback.textures[0].encoded.starts_with(b"\x89PNG"));
}

#[test]
fn lights_are_posed_by_their_nodes() {
    let model = load_flat(renderer_asset("point_light.gltf"), &LoadOptions::default()).unwrap();
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
    let plain = load_flat(
        triangle_gltf(dir.path(), "plain.gltf", r#"{"mesh":0}"#, ""),
        &LoadOptions::default(),
    )
    .unwrap();
    let mirrored = load_flat(
        triangle_gltf(
            dir.path(),
            "mirrored.gltf",
            r#"{"mesh":0,"scale":[-1,1,1]}"#,
            "",
        ),
        &LoadOptions::default(),
    )
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
    let skinned = load_flat(
        triangle_gltf(
            dir.path(),
            "skinned.gltf",
            r#"{"mesh":0,"skin":0,"translation":[5,0,0]},{}"#,
            skin,
        ),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(skinned.vertices[0].position, [0.0, 0.0, 0.0]);
    let moved = load_flat(
        triangle_gltf(
            dir.path(),
            "moved.gltf",
            r#"{"mesh":0,"translation":[5,0,0]}"#,
            "",
        ),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(moved.vertices[0].position, [5.0, 0.0, 0.0]);
}

#[test]
fn a_primitive_without_a_material_draws_with_the_neutral_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let model = load_flat(
        triangle_gltf(dir.path(), "bare.gltf", r#"{"mesh":0}"#, ""),
        &LoadOptions::default(),
    )
    .unwrap();
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
    let model = load_flat(
        triangle_gltf(dir.path(), "slots.gltf", r#"{"mesh":0}"#, extra),
        &LoadOptions::default(),
    )
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
    let model = load_flat(
        triangle_gltf(&dir, "escape.gltf", r#"{"mesh":0}"#, extra),
        &LoadOptions::default(),
    )
    .unwrap();
    assert!(model.textures.is_empty());
    assert_eq!(model.materials[0].base_texture, -1);
}

#[test]
fn a_file_without_triangles_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = triangle_gltf(dir.path(), "empty.gltf", r#"{}"#, "");
    let err = load_flat(path, &LoadOptions::default()).unwrap_err();
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
    let flat = load_flat(&path, &LoadOptions::default()).unwrap();
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
    let flat = load_flat(&path, &LoadOptions::default()).unwrap();
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
    let model = load_flat(
        triangle_gltf(dir.path(), "short.gltf", r#"{"mesh":0}"#, extra),
        &LoadOptions::default(),
    )
    .unwrap();
    assert!(model.textures.is_empty());
}

/// Self-contained seeds: every buffer and image is embedded, so a mutant read from a temp dir resolves nothing outside it.
const FUZZ_SEEDS: [&str; 8] = [
    "cube.glb",
    "cube.gltf",
    "cube_textured.gltf",
    "cube_alpha.gltf",
    "cube_normal_transform.gltf",
    "cube_on_plane.gltf",
    "point_light.gltf",
    "skinned_bar.gltf",
];

#[derive(Clone, Debug)]
enum Mutation {
    Overwrite(proptest::sample::Index, u8),
    /// A digit for a digit keeps the JSON parseable and moves a count, offset or index instead.
    Digit(proptest::sample::Index, u8),
    Truncate(proptest::sample::Index),
}

fn mutation() -> impl proptest::strategy::Strategy<Value = Mutation> {
    use proptest::prelude::*;
    prop_oneof![
        3 => (any::<proptest::sample::Index>(), any::<u8>()).prop_map(|(i, b)| Mutation::Overwrite(i, b)),
        6 => (any::<proptest::sample::Index>(), 0u8..10).prop_map(|(i, d)| Mutation::Digit(i, d)),
        1 => any::<proptest::sample::Index>().prop_map(Mutation::Truncate),
    ]
}

fn mutate(mut bytes: Vec<u8>, mutations: &[Mutation]) -> Vec<u8> {
    for m in mutations {
        if bytes.is_empty() {
            break;
        }
        match m {
            Mutation::Overwrite(i, b) => {
                let at = i.index(bytes.len());
                bytes[at] = *b;
            }
            Mutation::Digit(i, d) => {
                let digits: Vec<usize> = (0..bytes.len())
                    .filter(|&k| bytes[k].is_ascii_digit())
                    .collect();
                if !digits.is_empty() {
                    bytes[digits[i.index(digits.len())]] = b'0' + d;
                }
            }
            Mutation::Truncate(i) => bytes.truncate(i.index(bytes.len())),
        }
    }
    bytes
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

    /// The cgltf fuzz target's successor: a mutated file is an error or consistent arrays, never a panic.
    #[test]
    fn a_mutated_file_is_rejected_or_flattens_consistently(
        seed in 0..FUZZ_SEEDS.len(),
        mutations in proptest::collection::vec(mutation(), 1..8),
        mikktspace_tangents in proptest::bool::ANY,
    ) {
        let name = FUZZ_SEEDS[seed];
        let bytes = mutate(std::fs::read(renderer_asset(name)).unwrap(), &mutations);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        if let Ok(model) = load_flat(&path, &LoadOptions { mikktspace_tangents }) {
            assert_consistent(&model);
            let materials = model.materials.len();
            for material in &model.materials {
                for slot in [material.base_texture, material.metallic_roughness_texture, material.normal_texture, material.emissive_texture] {
                    proptest::prop_assert!(slot >= -1 && (slot as i64) < model.textures.len() as i64);
                }
            }
            proptest::prop_assert!(materials > 0, "the neutral fallback is always last");
        }
    }
}

/// Each case once made gltf 1.4.1 panic, which aborts the C++ engine through the bridge; now each is an error.
#[test]
fn accessors_the_reader_would_panic_on_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = triangle_gltf(dir.path(), "base.gltf", r#"{"mesh":0}"#, "");
    let json = std::fs::read_to_string(&path).unwrap();
    let cases = [
        ("dangling POSITION", r#""POSITION":0"#, r#""POSITION":7"#),
        (
            "float indices",
            r#""TEXCOORD_0":1}}"#,
            r#""TEXCOORD_0":1},"indices":1}"#,
        ),
        (
            "VEC3 texcoords",
            r#""count":3,"type":"VEC2""#,
            r#""count":3,"type":"VEC3""#,
        ),
        (
            "no elements",
            r#""count":3,"type":"VEC2""#,
            r#""count":0,"type":"VEC2""#,
        ),
        (
            "overrun",
            r#""count":3,"type":"VEC2""#,
            r#""count":9,"type":"VEC2""#,
        ),
    ];
    for (name, from, to) in cases {
        assert!(json.contains(from), "{name}: the fixture changed");
        let case = dir.path().join("case.gltf");
        std::fs::write(&case, json.replacen(from, to, 1)).unwrap();
        assert!(
            load_flat(&case, &LoadOptions::default()).is_err(),
            "{name} loaded"
        );
    }
    assert!(load_flat(&path, &LoadOptions::default()).is_ok());
}

/// Two primitives on one 1000-vertex POSITION and NORMAL accessor, drawing corners 0, 1, 10 and 987, 997, 998.
fn shared_accessor_gltf(dir: &Path) -> PathBuf {
    let position = |i: u32| [(i % 10) as f32, ((i / 10) % 10) as f32, (i / 100) as f32];
    let mut bytes: Vec<u8> = (0..1000)
        .flat_map(position)
        .flat_map(f32::to_le_bytes)
        .collect();
    bytes.extend(
        (0..1000)
            .flat_map(|_| [0.0f32, 0.0, 1.0])
            .flat_map(f32::to_le_bytes),
    );
    bytes.extend(
        [0u32, 1, 10, 987, 997, 998]
            .iter()
            .flat_map(|i| i.to_le_bytes()),
    );
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}},
                                 {{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":3}}]}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":1000,"type":"VEC3","min":[0,0,0],"max":[9,9,9]}},
                     {{"bufferView":1,"componentType":5126,"count":1000,"type":"VEC3"}},
                     {{"bufferView":2,"componentType":5125,"count":3,"type":"SCALAR"}},
                     {{"bufferView":2,"byteOffset":12,"componentType":5125,"count":3,"type":"SCALAR"}}],
        "bufferViews":[{{"buffer":0,"byteLength":12000}},{{"buffer":0,"byteOffset":12000,"byteLength":12000}},
                       {{"buffer":0,"byteOffset":24000,"byteLength":24}}],
        "buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}]}}"#,
        bytes.len(),
        base64(&bytes)
    );
    let path = dir.join("shared.gltf");
    std::fs::write(&path, json).unwrap();
    path
}

#[test]
fn primitives_sharing_an_accessor_each_keep_only_the_corners_they_draw() {
    let dir = tempfile::tempdir().unwrap();
    let path = shared_accessor_gltf(dir.path());
    let flat = load_flat(&path, &LoadOptions::default()).unwrap();
    assert_consistent(&flat);
    let counts: Vec<usize> = flat.ranges.iter().map(|r| r.vertex_count).collect();
    assert_eq!(counts, [3, 3], "not the whole accessor each");
    assert_eq!(flat.indices, [0, 1, 2, 3, 4, 5]);
    let positions: Vec<[f32; 3]> = flat.vertices.iter().map(|v| v.position).collect();
    assert_eq!(
        positions,
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [7.0, 8.0, 9.0],
            [7.0, 9.0, 9.0],
            [8.0, 9.0, 9.0]
        ],
        "the used corners in file order"
    );
    let scene = kataglyphis_assets::load_gltf(&path).unwrap();
    let counts: Vec<usize> = scene.primitives.iter().map(|p| p.vertices.len()).collect();
    assert_eq!(counts, [3, 3]);
    assert_eq!(scene.primitives[1].indices, [0, 1, 2]);
}

#[test]
fn the_first_directional_light_becomes_the_sun_and_leaves_the_lights() {
    let dir = tempfile::tempdir().unwrap();
    // Node 1 pitches -90 degrees about X, so the Sun's -Z travels straight down.
    let nodes = r#"{"mesh":0,"children":[1,2,3]},
        {"extensions":{"KHR_lights_punctual":{"light":0}},"rotation":[-0.7071068,0,0,0.7071068]},
        {"extensions":{"KHR_lights_punctual":{"light":1}},"translation":[0,2,0]},
        {"extensions":{"KHR_lights_punctual":{"light":0}}}"#;
    let lights = r#","extensionsUsed":["KHR_lights_punctual"],"extensions":{"KHR_lights_punctual":{"lights":[
        {"type":"directional","color":[1,0.9,0.76],"intensity":4098},{"type":"point","intensity":5}]}}"#;
    let flat = load_flat(
        triangle_gltf(dir.path(), "sun.gltf", nodes, lights),
        &LoadOptions::default(),
    )
    .unwrap();
    let sun = flat.sun.expect("a directional light");
    assert_eq!(
        (sun.kind, sun.color, sun.intensity),
        (3, [1.0, 0.9, 0.76], 4098.0)
    );
    assert!((glam::Vec3::from_array(sun.direction) - glam::Vec3::NEG_Y).length() < 1e-5);
    let kinds: Vec<u32> = flat.lights.iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [1, 3],
        "the point light and the second directional one stay"
    );

    // Scaled to nothing, the first directional light has no direction to adopt; it is dropped, not drawn along -Z.
    let hidden = nodes.replacen(
        r#""rotation":[-0.7071068,0,0,0.7071068]"#,
        r#""scale":[0,0,0]"#,
        1,
    );
    let flat = load_flat(
        triangle_gltf(dir.path(), "hidden.gltf", &hidden, lights),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(flat.sun, None);
    let kinds: Vec<u32> = flat.lights.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, [1, 3]);
}

#[test]
fn equal_samplers_share_a_slot_whatever_their_index() {
    let dir = tempfile::tempdir().unwrap();
    let extra = format!(
        r#","images":[{{"uri":"{}"}}],"samplers":[{{"wrapS":33071}},{{"wrapS":33071}},{{}}],
        "textures":[{{"source":0,"sampler":0}},{{"source":0,"sampler":1}},{{"source":0}},{{"source":0,"sampler":2}}],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"emissiveTexture":{{"index":1}}}},
                     {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":2}}}},"emissiveTexture":{{"index":3}}}}]"#,
        embedded_png_uri()
    );
    let model = load_flat(
        triangle_gltf(dir.path(), "samplers.gltf", r#"{"mesh":0}"#, &extra),
        &LoadOptions::default(),
    )
    .unwrap();
    // Two clamping sampler objects are one sampler; no sampler and an empty one are glTF's default.
    assert_eq!(
        (
            model.materials[0].base_texture,
            model.materials[0].emissive_texture
        ),
        (0, 0)
    );
    assert_eq!(
        (
            model.materials[1].base_texture,
            model.materials[1].emissive_texture
        ),
        (1, 1)
    );
    assert_eq!(model.textures.len(), 2);
}

/// `triangle_gltf` whose buffer is the URI `uri` instead of a data URI.
fn triangle_with_buffer(dir: &Path, uri: &str) -> PathBuf {
    let path = triangle_gltf(dir, "external.gltf", r#"{"mesh":0}"#, "");
    let json = std::fs::read_to_string(&path).unwrap();
    let start = json.find("data:application").unwrap();
    let end = start + json[start..].find('"').unwrap();
    std::fs::write(&path, format!("{}{uri}{}", &json[..start], &json[end..])).unwrap();
    path
}

#[test]
fn a_buffer_must_be_a_regular_file_inside_the_document_directory() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("model");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("tri.bin"), triangle_bytes()).unwrap();
    std::fs::write(root.path().join("outside.bin"), triangle_bytes()).unwrap();
    std::fs::create_dir(dir.join("folder.bin")).unwrap();
    let mut short = triangle_bytes();
    short.truncate(58);
    std::fs::write(dir.join("short2.bin"), &short).unwrap();
    short.truncate(56);
    std::fs::write(dir.join("short4.bin"), &short).unwrap();
    let absolute = format!("file://{}", dir.join("tri.bin").display());
    for (uri, loads) in [
        ("tri.bin", true),
        ("sub/../tri.bin", true),
        // gltf pads a buffer to 4 bytes before it checks byteLength, so two bytes short still load.
        ("short2.bin", true),
        ("short4.bin", false),
        ("../outside.bin", false),
        ("folder.bin", false),
        ("missing.bin", false),
        ("%FF.bin", false),
        (absolute.as_str(), false),
        ("https://example.com/tri.bin", false),
    ] {
        let result = load_flat(triangle_with_buffer(&dir, uri), &LoadOptions::default());
        assert_eq!(result.is_ok(), loads, "{uri}: {:?}", result.err());
        assert_eq!(
            kataglyphis_assets::load_gltf(dir.join("external.gltf")).is_ok(),
            loads,
            "{uri}"
        );
    }
    let err = load_flat(
        triangle_with_buffer(&dir, "../outside.bin"),
        &LoadOptions::default(),
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("outside the document's directory"),
        "{err:#}"
    );
    let err = load_flat(
        triangle_with_buffer(&dir, "folder.bin"),
        &LoadOptions::default(),
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("not a regular file"), "{err:#}");
    let err = load_flat(&dir, &LoadOptions::default()).unwrap_err();
    assert!(
        format!("{err:#}").contains("not a regular file"),
        "the document itself: {err:#}"
    );
}

#[test]
fn an_in_memory_import_reads_no_buffer_file_and_never_panics_on_its_uri() {
    let dir = tempfile::tempdir().unwrap();
    for uri in ["tri.bin", "%FF.bin"] {
        let path = triangle_with_buffer(dir.path(), uri);
        let bytes = std::fs::read(path).unwrap();
        assert!(
            kataglyphis_assets::load_gltf_slice(&bytes).is_err(),
            "{uri}"
        );
    }
}

#[test]
fn an_image_whose_escape_is_not_utf8_fails_the_scene_load_instead_of_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let extra = r#","images":[{"uri":"%FF.png"}],"textures":[{"source":0}],
        "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#;
    let path = triangle_gltf(dir.path(), "escape.gltf", r#"{"mesh":0}"#, extra);
    let err = kataglyphis_assets::load_gltf(&path).unwrap_err();
    assert!(format!("{err:#}").contains("not UTF-8"), "{err:#}");
    assert!(
        load_flat(&path, &LoadOptions::default())
            .unwrap()
            .textures
            .is_empty(),
        "the flat load drops the image"
    );
}

#[cfg(unix)]
mod fifos {
    use super::*;

    fn fifo(path: &Path) {
        let made = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(made.success());
    }

    /// Off this thread, so a load that opens a FIFO fails the test instead of hanging it.
    fn finishes<T: Send + 'static>(load: impl FnOnce() -> T + Send + 'static) -> T {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || send.send(load()));
        receive
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the load blocked on a FIFO")
    }

    #[test]
    fn a_fifo_is_never_opened_as_a_document_buffer_or_image() {
        let dir = tempfile::tempdir().unwrap();
        fifo(&dir.path().join("pipe.gltf"));
        fifo(&dir.path().join("pipe.bin"));
        fifo(&dir.path().join("pipe.png"));
        let document = dir.path().join("pipe.gltf");
        assert!(finishes(move || load_flat(document, &LoadOptions::default())).is_err());
        let buffer = triangle_with_buffer(dir.path(), "pipe.bin");
        assert!(finishes(move || load_flat(buffer, &LoadOptions::default())).is_err());

        let extra = r#","images":[{"uri":"pipe.png"}],"textures":[{"source":0}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#;
        let image = triangle_gltf(dir.path(), "image.gltf", r#"{"mesh":0}"#, extra);
        let scene_image = image.clone();
        let flat = finishes(move || load_flat(image, &LoadOptions::default())).unwrap();
        assert!(flat.textures.is_empty(), "the image gets no slot");
        let err = finishes(move || kataglyphis_assets::load_gltf(scene_image)).unwrap_err();
        assert!(format!("{err:#}").contains("not a regular file"), "{err:#}");

        // A file: URI names the FIFO by its absolute path, which gltf's reader opens as written.
        let pipe = dir.path().join("pipe.png");
        for uri in [
            format!("file://{}", pipe.display()),
            format!("file:{}", pipe.display()),
        ] {
            let extra = extra.replacen("\"pipe.png\"", &format!("\"{uri}\""), 1);
            let image = triangle_gltf(dir.path(), "file_uri.gltf", r#"{"mesh":0}"#, &extra);
            let scene_image = image.clone();
            let flat = finishes(move || load_flat(image, &LoadOptions::default())).unwrap();
            assert!(flat.textures.is_empty(), "{uri}: the image gets no slot");
            let err = finishes(move || kataglyphis_assets::load_gltf(scene_image)).unwrap_err();
            assert!(
                format!("{err:#}").contains("not a regular file"),
                "{uri}: {err:#}"
            );
        }
    }
}
