//! Headless golden tests asserting structural pixel properties, which survive driver differences.

use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf")
}

fn textured_cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_textured.gltf")
}

#[test]
fn gltf_loader_reads_cube() {
    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    assert_eq!(scene.primitives.len(), 1);
    assert_eq!(scene.vertex_count(), 24);
    assert_eq!(scene.triangle_count(), 12);

    let material = &scene.primitives[0].material;
    assert!((material.base_color[0] - 0.8).abs() < 1e-6);
    assert!((material.base_color[3] - 1.0).abs() < 1e-6);
    assert!(material.base_color_texture.is_none());
}

#[test]
fn gltf_loader_reads_base_color_texture() {
    let scene = load_gltf(textured_cube_path()).expect("cube_textured.gltf must load");
    let material = &scene.primitives[0].material;

    let texture_ref = material
        .base_color_texture
        .as_ref()
        .expect("textured cube must expose its base color texture");
    let texture = &texture_ref.texture;
    assert_eq!((texture.width, texture.height), (2, 2));
    assert_eq!(texture.rgba8.len(), 16);
    // 2x2 checker: green at (0,0), magenta at (1,0).
    assert_eq!(&texture.rgba8[0..4], &[40, 220, 60, 255]);
    assert_eq!(&texture.rgba8[4..8], &[220, 40, 200, 255]);
    // The asset requests NEAREST filtering (A1: sampler modes honored).
    assert!(texture_ref.sampler.mag_nearest && texture_ref.sampler.min_nearest);
    assert!(texture_ref.srgb);
    // KHR_texture_transform: offset (0.25, 0), scale (2, 2), no rotation.
    let t = material.base_uv_transform;
    assert!((t[0][0] - 2.0).abs() < 1e-5 && (t[1][1] - 2.0).abs() < 1e-5);
    assert!((t[0][2] - 0.25).abs() < 1e-5 && t[1][2].abs() < 1e-5);
    assert!(t[0][1].abs() < 1e-5 && t[1][0].abs() < 1e-5);
}

#[test]
fn gltf_loader_applies_emissive_strength() {
    // emissiveFactor [0.5, 0.4, 0.3] times emissive_strength 3.0: HDR emitters exceed [0,1].
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/cube_emissive_strength.gltf");
    let scene = load_gltf(path).expect("cube_emissive_strength.gltf must load");
    let e = scene.primitives[0].material.emissive_factor;
    assert!((e[0] - 1.5).abs() < 1e-5, "r: {}", e[0]);
    assert!((e[1] - 1.2).abs() < 1e-5, "g: {}", e[1]);
    assert!((e[2] - 0.9).abs() < 1e-5, "b: {}", e[2]);
}

#[test]
fn gltf_loader_reads_morph_target_and_default_weight() {
    // One +Y morph target with a mesh default weight of 1.0, which must not load as zero.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_morph.gltf");
    let scene = load_gltf(path).expect("cube_morph.gltf must load");
    let prim = &scene.primitives[0];
    assert_eq!(prim.morph_targets.len(), 1, "one morph target expected");
    assert_eq!(
        prim.morph_targets[0].position_deltas.len(),
        24,
        "delta count must match the cube's vertices"
    );
    // Every delta is (0, 1, 0).
    for d in &prim.morph_targets[0].position_deltas {
        assert!(d.x.abs() < 1e-6 && (d.y - 1.0).abs() < 1e-6 && d.z.abs() < 1e-6);
    }
    // The mesh default weight must be honored.
    assert_eq!(
        prim.morph_weights,
        vec![1.0],
        "mesh default weight [1.0] must be applied at load"
    );
}

#[test]
fn renders_cube_headless() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);

    let camera = OrbitCamera::default();
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");
    assert_eq!(pixels.len(), (width * height * 4) as usize);

    let pixel = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };

    // The target is Rgba8UnormSrgb, so read-back bytes are sRGB-encoded.

    // A neighbourhood, not the centre pixel: at 45 degrees yaw it grazes a face seam and may be dark.
    let center_neighbourhood_is_red = (width / 2 - 4..=width / 2 + 4)
        .flat_map(|x| (height / 2 - 4..=height / 2 + 4).map(move |y| (x, y)))
        .map(|(x, y)| pixel(x, y))
        .any(|p| p[0] > 110 && p[0] > p[1] + 40 && p[0] > p[2] + 40);
    assert!(
        center_neighbourhood_is_red,
        "no red cube pixel found near the frame centre, got {:?}",
        pixel(width / 2, height / 2)
    );

    // Corner: procedural sky (blue-dominant gradient), untouched by the cube.
    let corner = pixel(2, 2);
    assert!(
        corner[2] > corner[0] && corner[2] > 60,
        "corner pixel should be sky (blue-dominant), got {corner:?}"
    );

    // The cube must cover a plausible portion of the frame: count lit pixels.
    let lit = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 110 && p[0] > p[1] + 40)
        .count();
    let total = (width * height) as usize;
    assert!(
        lit > total / 20 && lit < total / 2,
        "cube coverage out of range: {lit}/{total} lit pixels"
    );
}

#[test]
fn renders_textured_cube_headless() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(textured_cube_path()).expect("cube_textured.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);

    let camera = OrbitCamera::default();
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");

    // Both checker colours must appear, proving the texture is sampled.
    let mut green = 0usize;
    let mut magenta = 0usize;
    for p in pixels.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        if g > 100 && g > r + 30 && g > b + 30 {
            green += 1;
        } else if r > 100 && b > 80 && r > g + 30 {
            magenta += 1;
        }
    }
    assert!(
        green > 200 && magenta > 200,
        "expected both checker colors, got {green} green / {magenta} magenta pixels"
    );
}

/// A cube above the camera's aim point must land in the upper half of the frame, and below it in the lower half.
#[test]
fn world_up_is_screen_up() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);

    let mut red_centroid_y = |aim_y: f32| -> f64 {
        let camera = OrbitCamera {
            target: glam::Vec3::new(0.0, aim_y, 0.0),
            ..OrbitCamera::default()
        };
        let pixels = renderer
            .render_to_pixels(&gpu, width, height, &camera)
            .expect("headless render must succeed");
        let rows: Vec<u32> = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[0] > 110 && p[0] as i32 > p[1] as i32 + 40)
            .map(|(i, _)| i as u32 / width)
            .collect();
        assert!(rows.len() > 200, "cube not found at aim_y {aim_y}");
        rows.iter().map(|&y| f64::from(y)).sum::<f64>() / rows.len() as f64
    };

    // Rows grow downward: aiming below the cube must put it above the centre line.
    let (above, below) = (red_centroid_y(-1.2), red_centroid_y(1.2));
    eprintln!("orientation golden: centroid_y aimed below {above:.1}, aimed above {below:.1}");
    let half = f64::from(height) / 2.0;
    assert!(
        above < half * 0.6,
        "a cube above the aim point must be in the upper half, centroid_y {above:.1}"
    );
    assert!(
        below > half * 1.4,
        "a cube below the aim point must be in the lower half, centroid_y {below:.1}"
    );
}

