//! The ray- and path-traced modes on a real adapter, ported from the C++ goldens: structural pixel properties, and HDR values where the estimator has an analytic answer.

use std::f64::consts::PI;
use std::sync::Arc;
use std::time::Duration;

use glam::{DVec3, Mat4, Vec3, Vec4};
use kataglyphis_webgpu_renderer::render::forward::ShaderSources;
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::render::tracer::{FurnaceSettings, RenderMode, TraceSettings};
use kataglyphis_webgpu_renderer::scene::{
    AlphaMode, CpuMaterial, CpuPrimitive, CpuSampler, CpuScene, CpuTexture, CpuTextureRef, Vertex,
};
use kataglyphis_webgpu_renderer::{
    load_gltf, EquirectImage, ForwardRenderer, GpuContext, OrbitCamera, SceneViewport,
};

/// The shadow tests' low sun, so the cube's shadow falls where the camera looks.
const LOW_SUN: Vec4 = Vec4::new(-1.0, 0.7, -0.3, 0.15);

/// A renderer over one scene with its traced scene built, the camera it renders with and the square output's side.
struct Rig {
    gpu: GpuContext,
    renderer: ForwardRenderer,
    camera: OrbitCamera,
    size: u32,
}

impl Rig {
    /// Bloom and SSAO off, so a pixel is the mode's own radiance through exposure and ACES.
    fn new(scene: &CpuScene, size: u32, camera: OrbitCamera) -> Option<Self> {
        let gpu = GpuContext::headless_or_skip()?;
        let mut renderer = ForwardRenderer::new(&gpu, size, size);
        renderer.upload_scene(&gpu, scene);
        renderer.set_trace_source(Arc::new(scene.clone()));
        renderer.bloom_strength = 0.0;
        renderer.ssao_strength = 0.0;
        let built = renderer
            .tracer_mut()
            .finish_build(&gpu, Duration::from_secs(120));
        assert!(built, "{:?}", renderer.trace_status());
        Some(Self {
            gpu,
            renderer,
            camera,
            size,
        })
    }

    /// `frames` frames in `mode`; the last one's RGBA8 pixels, sRGB-encoded.
    fn frames(&mut self, mode: RenderMode, frames: u32) -> Vec<u8> {
        self.renderer.render_mode = mode;
        let mut pixels = Vec::new();
        for _ in 0..frames {
            pixels = self
                .renderer
                .render_to_pixels(&self.gpu, self.size, self.size, &self.camera)
                .expect("renders");
        }
        pixels
    }

    /// The last frame's HDR radiance, RGBA per pixel.
    fn hdr(&self) -> Vec<[f32; 4]> {
        let bits = self
            .renderer
            .read_hdr_target(&self.gpu)
            .expect("reads back");
        bits.as_chunks::<4>()
            .0
            .iter()
            .map(|p| p.map(half_to_f32))
            .collect()
    }

    /// Mean HDR radiance over the `side` x `side` square at the frame's centre.
    fn centre_radiance(&self, side: u32) -> f32 {
        let hdr = self.hdr();
        let start = (self.size - side) / 2;
        let mut sum = 0.0;
        for y in start..start + side {
            for x in start..start + side {
                let p = hdr[(y * self.size + x) as usize];
                sum += (p[0] + p[1] + p[2]) / 3.0;
            }
        }
        sum / (side * side) as f32
    }
}

fn asset(name: &str) -> CpuScene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name);
    load_gltf(&path).unwrap_or_else(|e| panic!("{name} must load: {e:?}"))
}

fn luma(p: &[u8; 4]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

fn pixels(frame: &[u8]) -> &[[u8; 4]] {
    frame.as_chunks::<4>().0
}

/// Looking down at cube_on_plane, where the low sun throws the cube's shadow.
fn above_the_plane() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    }
}

/// A vertex of a flat, untextured test primitive.
fn vertex(position: Vec3, normal: Vec3, uv: [f32; 2]) -> Vertex {
    Vertex {
        position: position.to_array(),
        normal: normal.to_array(),
        uv,
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0; 4],
        uv1: uv,
    }
}

