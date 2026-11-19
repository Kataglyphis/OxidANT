//! The tracer from outside: `bvh_traverse.slang` agrees with the Rust layout, and a traced mode builds the scene on first use.

use std::sync::Arc;
use std::time::Duration;

use kataglyphis_webgpu_renderer::render::tracer::geometry::{TRIANGLE_MASKED, TRIANGLE_MATERIAL};
use kataglyphis_webgpu_renderer::render::tracer::limits::TraceBufferSizes;
use kataglyphis_webgpu_renderer::render::tracer::material::{flags, GpuMaterial, NO_LAYER};
use kataglyphis_webgpu_renderer::render::tracer::query::{TraceRay, STACK_SIZE};
use kataglyphis_webgpu_renderer::render::tracer::scene::{GpuInstance, INSTANCE_MIRRORED};
use kataglyphis_webgpu_renderer::render::tracer::{
    RenderMode, TraceDebugPass, TraceStage, TraceStats, TraceStatus, TRACE_SCENE_GROUP,
};
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
    let pairs: [(&str, u64); 14] = [
        ("TRACE_STACK_SIZE", STACK_SIZE as u64),
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
        (0, "tracerRecords"),
        (1, "tracerTriangles"),
        (2, "tracerPositions"),
        (3, "tracerVertices"),
        (4, "tracerInfo"),
        (9, "tracerNodeMax"),
        (10, "tracerVertexExtras"),
    ];
    for (binding, name) in names {
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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets/cube_on_plane.gltf");
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
    let scene = load_gltf(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf"),
    )
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
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf");
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

/// Member offsets and size of the struct `global` is declared with in `wgsl`, as naga lays them out.
fn wgsl_layout(wgsl: &str, global: &str) -> (Vec<u32>, u32) {
    let module = naga::front::wgsl::parse_str(wgsl).expect("parses");
    let (_, var) = module
        .global_variables
        .iter()
        .find(|(_, var)| var.name.as_deref() == Some(global))
        .unwrap_or_else(|| panic!("no global {global}"));
    let naga::TypeInner::Struct { members, span } = &module.types[var.ty].inner else {
        panic!("{global} is no struct");
    };
    (members.iter().map(|m| m.offset).collect(), *span)
}

#[test]
fn the_traced_kernels_read_the_rust_uniforms_and_workgroups() {
    use kataglyphis_webgpu_renderer::render::tracer::modes::{
        TraceFrameUniforms, TraceTileUniforms, KERNEL_GROUP_SIZE, TRACE_LIGHTS_GROUP,
    };
    use std::mem::{offset_of, size_of};
    let traced = include_str!("../src/shaders/traced.wgsl");
    let frame = (
        vec![
            offset_of!(TraceFrameUniforms, size) as u32,
            offset_of!(TraceFrameUniforms, inv_view_proj) as u32,
            offset_of!(TraceFrameUniforms, light_dir) as u32,
            offset_of!(TraceFrameUniforms, light_radiance) as u32,
            offset_of!(TraceFrameUniforms, furnace) as u32,
            offset_of!(TraceFrameUniforms, view_proj) as u32,
            offset_of!(TraceFrameUniforms, lights) as u32,
        ],
        size_of::<TraceFrameUniforms>() as u32,
    );
    assert_eq!(wgsl_layout(traced, "traceFrame_0"), frame);
    assert_eq!(
        wgsl_layout(traced, "traceTile_0"),
        (vec![0, 16], size_of::<TraceTileUniforms>() as u32)
    );
    for (binding, name) in ["tracerLights", "tracerEnvironment"].iter().enumerate() {
        let declared = format!("@binding({binding}) @group({TRACE_LIGHTS_GROUP}) var");
        assert!(
            traced
                .lines()
                .any(|line| line.starts_with(&declared) && line.contains(&format!(" {name}_0 "))),
            "traced.wgsl declares no {name} at {declared}"
        );
    }
    // The resolve binds the frame's buffer and reads its first field only.
    let resolve = include_str!("../src/shaders/trace_resolve.wgsl");
    assert_eq!(wgsl_layout(resolve, "resolveParams_0"), (vec![0], 16));
    let group = format!("@workgroup_size({KERNEL_GROUP_SIZE}, {KERNEL_GROUP_SIZE}, 1)");
    assert_eq!(
        traced.matches(&group).count(),
        2,
        "both kernels are {group}"
    );
}

/// The calls to `callee` that inlining every call in `block` copies, as SwiftShader compiles a kernel.
fn inlined_calls(module: &naga::Module, block: &naga::Block, callee: &str) -> usize {
    use naga::Statement;
    block
        .iter()
        .map(|statement| match statement {
            Statement::Call { function, .. } => {
                let called = &module.functions[*function];
                if called.name.as_deref() == Some(callee) {
                    1
                } else {
                    inlined_calls(module, &called.body, callee)
                }
            }
            Statement::Block(inner) => inlined_calls(module, inner, callee),
            Statement::If { accept, reject, .. } => {
                inlined_calls(module, accept, callee) + inlined_calls(module, reject, callee)
            }
            Statement::Switch { cases, .. } => cases
                .iter()
                .map(|case| inlined_calls(module, &case.body, callee))
                .sum(),
            Statement::Loop {
                body, continuing, ..
            } => inlined_calls(module, body, callee) + inlined_calls(module, continuing, callee),
            _ => 0,
        })
        .sum()
}

/// SwiftShader inlines a whole traversal per call site, so each kernel traces all its shadow rays from one.
#[test]
fn each_traced_kernel_has_one_shadow_ray_call_site() {
    let module =
        naga::front::wgsl::parse_str(include_str!("../src/shaders/traced.wgsl")).expect("parses");
    for kernel in ["cs_ray_trace", "cs_path_trace"] {
        let entry = module
            .entry_points
            .iter()
            .find(|entry| entry.name == kernel)
            .unwrap_or_else(|| panic!("traced.wgsl has no {kernel}"));
        assert_eq!(
            inlined_calls(&module, &entry.function.body, "trace_any_0"),
            1,
            "{kernel} must reach trace_any from exactly one call site, a loop over its lights"
        );
    }
}

/// Rays from `eye` towards a 40 x 40 grid over the far face of the box.
fn rays_through(eye: glam::Vec3, (min, max): (glam::Vec3, glam::Vec3)) -> Vec<TraceRay> {
    let mut rays = Vec::new();
    for i in 0..40 {
        for j in 0..40 {
            let (u, v) = ((i as f32 + 0.5) / 40.0, (j as f32 + 0.5) / 40.0);
            let target = glam::Vec3::new(
                min.x + (max.x - min.x) * u,
                min.y + (max.y - min.y) * v,
                max.z,
            );
            rays.push(TraceRay {
                origin: eye,
                tmin: 0.0,
                direction: target - eye,
                tmax: 2.0,
            });
        }
    }
    rays
}

/// The full Colosseum builds, fits the device and traces as on the CPU; on request only (`docs/testing.md`).
#[test]
#[ignore = "needs resources/scenes/Colosseum-gltf and about 3 GiB"]
fn the_full_colosseum_traces_on_this_device() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/scenes/Colosseum-gltf/Colosseum-gltf.gltf");
    let scene = load_gltf(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e:?}; scripts/linux/fetch-models.sh --scenes fetches it",
            path.display()
        )
    });
    let size = 160;
    let mut renderer = ForwardRenderer::new(&gpu, size, size);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_trace_source(Arc::new(scene));
    // The CPU traversal below reads the triangles the upload would otherwise drop.
    renderer.tracer_mut().options.keep_cpu_geometry = true;
    let built = renderer
        .tracer_mut()
        .finish_build(&gpu, Duration::from_secs(3600));
    assert!(built, "{:?}", renderer.trace_status());
    let traced = renderer.tracer().cpu_scene().expect("built");
    let binding = gpu.device.limits().max_storage_buffer_binding_size;
    eprintln!(
        "COLOSSEUM: {} nodes, {} instances, {} materials, {} triangles, {} vertices; {:?}; binding limit {binding}",
        traced.nodes.len(),
        traced.instances.len(),
        traced.materials.len(),
        traced.triangles.len(),
        traced.vertices.len(),
        TraceBufferSizes::of(traced)
    );
    let (min, max) = renderer.scene_bounds().expect("bounds");
    let eye = (min + max) * 0.5 + glam::Vec3::new(0.0, (max.y - min.y) * 0.5, min.z - max.z);
    let rays = rays_through(eye, (min, max));
    let pass = TraceDebugPass::new(&gpu.device);
    let gpu_scene = renderer.tracer().gpu_scene().expect("uploaded");
    let hits = pass.trace_closest(&gpu, gpu_scene, &rays).expect("traces");
    let (mut agree, mut hit) = (0, 0);
    for (ray, got) in rays.iter().zip(&hits) {
        let expected = traced.trace_closest(ray);
        hit += usize::from(expected.is_some());
        agree += usize::from(match expected {
            Some(e) => (got.instance, got.triangle) == (e.instance, e.triangle),
            None => !got.is_hit(),
        });
    }
    eprintln!("COLOSSEUM: {agree} of {} rays agree, {hit} hit", rays.len());
    // The ground spans kilometres, so most rays towards its box pass over the arena.
    assert!(hit >= 100, "only {hit} rays hit");
    assert!(agree * 100 >= rays.len() * 99, "only {agree} rays agree");
    let camera = OrbitCamera {
        target: (min + max) * 0.5,
        radius: (max - min).length() * 0.7,
        far: (max - min).length() * 4.0,
        ..OrbitCamera::default()
    };
    renderer.render_mode = RenderMode::RayTraced;
    renderer
        .render_to_pixels(&gpu, size, size, &camera)
        .expect("renders");
    assert_eq!(renderer.drawn_mode(), RenderMode::RayTraced);
}