/// A morph weight ramping 0 -> 1 must re-blend, re-upload and visibly lift the cube.
#[test]
fn morph_weight_lifts_the_silhouette() {
    use glam::{Quat, Vec3};
    use kataglyphis_webgpu_renderer::scene::{
        ChannelValues, CpuAnimation, CpuAnimationChannel, CpuNode, Interpolation, MorphTarget,
    };

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // One +Y morph target driven by a linear weight channel, 0 at t=0 and 1 at t=1.
    let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut prim = scene.primitives[0].clone();
    let vcount = prim.vertices.len();
    prim.node_index = Some(0);
    prim.morph_targets = vec![MorphTarget {
        position_deltas: vec![Vec3::new(0.0, 0.6, 0.0); vcount],
        normal_deltas: vec![Vec3::ZERO; vcount],
        tangent_deltas: vec![Vec3::ZERO; vcount],
    }];
    prim.morph_weights = vec![0.0];
    scene.primitives = vec![prim];
    scene.nodes = vec![CpuNode {
        parent: None,
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    }];
    scene.animations = vec![CpuAnimation {
        // Longer than the last keyframe, so t=1.0 lands at full weight instead of wrapping.
        name: "morph".into(),
        duration: 2.0,
        channels: vec![CpuAnimationChannel {
            node: 0,
            times: vec![0.0, 1.0],
            values: ChannelValues::MorphWeights(vec![0.0, 1.0]),
            interpolation: Interpolation::Linear,
        }],
    }];

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    let camera = OrbitCamera::default();

    // Count and mean row of the red cube pixels; rows grow downward, so lifting lowers it.
    let cube_stats = |pixels: &[u8]| -> (usize, f64) {
        let mut count = 0usize;
        let mut sum_y = 0f64;
        for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
            if r > 110 && r > g + 40 && r > b + 40 {
                count += 1;
                sum_y += (i as u32 / width) as f64;
            }
        }
        (count, if count > 0 { sum_y / count as f64 } else { 0.0 })
    };

    renderer.set_animation_time(0.0);
    let base = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render at weight 0");
    let (lit0, cy0) = cube_stats(&base);

    renderer.set_animation_time(1.0);
    let morphed = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render at weight 1");
    let (lit1, cy1) = cube_stats(&morphed);

    eprintln!("morph golden: lit {lit0}->{lit1}, centroid_y {cy0:.1}->{cy1:.1}");

    // The cube stays clearly visible in both poses...
    let total = (width * height) as usize;
    assert!(
        lit0 > total / 40 && lit1 > total / 40,
        "cube must be visible at both weights, got {lit0}/{lit1} lit"
    );
    // ...the frame actually changes (the re-blend + re-upload happened)...
    assert!(
        base != morphed,
        "weight 1 must produce a different frame than weight 0"
    );
    // ...and the +Y morph lifts the silhouette (centroid rises, i.e. smaller row).
    assert!(
        cy1 + 6.0 < cy0,
        "the +Y morph should raise the cube: centroid_y {cy0:.1} -> {cy1:.1}"
    );
}

#[test]
fn shadow_darkens_plane_under_cube() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let scene = load_gltf(path).expect("cube_on_plane.gltf must load");
    assert_eq!(scene.primitives.len(), 2);

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    // Low light, so the shadow falls where the camera looks rather than under the cube.
    renderer.light_dir_ambient = glam::Vec4::new(-1.0, 0.7, -0.3, 0.15);

    // Look down from above so the plane fills most of the frame.
    let camera = OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");

    // Sunlit plane pixels are near-neutral; the shadowed patch gets only ambient. Both must exist.
    let mut lit_plane = 0usize;
    let mut shadowed_plane = 0usize;
    for p in pixels.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        let neutral = (r - g).abs() < 25 && (g - b).abs() < 25 && (r - b).abs() < 25;
        if neutral && r > 180 {
            lit_plane += 1;
        } else if r < 110 && b > r + 15 && b < 180 {
            // With analytic IBL the shadow receives only blue hemisphere irradiance.
            shadowed_plane += 1;
        }
    }
    assert!(
        lit_plane > 1000,
        "expected a large sunlit plane area, got {lit_plane} pixels"
    );
    assert!(
        shadowed_plane > 150,
        "expected a shadowed patch under the cube, got {shadowed_plane} pixels"
    );
}

/// A fresh renderer's very first frame must already cast shadows, at two tile-grid sizes.
#[test]
fn first_frame_uses_the_correct_cascade_and_tile_counts() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let scene = load_gltf(path).expect("cube_on_plane.gltf must load");

    let camera = OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    };

    let first_frame_shadowed_pixels = |width: u32, height: u32| -> usize {
        let mut renderer = ForwardRenderer::new(&gpu, width, height);
        renderer.upload_scene(&gpu, &scene);
        renderer.light_dir_ambient = glam::Vec4::new(-1.0, 0.7, -0.3, 0.15);

        let pixels = renderer
            .render_to_pixels(&gpu, width, height, &camera)
            .expect("headless render must succeed");

        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                let (r, b) = (p[0] as i32, p[2] as i32);
                r < 110 && b > r + 15 && b < 180
            })
            .count()
    };

    let shadowed_256 = first_frame_shadowed_pixels(256, 256);
    assert!(
        shadowed_256 > 150,
        "256x256 first frame: expected a shadowed patch under the cube, got {shadowed_256} pixels"
    );

    let shadowed_512 = first_frame_shadowed_pixels(512, 512);
    assert!(
        shadowed_512 > 150,
        "512x512 first frame: expected a shadowed patch under the cube, got {shadowed_512} pixels"
    );
}

/// The superproject's `forward.slang`, or `None` (a skip) in a checkout of this submodule alone.
fn slang_source() -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../Resources/ShadersSlang/forward/forward.slang");
    match std::fs::read_to_string(&path) {
        Ok(source) => Some(source),
        Err(err) => {
            eprintln!("SKIP: could not read {}: {err}", path.display());
            None
        }
    }
}

