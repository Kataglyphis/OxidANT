//! Culling bounds follow skinning, which ignores the node matrix; asserted on `primitive_world_aabb`.
//! See crates/webgpu_renderer/docs/renderer-bounds-invariant.md.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::scene::{CpuNode, CpuSkin};
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf")
}

/// The cube fully weighted to a one-joint skin whose joint (node 1) sits at `joint_translation`.
fn skinned_cube_scene(joint_translation: Vec3) -> kataglyphis_webgpu_renderer::CpuScene {
    let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut prim = scene.primitives[0].clone();
    for v in &mut prim.vertices {
        v.joints = [0.0, 0.0, 0.0, 0.0];
        v.weights = [1.0, 0.0, 0.0, 0.0];
    }
    prim.node_index = Some(0);
    prim.skin_index = Some(0);
    prim.transform = Mat4::IDENTITY;
    scene.primitives = vec![prim];
    scene.nodes = vec![
        CpuNode {
            parent: None,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        CpuNode {
            parent: None,
            translation: joint_translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
    ];
    scene.skins = vec![CpuSkin {
        joints: vec![1],
        inverse_bind_matrices: vec![Mat4::IDENTITY],
    }];
    scene.animations = Vec::new();
    scene
}

#[test]
fn bounds_follow_the_joint_not_the_bind_pose() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // The joint lifts the cube 10 units, entirely outside its bind-pose AABB.
    let scene = skinned_cube_scene(Vec3::new(0.0, 10.0, 0.0));
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_animation_time(0.0);

    let (min, max) = renderer
        .primitive_world_aabb(0)
        .expect("primitive 0 exists");

    assert!(
        max.y >= 9.5,
        "bounds must reach the joint-driven pose (~10.5), got max.y={} (min.y={})",
        max.y,
        min.y
    );
    // X/Z are untouched by the joint, so the box must not balloon sideways.
    assert!(
        max.x <= 1.0 && min.x >= -1.0,
        "bounds must stay tight where the joint does not move, got x=[{}, {}]",
        min.x,
        max.x
    );
}

#[test]
fn an_identity_joint_leaves_bounds_at_the_bind_pose() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Over-widening guard: an identity joint keeps the cube's own box.
    let scene = skinned_cube_scene(Vec3::ZERO);
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_animation_time(0.0);

    let (min, max) = renderer
        .primitive_world_aabb(0)
        .expect("primitive 0 exists");
    assert!(
        (max.y - 0.5).abs() < 1e-4 && (min.y + 0.5).abs() < 1e-4,
        "identity joint must leave the bind-pose box, got y=[{}, {}]",
        min.y,
        max.y
    );
}

/// Bounds must include instance transforms, or instances cull when the base leaves the view.
#[test]
fn bounds_span_every_instance() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = skinned_cube_scene(Vec3::ZERO);
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);

    let (base_min, base_max) = renderer.primitive_world_aabb(0).expect("primitive 0");

    // Two instances, 50 units apart on X, neither at the origin.
    renderer.set_instances(
        &gpu,
        0,
        &[
            Mat4::from_translation(Vec3::new(-50.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(50.0, 0.0, 0.0)),
        ],
    );
    let (min, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        min.x <= -50.0 && max.x >= 50.0,
        "bounds must span both instances, got x=[{}, {}]",
        min.x,
        max.x
    );

    // Clearing instances must collapse the box back.
    renderer.set_instances(&gpu, 0, &[]);
    let (rmin, rmax) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        (rmin.x - base_min.x).abs() < 1e-4 && (rmax.x - base_max.x).abs() < 1e-4,
        "clearing instances must restore the base box, got x=[{}, {}] vs [{}, {}]",
        rmin.x,
        rmax.x,
        base_min.x,
        base_max.x
    );
}

/// scene_bounds alone fits the shadow cascades, so it must track instances too.
#[test]
fn scene_bounds_track_instances() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = skinned_cube_scene(Vec3::ZERO);
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let (base_min, base_max) = renderer.scene_bounds().expect("bounds after upload");

    renderer.set_instances(
        &gpu,
        0,
        &[Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0))],
    );
    let (min, max) = renderer.scene_bounds().expect("bounds after instancing");
    assert!(
        max.x >= 39.0,
        "scene bounds must follow the instance out to x=40, got x=[{}, {}]",
        min.x,
        max.x
    );

    // And collapse back when the instances are cleared.
    renderer.set_instances(&gpu, 0, &[]);
    let (rmin, rmax) = renderer.scene_bounds().expect("bounds after reset");
    assert!(
        (rmax.x - base_max.x).abs() < 1e-4 && (rmin.x - base_min.x).abs() < 1e-4,
        "clearing instances must restore scene bounds, got x=[{}, {}] vs [{}, {}]",
        rmin.x,
        rmax.x,
        base_min.x,
        base_max.x
    );
}

/// Node and instance scale must shade alike; raw instance matrices shear normals off the surface.
/// Ignored: fails on every adapter until `forward.slang` in the C++ engine repo is fixed (BACKLOG.md).
#[test]
#[ignore = "known instanced-normal shading bug, 987 px differ; see BACKLOG.md"]
fn a_non_uniform_instance_scale_shades_like_the_same_node_scale() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (w, h) = (128, 128);
    let camera = kataglyphis_webgpu_renderer::OrbitCamera::default();
    // Rotate too: on axis-aligned normals a plain scale hides the missing inverse-transpose.
    let squash = Mat4::from_scale(Vec3::new(0.25, 1.0, 1.0))
        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4);

    let base = load_gltf(cube_path()).expect("cube.gltf must load");

    // A: squash baked into the primitive's own transform, identity instance.
    let mut scene_a = base.clone();
    scene_a.primitives[0].transform = squash;
    scene_a.primitives[0].node_index = None; // keep the baked transform
    let mut ra = ForwardRenderer::new(&gpu, w, h);
    ra.upload_scene(&gpu, &scene_a);
    let pixels_a = ra.render_to_pixels(&gpu, w, h, &camera).expect("render a");

    // B: identity primitive, squash supplied as an instance transform.
    let mut scene_b = base.clone();
    scene_b.primitives[0].transform = Mat4::IDENTITY;
    scene_b.primitives[0].node_index = None;
    let mut rb = ForwardRenderer::new(&gpu, w, h);
    rb.upload_scene(&gpu, &scene_b);
    rb.set_instances(&gpu, 0, &[squash]);
    let pixels_b = rb.render_to_pixels(&gpu, w, h, &camera).expect("render b");

    // Compare only lit cube pixels; count how many differ beyond dither noise.
    let mut differing = 0usize;
    for (a, b) in pixels_a.chunks_exact(4).zip(pixels_b.chunks_exact(4)) {
        let d = (a[0] as i32 - b[0] as i32).abs()
            + (a[1] as i32 - b[1] as i32).abs()
            + (a[2] as i32 - b[2] as i32).abs();
        if d > 12 {
            differing += 1;
        }
    }
    assert!(
        differing < 40,
        "node-scale and instance-scale must shade the same, {differing} pixels differ"
    );
}
