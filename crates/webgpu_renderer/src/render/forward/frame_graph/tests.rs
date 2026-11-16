use super::*;

/// Every optional pass on, so each mode and path records its maximal frame.
fn maximal(mode: RenderMode, path: RasterPath) -> FrameConfig {
    FrameConfig {
        size: (1920, 1080),
        mode,
        path,
        clouds: Some(CloudShape {
            half: false,
            noise: true,
        }),
        bloom: true,
        ssao: true,
        histogram: true,
        occlusion: true,
        fxaa: true,
    }
}

/// Forward, deferred, ray traced, path traced.
fn every_maximal_frame() -> [FrameConfig; 4] {
    [
        maximal(RenderMode::Raster, RasterPath::Forward),
        maximal(RenderMode::Raster, RasterPath::Deferred),
        maximal(RenderMode::RayTraced, RasterPath::Forward),
        maximal(RenderMode::PathTraced, RasterPath::Forward),
    ]
}

fn names(config: &FrameConfig) -> Vec<&'static str> {
    let plan = plan(config).expect("every frame's graph compiles");
    plan.passes.iter().map(|pass| pass.name()).collect()
}

/// Every combination of the settings that change the frame's rows.
fn every_config() -> Vec<FrameConfig> {
    let mut configs = Vec::new();
    let clouds = [
        None,
        Some((false, false)),
        Some((false, true)),
        Some((true, false)),
        Some((true, true)),
    ];
    for mode in RenderMode::ALL {
        for path in RasterPath::ALL {
            for cloud in clouds {
                for flags in 0u8..32 {
                    let bit = |n: u8| flags & (1 << n) != 0;
                    configs.push(FrameConfig {
                        size: (640, 360),
                        mode,
                        path,
                        clouds: cloud.map(|(half, noise)| CloudShape { half, noise }),
                        bloom: bit(0),
                        ssao: bit(1),
                        histogram: bit(2),
                        occlusion: bit(3),
                        fxaa: bit(4),
                    });
                }
            }
        }
    }
    configs
}

#[test]
fn every_frame_compiles_and_culls_nothing_it_records() {
    for config in every_config() {
        let rows = rows(&config);
        let plan = plan(&config).unwrap_or_else(|error| panic!("{config:?}: {error}"));
        let recorded: Vec<FramePass> = rows.iter().map(|row| row.0).collect();
        assert_eq!(
            plan.passes, recorded,
            "{config:?}: the graph reordered or culled a row the frame records"
        );
    }
}

#[test]
fn the_maximal_frames_record_their_passes_in_the_hand_written_order() {
    let post = [
        "histogram",
        "exposure_reduce",
        "cloud_noise",
        "clouds",
        "bloom",
    ];
    let tail = ["tonemap", "fxaa"];
    let forward = ["shadow", "forward+sky", "depth_resolve", "occlusion_cull"];
    let deferred = [
        "shadow",
        "gbuffer",
        "deferred_lighting+sky",
        "occlusion_cull",
    ];
    let [f, d, r, p] = every_maximal_frame().map(|config| names(&config));
    assert_eq!(f, [&forward[..], &post, &["ssao"], &tail].concat());
    assert_eq!(d, [&deferred[..], &post, &["ssao"], &tail].concat());
    assert_eq!(
        r,
        [&["ray_trace", "trace_resolve"][..], &post, &tail].concat()
    );
    assert_eq!(
        p,
        [&["path_trace", "trace_resolve"][..], &post, &tail].concat()
    );
}

#[test]
fn every_timed_pass_has_a_row_and_a_traced_one_only_in_traced_frames() {
    let frames = every_maximal_frame().map(|config| plan(&config).expect("compiles").passes);
    for timed in TimedPass::ALL {
        let in_frame = |passes: &Vec<FramePass>| passes.iter().any(|p| p.timed() == Some(timed));
        assert!(
            frames.iter().any(in_frame),
            "TimedPass::{timed:?} has no row"
        );
        let traced_only = !in_frame(&frames[0]) && !in_frame(&frames[1]);
        assert_eq!(timed.is_traced(), traced_only, "{timed:?}");
    }
}

#[test]
fn each_frame_times_the_passes_its_timings_report_and_no_others() {
    let mode_of = [
        (RenderMode::Raster, RasterPath::Forward),
        (RenderMode::Raster, RasterPath::Deferred),
        (RenderMode::RayTraced, RasterPath::Forward),
        (RenderMode::PathTraced, RasterPath::Forward),
    ];
    for (config, (mode, path)) in every_maximal_frame().iter().zip(mode_of) {
        let timed: Vec<TimedPass> = plan(config)
            .expect("compiles")
            .passes
            .iter()
            .filter_map(|pass| pass.timed())
            .collect();
        let path = if mode.is_traced() {
            RasterPath::Forward
        } else {
            path
        };
        let expected: Vec<TimedPass> = TimedPass::ALL
            .into_iter()
            .filter(|&pass| super::super::traced::records(pass, mode, path, true))
            .collect();
        assert_eq!(timed, expected, "{mode:?} {path:?}");
    }
}