/// The value of `static const <ty> <name> = <value>;` in `source`, parsed as `T`.
fn parse_slang_constant<T: std::str::FromStr>(source: &str, name: &str) -> Option<T> {
    let needle = format!(" {name} = ");
    let start = source.find(&needle)? + needle.len();
    let end = start + source[start..].find(';')?;
    source[start..end].trim().parse().ok()
}

/// Pins Rust `CASCADE_COUNT` against `forward.slang`'s; no GPU needed.
#[test]
fn cascade_count_matches_the_slang_constant() {
    assert_eq!(
        kataglyphis_webgpu_renderer::render::forward::CASCADE_COUNT,
        3
    );

    let Some(source) = slang_source() else {
        return;
    };
    let shader_cascade_count: usize = parse_slang_constant(&source, "CASCADE_COUNT")
        .expect("forward.slang must declare `static const int CASCADE_COUNT = <n>;`");
    assert_eq!(
        kataglyphis_webgpu_renderer::render::forward::CASCADE_COUNT,
        shader_cascade_count,
        "Rust CASCADE_COUNT and forward.slang's CASCADE_COUNT have drifted apart"
    );
}

/// Pins `render::tile_grid::TILE_SIZE` against `forward.slang`'s; no GPU needed.
#[test]
fn tile_size_matches_the_slang_constant() {
    let Some(source) = slang_source() else {
        return;
    };
    let shader_tile_size: u32 = parse_slang_constant(&source, "TILE_SIZE")
        .expect("forward.slang must declare `static const uint TILE_SIZE = <n>;`");
    assert_eq!(
        kataglyphis_webgpu_renderer::render::tile_grid::TILE_SIZE,
        shader_tile_size,
        "Rust TILE_SIZE and forward.slang's TILE_SIZE have drifted apart"
    );
}

#[test]
fn alpha_modes_blend_and_mask() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_alpha.gltf");
    let scene = load_gltf(&path).expect("cube_alpha.gltf must load");
    assert_eq!(scene.primitives.len(), 4);

    use kataglyphis_webgpu_renderer::scene::AlphaMode;
    let modes: Vec<AlphaMode> = scene
        .primitives
        .iter()
        .map(|p| p.material.alpha_mode)
        .collect();
    assert!(modes.contains(&AlphaMode::Blend));
    assert!(modes
        .iter()
        .any(|m| matches!(m, AlphaMode::Mask(c) if (c - 0.5).abs() < 1e-6)));

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);

    // Look down so the translucent green quad overlaps the red cube.
    let camera = OrbitCamera {
        radius: 5.0,
        pitch_deg: 60.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");

    let mut blended_over_cube = 0usize;
    let mut yellowish = 0usize;
    for p in pixels.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        // Green blend quad over the white plane: tinted, with red/blue still showing through.
        if g > 140 && g > r + 25 && g > b + 25 && r > 70 && b > 60 {
            blended_over_cube += 1;
        }
        // The below-cutoff yellow MASK quad must vanish; only strong yellow counts, not olive mixes.
        if r > 140 && g > 140 && b * 3 < r {
            yellowish += 1;
        }
    }
    assert!(
        blended_over_cube > 200,
        "expected the green BLEND quad composited over the red cube, got {blended_over_cube} pixels"
    );
    assert_eq!(
        yellowish, 0,
        "MASK quad below cutoff must be discarded entirely, got {yellowish} yellow pixels"
    );
}

#[test]
fn punctual_lights_pool_on_plane() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/point_light.gltf");
    let scene = load_gltf(&path).expect("point_light.gltf must load");
    assert_eq!(scene.lights.len(), 2);
    use kataglyphis_webgpu_renderer::scene::CpuLightKind;
    assert!(scene
        .lights
        .iter()
        .any(|l| matches!(l.kind, CpuLightKind::Point)));
    assert!(scene
        .lights
        .iter()
        .any(|l| matches!(l.kind, CpuLightKind::Spot { .. })));

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    // Dim the sun so the punctual pools dominate.
    renderer.light_color_intensity.w = 0.4;

    let camera = OrbitCamera {
        radius: 7.0,
        pitch_deg: 65.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");

    let mut red_pool = 0usize;
    let mut green_pool = 0usize;
    for p in pixels.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        if r > 120 && r > g + 40 && r > b + 40 {
            red_pool += 1;
        } else if g > 120 && g > r + 40 && g > b + 40 {
            green_pool += 1;
        }
    }
    assert!(
        red_pool > 200,
        "expected a red point-light pool on the plane, got {red_pool} pixels"
    );
    assert!(
        green_pool > 100,
        "expected a green spot-light pool on the plane, got {green_pool} pixels"
    );
}

#[test]
fn bloom_adds_energy_around_bright_sources() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/point_light.gltf");
    let scene = load_gltf(&path).expect("point_light.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    renderer.light_color_intensity.w = 0.4;

    let camera = OrbitCamera {
        radius: 7.0,
        pitch_deg: 65.0,
        ..OrbitCamera::default()
    };

    let total = |pixels: &[u8]| -> u64 { pixels.iter().map(|&b| b as u64).sum() };

    renderer.bloom_strength = 0.0;
    let without = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render without bloom");
    renderer.bloom_strength = 1.5;
    let with = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render with bloom");

    let (sum_without, sum_with) = (total(&without), total(&with));
    assert!(
        sum_with > sum_without + 50_000,
        "bloom should add visible energy: {sum_without} -> {sum_with}"
    );
}

#[test]
fn ssao_darkens_geometry() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_alpha.gltf");
    let scene = load_gltf(&path).expect("cube_alpha.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    renderer.bloom_strength = 0.0;

    let camera = OrbitCamera {
        radius: 5.0,
        pitch_deg: 60.0,
        ..OrbitCamera::default()
    };
    let total = |pixels: &[u8]| -> u64 { pixels.iter().map(|&b| b as u64).sum() };

    renderer.ssao_strength = 0.0;
    let without = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render without ssao");
    renderer.ssao_strength = 1.0;
    let with = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render with ssao");

    let (sum_without, sum_with) = (total(&without), total(&with));
    assert!(
        sum_with + 50_000 < sum_without,
        "SSAO should remove energy near geometry: {sum_without} -> {sum_with}"
    );
}