fn primitive(vertices: Vec<Vertex>, indices: Vec<u32>, material: CpuMaterial) -> CpuPrimitive {
    CpuPrimitive {
        vertices,
        indices,
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material,
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// A 2 x 2 card in the XY plane facing +Z.
fn card(material: CpuMaterial) -> CpuPrimitive {
    let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let vertices = corners
        .iter()
        .map(|&[x, y]| {
            vertex(
                Vec3::new(x, y, 0.0),
                Vec3::Z,
                [(x + 1.0) * 0.5, (1.0 - y) * 0.5],
            )
        })
        .collect();
    primitive(vertices, vec![0, 1, 2, 0, 2, 3], material)
}

/// A unit sphere at the origin, 32 x 16 quads, its normals the analytic ones.
fn sphere(material: CpuMaterial) -> CpuPrimitive {
    let (columns, rows) = (32u32, 16u32);
    let mut vertices = Vec::new();
    for row in 0..=rows {
        let theta = std::f32::consts::PI * row as f32 / rows as f32;
        for column in 0..=columns {
            let phi = std::f32::consts::TAU * column as f32 / columns as f32;
            let n = Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
            vertices.push(vertex(
                n,
                n,
                [column as f32 / columns as f32, row as f32 / rows as f32],
            ));
        }
    }
    let mut indices = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let a = row * (columns + 1) + column;
            let b = a + columns + 1;
            indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    primitive(vertices, indices, material)
}

fn scene_of(primitives: Vec<CpuPrimitive>) -> CpuScene {
    CpuScene {
        primitives,
        ..CpuScene::default()
    }
}

/// `brdf.slang`'s `brdf_direct` for a white surface times n.l, integrated over the hemisphere: a convex furnace pixel's value.
fn directional_albedo(mu: f64, metallic: f64, roughness: f64) -> f64 {
    const STEPS: usize = 160;
    let f0 = 0.04 * (1.0 - metallic) + metallic;
    let v = DVec3::new((1.0 - mu * mu).max(0.0).sqrt(), 0.0, mu);
    let a2 = roughness.max(0.045).powi(4);
    let k = (roughness + 1.0).powi(2) / 8.0;
    let smith = |c: f64| c / (c * (1.0 - k) + k).max(1e-6);
    let n_dot_v = mu.max(1e-4);
    let mut sum = 0.0;
    for i in 0..STEPS {
        let theta = (i as f64 + 0.5) / STEPS as f64 * PI * 0.5;
        for j in 0..STEPS {
            let phi = (j as f64 + 0.5) / STEPS as f64 * 2.0 * PI;
            let l = DVec3::new(
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            );
            let h = (v + l).normalize();
            let d = h.z * h.z * (a2 - 1.0) + 1.0;
            let ndf = a2 / (PI * (d * d).max(1e-20));
            let f = f0 + (1.0 - f0) * (1.0 - h.dot(v).max(0.0)).clamp(0.0, 1.0).powi(5);
            let specular = ndf * smith(n_dot_v) * smith(l.z) * f / (4.0 * n_dot_v * l.z).max(1e-6);
            let diffuse = (1.0 - f) * (1.0 - metallic) / PI;
            sum += (specular + diffuse) * l.z * theta.sin();
        }
    }
    sum * (PI * 0.5 / STEPS as f64) * (2.0 * PI / STEPS as f64)
}

/// What the tonemap writes for `radiance` at `ev` with bloom and SSAO off: ACES (Narkowicz), then the exact sRGB encode.
fn tonemapped(radiance: f32, ev: f32) -> f32 {
    let x = radiance * ev.exp2();
    let aces = ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0);
    let srgb = if aces < 0.003_130_8 {
        aces * 12.92
    } else {
        1.055 * aces.powf(1.0 / 2.4) - 0.055
    };
    srgb * 255.0
}

/// The plane pixels below half the lit plane's brightness, the plane being what the raster frame does not show red.
fn shadow_mask(frame: &[u8], plane: &[bool]) -> Vec<bool> {
    let mut lit: Vec<f32> = pixels(frame)
        .iter()
        .zip(plane)
        .filter(|(_, &on_plane)| on_plane)
        .map(|(p, _)| luma(p))
        .collect();
    lit.sort_by(f32::total_cmp);
    let bright = lit[lit.len() * 9 / 10];
    pixels(frame)
        .iter()
        .zip(plane)
        .map(|(p, &on_plane)| on_plane && luma(p) < bright * 0.5)
        .collect()
}

/// C++ `RaytracedShadowsAreDarkerThanLitGround`, and the traced shadow where the cascades put theirs.
#[test]
fn ray_traced_shadows_land_where_the_raster_shadows_are_and_go_black() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 128, above_the_plane()) else {
        return;
    };
    rig.renderer.light_dir_ambient = LOW_SUN;
    // The traced shadow ray blocks the sun whole, so the cascades compare at full strength, not the C++ default 0.65.
    rig.renderer.shadows.strength = 1.0;
    let raster = rig.frames(RenderMode::Raster, 1);
    let traced = rig.frames(RenderMode::RayTraced, 1);
    let plane: Vec<bool> = pixels(&raster)
        .iter()
        .map(|p| i32::from(p[0]) < i32::from(p[1]) + 40)
        .collect();
    let (raster_shadow, traced_shadow) =
        (shadow_mask(&raster, &plane), shadow_mask(&traced, &plane));
    let both = raster_shadow
        .iter()
        .zip(&traced_shadow)
        .filter(|(a, b)| **a && **b)
        .count();
    let either = raster_shadow
        .iter()
        .zip(&traced_shadow)
        .filter(|(a, b)| **a || **b)
        .count();
    let iou = both as f32 / either.max(1) as f32;
    eprintln!("shadow IoU, ray traced against the cascades: {iou:.3} over {either} pixels");
    assert!(either > 400, "no shadow to compare: {either} pixels");
    assert!(
        iou > 0.75,
        "the traced shadow is not where the raster one is: IoU {iou}"
    );
    // The C++ mode has no ambient term, so a shadowed plane pixel goes black, not to the IBL's blue.
    let mut plane_luma: Vec<f32> = pixels(&traced)
        .iter()
        .zip(&plane)
        .filter(|(_, &on_plane)| on_plane)
        .map(|(p, _)| luma(p))
        .collect();
    plane_luma.sort_by(f32::total_cmp);
    let decile = plane_luma.len() / 10;
    let dark: f32 = plane_luma[..decile].iter().sum::<f32>() / decile as f32;
    let lit: f32 = plane_luma[plane_luma.len() - decile..].iter().sum::<f32>() / decile as f32;
    assert!(
        dark < lit * 0.05,
        "darkest decile {dark} against the lit {lit}"
    );
}

