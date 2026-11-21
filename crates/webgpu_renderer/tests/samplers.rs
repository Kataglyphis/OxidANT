//! glTF sampler filters asserted on the frame, where a wrong mip choice shows as a blended colour.

use std::sync::Arc;

use glam::Mat4;
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::scene::{
    CpuMaterial, CpuPrimitive, CpuSampler, CpuTexture, CpuTextureRef, Vertex,
};
use kataglyphis_webgpu_renderer::{CpuScene, ForwardRenderer, GpuContext, OrbitCamera};

/// A 128x128 texture of single red and blue texels in a checkerboard.
fn checker() -> CpuTexture {
    let rgba8 = (0..128 * 128)
        .flat_map(|i| {
            if (i % 128 + i / 128) % 2 == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    CpuTexture {
        width: 128,
        height: 128,
        rgba8,
        compressed: None,
    }
}

/// An unlit 2x2 quad in the XY plane, facing +Z, textured with `checker` through `sampler`.
fn checker_quad(sampler: CpuSampler) -> CpuScene {
    let corner = |x: f32, y: f32| Vertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [(x + 1.0) * 0.5, (1.0 - y) * 0.5],
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0; 4],
        uv1: [0.0; 2],
    };
    let material = CpuMaterial {
        unlit: true,
        base_color_texture: Some(CpuTextureRef {
            texture: Arc::new(checker()),
            sampler,
            srgb: true,
        }),
        ..CpuMaterial::default()
    };
    CpuScene {
        primitives: vec![CpuPrimitive {
            vertices: vec![
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            transform: Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material,
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }],
        ..CpuScene::default()
    }
}

/// glTF's NEAREST min filter means level 0; the box-filtered mips average red and blue to purple.
#[test]
fn a_minified_nearest_texture_samples_only_its_own_texels() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // 16 pixels across about 53 texels: any mip level above 0 is a red-blue average.
    let (width, height) = (16, 16);
    let mut renderer = ForwardRenderer::new(&gpu, width, height);
    renderer.bloom_strength = 0.0;
    renderer.ssao_strength = 0.0;
    // FXAA would blend neighbouring texels, the very mix this test looks for.
    renderer.fxaa = FxaaMode::Off;
    let nearest = CpuSampler {
        min_nearest: true,
        mip_nearest: true,
        mipmaps: false,
        ..CpuSampler::default()
    };
    renderer.upload_scene(&gpu, &checker_quad(nearest));
    let camera = OrbitCamera {
        radius: 1.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, width, height, &camera)
        .expect("render");
    let blended = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 80 && p[2] > 80)
        .count();
    let pure = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| (p[0] > 80) != (p[2] > 80))
        .count();
    assert_eq!(blended, 0, "no pixel may mix red and blue texels");
    assert_eq!(
        pure,
        (width * height) as usize,
        "the quad must fill the frame with texels"
    );
}