/// A fronto-parallel face must read as unoccluded, which a flat `1 - strength` SSAO would fail.
#[test]
fn ssao_leaves_a_flat_surface_unoccluded() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_alpha.gltf");
    let scene = load_gltf(&path).expect("cube_alpha.gltf must load");
    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    renderer.bloom_strength = 0.0;

    // Head-on view of the cube's +Z face, filling most of the frame.
    let camera = OrbitCamera {
        target: glam::Vec3::new(0.0, 0.5, 0.0),
        radius: 2.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };

    renderer.ssao_strength = 0.0;
    let without = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render without ssao");
    renderer.ssao_strength = 1.0;
    let with = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render with ssao");

    // The central half stays inside the face, away from silhouette edges.
    let margin = width as usize / 4;
    let mut max_diff = 0i32;
    for y in margin..(height as usize - margin) {
        for x in margin..(width as usize - margin) {
            let idx = (y * width as usize + x) * 4;
            for c in 0..3 {
                let diff = (with[idx + c] as i32 - without[idx + c] as i32).abs();
                max_diff = max_diff.max(diff);
            }
        }
    }
    assert!(
        max_diff <= 6,
        "flat fronto-parallel surface should read as unoccluded at any ssao_strength, \
         max channel diff over the interior was {max_diff}"
    );
}

#[test]
fn animation_moves_the_cube() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_animated.gltf");
    let scene = load_gltf(&path).expect("cube_animated.gltf must load");
    assert_eq!(scene.animations.len(), 1);
    assert!((scene.animations[0].duration - 2.0).abs() < 1e-5);

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    let camera = OrbitCamera {
        radius: 6.0,
        pitch_deg: 20.0,
        yaw_deg: 90.0, // look along -z so x maps to screen x
        ..OrbitCamera::default()
    };

    let red_centroid_x = |pixels: &[u8]| -> f32 {
        let (mut sum_x, mut count) = (0.0f32, 0u32);
        for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
            if r > 110 && r > g + 40 && r > b + 40 {
                sum_x += (i % width as usize) as f32;
                count += 1;
            }
        }
        assert!(count > 200, "cube not found (only {count} red pixels)");
        sum_x / count as f32
    };

    renderer.set_animation_time(0.05);
    let start = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render at t=0");
    renderer.set_animation_time(1.95);
    let end = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render at t=1.95");

    let (x0, x1) = (red_centroid_x(&start), red_centroid_x(&end));
    assert!(
        (x1 - x0).abs() > 30.0,
        "animated cube should move across the frame: centroid {x0:.1} -> {x1:.1}"
    );
}

#[test]
fn skinning_bends_the_bar() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/skinned_bar.gltf");
    let scene = load_gltf(&path).expect("skinned_bar.gltf must load");
    assert_eq!(scene.skins.len(), 1);
    assert_eq!(scene.skins[0].joints.len(), 2);
    assert_eq!(scene.skins[0].inverse_bind_matrices.len(), 2);
    // Vertices must carry non-zero skin weights.
    assert!(scene.primitives[0]
        .vertices
        .iter()
        .any(|v| v.weights.iter().sum::<f32>() > 0.5));

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    let camera = OrbitCamera {
        radius: 5.0,
        pitch_deg: 5.0,
        yaw_deg: 90.0,
        target: glam::Vec3::new(0.0, 1.0, 0.0),
        ..OrbitCamera::default()
    };

    // Mean x of the bar's red pixels in the top half, which bending joint 1 swings sideways.
    let upper_centroid_x = |pixels: &[u8]| -> f32 {
        let (mut sum, mut count) = (0.0f32, 0u32);
        for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (i % width as usize, i / width as usize);
            if y > (height as usize) / 2 {
                continue;
            }
            let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
            if r > 90 && r > g + 30 && r > b + 30 {
                sum += x as f32;
                count += 1;
            }
        }
        assert!(count > 50, "bar not visible in upper half ({count} px)");
        sum / count as f32
    };

    renderer.set_animation_time(0.0);
    let straight = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render straight");
    renderer.set_animation_time(1.0);
    let bent = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render bent");

    let (x0, x1) = (upper_centroid_x(&straight), upper_centroid_x(&bent));
    assert!(
        (x1 - x0).abs() > 8.0,
        "skinned bar should bend: upper centroid {x0:.1} -> {x1:.1}"
    );
}

#[test]
fn loads_binary_glb() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.glb");
    let bytes = std::fs::read(&path).expect("cube.glb must exist");
    // GLB magic + version.
    assert_eq!(&bytes[0..4], b"glTF");

    let scene = kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf_slice(&bytes)
        .expect("cube.glb must load from memory");
    assert_eq!(scene.primitives.len(), 1);
    assert_eq!(scene.vertex_count(), 24);
    assert_eq!(scene.triangle_count(), 12);
}

#[test]
fn reads_gltf_cameras() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_animated.gltf");
    let scene = load_gltf(&path).expect("cube_animated.gltf must load");
    assert_eq!(scene.cameras.len(), 1);
    let camera = &scene.cameras[0];
    assert_eq!(camera.name.as_deref(), Some("demo_cam"));
    match camera.projection {
        kataglyphis_webgpu_renderer::scene::CpuCameraProjection::Perspective {
            yfov_rad,
            znear,
            zfar,
        } => {
            // The asset authors yfov as 0.7854 rad (45 deg).
            assert!((yfov_rad - std::f32::consts::FRAC_PI_4).abs() < 1e-3);
            assert!((znear - 0.1).abs() < 1e-6);
            assert_eq!(zfar, Some(100.0));
        }
        other => panic!("expected a perspective camera, got {other:?}"),
    }
    // Its node must exist and sit where the asset places it.
    let world = kataglyphis_webgpu_renderer::CpuScene::compute_world_transforms(&scene.nodes);
    let position = world[camera.node].transform_point3(glam::Vec3::ZERO);
    assert!((position - glam::Vec3::new(0.0, 1.0, 5.0)).length() < 1e-4);
}

#[test]
fn reads_orthographic_gltf_cameras() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/cube_ortho_camera.gltf");
    let scene = load_gltf(&path).expect("cube_ortho_camera.gltf must load");
    assert_eq!(scene.cameras.len(), 1);
    let camera = &scene.cameras[0];
    assert_eq!(camera.name.as_deref(), Some("demo_ortho_cam"));
    match camera.projection {
        kataglyphis_webgpu_renderer::scene::CpuCameraProjection::Orthographic {
            xmag,
            ymag,
            znear,
            zfar,
        } => {
            assert!((xmag - 2.5).abs() < 1e-6);
            assert!((ymag - 1.5).abs() < 1e-6);
            assert!((znear - 0.1).abs() < 1e-6);
            assert!((zfar - 100.0).abs() < 1e-6);
        }
        other => panic!("expected an orthographic camera, got {other:?}"),
    }
    let world = kataglyphis_webgpu_renderer::CpuScene::compute_world_transforms(&scene.nodes);
    let position = world[camera.node].transform_point3(glam::Vec3::ZERO);
    assert!((position - glam::Vec3::new(0.0, 1.0, 5.0)).length() < 1e-4);
}

