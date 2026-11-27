//! F5 over the post chain: the tonemap, FXAA, bloom, SSAO, histogram and depth resolve reload or keep their pipelines.

use kataglyphis_webgpu_renderer::render::forward::ShaderSources;
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::{
    load_gltf, ForwardRenderer, GpuContext, OrbitCamera, TonemapPass,
};

const SIZE: u32 = 64;

fn renderer(gpu: &GpuContext) -> ForwardRenderer {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets/cube_on_plane.gltf");
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(gpu, &load_gltf(path).expect("cube_on_plane.gltf must load"));
    renderer
}

fn camera() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 25.0,
        ..OrbitCamera::default()
    }
}

fn render(gpu: &GpuContext, renderer: &mut ForwardRenderer) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, &camera())
        .expect("headless render must succeed")
}

/// `source` with every `old` replaced by `new`; fails when the WGSL no longer has `old`.
fn edited(source: &str, old: &str, new: &str) -> String {
    let edited = source.replace(old, new);
    assert_ne!(edited, source, "the WGSL no longer contains `{old}`");
    edited
}

/// The six post sources, each with the entry point its pass builds renamed away.
fn without_entry_points() -> [(&'static str, String); 6] {
    let shipped = ShaderSources::shipped();
    [
        (
            "tonemap",
            edited(shipped.tonemap, "fn fs_main(", "fn fs_gone("),
        ),
        ("fxaa", edited(shipped.fxaa, "fn fs_main(", "fn fs_gone(")),
        (
            "bloom",
            edited(shipped.bloom, "fn fs_brightpass(", "fn fs_gone("),
        ),
        ("ssao", edited(shipped.ssao, "fn fs_ssao(", "fn fs_gone(")),
        (
            "histogram",
            edited(shipped.histogram, "fn cs_reduce_exposure(", "fn cs_gone("),
        ),
        (
            "depth_resolve",
            edited(shipped.depth_resolve, "fn fs_main(", "fn fs_gone("),
        ),
    ]
}

/// The shipped sources with `name`'s replaced by `wgsl`.
fn with_source<'a>(name: &str, wgsl: &'a str) -> ShaderSources<'a> {
    let mut sources = ShaderSources::shipped();
    let slot = match name {
        "tonemap" => &mut sources.tonemap,
        "fxaa" => &mut sources.fxaa,
        "bloom" => &mut sources.bloom,
        "ssao" => &mut sources.ssao,
        "histogram" => &mut sources.histogram,
        "depth_resolve" => &mut sources.depth_resolve,
        other => panic!("no post source {other}"),
    };
    *slot = wgsl;
    sources
}

#[test]
fn a_post_shader_without_its_entry_point_is_refused_and_the_frame_kept() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.fxaa = FxaaMode::Always;
    let before = render(&gpu, &mut renderer);
    for (name, broken) in without_entry_points() {
        assert!(
            renderer
                .reload_shaders(&gpu, &with_source(name, &broken))
                .is_err(),
            "a {name} shader without its entry point must be refused"
        );
        let kept = render(&gpu, &mut renderer);
        assert!(
            kept == before,
            "the refused {name} reload changed the frame"
        );
    }
}

/// Every pixel of `frame` within 2 levels of `rgb`.
fn all_near(frame: &[u8], rgb: [u8; 3]) -> bool {
    frame
        .as_chunks::<4>()
        .0
        .iter()
        .all(|px| (0..3).all(|c| px[c].abs_diff(rgb[c]) <= 2))
}

/// The host's own pass and a screenshot's fresh one both draw with the reloaded tonemap, then with the shipped one.
#[test]
fn a_tonemap_reload_reaches_the_window_pass_and_the_screenshot() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    let shipped = ShaderSources::shipped();
    let before = render(&gpu, &mut renderer);
    // Linear 0.5 magenta, which the sRGB target stores as 188.
    let flat = edited(
        shipped.tonemap,
        "pixelOutput_0( vec4<f32>(",
        "pixelOutput_0( vec4<f32>(0.5f, 0.0f, 0.5f, 1.0f) + 0.0f * vec4<f32>(",
    );
    let mut window_pass = TonemapPass::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    renderer
        .reload_shaders(&gpu, &with_source("tonemap", &flat))
        .expect("a valid tonemap must reload");
    let screenshot = render(&gpu, &mut renderer);
    assert!(
        all_near(&screenshot, [188, 0, 188]),
        "the screenshot's tonemap is not the reloaded one"
    );
    let output = output_view(&gpu);
    renderer.render_tonemapped(&gpu, &mut window_pass, &output, SIZE, SIZE, &camera());
    let reloaded_id = window_pass.shaders_id();
    assert_ne!(reloaded_id, 0, "the window's pass kept the shipped tonemap");

    renderer
        .reload_shaders(&gpu, &shipped)
        .expect("the shipped shaders must reload");
    renderer.render_tonemapped(&gpu, &mut window_pass, &output, SIZE, SIZE, &camera());
    assert_ne!(window_pass.shaders_id(), reloaded_id);
    assert!(
        render(&gpu, &mut renderer) == before,
        "the shipped tonemap did not come back"
    );
}

