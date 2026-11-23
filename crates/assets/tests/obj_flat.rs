//! The OBJ as BeschleunigerBallett's engine reads it through the cxx bridge: `obj::load_obj_flat`.

use std::path::{Path, PathBuf};

use kataglyphis_assets::flat::FlatModel;
use kataglyphis_assets::gltf_loader::load_gltf_slice;
use kataglyphis_assets::obj::{load_obj_flat, obj_to_glb, parse_obj};
use kataglyphis_assets::scene::AlphaMode;
use kataglyphis_assets::LoadOptions;

fn mask_card() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/obj_mask/card.obj")
}

fn load(path: &Path) -> FlatModel {
    load_obj_flat(path, &LoadOptions::default()).expect("the OBJ loads")
}

/// `obj` and `mtl` written as `model.obj` and `model.mtl` into a fresh directory.
fn write_model(obj: &str, mtl: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("model.mtl"), mtl).unwrap();
    let path = dir.path().join("model.obj");
    std::fs::write(&path, format!("mtllib model.mtl\n{obj}")).unwrap();
    (dir, path)
}

/// One triangle per material, in the order the materials are declared.
fn triangles_using(materials: &[&str]) -> String {
    let mut obj = String::new();
    for (k, name) in materials.iter().enumerate() {
        let x = k as f32 * 2.0;
        obj += &format!(
            "v {x} 0 0\nv {} 0 0\nv {x} 1 0\nvt 0 0\nusemtl {name}\nf -3/1 -2/1 -1/1\n",
            x + 1.0
        );
    }
    obj
}

/// Every material rule the owner set on 2026-11-19, one material each.
const RULES_MTL: &str = "\
newmtl blend_d\nKd 1 0 0\nd 0.5\nNs 10\n\
newmtl blend_tr\nKd 0 1 0\nTr 0.25\n\
newmtl glow_map\nmap_Ke glow.png\n\
newmtl plain\nNs 10\n\
newmtl authored\nKd 0.2 0.3 0.4\nPr 0.3\nPm 0.7\nNs 900\nKe 2 1 0\n\
newmtl d_beats_tr\nTr 0.5\nd 1\n";
const RULES: [&str; 6] = [
    "blend_d",
    "blend_tr",
    "glow_map",
    "plain",
    "authored",
    "d_beats_tr",
];

#[test]
fn the_owners_material_rules_reach_the_engine() {
    let (dir, path) = write_model(&triangles_using(&RULES), RULES_MTL);
    std::fs::write(dir.path().join("glow.png"), b"\x89PNG not decoded here").unwrap();
    let model = load(&path);
    let m = &model.materials;
    assert_eq!(
        m.len(),
        RULES.len() + 1,
        "one per newmtl, then the neutral fallback"
    );
    assert!(m[0].alpha_blend && m[0].dissolve == 0.5, "d below 1 blends");
    assert!(
        m[1].alpha_blend && m[1].dissolve == 0.75,
        "Tr above 0 blends"
    );
    assert_eq!(
        m[2].emission, [1.0; 3],
        "map_Ke without Ke glows at factor 1"
    );
    assert!(m[2].emissive_texture >= 0);
    assert_eq!(m[2].diffuse, [1.0; 3], "no Kd is white");
    assert_eq!(
        m[3].roughness,
        (2.0f32 / 12.0).sqrt(),
        "Ns without Pr takes the shininess curve"
    );
    assert_eq!(m[2].roughness, 1.0, "neither Pr nor Ns is roughness 1");
    assert_eq!(
        (m[4].roughness, m[4].metallic),
        (0.3, 0.7),
        "Pr wins over Ns"
    );
    assert_eq!(m[4].emission, [2.0, 1.0, 0.0], "Ke keeps its HDR magnitude");
    assert!(
        !m[5].alpha_blend && m[5].dissolve == 1.0,
        "a d wins over a Tr, as in tinyobjloader"
    );
    assert!(m
        .iter()
        .all(|m| m.alpha_cutoff == -1.0 && m.alpha_texture == -1));
}

