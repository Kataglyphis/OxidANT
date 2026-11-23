//! The flattened glTF BeschleunigerBallett's engine reads through the cxx bridge.

use std::path::{Path, PathBuf};

use kataglyphis_assets::flat::{load_flat, FlatModel};
use kataglyphis_assets::LoadOptions;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name)
}

/// A quad whose second triangle mirrors its UVs: MikkTSpace splits the two shared corners, Lengyel's tangents do not.
fn mirrored_uv_quad() -> PathBuf {
    fixture("mirrored_uv_quad.gltf")
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
    let model = load_flat(fixture("cube.gltf"), &LoadOptions::default()).unwrap();
    assert_consistent(&model);
    assert_eq!(model.ranges.len(), 1);
    assert_eq!(model.indices.len(), 36);
    let fallback = model.materials.len() as u32 - 1;
    assert!(model.material_index.iter().all(|&m| m < fallback));
    for v in &model.vertices {
        let n = glam::Vec3::from_array(v.normal);
        assert!((n.length() - 1.0).abs() < 1e-4, "normal {n:?}");
        assert!(v.tangent[3] == 1.0 || v.tangent[3] == -1.0);
        let t = glam::Vec3::from_slice(&v.tangent[..3]);
        assert!(
            (t.length() - 1.0).abs() < 1e-4 && t.dot(n).abs() < 1e-4,
            "tangent {t:?}"
        );
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
    let model = load_flat(fixture("cube.glb"), &LoadOptions::default()).unwrap();
    assert_consistent(&model);
    assert_eq!(model.indices.len(), 36);
}

#[test]
fn a_textured_material_gets_the_encoded_image_and_its_sampler() {
    let model = load_flat(fixture("cube_textured.gltf"), &LoadOptions::default()).unwrap();
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
    let only = load_flat(fixture("cube_basisu.gltf"), &LoadOptions::default()).unwrap();
    assert!(
        only.textures.is_empty(),
        "stb cannot decode KTX2, so no slot"
    );
    assert_eq!(only.materials[0].base_texture, -1);
    let fallback = load_flat(
        fixture("cube_basisu_fallback.gltf"),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(fallback.textures.len(), 1);
    assert!(fallback.textures[0].encoded.starts_with(b"\x89PNG"));
}

#[test]
fn lights_are_posed_by_their_nodes() {
    let model = load_flat(fixture("point_light.gltf"), &LoadOptions::default()).unwrap();
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

/// A skinned triangle: vertices 0 and 1 on the skin's one joint, node 1, vertex 2 unweighted; `ibm` is column-major.
fn skinned_triangle(dir: &Path, nodes: &str, ibm: [f32; 16]) -> PathBuf {
    let mut bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    bytes.extend(f32_bytes(&[0.0f32, 0.0, 1.0].repeat(3)));
    bytes.extend(f32_bytes(&[1.0f32, 0.0, 0.0, 1.0].repeat(3)));
    bytes.extend([0u8; 12]);
    bytes.extend(f32_bytes(&[
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ]));
    bytes.extend(f32_bytes(&ibm));
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_lights_punctual"],
        "extensions":{{"KHR_lights_punctual":{{"lights":[{{"type":"point"}}]}}}},
        "scene":0,"scenes":[{{"nodes":[0,1]}}],"nodes":[{nodes}],"skins":[{{"joints":[1],"inverseBindMatrices":5}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"JOINTS_0":3,"WEIGHTS_0":4}}}}]}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
          {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},
          {{"bufferView":3,"componentType":5121,"count":3,"type":"VEC4"}},{{"bufferView":4,"componentType":5126,"count":3,"type":"VEC4"}},
          {{"bufferView":5,"componentType":5126,"count":1,"type":"MAT4"}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},
          {{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":12}},
          {{"buffer":0,"byteOffset":132,"byteLength":48}},{{"buffer":0,"byteOffset":180,"byteLength":64}}],
        "buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}]}}"#,
        bytes.len(),
        base64(&bytes)
    );
    let path = dir.join("skinned_triangle.gltf");
    std::fs::write(&path, json).unwrap();
    path
}

