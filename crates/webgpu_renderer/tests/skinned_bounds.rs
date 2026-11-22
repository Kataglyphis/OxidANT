//! Culling bounds follow skinning, which ignores the node matrix; asserted on `primitive_world_aabb`.
//! See crates/webgpu_renderer/docs/renderer-bounds-invariant.md.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::scene::{CpuMaterial, CpuNode, CpuPrimitive, CpuSkin, Vertex};
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf")
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

/// Red cube pixels in a 128x128 frame looking at `target`.
fn red_pixels_around(renderer: &mut ForwardRenderer, gpu: &GpuContext, target: Vec3) -> usize {
    let camera = kataglyphis_webgpu_renderer::OrbitCamera {
        target,
        ..Default::default()
    };
    let pixels = renderer
        .render_to_pixels(gpu, 128, 128, &camera)
        .expect("render");
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 110 && p[0] as i32 > p[1] as i32 + 40 && p[0] as i32 > p[2] as i32 + 40)
        .count()
}

/// A rig with no clip never reaches `set_animation_time`; its joints must still pose the mesh.
#[test]
fn an_unanimated_skin_draws_at_its_joints_pose() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = skinned_cube_scene(Vec3::new(0.0, 10.0, 0.0));
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);

    let at_joint = red_pixels_around(&mut renderer, &gpu, Vec3::new(0.0, 10.0, 0.0));
    assert!(
        at_joint > 200,
        "the cube must draw where its joint holds it (y = 10), found {at_joint} red pixels"
    );
    let at_bind_pose = red_pixels_around(&mut renderer, &gpu, Vec3::ZERO);
    assert_eq!(
        at_bind_pose, 0,
        "nothing may draw at the raw bind pose (the origin)"
    );
}

/// Cascades fit `scene_bounds` alone, so it must cover the joint-posed box from upload on.
#[test]
fn scene_bounds_after_upload_cover_the_joint_pose() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = skinned_cube_scene(Vec3::new(0.0, 10.0, 0.0));
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let (_, max) = renderer.scene_bounds().expect("bounds after upload");
    assert!(
        max.y >= 10.0,
        "scene bounds must reach the joint pose (~10.5) without any animation call, got max.y={}",
        max.y
    );
}

/// The cube fully on joint 299 of a 300-joint skin, that joint at `lift`.
fn cube_on_joint_299(lift: Vec3) -> kataglyphis_webgpu_renderer::CpuScene {
    let mut scene = skinned_cube_scene(Vec3::ZERO);
    let first = scene.nodes.len();
    for j in 0..300 {
        scene.nodes.push(CpuNode {
            parent: None,
            translation: if j == 299 { lift } else { Vec3::ZERO },
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        });
    }
    scene.skins = vec![CpuSkin {
        joints: (first..first + 300).collect(),
        inverse_bind_matrices: vec![Mat4::IDENTITY; 300],
    }];
    for v in &mut scene.primitives[0].vertices {
        v.joints = [299.0, 0.0, 0.0, 0.0];
    }
    scene
}

/// A 256-joint cap truncated the buffer and the bounds; VRM and game rigs exceed it.
#[test]
fn a_skin_over_256_joints_poses_and_bounds_its_last_joint() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_on_joint_299(Vec3::new(0.0, 10.0, 0.0));
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let (_, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        max.y >= 9.5,
        "bounds must cover joint 299's pose, got max.y={}",
        max.y
    );
    let at_joint = red_pixels_around(&mut renderer, &gpu, Vec3::new(0.0, 10.0, 0.0));
    assert!(
        at_joint > 200,
        "joint 299 must pose the cube, found {at_joint} red pixels"
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
#[test]
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
    for (a, b) in pixels_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels_b.as_chunks::<4>().0)
    {
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

/// Skinned normals take the inverse transpose too: a non-uniform instance shades as on the plain mesh.
#[test]
fn a_non_uniform_instance_scale_shades_a_skinned_mesh_like_an_unskinned_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (w, h) = (128, 128);
    let camera = kataglyphis_webgpu_renderer::OrbitCamera::default();
    // Rotated as well, since an axis-aligned cube hides a wrong normal transform under a plain scale.
    let squash = Mat4::from_scale(Vec3::new(0.25, 1.0, 1.0))
        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4);

    let mut plain = load_gltf(cube_path()).expect("cube.gltf must load");
    plain.primitives[0].transform = Mat4::IDENTITY;
    plain.primitives[0].node_index = None;
    let mut ra = ForwardRenderer::new(&gpu, w, h);
    ra.upload_scene(&gpu, &plain);
    ra.set_instances(&gpu, 0, &[squash]);
    let pixels_a = ra.render_to_pixels(&gpu, w, h, &camera).expect("render a");

    // The joint sits at the origin with an identity inverse bind, so skinning moves nothing.
    let mut rb = ForwardRenderer::new(&gpu, w, h);
    rb.upload_scene(&gpu, &skinned_cube_scene(Vec3::ZERO));
    rb.set_instances(&gpu, 0, &[squash]);
    let pixels_b = rb.render_to_pixels(&gpu, w, h, &camera).expect("render b");

    let differing = pixels_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels_b.as_chunks::<4>().0)
        .filter(|(a, b)| {
            let d: i32 = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| (*x as i32 - *y as i32).abs())
                .sum();
            d > 12
        })
        .count();
    assert!(
        differing < 40,
        "a skinned and a plain cube under the same instance shade differently in {differing} pixels"
    );
}

/// Half-float HDR channels with every exponent bit set: Inf, or a NaN the adapter's min and max kept.
fn non_finite_channels(hdr: &[u16]) -> usize {
    hdr.iter().filter(|h| *h & 0x7c00 == 0x7c00).count()
}