fn renderer_tracing(gpu: &GpuContext, asset: &str) -> ForwardRenderer {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets")
        .join(asset);
    let scene = load_gltf(&path).expect("the test scene loads");
    let mut renderer = ForwardRenderer::new(gpu, 64, 64);
    renderer.upload_scene(gpu, &scene);
    renderer.set_trace_source(Arc::new(scene));
    renderer
}

/// Frames until the traced scene is ready, and the upload's fraction on each frame it was uploading.
fn frames_to_ready(gpu: &GpuContext, renderer: &mut ForwardRenderer) -> Vec<f32> {
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let mut fractions = Vec::new();
    loop {
        let tracer = renderer.tracer_mut();
        tracer.advance(gpu, true);
        match tracer.status() {
            TraceStatus::Ready(_) => return fractions,
            TraceStatus::Building(progress) if progress.stage == TraceStage::Uploading => {
                assert!(tracer.gpu_scene().is_none(), "a half-uploaded scene traced");
                fractions.push(progress.fraction);
            }
            TraceStatus::Building(_) => std::thread::sleep(Duration::from_millis(2)),
            other => panic!("the build stopped: {other:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no traced scene in two minutes"
        );
    }
}

/// A small budget spreads the first upload over frames; the scene then traces as one upload does, without its CPU geometry.
#[test]
fn the_first_upload_spreads_over_frames_and_then_frees_the_cpu_geometry() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let asset = "cube_textured.gltf";
    let mut spread = renderer_tracing(&gpu, asset);
    spread.tracer_mut().options.upload_bytes_per_frame = 256;
    let fractions = frames_to_ready(&gpu, &mut spread);
    eprintln!(
        "uploaded over {} frames: {fractions:?}",
        fractions.len() + 1
    );
    assert!(
        fractions.len() >= 4,
        "the upload took {} frames",
        fractions.len() + 1
    );
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]) && fractions.iter().all(|&f| f < 1.0));

    let mut whole = renderer_tracing(&gpu, asset);
    whole.tracer_mut().options.keep_cpu_geometry = true;
    assert!(whole
        .tracer_mut()
        .finish_build(&gpu, Duration::from_secs(120)));
    let (released, kept) = (
        spread.tracer().cpu_scene().expect("ready"),
        whole.tracer().cpu_scene().expect("ready"),
    );
    assert!(released.geometry_released() && !kept.geometry_released());
    assert!(
        released.triangles.is_empty()
            && released.positions.is_empty()
            && released.vertices.is_empty()
    );
    assert!(released.srgb.layers.is_empty() && !kept.srgb.layers.is_empty());
    assert_eq!(
        released.nodes, kept.nodes,
        "the top level rebuilds from the nodes, so they stay"
    );
    let untimed = |stats: &TraceStats| TraceStats {
        build_seconds: None,
        stage_seconds: None,
        ..stats.clone()
    };
    assert_eq!(untimed(&released.stats), untimed(&kept.stats));

    let (min, max) = spread.scene_bounds().expect("bounds");
    let eye = (min + max) * 0.5 + glam::Vec3::new(0.3, 0.4, (max.z - min.z) * 3.0);
    let rays = rays_through(eye, (min - 0.2, max + 0.2));
    let pass = TraceDebugPass::new(&gpu.device);
    let traced = |renderer: &ForwardRenderer| {
        let scene = renderer.tracer().gpu_scene().expect("uploaded");
        pass.trace_closest(&gpu, scene, &rays).expect("traces")
    };
    let (from_spread, from_whole) = (traced(&spread), traced(&whole));
    let mut hits = 0;
    for ((ray, a), b) in rays.iter().zip(&from_spread).zip(&from_whole) {
        let expected = kept
            .trace_closest(ray)
            .map(|hit| (hit.instance, hit.triangle));
        hits += usize::from(expected.is_some());
        assert_eq!(
            (a.instance, a.triangle),
            (b.instance, b.triangle),
            "{ray:?}"
        );
        assert_eq!(
            a.is_hit().then_some((a.instance, a.triangle)),
            expected,
            "{ray:?}"
        );
    }
    assert!(
        hits > rays.len() / 4,
        "only {hits} of {} rays hit the cube",
        rays.len()
    );
}

/// A mesh an animation deforms is re-posed from the triangles and vertices, so those stay after the upload.
#[test]
fn a_scene_with_a_deformed_mesh_keeps_its_cpu_geometry() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_tracing(&gpu, "skinned_bar.gltf");
    assert!(renderer
        .tracer_mut()
        .finish_build(&gpu, Duration::from_secs(120)));
    let scene = renderer.tracer().cpu_scene().expect("ready");
    assert!(
        !scene.deformed.is_empty(),
        "skinned_bar.gltf must deform, or this test proves nothing"
    );
    assert!(!scene.geometry_released() && !scene.triangles.is_empty());
}