#[test]
fn orthographic_projection_matrix_is_finite_and_maps_near_far_to_expected_depth() {
    use kataglyphis_webgpu_renderer::scene::CpuCameraProjection;

    let projection = CpuCameraProjection::Orthographic {
        xmag: 2.5,
        ymag: 1.5,
        znear: 0.1,
        zfar: 100.0,
    };
    let matrix = projection.matrix(1.777);
    assert!(matrix.to_cols_array().iter().all(|v| v.is_finite()));

    // WebGPU/wgpu clip space: near maps to NDC z=0, far maps to NDC z=1.
    let near_ndc = matrix.project_point3(glam::Vec3::new(0.0, 0.0, -0.1));
    let far_ndc = matrix.project_point3(glam::Vec3::new(0.0, 0.0, -100.0));
    assert!((near_ndc.z - 0.0).abs() < 1e-4);
    assert!((far_ndc.z - 1.0).abs() < 1e-4);
}

#[test]
fn resize_handles_zero_dimensions() {
    let Some(mut gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // No surface: resize must be a no-op, as the windowed path relies on when minimized.
    gpu.resize(0, 0);
    gpu.resize(800, 600);
    gpu.reconfigure();
}

/// A non-sRGB target (as WebGPU canvases are) must hold the same encoded bytes as an sRGB one.
#[test]
fn non_srgb_target_is_gamma_encoded_like_an_srgb_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (128, 128);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    let camera = OrbitCamera::default();

    let srgb = renderer
        .render_to_pixels_with_format(
            &gpu,
            width,
            height,
            &camera,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
        .expect("sRGB render must succeed");
    let unorm = renderer
        .render_to_pixels_with_format(
            &gpu,
            width,
            height,
            &camera,
            wgpu::TextureFormat::Rgba8Unorm,
        )
        .expect("non-sRGB render must succeed");

    assert_eq!(srgb.len(), unorm.len());

    let mean = |px: &[u8]| px.iter().map(|&b| b as f64).sum::<f64>() / px.len() as f64;
    let mean_srgb = mean(&srgb);
    let mean_unorm = mean(&unorm);

    // Same curve in hardware and shader, so only per-driver rounding separates them.
    assert!(
        (mean_srgb - mean_unorm).abs() < 2.0,
        "non-sRGB target should be gamma-encoded to match the sRGB one; \
         mean was {mean_srgb:.2} (sRGB) vs {mean_unorm:.2} (non-sRGB). \
         A much darker non-sRGB mean means the shader-side encode is not running."
    );

    // Guard against the assertion above being satisfied by a black frame.
    assert!(
        mean_srgb > 20.0,
        "reference render looks blank (mean {mean_srgb:.2}); the comparison above would prove nothing"
    );
}

/// Auto-exposure wiring end to end, where a disconnect would silently do nothing.
#[test]
fn auto_exposure_brightens_a_dark_scene_over_successive_frames() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (128, 128);

    let mean_of = |pixels: &[u8]| -> f64 {
        pixels.iter().map(|&b| b as f64).sum::<f64>() / pixels.len() as f64
    };

    // Underlit on purpose: manual exposure leaves it dark, auto should pull it up.
    let render = |auto: bool, frames: usize| -> f64 {
        let mut renderer = ForwardRenderer::new(&gpu, width, height);
        renderer.upload_scene(&gpu, &scene);
        renderer.light_dir_ambient = glam::Vec4::new(-0.4, 1.0, 0.3, 0.01);
        renderer.light_color_intensity = glam::Vec4::new(1.0, 1.0, 1.0, 0.05);
        renderer.auto_exposure = auto;
        renderer.exposure_ev = 0.0;
        // Large steps converge in a few frames; this tests the wiring, not the rate.
        renderer.frame_delta_seconds = 0.5;

        let camera = OrbitCamera::default();
        let mut pixels = Vec::new();
        for _ in 0..frames {
            pixels = renderer
                .render_to_pixels(&gpu, width, height, &camera)
                .expect("headless render must succeed");
        }
        mean_of(&pixels)
    };

    let manual = render(false, 6);
    let automatic = render(true, 6);

    assert!(
        manual > 1.0,
        "the reference render is essentially black ({manual}); the comparison below would prove nothing"
    );
    // A modest bound: the already well-exposed sky fills much of the frame.
    assert!(
        automatic > manual * 1.08,
        "auto-exposure did not brighten an underlit scene: mean {automatic} with auto vs {manual} manual"
    );
}

/// Manual exposure shares the auto path's GPU buffer and must still reach the pixels.
#[test]
fn manual_exposure_still_controls_brightness() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (128, 128);

    let render_at = |ev: f32| -> f64 {
        let mut renderer = ForwardRenderer::new(&gpu, width, height);
        renderer.upload_scene(&gpu, &scene);
        renderer.auto_exposure = false;
        renderer.exposure_ev = ev;
        let camera = OrbitCamera::default();
        let pixels = renderer
            .render_to_pixels(&gpu, width, height, &camera)
            .expect("headless render must succeed");
        pixels.iter().map(|&b| b as f64).sum::<f64>() / pixels.len() as f64
    };

    let dark = render_at(-3.0);
    let bright = render_at(2.0);

    assert!(
        bright > dark * 1.2,
        "manual exposure stopped affecting the image: EV -3 gave {dark}, EV +2 gave {bright}"
    );
}

/// Instances must land at their own transforms, not stack invisibly on the original.
#[test]
fn instances_appear_at_their_own_transforms() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (192, 192);
    let camera = OrbitCamera {
        radius: 14.0,
        ..OrbitCamera::default()
    };

    // Non-sky pixels: the cube is red-dominant, the sky blue-dominant.
    let covered = |pixels: &[u8]| -> usize {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] as i32 > p[2] as i32 + 20)
            .count()
    };

    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    assert_eq!(
        renderer.instance_count(0),
        1,
        "primitives start with one identity instance"
    );

    let single = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render");
    let single_covered = covered(&single);
    assert!(
        single_covered > 100,
        "the reference cube barely rendered ({single_covered} px)"
    );

    // Three copies, spread far enough apart not to overlap on screen.
    renderer.set_instances(
        &gpu,
        0,
        &[
            glam::Mat4::from_translation(glam::Vec3::new(-4.0, 0.0, 0.0)),
            glam::Mat4::IDENTITY,
            glam::Mat4::from_translation(glam::Vec3::new(4.0, 0.0, 0.0)),
        ],
    );
    assert_eq!(renderer.instance_count(0), 3);

    let instanced = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render");
    let instanced_covered = covered(&instanced);

    assert!(
        instanced_covered > single_covered * 2,
        "three separated instances should cover far more than one cube: {instanced_covered} vs {single_covered}"
    );
}