/// The C++ shadow rig (`ShadowTest/shadow_rig.obj`): a 60 x 60 ground at y = 0 under a 44 x 2 x 12 slab centred at (0, 9, -4).
fn shadow_rig() -> CpuScene {
    let ground = quad([
        Vec3::new(-30.0, 0.0, -30.0),
        Vec3::new(-30.0, 0.0, 30.0),
        Vec3::new(30.0, 0.0, 30.0),
        Vec3::new(30.0, 0.0, -30.0),
    ]);
    let (min, max) = (Vec3::new(-22.0, 8.0, -10.0), Vec3::new(22.0, 10.0, 2.0));
    let mut slab = Vec::new();
    for axis in 0..3 {
        for side in [min, max] {
            let normal = (side - (min + max) * 0.5) * Vec3::AXES[axis];
            slab.push(box_face(min, max, axis, normal.normalize()));
        }
    }
    let mut primitives = vec![ground];
    primitives.extend(slab);
    scene_of(primitives)
}

/// A quad from four counter-clockwise corners, its normal theirs.
fn quad(corners: [Vec3; 4]) -> CpuPrimitive {
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize();
    let vertices = corners
        .iter()
        .map(|&c| vertex(c, normal, [0.0, 0.0]))
        .collect();
    primitive(vertices, vec![0, 1, 2, 0, 2, 3], CpuMaterial::default())
}

