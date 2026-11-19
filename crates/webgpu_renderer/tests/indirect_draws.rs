//! The draw cull's indirect arguments against a real adapter: culled draws carry no instances, drawn ones exact args.

use glam::{Mat4, Vec3};
use kataglyphis_webgpu_renderer::scene::AlphaMode;
use kataglyphis_webgpu_renderer::{
    load_gltf, CpuPrimitive, CpuScene, ForwardRenderer, GpuContext, OrbitCamera,
};
use wgpu::util::DrawIndexedIndirectArgs;

const SIZE: u32 = 128;

fn cube(transform: Mat4) -> CpuPrimitive {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf");
    let mut prim = load_gltf(path).expect("cube.gltf must load").primitives[0].clone();
    prim.transform = transform;
    prim
}

fn at(x: f32, y: f32, z: f32) -> CpuPrimitive {
    cube(Mat4::from_translation(Vec3::new(x, y, z)))
}

fn renderer(gpu: &GpuContext, primitives: Vec<CpuPrimitive>) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(
        gpu,
        &CpuScene {
            primitives,
            ..Default::default()
        },
    );
    renderer
}

/// Looking down -Z at `target` from 4 units away.
fn looking_at(target: Vec3) -> OrbitCamera {
    OrbitCamera {
        target,
        radius: 4.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    }
}

fn render(renderer: &mut ForwardRenderer, gpu: &GpuContext, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

/// The camera arguments of primitive `index`'s slots.
fn camera_args(
    renderer: &ForwardRenderer,
    gpu: &GpuContext,
    index: usize,
) -> Vec<DrawIndexedIndirectArgs> {
    let args = renderer.read_draw_args(gpu, 0).expect("args readback");
    args[renderer.draw_slots_of(index)].to_vec()
}

fn exact(index_count: u32, instance_count: u32) -> DrawIndexedIndirectArgs {
    DrawIndexedIndirectArgs {
        index_count,
        instance_count,
        first_index: 0,
        base_vertex: 0,
        first_instance: 0,
    }
}

fn assert_args_eq(
    actual: &[DrawIndexedIndirectArgs],
    expected: &[DrawIndexedIndirectArgs],
    what: &str,
) {
    let key = |a: &DrawIndexedIndirectArgs| {
        (
            a.index_count,
            a.instance_count,
            a.first_index,
            a.base_vertex,
            a.first_instance,
        )
    };
    let actual: Vec<_> = actual.iter().map(key).collect();
    let expected: Vec<_> = expected.iter().map(key).collect();
    assert_eq!(actual, expected, "{what}");
}

#[test]
fn a_culled_draw_carries_no_instances_and_a_drawn_one_its_exact_arguments() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, vec![at(0.0, 0.0, 0.0), at(40.0, 0.0, 0.0)]);
    let camera = looking_at(Vec3::ZERO);
    render(&mut renderer, &gpu, &camera);
    let indices = renderer
        .selected_index_count(0, camera.eye())
        .expect("primitive 0 exists");

    assert_args_eq(
        &camera_args(&renderer, &gpu, 0),
        &[exact(indices, 1)],
        "the cube in view",
    );
    assert_args_eq(
        &camera_args(&renderer, &gpu, 1),
        &[exact(indices, 0)],
        "the cube 40 units aside",
    );
    assert_eq!(renderer.occlusion_cull_stats(), (1, 1));
    assert_eq!(renderer.primitive_stats(), (1, 2));

    renderer.frustum_culling = false;
    render(&mut renderer, &gpu, &camera);
    assert_args_eq(
        &camera_args(&renderer, &gpu, 1),
        &[exact(indices, 1)],
        "with frustum culling off the cube aside draws",
    );
}

#[test]
fn a_mirrored_run_is_its_own_draw_and_both_carry_their_instance_counts() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, vec![at(0.0, 0.0, 0.0)]);
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    let transforms = [
        Mat4::from_translation(Vec3::new(-1.0, 0.0, 0.0)),
        Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)) * mirror,
        Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)),
    ];
    renderer.set_instances(&gpu, 0, &transforms);
    let camera = OrbitCamera {
        radius: 8.0,
        ..looking_at(Vec3::ZERO)
    };
    render(&mut renderer, &gpu, &camera);
    let indices = renderer
        .selected_index_count(0, camera.eye())
        .expect("exists");

    assert_args_eq(
        &camera_args(&renderer, &gpu, 0),
        &[exact(indices, 2), exact(indices, 1)],
        "two upright instances, then the mirrored one",
    );
}