fn output_view(gpu: &GpuContext) -> wgpu::TextureView {
    gpu.device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("post_reload_output"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

#[test]
fn an_fxaa_reload_filters_with_the_new_shader() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.fxaa = FxaaMode::Always;
    let before = render(&gpu, &mut renderer);
    let green = edited(
        ShaderSources::shipped().fxaa,
        "pixelOutput_0( vec4<f32>(",
        "pixelOutput_0( vec4<f32>(0.0f, 0.5f, 0.0f, 1.0f) + 0.0f * vec4<f32>(",
    );
    renderer
        .reload_shaders(&gpu, &with_source("fxaa", &green))
        .expect("a valid FXAA shader must reload");
    assert!(all_near(&render(&gpu, &mut renderer), [0, 188, 0]));
    renderer.fxaa = FxaaMode::Off;
    let unfiltered = render(&gpu, &mut renderer);
    assert!(!all_near(&unfiltered, [0, 188, 0]) && unfiltered != before);
}

#[test]
fn a_depth_resolve_reload_writes_the_new_depth() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    let quarter = edited(
        ShaderSources::shipped().depth_resolve,
        "o_1.depth_0 = minDepth_0;",
        "o_1.depth_0 = 0.25f + 0.0f * minDepth_0;",
    );
    renderer
        .reload_shaders(&gpu, &with_source("depth_resolve", &quarter))
        .expect("a valid depth resolve must reload");
    render(&gpu, &mut renderer);
    let depth = renderer.read_depth_target(&gpu).expect("depth readback");
    assert!(
        depth.iter().all(|&d| d == 0.25),
        "the resolve kept the old shader"
    );
}

#[test]
fn a_histogram_reload_reduces_with_the_new_kernel() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    let shifted = edited(
        ShaderSources::shipped().histogram,
        "exposureState_0[i32(0)] = exposureParams_0.values_0.w;",
        "exposureState_0[i32(0)] = exposureParams_0.values_0.w + 3.0f;",
    );
    renderer
        .reload_shaders(&gpu, &with_source("histogram", &shifted))
        .expect("a valid histogram shader must reload");
    render(&gpu, &mut renderer);
    assert_eq!(renderer.read_exposure(&gpu), (3.0, 0.0));
}

/// Mean of one channel over the frame.
fn mean(frame: &[u8], channel: usize) -> f32 {
    let sum: u32 = frame
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| u32::from(px[channel]))
        .sum();
    sum as f32 / (frame.len() / 4) as f32
}

#[test]
fn bloom_and_ssao_reloads_change_what_they_add() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let shipped = ShaderSources::shipped();
    let blue_bloom = edited(
        shipped.bloom,
        "pixelOutput_0( vec4<f32>(bloom_bright_0(",
        "pixelOutput_0( vec4<f32>(0.0f, 0.0f, 4.0f, 1.0f) + 0.0f * vec4<f32>(bloom_bright_0(",
    );
    let full_occlusion = edited(
        shipped.ssao,
        "pixelOutput_0( vec4<f32>(clamp(1.0f - u_0",
        "pixelOutput_0( vec4<f32>(0.0f) + 0.0f * vec4<f32>(clamp(1.0f - u_0",
    );
    let mut renderer = renderer(&gpu);
    let before = render(&gpu, &mut renderer);
    renderer
        .reload_shaders(&gpu, &with_source("bloom", &blue_bloom))
        .expect("a valid bloom shader must reload");
    let bloomed = render(&gpu, &mut renderer);
    eprintln!("blue {} -> {}", mean(&before, 2), mean(&bloomed, 2));
    assert!(
        mean(&bloomed, 2) > mean(&before, 2) + 20.0,
        "the bloom reload added no blue"
    );

    let mut renderer = self::renderer(&gpu);
    renderer
        .reload_shaders(&gpu, &with_source("ssao", &full_occlusion))
        .expect("a valid SSAO shader must reload");
    let occluded = render(&gpu, &mut renderer);
    eprintln!("green {} -> {}", mean(&before, 1), mean(&occluded, 1));
    assert!(
        mean(&occluded, 1) < mean(&before, 1) - 5.0,
        "the SSAO reload darkened nothing"
    );
}