/// The face of the `min`..`max` box whose outward normal is `normal`, along `axis`.
fn box_face(min: Vec3, max: Vec3, axis: usize, normal: Vec3) -> CpuPrimitive {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let corner = |a: bool, b: bool| {
        let mut c = if normal[axis] > 0.0 { max } else { min };
        c[u] = if a { max[u] } else { min[u] };
        c[v] = if b { max[v] } else { min[v] };
        c
    };
    let mut corners = [
        corner(false, false),
        corner(true, false),
        corner(true, true),
        corner(false, true),
    ];
    if (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .dot(normal)
        < 0.0
    {
        corners.reverse();
    }
    quad(corners)
}

/// C++ `PathTracingPassesTheWhiteFurnaceTest`'s rig half: a sky of 2 at -1 EV is the C++ golden's radiance 1, ACES-encoded to 231.6.
#[test]
fn the_white_furnace_converges_to_the_sky_through_the_post_chain() {
    // Down on the rig, so every pixel is the ground or the slab, as in the C++ crop.
    let camera = OrbitCamera {
        target: Vec3::new(0.0, 4.0, -4.0),
        radius: 30.0,
        yaw_deg: 90.0,
        pitch_deg: 60.0,
        ..OrbitCamera::default()
    };
    let Some(mut rig) = Rig::new(&shadow_rig(), 64, camera) else {
        return;
    };
    let furnace = FurnaceSettings {
        radiance: 2.0,
        ..FurnaceSettings::default()
    };
    rig.renderer.trace_settings = TraceSettings {
        samples_per_pixel: 8,
        max_bounces: 16,
        furnace: Some(furnace),
    };
    rig.renderer.exposure_ev = -1.0;
    let frame = rig.frames(RenderMode::PathTraced, 12);
    let relative: Vec<f32> = rig
        .hdr()
        .iter()
        .map(|p| (p[0] + p[1] + p[2]) / 6.0)
        .collect();
    let mean = relative.iter().sum::<f32>() / relative.len() as f32;
    let uniform = relative.iter().filter(|r| (*r - 1.0).abs() <= 0.1).count();
    eprintln!(
        "furnace HDR mean {mean} of the sky, {uniform} of {} within 0.1",
        relative.len()
    );
    // The C++ bounds: E(mu) <= 1 at roughness 0.5 caps the mean at 1, and 1 % lost a bounce stays above 0.98.
    assert!((0.98..1.002).contains(&mean), "{mean}");
    assert!(
        uniform * 100 > relative.len() * 98,
        "geometry shows through the furnace"
    );
    let expected = tonemapped(1.0, 0.0);
    let lumas: Vec<f32> = pixels(&frame).iter().map(luma).collect();
    let mean8 = lumas.iter().sum::<f32>() / lumas.len() as f32;
    eprintln!("furnace 8-bit mean {mean8} (analytic {expected:.1})");
    assert!((mean8 - expected).abs() < 3.0, "{mean8} against {expected}");
}

/// A lone white card's centre radiance in the furnace, seen at `mu` (1 or 0.5), and its analytic E(mu).
fn card_albedo(rig: &mut Rig, metallic: f32, roughness: f32, mu: f64) -> (f64, f64) {
    rig.renderer.trace_settings = TraceSettings {
        samples_per_pixel: 8,
        max_bounces: 2,
        furnace: Some(FurnaceSettings {
            radiance: 1.0,
            metallic,
            roughness,
        }),
    };
    rig.camera = OrbitCamera {
        target: Vec3::ZERO,
        radius: 2.0,
        // The card faces +Z, so the eye's yaw from +X is asin(mu).
        yaw_deg: mu.asin().to_degrees() as f32,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    rig.frames(RenderMode::PathTraced, 16);
    let measured = f64::from(rig.centre_radiance(16));
    let expected = directional_albedo(mu, f64::from(metallic), f64::from(roughness));
    eprintln!("white card, metallic {metallic}, roughness {roughness}, mu {mu}: {measured:.4}, E(mu) {expected:.4}");
    (measured, expected)
}

fn lone_card() -> Option<Rig> {
    Rig::new(
        &scene_of(vec![card(CpuMaterial::default())]),
        64,
        OrbitCamera::default(),
    )
}

/// C++ `PathTracingPassesTheWhiteMetalFurnaceTest`: a metal card returns its GGX lobe's directional albedo of a uniform sky.
#[test]
fn a_white_metal_card_reflects_its_directional_albedo() {
    let Some(mut rig) = lone_card() else {
        return;
    };
    for roughness in [0.3, 0.7] {
        for mu in [1.0, 0.5] {
            let (measured, expected) = card_albedo(&mut rig, 1.0, roughness, mu);
            assert!(
                (measured - expected).abs() < 0.03,
                "{measured} against {expected}"
            );
        }
    }
}

/// C++ `PathTracingPassesTheWhiteFurnaceTest`'s card half: at 0.25 the 0.1 floor on the GGX lobe's chance decides the mixture.
#[test]
fn a_white_dielectric_card_reflects_its_directional_albedo() {
    let Some(mut rig) = lone_card() else {
        return;
    };
    for roughness in [0.5, 0.25] {
        for mu in [1.0, 0.5] {
            let (measured, expected) = card_albedo(&mut rig, 0.0, roughness, mu);
            assert!(
                (measured - expected).abs() < 0.01,
                "the lobe mixture's weight is off at roughness {roughness}, mu {mu}: {measured} against {expected}"
            );
        }
    }
}

/// Mean absolute change of the HDR radiance between two frames.
fn change(a: &[[f32; 4]], b: &[[f32; 4]]) -> f32 {
    let total: f32 = a
        .iter()
        .zip(b)
        .map(|(p, q)| (0..3).map(|c| (p[c] - q[c]).abs()).sum::<f32>())
        .sum();
    total / (a.len() * 3) as f32
}

/// C++ `PathTracingAccumulatesAndConverges`: new samples every frame, and a mean that moves less the more it holds.
#[test]
fn path_tracing_accumulates_and_converges() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 64, OrbitCamera::default()) else {
        return;
    };
    rig.renderer.trace_settings = TraceSettings {
        samples_per_pixel: 1,
        max_bounces: 4,
        furnace: None,
    };
    rig.frames(RenderMode::PathTraced, 1);
    let first = rig.hdr();
    rig.frames(RenderMode::PathTraced, 1);
    let early = change(&first, &rig.hdr());
    rig.frames(RenderMode::PathTraced, 30);
    let before = rig.hdr();
    rig.frames(RenderMode::PathTraced, 1);
    let late = change(&before, &rig.hdr());
    eprintln!("mean change per frame: early {early:.5}, late {late:.5}");
    assert_eq!(rig.renderer.accumulated_samples(), 33);
    assert!(early > 1e-3, "frames draw the same samples: {early}");
    assert!(
        late < early / 4.0,
        "the mean does not settle: {early} then {late}"
    );
}

/// A premise of the path tracer's mean, and how a test changes it.
type Premise = (&'static str, fn(&mut Rig));

/// The C++ `PathTracingHistoryKey` and its other resets, with the sample count as the oracle.
#[test]
fn the_accumulation_starts_over_whenever_a_premise_changes() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 32, OrbitCamera::default()) else {
        return;
    };
    rig.renderer.trace_settings.max_bounces = 2;
    let changes: [Premise; 14] = [
        ("the camera", |r| r.camera.yaw_deg += 1.0),
        ("the viewport", |r| r.camera.viewport.x0 = 0.2),
        ("the sun's direction", |r| {
            r.renderer.light_dir_ambient.x += 0.1
        }),
        ("the sun's intensity", |r| {
            r.renderer.light_color_intensity.w += 1.0
        }),
        ("the sky's light", |r| r.renderer.light_dir_ambient.w += 0.1),
        ("the samples", |r| {
            r.renderer.trace_settings.samples_per_pixel = 3
        }),
        ("the bounces", |r| r.renderer.trace_settings.max_bounces = 3),
        ("the furnace", |r| {
            r.renderer.trace_settings.furnace = Some(FurnaceSettings::default())
        }),
        ("the model transform", |r| {
            let root = Mat4::from_translation(Vec3::Y);
            r.renderer.tracer_mut().set_root_transform(&r.gpu, root);
        }),
        ("the environment", |r| {
            let sky = EquirectImage::constant(8, 4, [1.0, 1.0, 1.0]);
            r.renderer.set_environment(&r.gpu, &sky);
        }),
        ("an explicit reset", |r| r.renderer.reset_accumulation()),
        ("a raster frame", |r| drop(r.frames(RenderMode::Raster, 1))),
        ("a ray-traced frame", |r| {
            drop(r.frames(RenderMode::RayTraced, 1))
        }),
        ("the output size", |r| r.size = 48),
    ];
    for (premise, change) in changes {
        rig.renderer.reset_accumulation();
        rig.frames(RenderMode::PathTraced, 3);
        let per_frame = u64::from(rig.renderer.trace_settings.samples_per_pixel);
        assert_eq!(
            rig.renderer.accumulated_samples(),
            3 * per_frame,
            "before {premise}"
        );
        change(&mut rig);
        rig.frames(RenderMode::PathTraced, 1);
        let per_frame = u64::from(rig.renderer.trace_settings.samples_per_pixel);
        assert_eq!(
            rig.renderer.accumulated_samples(),
            per_frame,
            "{premise} kept the mean"
        );
    }
}

