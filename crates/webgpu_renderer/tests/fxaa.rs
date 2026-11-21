//! FXAA after the tonemap: it smooths the deferred path's one-sample staircase and leaves every flat region alone.

use std::path::Path;

use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::scene::Vertex;
use kataglyphis_webgpu_renderer::{
    load_gltf, CpuMaterial, CpuPrimitive, CpuScene, ForwardRenderer, GpuContext, OrbitCamera,
};

const W: u32 = 256;
const H: u32 = 256;

/// A dark unlit card filling the left half, its right edge 8.6 degrees off vertical: steps about 6.6 rows long.
fn sloped_card() -> CpuScene {
    let corners = [[-5.0f32, -3.0], [-0.45, -3.0], [0.45, 3.0], [-5.0, 3.0]];
    let vertices = corners
        .iter()
        .map(|[x, y]| Vertex {
            position: [*x, *y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0; 4],
            uv1: [0.0, 0.0],
        })
        .collect();
    let card = CpuPrimitive {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
        transform: glam::Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            base_color: [0.04, 0.04, 0.04, 1.0],
            unlit: true,
            double_sided: true,
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    };
    CpuScene {
        primitives: vec![card],
        ..CpuScene::default()
    }
}

/// From +z, head on, so the card's plane faces the camera and the sky fills the right half.
fn head_on() -> OrbitCamera {
    OrbitCamera {
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    }
}

/// One frame of `scene` on `path` with `fxaa`, without bloom and SSAO, so the step is the only blur.
fn frame(
    gpu: &GpuContext,
    (scene, camera): (&CpuScene, &OrbitCamera),
    (path, fxaa): (RasterPath, FxaaMode),
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let mut renderer = ForwardRenderer::new(gpu, W, H);
    renderer.upload_scene(gpu, scene);
    renderer.bloom_strength = 0.0;
    renderer.ssao_strength = 0.0;
    renderer.raster_path = path;
    renderer.fxaa = fxaa;
    renderer
        .render_to_pixels_with_format(gpu, W, H, camera, format)
        .expect("headless render must succeed")
}

fn luma(frame: &[u8], x: u32, y: u32) -> f64 {
    let i = ((y * W + x) * 4) as usize;
    0.2126 * f64::from(frame[i])
        + 0.7152 * f64::from(frame[i + 1])
        + 0.0722 * f64::from(frame[i + 2])
}

/// How the card's edge is drawn across the middle rows.
#[derive(Debug)]
struct EdgeStats {
    /// Rows holding a pixel between the card's and the sky's level by over a tenth of the gap.
    blended_rows: usize,
    rows: usize,
    /// Squared luma steps between horizontal neighbours across the edge band: one hard step costs the most.
    gradient_energy: f64,
}

fn edge_stats(frame: &[u8]) -> EdgeStats {
    let rows = H / 4..3 * H / 4;
    let band = W / 4..3 * W / 4;
    let (mut blended_rows, mut gradient_energy) = (0, 0.0);
    for y in rows.clone() {
        let (card, sky) = (luma(frame, W / 8, y), luma(frame, 7 * W / 8, y));
        let margin = (sky - card).abs() * 0.1;
        let (low, high) = (card.min(sky) + margin, card.max(sky) - margin);
        let between = band
            .clone()
            .any(|x| (low..high).contains(&luma(frame, x, y)));
        blended_rows += usize::from(between);
        for x in band.clone() {
            gradient_energy += (luma(frame, x + 1, y) - luma(frame, x, y)).powi(2);
        }
    }
    EdgeStats {
        blended_rows,
        rows: rows.len(),
        gradient_energy,
    }
}

/// Pixels more than two away from any step over 4 levels in `reference`: where FXAA must not touch.
fn flat_mask(reference: &[u8]) -> Vec<bool> {
    let mut near_edge = vec![false; (W * H) as usize];
    for y in 0..H {
        for x in 0..W - 1 {
            let i = ((y * W + x) * 4) as usize;
            let step = (0..3).any(|c| reference[i + c].abs_diff(reference[i + 4 + c]) > 4);
            let below = y + 1 < H
                && (0..3)
                    .any(|c| reference[i + c].abs_diff(reference[i + (W * 4) as usize + c]) > 4);
            if !(step || below) {
                continue;
            }
            for ny in y.saturating_sub(2)..(y + 4).min(H) {
                for nx in x.saturating_sub(2)..(x + 4).min(W) {
                    near_edge[(ny * W + nx) as usize] = true;
                }
            }
        }
    }
    near_edge.iter().map(|near| !near).collect()
}

