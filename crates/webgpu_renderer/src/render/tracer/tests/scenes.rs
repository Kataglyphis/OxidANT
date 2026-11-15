//! Generated scenes and ray sets the CPU and GPU tests share.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::Rng;
use crate::render::tracer::build::{TraceBuildOptions, TraceSceneBuilder};
use crate::render::tracer::query::{TraceRay, FAR};
use crate::render::tracer::scene::TraceScene;
use crate::scene::{
    AlphaMode, CpuMaterial, CpuPrimitive, CpuSampler, CpuScene, CpuTexture, CpuTextureRef, Vertex,
};

pub(crate) fn vertex(position: Vec3, uv: [f32; 2]) -> Vertex {
    Vertex {
        position: position.to_array(),
        normal: [0.0, 0.0, 1.0],
        uv,
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0, 1.0, 1.0, 1.0],
        uv1: uv,
    }
}

pub(crate) fn primitive(vertices: Vec<Vertex>, indices: Vec<u32>, transform: Mat4) -> CpuPrimitive {
    CpuPrimitive {
        vertices,
        indices,
        transform,
        node_index: None,
        skin_index: None,
        material: CpuMaterial::default(),
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// `count` random triangles of up to `size` across inside a box of side 20.
pub(crate) fn soup(count: usize, size: f32, seed: u64) -> CpuScene {
    let mut rng = Rng::new(seed);
    let mut vertices = Vec::with_capacity(count * 3);
    for _ in 0..count {
        let centre = rng.signed3() * 10.0;
        for _ in 0..3 {
            vertices.push(vertex(
                centre + rng.signed3() * size,
                [rng.next_f32(), rng.next_f32()],
            ));
        }
    }
    let indices = (0..vertices.len() as u32).collect();
    CpuScene {
        primitives: vec![primitive(vertices, indices, Mat4::IDENTITY)],
        ..CpuScene::default()
    }
}

/// A `side` x `side` height field: shared edges everywhere, the case a tie rule must settle.
pub(crate) fn terrain(side: u32, seed: u64) -> CpuScene {
    let mut rng = Rng::new(seed);
    let mut vertices = Vec::new();
    for y in 0..=side {
        for x in 0..=side {
            let p = Vec3::new(x as f32, rng.next_f32() * 2.0, y as f32)
                - Vec3::new(side as f32 * 0.5, 0.0, side as f32 * 0.5);
            vertices.push(vertex(p, [x as f32 / side as f32, y as f32 / side as f32]));
        }
    }
    let mut indices = Vec::new();
    let row = side + 1;
    for y in 0..side {
        for x in 0..side {
            let i = y * row + x;
            indices.extend([i, i + row, i + 1, i + 1, i + row, i + row + 1]);
        }
    }
    CpuScene {
        primitives: vec![primitive(vertices, indices, Mat4::IDENTITY)],
        ..CpuScene::default()
    }
}

/// A unit cube's twelve triangles, centred on the origin.
pub(crate) fn cube_primitive(transform: Mat4) -> CpuPrimitive {
    let corners: Vec<Vertex> = (0..8)
        .map(|i| {
            let p = Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32) - 0.5;
            vertex(p, [p.x + 0.5, p.y + 0.5])
        })
        .collect();
    let faces: [[u32; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let indices = faces
        .iter()
        .flat_map(|f| [f[0], f[1], f[2], f[0], f[2], f[3]])
        .collect();
    primitive(corners, indices, transform)
}

/// Thirty copies of one cube under random rotations, scales (one mirrored) and offsets, over a merged ground soup.
pub(crate) fn instanced(seed: u64) -> CpuScene {
    let mut rng = Rng::new(seed);
    let mut scene = soup(200, 2.0, seed + 1);
    for i in 0..30 {
        let axis = rng.signed3().normalize_or(Vec3::Y);
        let rotation = Quat::from_axis_angle(axis, rng.next_f32() * std::f32::consts::TAU);
        let mut scale =
            Vec3::splat(0.5 + rng.next_f32() * 2.0) * Vec3::new(1.0, 0.5 + rng.next_f32(), 1.0);
        if i == 7 {
            scale.x = -scale.x;
        }
        let transform = Mat4::from_scale_rotation_translation(scale, rotation, rng.signed3() * 9.0);
        scene.primitives.push(cube_primitive(transform));
    }
    scene
}

/// A 64-texel texture whose left half is transparent and right half opaque.
pub(crate) fn half_transparent() -> Arc<CpuTexture> {
    let mut rgba8 = Vec::with_capacity(64 * 64 * 4);
    for _y in 0..64 {
        for x in 0..64 {
            rgba8.extend([200, 100, 50, if x < 32 { 0 } else { 255 }]);
        }
    }
    Arc::new(CpuTexture {
        width: 64,
        height: 64,
        rgba8,
        compressed: None,
    })
}

/// A MASK card in the plane z = `z` spanning [-1, 1]^2: its left half is cut out, its right half solid.
pub(crate) fn masked_card(z: f32, cutoff: f32) -> CpuPrimitive {
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| vertex(Vec3::new(x, y, z), [(x + 1.0) * 0.5, (1.0 - y) * 0.5]));
    let mut card = primitive(corners.to_vec(), vec![0, 1, 2, 0, 2, 3], Mat4::IDENTITY);
    card.material = CpuMaterial {
        alpha_mode: AlphaMode::Mask(cutoff),
        double_sided: true,
        base_color_texture: Some(CpuTextureRef {
            texture: half_transparent(),
            sampler: CpuSampler::default(),
            srgb: true,
        }),
        ..CpuMaterial::default()
    };
    card
}

/// Two masked cards in front of an opaque wall at z = -2.
pub(crate) fn masked() -> CpuScene {
    let mut wall = cube_primitive(Mat4::from_scale_rotation_translation(
        Vec3::new(4.0, 4.0, 0.1),
        Quat::IDENTITY,
        Vec3::new(0.0, 0.0, -2.0),
    ));
    wall.material.base_color = [0.2, 0.9, 0.2, 1.0];
    CpuScene {
        primitives: vec![masked_card(1.0, 0.5), masked_card(0.0, 0.5), wall],
        ..CpuScene::default()
    }
}

pub(crate) fn asset(name: &str) -> CpuScene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name);
    crate::load_gltf(&path).unwrap_or_else(|e| panic!("{name}: {e:#}"))
}