/// A new scene is a premise too: its first traced frame starts over, with no raster frame between to reset it.
#[test]
fn a_new_scene_starts_the_accumulation_over() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 32, OrbitCamera::default()) else {
        return;
    };
    rig.frames(RenderMode::PathTraced, 2);
    assert_eq!(rig.renderer.accumulated_samples(), 16);
    let scene = asset("cube.gltf");
    rig.renderer.upload_scene(&rig.gpu, &scene);
    rig.renderer.set_trace_source(Arc::new(scene));
    // Built before the frame: one frame racing its build thread may draw either mode.
    assert!(rig
        .renderer
        .tracer_mut()
        .finish_build(&rig.gpu, Duration::from_secs(120)));
    rig.frames(RenderMode::PathTraced, 1);
    assert_eq!(rig.renderer.drawn_mode(), RenderMode::PathTraced);
    assert_eq!(rig.renderer.accumulated_samples(), 8);
}

/// An output whose accumulation the device cannot hold rasters, and says so: drawn mode, sample count and the reason.
#[test]
fn an_accumulation_past_the_device_limit_draws_and_reports_raster() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 32, OrbitCamera::default()) else {
        return;
    };
    rig.renderer.ssao_strength = 0.7;
    let raster = rig.frames(RenderMode::Raster, 2);
    // One byte short of a 32 x 32 accumulation; a 16 x 16 one still fits.
    rig.renderer.cap_accumulation_bytes(32 * 32 * 16 - 1);
    let refused = rig.frames(RenderMode::PathTraced, 2);
    assert_eq!(rig.renderer.drawn_mode(), RenderMode::Raster);
    assert_eq!(rig.renderer.accumulated_samples(), 0);
    let why = rig.renderer.traced_fallback().expect("a reason");
    assert!(why.contains("32 x 32 accumulation"), "{why}");
    // The raster frame keeps its SSAO, which a frame reported as traced would drop.
    assert!(refused == raster, "the refused frame is not the raster one");
    rig.size = 16;
    rig.frames(RenderMode::PathTraced, 1);
    assert_eq!(rig.renderer.drawn_mode(), RenderMode::PathTraced);
    assert_eq!(rig.renderer.traced_fallback(), None);
    assert_eq!(rig.renderer.accumulated_samples(), 8);
}

/// cube_on_plane's ground plane alone, and with a horizontal 2 x 2 card above it whose texture's right half is cut.
fn card_over_plane(alpha_mode: AlphaMode) -> [CpuScene; 2] {
    let base = asset("cube_on_plane.gltf");
    let plane = base
        .primitives
        .iter()
        .find(|p| p.vertices.len() == 4)
        .expect("the ground plane is the 4-vertex primitive")
        .clone();
    let cutout = CpuTextureRef {
        texture: Arc::new(CpuTexture {
            width: 2,
            height: 1,
            rgba8: vec![255, 255, 255, 255, 255, 255, 255, 0],
            compressed: None,
        }),
        sampler: CpuSampler {
            mag_nearest: true,
            min_nearest: true,
            ..CpuSampler::default()
        },
        srgb: true,
    };
    let mut card = card(CpuMaterial {
        alpha_mode,
        metallic_factor: 0.0,
        base_color_texture: Some(cutout),
        ..CpuMaterial::default()
    });
    card.transform =
        Mat4::from_translation(Vec3::Y * 1.4) * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    [scene_of(vec![plane.clone()]), scene_of(vec![plane, card])]
}

/// Pixels a caster darkens below `keep` of the plane alone; the lit white card itself never is.
fn shadowed(baseline: &[u8], with_card: &[u8], keep: f32) -> usize {
    pixels(baseline)
        .iter()
        .zip(pixels(with_card))
        .filter(|(b, w)| luma(w) < luma(b) * keep)
        .count()
}