/// The largest channel difference between `a` and `b` over the pixels `mask` picks.
fn max_diff_where(a: &[u8], b: &[u8], mask: &[bool]) -> u8 {
    let pixels = a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0);
    pixels
        .zip(mask)
        .filter(|(_, picked)| **picked)
        .map(|((a, b), _)| (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

#[test]
fn fxaa_smooths_the_deferred_staircase_and_leaves_flat_regions_alone() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (scene, camera) = (sloped_card(), head_on());
    let card = (&scene, &camera);
    // The web canvas's format too, where the tonemap encodes sRGB itself and FXAA reads gamma-encoded values.
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        let off = frame(&gpu, card, (RasterPath::Deferred, FxaaMode::Off), format);
        let on = frame(
            &gpu,
            card,
            (RasterPath::Deferred, FxaaMode::Deferred),
            format,
        );
        let msaa = frame(&gpu, card, (RasterPath::Forward, FxaaMode::Off), format);
        let (off_edge, on_edge) = (edge_stats(&off), edge_stats(&on));
        eprintln!(
            "{format:?}: off {off_edge:?}, FXAA {on_edge:?}, forward 4x MSAA {:?}",
            edge_stats(&msaa)
        );
        assert!(
            off_edge.blended_rows * 10 < off_edge.rows,
            "control: one sample per pixel draws a hard staircase: {off_edge:?}"
        );
        // Forward's MSAA blends about 58 % of these rows; FXAA leaves the middle of each step alone.
        assert!(
            on_edge.blended_rows * 3 > on_edge.rows * 2,
            "FXAA left rows unblended: {on_edge:?}"
        );
        assert!(
            on_edge.gradient_energy < off_edge.gradient_energy * 0.7,
            "FXAA did not soften the step: {off_edge:?} -> {on_edge:?}"
        );
        let flat = flat_mask(&off);
        let untouched = flat.iter().filter(|&&f| f).count();
        assert!(
            untouched > (W * H / 2) as usize,
            "the mask is mostly edge: {untouched}"
        );
        let moved = max_diff_where(&off, &on, &flat);
        assert!(moved <= 1, "{format:?}: FXAA moved a flat pixel by {moved}");
    }
}

/// The default, Always as in BeschleunigerBallett, filters a forward frame on top of its MSAA; Deferred only leaves it.
#[test]
fn fxaa_filters_forward_frames_by_default_but_not_deferred_only() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (scene, camera) = (sloped_card(), head_on());
    let card = (&scene, &camera);
    let srgb = wgpu::TextureFormat::Rgba8UnormSrgb;
    let off = frame(&gpu, card, (RasterPath::Forward, FxaaMode::Off), srgb);
    let deferred_only = frame(&gpu, card, (RasterPath::Forward, FxaaMode::Deferred), srgb);
    assert_eq!(off, deferred_only, "Deferred only filtered a forward frame");
    let default = frame(&gpu, card, (RasterPath::Forward, FxaaMode::default()), srgb);
    assert!(off != default, "the default leaves the forward frame alone");
    let moved = max_diff_where(&off, &default, &flat_mask(&off));
    assert!(moved <= 1, "FXAA moved a flat forward pixel by {moved}");
}

/// How far a deferred frame with `fxaa` sits from forward's 4x MSAA one.
#[derive(Debug)]
struct Gap {
    /// The mean channel difference within two pixels of a forward edge.
    edges: f64,
    /// The same elsewhere.
    flat: f64,
    /// The deferred frame's squared luma steps to its right and lower neighbours against forward's: how much harder its edges are.
    energy_ratio: f64,
}

/// Squared luma steps between every pixel and its right and lower neighbours.
fn frame_energy(frame: &[u8]) -> f64 {
    let mut energy = 0.0;
    for y in 0..H - 1 {
        for x in 0..W - 1 {
            let centre = luma(frame, x, y);
            energy +=
                (luma(frame, x + 1, y) - centre).powi(2) + (luma(frame, x, y + 1) - centre).powi(2);
        }
    }
    energy
}

fn deferred_against_forward(gpu: &GpuContext, scene: &CpuScene, fxaa: FxaaMode) -> Gap {
    let srgb = wgpu::TextureFormat::Rgba8UnormSrgb;
    let camera = OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    };
    let view = (scene, &camera);
    let forward = frame(gpu, view, (RasterPath::Forward, FxaaMode::Off), srgb);
    let deferred = frame(gpu, view, (RasterPath::Deferred, fxaa), srgb);
    let flat = flat_mask(&forward);
    let (mut sums, mut counts) = ([0.0f64; 2], [0usize; 2]);
    let pixels = forward
        .as_chunks::<4>()
        .0
        .iter()
        .zip(deferred.as_chunks::<4>().0);
    for ((f, d), is_flat) in pixels.zip(&flat) {
        let diffs = [0, 1, 2].map(|c| f[c].abs_diff(d[c]));
        sums[usize::from(*is_flat)] += diffs.iter().map(|&d| f64::from(d)).sum::<f64>() / 3.0;
        counts[usize::from(*is_flat)] += 1;
    }
    Gap {
        edges: sums[0] / counts[0].max(1) as f64,
        flat: sums[1] / counts[1].max(1) as f64,
        energy_ratio: frame_energy(&deferred) / frame_energy(&forward),
    }
}

/// The guard `BACKLOG.md` named: on a shadowed cube the deferred frame's edges move towards forward's MSAA.
#[test]
fn fxaa_brings_the_deferred_edges_towards_forwards_msaa() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube_on_plane.gltf");
    let scene = load_gltf(path).expect("cube_on_plane.gltf must load");
    let off = deferred_against_forward(&gpu, &scene, FxaaMode::Off);
    let on = deferred_against_forward(&gpu, &scene, FxaaMode::Deferred);
    eprintln!("deferred against forward: without FXAA {off:?}, with {on:?}");
    // Measured on lavapipe: edge energy 1.42 times forward's without FXAA, 0.83 with; edges 2.58 -> 2.28 levels apart.
    assert!(
        (on.energy_ratio - 1.0).abs() < (off.energy_ratio - 1.0).abs() * 0.6,
        "FXAA did not bring the edge energy towards forward's: {off:?} -> {on:?}"
    );
    assert!(
        on.edges < off.edges * 0.95,
        "FXAA did not close the edge gap: {off:?} -> {on:?}"
    );
    assert!(
        (on.flat - off.flat).abs() < 0.05,
        "FXAA moved the frame away from the edges: {off:?} -> {on:?}"
    );
}
