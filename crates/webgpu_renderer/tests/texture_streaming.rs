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
        "16 x 2048^2 RGBA8: whole {:.1} MiB in {whole_time:?}, tail {:.2} MiB in {tail_time:?}",
        mib(whole.resident_bytes),
        mib(tail.resident_bytes),
    );
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