/// C++ `MaskCardDiscardsCutoutTexelsInRaytracing` and `PathTracedMaskCardShowsItsCutout`, at the shadow: the cut half lets the sun through.
#[test]
fn a_mask_cutout_lets_the_sun_through_in_both_traced_modes() {
    let [plane, opaque] = card_over_plane(AlphaMode::Opaque);
    let [_, masked] = card_over_plane(AlphaMode::Mask(0.5));
    let render = |scene: &CpuScene, mode: RenderMode| -> Option<Vec<u8>> {
        let mut rig = Rig::new(scene, 96, above_the_plane())?;
        rig.renderer.light_dir_ambient = LOW_SUN;
        Some(rig.frames(mode, 4))
    };
    // The path tracer's sky fills a shadow in, so there the sun's loss alone marks it.
    for (mode, keep) in [(RenderMode::RayTraced, 0.5), (RenderMode::PathTraced, 0.85)] {
        let Some(baseline) = render(&plane, mode) else {
            return;
        };
        let full = shadowed(&baseline, &render(&opaque, mode).expect("renders"), keep);
        let cut = shadowed(&baseline, &render(&masked, mode).expect("renders"), keep);
        let ratio = cut as f32 / full.max(1) as f32;
        eprintln!("{mode:?}: the masked card shadows {cut} pixels, the opaque one {full}");
        assert!(
            full > 200,
            "{mode:?}: the opaque card casts no shadow the camera sees"
        );
        assert!(
            (0.3..0.7).contains(&ratio),
            "{mode:?}: masked against opaque shadow {ratio}"
        );
    }
}

/// The HDR radiance of the pixels well inside a unit sphere at the target, 4 units off, at 96 x 96.
fn sphere_disk(rig: &Rig) -> Vec<Vec3> {
    let hdr = rig.hdr();
    (0..96 * 96)
        .filter(|i| ((i % 96) as f32 - 47.5).hypot((i / 96) as f32 - 47.5) < 26.0)
        .map(|i| Vec3::new(hdr[i][0], hdr[i][1], hdr[i][2]))
        .collect()
}

/// C++ `PathTracingHonorsTheUntexturedMetallicFactor`: the factor alone, without a texture, turns a sphere to metal.
#[test]
fn the_untextured_metallic_factor_changes_a_spheres_look() {
    let camera = OrbitCamera {
        target: Vec3::ZERO,
        radius: 4.0,
        yaw_deg: 90.0,
        pitch_deg: 20.0,
        ..OrbitCamera::default()
    };
    let orange = |metallic_factor: f32| CpuMaterial {
        base_color: [0.9, 0.5, 0.15, 1.0],
        metallic_factor,
        roughness_factor: 0.4,
        ..CpuMaterial::default()
    };
    for mode in [RenderMode::RayTraced, RenderMode::PathTraced] {
        let mut disks = Vec::new();
        for metallic in [0.0, 1.0] {
            let Some(mut rig) = Rig::new(
                &scene_of(vec![sphere(orange(metallic))]),
                96,
                camera.clone(),
            ) else {
                return;
            };
            rig.frames(mode, 6);
            disks.push(sphere_disk(&rig));
        }
        let mean = |disk: &[Vec3]| {
            disk.iter().map(|p| p.element_sum() / 3.0).sum::<f32>() / disk.len() as f32
        };
        let difference = disks[0]
            .iter()
            .zip(&disks[1])
            .map(|(a, b)| (*a - *b).abs().element_sum() / 3.0)
            .sum::<f32>()
            / disks[0].len() as f32;
        let (dielectric, metal) = (mean(&disks[0]), mean(&disks[1]));
        eprintln!("{mode:?}: sphere disk radiance, dielectric {dielectric:.4}, metal {metal:.4}, mean change {difference:.4}");
        assert!(
            dielectric > 0.05,
            "{mode:?}: the dielectric sphere is unlit"
        );
        assert!(
            difference > 0.25 * dielectric,
            "{mode:?}: the metal shades like its dielectric twin"
        );
        if mode == RenderMode::PathTraced {
            // C++ `PathTracedMetalCardReflectsTheSkyAndTheSun`: the GGX lobe reflects the sky, so a metal is never black.
            assert!(
                metal > 0.1 * dielectric,
                "the path-traced metal is (nearly) black: {metal}"
            );
        }
    }
}

/// C++ `PathTracedUnlitCardKeepsItsAlbedo`: an unlit surface shows its albedo in every mode, the path tracer's last radiance.
#[test]
fn an_unlit_material_keeps_its_albedo_in_every_mode() {
    let Some(mut rig) = Rig::new(&asset("cube_unlit.gltf"), 64, OrbitCamera::default()) else {
        return;
    };
    for mode in RenderMode::ALL {
        rig.frames(mode, 2);
        let hdr = rig.hdr();
        let centre = hdr[(32 * 64 + 32) as usize];
        eprintln!("{mode:?}: unlit centre {centre:?}");
        for (channel, albedo) in [0.8f32, 0.2, 0.2].into_iter().enumerate() {
            assert!(
                (centre[channel] - albedo).abs() < 0.01,
                "{mode:?} channel {channel}: {centre:?}"
            );
        }
    }
}

/// C++ `Integration.RenderModesSelectableInGui`'s round trip: tracing leaves no trace in the raster frame, SSAO and bloom included.
#[test]
fn a_mode_switch_round_trip_leaves_raster_unchanged() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 64, OrbitCamera::default()) else {
        return;
    };
    rig.renderer.bloom_strength = 0.6;
    rig.renderer.ssao_strength = 0.7;
    let before = rig.frames(RenderMode::Raster, 2);
    let traced = rig.frames(RenderMode::RayTraced, 2);
    rig.frames(RenderMode::PathTraced, 2);
    let after = rig.frames(RenderMode::Raster, 2);
    assert_ne!(before, traced, "the ray-traced frame is the raster one");
    assert!(
        before == after,
        "a raster frame after tracing differs from the one before"
    );
}