const SCALE_2: [f32; 16] = [
    2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn assert_near(got: &[f32], want: &[f32], what: &str) {
    let off = got
        .iter()
        .zip(want)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(off < 1e-5, "{what}: {got:?}, expected {want:?}");
}

/// The engine draws no animation, so a skin arrives at its joints' rest pose; its node's transform is ignored.
#[test]
fn a_skinned_mesh_is_baked_at_its_joints_rest_pose() {
    let dir = tempfile::tempdir().unwrap();
    // Joint 1 turns 90 degrees about z and lifts by 3, over an inverse bind that doubles the mesh.
    let nodes = r#"{"mesh":0,"skin":0,"translation":[5,0,0],"extensions":{"KHR_lights_punctual":{"light":0}}},
        {"translation":[0,3,0],"rotation":[0,0,0.70710677,0.70710677]}"#;
    let model = load_flat(
        skinned_triangle(dir.path(), nodes, SCALE_2),
        &LoadOptions::default(),
    )
    .unwrap();
    let [v0, v1, v2] = [0, 1, 2].map(|i| model.vertices[i]);
    assert_near(&v0.position, &[0.0, 3.0, 0.0], "vertex 0 follows the joint");
    assert_near(
        &v1.position,
        &[0.0, 5.0, 0.0],
        "vertex 1 is doubled, turned and lifted",
    );
    assert_near(
        &v1.tangent,
        &[0.0, 1.0, 0.0, 1.0],
        "the tangent turns with the joint",
    );
    assert_near(
        &v1.normal,
        &[0.0, 0.0, 1.0],
        "the normal about the turn's axis stays",
    );
    assert_near(
        &v2.position,
        &[0.0, 1.0, 0.0],
        "an unweighted vertex ignores the node's translation",
    );
    assert_near(
        &v2.tangent,
        &[1.0, 0.0, 0.0, 1.0],
        "an unweighted vertex keeps its frame",
    );
    let lights: Vec<[f32; 3]> = model.lights.iter().map(|l| l.position).collect();
    assert_eq!(
        lights,
        [[5.0, 0.0, 0.0]],
        "only the mesh ignores the skinned node's transform"
    );

    let mirrored = r#"{"mesh":0,"skin":0},{"scale":[-1,1,1]}"#;
    let model = load_flat(
        skinned_triangle(dir.path(), mirrored, IDENTITY),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_near(
        &model.vertices[1].position,
        &[-1.0, 0.0, 0.0],
        "the mirrored vertex",
    );
    assert_near(
        &model.vertices[1].tangent,
        &[-1.0, 0.0, 0.0, -1.0],
        "a mirroring joint flips the handedness",
    );
}

/// `load_flat` reads a skin's inverse binds now; gltf 1.4.1's reader panics on a malformed accessor instead of failing.
#[test]
fn inverse_bind_accessors_the_reader_would_panic_on_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let nodes = r#"{"mesh":0,"skin":0},{}"#;
    let json = std::fs::read_to_string(skinned_triangle(dir.path(), nodes, IDENTITY)).unwrap();
    let matrices = r#""componentType":5126,"count":1,"type":"MAT4""#;
    assert!(json.contains(matrices), "the fixture changed");
    for (name, broken) in [
        ("VEC4", r#""componentType":5126,"count":1,"type":"VEC4""#),
        ("bytes", r#""componentType":5121,"count":1,"type":"MAT4""#),
        (
            "no elements",
            r#""componentType":5126,"count":0,"type":"MAT4""#,
        ),
        ("overrun", r#""componentType":5126,"count":2,"type":"MAT4""#),
    ] {
        let case = dir.path().join("case.gltf");
        std::fs::write(&case, json.replacen(matrices, broken, 1)).unwrap();
        assert!(
            load_flat(&case, &LoadOptions::default()).is_err(),
            "{name} loaded"
        );
    }
}

/// glTF ignores a skinned mesh's node transform; the scene keeps the skin's joints to pose it.
#[test]
fn the_scene_gives_a_skinned_primitive_no_node_transform() {
    let dir = tempfile::tempdir().unwrap();
    let nodes = r#"{"mesh":0,"skin":0,"translation":[5,0,0]},{"translation":[0,3,0]}"#;
    let scene =
        kataglyphis_assets::load_gltf(skinned_triangle(dir.path(), nodes, IDENTITY)).unwrap();
    assert_eq!(scene.primitives[0].transform, glam::Mat4::IDENTITY);
    assert_eq!(scene.primitives[0].skin_index, Some(0));
    let plain = r#"{"mesh":0,"translation":[5,0,0]},{"translation":[0,3,0]}"#;
    let scene =
        kataglyphis_assets::load_gltf(skinned_triangle(dir.path(), plain, IDENTITY)).unwrap();
    assert_eq!(
        scene.primitives[0].transform,
        glam::Mat4::from_translation(glam::Vec3::new(5.0, 0.0, 0.0))
    );
}

/// gltfpack moves a skinned mesh's dequantization into its inverse binds; only the rest pose's joints undo it.
#[test]
fn a_gltfpack_quantized_skin_arrives_dequantized() {
    let step = 0.5 / 4096.0;
    let float = load_flat(fixture("skinned_bar.gltf"), &LoadOptions::default()).unwrap();
    let quantized = load_flat(
        fixture("skinned_bar_quantized.gltf"),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(quantized.indices, float.indices);
    for (q, f) in quantized.vertices.iter().zip(&float.vertices) {
        let off = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max)
        };
        assert!(off(&q.position, &f.position) <= step, "{q:?} against {f:?}");
        assert!(
            off(&q.normal, &f.normal) <= 1.0 / 127.0,
            "{q:?} against {f:?}"
        );
    }
    let scene = kataglyphis_assets::load_gltf(fixture("skinned_bar_quantized.gltf")).unwrap();
    assert!(
        scene.primitives[0]
            .vertices
            .iter()
            .any(|v| v.position[1] > 16000.0),
        "the scene keeps the stored integers for its joints to pose"
    );
}

/// Per triangle: whether its corners in index order wind counter-clockwise about their summed normals.
fn fronts_out(positions: &[glam::Vec3], normals: &[glam::Vec3], indices: &[u32]) -> Vec<bool> {
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[a, b, c]| {
            let [a, b, c] = [a, b, c].map(|i| i as usize);
            let face = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
            face.dot(normals[a] + normals[b] + normals[c]) > 0.0
        })
        .collect()
}

/// A joint that mirrors X places the left card: the bake reverses its triangles, so both fronts face +Z.
#[test]
fn a_skin_mirrored_by_its_joints_keeps_its_fronts_out_in_the_bake() {
    let model = load_flat(fixture("skinned_mirrored.gltf"), &LoadOptions::default()).unwrap();
    let positions: Vec<glam::Vec3> = model.vertices.iter().map(|v| v.position.into()).collect();
    let normals: Vec<glam::Vec3> = model.vertices.iter().map(|v| v.normal.into()).collect();
    assert_eq!(fronts_out(&positions, &normals, &model.indices), [true; 4]);
    for v in &model.vertices {
        assert_eq!(v.normal, [0.0, 0.0, 1.0], "a mirror in X keeps a +Z normal");
    }
    // The mirrored card's tangent sign flips with its joint, as it did before the winding did.
    let signs = |left: bool| -> Vec<f32> {
        let side = model
            .vertices
            .iter()
            .filter(|v| (v.position[0] < 0.0) == left);
        side.map(|v| v.tangent[3]).collect()
    };
    let (upright, mirrored) = (signs(false), signs(true));
    assert_eq!((upright.len(), mirrored.len()), (4, 4));
    assert!(mirrored.iter().chain(&upright).all(|w| w.abs() == 1.0));
    let flipped =
        upright.iter().all(|w| *w == upright[0]) && mirrored.iter().all(|w| *w == -upright[0]);
    assert!(flipped, "{upright:?} {mirrored:?}");
}

/// The scene keeps the bind pose, wound for the joints at the file's pose: posed there, both fronts face +Z.
#[test]
fn a_skin_mirrored_by_its_joints_keeps_its_fronts_out_in_the_scene() {
    use kataglyphis_assets::skin::{joint_matrices, skin_vertex};
    let scene = kataglyphis_assets::load_gltf(fixture("skinned_mirrored.gltf")).unwrap();
    let prim = &scene.primitives[0];
    let world = kataglyphis_assets::CpuScene::compute_world_transforms(&scene.nodes);
    let joints = joint_matrices(&scene.skins[0], &world);
    let posed: Vec<_> = prim
        .vertices
        .iter()
        .map(|v| skin_vertex(v, &joints, prim.transform))
        .collect();
    let positions: Vec<glam::Vec3> = posed.iter().map(|v| v.position.into()).collect();
    let normals: Vec<glam::Vec3> = posed.iter().map(|v| v.normal.into()).collect();
    assert_eq!(fronts_out(&positions, &normals, &prim.indices), [true; 4]);
    let bind: Vec<glam::Vec3> = prim.vertices.iter().map(|v| v.position.into()).collect();
    let bind_normals: Vec<glam::Vec3> = prim.vertices.iter().map(|v| v.normal.into()).collect();
    assert_eq!(
        fronts_out(&bind, &bind_normals, &prim.indices),
        [true, true, false, false],
        "only the mirrored card's triangles are reversed in the bind pose"
    );
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

/// A 1x1 RGBA PNG of (200, 40, 0, 255).
const PIXEL_PNG: [u8; 70] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x38, 0xA1, 0xC1, 0xF0,
    0x1F, 0x00, 0x04, 0x9C, 0x01, 0xF0, 0xAA, 0x79, 0x40, 0x64, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// `dir`'s triangle drawn with one base colour texture whose image is `uri`, escaped for JSON: a Windows path has `\`.
fn triangle_with_image(dir: &Path, uri: &str) -> PathBuf {
    let uri = uri.replace('\\', r"\\").replace('"', r#"\""#);
    let extra = format!(
        r#","images":[{{"uri":"{uri}"}}],"textures":[{{"source":0}}],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}]"#
    );
    let path = triangle_gltf(dir, "image.gltf", r#"{"mesh":0}"#, &extra);
    let json = std::fs::read_to_string(&path).unwrap();
    let drawn = json.replacen(r#""TEXCOORD_0":1}}"#, r#""TEXCOORD_0":1},"material":0}"#, 1);
    assert_ne!(drawn, json, "the primitive takes the material");
    std::fs::write(&path, drawn).unwrap();
    path
}

#[test]
fn both_loaders_read_an_image_only_by_a_relative_uri_inside_the_document_directory() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("model");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let ktx2 = include_bytes!("assets/green_uastc.ktx2");
    for folder in [dir.as_path(), root.path()] {
        std::fs::write(folder.join("pixel.png"), PIXEL_PNG).unwrap();
        std::fs::write(folder.join("green.ktx2"), ktx2).unwrap();
    }
    // Only unix adds the symlink case below.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut cases = vec![
        ("pixel.png".to_owned(), None),
        ("sub/../pixel.png".to_owned(), None),
        (r"sub\..\pixel.png".to_owned(), None),
        ("green.ktx2".to_owned(), None),
        (
            "../pixel.png".to_owned(),
            Some("outside the document's directory"),
        ),
        (
            "sub/../../green.ktx2".to_owned(),
            Some("outside the document's directory"),
        ),
        // A file: URI names no file beside the document, even one that lies inside its directory.
        (
            format!("file://{}", dir.join("pixel.png").display()),
            Some("not a relative URI"),
        ),
        (
            format!("file:{}", root.path().join("pixel.png").display()),
            Some("not a relative URI"),
        ),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.path().join("pixel.png"), dir.join("link.png")).unwrap();
        cases.push((
            "link.png".to_owned(),
            Some("outside the document's directory"),
        ));
    }
    for (uri, refusal) in cases {
        let path = triangle_with_image(&dir, &uri);
        let scene = kataglyphis_assets::load_gltf(&path);
        match refusal {
            None => {
                let scene = scene.unwrap();
                let texture = scene.primitives[0].material.base_color_texture.as_ref();
                let texture = &texture
                    .unwrap_or_else(|| panic!("{uri}: no texture"))
                    .texture;
                if uri.ends_with(".png") {
                    assert_eq!(texture.rgba8, [200, 40, 0, 255], "{uri}");
                } else {
                    assert!(texture.width > 0 && texture.height > 0, "{uri}");
                }
            }
            Some(reason) => {
                let err = format!("{:#}", scene.unwrap_err());
                assert!(err.contains(reason), "{uri}: {err}");
            }
        }
        let flat = load_flat(&path, &LoadOptions::default()).unwrap();
        assert_eq!(flat.textures.len(), usize::from(refusal.is_none()), "{uri}");
    }
}

/// A Linux folder name need not be UTF-8; both loaders read an image there by its path, not by a UTF-8 URI.
#[cfg(target_os = "linux")]
#[test]
fn both_loaders_read_an_image_in_a_folder_whose_name_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt as _;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(std::ffi::OsStr::from_bytes(b"model-\xff"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("pixel.png"), PIXEL_PNG).unwrap();
    let path = triangle_with_image(&dir, "pixel.png");
    let scene = kataglyphis_assets::load_gltf(&path).unwrap();
    let texture = scene.primitives[0].material.base_color_texture.as_ref();
    assert_eq!(texture.unwrap().texture.rgba8, [200, 40, 0, 255]);
    let flat = load_flat(&path, &LoadOptions::default()).unwrap();
    assert_eq!(flat.textures.len(), 1);
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
    let json = std::fs::read_to_string(fixture("cube_textured.gltf")).unwrap();
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
const FUZZ_SEEDS: [&str; 11] = [
    "cube.glb",
    "cube.gltf",
    "cube_quantized.gltf",
    "cube_textured.gltf",
    "cube_alpha.gltf",
    "cube_normal_transform.gltf",
    "cube_on_plane.gltf",
    "point_light.gltf",
    "skinned_bar.gltf",
    "skinned_bar_quantized.gltf",
    "skinned_mirrored.gltf",
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
        let bytes = mutate(std::fs::read(fixture(name)).unwrap(), &mutations);
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

        // A file: URI names the FIFO by its absolute path, which neither loader opens.
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
                format!("{err:#}").contains("not a relative URI"),
                "{uri}: {err:#}"
            );
        }
    }
}

/// `cube_morph.gltf`, whose one target lifts every vertex by 1, with `node_weights` on its node.
fn cube_morph_with_node_weights(dir: &Path, node_weights: &str) -> PathBuf {
    let json = std::fs::read_to_string(fixture("cube_morph.gltf")).unwrap();
    let node = "\"mesh\": 0\n  }";
    assert!(json.contains(node), "the fixture changed");
    let path = dir.join("cube_morph_node.gltf");
    let weighted = format!("\"mesh\": 0, \"weights\": {node_weights}\n  }}");
    std::fs::write(&path, json.replacen(node, &weighted, 1)).unwrap();
    path
}

#[test]
fn default_morph_weights_bake_into_the_flat_vertices_and_the_scene_keeps_them_apart() {
    let path = fixture("cube_morph.gltf");
    let flat = load_flat(&path, &LoadOptions::default()).unwrap();
    let scene = kataglyphis_assets::load_gltf(&path).unwrap();
    let neutral = &scene.primitives[0];
    // The renderer blends from the neutral pose itself, so the shared loader must not bake the weight too.
    assert_eq!(neutral.morph_weights, [1.0]);
    assert_eq!(flat.vertices.len(), neutral.vertices.len());
    let lift = |model: &FlatModel| -> Vec<f32> {
        let base = neutral.vertices.iter().map(|v| v.position);
        let moved = model.vertices.iter().map(|v| v.position);
        moved.zip(base).map(|(m, b)| m[1] - b[1]).collect()
    };
    assert!(
        lift(&flat).iter().all(|&dy| dy == 1.0),
        "the mesh's weight 1.0 is baked"
    );
    assert!(flat
        .vertices
        .iter()
        .zip(&neutral.vertices)
        .all(|(m, b)| m.position[0] == b.position[0]));

    let dir = tempfile::tempdir().unwrap();
    let options = LoadOptions::default();
    let node_wins =
        load_flat(cube_morph_with_node_weights(dir.path(), "[0.25]"), &options).unwrap();
    assert!(
        lift(&node_wins).iter().all(|&dy| dy == 0.25),
        "the node's weights win over the mesh's"
    );
    let mismatched = load_flat(
        cube_morph_with_node_weights(dir.path(), "[0.25, 0.5]"),
        &options,
    );
    assert!(
        lift(&mismatched.unwrap()).iter().all(|&dy| dy == 0.0),
        "a count off the targets is ignored"
    );
}

/// `triangle_gltf` on one node under `extra`'s top-level members, flattened with the default options.
fn flat_with(extra: &str) -> FlatModel {
    let dir = tempfile::tempdir().unwrap();
    let path = triangle_gltf(dir.path(), "doc.gltf", r#"{"mesh":0}"#, extra);
    load_flat(path, &LoadOptions::default()).unwrap()
}

/// `count` images that embed the same PNG, so only their index tells them apart.
fn same_png_images(count: usize) -> String {
    let image = format!(r#"{{"uri":"{}"}}"#, embedded_png_uri());
    format!(r#""images":[{}]"#, vec![image; count].join(","))
}

#[test]
fn material_factors_reach_the_flat_material() {
    let extra = format!(
        r#",{},"textures":[{{"source":0}}],"extensionsUsed":["KHR_materials_emissive_strength"],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.25,0.5,1.0,0.25],"baseColorTexture":{{"index":0}},
                        "metallicFactor":0.75,"roughnessFactor":0.5}},
                       "normalTexture":{{"index":0,"scale":0.5}},"emissiveFactor":[0.2,0.4,0.6]}},
                     {{"emissiveFactor":[1,1,1],"extensions":{{"KHR_materials_emissive_strength":{{"emissiveStrength":4}}}}}}]"#,
        same_png_images(1)
    );
    let model = flat_with(&extra);
    let textured = &model.materials[0];
    // glTF's base colour is the factor times the texture, so the factor stays beside the texture.
    assert_eq!(
        (textured.diffuse, textured.dissolve),
        ([0.25, 0.5, 1.0], 0.25)
    );
    assert_eq!(textured.base_texture, 0);
    assert_eq!(
        (textured.metallic, textured.roughness, textured.normal_scale),
        (0.75, 0.5, 0.5)
    );
    assert_eq!(
        textured.emission,
        [0.2, 0.4, 0.6],
        "no strength leaves the factor"
    );
    assert_eq!(
        model.materials[1].emission, [4.0; 3],
        "the strength scales the factor past 1"
    );
}

