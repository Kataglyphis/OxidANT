//! OBJ -> glTF conversion, verified by loading the hand-written JSON back through the real loader.

use kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf;
use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{convert_file, parse_obj, to_gltf};

/// A unit cube with normals and UVs, as a Blender-style OBJ.
const CUBE_OBJ: &str = "\
# a comment that must be ignored
mtllib ignored.mtl
o Cube
v -1.0 -1.0  1.0
v  1.0 -1.0  1.0
v  1.0  1.0  1.0
v -1.0  1.0  1.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.0 1.0
vn 0.0 0.0 1.0
usemtl Material
s off
f 1/1/1 2/2/1 3/3/1
f 1/1/1 3/3/1 4/4/1
";

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kataglyphis_obj_gltf_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn parses_positions_normals_uvs_and_triangulates() {
    let mesh = parse_obj(CUBE_OBJ).expect("the cube must parse");

    assert_eq!(
        mesh.triangle_count(),
        2,
        "two faces should produce two triangles"
    );
    // Four distinct position/uv/normal triples, all sharing one normal.
    assert_eq!(mesh.positions.len(), 4);
    assert_eq!(mesh.normals.len(), 4);
    assert_eq!(mesh.uvs.len(), 4);

    let (min, max) = mesh.bounds();
    assert_eq!(min, [-1.0, -1.0, 1.0]);
    assert_eq!(max, [1.0, 1.0, 1.0]);
}

#[test]
fn the_v_axis_is_flipped_for_gltf() {
    // OBJ's V points up, glTF's down; a missing flip mirrors every texture vertically.
    let mesh = parse_obj(CUBE_OBJ).expect("parse");
    // OBJ vt 0.0 0.0 -> glTF 0.0 1.0
    assert!(
        mesh.uvs.iter().any(|uv| (uv[1] - 1.0).abs() < 1e-6),
        "expected a flipped V of 1.0 among {:?}",
        mesh.uvs
    );
}

#[test]
fn quads_are_fan_triangulated() {
    let quad = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
f 1 2 3 4
";
    let mesh = parse_obj(quad).expect("parse");
    assert_eq!(mesh.triangle_count(), 2, "a quad must become two triangles");
    assert_eq!(
        mesh.positions.len(),
        4,
        "fan triangulation must not duplicate vertices"
    );
}

#[test]
fn malformed_input_is_rejected_rather_than_guessed_at() {
    // A silent fix-up of any of these would make the asset differ from its source unnoticed.
    assert!(
        parse_obj("v 1.0 2.0\nf 1 1 1\n").is_err(),
        "a 2-component vertex must be rejected"
    );
    assert!(
        parse_obj("v 0 0 0\nf 1 2 3\n").is_err(),
        "an out-of-range index must be rejected"
    );
    assert!(
        parse_obj("v 0 0 0\nf -1 -2 -3\n").is_err(),
        "relative indices are unsupported, not ignored"
    );
    assert!(
        parse_obj("v 0 0 0\nf 0 0 0\n").is_err(),
        "index 0 is invalid in OBJ"
    );
    assert!(
        parse_obj("# nothing here\n").is_err(),
        "an empty OBJ must not produce an empty mesh silently"
    );
    assert!(
        parse_obj("curv 0 1 2\n").is_err(),
        "an unsupported directive must not be skipped quietly"
    );
}

#[test]
fn converted_gltf_loads_back_with_matching_geometry() {
    let dir = temp_dir("roundtrip");
    let obj_path = dir.join("cube.obj");
    let gltf_path = dir.join("cube.gltf");
    std::fs::write(&obj_path, CUBE_OBJ).expect("write obj");

    let source = convert_file(&obj_path, &gltf_path).expect("conversion must succeed");

    // The renderer's own loader, not a bespoke parser.
    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    assert_eq!(scene.primitives.len(), 1);
    let loaded = &scene.primitives[0];

    assert_eq!(
        loaded.indices.len(),
        source.indices.len(),
        "index count changed through the round trip"
    );
    assert_eq!(
        loaded.vertices.len(),
        source.positions.len(),
        "vertex count changed through the round trip"
    );

    for (index, vertex) in loaded.vertices.iter().enumerate() {
        for axis in 0..3 {
            assert!(
                (vertex.position[axis] - source.positions[index][axis]).abs() < 1e-6,
                "vertex {index} position differs on axis {axis}: {:?} vs {:?}",
                vertex.position,
                source.positions[index]
            );
        }
    }
}

