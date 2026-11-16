//! Mip-tail streaming: a material texture uploads its coarse tail, then sharpens as the camera needs it.

use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Vec3};
use kataglyphis_webgpu_renderer::scene::{
    AlphaMode, CompressedFormat, CompressedTexture, CpuSampler, CpuScene, CpuTexture, CpuTextureRef,
};
use kataglyphis_webgpu_renderer::{
    load_gltf, ForwardRenderer, GpuContext, OrbitCamera, TextureResidency, TextureStreaming,
};

const SIZE: u32 = 128;

/// Bytes of a square RGBA8 chain from `side` down to 1x1.
fn rgba8_chain_bytes(mut side: u64) -> u64 {
    let mut bytes = 0;
    loop {
        bytes += side * side * 4;
        if side == 1 {
            return bytes;
        }
        side /= 2;
    }
}

/// A checker of 2-texel cells in two saturated colours: any coarser level averages it to one flat tone.
fn checker(side: u32, seed: u8) -> CpuTexture {
    let mut rgba8 = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let on = ((x / 2) + (y / 2)) % 2 == 0;
            let texel = if on {
                [255, seed, 0, 255]
            } else {
                [0, 255 - seed, 255, 255]
            };
            rgba8.extend_from_slice(&texel);
        }
    }
    CpuTexture {
        width: side,
        height: side,
        rgba8,
        compressed: None,
    }
}

fn texture_ref(texture: CpuTexture) -> Option<CpuTextureRef> {
    Some(CpuTextureRef {
        texture: Arc::new(texture),
        sampler: CpuSampler::default(),
        srgb: true,
    })
}

/// The unit test cube (UV 0..1 a face) with `texture` as its base colour.
fn cube_with(texture: CpuTexture) -> CpuScene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    let mut scene = load_gltf(path).expect("cube.gltf must load");
    let material = &mut scene.primitives[0].material;
    material.base_color = [1.0; 4];
    material.base_color_texture = texture_ref(texture);
    scene
}

fn camera_at(radius: f32) -> OrbitCamera {
    OrbitCamera {
        radius,
        near: 0.01,
        ..OrbitCamera::default()
    }
}

fn streaming_renderer(gpu: &GpuContext, scene: &CpuScene) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.texture_streaming = TextureStreaming::INTERACTIVE;
    renderer.upload_scene(gpu, scene);
    renderer
}

fn render(renderer: &mut ForwardRenderer, gpu: &GpuContext, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("render")
}

#[test]
fn an_upload_holds_only_the_tail_until_the_camera_comes_close() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = streaming_renderer(&gpu, &cube_with(checker(1024, 0)));
    let tail = rgba8_chain_bytes(128);
    let full = rgba8_chain_bytes(1024);
    let residency = renderer.texture_residency();
    assert_eq!((residency.textures, residency.streaming), (1, 1));
    assert_eq!(
        residency.resident_bytes, tail,
        "the upload writes the 128-texel tail"
    );
    assert_eq!(residency.full_bytes, full);

    // From 200 units the cube is a few pixels: the tail is all it needs.
    for _ in 0..3 {
        render(&mut renderer, &gpu, &camera_at(200.0));
    }
    assert_eq!(renderer.texture_residency().resident_bytes, tail);

    // Close up, one level a frame: 256, 512, then the full 1024.
    let mut resident = Vec::new();
    for _ in 0..4 {
        render(&mut renderer, &gpu, &camera_at(1.0));
        resident.push(renderer.texture_residency().resident_bytes);
    }
    let steps = [256, 512, 1024, 1024].map(rgba8_chain_bytes);
    assert_eq!(resident, steps, "one level a frame toward the finest");
    assert_eq!(renderer.texture_residency().streaming, 0);
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "streaming failed validation: {error:?}");
}

