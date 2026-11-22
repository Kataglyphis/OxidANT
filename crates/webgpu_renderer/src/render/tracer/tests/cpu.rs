//! The CPU traversal against brute force, the alpha test, top-level rebuilds and the build budget.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec3A};

use super::scenes::{self, build, rays};
use crate::render::tracer::build::{TraceBuildOptions, TraceSceneBuilder};
use crate::render::tracer::bvh::Aabb;
use crate::render::tracer::query::{TraceRay, FAR};
use crate::render::tracer::scene::{InstanceChange, TraceScene};
use crate::scene::{ChannelValues, CpuAnimation, CpuAnimationChannel, CpuScene, Interpolation};

fn named_scenes() -> Vec<(&'static str, CpuScene)> {
    vec![
        ("soup", scenes::soup(2000, 1.5, 1)),
        ("slivers", scenes::soup(1500, 0.05, 2)),
        ("terrain", scenes::terrain(40, 3)),
        ("instanced", scenes::instanced(4)),
        ("masked", scenes::masked()),
        ("cube_on_plane", scenes::asset("cube_on_plane.gltf")),
        ("cube_textured", scenes::asset("cube_textured.gltf")),
    ]
}

/// Every hit and miss equals the brute-force one bit for bit, and any-hit agrees with it.
fn assert_matches_brute_force(name: &str, scene: &TraceScene, count: usize) -> usize {
    let mut hits = 0;
    for (i, ray) in rays(scene, count, 77).iter().enumerate() {
        let expected = scene.brute_force_closest(ray);
        assert_eq!(
            scene.trace_closest(ray),
            expected,
            "{name}: ray {i} {ray:?}"
        );
        assert_eq!(
            scene.trace_any(ray),
            expected.is_some(),
            "{name}: any-hit of ray {i}"
        );
        hits += usize::from(expected.is_some());
    }
    hits
}

#[test]
fn closest_hits_equal_brute_force_on_random_rays_over_every_scene() {
    for (name, cpu) in named_scenes() {
        let scene = build(cpu);
        let hits = assert_matches_brute_force(name, &scene, 1000);
        assert!(
            hits > 200,
            "{name}: only {hits} of 1000 rays hit, too few to mean anything"
        );
    }
}

#[test]
fn repeated_meshes_trace_as_instances_of_one_mesh() {
    let scene = build(scenes::instanced(4));
    assert_eq!(scene.meshes.len(), 2, "the merged soup and one cube");
    assert_eq!(scene.instances.len(), 31);
    assert_eq!(scene.stats.unique_triangles, 200 + 12);
    assert_eq!(scene.stats.triangles, 200 + 30 * 12);
    assert_eq!(
        scene.instances.iter().filter(|i| i.flags & 1 != 0).count(),
        1,
        "one mirrored cube"
    );
}

#[test]
fn a_masked_cut_out_lets_rays_through_and_its_solid_half_stops_them() {
    let scene = build(scenes::masked());
    let ray = |x: f32| TraceRay {
        origin: Vec3::new(x, 0.3, 5.0),
        tmin: 0.0,
        direction: Vec3::NEG_Z,
        tmax: FAR,
    };
    let solid = scene.trace_closest(&ray(0.5)).expect("the solid half");
    assert!(
        (solid.t - 4.0).abs() < 1e-5,
        "the front card at z = 1: {}",
        solid.t
    );
    let through = scene.trace_closest(&ray(-0.5)).expect("the wall behind");
    assert!(
        (through.t - 6.95).abs() < 1e-4,
        "both cut-outs pass to the wall at z = -1.95: {}",
        through.t
    );
    let mut opaque = scenes::masked();
    for prim in &mut opaque.primitives {
        prim.material.alpha_mode = crate::scene::AlphaMode::Opaque;
    }
    let hit = build(opaque)
        .trace_closest(&ray(-0.5))
        .expect("an opaque card");
    assert!(
        (hit.t - 4.0).abs() < 1e-5,
        "OPAQUE ignores the texture's alpha: {}",
        hit.t
    );
}

