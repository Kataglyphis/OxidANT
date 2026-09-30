//! Occlusion detection and culling against a real adapter: hidden reads 0 samples, visible > 0.

use glam::{Mat4, Vec3};
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext, OrbitCamera};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf")
}

/// The bundled unit cube (positions in [-0.5, 0.5]) transformed by `transform`.
fn cube_with_transform(transform: Mat4) -> kataglyphis_webgpu_renderer::CpuPrimitive {
    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut prim = scene.primitives[0].clone();
    prim.transform = transform;
    prim
}

/// A camera on +Z looking down -Z at the origin (yaw 90, pitch 0).
fn looking_down_neg_z() -> OrbitCamera {
    OrbitCamera {
        radius: 8.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    }
}

/// Renders enough frames for the never-awaited occlusion readback to land.
fn render_until_readback_lands(
    renderer: &mut ForwardRenderer,
    gpu: &GpuContext,
    camera: &OrbitCamera,
) {
    for _ in 0..64 {
        renderer
            .render_to_pixels(gpu, 256, 256, camera)
            .expect("headless render must succeed");
    }
}

#[test]
fn a_cube_hidden_behind_another_reads_back_zero_samples() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // A large occluder (index 0) fully covers a small cube behind it (index 1).
    let occluder = cube_with_transform(
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)) * Mat4::from_scale(Vec3::splat(4.0)),
    );
    let hidden = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, 0.0, -3.0)));

    let scene = CpuScene {
        primitives: vec![occluder, hidden],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;

    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);

    let samples = renderer.occlusion_samples();
    let visibility = renderer.occlusion_visibility();
    eprintln!("occluded-scene samples: {samples:?}");
    assert_eq!(
        visibility.len(),
        2,
        "expected one visibility entry per primitive, got {visibility:?} (samples {samples:?})"
    );

    assert!(
        visibility[0],
        "the occluder should be visible (> 0 samples), measured {}",
        samples[0]
    );
    assert!(
        !visibility[1],
        "the hidden cube should be occluded (0 samples), measured {}",
        samples[1]
    );
    assert_eq!(
        samples[1], 0,
        "a fully occluded box must count exactly zero fragments, got {}",
        samples[1]
    );
}

#[test]
fn two_side_by_side_cubes_are_both_visible() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Two cubes offset along x, both fully in view and occluding nothing.
    let left = cube_with_transform(Mat4::from_translation(Vec3::new(-2.0, 0.0, 0.0)));
    let right = cube_with_transform(Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)));

    let scene = CpuScene {
        primitives: vec![left, right],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;

    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);

    let samples = renderer.occlusion_samples();
    let visibility = renderer.occlusion_visibility();
    eprintln!("side-by-side samples: {samples:?}");
    assert_eq!(visibility.len(), 2, "expected two visibility entries");
    assert!(
        visibility[0] && visibility[1],
        "both visible cubes should report > 0 samples, measured {samples:?}"
    );
}

#[test]
fn detection_is_off_by_default() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);

    // Default-off contract: nothing is measured.
    assert!(!renderer.occlusion_queries_enabled);
    let camera = OrbitCamera::default();
    for _ in 0..4 {
        renderer
            .render_to_pixels(&gpu, 128, 128, &camera)
            .expect("headless render must succeed");
    }
    assert!(
        renderer.occlusion_visibility().is_empty(),
        "no visibility should be recorded while detection is off"
    );
}

#[test]
fn an_occluded_primitive_is_actually_skipped_in_the_opaque_pass() {
    // Both cubes are frustum-visible, so skipping the hidden one proves occlusion culling.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let occluder = cube_with_transform(
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)) * Mat4::from_scale(Vec3::splat(4.0)),
    );
    let hidden = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, 0.0, -3.0)));
    let scene = CpuScene {
        primitives: vec![occluder, hidden],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;

    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);
    // One more frame so the loop sees the settled visibility.
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");

    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!(considered, 2, "both cubes are within the frustum");
    assert_eq!(
        drawn, 1,
        "the hidden cube must be occlusion-culled, leaving 1 draw (got {drawn})"
    );

    // Disabling it draws both again next frame - the skip is gated on the flag.
    renderer.occlusion_queries_enabled = false;
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");
    let (drawn_off, _) = renderer.occlusion_cull_stats();
    assert_eq!(drawn_off, 2, "with culling off both cubes draw");
}

#[test]
fn two_visible_cubes_are_both_drawn_with_culling_on() {
    // Over-culling guard: two visible cubes both draw with occlusion on.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let left = cube_with_transform(Mat4::from_translation(Vec3::new(-3.0, 0.0, 0.0)));
    let right = cube_with_transform(Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)));
    let scene = CpuScene {
        primitives: vec![left, right],
        ..Default::default()
    };
    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;
    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");
    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!((drawn, considered), (2, 2), "both visible cubes must draw");
}