/// C++ `RaytracingFrameSkipsTheRasterPass` and `TracedFramesSkipTheShadowPass`: no raster pass runs, reports or is timed.
#[test]
fn traced_frames_skip_the_raster_passes_and_time_their_own() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 32, OrbitCamera::default()) else {
        return;
    };
    let timed = rig.renderer.enable_gpu_timing(&rig.gpu);
    rig.renderer.trace_settings.samples_per_pixel = 1;
    rig.frames(RenderMode::Raster, 3);
    assert!(rig.renderer.occlusion_cull_stats().1 > 0 && rig.renderer.shadow_caster_stats().1 > 0);
    // Past the timing window, so a raster pass's last sample has left its average.
    for (mode, own) in [
        (RenderMode::RayTraced, "RayTrace"),
        (RenderMode::PathTraced, "PathTrace"),
    ] {
        rig.frames(mode, 40);
        assert_eq!(rig.renderer.occlusion_cull_stats(), (0, 0), "{mode:?}");
        assert_eq!(rig.renderer.shadow_caster_stats(), (0, 0), "{mode:?}");
        if !timed {
            continue;
        }
        let names: Vec<&str> = rig
            .renderer
            .gpu_timings_ms()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        eprintln!("{mode:?} timings: {:?}", rig.renderer.gpu_timings_ms());
        assert!(
            names.contains(&own) && names.contains(&"TraceResolve"),
            "{names:?}"
        );
        for raster in [
            "ShadowCascades",
            "Forward",
            "GBuffer",
            "DeferredLighting",
            "Ssao",
            "OcclusionCull",
        ] {
            assert!(
                !names.contains(&raster),
                "{mode:?} still times {raster}: {names:?}"
            );
        }
    }
}

/// The cube's pixels: red or near-black against the bluish sky, in raster and traced frames alike.
fn cube_centroid(frame: &[u8], size: u32) -> (f32, f32) {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
    for (i, p) in pixels(frame).iter().enumerate() {
        let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
        if r > g + 30 || r + g + b < 60 {
            x += (i as u32 % size) as f32;
            y += (i as u32 / size) as f32;
            n += 1.0;
        }
    }
    assert!(n > 20.0, "the cube is not in view");
    (x / n, y / n)
}

/// The traced camera is the raster one, the docked editor's lens shift included.
#[test]
fn the_traced_camera_follows_the_viewport_lens_shift() {
    let Some(mut rig) = Rig::new(&asset("cube.gltf"), 96, OrbitCamera::default()) else {
        return;
    };
    let docked = SceneViewport {
        x0: 0.155,
        y0: 0.04,
        x1: 0.79,
        y1: 0.77,
    };
    let mut centroids = Vec::new();
    for viewport in [SceneViewport::FULL, docked] {
        rig.camera.viewport = viewport;
        let raster = cube_centroid(&rig.frames(RenderMode::Raster, 1), 96);
        let traced = cube_centroid(&rig.frames(RenderMode::RayTraced, 1), 96);
        eprintln!("{viewport:?}: cube centroid raster {raster:?}, ray traced {traced:?}");
        let offset = (raster.0 - traced.0).hypot(raster.1 - traced.1);
        assert!(
            offset < 1.0,
            "the traced cube sits {offset} px off the raster one"
        );
        centroids.push(traced);
    }
    let shift = (centroids[0].0 - centroids[1].0).hypot(centroids[0].1 - centroids[1].1);
    assert!(
        shift > 5.0,
        "the docked viewport did not move the traced cube: {shift} px"
    );
}

/// An animated node moves in the traced frame as in the raster one, and its move starts the mean over.
#[test]
fn the_traced_scene_follows_an_animated_node() {
    let Some(mut rig) = Rig::new(&asset("cube_animated.gltf"), 64, OrbitCamera::default()) else {
        return;
    };
    rig.renderer.set_animation_time(0.0);
    let start = rig.frames(RenderMode::RayTraced, 1);
    rig.renderer.set_animation_time(1.0);
    let moved = rig.frames(RenderMode::RayTraced, 1);
    assert_ne!(start, moved, "the traced cube did not slide");
    rig.renderer.set_animation_time(1.0);
    assert_eq!(
        rig.frames(RenderMode::RayTraced, 1),
        moved,
        "a held pose traces the same frame"
    );
    rig.frames(RenderMode::PathTraced, 2);
    assert_eq!(
        rig.renderer.accumulated_samples(),
        16,
        "a held pose accumulates"
    );
    rig.renderer.set_animation_time(1.5);
    rig.frames(RenderMode::PathTraced, 1);
    assert_eq!(rig.renderer.accumulated_samples(), 8, "a move starts over");
}

