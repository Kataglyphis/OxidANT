//! `load_flat` from a file, as the C++ engine loads through the bridge: `cube.glb` and a gated 294,912-triangle grid.
use std::hint::black_box;
use std::path::Path;

use criterion::{criterion_group, criterion_main, Criterion};
use kataglyphis_assets::flat::load_flat;
use kataglyphis_assets::LoadOptions;

/// Quads per side of the generated grid, two triangles each.
const GRID: u32 = 384;

/// Positions, normals, uvs and u32 indices of a `side` x `side` quad grid on the unit square in the xz plane.
fn grid_buffer(side: u32) -> (Vec<u8>, [usize; 4]) {
    let n = side + 1;
    let corners = || (0..n).flat_map(move |z| (0..n).map(move |x| (x, z)));
    let unit = |i: u32| i as f32 / side as f32;
    let floats =
        |values: Vec<f32>| -> Vec<u8> { values.iter().flat_map(|f| f.to_le_bytes()).collect() };
    let positions = floats(
        corners()
            .flat_map(|(x, z)| [unit(x), 0.0, unit(z)])
            .collect(),
    );
    let normals = floats(corners().flat_map(|_| [0.0, 1.0, 0.0]).collect());
    let uvs = floats(corners().flat_map(|(x, z)| [unit(x), unit(z)]).collect());
    let indices: Vec<u8> = (0..side)
        .flat_map(|z| (0..side).map(move |x| z * n + x))
        .flat_map(|i| [i, i + n, i + 1, i + 1, i + n, i + n + 1])
        .flat_map(u32::to_le_bytes)
        .collect();
    let lengths = [positions.len(), normals.len(), uvs.len(), indices.len()];
    ([positions, normals, uvs, indices].concat(), lengths)
}

/// The grid as a GLB: one node, one primitive, its four accessors in one binary chunk.
fn grid_glb(side: u32) -> Vec<u8> {
    let (bin, [positions, normals, uvs, indices]) = grid_buffer(side);
    let vertices = (side + 1) * (side + 1);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3}}]}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":{vertices},"type":"VEC3","min":[0,0,0],"max":[1,0,1]}},
          {{"bufferView":1,"componentType":5126,"count":{vertices},"type":"VEC3"}},
          {{"bufferView":2,"componentType":5126,"count":{vertices},"type":"VEC2"}},
          {{"bufferView":3,"componentType":5125,"count":{},"type":"SCALAR"}}],
        "bufferViews":[{{"buffer":0,"byteLength":{positions}}},{{"buffer":0,"byteOffset":{positions},"byteLength":{normals}}},
          {{"buffer":0,"byteOffset":{},"byteLength":{uvs}}},{{"buffer":0,"byteOffset":{},"byteLength":{indices}}}],
        "buffers":[{{"byteLength":{}}}]}}"#,
        indices / 4,
        positions + normals,
        positions + normals + uvs,
        bin.len()
    );
    let mut json = json.into_bytes();
    json.resize(json.len().next_multiple_of(4), b' ');
    let chunk = |kind: &[u8; 4], data: &[u8]| {
        [&(data.len() as u32).to_le_bytes()[..], &kind[..], data].concat()
    };
    let body = [chunk(b"JSON", &json), chunk(b"BIN\0", &bin)].concat();
    let header = [
        &b"glTF"[..],
        &2u32.to_le_bytes(),
        &(12 + body.len() as u32).to_le_bytes(),
    ]
    .concat();
    [header, body].concat()
}

fn load(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_flat");
    let cube = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.glb");
    group.bench_function("cube_glb", |b| {
        b.iter(|| load_flat(black_box(&cube), &LoadOptions::default()).unwrap())
    });

    let dir = tempfile::tempdir().unwrap();
    let grid = dir.path().join("grid.glb");
    std::fs::write(&grid, grid_glb(GRID)).unwrap();
    // A load that failed or lost triangles would time the wrong thing.
    let model = load_flat(&grid, &LoadOptions::default()).unwrap();
    assert_eq!(model.indices.len(), (GRID * GRID * 6) as usize);
    group.sample_size(10);
    group.bench_function("grid_295k_triangles", |b| {
        b.iter(|| load_flat(black_box(&grid), &LoadOptions::default()).unwrap())
    });
    group.finish();
}

criterion_group!(benches, load);
criterion_main!(benches);