#[test]
fn the_clouds_march_after_the_depth_and_before_bloom_and_the_tonemap() {
    for config in every_maximal_frame() {
        let names = names(&config);
        let at = |name: &str| names.iter().position(|n| *n == name).expect(name);
        let depth = ["depth_resolve", "gbuffer", "trace_resolve"]
            .iter()
            .find_map(|name| names.iter().position(|n| n == name))
            .expect("a pass writes the single-sample depth");
        let clouds = at("clouds");
        assert!(depth < clouds && at("cloud_noise") < clouds);
        assert!(clouds < at("bloom") && at("bloom") < at("tonemap"));
    }
}

#[test]
fn fxaa_filters_the_tonemapped_frame_last_and_off_the_tonemap_writes_the_output() {
    for mut config in every_maximal_frame() {
        assert_eq!(names(&config).last(), Some(&"fxaa"));
        config.fxaa = false;
        assert_eq!(names(&config).last(), Some(&"tonemap"));
    }
}

#[test]
fn only_the_forward_path_allocates_msaa_and_only_the_deferred_one_a_gbuffer() {
    for config in every_config() {
        let graph = plan(&config).expect("compiles").graph;
        let has = |resource| graph.slot(resource).is_some();
        let forward = !config.mode.is_traced() && config.path == RasterPath::Forward;
        let deferred = !config.mode.is_traced() && config.path == RasterPath::Deferred;
        assert_eq!(has(HdrMsaa) && has(DepthMsaa), forward, "{config:?}");
        assert_eq!(has(GBufferNormal), deferred, "{config:?}");
        assert_eq!(has(CloudColor), config.clouds.is_some(), "{config:?}");
        assert!(has(HdrColor) && has(Depth), "{config:?}");
    }
}

#[test]
fn a_deferred_frame_marches_its_clouds_into_a_gbuffer_texture() {
    let mut config = maximal(RenderMode::Raster, RasterPath::Deferred);
    let graph = plan(&config).expect("compiles").graph;
    assert_eq!(graph.slot(CloudColor), graph.slot(GBufferNormal));
    config.clouds = Some(CloudShape {
        half: true,
        noise: false,
    });
    let graph = plan(&config).expect("compiles").graph;
    assert_eq!(graph.slot(CloudColor), graph.slot(GBufferNormal));
    assert!(graph.slot(CloudHalf).is_some());
}

/// What the targets took before the graph: the forward targets always, the G-buffer after the first deferred
/// frame, the cloud target after the first cloud frame, each its own texture.
fn before_the_graph(size: (u32, u32), half_clouds: bool) -> u64 {
    let targets = transient_targets(size);
    let bytes = |resource| {
        let (_, desc) = targets
            .iter()
            .find(|(r, _)| *r == resource)
            .expect("declared");
        desc.bytes()
    };
    let always = [HdrMsaa, DepthMsaa, HdrColor, Depth, DepthSpread];
    let deferred = [GBufferNormal, GBufferAlbedo, GBufferMaterial, CloudColor];
    let half = if half_clouds { bytes(CloudHalf) } else { 0 };
    always.into_iter().chain(deferred).map(bytes).sum::<u64>() + half
}

#[test]
fn a_deferred_frame_with_clouds_and_post_holds_a_third_of_the_old_target_memory() {
    let mib = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    for half in [false, true] {
        let mut config = maximal(RenderMode::Raster, RasterPath::Deferred);
        config.clouds = Some(CloudShape { half, noise: false });
        let graph = plan(&config).expect("compiles").graph;
        let before = before_the_graph(config.size, half);
        let (unaliased, after) = (graph.unaliased_bytes(), graph.transient_bytes());
        eprintln!(
            "1920x1080 deferred + clouds (half {half}) + post: {:.1} MiB before, {:.1} MiB unaliased, {:.1} MiB aliased",
            mib(before),
            mib(unaliased),
            mib(after)
        );
        let pixels = 1920 * 1080;
        // HDR 8, depth 4 and the G-buffer 20 bytes a pixel; the clouds' 8 alias the G-buffer normal.
        assert_eq!(after, 32 * pixels + if half { 8 * 960 * 540 } else { 0 });
        assert_eq!(unaliased - after, 8 * pixels);
        assert_eq!(before, 96 * pixels + if half { 8 * 960 * 540 } else { 0 });
    }
}
