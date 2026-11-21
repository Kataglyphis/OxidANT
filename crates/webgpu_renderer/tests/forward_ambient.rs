//! `fs_main`'s ambient term: fallback specular, the ambient slider and the environment intensity.
//! See <https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/source/shader-sharing.md>.

use glam::{Mat4, Vec3};
use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::scene::{CpuMaterial, CpuPrimitive, CpuScene, Vertex};
use kataglyphis_webgpu_renderer::{
    load_gltf, EquirectImage, ForwardRenderer, GpuContext, OrbitCamera,
};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf")
}

fn render(renderer: &mut ForwardRenderer, gpu: &GpuContext) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, 128, 128, &OrbitCamera::default())
        .expect("headless render must succeed")
}

/// Mean R+G+B over the red-dominant cube pixels, which the blue-dominant sky never matches.
fn cube_luma(pixels: &[u8]) -> f64 {
    let mut total = 0u64;
    let mut count = 0u64;
    for pixel in pixels.as_chunks::<4>().0 {
        if pixel[0] > pixel[2] {
            total += pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64;
            count += 1;
        }
    }
    assert!(count > 100, "found only {count} cube pixels to compare");
    total as f64 / count as f64
}

#[test]
fn a_metal_without_an_environment_still_gets_ambient_specular() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let mut scene = load_gltf(cube_path()).expect("cube.gltf must load");
    // Full metal: a diffuse-only fallback renders it exactly black.
    scene.primitives[0].material.metallic_factor = 1.0;
    scene.primitives[0].material.roughness_factor = 0.05;

    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    assert!(!renderer.environment_enabled());
    // No direct light, so only ambient remains.
    renderer.light_color_intensity.w = 0.0;
    renderer.light_dir_ambient.w = 1.0;

    let pixels = render(&mut renderer, &gpu);
    let luma = cube_luma(&pixels);
    assert!(
        luma > 10.0,
        "a metal cube lit only by analytic ambient specular should not render black, got mean luma {luma:.2}"
    );
}

#[test]
fn environment_intensity_scales_the_ambient_term() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");

    // Set before the bake here; `ibl_intensity_takes_effect_after_set_environment` covers after.
    let mut dim = ForwardRenderer::new(&gpu, 128, 128);
    dim.upload_scene(&gpu, &scene);
    dim.ibl_intensity = 1.0;
    dim.set_environment(&gpu, &EquirectImage::constant(64, 32, [3.0, 3.0, 3.0]));
    let dim_luma = cube_luma(&render(&mut dim, &gpu));

    let mut bright = ForwardRenderer::new(&gpu, 128, 128);
    bright.upload_scene(&gpu, &scene);
    bright.ibl_intensity = 2.0;
    bright.set_environment(&gpu, &EquirectImage::constant(64, 32, [3.0, 3.0, 3.0]));
    let bright_luma = cube_luma(&render(&mut bright, &gpu));

    eprintln!("cube mean luma: ibl_intensity 1.0 -> {dim_luma:.2}, 2.0 -> {bright_luma:.2}");
    assert!(
        bright_luma > dim_luma + 5.0,
        "doubling environment intensity should brighten the ambient term: {dim_luma:.2} -> {bright_luma:.2}"
    );
}

#[test]
fn ambient_strength_scales_the_ambient_term() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");

    let render_at_strength = |strength: f32| -> Vec<u8> {
        let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
        renderer.upload_scene(&gpu, &scene);
        // Isolate the ambient term from the direct sun.
        renderer.light_color_intensity.w = 0.0;
        renderer.light_dir_ambient.w = strength;
        render(&mut renderer, &gpu)
    };

    let dim_luma = cube_luma(&render_at_strength(0.2));
    let bright_luma = cube_luma(&render_at_strength(1.0));

    eprintln!("cube mean luma: ambient 0.2 -> {dim_luma:.2}, 1.0 -> {bright_luma:.2}");
    assert!(
        bright_luma > dim_luma + 5.0,
        "raising light_dir_ambient.w should brighten the ambient term: {dim_luma:.2} -> {bright_luma:.2}"
    );
}