#[test]
fn the_declared_buffer_length_matches_the_bytes_written() {
    // Loaders trust byteLength, so this also guards the offset arithmetic.
    let mesh = parse_obj(CUBE_OBJ).expect("parse");
    let (json, bin) = to_gltf(&mesh, "cube.bin");

    let declared = json
        .split("\"byteLength\": ")
        .nth(1)
        .and_then(|rest| rest.split([',', ' ', '}']).next())
        .and_then(|value| value.parse::<usize>().ok())
        .expect("the buffer must declare a byteLength");

    assert_eq!(
        declared,
        bin.len(),
        "declared buffer length disagrees with the data"
    );
}

#[test]
fn converts_a_real_engine_asset() {
    // A C++ engine model; skips when the superproject tree is absent.
    let obj = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../Resources/Models/ShadowTest/shadow_rig.obj");
    if !obj.exists() {
        eprintln!("SKIP: {} not present", obj.display());
        return;
    }

    let dir = temp_dir("engine_asset");
    let gltf_path = dir.join("shadow_rig.gltf");
    let mesh = convert_file(&obj, &gltf_path).expect("engine asset must convert");
    assert!(mesh.triangle_count() > 0);

    let scene = load_gltf(&gltf_path).expect("converted engine asset must load");
    assert_eq!(scene.primitives.len(), 1);
    assert_eq!(scene.primitives[0].indices.len(), mesh.indices.len());
}

const TWO_MATERIAL_OBJ: &str = "\
mtllib pair.mtl
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
vn 0.0 0.0 1.0
usemtl red
f 1//1 2//1 3//1
usemtl blue
f 1//1 3//1 4//1
";

const PAIR_MTL: &str = "\
newmtl red
Ka 0.1 0.1 0.1
Kd 0.9 0.1 0.05
Ks 0.5 0.5 0.5
Ns 32
d 1
illum 2

newmtl blue
Kd 0.05 0.15 0.85
d 0.4
";

#[test]
fn mtl_diffuse_and_opacity_become_base_color() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    let materials = parse_mtl(PAIR_MTL);
    assert_eq!(materials.len(), 2);

    assert_eq!(materials[0].name, "red");
    assert!((materials[0].base_color[0] - 0.9).abs() < 1e-6);
    assert!(
        (materials[0].base_color[3] - 1.0).abs() < 1e-6,
        "d 1 is fully opaque"
    );

    assert_eq!(materials[1].name, "blue");
    assert!((materials[1].base_color[2] - 0.85).abs() < 1e-6);
    assert!(
        (materials[1].base_color[3] - 0.4).abs() < 1e-6,
        "d 0.4 is the alpha"
    );
}

#[test]
fn tr_is_inverted_relative_to_d() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    // Tr = 1 - d; confusing them swaps opaque and transparent materials.
    let by_opacity = parse_mtl("newmtl a\nd 0.25\n");
    let by_transparency = parse_mtl("newmtl a\nTr 0.25\n");

    assert!((by_opacity[0].base_color[3] - 0.25).abs() < 1e-6);
    assert!((by_transparency[0].base_color[3] - 0.75).abs() < 1e-6);
}

#[test]
fn mtl_emissive_becomes_emissive_factor() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nKe 0.5 0.25 0.0\n");
    assert_eq!(materials[0].emissive, [0.5, 0.25, 0.0]);

    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""emissiveFactor": [0.5, 0.25, 0]"#),
        "expected an emissiveFactor entry, got: {json}"
    );
}

#[test]
fn mtl_without_ke_emits_no_emissive_factor() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nKd 1 0 0\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        !json.contains("emissiveFactor"),
        "a material with no Ke must not emit emissiveFactor: {json}"
    );
}

#[test]
fn an_hdr_ke_emits_emissive_strength() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    // The C++ engine keeps HDR `Ke` verbatim, so carry it via emissive_strength, not a clamp.
    let materials = parse_mtl("newmtl a\nKe 4 4 4\n");
    assert_eq!(
        materials[0].emissive,
        [4.0, 4.0, 4.0],
        "Ke must be stored unclamped"
    );

    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""emissiveFactor": [1, 1, 1]"#),
        "expected a normalised emissiveFactor of [1, 1, 1], got: {json}"
    );
    assert!(
        json.contains(
            r#""extensions": { "KHR_materials_emissive_strength": { "emissiveStrength": 4 } }"#
        ),
        "expected a KHR_materials_emissive_strength extension, got: {json}"
    );
    assert!(
        json.contains(r#""extensionsUsed": ["KHR_materials_emissive_strength"]"#),
        "expected extensionsUsed to name the extension, got: {json}"
    );
}