#[test]
fn the_engine_and_the_webgpu_renderer_read_one_set_of_material_rules() {
    let (dir, path) = write_model(&triangles_using(&RULES), RULES_MTL);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/obj_mask/red.bmp"),
        dir.path().join("glow.png"),
    )
    .unwrap();
    let flat = load(&path);
    let scene = load_gltf_slice(&obj_to_glb(&path).expect("converts")).expect("the GLB loads");
    assert_eq!(scene.primitives.len(), RULES.len());
    for (primitive, name) in scene.primitives.iter().zip(RULES) {
        let index = RULES.iter().position(|rule| *rule == name).unwrap();
        let (gpu, engine) = (&primitive.material, &flat.materials[index]);
        let strength = gpu.emissive_factor;
        assert_eq!(&gpu.base_color[..3], &engine.diffuse, "{name}");
        assert_eq!(gpu.base_color[3], engine.dissolve, "{name}");
        assert_eq!(
            gpu.alpha_mode == AlphaMode::Blend,
            engine.alpha_blend,
            "{name}"
        );
        assert_eq!(gpu.metallic_factor, engine.metallic, "{name}");
        assert!(
            (gpu.roughness_factor - engine.roughness).abs() < 1e-6,
            "{name}"
        );
        for (a, b) in strength.iter().zip(engine.emission) {
            assert!(
                (a - b).abs() < 1e-6,
                "{name}: emission {strength:?} against {:?}",
                engine.emission
            );
        }
    }
}

#[test]
fn each_usemtl_run_is_one_range_even_when_its_material_returns() {
    let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\n\
               usemtl a\nf 1 2 3\nusemtl b\nf 2 4 3\nusemtl a\nf 1 3 4\n";
    let (_dir, path) = write_model(obj, "newmtl a\nnewmtl b\n");
    let model = load(&path);
    let per_range: Vec<(usize, u32)> = model
        .ranges
        .iter()
        .map(|range| (range.tri_count, model.material_index[range.tri_start]))
        .collect();
    assert_eq!(
        per_range,
        [(1, 0), (1, 1), (1, 0)],
        "owner 2026-11-24: one mesh per usemtl group"
    );
    for range in &model.ranges {
        let indices = &model.indices[range.index_start..range.index_start + range.index_count];
        let block = range.vertex_base..range.vertex_base + range.vertex_count;
        assert!(
            indices.iter().all(|&i| block.contains(&(i as usize))),
            "a range uses only its own vertices"
        );
    }
}

#[test]
fn a_map_d_mask_and_a_bmp_reach_the_engine_as_the_files_bytes() {
    let model = load(&mask_card());
    let material = &model.materials[0];
    assert_eq!(material.alpha_cutoff, 0.5, "map_d cuts out at 0.5");
    assert!(!material.alpha_blend);
    let dir = mask_card().with_file_name("");
    let base = &model.textures[material.base_texture as usize];
    let mask = &model.textures[material.alpha_texture as usize];
    assert_eq!(
        base.encoded,
        std::fs::read(dir.join("red.bmp")).unwrap(),
        "passed through, not re-encoded"
    );
    assert_eq!(
        mask.encoded,
        std::fs::read(dir.join("half_mask.tga")).unwrap()
    );
    assert!(base.srgb && !mask.srgb, "colour in sRGB, the mask linear");
}

#[test]
fn a_texture_named_twice_or_missing_takes_one_slot_or_none() {
    let mtl =
        "newmtl a\nmap_Kd tex.png\nmap_Bump tex.png\nnewmtl b\nmap_Kd ./tex.png\nnorm gone.png\n";
    let (dir, path) = write_model(&triangles_using(&["a", "b"]), mtl);
    std::fs::write(dir.path().join("tex.png"), b"bytes").unwrap();
    let model = load(&path);
    let [a, b] = [&model.materials[0], &model.materials[1]];
    assert_eq!(
        a.base_texture, b.base_texture,
        "two spellings of one file share a slot"
    );
    assert_ne!(
        a.base_texture, a.normal_texture,
        "one file in sRGB and linear takes two"
    );
    assert_eq!(b.normal_texture, -1, "a missing file draws untextured");
    assert_eq!(model.textures.len(), 2);
}

