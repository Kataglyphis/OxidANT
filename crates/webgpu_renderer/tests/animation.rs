//! Animation playback asserted on what the renderer exposes: culling bounds and the drawn frame.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::scene::CpuNode;
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext};

fn asset(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name)
}

/// cube_animated.gltf plus a static copy of its cube on a node hidden with scale 0.
fn animated_scene_with_a_hidden_cube() -> CpuScene {
    let mut scene = load_gltf(asset("cube_animated.gltf")).expect("cube_animated.gltf must load");
    let mut hidden = scene.primitives[0].clone();
    scene.nodes.push(CpuNode {
        parent: None,
        translation: Vec3::new(3.0, 0.0, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ZERO,
    });
    hidden.node_index = Some(scene.nodes.len() - 1);
    hidden.transform =
        Mat4::from_scale_rotation_translation(Vec3::ZERO, Quat::IDENTITY, Vec3::new(3.0, 0.0, 0.0));
    scene.primitives.push(hidden);
    scene
}

/// A hidden node is hidden at upload; the first animation frame must not unhide it.
#[test]
fn a_zero_scale_node_stays_hidden_once_animation_runs() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = animated_scene_with_a_hidden_cube();
    let hidden = scene.primitives.len() - 1;
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    let (min, max) = renderer
        .primitive_world_aabb(hidden)
        .expect("hidden primitive");
    assert_eq!(min, max, "precondition: hidden at upload");

    renderer.set_animation_time(0.5);
    let (min, max) = renderer
        .primitive_world_aabb(hidden)
        .expect("hidden primitive");
    assert_eq!(
        min, max,
        "a zero-scale node must keep a zero-volume box after set_animation_time, got {min:?}..{max:?}"
    );
}