#[test]
fn a_non_hdr_ke_emits_no_emissive_strength_extension() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    // Non-HDR output stays byte-identical: no unconditional extensions arrays.
    let materials = parse_mtl("newmtl a\nKe 0.5 0.5 0.5\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        !json.contains("extensions"),
        "a non-HDR Ke must not emit an extensions block: {json}"
    );
    assert!(
        !json.contains("extensionsUsed"),
        "a non-HDR Ke must not emit extensionsUsed: {json}"
    );
}

#[test]
fn an_hdr_ke_round_trips_through_the_real_gltf_loader() {
    // The loader folds factor * strength back to HDR, so both renderers agree on brightness.
    let dir = temp_dir("hdr_emissive");
    std::fs::write(dir.join("hdr.mtl"), "newmtl painted\nKd 1 1 1\nKe 4 4 4\n").expect("mtl");
    let obj_path = dir.join("hdr.obj");
    std::fs::write(
        &obj_path,
        "mtllib hdr.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nusemtl painted\nf 1 2 3\n",
    )
    .expect("obj");
    let gltf_path = dir.join("hdr.gltf");

    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");
    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    let emissive = scene.primitives[0].material.emissive_factor;
    for channel in emissive {
        assert!(
            (channel - 4.0).abs() < 1e-4,
            "expected the HDR emissive factor to survive the round trip as 4.0, got {emissive:?}"
        );
    }
}

#[test]
fn each_usemtl_run_becomes_its_own_primitive() {
    let dir = temp_dir("materials");
    std::fs::write(dir.join("pair.mtl"), PAIR_MTL).expect("write mtl");
    let obj_path = dir.join("pair.obj");
    std::fs::write(&obj_path, TWO_MATERIAL_OBJ).expect("write obj");
    let gltf_path = dir.join("pair.gltf");

    let mesh = convert_file(&obj_path, &gltf_path).expect("conversion must succeed");
    assert_eq!(
        mesh.submeshes.len(),
        2,
        "two usemtl runs should produce two submeshes"
    );

    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    assert_eq!(
        scene.primitives.len(),
        2,
        "each material run should load as its own primitive"
    );

    // The colours must survive, and land on the right half.
    let red = scene
        .primitives
        .iter()
        .find(|p| p.material.base_color[0] > 0.5)
        .expect("a red-dominant primitive");
    let blue = scene
        .primitives
        .iter()
        .find(|p| p.material.base_color[2] > 0.5)
        .expect("a blue-dominant primitive");

    assert!((red.material.base_color[0] - 0.9).abs() < 1e-4);
    assert!((blue.material.base_color[2] - 0.85).abs() < 1e-4);
    assert!(
        (blue.material.base_color[3] - 0.4).abs() < 1e-4,
        "the transparent material lost its alpha: {:?}",
        blue.material.base_color
    );
}

#[test]
fn material_runs_share_one_vertex_buffer() {
    // Per-material vertex data would duplicate shared vertices and change the geometry.
    let dir = temp_dir("shared_vertices");
    std::fs::write(dir.join("pair.mtl"), PAIR_MTL).expect("write mtl");
    let obj_path = dir.join("pair.obj");
    std::fs::write(&obj_path, TWO_MATERIAL_OBJ).expect("write obj");
    let gltf_path = dir.join("pair.gltf");

    let mesh = convert_file(&obj_path, &gltf_path).expect("convert");
    assert_eq!(
        mesh.positions.len(),
        4,
        "the quad's four corners must not be duplicated"
    );

    let scene = load_gltf(&gltf_path).expect("load");
    let total_indices: usize = scene.primitives.iter().map(|p| p.indices.len()).sum();
    assert_eq!(
        total_indices,
        mesh.indices.len(),
        "index data changed across the split"
    );
}

#[test]
fn an_obj_without_materials_still_gets_a_usable_default() {
    // glTF's default material (metallic 1, roughness 1) renders as a dark mirror.
    let dir = temp_dir("no_materials");
    let obj_path = dir.join("cube.obj");
    std::fs::write(&obj_path, CUBE_OBJ).expect("write obj");
    let gltf_path = dir.join("cube.gltf");

    convert_file(&obj_path, &gltf_path).expect("convert");
    let scene = load_gltf(&gltf_path).expect("load");

    let material = &scene.primitives[0].material;
    assert!(
        material.metallic_factor < 0.01,
        "converted assets must not be metallic by default"
    );
    assert!(
        material.base_color[0] > 0.9,
        "an untextured OBJ should convert to an untinted material"
    );
}