/// Rigs hide parts with a zero-scale joint, whose vertices' normals and tangents collapse to zero.
#[test]
fn a_vertex_on_a_zero_scale_joint_never_shades_nan() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // The top corners ride a zero-scale joint at the top face's centre, so every visible face meets it.
    let mut scene = skinned_cube_scene(Vec3::ZERO);
    scene.nodes.push(CpuNode {
        parent: None,
        translation: Vec3::new(0.0, 0.5, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ZERO,
    });
    scene.skins[0].joints.push(2);
    scene.skins[0].inverse_bind_matrices.push(Mat4::IDENTITY);
    for v in &mut scene.primitives[0].vertices {
        if v.position[1] > 0.0 {
            v.joints = [1.0, 0.0, 0.0, 0.0];
        }
    }
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);

    let red = red_pixels_around(&mut renderer, &gpu, Vec3::ZERO);
    let hdr = renderer.read_hdr_target(&gpu).expect("HDR readback");
    assert_eq!(non_finite_channels(&hdr), 0, "the pyramid left Inf or NaN");
    // Where min and max drop the NaN (lavapipe), the faces shade near-black instead.
    assert!(
        red > 200,
        "the pyramid must shade red, found {red} red pixels"
    );
}

/// A red octahedron whose smooth normals are its corners.
fn smooth_octahedron() -> CpuPrimitive {
    let corners = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let vertices = corners
        .iter()
        .map(|&c| Vertex {
            position: (c * 0.8).to_array(),
            normal: c.to_array(),
            uv: [0.0, 0.0],
            tangent: if c.x == 0.0 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            },
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0; 4],
            uv1: [0.0, 0.0],
        })
        .collect();
    CpuPrimitive {
        vertices,
        indices: vec![
            0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 4, 2, 1, 3, 4, 1, 5, 3, 1, 2, 5,
        ],
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            base_color: [0.8, 0.2, 0.2, 1.0],
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// An instance with a zero scale axis zeroes every normal lying across it, here the y and z corners'.
#[test]
fn a_flattened_instance_never_shades_nan() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(
        &gpu,
        &CpuScene {
            primitives: vec![smooth_octahedron()],
            ..CpuScene::default()
        },
    );
    renderer.set_instances(&gpu, 0, &[Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0))]);
    // Looking down -x at the flat diamond the +x half leaves.
    let camera = kataglyphis_webgpu_renderer::OrbitCamera {
        yaw_deg: 0.0,
        pitch_deg: 0.0,
        ..Default::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, 128, 128, &camera)
        .expect("render");
    let red = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] as i32 > p[2] as i32 + 20)
        .count();
    let hdr = renderer.read_hdr_target(&gpu).expect("HDR readback");
    assert_eq!(non_finite_channels(&hdr), 0, "the diamond left Inf or NaN");
    assert!(
        red > 300,
        "the diamond must shade red, found {red} red pixels"
    );
}

/// glTF ignores a skinned mesh's node transform, so animating that node moves no vertex, unweighted ones included.
#[test]
fn an_animated_skinned_node_moves_neither_its_unweighted_vertices_nor_its_bounds() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut scene = skinned_cube_scene(Vec3::ZERO);
    for v in &mut scene.primitives[0].vertices {
        v.weights = [0.0; 4];
    }
    scene.animations =
        vec![kataglyphis_webgpu_renderer::scene::CpuAnimation {
            name: "slide".to_owned(),
            duration: 1.0,
            channels: vec![kataglyphis_webgpu_renderer::scene::CpuAnimationChannel {
                node: 0,
                times: vec![0.0, 1.0],
                values: kataglyphis_webgpu_renderer::scene::ChannelValues::Translation(vec![
                Vec3::new(40.0, 0.0, 0.0);
                2
            ]),
                interpolation: kataglyphis_webgpu_renderer::scene::Interpolation::Linear,
            }],
        }];
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_animation_time(0.5);
    let (_, max) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        max.x < 1.0,
        "the box must stay at the cube, got max.x={}",
        max.x
    );
    let at_origin = red_pixels_around(&mut renderer, &gpu, Vec3::ZERO);
    assert!(
        at_origin > 200,
        "the cube must stay at the origin, found {at_origin} red pixels"
    );
    let at_node = red_pixels_around(&mut renderer, &gpu, Vec3::new(40.0, 0.0, 0.0));
    assert_eq!(at_node, 0, "nothing may follow the skinned node to x = 40");
}

/// Only an unweighted vertex draws by the model matrix; a gltfpack skin's integer positions would span 16k units.
#[test]
fn a_fully_weighted_skin_is_bounded_by_its_joints_alone() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut scene = skinned_cube_scene(Vec3::new(0.0, 10.0, 0.0));
    scene.animations = vec![kataglyphis_webgpu_renderer::scene::CpuAnimation {
        name: "hold".to_owned(),
        duration: 1.0,
        channels: vec![kataglyphis_webgpu_renderer::scene::CpuAnimationChannel {
            node: 1,
            times: vec![0.0],
            values: kataglyphis_webgpu_renderer::scene::ChannelValues::Translation(vec![
                Vec3::new(0.0, 10.0, 0.0),
            ]),
            interpolation: kataglyphis_webgpu_renderer::scene::Interpolation::Linear,
        }],
    }];
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let (min, _) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        min.y > 9.0,
        "the upload kept the cube's own box, min.y={}",
        min.y
    );
    renderer.set_animation_time(0.5);
    let (min, _) = renderer.primitive_world_aabb(0).expect("primitive 0");
    assert!(
        min.y > 9.0,
        "the animation kept the cube's own box, min.y={}",
        min.y
    );
}
