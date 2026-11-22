//! Per-pass GPU timestamp queries against a real adapter; the arithmetic is unit-tested in-crate.

use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::gpu_timing::{GpuTiming, TimedPass};
use kataglyphis_webgpu_renderer::scene::{CpuLight, CpuLightKind, CpuScene};
use kataglyphis_webgpu_renderer::{
    load_gltf, ForwardRenderer, GpuContext, OrbitCamera, TonemapPass,
};

/// The same cube the golden tests use, so every pass has real work to do.
fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf")
}

/// The cube under a point light, whose cube shadow the light shadows pass draws.
fn lit_cube() -> CpuScene {
    let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
    scene.lights.push(CpuLight {
        kind: CpuLightKind::Point,
        color: [1.0; 3],
        intensity: 5.0,
        range: 0.0,
        position: [0.0, 3.0, 0.0],
        direction: [0.0, -1.0, 0.0],
        node: None,
    });
    scene
}

/// A `width` x `height` target, then `frames` frames of `renderer`, each waited for so its readback lands.
fn render_frames(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    (width, height): (u32, u32),
    frames: u32,
) {
    let camera = OrbitCamera::default();
    let mut tonemap = TonemapPass::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu_timing_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    for _ in 0..frames {
        renderer.render_tonemapped(gpu, &mut tonemap, &view, width, height, &camera);
        // The frame path never waits, so a headless test must let the map callbacks run.
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

/// The adapter offers timestamps; every CI adapter (lavapipe, WARP) does, so under REQUIRE_GPU a miss fails.
fn timestamps(gpu: &GpuContext) -> bool {
    if !gpu.supports_timestamps {
        assert!(
            !GpuContext::gpu_required(),
            "KATAGLYPHIS_REQUIRE_GPU is set but the adapter lacks TIMESTAMP_QUERY"
        );
        eprintln!("adapter has no TIMESTAMP_QUERY; skipping");
    }
    gpu.supports_timestamps
}

fn ms(timings: &[(&str, f32)], name: &str) -> Option<f32> {
    timings.iter().find(|(n, _)| *n == name).map(|(_, ms)| *ms)
}

/// The passes `path` records with occlusion culling, the histogram and the clouds on: all but the other path's and the traced modes'.
/// FXAA at its default, Always, filters both paths.
fn passes_of(path: RasterPath) -> Vec<TimedPass> {
    let other: &[TimedPass] = match path {
        RasterPath::Forward => &[TimedPass::GBuffer, TimedPass::DeferredLighting],
        RasterPath::Deferred => &[TimedPass::Forward],
    };
    TimedPass::ALL
        .into_iter()
        .filter(|pass| !other.contains(pass) && !pass.is_traced())
        .collect()
}

/// 64 frames of the lit cube on `path` with every optional pass on, then the averaged timings.
fn measured_timings(gpu: &GpuContext, path: RasterPath) -> Vec<(&'static str, f32)> {
    let mut renderer = ForwardRenderer::new(gpu, 256, 256);
    assert!(
        renderer.enable_gpu_timing(gpu),
        "the adapter reports TIMESTAMP_QUERY, so enabling must succeed"
    );
    renderer.upload_scene(gpu, &lit_cube());
    // Occlusion cull, the histogram and the clouds only record when enabled, and every pass must report.
    renderer.occlusion_queries_enabled = true;
    renderer.auto_exposure = true;
    renderer.clouds.enabled = true;
    // The deferred run times the half-resolution march and its upsample as one Clouds span, forward the full march.
    renderer.clouds.half_resolution = path == RasterPath::Deferred;
    renderer.raster_path = path;
    // Enough frames to clear the async readback latency and fill the averaging window.
    render_frames(gpu, &mut renderer, (256, 256), 64);
    renderer.gpu_timings_ms()
}

#[test]
fn every_pass_reports_a_finite_non_negative_duration() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !timestamps(&gpu) {
        return;
    }
    for path in RasterPath::ALL {
        let timings = measured_timings(&gpu, path);
        let expected = passes_of(path);
        assert_eq!(
            timings.len(),
            expected.len(),
            "{path:?}: not every pass reported: {timings:?}"
        );
        for (pass, (name, ms)) in expected.iter().zip(&timings) {
            assert_eq!(*name, pass.name(), "passes must report in record order");
            assert!(ms.is_finite(), "{name} reported a non-finite duration");
            assert!(*ms >= 0.0, "{name} reported a negative duration {ms}");
            // Over a second for a 256x256 cube means wrong tick scaling, not a slow GPU.
            assert!(*ms < 1000.0, "{name} reported an implausible {ms} ms");
            eprintln!("{path:?} {name}: {ms:.4} ms");
        }
        // Main has no stamps of its own: it spans the scopes that drew, gaps between them included.
        let main = ms(&timings, "Main").expect("Main reports");
        let nested: f32 = timings
            .iter()
            .filter(|(name, _)| {
                TimedPass::ALL
                    .iter()
                    .any(|p| p.name() == *name && p.nested_in().is_some())
            })
            .map(|(_, ms)| ms)
            .sum();
        assert!(
            main >= nested * (1.0 - 1e-4),
            "{path:?}: Main {main} under its nested scopes' {nested}"
        );
        if path == RasterPath::Forward {
            assert_eq!(
                Some(main),
                ms(&timings, "Forward"),
                "a lone nested scope is Main's whole span"
            );
        }
    }
}

/// The C++ engine's `PunctualShadows`: the light cubes time apart from the sun's cascades, each only while it draws.
#[test]
fn the_light_shadows_time_apart_from_the_cascades() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !timestamps(&gpu) {
        return;
    }
    let scene = lit_cube();
    let timed_with = |cascades: bool, cubes: bool| {
        let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
        assert!(renderer.enable_gpu_timing(&gpu));
        renderer.upload_scene(&gpu, &scene);
        renderer.shadows.enabled = cascades;
        renderer.shadows.punctual = cubes;
        render_frames(&gpu, &mut renderer, (64, 64), 40);
        assert_eq!(renderer.punctual_shadow_cubes(), usize::from(cubes));
        renderer.gpu_timings_ms()
    };
    let cubes_only = timed_with(false, true);
    assert!(
        ms(&cubes_only, "PunctualShadows").is_some(),
        "{cubes_only:?}"
    );
    assert_eq!(
        ms(&cubes_only, "ShadowCascades"),
        None,
        "the cube faces time as cascades: {cubes_only:?}"
    );
    let cascades_only = timed_with(true, false);
    assert!(
        ms(&cascades_only, "ShadowCascades").is_some(),
        "{cascades_only:?}"
    );
    assert_eq!(
        ms(&cascades_only, "PunctualShadows"),
        None,
        "{cascades_only:?}"
    );
}