#[test]
fn a_streamed_frame_ends_identical_to_an_unstreamed_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_with(checker(1024, 40));
    let near = camera_at(1.0);
    let mut whole = ForwardRenderer::new(&gpu, SIZE, SIZE);
    whole.upload_scene(&gpu, &scene);
    assert_eq!(
        whole.texture_residency().streaming,
        0,
        "streaming is off by default"
    );
    let expected = render(&mut whole, &gpu, &near);

    let mut streamed = streaming_renderer(&gpu, &scene);
    // Its first frame already steps once, to 256: still a flat tone where the checker should show.
    let first = render(&mut streamed, &gpu, &near);
    assert_ne!(
        first, expected,
        "a frame from the coarse levels must differ"
    );
    let mut last = first;
    for _ in 0..3 {
        last = render(&mut streamed, &gpu, &near);
    }
    assert_eq!(streamed.texture_residency().streaming, 0);
    assert!(
        last == expected,
        "the fully streamed frame must match the whole upload"
    );
}

#[test]
fn switching_streaming_off_finishes_every_texture_on_the_next_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = streaming_renderer(&gpu, &cube_with(checker(512, 0)));
    assert_eq!(renderer.texture_residency().streaming, 1);
    renderer.texture_streaming.enabled = false;
    // Even from far away: off means full resolution.
    render(&mut renderer, &gpu, &camera_at(200.0));
    let residency = renderer.texture_residency();
    assert_eq!(residency.streaming, 0);
    assert_eq!(residency.resident_bytes, residency.full_bytes);

    let mut again = streaming_renderer(&gpu, &cube_with(checker(512, 0)));
    again.finish_texture_streaming(&gpu);
    assert_eq!(again.texture_residency().streaming, 0);
}

#[test]
fn a_tight_budget_streams_one_texture_a_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut scene = cube_with(checker(512, 0));
    scene.primitives[0].material.emissive_texture = texture_ref(checker(512, 90));
    scene.primitives[0].material.emissive_factor = [0.1; 3];
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    renderer.texture_streaming = TextureStreaming {
        bytes_per_frame: 1,
        ..TextureStreaming::INTERACTIVE
    };
    renderer.upload_scene(&gpu, &scene);
    let tail = rgba8_chain_bytes(128);
    assert_eq!(renderer.texture_residency().resident_bytes, 2 * tail);

    render(&mut renderer, &gpu, &camera_at(1.0));
    assert_eq!(
        renderer.texture_residency().resident_bytes,
        tail + rgba8_chain_bytes(256),
        "a 1-byte budget still runs one step, and only one"
    );
    for _ in 0..3 {
        render(&mut renderer, &gpu, &camera_at(1.0));
    }
    assert_eq!(
        renderer.texture_residency().resident_bytes,
        2 * rgba8_chain_bytes(512)
    );
}

/// A square BC1 chain of arbitrary blocks (every bit pattern is a valid BC1 block).
fn bc1_noise(side: u32, seed: usize) -> CpuTexture {
    let mips = (0..=side.ilog2())
        .map(|level| {
            let blocks = (side >> level).max(1).div_ceil(4) as usize;
            (0..blocks * blocks * 8)
                .map(|i| ((i + seed) * 37 % 251) as u8)
                .collect()
        })
        .collect();
    CpuTexture {
        width: side,
        height: side,
        rgba8: Vec::new(),
        compressed: Some(CompressedTexture {
            format: CompressedFormat::Bc1RgbaUnorm,
            mips,
            declared_srgb: None,
            basis_source: None,
        }),
    }
}

/// A MASK caster's shadow bundle bakes its alpha-testing group, which a new view must replace.
#[test]
fn a_masked_block_compressed_texture_streams_without_validation_errors() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut scene = cube_with(bc1_noise(1024, 0));
    scene.primitives[0].material.alpha_mode = AlphaMode::Mask(0.5);
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = streaming_renderer(&gpu, &scene);
    assert_eq!(renderer.texture_residency().streaming, 1);
    for _ in 0..5 {
        render(&mut renderer, &gpu, &camera_at(1.0));
    }
    let residency = renderer.texture_residency();
    assert_eq!(residency.streaming, 0);
    assert_eq!(residency.resident_bytes, residency.full_bytes);
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "streaming failed validation: {error:?}");
}