#[test]
fn a_usemtl_naming_an_undeclared_material_is_visible_not_silent() {
    // An undeclared material is an authoring error; mapping it to material 0 would look intended.
    let mesh = parse_obj("v 0 0 0\nv 1 0 0\nv 1 1 0\nusemtl ghost\nf 1 2 3\n").expect("parse");
    assert!(
        mesh.materials.iter().any(|m| m.name == "ghost"),
        "the undeclared name should survive into the output: {:?}",
        mesh.materials.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
}

const TEXTURED_OBJ: &str = "\
mtllib textured.mtl
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vn 0.0 0.0 1.0
usemtl painted
f 1/1/1 2/2/1 3/3/1
";

/// Writes a 2x2 PNG with distinct corners, so a round trip proves the pixels survived.
fn write_test_png(path: &std::path::Path) {
    use image::{ImageBuffer, Rgba};
    let mut buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(2, 2);
    buffer.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    buffer.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
    buffer.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
    buffer.put_pixel(1, 1, Rgba([255, 255, 0, 255]));
    buffer.save(path).expect("write png");
}

#[test]
fn map_kd_takes_the_filename_not_the_first_option() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    // MTL options precede the path, so the first token would be "-s".
    let plain = parse_mtl("newmtl a\nmap_Kd wood.png\n");
    assert_eq!(plain[0].base_color_texture.as_deref(), Some("wood.png"));

    let with_options = parse_mtl("newmtl a\nmap_Kd -s 1 1 1 -o 0 0 0 wood.png\n");
    assert_eq!(
        with_options[0].base_color_texture.as_deref(),
        Some("wood.png"),
        "the filename must be taken from the end, past the options"
    );
}

#[test]
fn a_textured_obj_converts_to_a_gltf_with_a_loadable_texture() {
    let dir = temp_dir("textured");
    write_test_png(&dir.join("paint.png"));
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl painted\nKd 1 1 1\nmap_Kd paint.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");

    let out = temp_dir("textured_out");
    let gltf_path = out.join("textured.gltf");
    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");

    // Copied next to the output, or the document loads only where it was made.
    assert!(
        out.join("paint.png").exists(),
        "the referenced texture was not copied next to the glTF"
    );

    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    let material = &scene.primitives[0].material;
    let texture = material
        .base_color_texture
        .as_ref()
        .expect("the material must carry a base colour texture");

    assert_eq!((texture.texture.width, texture.texture.height), (2, 2));
    assert!(texture.srgb, "base colour is authored in sRGB");
    // Top-left red, proving pixel data survived rather than just a reference.
    assert_eq!(&texture.texture.rgba8[0..4], &[255, 0, 0, 255]);
}

#[test]
fn a_textures_subdirectory_layout_is_resolved() {
    // Engine OBJs name bare files living in `textures/`; see the superproject's model-loading.md.
    let dir = temp_dir("textures_subdir");
    let texture_dir = dir.join("textures");
    std::fs::create_dir_all(&texture_dir).expect("textures dir");
    write_test_png(&texture_dir.join("paint.png"));
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl painted\nKd 1 1 1\nmap_Kd paint.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");

    let out = temp_dir("textures_subdir_out");
    let gltf_path = out.join("textured.gltf");
    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");

    // The bare filename is copied and emitted, whichever candidate resolved it.
    assert!(
        out.join("paint.png").exists(),
        "the textures/-subdirectory texture was not copied next to the glTF"
    );

    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    let texture = scene.primitives[0]
        .material
        .base_color_texture
        .as_ref()
        .expect("the material must carry a base colour texture");
    assert_eq!((texture.texture.width, texture.texture.height), (2, 2));
}

#[test]
fn a_backslash_map_kd_under_textures_is_resolved() {
    // A backslash must normalise before resolution, or Linux treats it as a filename character.
    let dir = temp_dir("backslash_subdir");
    let texture_dir = dir.join("textures");
    std::fs::create_dir_all(&texture_dir).expect("textures dir");
    write_test_png(&texture_dir.join("paint.png"));
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl painted\nKd 1 1 1\nmap_Kd textures\\paint.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");
    let gltf_path = dir.join("textured.gltf");

    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");

    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    assert!(
        scene.primitives[0].material.base_color_texture.is_some(),
        "the backslash-authored texture under textures/ must still resolve"
    );
}

