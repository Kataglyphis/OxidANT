//! Images over WebGPU's default 8192-texel side must reach the GPU downsampled, never fail validation.

use std::sync::Arc;

use kataglyphis_webgpu_renderer::scene::{CpuSampler, CpuTexture, CpuTextureRef};
use kataglyphis_webgpu_renderer::{
    load_gltf, EquirectImage, ForwardRenderer, GpuContext, IblEnvironment, OrbitCamera,
};

/// A glTF texture wider than the device allows panicked at upload on native.
#[test]
fn a_material_texture_over_the_side_limit_uploads_downsampled() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (width, height) = (gpu.device.limits().max_texture_dimension_2d + 8, 4);
    let red = CpuTexture {
        width,
        height,
        rgba8: [255, 0, 0, 255].repeat((width * height) as usize),
        compressed: None,
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    let mut scene = load_gltf(path).expect("cube.gltf must load");
    let material = &mut scene.primitives[0].material;
    material.base_color = [1.0; 4];
    material.base_color_texture = Some(CpuTextureRef {
        texture: Arc::new(red),
        sampler: CpuSampler::default(),
        srgb: true,
    });

    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let pixels = renderer
        .render_to_pixels(&gpu, 128, 128, &OrbitCamera::default())
        .expect("render");
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "an oversized texture failed validation: {error:?}"
    );
    let red_pixels = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 110 && p[0] as i32 > p[1] as i32 + 40 && p[0] as i32 > p[2] as i32 + 40)
        .count();
    assert!(
        red_pixels > 200,
        "the downsampled texture must still show, {red_pixels} red pixels"
    );
}

/// `decode_hdr` accepts a 16k equirect, which the bake then uploaded at full width.
#[test]
fn an_equirect_over_the_side_limit_bakes_downsampled() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (width, height) = (2 * gpu.device.limits().max_texture_dimension_2d + 16, 2);
    let image = EquirectImage::new(width, height, vec![1.0; (width * height * 4) as usize])
        .expect("a well-sized image");

    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let environment = IblEnvironment::bake(&gpu, &image);
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "an oversized equirect failed validation: {error:?}"
    );
    assert!(environment.max_prefiltered_mip() > 0.0);
}