/// Sixteen cubes in a row, each with its own base colour texture.
fn sixteen_cubes(texture: impl Fn(usize) -> CpuTexture) -> CpuScene {
    let base = cube_with(checker(8, 0));
    let mut scene = base.clone();
    scene.primitives.clear();
    for i in 0..16 {
        let mut prim = base.primitives[0].clone();
        prim.transform = Mat4::from_translation(Vec3::new(i as f32 * 2.0, 0.0, 0.0));
        prim.material.base_color_texture = texture_ref(texture(i));
        scene.primitives.push(prim);
    }
    scene
}

/// Uploads `scene` and waits for the queue; the upload's wall time and what it left resident.
fn measure_upload(
    gpu: &GpuContext,
    scene: &CpuScene,
    streaming: TextureStreaming,
) -> (std::time::Duration, TextureResidency) {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.texture_streaming = streaming;
    let start = Instant::now();
    renderer.upload_scene(gpu, scene);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    (start.elapsed(), renderer.texture_residency())
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1 << 20)
}

/// The measured effect: sixteen 2048-texel textures, uploaded whole and as tails. Prints its numbers.
#[test]
fn a_tail_upload_holds_a_fraction_of_the_bytes() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let rgba8 = sixteen_cubes(|i| checker(2048, (i * 15) as u8));
    let (whole_time, whole) = measure_upload(&gpu, &rgba8, TextureStreaming::default());
    let (tail_time, tail) = measure_upload(&gpu, &rgba8, TextureStreaming::INTERACTIVE);
    eprintln!(
        "16 x 2048^2 RGBA8: whole {:.1} MiB in {whole_time:?}, tail {:.2} MiB in {tail_time:?}, \
         {:.1} MiB of chains kept on the CPU",
        mib(whole.resident_bytes),
        mib(tail.resident_bytes),
        mib(tail.cpu_bytes),
    );
    // The chains past the CPU budget are dropped, to be rebuilt from the scene's images.
    assert!(tail.cpu_bytes <= TextureStreaming::INTERACTIVE.cpu_bytes);
    assert_eq!(whole.cpu_bytes, 0);
    assert_eq!(whole.resident_bytes, 16 * rgba8_chain_bytes(2048));
    assert_eq!(tail.resident_bytes, 16 * rgba8_chain_bytes(128));
    assert_eq!(tail.full_bytes, whole.full_bytes);
    assert_eq!(tail.streaming, 16);

    // No CPU box filter: the time left is the upload's own.
    let bc1 = sixteen_cubes(|i| bc1_noise(2048, i));
    let (whole_time, whole) = measure_upload(&gpu, &bc1, TextureStreaming::default());
    let (tail_time, tail) = measure_upload(&gpu, &bc1, TextureStreaming::INTERACTIVE);
    eprintln!(
        "16 x 2048^2 BC1: whole {:.1} MiB in {whole_time:?}, tail {:.2} MiB in {tail_time:?}",
        mib(whole.resident_bytes),
        mib(tail.resident_bytes),
    );
    assert!(tail.resident_bytes * 100 < whole.resident_bytes);
    assert_eq!(tail.streaming, 16);
}

fn renderer_with(
    gpu: &GpuContext,
    scene: &CpuScene,
    streaming: TextureStreaming,
) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.texture_streaming = streaming;
    renderer.upload_scene(gpu, scene);
    renderer
}

/// `frames` frames from `camera`; the last one.
fn frames(
    renderer: &mut ForwardRenderer,
    gpu: &GpuContext,
    camera: &OrbitCamera,
    frames: usize,
) -> Vec<u8> {
    let mut last = Vec::new();
    for _ in 0..frames {
        last = render(renderer, gpu, camera);
    }
    last
}