#[test]
fn materials_sharing_a_map_emit_one_image() {
    let dir = temp_dir("shared_texture");
    write_test_png(&dir.join("shared.png"));
    std::fs::write(
        dir.join("shared.mtl"),
        "newmtl a\nKd 1 0 0\nmap_Kd shared.png\n\nnewmtl b\nKd 0 0 1\nmap_Kd shared.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("shared.obj");
    std::fs::write(
        &obj_path,
        "mtllib shared.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\n\
usemtl a\nf 1/1/1 2/2/1 3/3/1\nusemtl b\nf 1/1/1 3/3/1 4/4/1\n",
    )
    .expect("obj");

    let gltf_path = dir.join("shared.gltf");
    convert_file(&obj_path, &gltf_path).expect("convert");

    // One image per material would decode and upload the same file repeatedly.
    let json = std::fs::read_to_string(&gltf_path).expect("read gltf");
    let image_count = json.matches(r#""uri": "shared.png""#).count();
    assert_eq!(
        image_count, 1,
        "the shared texture should appear once, got {image_count}"
    );

    let scene = load_gltf(&gltf_path).expect("load");
    assert_eq!(scene.primitives.len(), 2);
    for primitive in &scene.primitives {
        assert!(
            primitive.material.base_color_texture.is_some(),
            "both materials should reference the shared texture"
        );
    }
}

#[test]
fn a_missing_texture_does_not_abort_the_conversion() {
    // OBJs routinely reference unshipped maps; geometry and colours still convert.
    let dir = temp_dir("missing_texture");
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl painted\nKd 0.2 0.4 0.6\nmap_Kd absent.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");
    let gltf_path = dir.join("textured.gltf");

    let mesh = convert_file(&obj_path, &gltf_path).expect("conversion must still succeed");
    assert_eq!(mesh.triangle_count(), 1);
    assert!(
        (mesh.materials[0].base_color[2] - 0.6).abs() < 1e-6,
        "base colour must survive"
    );
}

#[test]
fn an_untextured_obj_emits_no_texture_arrays() {
    // Empty arrays are noisy, and a texture naming sampler 0 of an empty array is invalid.
    let dir = temp_dir("untextured");
    let obj_path = dir.join("cube.obj");
    std::fs::write(&obj_path, CUBE_OBJ).expect("obj");
    let gltf_path = dir.join("cube.gltf");
    convert_file(&obj_path, &gltf_path).expect("convert");

    let json = std::fs::read_to_string(&gltf_path).expect("read");
    assert!(
        !json.contains("\"images\""),
        "no images array should be emitted"
    );
    assert!(
        !json.contains("\"textures\""),
        "no textures array should be emitted"
    );
    load_gltf(&gltf_path).expect("an untextured conversion must still load");
}

#[test]
fn a_material_name_with_json_metacharacters_still_produces_loadable_gltf() {
    // A quote and trailing backslash would end or escape the JSON string; declared materials all emit.
    let dir = temp_dir("quoted_material_name");
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl he said \"hi\"\\\\\nKd 1 0 0\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");
    let gltf_path = dir.join("quoted.gltf");

    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");
    load_gltf(&gltf_path).expect("the converted glTF must load despite the quote/backslash");
}

#[test]
fn a_backslash_texture_path_does_not_corrupt_the_document() {
    // A raw `textures\wood.png` in JSON makes `\w` an invalid escape.
    let dir = temp_dir("backslash_texture");
    let texture_dir = dir.join("textures");
    std::fs::create_dir_all(&texture_dir).expect("textures dir");
    write_test_png(&texture_dir.join("wood.png"));
    std::fs::write(
        dir.join("textured.mtl"),
        "newmtl painted\nKd 1 1 1\nmap_Kd textures\\wood.png\n",
    )
    .expect("mtl");
    let obj_path = dir.join("textured.obj");
    std::fs::write(&obj_path, TEXTURED_OBJ).expect("obj");
    let gltf_path = dir.join("textured.gltf");

    convert_file(&obj_path, &gltf_path).expect("conversion must still succeed");
    load_gltf(&gltf_path).expect("the converted glTF must load despite the backslash path");
}

#[test]
fn an_empty_mesh_emits_finite_accessor_bounds() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::ObjMesh;

    // Empty `bounds()` is +/-infinity, which is not a valid JSON number.
    let (json, _bin) = to_gltf(&ObjMesh::default(), "empty.bin");

    assert!(
        !json.contains("inf"),
        "accessor bounds must not contain inf"
    );
    assert!(
        !json.contains("NaN"),
        "accessor bounds must not contain NaN"
    );
}