#[test]
fn a_root_transform_moves_every_hit_and_keeps_the_meshes() {
    let mut scene = build(scenes::instanced(5));
    let (blas_nodes, bounds) = (
        scene.blas_node_count,
        scene.nodes[scene.tlas_root() as usize].bounds(),
    );
    let probe = rays(&scene, 400, 9);
    let hits = |scene: &TraceScene, shift: Vec3| {
        let moved = |ray: &TraceRay| TraceRay {
            origin: ray.origin + shift,
            ..*ray
        };
        probe
            .iter()
            .filter(|ray| scene.trace_closest(&moved(ray)).is_some())
            .count()
    };
    let before = hits(&scene, Vec3::ZERO);
    let shift = Vec3::new(3.0, -1.0, 2.0);
    scene.set_root_transform(Mat4::from_translation(shift));
    assert_eq!(scene.blas_node_count, blas_nodes, "the meshes stay");
    let moved = scene.nodes[scene.tlas_root() as usize].bounds();
    assert!((Vec3::from(moved.min - bounds.min) - shift).length() < 1e-3);
    let after = hits(&scene, shift);
    assert!(
        before.abs_diff(after) <= 2,
        "{before} hits before, {after} after the move"
    );
    assert!(
        hits(&scene, Vec3::ZERO) != before,
        "unmoved rays see the scene elsewhere"
    );
    assert_matches_brute_force("moved", &scene, 400);
}

#[test]
fn moving_an_instanced_primitive_moves_its_hits_and_leaves_its_twin() {
    let cube = |x: f32| scenes::cube_primitive(Mat4::from_translation(Vec3::new(x, 0.0, 0.0)));
    let source = CpuScene {
        primitives: vec![cube(0.0), cube(5.0)],
        ..CpuScene::default()
    };
    let mut scene = build(source);
    assert_eq!(
        (scene.meshes.len(), scene.instances.len()),
        (1, 2),
        "one cube, twice"
    );
    let down = |x: f32| TraceRay {
        origin: Vec3::new(x, 10.0, 0.0),
        tmin: 0.0,
        direction: Vec3::NEG_Y,
        tmax: FAR,
    };
    let lifted = Mat4::from_translation(Vec3::new(5.0, 3.0, 0.0));
    scene.set_primitive_transforms(&[Mat4::IDENTITY, lifted]);
    let t = |x: f32| scene.trace_closest(&down(x)).map_or(f32::NAN, |hit| hit.t);
    assert!(
        (t(0.0) - 9.5).abs() < 1e-5,
        "the first cube stays: {}",
        t(0.0)
    );
    assert!(
        (t(5.0) - 6.5).abs() < 1e-5,
        "the second is 3 higher: {}",
        t(5.0)
    );
    assert_eq!(
        scene.trace_closest(&down(5.0)),
        scene.brute_force_closest(&down(5.0))
    );
}

#[test]
fn a_stepped_build_traces_like_a_threaded_one() {
    let source = Arc::new(scenes::instanced(6));
    let threaded = TraceSceneBuilder::new(
        Arc::clone(&source),
        Vec::new(),
        TraceBuildOptions::default(),
    )
    .run()
    .expect("finishes");
    let mut builder = TraceSceneBuilder::new(
        source,
        Vec::new(),
        TraceBuildOptions {
            threads: 1,
            ..TraceBuildOptions::default()
        },
    );
    let progress = builder.progress();
    let mut steps = 0;
    let stepped = loop {
        steps += 1;
        if let Some(scene) = builder.step(64) {
            break scene;
        }
        assert!(progress.snapshot().fraction <= 1.0);
    };
    assert!(steps > 5, "{steps} steps");
    assert_eq!(stepped.triangles, threaded.triangles, "the same leaf order");
    for ray in rays(&stepped, 300, 12) {
        assert_eq!(stepped.trace_closest(&ray), threaded.trace_closest(&ray));
    }
}

#[test]
fn explicit_instances_add_copies_like_set_instances() {
    let scene = scenes::soup(10, 1.0, 8);
    let copies = vec![vec![
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0)),
    ]];
    let traced = TraceSceneBuilder::new(Arc::new(scene), copies, TraceBuildOptions::default())
        .run()
        .expect("finishes");
    assert_eq!(traced.instances.len(), 2);
    let ray = TraceRay {
        origin: Vec3::new(40.0, 0.0, 30.0),
        tmin: 0.0,
        direction: Vec3::NEG_Z,
        tmax: FAR,
    };
    assert_eq!(traced.trace_closest(&ray), traced.brute_force_closest(&ray));
}

/// A tessellated sphere of about `triangles` triangles.
fn sphere(triangles: usize) -> CpuScene {
    let rings = ((triangles / 2) as f32).sqrt() as u32;
    let mut vertices = Vec::new();
    for ring in 0..=rings {
        let theta = std::f32::consts::PI * ring as f32 / rings as f32;
        for seg in 0..=rings {
            let phi = std::f32::consts::TAU * seg as f32 / rings as f32;
            let p = Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            ) * 10.0;
            vertices.push(scenes::vertex(
                p,
                [seg as f32 / rings as f32, ring as f32 / rings as f32],
            ));
        }
    }
    let row = rings + 1;
    let indices = (0..rings)
        .flat_map(|r| (0..rings).map(move |s| (r, s)))
        .flat_map(|(r, s)| {
            let i = r * row + s;
            [i, i + row, i + 1, i + 1, i + row, i + row + 1]
        })
        .collect();
    CpuScene {
        primitives: vec![scenes::primitive(vertices, indices, Mat4::IDENTITY)],
        ..CpuScene::default()
    }
}

