//! The traversal kernel on the GPU against the CPU traversal: identical triangles, distances within rounding.

use glam::{Vec2, Vec3, Vec4};

use super::scenes::{self, build, rays};
use crate::context::GpuContext;
use crate::render::tracer::debug::{DebugHit, TraceDebugPass};
use crate::render::tracer::geometry::{unpack_vertex, TRIANGLE_MATERIAL};
use crate::render::tracer::gpu::TraceGpuScene;
use crate::render::tracer::query::{TraceHit, TraceRay, FAR};
use crate::render::tracer::scene::TraceScene;

/// Möller-Trumbore without the bounds: every triangle the ray's line meets, with its barycentrics.
fn line_hits(scene: &TraceScene, ray: &TraceRay) -> Vec<(f32, Vec3)> {
    let mut out = Vec::new();
    for inst in &scene.instances {
        let to_object = inst.world_to_object();
        let (o, d) = (
            to_object.transform_point3(ray.origin),
            to_object.transform_vector3(ray.direction),
        );
        let mesh = &scene.meshes[inst.mesh as usize];
        for tri in &scene.triangles
            [mesh.first_triangle as usize..(mesh.first_triangle + mesh.triangle_count) as usize]
        {
            let p = |k: usize| Vec4::from_array(scene.positions[tri[k] as usize]).truncate();
            let (e1, e2) = (p(1) - p(0), p(2) - p(0));
            let q = d.cross(e2);
            let det = e1.dot(q);
            if det.abs() < 1e-12 {
                continue;
            }
            let s = o - p(0);
            let (u, r) = (s.dot(q) / det, s.cross(e1));
            let v = d.dot(r) / det;
            out.push((e2.dot(r) / det, Vec3::new(1.0 - u - v, u, v)));
        }
    }
    out
}

/// No triangle edge within `slack` of the ray inside its interval, and no second hit within a relative 1e-4 of the first.
fn robust(scene: &TraceScene, ray: &TraceRay, hit: Option<TraceHit>) -> bool {
    let slack = 2e-3;
    let mut accepted = Vec::new();
    for (t, w) in line_hits(scene, ray) {
        if t < ray.tmin - 1e-3 || t > ray.tmax * 1.001 {
            continue;
        }
        if w.min_element() > -slack && w.min_element() < slack {
            return false;
        }
        if w.min_element() >= slack {
            accepted.push(t);
        }
    }
    let Some(hit) = hit else {
        return accepted.is_empty();
    };
    let near = accepted
        .iter()
        .filter(|t| (**t - hit.t).abs() <= 1e-4 * hit.t.abs().max(1.0))
        .count();
    near == 1
        && (hit.t - ray.tmin).abs() > 1e-3
        && (ray.tmax - hit.t).abs() > 1e-3 * ray.tmax.min(1e6)
}

fn upload(gpu: &GpuContext, scene: &TraceScene) -> TraceGpuScene {
    TraceGpuScene::upload(gpu, scene).expect("a test scene fits WebGPU's default limits")
}

/// Rays whose result does not hang on rounding must agree exactly in triangle and closely in distance.
fn assert_gpu_matches(name: &str, gpu: &GpuContext, pass: &TraceDebugPass, scene: &TraceScene) {
    let uploaded = upload(gpu, scene);
    let probe = rays(scene, 2000, 31);
    let gpu_hits = pass.trace_closest(gpu, &uploaded, &probe).expect("traces");
    let gpu_any = pass.trace_any(gpu, &uploaded, &probe).expect("traces");
    let (mut checked, mut hits) = (0, 0);
    for (i, (ray, got)) in probe.iter().zip(&gpu_hits).enumerate() {
        let expected = scene.trace_closest(ray);
        if !robust(scene, ray, expected) {
            continue;
        }
        checked += 1;
        assert_eq!(gpu_any[i], expected.is_some(), "{name}: any-hit of ray {i}");
        let Some(expected) = expected else {
            assert!(!got.is_hit(), "{name}: ray {i} hit {got:?} on the GPU only");
            continue;
        };
        hits += 1;
        assert_eq!(
            (got.instance, got.triangle),
            (expected.instance, expected.triangle),
            "{name}: ray {i} {ray:?}"
        );
        assert!(
            (got.t - expected.t).abs() <= 1e-4 * expected.t.max(1.0),
            "{name}: ray {i} t {} vs {}",
            got.t,
            expected.t
        );
        assert!(
            (got.barycentrics - expected.barycentrics)
                .abs()
                .max_element()
                < 1e-3,
            "{name}: ray {i}"
        );
    }
    eprintln!(
        "GPU-TRACE: {name}: {checked} of {} rays robust, {hits} hits",
        probe.len()
    );
    assert!(
        checked * 10 >= probe.len() * 8,
        "{name}: only {checked} robust rays"
    );
    assert!(hits >= 200, "{name}: only {hits} robust hits");
}