/// Mean x of the reddish pixels in the top half of a `size`-wide frame, where bending the bar's upper joint swings it.
fn upper_red_x(pixels: &[u8], size: u32) -> f32 {
    let (mut sum, mut count) = (0.0f32, 0u32);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i as u32 % size, i as u32 / size);
        let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
        if y < size / 2 && r > 40 && r > g + 20 && r > b + 20 {
            sum += x as f32;
            count += 1;
        }
    }
    assert!(count > 30, "the bar is not in the top half ({count} px)");
    sum / count as f32
}

/// A skinned bar bends in the traced frame as in the raster one: the tracer re-poses and refits its mesh.
#[test]
fn the_traced_scene_follows_a_skin() {
    let camera = OrbitCamera {
        radius: 5.0,
        pitch_deg: 5.0,
        yaw_deg: 90.0,
        target: glam::Vec3::new(0.0, 1.0, 0.0),
        ..OrbitCamera::default()
    };
    let size = 128;
    let Some(mut rig) = Rig::new(&asset("skinned_bar.gltf"), size, camera) else {
        return;
    };
    let mut centroids = Vec::new();
    for time in [0.0, 1.0] {
        rig.renderer.set_animation_time(time);
        let raster = upper_red_x(&rig.frames(RenderMode::Raster, 1), size);
        let traced = upper_red_x(&rig.frames(RenderMode::RayTraced, 1), size);
        eprintln!("t {time}: upper bar x raster {raster:.2}, ray traced {traced:.2}");
        // The traced frame has no ambient term, so its shadowed faces drop out of the red pixels.
        assert!(
            (raster - traced).abs() < 3.0,
            "at t {time} the traced bar sits at {traced}, the raster one at {raster}"
        );
        centroids.push((raster, traced));
    }
    let raster_bend = centroids[1].0 - centroids[0].0;
    let traced_bend = centroids[1].1 - centroids[0].1;
    assert!(
        raster_bend.abs() > 8.0,
        "the raster bar did not bend: {centroids:?}"
    );
    assert!(
        (traced_bend - raster_bend).abs() < 1.5,
        "the traced bar bent by {traced_bend} px, the raster one by {raster_bend}"
    );
    rig.frames(RenderMode::PathTraced, 2);
    rig.renderer.set_animation_time(1.5);
    rig.frames(RenderMode::PathTraced, 1);
    assert_eq!(
        rig.renderer.accumulated_samples(),
        8,
        "a bend starts the mean over"
    );
}

/// `source` with `old` replaced, which must occur: the generated WGSL's names move when the Slang does.
fn edited(source: &str, old: &str, new: &str) -> String {
    let edited = source.replace(old, new);
    assert_ne!(
        edited, source,
        "the generated WGSL no longer contains `{old}`"
    );
    edited
}

/// F5 rebuilds both kernels and the resolve inside the reload's error scope, though each compiles lazily on its mode's
/// first frame otherwise: a kernel or resolve without its entry point is refused before that mode ever drew, the old
/// pipelines keep drawing, an edited kernel draws from the next frame and the shipped WGSL brings the frame back.
#[test]
fn a_shader_reload_rebuilds_the_traced_kernels_or_keeps_them() {
    let Some(mut rig) = Rig::new(&asset("cube_on_plane.gltf"), 32, above_the_plane()) else {
        return;
    };
    let shipped = ShaderSources::shipped();
    let before = rig.frames(RenderMode::RayTraced, 1);
    assert!(
        !rig.renderer.traced_kernel_compiled(RenderMode::PathTraced),
        "the path kernel compiles on its first frame"
    );
    let no_path = edited(
        shipped.traced,
        "fn cs_path_trace(",
        "fn cs_path_trace_gone(",
    );
    let no_resolve = edited(shipped.trace_resolve, "fn fs_main(", "fn fs_main_gone(");
    for broken in [
        ShaderSources {
            traced: &no_path,
            ..shipped
        },
        ShaderSources {
            trace_resolve: &no_resolve,
            ..shipped
        },
    ] {
        assert!(
            rig.renderer.reload_shaders(&rig.gpu, &broken).is_err(),
            "a traced shader without its entry point must be refused"
        );
        assert!(
            rig.frames(RenderMode::RayTraced, 1) == before,
            "the refused reload changed the frame"
        );
    }
    // Every hit radiates a flat quarter; misses keep the sky pass.
    let flat = edited(
        shipped.traced,
        "vec4<f32>(hdr_safe_0(traced_0.xyz), traced_0.w)",
        "vec4<f32>(0.25f, 0.25f, 0.25f, traced_0.w)",
    );
    let reload = ShaderSources {
        traced: &flat,
        ..shipped
    };
    rig.renderer
        .reload_shaders(&rig.gpu, &reload)
        .expect("a valid kernel must reload");
    assert!(rig.renderer.traced_kernel_compiled(RenderMode::PathTraced));
    rig.frames(RenderMode::RayTraced, 1);
    let centre = rig.hdr()[(16 * 32 + 16) as usize];
    assert_eq!(&centre[..3], &[0.25; 3], "the edited kernel did not draw");
    rig.renderer
        .reload_shaders(&rig.gpu, &shipped)
        .expect("the shipped WGSL must reload");
    assert!(
        rig.frames(RenderMode::RayTraced, 1) == before,
        "the shipped kernel did not bring the frame back"
    );
}