#[test]
fn clearing_instances_restores_a_single_copy_rather_than_none() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let (width, height) = (128, 128);
    let camera = OrbitCamera::default();

    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);

    renderer.set_instances(&gpu, 0, &[glam::Mat4::IDENTITY, glam::Mat4::IDENTITY]);
    assert_eq!(renderer.instance_count(0), 2);

    // Zero instances would look exactly like a culling or upload bug.
    renderer.set_instances(&gpu, 0, &[]);
    assert_eq!(
        renderer.instance_count(0),
        1,
        "an empty slice must restore one identity instance"
    );

    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render");
    let lit = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] as i32 > p[2] as i32 + 20)
        .count();
    assert!(
        lit > 100,
        "the cube disappeared after clearing instances ({lit} px)"
    );
}

#[test]
fn growing_the_instance_count_reallocates_correctly() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 96, 96);
    renderer.upload_scene(&gpu, &scene);

    // The initial buffer holds one instance, so this exercises the grow path.
    for count in [1usize, 4, 2, 16] {
        let transforms: Vec<glam::Mat4> = (0..count)
            .map(|i| glam::Mat4::from_translation(glam::Vec3::new(i as f32, 0.0, 0.0)))
            .collect();
        renderer.set_instances(&gpu, 0, &transforms);
        assert_eq!(renderer.instance_count(0), count as u32);

        renderer
            .render_to_pixels(&gpu, 96, 96, &OrbitCamera::default())
            .expect("render must succeed at every instance count");
    }
}

/// Shadows survive caster culling; `drawn < considered` is unchecked while one cached bundle
/// serves every cascade, which leaves per-cascade culling off.
#[test]
fn caster_culling_engages_and_shadows_survive() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let mut scene = load_gltf(path).expect("cube_on_plane.gltf must load");

    // A third primitive far outside every cascade's fitted box.
    let mut far_cube = scene.primitives[0].clone();
    far_cube.transform = glam::Mat4::from_translation(glam::Vec3::new(500.0, 0.0, 500.0));
    scene.primitives.push(far_cube);

    let (width, height) = (256, 256);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    renderer.light_dir_ambient = glam::Vec4::new(-1.0, 0.7, -0.3, 0.15);

    let camera = OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("headless render must succeed");

    let (_drawn, considered) = renderer.shadow_caster_stats();
    assert!(considered > 0, "no casters considered - did the pass run?");

    // As in shadow_darkens_plane_under_cube: the shadow is blue-tinted, never neutral dark.
    let mut lit_plane = 0usize;
    let mut shadowed_plane = 0usize;
    for p in pixels.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        let neutral = (r - g).abs() < 25 && (g - b).abs() < 25 && (r - b).abs() < 25;
        if neutral && r > 180 {
            lit_plane += 1;
        } else if r < 110 && b > r + 15 && b < 180 {
            shadowed_plane += 1;
        }
    }
    assert!(lit_plane > 500, "lit plane vanished: {lit_plane}");
    assert!(
        shadowed_plane > 50,
        "the cube's shadow vanished - culling ate a visible caster ({shadowed_plane} shadowed px)"
    );
}

/// The shadow-caster bundle survives `set_animation_time`, which only rewrites buffer contents.
#[test]
fn shadow_caster_bundle_is_cached_across_frames() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_animated.gltf");
    let scene = load_gltf(&path).expect("cube_animated.gltf must load");

    let (width, height) = (64, 64);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.upload_scene(&gpu, &scene);
    assert!(
        !renderer.shadow_caster_bundle_is_cached(),
        "upload_scene must invalidate any previously cached bundle"
    );

    renderer
        .render_to_pixels(&gpu, width, height, &OrbitCamera::default())
        .expect("first render must succeed");
    assert!(
        renderer.shadow_caster_bundle_is_cached(),
        "the first frame after upload_scene must record and cache the bundle"
    );

    renderer.set_animation_time(1.0);
    renderer
        .render_to_pixels(&gpu, width, height, &OrbitCamera::default())
        .expect("render after set_animation_time must succeed");
    assert!(
        renderer.shadow_caster_bundle_is_cached(),
        "set_animation_time only rewrites buffer contents, not identity - \
         the cached bundle must survive it"
    );
}

#[test]
fn gltf_loader_reads_unlit_flag() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_unlit.gltf");
    let scene = load_gltf(path).expect("cube_unlit.gltf must load");
    assert!(
        scene.primitives[0].material.unlit,
        "KHR_materials_unlit must be parsed"
    );
    // The plain cube must NOT be marked unlit, or the flag is meaningless.
    let lit = load_gltf(cube_path()).expect("cube.gltf must load");
    assert!(!lit.primitives[0].material.unlit);
}

/// An unlit material is exactly base_color, so moving the light changes none of its pixels.
#[test]
fn unlit_material_ignores_the_light() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (w, h) = (128, 128);
    let camera = OrbitCamera::default();

    let render_both_lights = |path: std::path::PathBuf| -> (Vec<u8>, Vec<u8>) {
        let scene = load_gltf(path).expect("scene must load");
        let mut r = ForwardRenderer::new(&gpu, w, h);
        r.upload_scene(&gpu, &scene);
        r.light_dir_ambient = glam::Vec4::new(1.0, 1.0, 0.5, 0.05);
        let a = r.render_to_pixels(&gpu, w, h, &camera).expect("render a");
        // Swing the light to the opposite side and drop ambient.
        r.light_dir_ambient = glam::Vec4::new(-1.0, -1.0, -0.5, 0.0);
        let b = r.render_to_pixels(&gpu, w, h, &camera).expect("render b");
        (a, b)
    };

    // Only the cube's pixels: the procedural sky follows the sun.
    let centre_window = |px: &[u8]| -> Vec<u8> {
        let mut out = Vec::new();
        for y in (h / 2 - 12)..(h / 2 + 12) {
            for x in (w / 2 - 12)..(w / 2 + 12) {
                let i = ((y * w + x) * 4) as usize;
                out.extend_from_slice(&px[i..i + 4]);
            }
        }
        out
    };

    let unlit_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_unlit.gltf");
    let (ua, ub) = render_both_lights(unlit_path);
    assert_eq!(
        centre_window(&ua),
        centre_window(&ub),
        "an unlit material must be identical under any lighting"
    );

    let (la, lb) = render_both_lights(cube_path());
    assert_ne!(
        centre_window(&la),
        centre_window(&lb),
        "control: a lit material must change with the light"
    );
}