#[test]
fn per_vertex_colors_are_parsed_from_the_v_line() {
    let mesh = parse_obj("v 0 0 0 1 0 0\nv 1 0 0 1 0 0\nv 1 1 0 1 0 0\nf 1 2 3\n").expect("parse");
    for color in &mesh.colors {
        assert_eq!(*color, [1.0, 0.0, 0.0, 1.0]);
    }
    assert!(mesh.has_vertex_colors);
}

#[test]
fn a_seven_component_v_line_carries_alpha() {
    let mesh = parse_obj(
        "v 0 0 0 0.2 0.4 0.6 0.5\nv 1 0 0 0.2 0.4 0.6 0.5\nv 1 1 0 0.2 0.4 0.6 0.5\nf 1 2 3\n",
    )
    .expect("parse");
    for color in &mesh.colors {
        assert_eq!(*color, [0.2, 0.4, 0.6, 0.5]);
    }
}

#[test]
fn a_colorless_obj_has_no_vertex_colors_and_no_color_0() {
    let mesh = parse_obj(CUBE_OBJ).expect("parse");
    assert!(
        !mesh.has_vertex_colors,
        "a colourless OBJ must not be flagged as carrying colours"
    );

    let (json, _bin) = to_gltf(&mesh, "cube.bin");
    assert!(
        !json.contains("COLOR_0"),
        "no COLOR_0 attribute should be emitted for a colourless mesh"
    );
}

#[test]
fn a_colorless_obj_index_accessors_are_unchanged() {
    // The COLOR_0 slot must not repoint `indices` for a colourless mesh.
    let mesh = parse_obj(CUBE_OBJ).expect("parse");
    let (json, _bin) = to_gltf(&mesh, "cube.bin");

    assert!(
        json.contains(r#""indices": 3"#),
        "a colourless mesh's single primitive must still address index accessor 3: {json}"
    );
    assert!(
        json.contains(r#""bufferView": 3, "byteOffset": 0, "componentType": 5125"#),
        "the index accessor must still view bufferView 3: {json}"
    );
}

#[test]
fn a_vn_less_obj_gets_a_geometric_flat_normal_not_an_up_vector() {
    // XZ-plane triangles without `vn`; an up-vector fallback also passes, see the XY case.
    let mesh = parse_obj("v 0 0 0\nv 1 0 0\nv 1 0 1\nv 0 0 1\nf 1 2 3\nf 1 3 4\n").expect("parse");
    assert!(!mesh.has_normals, "no vn line appeared in the source");
    for normal in &mesh.normals {
        assert!(
            (normal[1].abs() - 1.0).abs() < 1e-6
                && normal[0].abs() < 1e-6
                && normal[2].abs() < 1e-6,
            "expected (0, +/-1, 0), got {normal:?}"
        );
    }
}

#[test]
fn a_vn_less_obj_in_the_xy_plane_gets_a_z_normal_not_a_fabricated_up_vector() {
    // An XY-plane triangle must get normal Z, not a fabricated up vector.
    let mesh = parse_obj("v 0 0 0\nv 1 0 0\nv 1 1 0\nf 1 2 3\n").expect("parse");
    for normal in &mesh.normals {
        assert!(
            (normal[2].abs() - 1.0).abs() < 1e-6
                && normal[0].abs() < 1e-6
                && normal[1].abs() < 1e-6,
            "expected (0, 0, +/-1), got {normal:?}"
        );
    }
}

#[test]
fn a_mixed_obj_preserves_explicit_normals_and_fills_the_rest_geometrically() {
    // An explicit `vn` survives verbatim; the vn-less face gets its own geometric normal.
    let mesh = parse_obj(
        "\
v 0 0 0
v 1 0 0
v 1 1 0
v 0 0 0
v 1 0 0
v 1 0 1
vn 1.0 0.0 0.0
f 1//1 2//1 3//1
f 4 5 6
",
    )
    .expect("parse");
    assert!(mesh.has_normals);

    // The explicit normal disagrees with the geometry, proving it was not recomputed.
    for i in 0..3 {
        assert_eq!(mesh.normals[i], [1.0, 0.0, 0.0]);
    }
    // The XZ-plane face without vn gets its own flat normal.
    for i in 3..6 {
        let n = mesh.normals[i];
        assert!(
            (n[1].abs() - 1.0).abs() < 1e-6 && n[0].abs() < 1e-6 && n[2].abs() < 1e-6,
            "expected the second face's own geometric normal (0, +/-1, 0), got {n:?}"
        );
    }
}

#[test]
fn parse_mtl_reads_map_bump_as_the_normal_texture() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    let materials = parse_mtl("newmtl a\nmap_Bump rock_normal.png\n");
    assert_eq!(
        materials[0].normal_texture.as_deref(),
        Some("rock_normal.png")
    );
}

#[test]
fn parse_mtl_prefers_norm_over_map_bump() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    // norm first: the later, less specific map_Bump must not override it.
    let norm_first = parse_mtl("newmtl a\nnorm better.png\nmap_Bump worse.png\n");
    assert_eq!(norm_first[0].normal_texture.as_deref(), Some("better.png"));

    // map_Bump first, norm second: norm always wins regardless of order.
    let bump_first = parse_mtl("newmtl a\nmap_Bump worse.png\nnorm better.png\n");
    assert_eq!(bump_first[0].normal_texture.as_deref(), Some("better.png"));
}

