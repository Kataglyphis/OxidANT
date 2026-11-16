//! The C++ Sun tab's shadow controls on the GPU: PCF radius, resolution, cascades, strength, switch and distance.
//! Structural pixel checks on `cube_on_plane.gltf`, ported from BeschleunigerBallett's shadow goldens.

use glam::{Vec3, Vec4};
use kataglyphis_webgpu_renderer::render::forward::CASCADE_COUNT;
use kataglyphis_webgpu_renderer::scene::{CpuMaterial, CpuPrimitive, Vertex};
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: u32 = 256;
/// Darker than this many levels counts as shadowed, as in the C++ goldens' masks.
const SHADOWED: f32 = 40.0;

fn cube_on_plane() -> CpuScene {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    load_gltf(path).expect("cube_on_plane.gltf must load")
}

fn camera() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    }
}

/// The proven shadow tests' low light, at full strength, without SSAO and bloom to keep the masks clean.
fn renderer(gpu: &GpuContext, scene: &CpuScene) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(gpu, scene);
    renderer.light_dir_ambient = Vec4::new(-1.0, 0.7, -0.3, 0.15);
    renderer.ssao_strength = 0.0;
    renderer.bloom_strength = 0.0;
    renderer.shadows.strength = 1.0;
    renderer
}

fn render(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

fn luma(p: &[u8; 4]) -> f32 {
    0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])
}

/// Per pixel, how many levels `shadowed` is darker than `lit`.
fn darkening(lit: &[u8], shadowed: &[u8]) -> Vec<f32> {
    lit.as_chunks::<4>()
        .0
        .iter()
        .zip(shadowed.as_chunks::<4>().0)
        .map(|(l, s)| luma(l) - luma(s))
        .collect()
}

/// The frame with shadows switched off, the reference every mask is taken against.
fn unshadowed(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    let enabled = renderer.shadows.enabled;
    renderer.shadows.enabled = false;
    let lit = render(gpu, renderer, camera);
    renderer.shadows.enabled = enabled;
    lit
}

fn shadowed_pixels(darkening: &[f32]) -> usize {
    darkening.iter().filter(|d| **d > SHADOWED).count()
}

#[test]
fn the_penumbra_grows_with_the_pcf_radius() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, &cube_on_plane());
    let lit = unshadowed(&gpu, &mut renderer, &camera());
    let mut masks = Vec::new();
    for radius in [0, 2, 5] {
        renderer.shadows.pcf_radius = radius;
        masks.push(darkening(&lit, &render(&gpu, &mut renderer, &camera())));
    }
    let umbra = masks[0].iter().copied().fold(0.0, f32::max);
    assert!(
        umbra > 80.0,
        "the hard shadow darkens by only {umbra} levels"
    );
    // Partly shadowed: darker than noise, lighter than the umbra.
    let penumbra = |mask: &[f32]| {
        mask.iter()
            .filter(|d| **d > 0.1 * umbra && **d < 0.9 * umbra)
            .count()
    };
    let counts: Vec<usize> = masks.iter().map(|mask| penumbra(mask)).collect();
    eprintln!("penumbra pixels at radius 0, 2, 5: {counts:?}");
    assert!(
        counts[0] < counts[1] && counts[1] < counts[2],
        "the penumbra must widen with the radius: {counts:?}"
    );
    assert!(counts[2] > counts[0] * 2, "{counts:?}");
    let umbras: Vec<usize> = masks.iter().map(|mask| shadowed_pixels(mask)).collect();
    assert!(umbras.iter().all(|count| *count > 500), "{umbras:?}");
}

#[test]
fn every_resolution_reallocates_the_maps_and_keeps_the_shadow() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_on_plane();
    let mut renderer = renderer(&gpu, &scene);
    let lit = unshadowed(&gpu, &mut renderer, &camera());
    for resolution in [512, 4096, 1024, 2048] {
        renderer.shadows.resolution = resolution;
        let switched = render(&gpu, &mut renderer, &camera());
        assert_eq!(renderer.shadow_map_resolution(), resolution);
        if resolution == 512 {
            let depths = renderer.read_shadow_map(&gpu).expect("shadow map readback");
            assert_eq!(depths.len(), CASCADE_COUNT * 512 * 512);
        }
        let shadowed = shadowed_pixels(&darkening(&lit, &switched));
        assert!(shadowed > 500, "{resolution}: {shadowed} shadowed pixels");
        // A fresh renderer at this resolution: the frame after a switch samples the new maps, not the old ones.
        let mut fresh = self::renderer(&gpu, &scene);
        fresh.shadows.resolution = resolution;
        let reference = render(&gpu, &mut fresh, &camera());
        let differing = differing_pixels(&switched, &reference, 2);
        assert!(
            differing < 10,
            "{resolution}: {differing} pixels differ from a fresh renderer"
        );
    }
}

