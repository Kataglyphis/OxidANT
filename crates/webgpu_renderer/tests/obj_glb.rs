//! The C++ engine's MaskCardDiscardsCutoutTexelsVisually, for an OBJ the renderer opens in memory.

use std::path::Path;

use kataglyphis_assets::obj::obj_to_glb;
use kataglyphis_webgpu_renderer::asset::gltf_loader::load_gltf_slice;
use kataglyphis_webgpu_renderer::scene::{AlphaMode, CpuScene};
use kataglyphis_webgpu_renderer::{ForwardRenderer, GpuContext, OrbitCamera};

/// `crates/assets/tests/assets/obj_mask`: a 2x2 card, `map_Kd red.bmp`, and `map_d half_mask.tga` clearing its left half.
fn mask_card() -> CpuScene {
    let obj =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/obj_mask/card.obj");
    load_gltf_slice(&obj_to_glb(&obj).expect("converts")).expect("the GLB loads")
}

fn red_pixels(gpu: &GpuContext, scene: &CpuScene) -> usize {
    let mut renderer = ForwardRenderer::new(gpu, 64, 64);
    renderer.upload_scene(gpu, scene);
    let camera = OrbitCamera {
        radius: 3.0,
        pitch_deg: 89.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(gpu, 64, 64, &camera)
        .expect("headless render");
    let red = |p: &&[u8; 4]| {
        p[0] > 80
            && i32::from(p[0]) > i32::from(p[1]) + 40
            && i32::from(p[0]) > i32::from(p[2]) + 40
    };
    pixels.as_chunks::<4>().0.iter().filter(red).count()
}

#[test]
fn an_obj_card_with_a_map_d_mask_draws_only_its_uncut_half() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let masked = mask_card();
    let mut opaque = masked.clone();
    opaque.primitives[0].material.alpha_mode = AlphaMode::Opaque;
    let (cut, whole) = (red_pixels(&gpu, &masked), red_pixels(&gpu, &opaque));
    assert!(
        whole > 1000,
        "the opaque card must fill the frame, {whole} red pixels"
    );
    let share = cut as f64 / whole as f64;
    eprintln!("masked card: {cut} of {whole} red pixels");
    assert!(
        (0.35..0.65).contains(&share),
        "the masked card keeps {cut} of the opaque card's {whole} red pixels"
    );
}