/// The build's rate on one large mesh, unoptimised test builds at a tenth; riscv64 runs under QEMU, whose clock says nothing.
#[test]
fn a_large_mesh_builds_within_its_budget() {
    let release = !cfg!(debug_assertions);
    let (triangles, seconds) = if release {
        (2_000_000, 4.0)
    } else {
        (300_000, 10.0)
    };
    let seconds = if cfg!(target_arch = "riscv64") {
        f64::INFINITY
    } else {
        seconds
    };
    let source = Arc::new(sphere(triangles));
    let count = source.triangle_count();
    let start = std::time::Instant::now();
    let scene = TraceSceneBuilder::new(source, Vec::new(), TraceBuildOptions::default())
        .run()
        .expect("finishes");
    let elapsed = start.elapsed().as_secs_f64();
    eprintln!(
        "BUILD: {count} triangles in {elapsed:.2} s, {} nodes, {:.1} M triangles/s",
        scene.nodes.len(),
        count as f64 / elapsed / 1e6
    );
    assert!(
        elapsed < seconds,
        "{count} triangles took {elapsed:.2} s, budget {seconds} s"
    );
    assert!(scene.nodes.len() < 2 * count);
}

/// Every inner node of `scene`'s mesh `mesh` bounds both children, and every leaf its triangles.
fn assert_mesh_bounds(scene: &TraceScene, mesh: usize) {
    let info = &scene.meshes[mesh];
    for index in info.root..info.root + info.node_count {
        let node = scene.nodes[index as usize];
        let mut inside = Aabb::EMPTY;
        if node.is_leaf() {
            for tri in &scene.triangles[node.first as usize..(node.first + node.count) as usize] {
                for &corner in &tri[..3] {
                    inside.grow_point(Vec3A::from_slice(&scene.positions[corner as usize][..3]));
                }
            }
        } else {
            inside = scene.nodes[node.first as usize].bounds();
            inside.grow(&scene.nodes[node.first as usize + 1].bounds());
        }
        assert!(
            node.bounds().contains(&inside),
            "node {index} of mesh {mesh}"
        );
    }
}

/// An animated skin is its own mesh: bent, it is re-posed, refitted and still traces as brute force.
#[test]
fn a_bent_skin_is_refitted_and_traces_like_brute_force() {
    let source = scenes::asset("skinned_bar.gltf");
    let mut scene = build(source.clone());
    assert_eq!(
        scene.deformed.len(),
        1,
        "the animated skin is a mesh of its own"
    );
    let mesh = scene.deformed[0].mesh as usize;
    let rest = scene.meshes[mesh].bounds;
    let mut nodes = source.nodes.clone();
    nodes[2].rotation = Quat::from_rotation_z(1.0);
    let world = CpuScene::compute_world_transforms(&nodes);
    assert_eq!(scene.pose_deformed(&world, &[]), vec![mesh as u32]);
    assert!(
        scene.pose_deformed(&world, &[]).is_empty(),
        "a held pose re-poses nothing"
    );
    let bent = scene.meshes[mesh].bounds;
    assert!(
        bent.max.x - bent.min.x > rest.max.x - rest.min.x + 0.5,
        "the bend did not widen the mesh: {rest:?} -> {bent:?}"
    );
    assert_mesh_bounds(&scene, mesh);
    let hits = assert_matches_brute_force("bent bar", &scene, 1000);
    assert!(hits > 200, "only {hits} of 1000 rays hit");
}

/// glTF ignores a skinned mesh's node transform: moving that node re-poses no unweighted vertex.
#[test]
fn a_moved_skinned_node_leaves_its_unweighted_vertices_in_place() {
    let mut source = scenes::asset("skinned_bar.gltf");
    for v in &mut source.primitives[0].vertices {
        v.weights = [0.0; 4];
    }
    let stored: Vec<Vec3A> = source.primitives[0]
        .vertices
        .iter()
        .map(|v| Vec3A::from_array(v.position))
        .collect();
    let own = Aabb::from_points(&stored);
    let mut scene = build(source.clone());
    let mesh = scene.deformed[0].mesh as usize;
    let mut nodes = source.nodes.clone();
    nodes[0].translation = Vec3::new(40.0, 0.0, 0.0);
    nodes[2].rotation = Quat::from_rotation_z(1.0);
    let world = CpuScene::compute_world_transforms(&nodes);
    scene.pose_deformed(&world, &[]);
    let moved = scene.meshes[mesh].bounds;
    assert!(
        (moved.max - own.max).abs().max_element() < 1e-4
            && (moved.min - own.min).abs().max_element() < 1e-4,
        "the bar left its own space for its node's: {own:?} -> {moved:?}"
    );
}