/// Pixels where any channel of `a` and `b` differs by more than `tolerance`.
fn differing_pixels(a: &[u8], b: &[u8], tolerance: u8) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| x.abs_diff(*y) > tolerance)
        })
        .count()
}

#[test]
fn one_two_and_three_cascades_each_cast_the_shadow_from_their_own_layers() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, &cube_on_plane());
    // Short enough that every cascade's slice holds part of the plane the camera looks down on.
    renderer.shadows.distance = 12.0;
    let lit = unshadowed(&gpu, &mut renderer, &camera());
    for count in 1..=CASCADE_COUNT as u32 {
        renderer.shadows.cascade_count = count;
        let frame = render(&gpu, &mut renderer, &camera());
        assert_eq!(renderer.shadow_map_layers(), count as usize);
        let shadowed = shadowed_pixels(&darkening(&lit, &frame));
        assert!(
            shadowed > 500,
            "{count} cascades: {shadowed} shadowed pixels"
        );
        let depths = renderer.read_shadow_map(&gpu).expect("shadow map readback");
        let matrices = renderer.shadow_cascade_matrices();
        for (cascade, matrix) in matrices.iter().enumerate().take(count as usize) {
            let matched = plane_texels_matching(&depths, cascade, *matrix);
            assert!(
                matched > 20,
                "{count} cascades: layer {cascade} holds the plane at its own matrix's depth \
                 in only {matched} probed texels"
            );
        }
    }
}

/// Probes on the plane, away from the cube, whose texel in `cascade`'s layer holds their own depth.
fn plane_texels_matching(depths: &[f32], cascade: usize, matrix: glam::Mat4) -> usize {
    let size = 2048usize;
    let mut matched = 0;
    for step in 0..=60 {
        let x = -3.8 + 7.6 * step as f32 / 60.0;
        for z in [-3.6, -3.0, 3.0, 3.6] {
            let ndc = matrix.project_point3(Vec3::new(x, 0.0, z));
            let (u, v) = (ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) || !(0.0..1.0).contains(&ndc.z)
            {
                continue;
            }
            let texel = (v * size as f32) as usize * size + (u * size as f32) as usize;
            if (depths[cascade * size * size + texel] - ndc.z).abs() < 2e-3 {
                matched += 1;
            }
        }
    }
    matched
}

#[test]
fn strength_scales_the_shadow_and_the_switch_takes_it_and_its_casters_away() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, &cube_on_plane());
    let lit = unshadowed(&gpu, &mut renderer, &camera());
    assert_eq!(
        renderer.shadow_caster_stats(),
        (0, 0),
        "no pass, no casters"
    );
    let mut mean_in_full_shadow = Vec::new();
    renderer.shadows.strength = 1.0;
    let full = darkening(&lit, &render(&gpu, &mut renderer, &camera()));
    let (drawn, considered) = renderer.shadow_caster_stats();
    assert!(
        considered > 0 && drawn <= considered,
        "{drawn} / {considered}"
    );
    let umbra: Vec<usize> = (0..full.len()).filter(|i| full[*i] > SHADOWED).collect();
    assert!(umbra.len() > 500, "{} shadowed pixels", umbra.len());
    for strength in [0.0, 0.65, 1.0] {
        renderer.shadows.strength = strength;
        let mask = darkening(&lit, &render(&gpu, &mut renderer, &camera()));
        mean_in_full_shadow.push(umbra.iter().map(|i| mask[*i]).sum::<f32>() / umbra.len() as f32);
    }
    eprintln!("mean darkening at strength 0, 0.65, 1: {mean_in_full_shadow:?}");
    assert!(
        mean_in_full_shadow[0].abs() < 0.5,
        "{mean_in_full_shadow:?}"
    );
    assert!(
        mean_in_full_shadow[0] < mean_in_full_shadow[1]
            && mean_in_full_shadow[1] < mean_in_full_shadow[2],
        "{mean_in_full_shadow:?}"
    );
}