/// Streaming that evicts after two frames, so a test sees it without a hundred renders.
const QUICK_EVICTION: TextureStreaming = TextureStreaming {
    evict_after_frames: 2,
    ..TextureStreaming::INTERACTIVE
};

#[test]
fn moving_away_drops_the_finer_levels_and_coming_back_restreams_them() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = cube_with(checker(1024, 70));
    let near = camera_at(1.0);
    let mut whole = ForwardRenderer::new(&gpu, SIZE, SIZE);
    whole.upload_scene(&gpu, &scene);
    let expected = render(&mut whole, &gpu, &near);
    let cpu_path = TextureStreaming {
        gpu_copies: false,
        ..QUICK_EVICTION
    };
    for streaming in [QUICK_EVICTION, cpu_path] {
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = renderer_with(&gpu, &scene, streaming);
        frames(&mut renderer, &gpu, &near, 4);
        let full = renderer.texture_residency();
        assert_eq!(full.resident_bytes, full.full_bytes);

        // Far away, after the two-frame wait, one level a frame back to the tail, and never past it.
        let far = camera_at(200.0);
        let mut resident = Vec::new();
        for _ in 0..6 {
            render(&mut renderer, &gpu, &far);
            resident.push(renderer.texture_residency().resident_bytes);
        }
        let steps = [1024, 512, 256, 128, 128, 128].map(rgba8_chain_bytes);
        assert_eq!(
            resident, steps,
            "a wait of two frames, then one level a frame"
        );
        let evicted = renderer.texture_residency();
        assert_eq!((evicted.evictions, evicted.streaming), (3, 1));
        // A copied eviction writes nothing from the CPU.
        assert_eq!(
            evicted.step_written_bytes == full.step_written_bytes,
            streaming.gpu_copies
        );

        let back = frames(&mut renderer, &gpu, &near, 4);
        assert_eq!(renderer.texture_residency().streaming, 0);
        assert!(
            back == expected,
            "a texture evicted and streamed back must render as the whole upload"
        );
        let error = pollster::block_on(scope.pop());
        assert!(error.is_none(), "eviction failed validation: {error:?}");
    }
}

#[test]
fn a_camera_at_the_boundary_does_not_thrash() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer_with(&gpu, &cube_with(checker(1024, 0)), QUICK_EVICTION);
    frames(&mut renderer, &gpu, &camera_at(1.0), 4);
    // A small range stays within the margin; far every other frame restarts the wait each time.
    for (near, far) in [(1.0, 1.6), (1.0, 200.0)] {
        for i in 0..12 {
            let radius = if i % 2 == 0 { near } else { far };
            render(&mut renderer, &gpu, &camera_at(radius));
        }
    }
    let residency = renderer.texture_residency();
    assert_eq!(residency.evictions, 0, "{residency:?}");
    assert_eq!(residency.resident_bytes, residency.full_bytes);
}

/// The copy path and the CPU path end in the same frame; the copy path writes only each step's new level.
#[test]
fn copying_resident_levels_renders_identically_and_writes_only_the_new_level() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    for (name, texture) in [("RGBA8", checker(1024, 20)), ("BC1", bc1_noise(1024, 3))] {
        let scene = cube_with(texture);
        let near = camera_at(1.0);
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut copied = renderer_with(&gpu, &scene, TextureStreaming::INTERACTIVE);
        let cpu_path = TextureStreaming {
            gpu_copies: false,
            ..TextureStreaming::INTERACTIVE
        };
        let mut written = renderer_with(&gpu, &scene, cpu_path);
        let copied_frame = frames(&mut copied, &gpu, &near, 4);
        let written_frame = frames(&mut written, &gpu, &near, 4);
        let error = pollster::block_on(scope.pop());
        assert!(error.is_none(), "copying failed validation: {error:?}");
        assert!(copied_frame == written_frame, "the two paths must agree");
        let (c, w) = (copied.texture_residency(), written.texture_residency());
        assert_eq!((c.streaming, w.streaming), (0, 0));
        eprintln!(
            "{name} 128 -> 1024 in three steps: CPU path writes {} B, copy path writes {} B and copies {} B",
            w.step_written_bytes,
            c.step_written_bytes,
            c.step_copied_bytes,
        );
        // Three steps write 256, 512 and 1024 once each, against every resident level again on the CPU path.
        assert_eq!(
            c.step_written_bytes + c.step_copied_bytes,
            w.step_written_bytes
        );
        assert_eq!(w.step_copied_bytes, 0);
        assert!(c.step_written_bytes * 10 < w.step_written_bytes * 8);
    }
}