/// A skin no animation moves keeps merging into the world-space mesh at its first pose.
#[test]
fn a_still_skin_merges_at_its_first_pose() {
    let mut source = scenes::asset("skinned_bar.gltf");
    source.animations.clear();
    let scene = build(source);
    assert!(scene.deformed.is_empty());
    assert_eq!(scene.meshes.len(), 1, "only the merged mesh");
}

/// A morphed mesh whose weights an animation drives is re-posed by the weights it is handed.
#[test]
fn animated_morph_weights_re_pose_their_mesh() {
    let mut source = scenes::asset("cube_morph.gltf");
    source.animations.push(CpuAnimation {
        name: "breathe".to_owned(),
        duration: 1.0,
        channels: vec![CpuAnimationChannel {
            node: 0,
            times: vec![0.0],
            values: ChannelValues::MorphWeights(vec![1.0]),
            interpolation: Interpolation::Linear,
        }],
    });
    let world = CpuScene::compute_world_transforms(&source.nodes);
    let mut scene = build(source);
    assert_eq!(
        scene.deformed.len(),
        1,
        "the morphed cube is a mesh of its own"
    );
    let mesh = scene.deformed[0].mesh as usize;
    let top = |scene: &TraceScene| scene.meshes[mesh].bounds.max.y;
    let full = top(&scene);
    assert_eq!(scene.pose_deformed(&world, &[&[0.0]]), vec![mesh as u32]);
    assert!(top(&scene) < full - 0.5, "{full} -> {}", top(&scene));
    assert_mesh_bounds(&scene, mesh);
    assert_matches_brute_force("morphed cube", &scene, 500);
}

/// A new instance count rebuilds the top level over the kept meshes; a merged primitive has no instances to change.
#[test]
fn a_new_instance_count_rebuilds_the_top_level_and_keeps_the_meshes() {
    let at = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
    let mut cubes = CpuScene {
        primitives: vec![
            scenes::cube_primitive(Mat4::IDENTITY),
            scenes::cube_primitive(at(-9.0)),
        ],
        ..CpuScene::default()
    };
    cubes.primitives[1].vertices[0].position[1] += 0.25;
    let explicit = vec![vec![at(0.0), at(3.0)]];
    let mut scene = TraceSceneBuilder::new(Arc::new(cubes), explicit, TraceBuildOptions::default())
        .run()
        .expect("finishes");
    let (meshes, blas) = (scene.meshes.clone(), scene.blas_node_count);
    assert_eq!(
        scene.instances.len(),
        3,
        "the merged cube and two instances"
    );
    let down = |x: f32| TraceRay {
        origin: Vec3::new(x, 10.0, 0.0),
        tmin: 0.0,
        direction: Vec3::NEG_Y,
        tmax: FAR,
    };
    assert!(scene.trace_closest(&down(6.0)).is_none());
    let three = [at(0.0), at(3.0), at(6.0)];
    assert_eq!(
        scene.set_instance_extras(0, &three),
        InstanceChange::Recounted
    );
    assert_eq!(scene.instances.len(), 4);
    assert_eq!(
        (scene.meshes.clone(), scene.blas_node_count),
        (meshes, blas),
        "the meshes stay"
    );
    assert!(
        scene.trace_closest(&down(6.0)).is_some(),
        "the third cube traces"
    );
    assert_matches_brute_force("three cubes", &scene, 400);
    let moved = [at(0.0), at(3.0), at(12.0)];
    assert_eq!(scene.set_instance_extras(0, &moved), InstanceChange::Moved);
    assert!(
        scene.trace_closest(&down(6.0)).is_none() && scene.trace_closest(&down(12.0)).is_some()
    );
    assert_eq!(scene.set_instance_extras(0, &[]), InstanceChange::Recounted);
    assert_eq!(scene.instances.len(), 2, "no transforms: drawn once");
    let before = scene.instances.clone();
    assert_eq!(
        scene.set_instance_extras(1, &three),
        InstanceChange::Refused
    );
    assert_eq!(scene.instances, before, "the merged cube stays");
}