/// An up-facing grey quad at height `y` over `[x0, x1] x [z0, z1]`.
fn quad(y: f32, (x0, x1): (f32, f32), (z0, z1): (f32, f32)) -> CpuPrimitive {
    let vertex = |x: f32, z: f32| Vertex {
        position: [x, y, z],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0; 4],
        uv1: [0.0, 0.0],
    };
    CpuPrimitive {
        vertices: vec![
            vertex(x0, z0),
            vertex(x1, z0),
            vertex(x1, z1),
            vertex(x0, z1),
        ],
        indices: vec![0, 2, 1, 0, 3, 2],
        transform: glam::Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.9,
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

#[test]
fn a_card_50_m_away_on_an_8_km_ground_still_casts_its_shadow() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let ground = quad(0.0, (-4000.0, 4000.0), (-4000.0, 4000.0));
    let card = quad(1.5, (-3.0, 3.0), (-43.0, -37.0));
    let camera = OrbitCamera {
        target: Vec3::new(0.0, 0.0, -40.0),
        radius: 50.0,
        yaw_deg: 90.0,
        pitch_deg: 30.0,
        far: 9000.0,
        ..OrbitCamera::default()
    };
    let shadowed = |primitives: Vec<CpuPrimitive>| {
        let scene = CpuScene {
            primitives,
            ..CpuScene::default()
        };
        let mut renderer = renderer(&gpu, &scene);
        renderer.light_dir_ambient = Vec4::new(1.0, 0.5, 0.0, 0.15);
        render(&gpu, &mut renderer, &camera)
    };
    let bare = shadowed(vec![ground.clone()]);
    let with_card = shadowed(vec![ground, card]);
    let count = shadowed_pixels(&darkening(&bare, &with_card));
    // Fitted to the scene, the cascades spread metre-sized texels and a bias of metres over this ground.
    assert!(count > 150, "the card shadowed only {count} ground pixels");
}

/// Pixels the shadows darken by `levels` or more, per PCF radius 0, 2, 3 and 5.
fn darkened_per_radius(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    camera: &OrbitCamera,
    levels: f32,
) -> Vec<usize> {
    let lit = unshadowed(gpu, renderer, camera);
    [0, 2, 3, 5]
        .iter()
        .map(|&radius| {
            renderer.shadows.pcf_radius = radius;
            let shadowed = render(gpu, renderer, camera);
            darkening(&lit, &shadowed)
                .iter()
                .filter(|d| **d >= levels)
                .count()
        })
        .collect()
}

/// C++ `DISABLED_PcfRadiusSweepMeasuresTheShadowRig`'s far-lit column: a receiver sloped against the light, no occluder.
#[test]
fn a_sloped_receiver_stays_lit_at_every_pcf_radius_and_shadows_still_darken() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let ground = quad(0.0, (-12.0, 12.0), (-12.0, 12.0));
    // High enough that its shadow clears it under every sun below.
    let card = quad(2.0, (-0.75, 0.75), (-0.75, 0.75));
    let camera = OrbitCamera {
        target: Vec3::new(-2.0, 0.0, 0.0),
        radius: 16.0,
        pitch_deg: 60.0,
        ..OrbitCamera::default()
    };
    // Sun elevations of about 15, 30 and 50 degrees: the lower, the steeper the ground's slope in the map.
    let suns = [
        Vec4::new(1.0, 0.27, 0.0, 0.15),
        Vec4::new(1.0, 0.6, 0.3, 0.15),
        Vec4::new(0.6, 1.0, 0.5, 0.15),
    ];
    let scene = |primitives| CpuScene {
        primitives,
        ..CpuScene::default()
    };
    let mut failures = Vec::new();
    for sun in suns {
        let mut bare = renderer(&gpu, &scene(vec![ground.clone()]));
        bare.light_dir_ambient = sun;
        let self_shadowed = darkened_per_radius(&gpu, &mut bare, &camera, 3.0);
        let mut occluded = renderer(&gpu, &scene(vec![ground.clone(), card.clone()]));
        occluded.light_dir_ambient = sun;
        let shadowed = darkened_per_radius(&gpu, &mut occluded, &camera, SHADOWED);
        eprintln!(
            "sun {sun}: lone ground darkened 3+ levels at radius 0, 2, 3, 5: {self_shadowed:?}; \
             card shadow pixels: {shadowed:?}"
        );
        if self_shadowed.iter().any(|count| *count > 0) {
            failures.push(format!(
                "the outer taps shadow the receiver itself under sun {sun}: {self_shadowed:?}"
            ));
        }
        if shadowed.iter().any(|count| *count <= 150) {
            failures.push(format!(
                "the card's shadow went under sun {sun}: {shadowed:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_pcf_radius_bound_matches_the_slang_constant() {
    use kataglyphis_webgpu_renderer::render::shadow_settings::MAX_PCF_RADIUS;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shaders/forward/forward_lighting.slang");
    let source = std::fs::read_to_string(&path).expect("forward_lighting.slang is owned here");
    let needle = "static const int MAX_PCF_RADIUS = ";
    let start = source
        .find(needle)
        .expect("forward_lighting.slang declares MAX_PCF_RADIUS")
        + needle.len();
    let value: u32 = source[start..start + source[start..].find(';').expect("a semicolon")]
        .trim()
        .parse()
        .expect("an integer");
    assert_eq!(
        value, MAX_PCF_RADIUS,
        "the shader clamps the radius the settings allow"
    );
}
