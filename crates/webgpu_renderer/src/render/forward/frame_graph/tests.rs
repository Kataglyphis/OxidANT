use super::*;

/// The headless readback's format, 4 bytes a pixel.
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Every optional pass on, so each mode and path records its maximal frame.
fn maximal(mode: RenderMode, path: RasterPath) -> FrameConfig {
    FrameConfig {
        size: (1920, 1080),
        output_size: (1920, 1080),
        output_format: OUTPUT,
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
                        output_size: (640, 360),
                        output_format: OUTPUT,
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
    // SSAO before the pass that lights the scene, whose ambient it scales.
    let forward = [
        "draw_cull",
        "shadow",
        "ssao",
        "forward+sky",
        "depth_resolve",
        "occlusion_cull",
    ];
    let deferred = [
        "draw_cull",
        "shadow",
        "gbuffer",
        "ssao",
        "deferred_lighting+sky",
        "occlusion_cull",
    ];
    let [f, d, r, p] = every_maximal_frame().map(|config| names(&config));
    assert_eq!(f, [&forward[..], &post, &tail].concat());
    assert_eq!(d, [&deferred[..], &post, &tail].concat());
    assert_eq!(
        r,
        [&["ray_trace", "trace_resolve"][..], &post, &tail].concat()
    );
    assert_eq!(
        p,
        [&["path_trace", "trace_resolve"][..], &post, &tail].concat()
    );
}

/// Whether one of `passes` opens `timed`'s scope, or for `Main` one of the scopes it spans.
fn times(passes: &[FramePass], timed: TimedPass) -> bool {
    passes.iter().any(|pass| {
        pass.timed()
            .iter()
            .any(|&t| t == timed || t.nested_in() == Some(timed))
    })
}

#[test]
fn every_timed_pass_has_a_row_and_a_traced_one_only_in_traced_frames() {
    let frames = every_maximal_frame().map(|config| plan(&config).expect("compiles").passes);
    for timed in TimedPass::ALL {
        assert!(
            frames.iter().any(|passes| times(passes, timed)),
            "TimedPass::{timed:?} has no row"
        );
        let traced_only = !times(&frames[0], timed) && !times(&frames[1], timed);
        assert_eq!(timed.is_traced(), traced_only, "{timed:?}");
    }
    for passes in &frames {
        assert!(times(passes, TimedPass::Main), "{passes:?} draws no scene");
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
        let passes = plan(config).expect("compiles").passes;
        let mut timed: Vec<TimedPass> = passes
            .iter()
            .flat_map(|pass| pass.timed().iter().copied())
            .collect();
        // Main has no row: it reports where its nested scopes do, ahead of them.
        let first_nested = timed
            .iter()
            .position(|pass| pass.nested_in() == Some(TimedPass::Main))
            .expect("every frame draws its scene in a scope Main spans");
        timed.insert(first_nested, TimedPass::Main);
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

#[test]
fn the_post_chain_allocates_only_the_targets_of_its_running_rows() {
    for config in every_config() {
        let graph = plan(&config).expect("compiles").graph;
        let has = |resource| graph.slot(resource).is_some();
        let ssao = config.ssao && !config.mode.is_traced();
        assert_eq!(has(Bloom) && has(BloomScratch), config.bloom, "{config:?}");
        assert_eq!(has(Bloom) || has(BloomScratch), config.bloom, "{config:?}");
        assert_eq!(has(AmbientRaw) && has(Ambient), ssao, "{config:?}");
        assert_eq!(has(AmbientRaw) || has(Ambient), ssao, "{config:?}");
        // Only the forward path draws a depth for SSAO; the deferred one has the G-buffer's.
        let prepass = ssao && config.path == RasterPath::Forward;
        assert_eq!(has(PrepassDepth), prepass, "{config:?}");
        assert_eq!(has(Tonemapped), config.fxaa, "{config:?}");
    }
}

#[test]
fn bloom_takes_the_half_resolution_cloud_march_when_the_sizes_agree() {
    let mut config = maximal(RenderMode::Raster, RasterPath::Deferred);
    config.clouds = Some(CloudShape {
        half: true,
        noise: false,
    });
    let graph = plan(&config).expect("compiles").graph;
    assert_eq!(graph.slot(Bloom), graph.slot(CloudHalf));
    assert_ne!(graph.slot(BloomScratch), graph.slot(Bloom));
    // An odd output's march rounds up and bloom rounds down, so they cannot share a texture.
    config.size = (1919, 1079);
    let graph = plan(&config).expect("compiles").graph;
    assert_ne!(graph.slot(Bloom), graph.slot(CloudHalf));
}

/// What the targets took before the graph: the forward targets always, the G-buffer after the first deferred
/// frame, the cloud target after the first cloud frame, bloom's pair, SSAO's pair and FXAA's input, each its own texture.
fn before_the_graph(size: (u32, u32), half_clouds: bool) -> u64 {
    let targets = transient_targets(size, size, OUTPUT);
    let bytes = |resource| {
        let (_, desc) = targets
            .iter()
            .find(|(r, _)| *r == resource)
            .expect("declared");
        desc.bytes()
    };
    let always = [HdrMsaa, DepthMsaa, HdrColor, Depth, DepthSpread];
    let deferred = [GBufferNormal, GBufferAlbedo, GBufferMaterial, CloudColor];
    let post = [Bloom, BloomScratch, AmbientRaw, Ambient, Tonemapped];
    let half = if half_clouds { bytes(CloudHalf) } else { 0 };
    always
        .into_iter()
        .chain(deferred)
        .chain(post)
        .map(bytes)
        .sum::<u64>()
        + half
}

/// Bytes of the transients `resources` name, as the graph backs them, each counted once per slot.
fn slot_bytes(graph: &CompiledGraph, resources: &[Resource]) -> u64 {
    let mut slots: Vec<usize> = resources.iter().filter_map(|&r| graph.slot(r)).collect();
    slots.sort_unstable();
    slots.dedup();
    slots.iter().map(|&slot| graph.slots[slot].bytes()).sum()
}

#[test]
fn a_deferred_frame_with_clouds_and_post_holds_two_fifths_of_the_old_target_memory() {
    let mib = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    let (pixels, quarter) = (1920 * 1080, 960 * 540);
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
        let rows = [
            (
                "scene + clouds",
                &[
                    HdrColor,
                    Depth,
                    GBufferNormal,
                    GBufferAlbedo,
                    GBufferMaterial,
                    CloudColor,
                ][..],
            ),
            ("bloom", &[Bloom, BloomScratch][..]),
            ("ssao", &[AmbientRaw, Ambient][..]),
            ("fxaa", &[Tonemapped][..]),
        ];
        for (name, resources) in rows {
            eprintln!("  {name}: {:.1} MiB", mib(slot_bytes(&graph, resources)));
        }
        let half_march = if half { 8 * quarter } else { 0 };
        // Bytes a pixel: HDR 8, depth 4, G-buffer 20, FXAA 4; a half pixel: bloom 16, SSAO 2; clouds alias.
        assert_eq!(after, 36 * pixels + 18 * quarter);
        assert_eq!(unaliased - after, 8 * pixels + half_march);
        assert_eq!(before, 100 * pixels + 18 * quarter + half_march);
    }
}

#[test]
fn a_scaled_frame_renders_the_scene_small_and_filters_at_the_output_size() {
    let targets = transient_targets((964, 604), (1928, 1207), OUTPUT);
    let size = |resource| {
        targets
            .iter()
            .find(|(r, _)| *r == resource)
            .map(|(_, desc)| desc.size)
            .expect("declared")
    };
    assert_eq!(
        size(Tonemapped),
        (1928, 1207),
        "FXAA reads the output's pixels"
    );
    for scene in [
        HdrMsaa,
        DepthMsaa,
        HdrColor,
        Depth,
        GBufferAlbedo,
        CloudColor,
    ] {
        assert_eq!(size(scene), (964, 604), "{scene:?}");
    }
    assert_eq!(size(Bloom), (482, 302));
}