/// `ibl_intensity` is a live knob: changing it after the bake must change the next frame.
#[test]
fn ibl_intensity_takes_effect_after_set_environment() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    renderer.set_environment(&gpu, &EquirectImage::constant(64, 32, [3.0, 3.0, 3.0]));
    let before = cube_luma(&render(&mut renderer, &gpu));
    renderer.ibl_intensity = 2.0;
    let after = cube_luma(&render(&mut renderer, &gpu));

    eprintln!("cube mean luma: ibl_intensity 1.0 -> {before:.2}, then 2.0 -> {after:.2}");
    assert!(
        after > before + 5.0,
        "raising ibl_intensity after set_environment must brighten the ambient term: \
         {before:.2} -> {after:.2}"
    );
}

/// A 2 x 2 white card in the XY plane facing +Z.
fn white_card(metallic: f32, roughness: f32) -> CpuScene {
    let vertices = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .iter()
        .map(|&[x, y]: &[f32; 2]| Vertex {
            position: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [(x + 1.0) * 0.5, (1.0 - y) * 0.5],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0; 4],
            uv1: [0.0; 2],
        })
        .collect();
    CpuScene {
        primitives: vec![CpuPrimitive {
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            transform: Mat4::IDENTITY,
            node_index: None,
            skin_index: None,
            material: CpuMaterial {
                metallic_factor: metallic,
                roughness_factor: roughness,
                ..CpuMaterial::default()
            },
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
        }],
        ..CpuScene::default()
    }
}

/// Mean HDR radiance over the 8 x 8 pixels at the centre of a 64 x 64 frame.
fn centre_radiance(renderer: &ForwardRenderer, gpu: &GpuContext) -> f32 {
    let bits = renderer.read_hdr_target(gpu).expect("reads back");
    let hdr = bits.as_chunks::<4>().0;
    let mut sum = 0.0;
    for y in 28..36 {
        for x in 28..36 {
            let p = hdr[y * 64 + x];
            sum += (half_to_f32(p[0]) + half_to_f32(p[1]) + half_to_f32(p[2])) / 3.0;
        }
    }
    sum / 64.0
}

/// The ambient term's white furnace: a white card returns all of a uniform environment, metal or dielectric, in both paths.
#[test]
fn a_white_card_returns_a_uniform_environment_whole() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut off = Vec::new();
    for path in RasterPath::ALL {
        let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
        renderer.raster_path = path;
        renderer.light_color_intensity.w = 0.0;
        renderer.light_dir_ambient.w = 1.0;
        renderer.bloom_strength = 0.0;
        renderer.ssao_strength = 0.0;
        renderer.set_environment(&gpu, &EquirectImage::constant(64, 32, [1.0, 1.0, 1.0]));
        for (metallic, roughness) in [
            (1.0, 0.3),
            (1.0, 0.7),
            (1.0, 1.0),
            (0.0, 0.25),
            (0.0, 0.5),
            (0.0, 1.0),
        ] {
            renderer.upload_scene(&gpu, &white_card(metallic, roughness));
            for mu in [1.0_f32, 0.5, 0.25] {
                let camera = OrbitCamera {
                    target: Vec3::ZERO,
                    radius: 2.0,
                    // The card faces +Z, so the eye's yaw from +X is asin(mu).
                    yaw_deg: mu.asin().to_degrees(),
                    pitch_deg: 0.0,
                    ..OrbitCamera::default()
                };
                renderer
                    .render_to_pixels(&gpu, 64, 64, &camera)
                    .expect("renders");
                let radiance = centre_radiance(&renderer, &gpu);
                eprintln!(
                    "{path:?}, metallic {metallic}, roughness {roughness}, mu {mu}: {radiance:.4}"
                );
                if (radiance - 1.0).abs() >= 0.005 {
                    off.push((path, metallic, roughness, mu, radiance));
                }
            }
        }
    }
    // Exact up to half floats (measured 0.9995-1.0000); single scattering gave a 0.7 metal 0.67.
    assert!(
        off.is_empty(),
        "white cards off a uniform environment (path, metallic, roughness, mu, radiance): {off:?}"
    );
}