#[test]
fn loading_a_new_scene_does_not_inherit_the_old_scene_visibility() {
    // Visibility is per index: scene B's first frame must not cull what scene A occluded.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Scene A: index 1 ends up occluded behind index 0.
    let occluder = cube_with_transform(
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)) * Mat4::from_scale(Vec3::splat(4.0)),
    );
    let hidden = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, 0.0, -3.0)));
    let scene_a = CpuScene {
        primitives: vec![occluder, hidden],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene_a);
    renderer.occlusion_queries_enabled = true;
    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");
    // Precondition: stale `false` visibility exists at index 1.
    assert_eq!(
        renderer.occlusion_cull_stats().0,
        1,
        "scene A must occlusion-cull its hidden cube, or this test proves nothing"
    );

    // Scene B: two visible cubes; index 1 is a different, visible primitive.
    let scene_b = CpuScene {
        primitives: vec![
            cube_with_transform(Mat4::from_translation(Vec3::new(-3.0, 0.0, 0.0))),
            cube_with_transform(Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0))),
        ],
        ..Default::default()
    };
    renderer.upload_scene(&gpu, &scene_b);

    // Before scene B's own queries land, leftover visibility would cull index 1.
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");
    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!(
        (drawn, considered),
        (2, 2),
        "a freshly loaded scene must not inherit the old scene's culling (got \
         {drawn}/{considered})"
    );
}

#[test]
fn an_occluded_primitive_is_skipped_with_gpu_culling() {
    // The compute-shader path must be interchangeable with the queries for the draw loop.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let occluder = cube_with_transform(
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)) * Mat4::from_scale(Vec3::splat(4.0)),
    );
    let hidden = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, 0.0, -3.0)));
    let scene = CpuScene {
        primitives: vec![occluder, hidden],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.gpu_culling_enabled = true;

    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);

    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!(considered, 2, "both cubes are within the frustum");
    assert_eq!(
        drawn, 1,
        "the hidden cube must be culled by the compute-shader path, leaving 1 draw (got {drawn})"
    );

    // Disabling it draws both again next frame - the skip is gated on the flag.
    renderer.gpu_culling_enabled = false;
    renderer
        .render_to_pixels(&gpu, 256, 256, &camera)
        .expect("render");
    let (drawn_off, _) = renderer.occlusion_cull_stats();
    assert_eq!(drawn_off, 2, "with culling off both cubes draw");
}

#[test]
fn two_visible_cubes_are_both_drawn_with_gpu_culling() {
    // Over-culling guard for the compute-shader path.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let left = cube_with_transform(Mat4::from_translation(Vec3::new(-3.0, 0.0, 0.0)));
    let right = cube_with_transform(Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)));
    let scene = CpuScene {
        primitives: vec![left, right],
        ..Default::default()
    };
    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.gpu_culling_enabled = true;
    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);
    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!((drawn, considered), (2, 2), "both visible cubes must draw");
}

#[test]
fn a_primitive_containing_the_camera_is_always_reported_visible() {
    // From inside its AABB the query reads 0; double-sided, as a culled cube renders nothing inside.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let mut prim = cube_with_transform(Mat4::IDENTITY);
    prim.material.double_sided = true;
    let scene = CpuScene {
        primitives: vec![prim],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;

    // Eye inside the unit cube, well past the 0.1 near plane.
    let camera = OrbitCamera {
        radius: 0.2,
        yaw_deg: 0.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };

    let mut observed = Vec::new();
    for _ in 0..64 {
        renderer
            .render_to_pixels(&gpu, 256, 256, &camera)
            .expect("headless render must succeed");
        if let Some(&visible) = renderer.occlusion_visibility().first() {
            observed.push(visible);
        }
    }

    assert!(
        !observed.is_empty(),
        "expected at least one readback to land in 64 frames"
    );
    assert!(
        observed.iter().all(|&v| v),
        "a primitive containing the camera must be reported visible on every \
         landed readback, got {observed:?}"
    );
}

#[test]
fn gpu_culling_respects_vertical_screen_position() {
    // Top-half-only occluder: a vertically mirrored NDC->uv mapping swaps both cubes' results.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // y in [0, 8] near the camera: only the top half of the frame.
    let occluder = cube_with_transform(
        Mat4::from_translation(Vec3::new(0.0, 4.0, 2.0))
            * Mat4::from_scale(Vec3::new(8.0, 8.0, 4.0)),
    );
    // Both behind the occluder's depth, one per screen half.
    let hidden_top = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, 2.0, -3.0)));
    let hidden_bottom = cube_with_transform(Mat4::from_translation(Vec3::new(0.0, -2.0, -3.0)));

    let scene = CpuScene {
        primitives: vec![occluder, hidden_top, hidden_bottom],
        ..Default::default()
    };

    let mut renderer = ForwardRenderer::new(&gpu, 256, 256);
    renderer.upload_scene(&gpu, &scene);
    renderer.gpu_culling_enabled = true;

    let camera = looking_down_neg_z();
    render_until_readback_lands(&mut renderer, &gpu, &camera);

    let visibility = renderer.gpu_culling_visibility();
    eprintln!("gpu-culling visibility: {visibility:?}");
    assert_eq!(
        visibility.len(),
        3,
        "expected one visibility entry per primitive, got {visibility:?}"
    );
    assert!(visibility[0], "the occluder itself must remain visible");
    assert!(
        !visibility[1],
        "the top-half cube sits behind the occluder in its own screen region and must be culled"
    );
    assert!(
        visibility[2],
        "the bottom-half cube is outside the occluder's screen footprint and must stay visible"
    );
}

#[test]
fn gpu_culling_is_off_by_default() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);

    assert!(!renderer.gpu_culling_enabled);
    let camera = OrbitCamera::default();
    for _ in 0..4 {
        renderer
            .render_to_pixels(&gpu, 128, 128, &camera)
            .expect("headless render must succeed");
    }
    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert_eq!(
        drawn, considered,
        "no primitive should be culled while gpu_culling_enabled is off"
    );
}