#[test]
fn parse_mtl_takes_the_last_token_of_an_option_carrying_map_bump() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    // Options precede the path; `-bm` sets normalTexture.scale.
    let materials = parse_mtl("newmtl a\nmap_Bump -bm 0.5 rock_normal.png\n");
    assert_eq!(
        materials[0].normal_texture.as_deref(),
        Some("rock_normal.png"),
        "the filename must be taken from the end, past the -bm option"
    );
    assert!(
        (materials[0].normal_scale - 0.5).abs() < 1e-6,
        "the -bm option must set normal_scale, got {}",
        materials[0].normal_scale
    );
}

#[test]
fn to_gltf_emits_a_normal_texture_pointing_at_a_distinct_image_when_the_maps_differ() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nmap_Kd base.png\nmap_Bump normal.png\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");

    assert!(
        json.contains(r#""baseColorTexture": { "index": 0 }"#),
        "expected the base colour texture at image index 0, got: {json}"
    );
    assert!(
        json.contains(r#""normalTexture": { "index": 1, "scale": 1 }"#),
        "expected a distinct normalTexture image index, got: {json}"
    );
}

#[test]
fn to_gltf_shares_one_image_when_map_kd_and_map_bump_name_the_same_file() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nmap_Kd shared.png\nmap_Bump shared.png\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");

    assert!(
        json.contains(r#""normalTexture": { "index": 0, "scale": 1 }"#),
        "expected the base colour and normal maps to share image index 0, got: {json}"
    );
    assert_eq!(
        json.matches(r#""uri": "shared.png""#).count(),
        1,
        "the shared file must appear once in the images array, got: {json}"
    );
}

#[test]
fn parse_mtl_takes_the_last_token_of_an_option_carrying_map_ke() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    // As for map_Kd/map_Bump, the path is the last token.
    let materials = parse_mtl("newmtl a\nmap_Ke -s 1 1 1 glow.png\n");
    assert_eq!(
        materials[0].emissive_texture.as_deref(),
        Some("glow.png"),
        "the filename must be taken from the end, past the options"
    );
}

#[test]
fn to_gltf_emits_an_emissive_texture_pointing_at_the_right_image() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nmap_Kd base.png\nmap_Ke glow.png\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");

    assert!(
        json.contains(r#""baseColorTexture": { "index": 0 }"#),
        "expected the base colour texture at image index 0, got: {json}"
    );
    assert!(
        json.contains(r#""emissiveTexture": { "index": 1 }"#),
        "expected a distinct emissiveTexture image index, got: {json}"
    );
}

#[test]
fn a_map_ke_without_ke_gets_a_normalised_emissive_factor() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    // glTF's default emissiveFactor [0,0,0] would render a map_Ke without Ke black.
    let materials = parse_mtl("newmtl a\nmap_Ke glow.png\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");

    assert!(
        json.contains(r#""emissiveFactor": [1, 1, 1]"#),
        "expected a normalised emissiveFactor of [1, 1, 1], got: {json}"
    );
}

#[test]
fn to_gltf_shares_one_image_when_map_kd_and_map_ke_name_the_same_file() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nmap_Kd wood.png\nmap_Ke wood.png\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");

    assert_eq!(
        json.matches(r#""uri": "wood.png""#).count(),
        1,
        "the shared file must appear once in the images array, got: {json}"
    );
}

#[test]
fn parse_mtl_reads_the_pbr_channels() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::parse_mtl;

    let materials = parse_mtl("newmtl a\nPm 1.0\nPr 0.25\n");
    assert_eq!(materials[0].metallic, Some(1.0));
    assert_eq!(materials[0].roughness, Some(0.25));

    let absent = parse_mtl("newmtl a\nKd 1 0 0\n");
    assert_eq!(
        absent[0].metallic, None,
        "Pm must stay None, not fall back to 0.0, when absent"
    );
    assert_eq!(
        absent[0].roughness, None,
        "Pr must stay None, not fall back to 1.0, when absent"
    );
}

#[test]
fn to_gltf_emits_the_authored_pbr_channels_and_falls_back_when_absent() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let authored = parse_mtl("newmtl a\nPm 1.0\nPr 0.25\n");
    let mesh = ObjMesh {
        materials: authored,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""metallicFactor": 1, "roughnessFactor": 0.25"#),
        "expected the authored Pm/Pr values, got: {json}"
    );

    let unauthored = parse_mtl("newmtl a\nKd 1 0 0\n");
    let mesh = ObjMesh {
        materials: unauthored,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""metallicFactor": 0.0, "roughnessFactor": 1.0"#),
        "expected the byte-identical 0.0/1.0 fallback, got: {json}"
    );
}

