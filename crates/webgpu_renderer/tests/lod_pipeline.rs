//! LOD on the render path: every test asserts the index count the draw loop uses, never a config bool.

use glam::Vec3;
use kataglyphis_webgpu_renderer::{
    build_lod_chain_with, load_gltf, ForwardRenderer, GpuContext, OrbitCamera, Simplifier,
};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf")
}

/// The bundled cube uploaded into a renderer with LOD in the requested state.
fn renderer_with_lod(gpu: &GpuContext, enabled: bool) -> ForwardRenderer {
    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(gpu, 128, 128);
    renderer.lod_enabled = enabled;
    // For a two-unit cube these bracket "up close" and "across the room".
    renderer.lod_switch_distances = vec![8.0, 24.0];
    renderer.upload_scene(gpu, &scene);
    renderer
}

#[test]
fn chains_are_built_at_upload_and_not_per_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let renderer = renderer_with_lod(&gpu, true);

    // No frame rendered yet: levels must not be simplified inside the frame.
    assert_eq!(
        renderer.lod_level_count(0),
        2,
        "both levels must exist immediately after upload"
    );

    let full = renderer
        .selected_index_count(0, Vec3::ZERO)
        .expect("primitive 0 exists");
    let level_0 = renderer.lod_level_index_count(0, 0).unwrap();
    let level_1 = renderer.lod_level_index_count(0, 1).unwrap();
    assert!(
        level_0 < full && level_1 < level_0,
        "levels must get strictly coarser: full {full}, l0 {level_0}, l1 {level_1}"
    );
}

#[test]
fn a_distant_primitive_draws_strictly_fewer_indices() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let renderer = renderer_with_lod(&gpu, true);

    // The cube sits at the origin, so the eye position IS the distance.
    let near = renderer
        .selected_index_count(0, Vec3::new(0.0, 0.0, 3.0))
        .unwrap();
    let middle = renderer
        .selected_index_count(0, Vec3::new(0.0, 0.0, 12.0))
        .unwrap();
    let far = renderer
        .selected_index_count(0, Vec3::new(0.0, 0.0, 60.0))
        .unwrap();

    assert!(
        far < middle && middle < near,
        "index count must fall with distance: near {near}, middle {middle}, far {far}"
    );
    assert_eq!(near % 3, 0, "the drawn range must stay whole triangles");
    assert_eq!(far % 3, 0, "the drawn range must stay whole triangles");
}

#[test]
fn lod_disabled_draws_full_detail_at_every_distance() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let renderer = renderer_with_lod(&gpu, false);

    assert_eq!(
        renderer.lod_level_count(0),
        0,
        "disabled LOD must not build or upload any levels"
    );

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let full = scene.primitives[0].indices.len() as u32;
    for distance in [0.5f32, 3.0, 12.0, 60.0, 10_000.0] {
        assert_eq!(
            renderer
                .selected_index_count(0, Vec3::new(0.0, 0.0, distance))
                .unwrap(),
            full,
            "with LOD off the full-detail buffer must be drawn at distance {distance}"
        );
    }
}

#[test]
fn an_lod_frame_still_renders() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_with_lod(&gpu, true);

    // A level's buffers with another level's index count draw out of range; wgpu catches it.
    let pixels = renderer
        .render_to_pixels(&gpu, 128, 128, &OrbitCamera::default())
        .expect("a frame with LOD enabled must render");
    assert_eq!(pixels.len(), 128 * 128 * 4);
    assert!(
        pixels.as_chunks::<4>().0.iter().any(|p| p[0] != pixels[0]),
        "the LOD frame came out uniformly flat"
    );
}

#[test]
fn morphed_primitives_are_excluded_from_lod() {
    // Only the full-res buffer is re-blended, so an LOD level would pop to the rest pose.
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let morph_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_morph.gltf");
    let scene = load_gltf(&morph_path).expect("cube_morph.gltf must load");
    assert_eq!(
        scene.primitives[0].morph_targets.len(),
        1,
        "the fixture must actually carry a morph target"
    );

    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.lod_enabled = true;
    renderer.lod_switch_distances = vec![8.0, 24.0];
    renderer.upload_scene(&gpu, &scene);

    assert_eq!(
        renderer.lod_level_count(0),
        0,
        "a morphed primitive must not build LOD levels even with LOD enabled"
    );

    let full = scene.primitives[0].indices.len() as u32;
    for distance in [0.5f32, 12.0, 60.0, 10_000.0] {
        assert_eq!(
            renderer
                .selected_index_count(0, Vec3::new(0.0, 0.0, distance))
                .unwrap(),
            full,
            "a morphed primitive must draw full detail at distance {distance}"
        );
    }
}

#[test]
fn quadric_level_zero_differs_from_full_detail() {
    // Clustering at 0.02 leaves a low-poly mesh unchanged; Quadric's budget halves it regardless.
    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let prim = &scene.primitives[0];
    let full_triangles = prim.indices.len() / 3;

    let clustered = build_lod_chain_with(prim, &[8.0], Simplifier::VertexClustering);
    assert_eq!(
        clustered[0].primitive.indices.len() / 3,
        full_triangles,
        "documenting the measured no-op: clustering at 0.02 does not touch a \
         low-poly mesh, which is why the render path uses Quadric"
    );

    let quadric = build_lod_chain_with(prim, &[8.0], Simplifier::Quadric);
    let level_0 = quadric[0].primitive.indices.len() / 3;
    assert!(
        level_0 < full_triangles,
        "quadric level 0 must actually simplify: {full_triangles} -> {level_0}"
    );
}

/// `world_center` drives LOD and transparent sorting, so a no-op animation must not move it.
#[test]
fn lod_selection_is_stable_across_a_no_op_animation_update() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_with_lod(&gpu, true);

    // Sample a spread of distances so at least one sits near a switch boundary.
    let eyes = [
        Vec3::new(0.0, 0.0, 3.0),
        Vec3::new(0.0, 0.0, 8.0),
        Vec3::new(0.0, 0.0, 12.0),
        Vec3::new(0.0, 0.0, 24.0),
        Vec3::new(0.0, 0.0, 60.0),
    ];
    let before: Vec<u32> = eyes
        .iter()
        .map(|e| renderer.selected_index_count(0, *e).unwrap())
        .collect();

    // Advancing to t=0 changes no pose whatsoever.
    renderer.set_animation_time(0.0);

    let after: Vec<u32> = eyes
        .iter()
        .map(|e| renderer.selected_index_count(0, *e).unwrap())
        .collect();
    assert_eq!(
        before, after,
        "a no-op animation update must not change LOD selection"
    );
}