#[test]
fn a_camera_turn_culls_what_left_the_view_and_draws_what_came_back() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, vec![at(-6.0, 0.0, 0.0), at(6.0, 0.0, 0.0)]);
    let left = looking_at(Vec3::new(-6.0, 0.0, 0.0));
    let right = looking_at(Vec3::new(6.0, 0.0, 0.0));
    let instances = |renderer: &ForwardRenderer| {
        [0, 1].map(|i| camera_args(renderer, &gpu, i)[0].instance_count)
    };

    for (camera, expected, what) in [
        (&left, [1, 0], "facing the left cube"),
        (&right, [0, 1], "turned to the right cube"),
        (&left, [1, 0], "turned back to the left"),
    ] {
        render(&mut renderer, &gpu, camera);
        assert_eq!(instances(&renderer), expected, "{what}");
        assert_eq!(renderer.occlusion_cull_stats(), (1, 1), "{what}");
    }
}

#[test]
fn a_blended_draw_outside_the_frustum_carries_no_instances() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut glass = [at(0.0, 0.0, 0.0), at(40.0, 0.0, 0.0)];
    for prim in &mut glass {
        prim.material.alpha_mode = AlphaMode::Blend;
        prim.material.base_color[3] = 0.5;
    }
    let mut renderer = renderer(&gpu, glass.to_vec());
    render(&mut renderer, &gpu, &looking_at(Vec3::ZERO));

    let drawn = [0, 1].map(|i| camera_args(&renderer, &gpu, i)[0].instance_count);
    assert_eq!(drawn, [1, 0]);
    assert_eq!(
        renderer.primitive_stats(),
        (1, 2),
        "one blended draw, none opaque"
    );
    assert_eq!(
        renderer.occlusion_cull_stats(),
        (0, 0),
        "blended draws are never occlusion-tested"
    );
}

#[test]
fn each_cascade_keeps_only_the_casters_inside_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, vec![at(0.0, 0.0, 0.0), at(0.0, 0.0, -900.0)]);
    renderer.shadows.cascade_count = 1;
    render(&mut renderer, &gpu, &looking_at(Vec3::ZERO));

    let cascade = renderer.read_draw_args(&gpu, 1).expect("cascade args");
    let drawn = [0, 1].map(|i| cascade[renderer.draw_slots_of(i)][0].instance_count);
    assert_eq!(drawn, [1, 0], "the far cube lies outside the one cascade");
    assert_eq!(renderer.shadow_caster_stats(), (1, 2));
}

/// A wall hides a row of cubes behind it; every occlusion mode must keep the frame the unculled one draws.
#[test]
fn occlusion_culls_hidden_draws_but_never_changes_a_pixel() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let wall = cube(
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0))
            * Mat4::from_scale(Vec3::new(8.0, 4.0, 0.2)),
    );
    let mut primitives = vec![wall];
    primitives.extend((-2..=2).map(|x| at(x as f32 * 1.2, 0.0, -2.0)));
    // Above the wall, so visible past it.
    primitives.push(at(0.0, 3.5, -2.0));
    let camera = OrbitCamera {
        radius: 10.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };

    let mut reference = renderer(&gpu, primitives.clone());
    let unculled = render(&mut reference, &gpu, &camera);
    for queries in [true, false] {
        let mut culled = renderer(&gpu, primitives.clone());
        culled.occlusion_queries_enabled = queries;
        culled.gpu_culling_enabled = !queries;
        let mut pixels = Vec::new();
        for _ in 0..8 {
            pixels = render(&mut culled, &gpu, &camera);
        }
        let hidden: Vec<u32> = (1..=5)
            .map(|i| camera_args(&culled, &gpu, i)[0].instance_count)
            .collect();
        assert_eq!(
            hidden, [0; 5],
            "queries {queries}: the five cubes behind the wall"
        );
        assert_eq!(
            camera_args(&culled, &gpu, 6)[0].instance_count,
            1,
            "queries {queries}: the cube above it"
        );
        assert!(
            pixels == unculled,
            "queries {queries}: occlusion culling changed the frame"
        );
    }
}