#[test]
fn ns_without_pr_derives_roughness_via_the_shininess_curve() {
    use kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf;
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{convert_file, parse_mtl};

    // sqrt(2/(Ns+2)), the C++ engine's material_rules.slang curve.
    let materials = parse_mtl("newmtl a\nNs 96\n");
    assert_eq!(materials[0].shininess, Some(96.0));

    let dir = temp_dir("ns_roughness");
    std::fs::write(dir.join("shiny.mtl"), "newmtl painted\nKd 1 1 1\nNs 96\n").expect("mtl");
    let obj_path = dir.join("shiny.obj");
    std::fs::write(
        &obj_path,
        TEXTURED_OBJ.replace("mtllib textured.mtl", "mtllib shiny.mtl"),
    )
    .expect("obj");
    let gltf_path = dir.join("shiny.gltf");

    convert_file(&obj_path, &gltf_path).expect("conversion must succeed");
    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    let roughness = scene.primitives[0].material.roughness_factor;
    assert!(
        (roughness - (2.0f32 / 98.0).sqrt()).abs() < 1e-5,
        "expected roughness derived from Ns 96, got {roughness}"
    );
}

#[test]
fn an_authored_pr_wins_over_ns() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nNs 96\nPr 0.3\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""roughnessFactor": 0.3"#),
        "an authored Pr must win over a derived-from-Ns roughness, got: {json}"
    );
}

#[test]
fn neither_pr_nor_ns_keeps_the_byte_identical_fallback() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    let materials = parse_mtl("newmtl a\nKd 1 0 0\n");
    let mesh = ObjMesh {
        materials,
        ..ObjMesh::default()
    };
    let (json, _bin) = to_gltf(&mesh, "a.bin");
    assert!(
        json.contains(r#""roughnessFactor": 1.0"#),
        "a .mtl with neither Pr nor Ns must still emit the literal 1.0, got: {json}"
    );
}

#[test]
fn an_invalid_ns_falls_back_to_the_literal_one_rather_than_nan() {
    use kataglyphis_webgpu_renderer::asset::obj_to_gltf::{parse_mtl, ObjMesh};

    for source in ["newmtl a\nNs -1\n", "newmtl a\nNs notanumber\n"] {
        let materials = parse_mtl(source);
        let mesh = ObjMesh {
            materials,
            ..ObjMesh::default()
        };
        let (json, _bin) = to_gltf(&mesh, "a.bin");
        assert!(
            !json.contains("NaN"),
            "invalid Ns must never produce NaN in the document: {json}"
        );
        assert!(
            json.contains(r#""roughnessFactor": 1.0"#),
            "invalid Ns ({source:?}) must fall back to the literal 1.0, got: {json}"
        );
    }
}

#[test]
fn vertex_colors_round_trip_through_the_real_gltf_loader() {
    let dir = temp_dir("vertex_colors");
    let obj_path = dir.join("triangle.obj");
    std::fs::write(
        &obj_path,
        "v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 1 1 0 0 0 1\nf 1 2 3\n",
    )
    .expect("write obj");
    let gltf_path = dir.join("triangle.gltf");

    let source = convert_file(&obj_path, &gltf_path).expect("conversion must succeed");
    assert!(source.has_vertex_colors);

    let scene = load_gltf(&gltf_path).expect("the converted glTF must load");
    let loaded = &scene.primitives[0];
    assert_eq!(loaded.vertices.len(), source.colors.len());
    for (index, vertex) in loaded.vertices.iter().enumerate() {
        for channel in 0..4 {
            assert!(
                (vertex.color[channel] - source.colors[index][channel]).abs() < 1e-6,
                "vertex {index} colour differs on channel {channel}: {:?} vs {:?}",
                vertex.color,
                source.colors[index]
            );
        }
    }
}