/// Sixteen cubes whose chains do not all fit the CPU budget: the dropped ones rebuild from the scene's images.
#[test]
fn chains_past_the_cpu_budget_rebuild_and_render_identically() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = sixteen_cubes(|i| checker(256, (i * 15) as u8));
    let budget = 3 * rgba8_chain_bytes(256);
    let bounded = TextureStreaming {
        cpu_bytes: budget,
        ..TextureStreaming::INTERACTIVE
    };
    let mut renderer = renderer_with(&gpu, &scene, bounded);
    let residency = renderer.texture_residency();
    assert_eq!(residency.cpu_bytes, budget, "three of sixteen chains stay");
    let mut unbounded = renderer_with(&gpu, &scene, TextureStreaming::INTERACTIVE);
    assert_eq!(
        unbounded.texture_residency().cpu_bytes,
        16 * rgba8_chain_bytes(256)
    );

    // Every texture steps to its finest: thirteen of them from a rebuilt chain.
    renderer.finish_texture_streaming(&gpu);
    unbounded.finish_texture_streaming(&gpu);
    let camera = OrbitCamera {
        target: Vec3::new(15.0, 0.0, 0.0),
        ..camera_at(4.0)
    };
    let bounded_frame = render(&mut renderer, &gpu, &camera);
    let unbounded_frame = render(&mut unbounded, &gpu, &camera);
    let after = renderer.texture_residency();
    assert_eq!(after.streaming, 0, "{after:?}");
    assert_eq!(after.regenerated, 13, "{after:?}");
    assert!(after.cpu_bytes <= budget, "{after:?}");
    assert_eq!(unbounded.texture_residency().regenerated, 0);
    assert!(
        bounded_frame == unbounded_frame,
        "a rebuilt chain must upload the same levels"
    );

    // Off keeps no chain at all.
    let off = renderer_with(&gpu, &scene, TextureStreaming::default());
    assert_eq!(off.texture_residency().cpu_bytes, 0);
}

/// The measured effect of eviction: sixteen 1024-texel textures, all resident, then the camera flies away.
/// Prints residency per frame; the 8 MiB budget spreads the evictions over a few frames.
#[test]
fn a_fly_away_returns_residency_to_the_tail() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = sixteen_cubes(|i| checker(1024, (i * 15) as u8));
    let mut renderer = renderer_with(&gpu, &scene, QUICK_EVICTION);
    let row = |radius| OrbitCamera {
        target: Vec3::new(15.0, 0.0, 0.0),
        ..camera_at(radius)
    };
    renderer.finish_texture_streaming(&gpu);
    render(&mut renderer, &gpu, &row(4.0));
    let close = renderer.texture_residency();
    assert_eq!(close.resident_bytes, close.full_bytes);
    let mut trace = vec![mib(close.resident_bytes)];
    for _ in 0..12 {
        render(&mut renderer, &gpu, &row(400.0));
        trace.push(mib(renderer.texture_residency().resident_bytes));
    }
    let away = renderer.texture_residency();
    eprintln!(
        "16 x 1024^2 RGBA8 fly-away, MiB resident per frame: {trace:.2?}; {} evictions, \
         {} B copied, {} B written by steps",
        away.evictions, away.step_copied_bytes, away.step_written_bytes
    );
    assert_eq!(away.resident_bytes, 16 * rgba8_chain_bytes(128));
    assert!(close.resident_bytes > 20 * away.resident_bytes);
}
