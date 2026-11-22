//! The renderer's CPU work the bridge is to share with the C++ engine: QEM, the tile-light grid, the cascade fit and
//! the tracer's BVH build, each on a fixed generated input.
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
// glam's `directx` projection: NDC Z in [0, 1], as the renderer's camera builds it.
use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3, Vec3A};
use kataglyphis_webgpu_renderer::render::cascades::{fit_cascades, CascadeLens};
use kataglyphis_webgpu_renderer::render::lights::{pack_punctual_lights, MAX_PUNCTUAL_LIGHTS};
use kataglyphis_webgpu_renderer::render::shadow_settings::ShadowSettings;
use kataglyphis_webgpu_renderer::render::tile_grid::{build_tile_light_grid, TileLightGridScratch};
use kataglyphis_webgpu_renderer::render::tracer::bvh::{self, Aabb, BvhParams};
use kataglyphis_webgpu_renderer::scene::{CpuLight, CpuLightKind, CpuMaterial, Vertex};
use kataglyphis_webgpu_renderer::{simplify_primitive_qem, CpuPrimitive};

/// Quads per side of the QEM height field, two triangles each: 32,768 triangles.
const QEM_SIDE: u32 = 128;
/// Quads per side of the BVH's terrain: 262,088 triangle boxes.
const BVH_SIDE: u32 = 362;

/// A rolling height field on the unit square, so every collapse costs QEM a real error.
fn height(x: f32, z: f32) -> f32 {
    0.05 * (x * 12.0).sin() * (z * 9.0).cos()
}

fn height_field(side: u32) -> CpuPrimitive {
    let n = side + 1;
    let vertices = (0..n)
        .flat_map(|z| (0..n).map(move |x| (x, z)))
        .map(|(x, z)| {
            let (u, v) = (x as f32 / side as f32, z as f32 / side as f32);
            Vertex {
                position: [u, height(u, v), v],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0.0; 4],
                weights: [0.0; 4],
                color: [1.0; 4],
                uv1: [u, v],
            }
        })
        .collect();
    let indices = (0..side)
        .flat_map(|z| (0..side).map(move |x| z * n + x))
        .flat_map(|i| [i, i + n, i + 1, i + 1, i + n, i + n + 1])
        .collect();
    CpuPrimitive {
        vertices,
        indices,
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial::default(),
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

fn qem(c: &mut Criterion) {
    let field = height_field(QEM_SIDE);
    let quarter = simplify_primitive_qem(&field, 0.25);
    // A simplifier that gave up would time nothing.
    assert!(quarter.indices.len() < field.indices.len() / 2);
    let mut group = c.benchmark_group("qem");
    group.sample_size(10);
    group.bench_function("height_field_32k_triangles_to_a_quarter", |b| {
        b.iter(|| simplify_primitive_qem(black_box(&field), 0.25))
    });
    group.finish();
}

/// As many point lights as the renderer packs, of range 4, on a 16 x 16 lattice 20 units before the camera.
fn lattice_lights() -> Vec<CpuLight> {
    (0..MAX_PUNCTUAL_LIGHTS)
        .map(|i| CpuLight {
            kind: CpuLightKind::Point,
            color: [1.0; 3],
            intensity: 1.0,
            range: 4.0,
            position: [
                (i % 16) as f32 * 2.0 - 15.0,
                (i / 16) as f32 * 1.2 - 9.0,
                -20.0,
            ],
            direction: [0.0, 0.0, -1.0],
            node: None,
        })
        .collect()
}

fn tile_grid(c: &mut Criterion) {
    let (packed, count) = pack_punctual_lights(&lattice_lights());
    let view = look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
    let proj = clip::perspective(60f32.to_radians(), 16.0 / 9.0, 0.1, 1000.0);
    let mut scratch = TileLightGridScratch::default();
    let mut group = c.benchmark_group("tile_grid");
    group.bench_function("256_point_lights_1080p", |b| {
        b.iter(|| {
            build_tile_light_grid(
                &mut scratch,
                black_box(&packed),
                count,
                (1920, 1080),
                &view,
                &proj,
            )
        })
    });
    group.finish();
}

fn cascades(c: &mut Criterion) {
    let lens = CascadeLens {
        view: look_at_mat4(Vec3::new(8.0, 4.0, 8.0), Vec3::ZERO, Vec3::Y),
        fov_y_deg: 60.0,
        aspect: 16.0 / 9.0,
        near: 0.1,
        far: 1000.0,
    };
    let settings = ShadowSettings::default();
    let bounds = Some((Vec3::splat(-50.0), Vec3::splat(50.0)));
    let light = Vec3::new(0.3, 1.0, 0.2);
    let mut group = c.benchmark_group("cascades");
    group.bench_function("fit_every_cascade", |b| {
        b.iter(|| fit_cascades(black_box(&lens), light, &settings, bounds))
    });
    group.finish();
}

/// The triangles of a `side` x `side` rolling terrain, as the tracer boxes them.
fn terrain_boxes(side: u32) -> Vec<Aabb> {
    let corner = |x: u32, z: u32| {
        let (u, v) = (x as f32 / side as f32, z as f32 / side as f32);
        Vec3A::new(u * 100.0, height(u, v) * 100.0, v * 100.0)
    };
    (0..side)
        .flat_map(|z| (0..side).map(move |x| (x, z)))
        .flat_map(|(x, z)| {
            let (a, b, c, d) = (
                corner(x, z),
                corner(x + 1, z),
                corner(x, z + 1),
                corner(x + 1, z + 1),
            );
            [Aabb::from_points(&[a, c, b]), Aabb::from_points(&[b, c, d])]
        })
        .collect()
}

fn bvh_build(c: &mut Criterion) {
    let boxes = terrain_boxes(BVH_SIDE);
    let built = bvh::build(boxes.clone(), BvhParams::BLAS);
    assert_eq!(built.order.len(), boxes.len());
    let mut group = c.benchmark_group("bvh");
    group.sample_size(10);
    group.bench_function("blas_262k_triangles", |b| {
        b.iter_batched(
            || boxes.clone(),
            |boxes| bvh::build(boxes, BvhParams::BLAS),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

criterion_group!(benches, qem, tile_grid, cascades, bvh_build);
criterion_main!(benches);
