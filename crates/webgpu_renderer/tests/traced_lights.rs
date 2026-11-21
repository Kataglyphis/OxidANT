//! The traced modes' punctual lights and the path tracer's environment on a real adapter: lit where raster lights, the analytic radiance, traced shadows.

use std::f64::consts::PI;
use std::sync::Arc;
use std::time::Duration;

use glam::{DVec3, Mat4, Vec2, Vec3, Vec4};
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::render::tracer::{RenderMode, TraceSettings};
use kataglyphis_webgpu_renderer::scene::{
    CpuLight, CpuLightKind, CpuMaterial, CpuPrimitive, CpuScene, Vertex,
};
use kataglyphis_webgpu_renderer::{EquirectImage, ForwardRenderer, GpuContext, OrbitCamera};

mod common;

const SIZE: u32 = 64;

/// A renderer with its traced scene built, lit by the scene's punctual lights alone: no sun, no sky light, no bloom or SSAO.
struct Rig {
    gpu: GpuContext,
    renderer: ForwardRenderer,
    camera: OrbitCamera,
}

impl Rig {
    fn new(scene: &CpuScene, camera: OrbitCamera) -> Option<Self> {
        let gpu = GpuContext::headless_or_skip()?;
        let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
        renderer.upload_scene(&gpu, scene);
        renderer.set_trace_source(Arc::new(scene.clone()));
        renderer.bloom_strength = 0.0;
        renderer.ssao_strength = 0.0;
        renderer.light_color_intensity.w = 0.0;
        renderer.light_dir_ambient.w = 0.0;
        renderer.trace_settings = TraceSettings {
            samples_per_pixel: 16,
            max_bounces: 2,
            furnace: None,
        };
        let built = renderer
            .tracer_mut()
            .finish_build(&gpu, Duration::from_secs(120));
        assert!(built, "{:?}", renderer.trace_status());
        Some(Self {
            gpu,
            renderer,
            camera,
        })
    }

    /// The HDR radiance after `frames` frames in `mode`.
    fn radiance(&mut self, mode: RenderMode, frames: u32) -> Vec<Vec3> {
        self.renderer.render_mode = mode;
        for _ in 0..frames {
            self.renderer
                .render_to_pixels(&self.gpu, SIZE, SIZE, &self.camera)
                .expect("renders");
        }
        let bits = self
            .renderer
            .read_hdr_target(&self.gpu)
            .expect("reads back");
        bits.as_chunks::<4>()
            .0
            .iter()
            .map(|p| Vec3::new(half_to_f32(p[0]), half_to_f32(p[1]), half_to_f32(p[2])))
            .collect()
    }

    /// Where each pixel centre's camera ray meets the ground (x, z), as `trace_shading.slang`'s `camera_ray` casts it.
    fn ground(&self) -> Vec<Vec2> {
        let inverse = self.camera.view_projection(1.0).inverse();
        (0..SIZE * SIZE)
            .map(|i| {
                let (px, py) = ((i % SIZE) as f32 + 0.5, (i / SIZE) as f32 + 0.5);
                let ndc = Vec2::new(px / SIZE as f32 * 2.0 - 1.0, 1.0 - py / SIZE as f32 * 2.0);
                let near = inverse.project_point3(ndc.extend(0.0));
                let far = inverse.project_point3(ndc.extend(1.0));
                let hit = near + (far - near) * (near.y / (near.y - far.y));
                Vec2::new(hit.x, hit.z)
            })
            .collect()
    }
}

fn vertex(position: Vec3, normal: Vec3) -> Vertex {
    Vertex {
        position: position.to_array(),
        normal: normal.to_array(),
        uv: [0.0, 0.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0; 4],
        uv1: [0.0, 0.0],
    }
}

/// A white dielectric of `roughness`.
fn white(roughness: f32) -> CpuMaterial {
    CpuMaterial {
        metallic_factor: 0.0,
        roughness_factor: roughness,
        ..CpuMaterial::default()
    }
}

