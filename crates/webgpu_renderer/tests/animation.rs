//! Animation playback asserted on what the renderer exposes: culling bounds and the drawn frame.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::scene::{
    ChannelValues, CpuAnimation, CpuAnimationChannel, CpuNode, Interpolation,
};
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext};

fn asset(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets")
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

/// cube.gltf's cube on node 0, with one clip per `(name, channel)`.
fn cube_with_clips(clips: Vec<(&str, CpuAnimationChannel)>) -> CpuScene {
    let mut scene = load_gltf(asset("cube.gltf")).expect("cube.gltf must load");
    assert_eq!(scene.primitives[0].node_index, Some(0));
    scene.animations = clips
        .into_iter()
        .map(|(name, channel)| CpuAnimation {
            name: name.to_string(),
            duration: 1.0,
            channels: vec![channel],
        })
        .collect();
    scene
}

/// Node 0 held at `value` on `path`, a two-key channel.
fn hold(value: Vec3, scale: bool) -> CpuAnimationChannel {
    let values = vec![value; 2];
    CpuAnimationChannel {
        node: 0,
        times: vec![0.0, 1.0],
        values: if scale {
            ChannelValues::Scale(values)
        } else {
            ChannelValues::Translation(values)
        },
        interpolation: Interpolation::Linear,
    }
}

fn centre_x(renderer: &ForwardRenderer) -> f32 {
    let (min, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    (min.x + max.x) * 0.5
}

/// Fox.glb's Survey, Walk and Run drive the same joints; sampling all of them showed only the last.
#[test]
fn only_the_active_clip_plays() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_with_clips(vec![
        ("one", hold(Vec3::new(1.0, 0.0, 0.0), false)),
        ("two", hold(Vec3::new(2.0, 0.0, 0.0), false)),
    ]);
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    assert_eq!(renderer.animation_names(), vec!["one", "two"]);
    assert_eq!(renderer.active_animation(), 0);

    renderer.set_animation_time(0.5);
    assert!(
        (centre_x(&renderer) - 1.0).abs() < 1e-5,
        "the first clip plays by default"
    );
    renderer.set_active_animation(1);
    renderer.set_animation_time(0.5);
    assert!(
        (centre_x(&renderer) - 2.0).abs() < 1e-5,
        "then only the selected one"
    );
    renderer.set_active_animation(7);
    assert_eq!(
        renderer.active_animation(),
        1,
        "an index past the last clip is ignored"
    );
}

#[test]
fn switching_clips_returns_undriven_nodes_to_their_rest_pose() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_with_clips(vec![
        ("slide", hold(Vec3::new(3.0, 0.0, 0.0), false)),
        ("grow", hold(Vec3::ONE, true)),
    ]);
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_animation_time(0.5);
    assert!((centre_x(&renderer) - 3.0).abs() < 1e-5);
    renderer.set_active_animation(1);
    renderer.set_animation_time(0.5);
    assert!(
        centre_x(&renderer).abs() < 1e-5,
        "the slide must not linger once a clip that does not drive translation plays"
    );
}

/// An app passing f64 seconds keeps smooth playback after days of uptime.
#[test]
fn animation_time_in_f64_advances_every_frame_after_three_days() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // The 2 s clip slides x from -1.5 to 1.5; 2^18 s is a whole number of clips.
    let scene = load_gltf(asset("cube_animated.gltf")).expect("cube_animated.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    let base = f64::from(1u32 << 18);
    let mut previous = f32::NEG_INFINITY;
    for k in 0..24 {
        renderer.set_animation_time(base + f64::from(k) / 144.0);
        let x = centre_x(&renderer);
        assert!(
            x > previous,
            "frame {k} did not advance the clip: {x} after {previous}"
        );
        previous = x;
    }
}

/// cube_morph.gltf: the unit cube and one target lifting every vertex 1.0 in +Y, at `weight`.
fn morph_cube(weight: f32) -> CpuScene {
    let mut scene = load_gltf(asset("cube_morph.gltf")).expect("cube_morph.gltf must load");
    scene.primitives[0].morph_weights = vec![weight];
    scene
}

/// glTF puts no range on morph weights; bounds built for [0, 1] let the frustum drop drawn geometry.
#[test]
fn morph_bounds_cover_default_weights_outside_zero_to_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &morph_cube(2.0));
    let (_, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        max.y >= 2.5 - 1e-5,
        "weight 2 lifts the top to 2.5, bounds end at {}",
        max.y
    );

    renderer.upload_scene(&gpu, &morph_cube(-1.0));
    let (min, _) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        min.y <= -1.5 + 1e-5,
        "weight -1 drops the bottom to -1.5, bounds end at {}",
        min.y
    );
}

#[test]
fn morph_bounds_follow_an_animated_weight_past_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut scene = morph_cube(0.0);
    let node = scene.primitives[0]
        .node_index
        .expect("the morph cube hangs from a node");
    scene.animations = vec![CpuAnimation {
        name: "swell".to_string(),
        duration: 2.0,
        channels: vec![CpuAnimationChannel {
            node,
            times: vec![0.0, 2.0],
            values: ChannelValues::MorphWeights(vec![0.0, 2.0]),
            interpolation: Interpolation::Linear,
        }],
    }];
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_animation_time(1.5);
    let (_, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        max.y >= 2.0 - 1e-5,
        "weight 1.5 lifts the top to 2.0, bounds end at {}",
        max.y
    );
    let (_, scene_max) = renderer.scene_bounds().expect("scene bounds");
    assert!(scene_max.y >= 2.0 - 1e-5, "the cascades must see it too");
}