#[test]
fn a_material_without_a_pbr_block_and_the_fallback_take_the_gltf_defaults() {
    let model = flat_with(r#","materials":[{}]"#);
    assert_eq!(
        model.materials.len(),
        2,
        "the declared material, then the fallback"
    );
    for material in &model.materials {
        assert_eq!((material.diffuse, material.dissolve), ([1.0; 3], 1.0));
        assert_eq!(
            (material.metallic, material.roughness, material.normal_scale),
            (1.0, 1.0, 1.0)
        );
        assert_eq!(
            (material.alpha_cutoff, material.alpha_blend, material.unlit),
            (-1.0, false, false)
        );
        assert_eq!(material.emission, [0.0; 3]);
        let slots = [
            material.base_texture,
            material.metallic_roughness_texture,
            material.normal_texture,
            material.emissive_texture,
        ];
        assert_eq!(slots, [-1; 4]);
    }
}

#[test]
fn alpha_modes_set_the_cutoff_and_the_blend_flag_of_material_and_range() {
    let dir = tempfile::tempdir().unwrap();
    let extra = r#","materials":[{"alphaMode":"MASK","alphaCutoff":0.25},{"alphaMode":"OPAQUE"},
        {"alphaMode":"BLEND","pbrMetallicRoughness":{"baseColorFactor":[1,0,0,0.5]}}]"#;
    let mut blended = Vec::new();
    let mut materials = Vec::new();
    for material in [1, 2] {
        let path = triangle_gltf(dir.path(), "alpha.gltf", r#"{"mesh":0}"#, extra);
        let json = std::fs::read_to_string(&path).unwrap();
        let drawn = format!(r#""material":{material},"attributes":{{"#);
        std::fs::write(&path, json.replacen(r#""attributes":{"#, &drawn, 1)).unwrap();
        let model = load_flat(&path, &LoadOptions::default()).unwrap();
        blended.push(model.ranges[0].alpha_blended);
        materials = model.materials;
    }
    assert_eq!(
        blended,
        [false, true],
        "only the BLEND mesh leaves the opaque passes"
    );
    let [mask, opaque, blend] = [&materials[0], &materials[1], &materials[2]];
    assert_eq!((mask.alpha_cutoff, mask.alpha_blend), (0.25, false));
    assert_eq!((opaque.alpha_cutoff, opaque.alpha_blend), (-1.0, false));
    // BLEND never alpha-tests; its factor's alpha rides in dissolve.
    assert_eq!(
        (blend.alpha_cutoff, blend.alpha_blend, blend.dissolve),
        (-1.0, true, 0.5)
    );
}

#[test]
fn khr_materials_unlit_marks_only_its_own_material() {
    let model = flat_with(
        r#","extensionsUsed":["KHR_materials_unlit"],"materials":[{"extensions":{"KHR_materials_unlit":{}}},{}]"#,
    );
    assert_eq!(
        (model.materials[0].unlit, model.materials[1].unlit),
        (true, false)
    );
}

#[test]
fn declared_images_get_dense_slots_whatever_their_bytes() {
    // Images 0 and 1 embed one PNG; image 2's six zero bytes decode to nothing, which the C++ upload finds out later.
    let images =
        same_png_images(2).replacen("]", r#",{"uri":"data:image/png;base64,AAAAAAAA"}]"#, 1);
    let base =
        |i: usize| format!(r#"{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":{i}}}}}}}"#);
    let extra = format!(
        r#",{images},"textures":[{{"source":0}},{{"source":1}},{{"source":2}}],"materials":[{},{},{}]"#,
        base(0),
        base(1),
        base(2)
    );
    let model = flat_with(&extra);
    let ids: Vec<i32> = model.materials.iter().map(|m| m.base_texture).collect();
    assert_eq!(
        ids,
        [0, 1, 2, -1],
        "slots key on the declared image, not its bytes, and never decode"
    );
    assert_eq!(model.textures[2].encoded, [0; 6]);
}

#[test]
fn every_slot_gets_its_own_texture_colour_space_and_uv_transform() {
    let scaled = |index: usize, scale: f32| {
        format!(
            r#"{{"index":{index},"extensions":{{"KHR_texture_transform":{{"scale":[{scale},{scale}]}}}}}}"#
        )
    };
    let all = format!(
        r#"{{"pbrMetallicRoughness":{{"baseColorTexture":{},"metallicRoughnessTexture":{}}},"normalTexture":{},"emissiveTexture":{}}}"#,
        scaled(0, 4.0),
        scaled(1, 3.0),
        scaled(2, 2.0),
        scaled(3, 1.5)
    );
    let plain = r#"{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}"#;
    let normal_only = format!(r#"{{"normalTexture":{}}}"#, scaled(2, 3.0));
    let turned = r#"{"pbrMetallicRoughness":{"baseColorTexture":{"index":0,"extensions":{"KHR_texture_transform":{"rotation":1.5707964}}}}}"#;
    let extra = format!(
        r#",{},"textures":[{{"source":0}},{{"source":1}},{{"source":2}},{{"source":3}}],
        "extensionsUsed":["KHR_texture_transform"],"materials":[{all},{plain},{normal_only},{turned}]"#,
        same_png_images(4)
    );
    let model = flat_with(&extra);
    let m = &model.materials[0];
    let slots = [
        m.base_texture,
        m.metallic_roughness_texture,
        m.normal_texture,
        m.emissive_texture,
    ];
    assert_eq!(slots, [0, 1, 2, 3]);
    let srgb: Vec<bool> = slots
        .iter()
        .map(|&s| model.textures[s as usize].srgb)
        .collect();
    assert_eq!(
        srgb,
        [true, false, false, true],
        "colour slots are sRGB, data slots linear"
    );
    let scale = |rows: [[f32; 3]; 2]| (rows[0][0], rows[1][1]);
    let scales = [
        m.base_uv,
        m.metallic_roughness_uv,
        m.normal_uv,
        m.emissive_uv,
    ]
    .map(scale);
    assert_eq!(
        scales,
        [(4.0, 4.0), (3.0, 3.0), (2.0, 2.0), (1.5, 1.5)],
        "no slot takes another's"
    );
    const IDENTITY: [[f32; 3]; 2] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert_eq!(model.materials[1].base_uv, IDENTITY);
    let n = &model.materials[2];
    assert_eq!(
        scale(n.normal_uv),
        (3.0, 3.0),
        "a transform on the normal slot alone"
    );
    assert_eq!(
        [n.base_uv, n.metallic_roughness_uv, n.emissive_uv],
        [IDENTITY; 3]
    );
    // The spec's T*R*S turns u = (1, 0) by -rotation: a quarter turn lands on (0, -1), not (0, 1).
    let rows = model.materials[3].base_uv;
    let u = [rows[0][0] + rows[0][2], rows[1][0] + rows[1][2]];
    assert!(u[0].abs() < 1e-6 && (u[1] + 1.0).abs() < 1e-6, "{u:?}");
}

#[test]
fn a_percent_encoded_image_uri_resolves_beside_the_document() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("my tex.png"), PIXEL_PNG).unwrap();
    let path = triangle_with_image(dir.path(), "my%20tex.png");
    let model = load_flat(path, &LoadOptions::default()).unwrap();
    assert_eq!(model.textures.len(), 1);
    assert_eq!(model.textures[0].encoded, PIXEL_PNG);
}

#[test]
fn a_truncated_document_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cut.gltf");
    std::fs::write(&path, r#"{ "asset": { "version": "2.0" "#).unwrap();
    assert!(load_flat(&path, &LoadOptions::default()).is_err());
}

#[test]
fn each_slot_records_the_uv_set_its_tex_coord_names() {
    let extra = format!(
        r#",{},"textures":[{{"source":0}}],"extensionsUsed":["KHR_texture_transform"],"materials":[
          {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0,"texCoord":0}},"metallicRoughnessTexture":{{"index":0,"texCoord":1}}}},
            "normalTexture":{{"index":0,"texCoord":2}},"emissiveTexture":{{"index":0,"texCoord":1}}}},
          {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0,"texCoord":0,"extensions":{{"KHR_texture_transform":{{"texCoord":1}}}}}}}},
            "emissiveTexture":{{"index":0,"texCoord":1,"extensions":{{"KHR_texture_transform":{{"texCoord":0}}}}}}}}]"#,
        same_png_images(1)
    );
    let model = flat_with(&extra);
    // Bits: 0 base colour, 1 metallic-roughness, 2 normal, 3 emissive; TEXCOORD_2 is unsupported and reads UV0.
    assert_eq!(model.materials[0].uv1_slot_mask, 0b1010);
    assert_eq!(
        model.materials[1].uv1_slot_mask, 0b0001,
        "the transform's texCoord wins both ways"
    );
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// One mesh of `primitive` over `accessors` and `views` into the embedded `bytes`, on one node.
fn geometry_gltf(
    dir: &Path,
    primitive: &str,
    accessors: &str,
    views: &str,
    bytes: &[u8],
) -> PathBuf {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
        "meshes":[{{"primitives":[{primitive}]}}],"accessors":[{accessors}],"bufferViews":[{views}],
        "buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}]}}"#,
        bytes.len(),
        base64(bytes)
    );
    let path = dir.join("geometry.gltf");
    std::fs::write(&path, json).unwrap();
    path
}