/// A quad from four counter-clockwise corners, its normal theirs.
fn quad(corners: [Vec3; 4], material: CpuMaterial) -> CpuPrimitive {
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize();
    CpuPrimitive {
        vertices: corners.iter().map(|&c| vertex(c, normal)).collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material,
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// A 20 x 20 rough white ground at y = 0, facing up.
fn ground_plane() -> CpuPrimitive {
    quad(
        [
            Vec3::new(-10.0, 0.0, -10.0),
            Vec3::new(-10.0, 0.0, 10.0),
            Vec3::new(10.0, 0.0, 10.0),
            Vec3::new(10.0, 0.0, -10.0),
        ],
        white(1.0),
    )
}

fn point_light(position: Vec3, intensity: f32, range: f32) -> CpuLight {
    CpuLight {
        kind: CpuLightKind::Point,
        color: [1.0; 3],
        intensity,
        range,
        position: position.to_array(),
        direction: [0.0, -1.0, 0.0],
        node: None,
    }
}

fn scene(primitives: Vec<CpuPrimitive>, lights: Vec<CpuLight>) -> CpuScene {
    CpuScene {
        primitives,
        lights,
        ..CpuScene::default()
    }
}

/// Straight down on the ground, 10 units up: the frame shows (x, z) within about 4 of the origin.
fn from_above() -> OrbitCamera {
    OrbitCamera {
        radius: 10.0,
        pitch_deg: 89.0,
        yaw_deg: 90.0,
        ..OrbitCamera::default()
    }
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, count) = values.fold((0.0, 0u32), |(s, c), v| (s + v, c + 1));
    assert!(count > 0, "an empty patch");
    sum / count as f32
}

/// Pixels brighter than a tenth of the frame's brightest, each frame's own.
fn lit(frame: &[Vec3]) -> Vec<bool> {
    let peak = frame.iter().map(|p| p.x).fold(0.0, f32::max);
    frame.iter().map(|p| p.x > peak * 0.1).collect()
}

fn iou(a: &[bool], b: &[bool]) -> f32 {
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    both as f32 / either.max(1) as f32
}

/// `brdf.slang`'s `brdf_direct` for a white dielectric, times the radiance and n.l.
fn brdf_direct(n: DVec3, v: DVec3, l: DVec3, roughness: f64, radiance: f64) -> f64 {
    let h = (l + v).normalize();
    let (n_dot_l, n_dot_v) = (n.dot(l).max(0.0), n.dot(v).max(1e-4));
    let (n_dot_h, h_dot_v) = (n.dot(h).max(0.0), h.dot(v).max(0.0));
    let r = roughness.max(0.045);
    let a2 = r.powi(4);
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let d = a2 / (PI * (denom * denom).max(1e-20));
    let k = (r + 1.0).powi(2) / 8.0;
    let smith = |c: f64| c / (c * (1.0 - k) + k).max(1e-6);
    let f = 0.04 + 0.96 * (1.0 - h_dot_v).clamp(0.0, 1.0).powi(5);
    let energy = common::ggx_energy(n_dot_v, roughness);
    let specular =
        common::multiscatter_gain(energy, 0.04) * d * smith(n_dot_v) * smith(n_dot_l) * f
            / (4.0 * n_dot_v * n_dot_l).max(1e-6);
    let diffuse = (1.0 - common::specular_albedo(energy, 0.04)).clamp(0.0, 1.0) / PI;
    (diffuse + specular) * radiance * n_dot_l
}

/// `punctual_light.slang`'s inverse square and range window for a point light, at a ground point, through the BRDF.
fn point_radiance(light: &CpuLight, ground: Vec2, eye: Vec3) -> f64 {
    let p = DVec3::new(f64::from(ground.x), 0.0, f64::from(ground.y));
    let to_light = Vec3::from(light.position).as_dvec3() - p;
    let distance = to_light.length();
    let mut attenuation = 1.0 / (distance * distance);
    if light.range > 0.0 {
        let k = (1.0 - (distance / f64::from(light.range)).powi(4)).clamp(0.0, 1.0);
        attenuation *= k * k;
    }
    let v = (eye.as_dvec3() - p).normalize();
    brdf_direct(
        DVec3::Y,
        v,
        to_light / distance,
        1.0,
        f64::from(light.intensity) * attenuation,
    )
}

/// Two point lights 1 above the ground, whose range windows end 2.29 and 1.73 from their feet.
fn ranged_point_lights() -> Vec<CpuLight> {
    vec![
        point_light(Vec3::new(0.0, 1.0, 0.0), 2.0, 2.5),
        point_light(Vec3::new(1.0, 1.0, 0.0), 4.0, 2.0),
    ]
}

/// The pixels whose ground point lies within 0.35 of the origin, and the mean analytic radiance `lights` give them.
fn centre_and_expected(rig: &Rig, lights: &[CpuLight]) -> (Vec<usize>, f32) {
    let ground = rig.ground();
    let eye = rig.camera.eye();
    let centre: Vec<usize> = (0..ground.len())
        .filter(|&i| ground[i].length() < 0.35)
        .collect();
    let expected = mean(centre.iter().map(|&i| {
        lights
            .iter()
            .map(|l| point_radiance(l, ground[i], eye))
            .sum::<f64>() as f32
    }));
    (centre, expected)
}

/// Pixels `frame` lights past both of `ranged_point_lights`' range windows.
fn lit_past_range(rig: &Rig, frame: &[Vec3]) -> usize {
    let ground = rig.ground();
    // A pixel is wider than 0.13, and the path tracer jitters inside it.
    (0..ground.len())
        .filter(|&i| ground[i].length() > 2.29 + 0.2 && ground[i].distance(Vec2::X) > 1.73 + 0.2)
        .filter(|&i| frame[i].x > 0.0)
        .count()
}

/// Two ranged point lights over the ground: the path tracer picks one a sample, so its 1/pdf decides the level.
#[test]
fn point_lights_light_the_discs_raster_lights_at_their_analytic_radiance() {
    let lights = ranged_point_lights();
    let Some(mut rig) = Rig::new(&scene(vec![ground_plane()], lights.clone()), from_above()) else {
        return;
    };
    let raster = rig.radiance(RenderMode::Raster, 1);
    let traced = rig.radiance(RenderMode::PathTraced, 12);
    let overlap = iou(&lit(&raster), &lit(&traced));
    let (centre, expected) = centre_and_expected(&rig, &lights);
    let (raster_centre, traced_centre) = (
        mean(centre.iter().map(|&i| raster[i].x)),
        mean(centre.iter().map(|&i| traced[i].x)),
    );
    eprintln!(
        "lit IoU {overlap:.3}; centre over {} pixels: analytic {expected:.4}, raster {raster_centre:.4}, traced {traced_centre:.4}",
        centre.len()
    );
    assert!(
        overlap > 0.9,
        "the traced discs are not raster's: IoU {overlap}"
    );
    assert!(
        (raster_centre / expected - 1.0).abs() < 0.01,
        "raster {raster_centre} against {expected}"
    );
    assert!(
        (traced_centre / expected - 1.0).abs() < 0.03,
        "traced {traced_centre} against {expected}"
    );
    assert_eq!(
        lit_past_range(&rig, &traced),
        0,
        "light past the range windows"
    );
}

/// The ray tracer adds every punctual light at the pixel centre, as the C++ closest-hit shader does: no pick, no noise.
#[test]
fn the_ray_traced_mode_lights_the_discs_raster_lights_at_their_analytic_radiance() {
    let lights = ranged_point_lights();
    let Some(mut rig) = Rig::new(&scene(vec![ground_plane()], lights.clone()), from_above()) else {
        return;
    };
    let raster = rig.radiance(RenderMode::Raster, 1);
    let traced = rig.radiance(RenderMode::RayTraced, 1);
    let overlap = iou(&lit(&raster), &lit(&traced));
    let (centre, expected) = centre_and_expected(&rig, &lights);
    let traced_centre = mean(centre.iter().map(|&i| traced[i].x));
    eprintln!(
        "ray traced: lit IoU {overlap:.3}; centre over {} pixels: analytic {expected:.4}, traced {traced_centre:.4}",
        centre.len()
    );
    assert!(
        overlap > 0.95,
        "the ray-traced discs are not raster's: IoU {overlap}"
    );
    assert!(
        (traced_centre / expected - 1.0).abs() < 0.01,
        "ray traced {traced_centre} against {expected}"
    );
    assert_eq!(
        lit_past_range(&rig, &traced),
        0,
        "light past the range windows"
    );
}

/// A spot cone's cut: nothing past the outer cone, everything inside the inner one, where raster lights it.
#[test]
fn a_spot_light_cuts_its_cone_where_raster_does() {
    let (inner, outer) = (20f32.to_radians(), 30f32.to_radians());
    let spot = CpuLight {
        kind: CpuLightKind::Spot {
            cos_inner: inner.cos(),
            cos_outer: outer.cos(),
        },
        ..point_light(Vec3::new(0.0, 2.0, 0.0), 8.0, 0.0)
    };
    let Some(mut rig) = Rig::new(&scene(vec![ground_plane()], vec![spot]), from_above()) else {
        return;
    };
    let raster = rig.radiance(RenderMode::Raster, 1);
    let traced = rig.radiance(RenderMode::PathTraced, 8);
    let overlap = iou(&lit(&raster), &lit(&traced));
    let ground = rig.ground();
    // A pixel is wider than 0.13, and the path tracer jitters inside it.
    let outside = (0..ground.len()).filter(|&i| ground[i].length() > 2.0 * outer.tan() + 0.2);
    let inside: Vec<usize> = (0..ground.len())
        .filter(|&i| ground[i].length() < 2.0 * inner.tan() - 0.2)
        .collect();
    let leaked = outside.filter(|&i| traced[i].x > 0.0).count();
    let dark = inside.iter().filter(|&&i| traced[i].x <= 0.0).count();
    eprintln!(
        "spot: lit IoU {overlap:.3}, {leaked} pixels lit past the cone, {dark} of {} dark inside",
        inside.len()
    );
    assert!(
        overlap > 0.9,
        "the traced cone is not raster's: IoU {overlap}"
    );
    assert_eq!(leaked, 0, "light past the outer cone");
    assert_eq!(dark, 0, "dark inside the inner cone");
}

/// The ground and a point light at (2.5, 1, 0) behind a wall at x = 1.5.
fn walled_point_light() -> CpuScene {
    let wall = quad(
        [
            Vec3::new(1.5, 0.0, 1.0),
            Vec3::new(1.5, 0.0, -1.0),
            Vec3::new(1.5, 1.5, -1.0),
            Vec3::new(1.5, 1.5, 1.0),
        ],
        white(1.0),
    );
    let light = point_light(Vec3::new(2.5, 1.0, 0.0), 6.0, 0.0);
    scene(vec![ground_plane(), wall], vec![light])
}

/// The wall's shadow covers the origin; the rays to (0, 3.2) pass beside the wall.
const BEHIND_THE_WALL: Vec2 = Vec2::ZERO;
const BESIDE_THE_WALL: Vec2 = Vec2::new(0.0, 3.2);

/// The pixels whose ground point lies within 0.25 of `centre`.
fn patch_pixels(ground: &[Vec2], centre: Vec2) -> Vec<usize> {
    (0..ground.len())
        .filter(|&i| ground[i].distance(centre) < 0.25)
        .collect()
}

/// The mean of `frame` over `patch_pixels`.
fn patch(ground: &[Vec2], frame: &[Vec3], centre: Vec2) -> f32 {
    mean(patch_pixels(ground, centre).iter().map(|&i| frame[i].x))
}

/// A wall between a point light and the ground: the traced shadow ray and raster's cube both darken the ground behind it.
#[test]
fn a_point_light_casts_a_traced_shadow_behind_an_occluder() {
    let Some(mut rig) = Rig::new(&walled_point_light(), from_above()) else {
        return;
    };
    // The unshadowed raster frame is the reference both shadows are measured against.
    rig.renderer.shadows.punctual = false;
    let raster = rig.radiance(RenderMode::Raster, 1);
    rig.renderer.shadows.punctual = true;
    let cube = rig.radiance(RenderMode::Raster, 1);
    let traced = rig.radiance(RenderMode::PathTraced, 8);
    let ground = rig.ground();
    let (behind, beside) = (BEHIND_THE_WALL, BESIDE_THE_WALL);
    let (raster_shadow, cube_shadow, traced_shadow) = (
        patch(&ground, &raster, behind),
        patch(&ground, &cube, behind),
        patch(&ground, &traced, behind),
    );
    let (raster_lit, cube_lit, traced_lit) = (
        patch(&ground, &raster, beside),
        patch(&ground, &cube, beside),
        patch(&ground, &traced, beside),
    );
    eprintln!("behind the wall: unshadowed {raster_shadow:.4}, cube {cube_shadow:.4}, traced {traced_shadow:.4}; beside it: unshadowed {raster_lit:.4}, cube {cube_lit:.4}, traced {traced_lit:.4}");
    assert!(
        traced_shadow < raster_shadow * 0.1,
        "no traced shadow: {traced_shadow} against {raster_shadow}"
    );
    assert!(
        cube_shadow < raster_shadow * 0.1,
        "no raster cube shadow: {cube_shadow} against {raster_shadow}"
    );
    assert!(
        (0.85..1.25).contains(&(traced_lit / raster_lit)),
        "the unshadowed ground: {traced_lit} against {raster_lit}"
    );
    assert!(
        (cube_lit / raster_lit - 1.0).abs() < 0.02,
        "the cube shadows the open ground: {cube_lit} against {raster_lit}"
    );
}

/// The ray tracer's negative control: each punctual light's own shadow ray leaves the ground behind the wall black.
#[test]
fn the_ray_traced_mode_shadows_a_point_light_behind_an_occluder() {
    let walled = walled_point_light();
    let Some(mut rig) = Rig::new(&walled, from_above()) else {
        return;
    };
    rig.renderer.shadows.punctual = false;
    let raster = rig.radiance(RenderMode::Raster, 1);
    let traced = rig.radiance(RenderMode::RayTraced, 1);
    let ground = rig.ground();
    let eye = rig.camera.eye();
    let expected = mean(
        patch_pixels(&ground, BESIDE_THE_WALL)
            .iter()
            .map(|&i| point_radiance(&walled.lights[0], ground[i], eye) as f32),
    );
    let (raster_shadow, traced_shadow) = (
        patch(&ground, &raster, BEHIND_THE_WALL),
        patch(&ground, &traced, BEHIND_THE_WALL),
    );
    let (raster_lit, traced_lit) = (
        patch(&ground, &raster, BESIDE_THE_WALL),
        patch(&ground, &traced, BESIDE_THE_WALL),
    );
    eprintln!("ray traced, behind the wall: unshadowed {raster_shadow:.4}, traced {traced_shadow:.4}; beside it: analytic {expected:.5}, unshadowed {raster_lit:.5}, traced {traced_lit:.5}");
    assert!(
        traced_shadow < raster_shadow * 0.1,
        "no ray-traced shadow: {traced_shadow} against {raster_shadow}"
    );
    assert!(
        (traced_lit / expected - 1.0).abs() < 0.01,
        "the unshadowed ground: {traced_lit} against {expected}"
    );
    // Without a normal map raster keeps the geometric normal; the flat fallback's tilt read +2.2 % here.
    assert!(
        (raster_lit / expected - 1.0).abs() < 0.01,
        "raster's unshadowed ground: {raster_lit} against {expected}"
    );
}

/// A white card under a constant coloured environment: a bounce that misses sees that colour, so the card converges to it times E(mu) of about 0.99.
#[test]
fn a_bounce_that_misses_sees_the_loaded_environment() {
    let card = quad(
        [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, 1.0, 0.0),
        ],
        white(0.5),
    );
    let head_on = OrbitCamera {
        radius: 2.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    let Some(mut rig) = Rig::new(&scene(vec![card], Vec::new()), head_on) else {
        return;
    };
    let environment = Vec3::new(0.8, 0.4, 0.2);
    rig.renderer.light_dir_ambient = Vec4::new(0.0, 1.0, 0.0, 1.0);
    rig.renderer.set_environment(
        &rig.gpu,
        &EquirectImage::constant(32, 16, environment.to_array()),
    );
    let traced = rig.radiance(RenderMode::PathTraced, 12);
    let start = SIZE / 2 - 8;
    let patch: Vec<usize> = (start..start + 16)
        .flat_map(|y| (start..start + 16).map(move |x| (y * SIZE + x) as usize))
        .collect();
    let measured = Vec3::new(
        mean(patch.iter().map(|&i| traced[i].x)),
        mean(patch.iter().map(|&i| traced[i].y)),
        mean(patch.iter().map(|&i| traced[i].z)),
    );
    let relative = measured / environment;
    eprintln!("card under the environment {environment}: {measured}, relative {relative}");
    for c in relative.to_array() {
        assert!(
            (0.95..1.01).contains(&c),
            "the card is not lit by the environment: {relative}"
        );
    }
}