#[test]
fn timings_stay_empty_until_enabled() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let renderer = ForwardRenderer::new(&gpu, 64, 64);
    // Default-off contract: an untimed renderer reports nothing, not zeroes.
    assert!(!renderer.gpu_timing_available());
    assert!(renderer.gpu_timings_ms().is_empty());
}

#[test]
fn a_disabled_subsystem_needs_no_device() {
    let timing = GpuTiming::unavailable();
    assert!(!timing.is_available());
    assert!(timing.timings_ms().is_empty());
}

/// A pass that stops running stops reporting; a resolve of its unwritten queries is a stale frame.
#[test]
fn a_pass_that_stops_running_drops_out_of_the_timings() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !timestamps(&gpu) {
        return;
    }
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    assert!(renderer.enable_gpu_timing(&gpu));
    renderer.upload_scene(&gpu, &load_gltf(cube_path()).expect("cube.gltf must load"));
    renderer.auto_exposure = true;
    let reported =
        |renderer: &ForwardRenderer, name: &str| ms(&renderer.gpu_timings_ms(), name).is_some();

    render_frames(&gpu, &mut renderer, (64, 64), 64);
    assert!(
        reported(&renderer, "Bloom") && reported(&renderer, "Histogram"),
        "precondition: bloom and the histogram must report while they run: {:?}",
        renderer.gpu_timings_ms()
    );
    renderer.bloom_strength = 0.0;
    renderer.auto_exposure = false;
    render_frames(&gpu, &mut renderer, (64, 64), 64);
    let timings = renderer.gpu_timings_ms();
    assert!(
        !reported(&renderer, "Bloom") && !reported(&renderer, "Histogram"),
        "bloom and the histogram stopped running but still report: {timings:?}"
    );
    assert!(
        reported(&renderer, "Forward"),
        "a pass that keeps running keeps reporting: {timings:?}"
    );
}