#[test]
fn every_vertex_has_a_unit_normal_and_tangent_and_its_uv_twice() {
    let model = load(&mask_card());
    for v in &model.vertices {
        let length = |x: &[f32]| x.iter().map(|c| c * c).sum::<f32>().sqrt();
        assert!((length(&v.normal) - 1.0).abs() < 1e-5);
        assert!((length(&v.tangent[..3]) - 1.0).abs() < 1e-5);
        assert_eq!(v.uv0, v.uv1, "OBJ's one UV set fills both");
    }
}

#[test]
fn polygons_split_as_tinyobjloader_splits_them() {
    // A quad whose 0-2 diagonal is the longer, then a pentagon with a notch at its corner (2, 1).
    let obj = "v 0 0 0\nv 1 0 0\nv 3 1 0\nv 0 1 0\nf 1 2 3 4\n\
               v 0 0 1\nv 4 0 1\nv 4 2 1\nv 2 1 1\nv 0 2 1\nf 5 6 7 8 9\n";
    let mesh = parse_obj(obj).expect("parse");
    let areas: Vec<f32> = mesh
        .indices
        .chunks(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.positions[t[k] as usize]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.0
        })
        .collect();
    assert_eq!(
        areas,
        [0.5, 1.5, 2.0, 0.0, 4.0],
        "1-2-4 and 2-3-4, then three ears, one of them flat"
    );
}

#[test]
fn an_authored_ke_of_zero_stays_dark_beside_a_map_ke() {
    // Blender writes `Ke 0 0 0` with its map at strength 0; only a map_Ke without any Ke glows at 1.
    let mtl = "newmtl off\nKe 0 0 0\nmap_Ke glow.png\nnewmtl on\nmap_Ke glow.png\n";
    let (dir, path) = write_model(&triangles_using(&["off", "on"]), mtl);
    std::fs::write(dir.path().join("glow.png"), b"png").unwrap();
    let model = load(&path);
    assert_eq!(model.materials[0].emission, [0.0; 3]);
    assert_eq!(model.materials[1].emission, [1.0; 3]);
}

#[test]
fn textures_resolve_beside_their_mtl_even_in_a_subfolder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub/textures")).unwrap();
    std::fs::write(
        dir.path().join("sub/x.mtl"),
        "newmtl a\nmap_Kd near.png\nmap_Ke far.png\nKe 1 1 1\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("sub/near.png"), b"near").unwrap();
    std::fs::write(dir.path().join("sub/textures/far.png"), b"far").unwrap();
    // A file of the same name beside the OBJ must not be taken: the .mtl's folder is the base.
    std::fs::write(dir.path().join("near.png"), b"wrong").unwrap();
    let path = dir.path().join("model.obj");
    std::fs::write(
        &path,
        format!("mtllib sub/x.mtl\n{}", triangles_using(&["a"])),
    )
    .unwrap();
    let model = load(&path);
    let bytes = |slot: i32| model.textures[slot as usize].encoded.clone();
    assert_eq!(bytes(model.materials[0].base_texture), b"near");
    assert_eq!(
        bytes(model.materials[0].emissive_texture),
        b"far",
        "the subfolder's textures/"
    );
}

#[test]
fn an_mtl_or_texture_outside_the_models_folder_is_refused() {
    let outer = tempfile::tempdir().unwrap();
    std::fs::write(outer.path().join("secret.mtl"), "newmtl a\nKd 1 0 0\n").unwrap();
    std::fs::write(outer.path().join("secret.png"), b"secret").unwrap();
    let dir = outer.path().join("model");
    std::fs::create_dir_all(&dir).unwrap();
    let absolute = outer.path().join("secret.png");
    let mtl = format!(
        "newmtl b\nmap_Kd ../secret.png\nnewmtl c\nmap_Kd {}\nnewmtl d\nmap_Kd \\\\server\\share\\secret.png\n\
         newmtl e\nmap_Kd C:/secret.png\nnewmtl f\nmap_Kd //server/secret.png\n",
        absolute.display()
    );
    std::fs::write(dir.join("model.mtl"), mtl).unwrap();
    let absolute_mtl = outer.path().join("secret.mtl");
    let obj = format!(
        "mtllib ../secret.mtl\nmtllib {}\nmtllib C:secret.mtl\nmtllib \\\\server\\secret.mtl\nmtllib model.mtl\n{}",
        absolute_mtl.display(),
        triangles_using(&["a", "b", "c", "d", "e", "f"])
    );
    let path = dir.join("model.obj");
    std::fs::write(&path, obj).unwrap();
    let model = load(&path);
    assert_eq!(
        model.materials[0].diffuse, [1.0; 3],
        "secret.mtl's red never loads; `a` is undeclared, so white"
    );
    assert!(
        model.textures.is_empty(),
        "no texture outside the folder is read"
    );
    assert!(model.materials.iter().all(|m| m.base_texture == -1));
}