/// A shared image uploads once, not once per referencing primitive.
#[test]
fn a_shared_texture_uploads_once() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(textured_cube_path()).expect("cube_textured.gltf must load");
    assert!(
        scene.primitives[0].material.base_color_texture.is_some(),
        "fixture must have a texture to share"
    );

    // Two primitives referencing the SAME Arc<CpuTexture>.
    let mut shared = scene.clone();
    let second = shared.primitives[0].clone();
    shared.primitives.push(second);

    let mut r = ForwardRenderer::new(&gpu, 64, 64);
    r.upload_scene(&gpu, &shared);
    let shared_uploads = r.uploaded_texture_count();

    // The single-primitive baseline uploads the same distinct images.
    let mut r1 = ForwardRenderer::new(&gpu, 64, 64);
    r1.upload_scene(&gpu, &scene);
    let single_uploads = r1.uploaded_texture_count();

    assert_eq!(
        shared_uploads, single_uploads,
        "duplicating a primitive must not upload its textures again \
         ({shared_uploads} vs {single_uploads})"
    );
    assert!(single_uploads > 0, "the fixture should upload something");

    // And the frame must still render correctly with the cached views.
    let px = r
        .render_to_pixels(&gpu, 64, 64, &OrbitCamera::default())
        .expect("render with shared textures must succeed");
    assert!(
        px.as_chunks::<4>().0.iter().any(|p| p[0] != px[0]),
        "frame is uniformly flat"
    );
}

/// A half-cut MASK card casts about half the opaque card's shadow.
/// A quad, not a cube: a cube's silhouette survives cutting half of every face.
#[test]
fn masked_card_casts_half_the_shadow_of_an_opaque_one() {
    use kataglyphis_webgpu_renderer::scene::{
        AlphaMode, CpuMaterial, CpuPrimitive, CpuSampler, CpuTexture, CpuTextureRef, Vertex,
    };
    use std::sync::Arc;

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Receiver: cube_on_plane's ground plane, without its cube.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let base = load_gltf(path).expect("cube_on_plane.gltf must load");
    let plane = base
        .primitives
        .iter()
        .find(|p| p.vertices.len() == 4)
        .expect("the ground plane is the 4-vertex primitive")
        .clone();

    // A 2x2 card at the proven shadow test's cube top, so its shadow lands where that camera looks.
    let card_vertices: Vec<Vertex> = [
        ([-1.0f32, 1.4, -1.0], [0.0f32, 0.0]),
        ([1.0, 1.4, -1.0], [1.0, 0.0]),
        ([1.0, 1.4, 1.0], [1.0, 1.0]),
        ([-1.0, 1.4, 1.0], [0.0, 1.0]),
    ]
    .iter()
    .map(|(pos, uv)| Vertex {
        position: *pos,
        normal: [0.0, 1.0, 0.0],
        uv: *uv,
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0, 1.0, 1.0, 1.0],
        uv1: [0.0, 0.0],
    })
    .collect();

    // Opaque left texel, transparent right: nearest filtering cuts the right half away.
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

    let card = |material: CpuMaterial| CpuPrimitive {
        vertices: card_vertices.clone(),
        // Wound to face the light: the shadow pipeline backface-culls.
        indices: vec![0, 2, 1, 0, 3, 2],
        transform: glam::Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material,
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    };

    let render = |primitives: Vec<CpuPrimitive>| -> Vec<u8> {
        let mut scene = base.clone();
        scene.primitives = primitives;

        let (width, height) = (256u32, 256u32);
        let mut renderer = ForwardRenderer::new(&gpu, width, height);
        renderer.upload_scene(&gpu, &scene);
        // The proven shadow test's light and camera.
        renderer.light_dir_ambient = glam::Vec4::new(-1.0, 0.7, -0.3, 0.15);
        // SSAO off: it darkens from forward depth, which the forward alpha test already halves.
        renderer.ssao_strength = 0.0;

        let camera = OrbitCamera {
            radius: 6.0,
            pitch_deg: 55.0,
            ..OrbitCamera::default()
        };
        renderer
            .render_to_pixels(&gpu, width, height, &camera)
            .expect("headless render must succeed")
    };

    // Pixels the caster darkens to near-black: the shadow is, the mid-gray card body is not.
    let baseline = render(vec![plane.clone()]);
    let shadowed_count = |caster_material: CpuMaterial| -> usize {
        let with_card = render(vec![plane.clone(), card(caster_material)]);
        baseline
            .as_chunks::<4>()
            .0
            .iter()
            .zip(with_card.as_chunks::<4>().0)
            .filter(|(b, w)| {
                let lum_b = (b[0] as i32 * 3 + b[1] as i32 * 6 + b[2] as i32) / 10;
                let lum_w = (w[0] as i32 * 3 + w[1] as i32 * 6 + w[2] as i32) / 10;
                lum_b - lum_w > 40 && lum_w < 30
            })
            .count()
    };

    let opaque = shadowed_count(CpuMaterial {
        base_color_texture: Some(cutout.clone()),
        ..CpuMaterial::default()
    });
    let masked = shadowed_count(CpuMaterial {
        alpha_mode: AlphaMode::Mask(0.5),
        base_color_texture: Some(cutout.clone()),
        ..CpuMaterial::default()
    });

    eprintln!("masked-card shadow counts: opaque = {opaque}, masked = {masked}");
    assert!(
        opaque > 300,
        "the opaque card must cast a substantial shadow, got {opaque} pixels"
    );
    // Roughly half, with generous tolerance for penumbra/PCF edges.
    assert!(
        masked < (opaque * 7) / 10,
        "masked shadow ({masked}) is not meaningfully smaller than opaque ({opaque}) - \
         the alpha test is not reaching the shadow pass"
    );
    assert!(
        masked > opaque / 5,
        "masked shadow ({masked}) nearly vanished vs opaque ({opaque}) - \
         the cut-out should keep about half"
    );
}

