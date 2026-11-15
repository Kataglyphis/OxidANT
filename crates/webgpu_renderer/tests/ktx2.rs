//! KTX2 (block-compressed texture) loading.

use std::sync::Arc;

use kataglyphis_webgpu_renderer::asset::ktx2_loader::load_ktx2;
use kataglyphis_webgpu_renderer::scene::{
    CompressedFormat, CompressedTexture, CpuSampler, CpuTexture, CpuTextureRef,
};
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

const RED_BC1: &[u8] = include_bytes!("assets/red_bc1.ktx2");

/// One BC1 block whose every texel is pure green (RGB565 0x07E0).
const GREEN_BC1_BLOCK: [u8; 8] = [0xE0, 0x07, 0xE0, 0x07, 0, 0, 0, 0];

fn green_blocks(count: usize) -> Vec<u8> {
    GREEN_BC1_BLOCK.repeat(count)
}

/// A BC1 KTX2 container around `levels` (level 0 first), borrowing red_bc1.ktx2's format descriptor.
fn bc1_ktx2(width: u32, height: u32, levels: &[Vec<u8>]) -> Vec<u8> {
    let dfd = &RED_BC1[0x68..0x94];
    let dfd_offset = 80 + 24 * levels.len();
    let data_start = dfd_offset + dfd.len();
    let mut out = RED_BC1[..12].to_vec();
    for word in [133u32, 1, width, height, 0, 0, 1, levels.len() as u32, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for word in [dfd_offset, dfd.len(), data_start, 0] {
        out.extend_from_slice(&(word as u32).to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 16]);
    let mut offset = data_start;
    for level in levels {
        for word in [offset, level.len(), level.len()] {
            out.extend_from_slice(&(word as u64).to_le_bytes());
        }
        offset += level.len();
    }
    out.extend_from_slice(dfd);
    for level in levels {
        out.extend_from_slice(level);
    }
    out
}

/// The test cube with `texture` as its base colour, rendered inside a validation scope.
fn render_cube_with(gpu: &GpuContext, texture: CpuTexture) -> (Option<wgpu::Error>, Vec<u8>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    let mut scene = load_gltf(path).expect("cube.gltf must load");
    let material = &mut scene.primitives[0].material;
    material.base_color = [1.0; 4];
    material.base_color_texture = Some(CpuTextureRef {
        texture: Arc::new(texture),
        sampler: CpuSampler::default(),
        srgb: true,
    });
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = ForwardRenderer::new(gpu, 128, 128);
    renderer.upload_scene(gpu, &scene);
    let pixels = renderer
        .render_to_pixels(gpu, 128, 128, &OrbitCamera::default())
        .expect("render");
    (pollster::block_on(scope.pop()), pixels)
}

fn green_pixels(pixels: &[u8]) -> usize {
    let green = |p: &&[u8; 4]| {
        p[1] > 80 && p[1] as i32 > p[0] as i32 + 40 && p[1] as i32 > p[2] as i32 + 40
    };
    pixels.as_chunks::<4>().0.iter().filter(green).count()
}

/// The adapter for the upload tests, or `None` (a skip) where BC is legitimately missing.
fn bc_gpu() -> Option<GpuContext> {
    let gpu = GpuContext::headless_or_skip()?;
    if !gpu.supports_bc {
        // Every CI adapter (lavapipe, WARP) offers BC, so under REQUIRE_GPU its absence is a regression.
        assert!(
            !GpuContext::gpu_required(),
            "KATAGLYPHIS_REQUIRE_GPU is set but the adapter lacks TEXTURE_COMPRESSION_BC"
        );
        eprintln!("SKIP: adapter lacks TEXTURE_COMPRESSION_BC");
        return None;
    }
    Some(gpu)
}

#[test]
fn loads_bc1_ktx2() {
    let texture = load_ktx2(RED_BC1).expect("red_bc1.ktx2 must load");
    assert_eq!((texture.width, texture.height), (8, 8));
    assert!(texture.rgba8.is_empty(), "compressed payload stays packed");

    let compressed = texture
        .compressed
        .as_ref()
        .expect("BC1 file must produce a compressed payload");
    assert_eq!(compressed.format, CompressedFormat::Bc1RgbaUnorm);
    assert_eq!(compressed.format.block_bytes(), 8);
    assert_eq!(compressed.mips.len(), 1);
    // 8x8 at 4x4 blocks = 4 blocks * 8 bytes.
    assert_eq!(compressed.mips[0].len(), 32);
}

#[test]
fn rejects_non_ktx2_input() {
    assert!(load_ktx2(b"definitely not a ktx2 file").is_err());
}

#[test]
fn the_test_container_builder_round_trips_the_fixture() {
    // Guards the builder the upload tests rely on: rebuilt from its own level, the fixture loads the same.
    let rebuilt = load_ktx2(&bc1_ktx2(8, 8, &[RED_BC1[0x98..0xb8].to_vec()])).expect("rebuilt");
    let original = load_ktx2(RED_BC1).expect("fixture");
    assert_eq!((rebuilt.width, rebuilt.height), (8, 8));
    assert_eq!(
        rebuilt.compressed.unwrap().mips,
        original.compressed.unwrap().mips
    );
}

#[test]
fn a_base_that_is_not_whole_blocks_is_refused_at_load() {
    // WebGPU cannot create a 6x6 BC texture, so the loader must refuse it for callers to fall back.
    let six = bc1_ktx2(6, 6, &[green_blocks(4)]);
    let err = load_ktx2(&six).expect_err("a 6x6 BC1 base must be refused");
    assert!(
        format!("{err:#}").contains("4x4"),
        "the error must name the block rule, got: {err:#}"
    );
}

#[test]
fn uploads_bc1_when_supported() {
    let Some(gpu) = bc_gpu() else {
        return;
    };
    // Block layout and rows-per-image are easy to get wrong; wgpu validation must stay quiet.
    let texture = load_ktx2(RED_BC1).expect("load");
    assert!(texture.compressed.is_some());
    let (error, _) = render_cube_with(&gpu, texture);
    assert!(
        error.is_none(),
        "uploading red_bc1.ktx2 failed validation: {error:?}"
    );
}

/// Levels below 4x4 are one block: copying their logical size fails wgpu's whole-block rule.
#[test]
fn full_bc1_mip_chains_upload_and_sample() {
    let Some(gpu) = bc_gpu() else {
        return;
    };
    let eight = bc1_ktx2(
        8,
        8,
        &[
            green_blocks(4),
            green_blocks(1),
            green_blocks(1),
            green_blocks(1),
        ],
    );
    let twelve = bc1_ktx2(
        12,
        12,
        &[
            green_blocks(9),
            green_blocks(4),
            green_blocks(1),
            green_blocks(1),
        ],
    );
    for (name, file) in [("8x8", eight), ("12x12", twelve)] {
        let texture = load_ktx2(&file).expect("a full BC1 chain must load");
        assert_eq!(texture.compressed.as_ref().unwrap().mips.len(), 4);
        let (error, pixels) = render_cube_with(&gpu, texture);
        assert!(error.is_none(), "{name} chain failed validation: {error:?}");
        let green = green_pixels(&pixels);
        assert!(
            green > 200,
            "{name} chain: the green texture must show, {green} green pixels"
        );
    }
}

/// A payload built in code bypasses the loader; the upload must still refuse it, not fail validation.
#[test]
fn an_invalid_compressed_payload_falls_back_instead_of_failing_validation() {
    let Some(gpu) = bc_gpu() else {
        return;
    };
    let six = CpuTexture {
        width: 6,
        height: 6,
        rgba8: Vec::new(),
        compressed: Some(CompressedTexture {
            format: CompressedFormat::Bc1RgbaUnorm,
            mips: vec![green_blocks(4)],
            declared_srgb: Some(true),
        }),
    };
    let (error, pixels) = render_cube_with(&gpu, six);
    assert!(
        error.is_none(),
        "a 6x6 BC1 payload must fall back, not fail validation: {error:?}"
    );
    assert_eq!(
        green_pixels(&pixels),
        0,
        "the fallback is white, not the payload"
    );
}

/// One 4x4 BC5 block: two BC4 halves whose endpoints are both 128, so every texel is (128, 128).
fn flat_bc5_normal_map() -> kataglyphis_webgpu_renderer::scene::CpuTextureRef {
    use kataglyphis_webgpu_renderer::scene::{
        CompressedTexture, CpuSampler, CpuTexture, CpuTextureRef,
    };
    CpuTextureRef {
        texture: std::sync::Arc::new(CpuTexture {
            width: 4,
            height: 4,
            rgba8: Vec::new(),
            compressed: Some(CompressedTexture {
                format: CompressedFormat::Bc5RgUnorm,
                mips: vec![[128, 128, 0, 0, 0, 0, 0, 0, 128, 128, 0, 0, 0, 0, 0, 0].to_vec()],
                declared_srgb: None,
            }),
        }),
        sampler: CpuSampler::default(),
        srgb: false,
    }
}

/// A 4x4 up-facing plane with an optional normal map, its tangent along +X.
fn lit_plane(
    normal_texture: Option<kataglyphis_webgpu_renderer::scene::CpuTextureRef>,
) -> kataglyphis_webgpu_renderer::CpuPrimitive {
    use kataglyphis_webgpu_renderer::scene::{CpuMaterial, Vertex};
    kataglyphis_webgpu_renderer::CpuPrimitive {
        vertices: [[-2.0f32, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]]
            .iter()
            .map(|[x, z]| Vertex {
                position: [*x, 0.0, *z],
                normal: [0.0, 1.0, 0.0],
                uv: [(x + 2.0) / 4.0, (z + 2.0) / 4.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0.0; 4],
                weights: [0.0; 4],
                color: [1.0; 4],
                uv1: [0.0, 0.0],
            })
            .collect(),
        indices: vec![0, 2, 1, 0, 3, 2],
        transform: glam::Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            metallic_factor: 0.0,
            roughness_factor: 0.8,
            normal_texture,
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// Mean R+G+B over the centre of a 64x64 frame of `plane` seen from above.
fn centre_luma(
    gpu: &kataglyphis_webgpu_renderer::GpuContext,
    plane: kataglyphis_webgpu_renderer::CpuPrimitive,
) -> f64 {
    use kataglyphis_webgpu_renderer::{CpuScene, ForwardRenderer, OrbitCamera};
    let mut renderer = ForwardRenderer::new(gpu, 64, 64);
    renderer.upload_scene(
        gpu,
        &CpuScene {
            primitives: vec![plane],
            ..CpuScene::default()
        },
    );
    let camera = OrbitCamera {
        radius: 3.0,
        pitch_deg: 70.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(gpu, 64, 64, &camera)
        .expect("headless render must succeed");
    let centre: Vec<f64> = (24..40)
        .flat_map(|y| (24..40).map(move |x| (y * 64 + x) * 4))
        .map(|i| f64::from(pixels[i]) + f64::from(pixels[i + 1]) + f64::from(pixels[i + 2]))
        .collect();
    centre.iter().sum::<f64>() / centre.len() as f64
}

/// BC5 holds a normal map's x and y only and samples blue as 0, so z must be rebuilt, not read.
#[test]
fn a_bc5_normal_map_shades_like_a_flat_one() {
    use kataglyphis_webgpu_renderer::GpuContext;

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !gpu.supports_bc {
        assert!(
            !GpuContext::gpu_required(),
            "KATAGLYPHIS_REQUIRE_GPU is set but the adapter lacks TEXTURE_COMPRESSION_BC"
        );
        eprintln!("SKIP: adapter lacks TEXTURE_COMPRESSION_BC");
        return;
    }
    let without_map = centre_luma(&gpu, lit_plane(None));
    let with_bc5 = centre_luma(&gpu, lit_plane(Some(flat_bc5_normal_map())));
    assert!(
        (with_bc5 - without_map).abs() < without_map * 0.03,
        "a flat BC5 normal map shades {with_bc5:.1} where no normal map shades {without_map:.1}: \
         its z came from the blue channel BC5 does not have"
    );
}
