//! The tracer from outside: `bvh_traverse.slang` agrees with the Rust layout, and a traced mode builds the scene on first use.

use std::sync::Arc;
use std::time::Duration;

use kataglyphis_webgpu_renderer::render::tracer::geometry::{TRIANGLE_MASKED, TRIANGLE_MATERIAL};
use kataglyphis_webgpu_renderer::render::tracer::material::{flags, GpuMaterial, NO_LAYER};
use kataglyphis_webgpu_renderer::render::tracer::query::STACK_SIZE;
use kataglyphis_webgpu_renderer::render::tracer::scene::{GpuInstance, INSTANCE_MIRRORED};
use kataglyphis_webgpu_renderer::render::tracer::{RenderMode, TraceStatus, TRACE_SCENE_GROUP};
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

const TRAVERSE: &str = include_str!("../../../shaders/tracer/bvh_traverse.slang");
const DEBUG_WGSL: &str = include_str!("../src/shaders/trace_debug.wgsl");

/// The value of `static const <type> <name> = <value>;` in `bvh_traverse.slang`.
fn constant(name: &str) -> u64 {
    let line = TRAVERSE
        .lines()
        .find(|line| line.starts_with("static const") && line.contains(&format!(" {name} = ")))
        .unwrap_or_else(|| panic!("bvh_traverse.slang declares no `{name}`"));
    let value = line
        .split('=')
        .nth(1)
        .expect("an initialiser")
        .trim()
        .trim_end_matches(';');
    let digits = value.trim_end_matches('u');
    match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).expect("a hex literal"),
        None => digits
            .parse()
            .unwrap_or_else(|_| panic!("`{name}` = {value} is no integer")),
    }
}

#[test]
fn the_slang_constants_match_the_rust_layout() {
    let pairs: [(&str, u64); 15] = [
        ("TRACE_STACK_SIZE", STACK_SIZE as u64),
        ("NODE_STRIDE", 2),
        ("INSTANCE_STRIDE", GpuInstance::RECORDS as u64),
        ("MATERIAL_STRIDE", GpuMaterial::RECORDS as u64),
        ("TRACE_NO_LAYER", u64::from(NO_LAYER)),
        ("TRIANGLE_MASKED", u64::from(TRIANGLE_MASKED)),
        ("TRIANGLE_MATERIAL", u64::from(TRIANGLE_MATERIAL)),
        ("INSTANCE_MIRRORED", u64::from(INSTANCE_MIRRORED)),
        ("ALPHA_MODE_MASK", u64::from(flags::ALPHA_MODE_MASK)),
        ("ALPHA_MASK", u64::from(flags::ALPHA_MASK)),
        ("MATERIAL_DOUBLE_SIDED", u64::from(flags::DOUBLE_SIDED)),
        ("MATERIAL_UNLIT", u64::from(flags::UNLIT)),
        ("MATERIAL_UV1_SHIFT", u64::from(flags::UV1_SHIFT)),
        ("MATERIAL_WRAP_SHIFT", u64::from(flags::WRAP_SHIFT)),
        ("WRAP_CLAMP", u64::from(flags::WRAP_CLAMP)),
    ];
    for (name, rust) in pairs {
        assert_eq!(constant(name), rust, "{name}");
    }
    assert_eq!(constant("WRAP_MIRROR"), u64::from(flags::WRAP_MIRROR));
}

#[test]
fn the_scene_binds_at_its_group_in_the_emitted_wgsl() {
    let names = [
        "tracerRecords",
        "tracerTriangles",
        "tracerPositions",
        "tracerVertices",
        "tracerInfo",
    ];
    for (binding, name) in names.iter().enumerate() {
        let declared = format!("@binding({binding}) @group({TRACE_SCENE_GROUP}) var<");
        let line = DEBUG_WGSL
            .lines()
            .find(|line| line.contains(&format!(" {name}_0 ")))
            .unwrap_or_else(|| panic!("trace_debug.wgsl declares no {name}"));
        assert!(line.starts_with(&declared), "{line}");
    }
    assert!(
        !DEBUG_WGSL.contains("textureSample("),
        "compute has no derivatives: only textureSampleLevel"
    );
    assert!(
        !DEBUG_WGSL.contains("workgroupBarrier"),
        "no barrier, so no uniformity rule to break in Tint"
    );
}

fn renderer_with_cube(gpu: &GpuContext) -> ForwardRenderer {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let scene = load_gltf(&path).expect("the test scene loads");
    let mut renderer = ForwardRenderer::new(gpu, 64, 64);
    renderer.upload_scene(gpu, &scene);
    renderer.set_trace_source(Arc::new(scene));
    renderer
}

#[test]
fn a_traced_mode_builds_the_scene_once_and_raster_never_does() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_with_cube(&gpu);
    let camera = OrbitCamera::default();
    renderer
        .render_to_pixels(&gpu, 64, 64, &camera)
        .expect("renders");
    assert_eq!(
        renderer.trace_status(),
        TraceStatus::Idle,
        "raster frames build nothing"
    );
    renderer.render_mode = RenderMode::PathTraced;
    renderer
        .render_to_pixels(&gpu, 64, 64, &camera)
        .expect("renders");
    assert!(
        matches!(
            renderer.trace_status(),
            TraceStatus::Building(_) | TraceStatus::Ready(_)
        ),
        "{:?}",
        renderer.trace_status()
    );
    assert!(renderer
        .tracer_mut()
        .finish_build(&gpu, Duration::from_secs(120)));
    let TraceStatus::Ready(stats) = renderer.trace_status() else {
        panic!("{:?}", renderer.trace_status());
    };
    assert!(stats.triangles >= 12 && stats.instances >= 1, "{stats:?}");
    assert!(renderer.tracer().gpu_scene().is_some());
    renderer
        .render_to_pixels(&gpu, 64, 64, &camera)
        .expect("renders");
    assert!(
        matches!(renderer.trace_status(), TraceStatus::Ready(_)),
        "it builds once"
    );
    let scene =
        load_gltf(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf"))
            .expect("loads");
    renderer.upload_scene(&gpu, &scene);
    assert_eq!(
        renderer.trace_status(),
        TraceStatus::Idle,
        "a new scene drops the old traced one"
    );
}

#[test]
fn a_scene_upload_without_a_trace_source_leaves_the_traced_modes_idle() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_with_cube(&gpu);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    renderer.upload_scene(&gpu, &load_gltf(&path).expect("loads"));
    renderer.render_mode = RenderMode::RayTraced;
    renderer
        .render_to_pixels(&gpu, 32, 32, &OrbitCamera::default())
        .expect("renders");
    assert_eq!(renderer.trace_status(), TraceStatus::Idle);
    assert!(!renderer
        .tracer_mut()
        .finish_build(&gpu, Duration::from_secs(1)));
}