/// COLOR_0 multiplies the base colour: green vertices on a white unlit material render green.
#[test]
fn vertex_colors_tint_the_surface() {
    use kataglyphis_webgpu_renderer::scene::{AlphaMode, CpuMaterial, CpuPrimitive, Vertex};

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // A quad facing the camera at the origin, all four vertices GREEN.
    let make_quad = |color: [f32; 4]| -> CpuPrimitive {
        let verts: Vec<Vertex> = [
            ([-2.0f32, -2.0, 0.0], [0.0f32, 0.0]),
            ([2.0, -2.0, 0.0], [1.0, 0.0]),
            ([2.0, 2.0, 0.0], [1.0, 1.0]),
            ([-2.0, 2.0, 0.0], [0.0, 1.0]),
        ]
        .iter()
        .map(|(p, uv)| Vertex {
            position: *p,
            normal: [0.0, 0.0, 1.0],
            uv: *uv,
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color,
            uv1: *uv,
        })
        .collect();
        CpuPrimitive {
            vertices: verts,
            indices: vec![0, 1, 2, 0, 2, 3],
            transform: glam::Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material: CpuMaterial {
                // White unlit: the only colour source is the vertex colour.
                base_color: [1.0, 1.0, 1.0, 1.0],
                alpha_mode: AlphaMode::Opaque,
                unlit: true,
                ..CpuMaterial::default()
            },
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }
    };

    let render = |color: [f32; 4]| -> Vec<u8> {
        let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
        scene.primitives = vec![make_quad(color)];
        let (w, h) = (128u32, 128u32);
        let mut renderer = ForwardRenderer::new(&gpu, w, h);
        renderer.upload_scene(&gpu, &scene);
        // Camera on +Z looking at the quad (yaw 90, pitch 0).
        let camera = OrbitCamera {
            radius: 5.0,
            yaw_deg: 90.0,
            pitch_deg: 0.0,
            ..OrbitCamera::default()
        };
        renderer
            .render_to_pixels(&gpu, w, h, &camera)
            .expect("headless render must succeed")
    };

    // Centre pixel of a green-vertex quad: green dominates both other channels.
    let green = render([0.0, 1.0, 0.0, 1.0]);
    let idx = ((64usize * 128) + 64) * 4;
    let (r, g, b) = (
        green[idx] as i32,
        green[idx + 1] as i32,
        green[idx + 2] as i32,
    );
    assert!(
        g > r + 40 && g > b + 40,
        "green vertex colour did not tint the surface (got r={r} g={g} b={b}) - COLOR_0 dropped"
    );

    // Control: a white-vertex quad renders neutral (all channels close).
    let white = render([1.0, 1.0, 1.0, 1.0]);
    let (wr, wg, wb) = (
        white[idx] as i32,
        white[idx + 1] as i32,
        white[idx + 2] as i32,
    );
    assert!(
        (wr - wg).abs() < 30 && (wg - wb).abs() < 30,
        "white vertex colour should render neutral (got r={wr} g={wg} b={wb})"
    );
}

/// A slot declaring TEXCOORD_1 samples UV1; constant UV0 would show one flat colour.
#[test]
fn texture_slot_samples_its_declared_uv_set() {
    use kataglyphis_webgpu_renderer::scene::{
        AlphaMode, CpuMaterial, CpuPrimitive, CpuSampler, CpuTexture, CpuTextureRef, Vertex,
    };
    use std::sync::Arc;

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // 2x1 texture: left half red, right half blue.
    let tex = CpuTextureRef {
        texture: Arc::new(CpuTexture {
            width: 2,
            height: 1,
            rgba8: vec![255, 0, 0, 255, 0, 0, 255, 255],
            compressed: None,
        }),
        sampler: CpuSampler {
            mag_nearest: true,
            min_nearest: true,
            ..CpuSampler::default()
        },
        srgb: true,
    };

    // UV0 is (0,0) everywhere (the red texel); UV1 spans 0..1 (red, then blue).
    let corners = [
        ([-2.0f32, -2.0, 0.0], [0.0f32, 0.0]),
        ([2.0, -2.0, 0.0], [1.0, 0.0]),
        ([2.0, 2.0, 0.0], [1.0, 1.0]),
        ([-2.0, 2.0, 0.0], [0.0, 1.0]),
    ];
    let verts: Vec<Vertex> = corners
        .iter()
        .map(|(p, uv1)| Vertex {
            position: *p,
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0], // constant UV0
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0, 1.0, 1.0, 1.0],
            uv1: *uv1, // spanning UV1
        })
        .collect();

    let render = |uv_set_mask: u32| -> Vec<u8> {
        let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
        scene.primitives = vec![CpuPrimitive {
            vertices: verts.clone(),
            indices: vec![0, 1, 2, 0, 2, 3],
            transform: glam::Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material: CpuMaterial {
                base_color: [1.0, 1.0, 1.0, 1.0],
                alpha_mode: AlphaMode::Opaque,
                unlit: true,
                base_color_texture: Some(tex.clone()),
                uv_set_mask,
                ..CpuMaterial::default()
            },
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }];
        let (w, h) = (128u32, 128u32);
        let mut renderer = ForwardRenderer::new(&gpu, w, h);
        renderer.upload_scene(&gpu, &scene);
        let camera = OrbitCamera {
            radius: 5.0,
            yaw_deg: 90.0,
            pitch_deg: 0.0,
            ..OrbitCamera::default()
        };
        renderer
            .render_to_pixels(&gpu, w, h, &camera)
            .expect("headless render must succeed")
    };

    let sample = |px: &[u8], x: usize| -> (i32, i32, i32) {
        let idx = ((64usize * 128) + x) * 4;
        (px[idx] as i32, px[idx + 1] as i32, px[idx + 2] as i32)
    };

    // Base slot on UV1 (bit 0): left of the quad is red, right is blue.
    let on_uv1 = render(0b0_0001);
    let (lr, _lg, lb) = sample(&on_uv1, 40);
    let (rr, _rg, rb) = sample(&on_uv1, 88);
    assert!(
        lr > lb + 40,
        "left of the quad should be red on UV1 (got r={lr} b={lb})"
    );
    assert!(
        rb > rr + 40,
        "right of the quad should be blue on UV1 (got r={rr} b={rb})"
    );

    // Base slot on UV0 (mask 0): the whole quad is the red texel.
    let on_uv0 = render(0);
    let (l0r, _l0g, l0b) = sample(&on_uv0, 40);
    let (r0r, _r0g, r0b) = sample(&on_uv0, 88);
    assert!(
        l0r > l0b + 40 && r0r > r0b + 40,
        "on UV0 the whole quad is red (l r={l0r} b={l0b}, r r={r0r} b={r0b})"
    );
}