/// The POSITION accessor and view of `count` vertices at the buffer's start, as JSON list members.
fn position_parts(count: usize) -> (String, String) {
    (
        format!(
            r#"{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3","min":[0,0,0],"max":[1,1,1]}}"#
        ),
        format!(r#"{{"buffer":0,"byteLength":{}}}"#, count * 12),
    )
}

#[test]
fn without_normals_each_face_of_a_shared_corner_gets_its_own_flat_normal() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    bytes.extend([0u16, 1, 2, 0, 3, 1].iter().flat_map(|i| i.to_le_bytes()));
    let (position, view) = position_parts(4);
    let path = geometry_gltf(
        dir.path(),
        r#"{"attributes":{"POSITION":0},"indices":1}"#,
        &format!(r#"{position},{{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}}"#),
        &format!(r#"{view},{{"buffer":0,"byteOffset":48,"byteLength":12}}"#),
        &bytes,
    );
    let model = load_flat(path, &LoadOptions::default()).unwrap();
    assert_eq!(model.vertices.len(), 6, "one vertex per corner");
    for (corner, &index) in model.indices.iter().enumerate() {
        let face = if corner < 3 {
            glam::Vec3::Z
        } else {
            glam::Vec3::Y
        };
        let normal = glam::Vec3::from_array(model.vertices[index as usize].normal);
        assert!(
            (normal - face).length() < 1e-6,
            "corner {corner}: {normal:?}"
        );
    }
}

#[test]
fn strips_and_fans_load_as_triangle_lists_with_their_own_winding() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    let (position, view) = position_parts(4);
    let corner = |model: &FlatModel| -> Vec<[f32; 3]> {
        model
            .indices
            .iter()
            .map(|&i| model.vertices[i as usize].position)
            .collect()
    };
    let (o, x, y, xy) = (
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    );
    // The odd strip triangle swaps its first two corners; a fan turns around its hub, vertex 0.
    for (mode, expected) in [(5, [o, x, y, y, x, xy]), (6, [o, x, y, o, y, xy])] {
        let primitive = format!(r#"{{"attributes":{{"POSITION":0}},"mode":{mode}}}"#);
        let path = geometry_gltf(dir.path(), &primitive, &position, &view, &bytes);
        let model = load_flat(path, &LoadOptions::default()).unwrap();
        assert_eq!(
            model.vertices.len(),
            6,
            "mode {mode}: one vertex per corner"
        );
        assert_eq!(corner(&model), expected, "mode {mode}");
    }
}

#[test]
fn an_out_of_range_index_drops_only_its_own_triangle() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    bytes.extend([0u16, 1, 2, 0, 1, 7].iter().flat_map(|i| i.to_le_bytes()));
    let (position, view) = position_parts(3);
    let path = geometry_gltf(
        dir.path(),
        r#"{"attributes":{"POSITION":0},"indices":1}"#,
        &format!(r#"{position},{{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}}"#),
        &format!(r#"{view},{{"buffer":0,"byteOffset":36,"byteLength":12}}"#),
        &bytes,
    );
    let model = load_flat(path, &LoadOptions::default()).unwrap();
    assert_consistent(&model);
    assert_eq!(model.vertices.len(), 3);
    assert_eq!(
        model.indices,
        [0, 1, 2],
        "the triangle naming vertex 7 is gone, the first stays"
    );
}

#[test]
fn color_0_reaches_the_vertices_and_a_vec3_one_is_opaque() {
    let dir = tempfile::tempdir().unwrap();
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let rgba = [
        1.0, 0.0, 0.0, 0.25, 0.0, 1.0, 0.0, 0.25, 0.0, 0.0, 1.0, 0.25,
    ];
    let rgb = [0.5; 9];
    let (position, view) = position_parts(3);
    let mut colours = Vec::new();
    for (kind, values) in [("VEC4", &rgba[..]), ("VEC3", &rgb[..])] {
        let mut bytes = f32_bytes(&positions);
        bytes.extend(f32_bytes(values));
        let path = geometry_gltf(
            dir.path(),
            r#"{"attributes":{"POSITION":0,"COLOR_0":1}}"#,
            &format!(
                r#"{position},{{"bufferView":1,"componentType":5126,"count":3,"type":"{kind}"}}"#
            ),
            &format!(
                r#"{view},{{"buffer":0,"byteOffset":36,"byteLength":{}}}"#,
                values.len() * 4
            ),
            &bytes,
        );
        let model = load_flat(path, &LoadOptions::default()).unwrap();
        colours.push(model.vertices.iter().map(|v| v.color).collect::<Vec<_>>());
    }
    let rgba_corners = [
        [1.0, 0.0, 0.0, 0.25],
        [0.0, 1.0, 0.0, 0.25],
        [0.0, 0.0, 1.0, 0.25],
    ];
    assert_eq!(colours[0], rgba_corners);
    assert_eq!(
        colours[1],
        [[0.5, 0.5, 0.5, 1.0]; 3],
        "a VEC3 colour has alpha 1"
    );
}

#[test]
fn texcoord_1_reaches_the_second_uv_set_and_its_absence_copies_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let uv0 = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let uv1 = [[0.0, 1.0], [0.0, 0.0], [1.0, 1.0]];
    let mut bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    bytes.extend(f32_bytes(uv0.as_flattened()));
    bytes.extend(f32_bytes(uv1.as_flattened()));
    let (position, view) = position_parts(3);
    let accessors = format!(
        r#"{position},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}},
        {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}"#
    );
    let views = format!(
        r#"{view},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":60,"byteLength":24}}"#
    );
    let sets = |attributes: &str| -> Vec<([f32; 2], [f32; 2])> {
        let primitive = format!(r#"{{"attributes":{{{attributes}}}}}"#);
        let path = geometry_gltf(dir.path(), &primitive, &accessors, &views, &bytes);
        let model = load_flat(path, &LoadOptions::default()).unwrap();
        model.vertices.iter().map(|v| (v.uv0, v.uv1)).collect()
    };
    let both = sets(r#""POSITION":0,"TEXCOORD_0":1,"TEXCOORD_1":2"#);
    assert_eq!(both, [(uv0[0], uv1[0]), (uv0[1], uv1[1]), (uv0[2], uv1[2])]);
    let first_only = sets(r#""POSITION":0,"TEXCOORD_0":1"#);
    assert!(
        first_only.iter().all(|(a, b)| a == b),
        "a slot on set 1 samples UV0, not (0, 0)"
    );
}

#[test]
fn authored_tangents_are_kept_instead_of_generated() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = f32_bytes(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    bytes.extend(f32_bytes(&[0.0f32, 0.0, 1.0].repeat(3)));
    bytes.extend(f32_bytes(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]));
    // Lengyel's tangent from these UVs is (1, 0, 0, 1); the authored one differs in axis and handedness.
    bytes.extend(f32_bytes(&[0.0f32, 1.0, 0.0, -1.0].repeat(3)));
    let (position, view) = position_parts(3);
    let accessors = format!(
        r#"{position},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},
        {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC4"}}"#
    );
    let views = format!(
        r#"{view},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},
        {{"buffer":0,"byteOffset":96,"byteLength":48}}"#
    );
    let primitive = r#"{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2,"TANGENT":3}}"#;
    let model = load_flat(
        geometry_gltf(dir.path(), primitive, &accessors, &views, &bytes),
        &LoadOptions::default(),
    )
    .unwrap();
    assert!(model
        .vertices
        .iter()
        .all(|v| v.tangent == [0.0, 1.0, 0.0, -1.0]));
}

#[test]
fn only_the_default_scene_and_its_descendants_load() {
    let dir = tempfile::tempdir().unwrap();
    let scene_0 = r#""scene":0,"scenes":[{"nodes":[0]}],"#;
    let load = |scenes: &str, nodes: &str| -> Vec<[f32; 3]> {
        let path = triangle_gltf(dir.path(), "scenes.gltf", nodes, "");
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(scene_0), "the fixture changed");
        std::fs::write(&path, json.replacen(scene_0, scenes, 1)).unwrap();
        let model = load_flat(&path, &LoadOptions::default()).unwrap();
        model.vertices.iter().map(|v| v.position).collect()
    };
    let two = r#"{"mesh":0},{"mesh":0,"translation":[100,0,0]}"#;
    let scene_1 = load(r#""scene":1,"scenes":[{"nodes":[0]},{"nodes":[1]}],"#, two);
    assert_eq!(
        scene_1,
        [[100.0, 0.0, 0.0], [101.0, 0.0, 0.0], [100.0, 1.0, 0.0]]
    );
    let orphan = load(scene_0, two);
    assert_eq!(
        orphan,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        "node 1 is in no scene"
    );
    let child = load(
        scene_0,
        r#"{"children":[1]},{"mesh":0,"translation":[50,0,0]}"#,
    );
    assert_eq!(
        child[0],
        [50.0, 0.0, 0.0],
        "a root's child loads with its own transform"
    );
}

#[test]
fn each_primitive_of_a_mesh_gets_its_own_range_and_material() {
    let dir = tempfile::tempdir().unwrap();
    let path = triangle_gltf(
        dir.path(),
        "two.gltf",
        r#"{"mesh":0}"#,
        r#","materials":[{},{}]"#,
    );
    let json = std::fs::read_to_string(&path).unwrap();
    let primitive = r#"{"attributes":{"POSITION":0,"TEXCOORD_0":1}}"#;
    assert!(json.contains(primitive), "the fixture changed");
    let two = r#"{"material":0,"attributes":{"POSITION":0,"TEXCOORD_0":1}},
                 {"material":1,"attributes":{"POSITION":0,"TEXCOORD_0":1}}"#;
    std::fs::write(&path, json.replacen(primitive, two, 1)).unwrap();
    let model = load_flat(&path, &LoadOptions::default()).unwrap();
    assert_consistent(&model);
    let bases: Vec<usize> = model.ranges.iter().map(|r| r.vertex_base).collect();
    assert_eq!(
        bases,
        [0, 3],
        "the second primitive's vertices follow the first's"
    );
    assert_eq!(model.material_index, [0, 1]);
}

#[test]
fn two_negative_scales_are_a_rotation_and_keep_the_winding() {
    let dir = tempfile::tempdir().unwrap();
    let nodes = r#"{"mesh":0,"scale":[-1,-1,1]}"#;
    let path = triangle_gltf(dir.path(), "turned.gltf", nodes, "");
    let model = load_flat(path, &LoadOptions::default()).unwrap();
    assert_eq!(model.indices, [0, 1, 2]);
    assert_eq!(model.vertices[1].position, [-1.0, 0.0, 0.0]);
}

/// KHR_mesh_quantization: integer attributes, read by their `normalized` flag through both loaders.
mod quantization {
    use super::*;

    const I8: u32 = 5120;
    const U8: u32 = 5121;
    const I16: u32 = 5122;
    const U16: u32 = 5123;
    const U32: u32 = 5125;
    const F32: u32 = 5126;

    /// One accessor: its glTF type name, componentType, `normalized` flag and one row per vertex.
    #[derive(Clone)]
    struct Attribute {
        name: &'static str,
        kind: u32,
        normalized: bool,
        rows: Vec<Vec<f64>>,
    }

    fn attribute(name: &'static str, kind: u32, normalized: bool, rows: &[&[f64]]) -> Attribute {
        Attribute {
            name,
            kind,
            normalized,
            rows: rows.iter().map(|row| row.to_vec()).collect(),
        }
    }

    fn component(kind: u32, value: f64) -> Vec<u8> {
        match kind {
            I8 => (value as i8).to_le_bytes().to_vec(),
            U8 => (value as u8).to_le_bytes().to_vec(),
            I16 => (value as i16).to_le_bytes().to_vec(),
            U16 => (value as u16).to_le_bytes().to_vec(),
            U32 => (value as u32).to_le_bytes().to_vec(),
            _ => (value as f32).to_le_bytes().to_vec(),
        }
    }

    fn glsl_type(width: usize) -> &'static str {
        ["SCALAR", "VEC2", "VEC3", "VEC4"][width - 1]
    }

    /// A triangle whose `attributes` and one morph target's `deltas` each get an accessor; elements padded to 4 bytes.
    fn triangle(dir: &Path, attributes: &[Attribute], deltas: &[Attribute]) -> PathBuf {
        triangle_with(dir, attributes, deltas, true)
    }

    /// [`triangle`], declaring KHR_mesh_quantization only when `quantized`.
    fn triangle_with(
        dir: &Path,
        attributes: &[Attribute],
        deltas: &[Attribute],
        quantized: bool,
    ) -> PathBuf {
        let (mut bytes, mut accessors, mut views) = (Vec::new(), Vec::new(), Vec::new());
        let mut add = |a: &Attribute| -> usize {
            let width = a.rows[0].len();
            let element: usize = component(a.kind, 0.0).len() * width;
            let stride = element.next_multiple_of(4);
            let offset = bytes.len();
            for row in &a.rows {
                let mut packed: Vec<u8> = row.iter().flat_map(|&v| component(a.kind, v)).collect();
                packed.resize(stride, 0);
                bytes.extend(packed);
            }
            let bounds = if a.name == "POSITION" {
                let axis = |pick: fn(f64, f64) -> f64| -> Vec<f64> {
                    (0..width)
                        .map(|k| a.rows.iter().map(|r| r[k]).reduce(pick).unwrap())
                        .collect()
                };
                format!(r#","min":{:?},"max":{:?}"#, axis(f64::min), axis(f64::max))
            } else {
                String::new()
            };
            views.push(format!(
                r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{},"byteStride":{stride}}}"#,
                stride * a.rows.len()
            ));
            accessors.push(format!(
                r#"{{"bufferView":{},"componentType":{},"normalized":{},"count":{},"type":"{}"{bounds}}}"#,
                views.len() - 1,
                a.kind,
                a.normalized,
                a.rows.len(),
                glsl_type(width)
            ));
            accessors.len() - 1
        };
        let mut members = |list: &[Attribute]| -> String {
            list.iter()
                .map(|a| format!(r#""{}":{}"#, a.name, add(a)))
                .collect::<Vec<_>>()
                .join(",")
        };
        let attributes = members(attributes);
        let targets = if deltas.is_empty() {
            String::new()
        } else {
            format!(r#","targets":[{{{}}}]"#, members(deltas))
        };
        let extensions = if quantized {
            r#""extensionsUsed":["KHR_mesh_quantization"],"extensionsRequired":["KHR_mesh_quantization"],"#
        } else {
            ""
        };
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},{extensions}"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{{attributes}}}{targets}}}]}}],"accessors":[{}],"bufferViews":[{}],
            "buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}]}}"#,
            accessors.join(","),
            views.join(","),
            bytes.len(),
            base64(&bytes)
        );
        let path = dir.join("quantized.gltf");
        std::fs::write(&path, json).unwrap();
        path
    }

    fn float_positions() -> Attribute {
        attribute(
            "POSITION",
            F32,
            false,
            &[&[0.0, 0.0, 0.0], &[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]],
        )
    }

    /// Per integer type: three raw values, their decoding when `normalized` (the glTF spec's table) and its maximum.
    const TYPES: [(u32, [f64; 3], [f32; 3]); 4] = [
        (I8, [-128.0, 127.0, 64.0], [-1.0, 1.0, 64.0 / 127.0]),
        (U8, [0.0, 255.0, 51.0], [0.0, 1.0, 0.2]),
        (
            I16,
            [-32768.0, 32767.0, -16384.0],
            [-1.0, 1.0, -16384.0 / 32767.0],
        ),
        (U16, [0.0, 65535.0, 13107.0], [0.0, 1.0, 0.2]),
    ];

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-6 * b.abs().max(1.0)
    }

    /// gltfpack's layout of `cube.gltf`: u16 positions under a dequantizing node scale and translation, i8 normals and
    /// u16 texcoords, both normalized, at strides 8, 4 and 4.
    #[test]
    fn a_gltfpack_quantized_cube_loads_where_its_float_twin_does() {
        let step = 1.0 / 16383.0;
        let options = LoadOptions::default();
        let float = load_flat(fixture("cube.gltf"), &options).unwrap();
        let quantized = load_flat(fixture("cube_quantized.gltf"), &options).unwrap();
        assert_eq!(quantized.indices, float.indices);
        assert_eq!(quantized.vertices.len(), float.vertices.len());
        for (q, f) in quantized.vertices.iter().zip(&float.vertices) {
            let off = |a: &[f32], b: &[f32]| {
                a.iter()
                    .zip(b)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f32::max)
            };
            assert!(off(&q.position, &f.position) <= step, "{q:?} against {f:?}");
            assert!(
                off(&q.normal, &f.normal) <= 1.0 / 127.0,
                "{q:?} against {f:?}"
            );
            assert!(off(&q.uv0, &f.uv0) <= 1.0 / 65535.0, "{q:?} against {f:?}");
            assert!(off(&q.tangent, &f.tangent) <= 1e-3, "{q:?} against {f:?}");
        }
        let float = kataglyphis_assets::load_gltf(fixture("cube.gltf")).unwrap();
        let quantized = kataglyphis_assets::load_gltf(fixture("cube_quantized.gltf")).unwrap();
        let world = |scene: &kataglyphis_assets::CpuScene| -> Vec<glam::Vec3> {
            let prim = &scene.primitives[0];
            prim.vertices
                .iter()
                .map(|v| {
                    prim.transform
                        .transform_point3(glam::Vec3::from_array(v.position))
                })
                .collect()
        };
        let stored = &quantized.primitives[0].vertices;
        assert!(
            stored.iter().any(|v| v.position[0] > 16000.0),
            "the scene keeps the stored integers under the node's dequantizing transform"
        );
        for (q, f) in world(&quantized).iter().zip(world(&float)) {
            assert!((*q - f).abs().max_element() <= step, "{q} against {f}");
        }
    }

    #[test]
    fn integer_positions_and_texcoords_decode_by_their_normalized_flag() {
        let dir = tempfile::tempdir().unwrap();
        for (kind, raw, unit) in TYPES {
            for normalized in [false, true] {
                let expected = if normalized {
                    unit
                } else {
                    raw.map(|v| v as f32)
                };
                let [a, b, c] = raw;
                let positions = attribute(
                    "POSITION",
                    kind,
                    normalized,
                    &[&[a, 0.0, 0.0], &[0.0, b, 0.0], &[0.0, 0.0, c]],
                );
                let uvs = attribute("TEXCOORD_0", kind, normalized, &[&[a, b], &[b, c], &[c, a]]);
                let path = triangle(dir.path(), &[positions, uvs], &[]);
                let scene = kataglyphis_assets::load_gltf(&path).unwrap();
                let flat = load_flat(&path, &LoadOptions::default()).unwrap();
                let case = format!("componentType {kind}, normalized {normalized}");
                let [x, y, z] = expected;
                let want_positions = [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, z]];
                let want_uvs = [[x, y], [y, z], [z, x]];
                let gltf: Vec<_> = scene.primitives[0]
                    .vertices
                    .iter()
                    .map(|v| (v.position, v.uv))
                    .collect();
                let flat: Vec<_> = flat.vertices.iter().map(|v| (v.position, v.uv0)).collect();
                for got in [gltf, flat] {
                    for (k, (position, uv)) in got.iter().enumerate() {
                        let all = position
                            .iter()
                            .zip(&want_positions[k])
                            .chain(uv.iter().zip(&want_uvs[k]));
                        for (got, want) in all {
                            assert!(near(*got, *want), "{case}, vertex {k}: {got} for {want}");
                        }
                    }
                }
            }
        }
    }

    /// Core glTF allows a u8 or u16 TEXCOORD only normalized; gltf's reader normalized one without the flag too.
    #[test]
    fn a_core_file_reads_unsigned_texcoords_normalized_whatever_their_flag() {
        let dir = tempfile::tempdir().unwrap();
        for (kind, raw, unit) in TYPES
            .into_iter()
            .filter(|(kind, ..)| [U8, U16].contains(kind))
        {
            let [a, b, c] = raw;
            let uvs = attribute("TEXCOORD_0", kind, false, &[&[a, b], &[b, c], &[c, a]]);
            let path = triangle_with(dir.path(), &[float_positions(), uvs], &[], false);
            let scene = kataglyphis_assets::load_gltf(&path).unwrap();
            let flat = load_flat(&path, &LoadOptions::default()).unwrap();
            let [x, y, z] = unit;
            let want = [[x, y], [y, z], [z, x]];
            let gltf: Vec<[f32; 2]> = scene.primitives[0].vertices.iter().map(|v| v.uv).collect();
            let flat: Vec<[f32; 2]> = flat.vertices.iter().map(|v| v.uv0).collect();
            for got in [gltf, flat] {
                for (k, uv) in got.iter().enumerate() {
                    for (got, want) in uv.iter().zip(&want[k]) {
                        assert!(
                            near(*got, *want),
                            "componentType {kind}, vertex {k}: {got} for {want}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn normalized_integer_normals_tangents_and_morph_deltas_decode() {
        let dir = tempfile::tempdir().unwrap();
        for (kind, max) in [(I8, 127.0), (I16, 32767.0)] {
            let min = -max - 1.0;
            let normals = attribute(
                "NORMAL",
                kind,
                true,
                &[&[0.0, 0.0, max], &[min, 0.0, 0.0], &[0.0, max, 0.0]],
            );
            let tangents = attribute(
                "TANGENT",
                kind,
                true,
                &[
                    &[max, 0.0, 0.0, max],
                    &[0.0, max, 0.0, min],
                    &[0.0, 0.0, min, max],
                ],
            );
            let half = (max / 2.0).round();
            let moved = attribute(
                "POSITION",
                kind,
                false,
                &[&[0.0, half, 0.0], &[0.0; 3], &[0.0; 3]],
            );
            let turned = attribute(
                "NORMAL",
                kind,
                true,
                &[&[half, 0.0, 0.0], &[0.0; 3], &[0.0; 3]],
            );
            let path = triangle(
                dir.path(),
                &[float_positions(), normals, tangents],
                &[moved, turned],
            );
            let scene = kataglyphis_assets::load_gltf(&path).unwrap();
            let prim = &scene.primitives[0];
            let got: Vec<_> = prim
                .vertices
                .iter()
                .map(|v| (v.normal, v.tangent))
                .collect();
            assert_eq!(
                got,
                [
                    ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]),
                    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0, -1.0]),
                    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0, 1.0]),
                ],
                "componentType {kind}"
            );
            let target = &prim.morph_targets[0];
            assert_eq!(
                target.position_deltas[0],
                glam::Vec3::new(0.0, half as f32, 0.0),
                "the integer itself"
            );
            assert!(
                near(target.normal_deltas[0].x, (half / max) as f32),
                "componentType {kind}"
            );
        }
    }

    #[test]
    fn formats_the_extension_does_not_allow_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let normal: &[f64] = &[0.0, 0.0, 1.0];
        let rows: &[&[f64]] = &[normal; 3];
        let tangent: &[f64] = &[0.0, 0.0, 1.0, 1.0];
        let uv: &[f64] = &[0.0, 1.0];
        let cases = [
            (
                "an unnormalized byte normal",
                vec![attribute("NORMAL", I8, false, rows)],
                vec![],
            ),
            (
                "an unsigned normal",
                vec![attribute("NORMAL", U8, true, rows)],
                vec![],
            ),
            (
                "an unsigned tangent",
                vec![attribute("TANGENT", U16, true, &[tangent; 3])],
                vec![],
            ),
            (
                "u32 texcoords",
                vec![attribute("TEXCOORD_0", U32, false, &[uv; 3])],
                vec![],
            ),
            (
                "an unsigned morph position",
                vec![],
                vec![attribute("POSITION", U8, false, rows)],
            ),
            (
                "an unnormalized morph normal",
                vec![],
                vec![attribute("NORMAL", I16, false, rows)],
            ),
        ];
        for (name, attributes, deltas) in cases {
            let attributes = [vec![float_positions()], attributes].concat();
            let path = triangle(dir.path(), &attributes, &deltas);
            assert!(
                kataglyphis_assets::load_gltf(&path).is_err(),
                "{name} loaded"
            );
            assert!(
                load_flat(&path, &LoadOptions::default()).is_err(),
                "{name} flattened"
            );
        }
        let u32_positions = attribute(
            "POSITION",
            U32,
            false,
            &[&[0.0, 0.0, 0.0], &[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]],
        );
        let path = triangle(dir.path(), &[u32_positions], &[]);
        assert!(
            load_flat(&path, &LoadOptions::default()).is_err(),
            "u32 positions flattened"
        );
        let path = triangle(dir.path(), &[float_positions()], &[]);
        assert!(
            load_flat(&path, &LoadOptions::default()).is_ok(),
            "the base case loads"
        );
    }
}
