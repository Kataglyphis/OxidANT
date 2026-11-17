//! Render scale on the GPU: the scene targets shrink, the output keeps its size and shows the same picture.

use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::render_scale::RenderScale;
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: (u32, u32) = (256, 192);

/// The red cube's share of the output's pixels.
fn red_share(pixels: &[u8]) -> f64 {
    let pixels = pixels.as_chunks::<4>().0;
    let red = pixels
        .iter()
        .filter(|p| p[0] > 110 && p[0] > p[1] + 40 && p[0] > p[2] + 40)
        .count();
    red as f64 / pixels.len() as f64
}

fn render(gpu: &GpuContext, path: RasterPath, scale: f32) -> (Vec<u8>, usize) {
    let path_to_cube =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    let scene = load_gltf(path_to_cube).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(gpu, SIZE.0, SIZE.1);
    renderer.upload_scene(gpu, &scene);
    renderer.raster_path = path;
    renderer.render_scale = RenderScale {
        scale,
        ..RenderScale::default()
    };
    let pixels = renderer
        .render_to_pixels(gpu, SIZE.0, SIZE.1, &OrbitCamera::default())
        .expect("headless render must succeed");
    let hdr = renderer
        .read_hdr_target(gpu)
        .expect("the HDR target reads back");
    (pixels, hdr.len() / 4)
}

#[test]
fn a_half_scale_frame_fills_the_output_with_the_same_picture_from_a_quarter_of_the_pixels() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    for path in RasterPath::ALL {
        let (full, full_texels) = render(&gpu, path, 1.0);
        let (half, half_texels) = render(&gpu, path, 0.5);
        assert_eq!(
            half.len(),
            full.len(),
            "{path:?}: the output keeps its size"
        );
        assert_eq!(full_texels, (SIZE.0 * SIZE.1) as usize, "{path:?}");
        assert_eq!(half_texels, (SIZE.0 / 2 * SIZE.1 / 2) as usize, "{path:?}");
        let (full_share, half_share) = (red_share(&full), red_share(&half));
        assert!(full_share > 0.05, "{path:?}: the cube shows ({full_share})");
        assert!(
            (half_share - full_share).abs() < 0.2 * full_share,
            "{path:?}: the upsampled cube covers {half_share} of the frame, the full one {full_share}"
        );
        let corner = &half[..4];
        assert!(
            corner[2] > corner[0],
            "{path:?}: the sky fills the corner, got {corner:?}"
        );
    }
}
