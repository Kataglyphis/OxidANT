//! KTX2 (block-compressed texture) loading.

use kataglyphis_webgpu_renderer::asset::ktx2_loader::load_ktx2;
use kataglyphis_webgpu_renderer::scene::CompressedFormat;

const RED_BC1: &[u8] = include_bytes!("assets/red_bc1.ktx2");

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
fn uploads_bc1_when_supported() {
    use kataglyphis_webgpu_renderer::GpuContext;
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !gpu.supports_bc {
        // Every CI adapter (lavapipe, WARP) offers BC, so under REQUIRE_GPU its absence is a regression.
        assert!(
            !GpuContext::gpu_required(),
            "KATAGLYPHIS_REQUIRE_GPU is set but the adapter lacks TEXTURE_COMPRESSION_BC"
        );
        eprintln!("SKIP: adapter lacks TEXTURE_COMPRESSION_BC");
        return;
    }
    // Block layout and rows-per-image are easy to get wrong; wgpu validation must stay quiet.
    let texture = load_ktx2(RED_BC1).expect("load");
    let scene = kataglyphis_webgpu_renderer::CpuScene {
        primitives: vec![],
        ..Default::default()
    };
    let mut renderer = kataglyphis_webgpu_renderer::ForwardRenderer::new(&gpu, 64, 64);
    renderer.upload_scene(&gpu, &scene);
    assert!(texture.compressed.is_some());
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