#[test]
fn a_coordinate_that_is_not_finite_reads_zero() {
    let mesh = parse_obj("v nan 1 inf\nv 1 0 0\nv 0 1 -inf\nf 1 2 3\n").expect("parse");
    assert_eq!(mesh.positions[0], [0.0, 1.0, 0.0]);
    assert_eq!(mesh.positions[2], [0.0, 1.0, 0.0]);
    assert_eq!(mesh.skipped_lines, 2, "each such line is reported");
}

#[test]
fn a_face_over_the_corner_cap_is_skipped_and_one_at_it_splits() {
    let ring = |n: usize| -> String {
        let mut obj = String::new();
        for k in 0..n {
            let angle = k as f32 * std::f32::consts::TAU / n as f32;
            obj += &format!("v {} {} 0\n", angle.cos(), angle.sin());
        }
        obj + "f" + &(1..=n).map(|k| format!(" {k}")).collect::<String>() + "\n"
    };
    let at_cap = parse_obj(&ring(2048)).expect("a 2048-gon splits");
    assert_eq!(at_cap.triangle_count(), 2046);
    let over = parse_obj(&(ring(2049) + "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n"))
        .expect("the triangle stays");
    assert_eq!((over.triangle_count(), over.skipped_lines), (1, 1));
}

#[test]
fn faces_sharing_a_corner_without_vn_keep_their_own_flat_normals() {
    // A roof ridge: the two slopes share corners 1 and 2 but face different ways.
    let mesh = parse_obj("v 0 1 0\nv 1 1 0\nv 0 0 1\nv 0 0 -1\nf 1 2 3\nf 2 1 4\n").expect("parse");
    assert_eq!(mesh.positions.len(), 6, "no corner without vn is shared");
    let normal = |i: usize| mesh.normals[mesh.indices[i] as usize];
    for k in 0..3 {
        assert_eq!(normal(k), normal(0), "the first slope's corners agree");
        assert_eq!(normal(3 + k), normal(3), "the second slope's corners agree");
    }
    assert_ne!(normal(0), normal(3));
}

#[test]
fn the_bridge_failure_names_the_file() {
    let error = load_obj_flat("/no/such/model.obj", &LoadOptions::default()).unwrap_err();
    assert!(format!("{error:#}").contains("model.obj"), "{error:#}");
}

/// Not-finite coordinates, paths that leave the folder, many materials and a large polygon.
fn seed_hostile() -> (String, String) {
    let mut obj = String::from("mtllib model.mtl\nmtllib ../up.mtl\nmtllib /abs.mtl\nmtllib C:drive.mtl\nmtllib \\\\unc\\x.mtl\n");
    obj += "v nan 0 0\nv inf 1 0\nv 0 -inf 1\nv 1 1 1\n";
    let mut mtl = String::new();
    for m in 0..40 {
        mtl += &format!("newmtl m{m}\nKd 0.{m} 0 0\nmap_Kd ../t{m}.png\nmap_d /abs{m}.png\nnorm \\\\unc\\n{m}.png\n");
        obj += &format!("usemtl m{m}\nf 1 2 3\nf -1 -2 -3 -4\n");
    }
    for k in 0..300 {
        let angle = k as f32 * std::f32::consts::TAU / 300.0;
        obj += &format!(
            "v {} {} 2\n",
            angle.cos(),
            angle.sin() * (1.0 + (k % 3) as f32)
        );
    }
    obj += "f";
    for k in 0..300 {
        obj += &format!(" {}", -300 + k);
    }
    obj += "\n";
    (obj, mtl)
}