pub(crate) fn build(scene: CpuScene) -> TraceScene {
    TraceSceneBuilder::new(Arc::new(scene), Vec::new(), TraceBuildOptions::default())
        .run()
        .expect("an uncancelled build finishes")
}

/// Rays from around the scene: half at random, half aimed at random triangles' centres.
pub(crate) fn rays(scene: &TraceScene, count: usize, seed: u64) -> Vec<TraceRay> {
    let mut rng = Rng::new(seed);
    let bounds = scene.nodes[scene.tlas_root() as usize].bounds();
    let (centre, half) = (
        Vec3::from(bounds.min + bounds.max) * 0.5,
        Vec3::from(bounds.max - bounds.min) * 0.6 + 0.1,
    );
    (0..count)
        .map(|i| {
            let origin = centre + rng.signed3() * half;
            let direction = if i % 2 == 0 {
                rng.signed3().normalize_or(Vec3::X)
            } else {
                let pick = |rng: &mut Rng, len: usize| (rng.next_f32() * len as f32) as usize % len;
                let instance = &scene.instances[pick(&mut rng, scene.instances.len())];
                let mesh = &scene.meshes[instance.mesh as usize];
                let tri = scene.triangles
                    [mesh.first_triangle as usize + pick(&mut rng, mesh.triangle_count as usize)];
                let p = |v: u32| {
                    instance
                        .object_to_world()
                        .transform_point3(Vec3::from_slice(&scene.positions[v as usize][..3]))
                };
                ((p(tri[0]) + p(tri[1]) + p(tri[2])) / 3.0 - origin).normalize_or(Vec3::Y)
            };
            TraceRay {
                origin,
                tmin: if i % 3 == 0 { 0.0 } else { 1e-3 },
                direction,
                tmax: if i % 5 == 0 { half.length() } else { FAR },
            }
        })
        .collect()
}