#[test]
fn gpu_closest_and_any_hits_equal_the_cpu_traversal() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let pass = TraceDebugPass::new(&gpu.device);
    for (name, cpu) in [
        ("soup", scenes::soup(3000, 1.5, 1)),
        ("terrain", scenes::terrain(48, 3)),
        ("instanced", scenes::instanced(4)),
        ("cube_on_plane", scenes::asset("cube_on_plane.gltf")),
    ] {
        assert_gpu_matches(name, &gpu, &pass, &build(cpu));
    }
}

fn straight_down(x: f32, tmax: f32) -> TraceRay {
    TraceRay {
        origin: Vec3::new(x, 0.3, 5.0),
        tmin: 0.0,
        direction: Vec3::NEG_Z,
        tmax,
    }
}

#[test]
fn gpu_alpha_test_cuts_out_the_transparent_half_in_both_queries() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = build(scenes::masked());
    let pass = TraceDebugPass::new(&gpu.device);
    let uploaded = upload(&gpu, &scene);
    let probe = [
        straight_down(0.5, FAR),
        straight_down(-0.5, FAR),
        straight_down(0.5, 3.5),
        straight_down(-0.5, 6.0),
    ];
    let hits = pass.trace_closest(&gpu, &uploaded, &probe).expect("traces");
    assert!(
        (hits[0].t - 4.0).abs() < 1e-4,
        "the solid half of the front card: {:?}",
        hits[0]
    );
    let red = crate::render::texture::srgb_to_linear(200);
    assert!(
        hits[0].base_alpha > 0.99 && (hits[0].base_red - red).abs() < 0.01,
        "sRGB decoded: {:?}",
        hits[0]
    );
    assert!(
        (hits[1].t - 6.95).abs() < 1e-3,
        "through both cut-outs to the wall: {:?}",
        hits[1]
    );
    assert!(
        !hits[2].is_hit() && !hits[3].is_hit(),
        "nothing before the cards, nothing solid before the wall"
    );
    let any = pass.trace_any(&gpu, &uploaded, &probe).expect("traces");
    assert_eq!(any, [true, true, false, false]);
}

/// The attributes `trace_surface` decodes equal the CPU's unpacking of the same vertices.
fn expected_surface(scene: &TraceScene, ray: &TraceRay, hit: &DebugHit) -> (Vec3, Vec2, u32, bool) {
    let tri = scene.triangles[hit.triangle as usize];
    let inst = &scene.instances[hit.instance as usize];
    let w = [
        1.0 - hit.barycentrics.x - hit.barycentrics.y,
        hit.barycentrics.x,
        hit.barycentrics.y,
    ];
    let corners = [0, 1, 2].map(|k| unpack_vertex(&scene.vertices[tri[k] as usize]));
    let to_world_normal = inst.world_to_object().transpose();
    let n = corners
        .iter()
        .zip(w)
        .map(|(c, w)| c.normal * w)
        .sum::<Vec3>();
    let uv = corners.iter().zip(w).map(|(c, w)| c.uv0 * w).sum::<Vec2>();
    let p = |k: usize| Vec4::from_array(scene.positions[tri[k] as usize]).truncate();
    let geometric = to_world_normal.transform_vector3((p(1) - p(0)).cross(p(2) - p(0)));
    let front = geometric.dot(ray.direction) < 0.0;
    (
        to_world_normal.transform_vector3(n).normalize(),
        uv,
        tri[3] & TRIANGLE_MATERIAL,
        front,
    )
}

#[test]
fn gpu_surfaces_decode_the_packed_attributes() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = build(scenes::instanced(4));
    let pass = TraceDebugPass::new(&gpu.device);
    let probe = rays(&scene, 600, 41);
    let hits = pass
        .trace_closest(&gpu, &upload(&gpu, &scene), &probe)
        .expect("traces");
    let mut seen = 0;
    for (ray, hit) in probe.iter().zip(&hits).filter(|(_, h)| h.is_hit()) {
        let (normal, uv, material, front) = expected_surface(&scene, ray, hit);
        assert!(
            (hit.normal - normal).length() < 2e-3,
            "{} vs {normal}",
            hit.normal
        );
        assert!(
            (hit.uv0 - uv).abs().max_element() < 1e-4,
            "{} vs {uv}",
            hit.uv0
        );
        assert_eq!((hit.material, hit.front_face), (material, front));
        assert!((hit.vertex_alpha - 1.0).abs() < 1e-4);
        let mirrored = scene.instances[hit.instance as usize].flags & 1 != 0;
        assert_eq!(hit.handedness, if mirrored { -1.0 } else { 1.0 });
        seen += 1;
    }
    assert!(seen > 200, "{seen} hits");
}