fn seed_cube() -> &'static str {
    "mtllib model.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1 0.5 0.5 0.5\nvt 0 0\nvt 1 0\nvt 1 1\n\
     vn 0 0 1\nvn 0 1 0\no a\nusemtl a\nf 1/1/1 2/2/1 3/3/1 4/1/1\ng b\nusemtl b\nf -1//2 -2//2 -3//2 -4//2 -5//2\n\
     f 1 2 6\nl 1 2\ns off\n"
}

#[derive(Clone, Debug)]
enum Mutation {
    Overwrite(proptest::sample::Index, u8),
    /// A digit for a digit keeps the line's shape and moves an index or a coordinate instead.
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
    for mutation in mutations {
        if bytes.is_empty() {
            break;
        }
        match mutation {
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
            Mutation::Truncate(i) => bytes.truncate(i.index(bytes.len() + 1)),
        }
    }
    bytes
}

/// What the engine relies on: ranges tile the arrays, ids index what exists.
fn assert_consistent(model: &FlatModel) -> Result<(), proptest::test_runner::TestCaseError> {
    use proptest::prop_assert;
    prop_assert!(model.indices.len().is_multiple_of(3));
    prop_assert!(model.material_index.len() * 3 == model.indices.len());
    prop_assert!(model
        .indices
        .iter()
        .all(|&i| (i as usize) < model.vertices.len()));
    prop_assert!(model
        .material_index
        .iter()
        .all(|&m| (m as usize) < model.materials.len()));
    let tiled: usize = model.ranges.iter().map(|range| range.index_count).sum();
    prop_assert!(tiled == model.indices.len());
    for material in &model.materials {
        for slot in [
            material.base_texture,
            material.normal_texture,
            material.emissive_texture,
            material.alpha_texture,
        ] {
            prop_assert!(slot >= -1 && (slot as i64) < model.textures.len() as i64);
        }
    }
    prop_assert!(model.vertices.iter().all(|v| v
        .normal
        .iter()
        .chain(&v.tangent)
        .all(|c| c.is_finite())));
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

    /// The obj_parsing_fuzz_test's successor: a mutated OBJ is an error or consistent arrays, never a panic.
    #[test]
    fn a_mutated_obj_loads_consistently_or_fails_cleanly(
        seed in 0usize..3,
        mutations in proptest::collection::vec(mutation(), 1..8),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let (obj, mtl, mtl_name) = match seed {
            0 => {
                let card = mask_card();
                for name in ["red.bmp", "half_mask.tga"] {
                    std::fs::copy(card.with_file_name(name), dir.path().join(name)).unwrap();
                }
                (std::fs::read(&card).unwrap(), std::fs::read(card.with_file_name("card.mtl")).unwrap(), "card.mtl")
            }
            1 => (seed_cube().as_bytes().to_vec(), b"newmtl a\nmap_Kd nowhere.png\nnewmtl b\nd 0.5\n".to_vec(), "model.mtl"),
            _ => {
                let (obj, mtl) = seed_hostile();
                (obj.into_bytes(), mtl.into_bytes(), "model.mtl")
            }
        };
        let path = dir.path().join("model.obj");
        std::fs::write(&path, mutate(obj, &mutations)).unwrap();
        std::fs::write(dir.path().join(mtl_name), mutate(mtl, &mutations)).unwrap();
        if let Ok(model) = load_obj_flat(&path, &LoadOptions::default()) {
            assert_consistent(&model)?;
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(8))]

    /// A face around the corner cap with jittered, often concave corners: it splits, or is skipped, in bounded time.
    #[test]
    fn a_polygon_near_the_corner_cap_loads_consistently(
        corners in 2040usize..2056,
        jitter in proptest::collection::vec(0.5f32..1.5, 2056),
    ) {
        let mut obj = String::new();
        for (k, scale) in jitter.iter().take(corners).enumerate() {
            let angle = k as f32 * std::f32::consts::TAU / corners as f32;
            obj += &format!("v {} {} 0\n", angle.cos() * scale, angle.sin() * scale);
        }
        obj += "f";
        for k in 1..=corners {
            obj += &format!(" {k}");
        }
        obj += "\nv 0 0 1\nv 1 0 1\nv 0 1 1\nf -3 -2 -1\n";
        let mesh = parse_obj(&obj).expect("the closing triangle always draws");
        proptest::prop_assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
        proptest::prop_assert!(mesh.triangle_count() <= corners - 2 + 1);
    }
}
