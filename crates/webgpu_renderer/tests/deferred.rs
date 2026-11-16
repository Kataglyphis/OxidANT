//! The deferred raster path against the forward one on the same renderer: structure, not exact
//! pixels, because forward resolves 4x MSAA and deferred shades one sample. Every bound is measured.
//! The parity views turn FXAA off; the rest draw deferred as the editor does, with it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::Vec4;
use kataglyphis_webgpu_renderer::render::forward::{RasterPath, ShaderSources};
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::scene::{AlphaMode, CpuSampler, CpuTextureRef, Vertex};
use kataglyphis_webgpu_renderer::{
    load_gltf, CpuMaterial, CpuPrimitive, CpuScene, CpuTexture, EquirectImage, ForwardRenderer,
    GpuContext, OrbitCamera,
};

const W: u32 = 256;
const H: u32 = 256;
/// A channel step this large between neighbours marks a silhouette or crease in the forward frame.
const EDGE_STEP: u8 = 12;
/// A channel difference this large is plainly visible.
const VISIBLE: u8 = 24;

fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name)
}

fn scene(name: &str) -> CpuScene {
    load_gltf(asset(name)).unwrap_or_else(|e| panic!("{name} must load: {e:#}"))
}

/// The proven shadow test's view: from above, so the plane fills most of the frame.
fn top_down(radius: f32, pitch_deg: f32) -> OrbitCamera {
    OrbitCamera {
        radius,
        pitch_deg,
        ..OrbitCamera::default()
    }
}

/// One frame on `path`, which the renderer keeps, as the editor's switch leaves it.
fn frame(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    path: RasterPath,
    camera: &OrbitCamera,
) -> Vec<u8> {
    renderer.raster_path = path;
    renderer
        .render_to_pixels(gpu, W, H, camera)
        .expect("headless render must succeed")
}

fn pixels(frame: &[u8]) -> &[[u8; 4]] {
    frame.as_chunks::<4>().0
}

fn luma(p: &[u8; 4]) -> f64 {
    0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
}

fn mean_luma(frame: &[u8]) -> f64 {
    pixels(frame).iter().map(luma).sum::<f64>() / pixels(frame).len() as f64
}

fn max_channel_diff(a: &[u8; 4], b: &[u8; 4]) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// The share of pixels the scene changes against its sky alone: what its geometry covers and lights.
fn coverage(frame: &[u8], sky: &[u8]) -> f64 {
    let changed = pixels(frame)
        .iter()
        .zip(pixels(sky))
        .filter(|(a, b)| max_channel_diff(a, b) > 8)
        .count();
    changed as f64 / pixels(frame).len() as f64
}

/// Pixels beside a colour step in `frame`: silhouettes and creases, where MSAA blends what one sample cannot.
fn edges(frame: &[u8]) -> Vec<bool> {
    let px = pixels(frame);
    let at = |x: i64, y: i64| &px[(y * i64::from(W) + x) as usize];
    let mut mask = vec![false; px.len()];
    for y in 0..i64::from(H) {
        for x in 0..i64::from(W) {
            let centre = at(x, y);
            let step = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
                .filter(|&(nx, ny)| nx >= 0 && ny >= 0 && nx < i64::from(W) && ny < i64::from(H))
                .any(|(nx, ny)| max_channel_diff(at(nx, ny), centre) > EDGE_STEP);
            mask[(y * i64::from(W) + x) as usize] = step;
        }
    }
    mask
}

/// How far deferred sits from forward on one view; logged whole, so each bound below is a measurement.
#[derive(Debug)]
struct Parity {
    mean_luma: [f64; 2],
    coverage: [f64; 2],
    mean_abs_diff: f64,
    /// The mean channel difference away from every forward edge.
    interior_mean_abs_diff: f64,
    /// Pixels a channel differs on by more than [`VISIBLE`].
    visible: usize,
    /// The share of those on a forward edge: anti-aliasing's signature, not shading's.
    visible_on_edges: f64,
}

fn parity(forward: &[u8], deferred: &[u8], sky: &[u8]) -> Parity {
    let edge = edges(forward);
    let (mut total, mut interior, mut interior_count, mut visible, mut on_edges) =
        (0.0, 0.0, 0, 0, 0);
    for (i, (f, d)) in pixels(forward).iter().zip(pixels(deferred)).enumerate() {
        let sum: f64 = (0..3).map(|c| f64::from(f[c].abs_diff(d[c]))).sum();
        total += sum;
        if !edge[i] {
            interior += sum;
            interior_count += 1;
        }
        if max_channel_diff(f, d) > VISIBLE {
            visible += 1;
            on_edges += usize::from(edge[i]);
        }
    }
    Parity {
        mean_luma: [mean_luma(forward), mean_luma(deferred)],
        coverage: [coverage(forward, sky), coverage(deferred, sky)],
        mean_abs_diff: total / (3.0 * pixels(forward).len() as f64),
        interior_mean_abs_diff: interior / (3.0 * f64::from(interior_count.max(1))),
        visible,
        visible_on_edges: on_edges as f64 / visible.max(1) as f64,
    }
}

/// `scene` from `camera` on both paths, and its sky alone, with `setup` applied to every renderer.
fn measure(
    gpu: &GpuContext,
    scene: &CpuScene,
    camera: &OrbitCamera,
    setup: impl Fn(&GpuContext, &mut ForwardRenderer),
) -> Parity {
    let renderer_for = |scene: &CpuScene| {
        let mut renderer = ForwardRenderer::new(gpu, W, H);
        renderer.upload_scene(gpu, scene);
        // These compare the shading; FXAA also smooths texture steps MSAA keeps, and tests/fxaa.rs measures it.
        renderer.fxaa = FxaaMode::Off;
        setup(gpu, &mut renderer);
        renderer
    };
    let mut renderer = renderer_for(scene);
    let forward = frame(gpu, &mut renderer, RasterPath::Forward, camera);
    let deferred = frame(gpu, &mut renderer, RasterPath::Deferred, camera);
    let sky_only = CpuScene {
        primitives: Vec::new(),
        ..scene.clone()
    };
    let sky = frame(
        gpu,
        &mut renderer_for(&sky_only),
        RasterPath::Forward,
        camera,
    );
    parity(&forward, &deferred, &sky)
}

/// Measured on lavapipe at 256x256 across the seven views below: the worst gaps were 0.37 levels of
/// mean luminance, 0.0021 of coverage, 0.50 of mean channel difference and 0.30 away from edges.
const MEAN_LUMA_GAP: f64 = 1.0;
const COVERAGE_GAP: f64 = 0.01;
/// The C++ golden's limit, which there compares two single-sample paths.
const MEAN_ABS_DIFF: f64 = 1.0;
/// The albedo's 8-bit G-buffer target alone costs about 0.3 levels on a white plane.
const INTERIOR_MEAN_ABS_DIFF: f64 = 0.75;

fn assert_parity(name: &str, p: &Parity) {
    eprintln!("{name}: {p:?}");
    let luma_gap = (p.mean_luma[0] - p.mean_luma[1]).abs();
    assert!(
        luma_gap <= MEAN_LUMA_GAP,
        "{name}: mean luminance {luma_gap:.3} apart: {p:?}"
    );
    let coverage_gap = (p.coverage[0] - p.coverage[1]).abs();
    assert!(
        coverage_gap <= COVERAGE_GAP,
        "{name}: coverage {coverage_gap:.4} apart: {p:?}"
    );
    assert!(
        p.coverage[0] > 0.02,
        "{name}: the scene covers nothing, the comparison is vacuous"
    );
    assert!(
        p.mean_abs_diff <= MEAN_ABS_DIFF,
        "{name}: per-pixel divergence: {p:?}"
    );
    assert!(
        p.interior_mean_abs_diff <= INTERIOR_MEAN_ABS_DIFF,
        "{name}: away from the edges the paths no longer shade alike: {p:?}"
    );
    assert!(
        p.visible == 0 || p.visible_on_edges > 0.85,
        "{name}: visible differences off the edges are shading, not anti-aliasing: {p:?}"
    );
}

/// The shadow test's light: low, so the cube's shadow falls where the camera looks.
fn low_sun(_: &GpuContext, renderer: &mut ForwardRenderer) {
    renderer.light_dir_ambient = Vec4::new(-1.0, 0.7, -0.3, 0.15);
    // Full strength, as the C++ shadow goldens measure: at the default 0.65 the shadow keeps sun and leaves the mask.
    renderer.shadows.strength = 1.0;
}

#[test]
fn deferred_matches_forward_on_the_shadowed_plane() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let p = measure(
        &gpu,
        &scene("cube_on_plane.gltf"),
        &top_down(6.0, 55.0),
        low_sun,
    );
    assert_parity("cube_on_plane", &p);
}

#[test]
fn deferred_matches_forward_with_a_baked_environment() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let with_sky_ibl = |gpu: &GpuContext, renderer: &mut ForwardRenderer| {
        low_sun(gpu, renderer);
        renderer.set_environment(gpu, &EquirectImage::sky(64, 32));
    };
    let p = measure(
        &gpu,
        &scene("cube_on_plane.gltf"),
        &top_down(6.0, 55.0),
        with_sky_ibl,
    );
    assert_parity("cube_on_plane with IBL", &p);
}

#[test]
fn deferred_matches_forward_under_punctual_lights() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let dim_sun = |_: &GpuContext, renderer: &mut ForwardRenderer| {
        renderer.light_color_intensity.w = 0.4;
    };
    let p = measure(
        &gpu,
        &scene("point_light.gltf"),
        &top_down(7.0, 65.0),
        dim_sun,
    );
    assert_parity("point_light", &p);
}

#[test]
fn deferred_matches_forward_on_mask_and_blend_materials() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let p = measure(
        &gpu,
        &scene("cube_alpha.gltf"),
        &top_down(5.0, 60.0),
        |_, _| {},
    );
    assert_parity("cube_alpha", &p);
}

#[test]
fn deferred_matches_forward_on_a_transformed_texture() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let p = measure(
        &gpu,
        &scene("cube_textured.gltf"),
        &OrbitCamera::default(),
        |_, _| {},
    );
    assert_parity("cube_textured", &p);
}

/// An 8-bit G-buffer would clip the strength-3 emission to 1 and the interior bound would catch it.
#[test]
fn deferred_matches_forward_on_an_emissive_strength_above_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_emissive_strength.gltf");
    let p = measure(&gpu, &scene, &OrbitCamera::default(), |_, _| {});
    assert_parity("cube_emissive_strength", &p);
}

/// A mirrored instance draws with the clockwise-front G-buffer set its first deferred frame builds.
#[test]
fn deferred_matches_forward_on_a_mirrored_instance() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let cube = scene
        .primitives
        .iter()
        .position(|p| p.vertices.len() != 4)
        .expect("the cube beside the 4-vertex plane");
    let with_mirror = move |gpu: &GpuContext, renderer: &mut ForwardRenderer| {
        low_sun(gpu, renderer);
        let mirrored = glam::Mat4::from_translation(glam::Vec3::new(-1.6, 0.0, 0.0))
            * glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0));
        renderer.set_instances(gpu, cube, &[glam::Mat4::IDENTITY, mirrored]);
    };
    let p = measure(&gpu, &scene, &top_down(6.0, 55.0), with_mirror);
    assert_parity("cube_on_plane with a mirrored instance", &p);
}

/// Plane pixels in the cube's shadow: with the analytic IBL they keep only blue sky light.
fn shadow_mask(frame: &[u8]) -> Vec<bool> {
    pixels(frame)
        .iter()
        .map(|p| {
            let (r, b) = (i32::from(p[0]), i32::from(p[2]));
            r < 110 && b > r + 15 && b < 180
        })
        .collect()
}

/// Intersection over union of two masks, `b` mirrored top to bottom when `flip_b`.
fn iou(a: &[bool], b: &[bool], flip_b: bool) -> f64 {
    let (mut both, mut either) = (0usize, 0usize);
    for y in 0..H {
        for x in 0..W {
            let in_a = a[(y * W + x) as usize];
            let row = if flip_b { H - 1 - y } else { y };
            let in_b = b[(row * W + x) as usize];
            both += usize::from(in_a && in_b);
            either += usize::from(in_a || in_b);
        }
    }
    both as f64 / either.max(1) as f64
}

/// Deferred rebuilds positions from the full-screen uv; a mirrored rebuild casts its shadows elsewhere.
#[test]
fn deferred_shadows_land_where_forward_shadows_land() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_on_plane.gltf"));
    low_sun(&gpu, &mut renderer);
    let camera = top_down(6.0, 55.0);
    let forward = shadow_mask(&frame(&gpu, &mut renderer, RasterPath::Forward, &camera));
    let deferred = shadow_mask(&frame(&gpu, &mut renderer, RasterPath::Deferred, &camera));
    let shadowed = forward.iter().filter(|&&s| s).count();
    let (same, flipped) = (
        iou(&deferred, &forward, false),
        iou(&deferred, &forward, true),
    );
    eprintln!(
        "forward shadow {shadowed} px, deferred IoU {same:.3}, against the mirror {flipped:.3}"
    );
    assert!(
        shadowed > 150,
        "forward casts no measurable shadow: the comparison would be vacuous"
    );
    assert!(
        shadowed < (W * H / 2) as usize,
        "forward darkens most of the frame: not a shadow"
    );
    assert!(
        same > 0.8,
        "deferred shadows do not land where forward's do: IoU {same:.3}"
    );
    assert!(
        same > flipped + 0.25,
        "deferred matches forward's mirror as well: {flipped:.3}"
    );
}

/// Counts of the pixels each predicate picks, forward first.
fn counts(
    forward: &[u8],
    deferred: &[u8],
    predicates: &[fn(i32, i32, i32) -> bool],
) -> Vec<[usize; 2]> {
    let count = |frame: &[u8], predicate: fn(i32, i32, i32) -> bool| {
        pixels(frame)
            .iter()
            .filter(|p| predicate(i32::from(p[0]), i32::from(p[1]), i32::from(p[2])))
            .count()
    };
    predicates
        .iter()
        .map(|&predicate| [count(forward, predicate), count(deferred, predicate)])
        .collect()
}

/// The same pixel count on both paths, give or take `share` of it.
fn assert_close(name: &str, [forward, deferred]: [usize; 2], share: f64) {
    eprintln!("{name}: forward {forward}, deferred {deferred}");
    let gap = forward.abs_diff(deferred) as f64;
    assert!(
        gap <= share * forward as f64 + 4.0,
        "{name}: forward {forward}, deferred {deferred}"
    );
}

/// The green BLEND quad composites over the red cube; the yellow MASK quad below its cutoff vanishes.
#[test]
fn deferred_cuts_out_mask_and_composites_blend() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_alpha.gltf"));
    let camera = top_down(5.0, 60.0);
    let forward = frame(&gpu, &mut renderer, RasterPath::Forward, &camera);
    let deferred = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    let [blended, yellow] = counts(
        &forward,
        &deferred,
        &[
            |r, g, b| g > 140 && g > r + 25 && g > b + 25 && r > 70 && b > 60,
            |r, g, b| r > 140 && g > 140 && b * 3 < r,
        ],
    )[..] else {
        unreachable!("two predicates")
    };
    assert!(
        blended[1] > 200,
        "the BLEND quad is missing from the deferred frame: {blended:?}"
    );
    assert_close("blended over the cube", blended, 0.1);
    assert_eq!(
        yellow,
        [0, 0],
        "the MASK quad below its cutoff must be discarded on both paths"
    );
}

/// The red point light and the green spot light pool on the plane as the forward pass lights them.
#[test]
fn punctual_lights_pool_on_the_deferred_plane() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("point_light.gltf"));
    renderer.light_color_intensity.w = 0.4;
    let camera = top_down(7.0, 65.0);
    let forward = frame(&gpu, &mut renderer, RasterPath::Forward, &camera);
    let deferred = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    let [red, green] = counts(
        &forward,
        &deferred,
        &[
            |r, g, b| r > 120 && r > g + 40 && r > b + 40,
            |r, g, b| g > 120 && g > r + 40 && g > b + 40,
        ],
    )[..] else {
        unreachable!("two predicates")
    };
    assert!(
        red[1] > 200 && green[1] > 100,
        "pools missing: red {red:?}, green {green:?}"
    );
    assert_close("red point pool", red, 0.05);
    assert_close("green spot pool", green, 0.05);
}

/// A 2x2 card at the origin, checkered into cut-outs, whose front faces away from the default camera.
fn back_facing_card(double_sided: bool) -> CpuPrimitive {
    let checker = [
        [255u8, 140, 20, 255],
        [255, 140, 20, 0],
        [255, 140, 20, 0],
        [255, 140, 20, 255],
    ];
    let texture = CpuTextureRef {
        texture: Arc::new(CpuTexture {
            width: 2,
            height: 2,
            rgba8: checker.concat(),
            compressed: None,
        }),
        sampler: CpuSampler {
            mag_nearest: true,
            min_nearest: true,
            ..CpuSampler::default()
        },
        srgb: true,
    };
    let vertices = [[-1.0f32, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .iter()
        .map(|[x, y]| Vertex {
            position: [*x, *y, 0.0],
            normal: [0.0, 0.0, -1.0],
            uv: [(x + 1.0) * 0.5, (1.0 - y) * 0.5],
            tangent: [-1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0; 4],
            uv1: [0.0, 0.0],
        })
        .collect();
    CpuPrimitive {
        vertices,
        // Counter-clockwise seen from -z, so the camera on the +z side sees the back face.
        indices: vec![0, 2, 1, 0, 3, 2],
        transform: glam::Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            alpha_mode: AlphaMode::Mask(0.5),
            double_sided,
            base_color_texture: Some(texture),
            metallic_factor: 0.0,
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// Culled out of the G-buffer, a back-facing card would never reach the lighting pass.
#[test]
fn a_double_sided_mask_card_shows_its_back_in_deferred() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let camera = OrbitCamera::default();
    let deferred_frame = |primitives: Vec<CpuPrimitive>| {
        let mut renderer = ForwardRenderer::new(&gpu, W, H);
        renderer.upload_scene(
            &gpu,
            &CpuScene {
                primitives,
                ..CpuScene::default()
            },
        );
        frame(&gpu, &mut renderer, RasterPath::Deferred, &camera)
    };
    let sky = deferred_frame(Vec::new());
    // The changed pixels, and their share of the box they span.
    let shown = |double_sided: bool| {
        let card = deferred_frame(vec![back_facing_card(double_sided)]);
        let (mut count, mut min, mut max) = (0usize, (W, H), (0, 0));
        for (i, (a, b)) in pixels(&card).iter().zip(pixels(&sky)).enumerate() {
            if max_channel_diff(a, b) > 12 {
                let (x, y) = (i as u32 % W, i as u32 / W);
                count += 1;
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
        }
        let area = (max.0.saturating_sub(min.0) + 1) * (max.1.saturating_sub(min.1) + 1);
        (count, count as f64 / f64::from(area))
    };
    let (count, share) = shown(true);
    eprintln!("double-sided card from behind: {count} px, {share:.2} of its box");
    assert!(
        count > 1500,
        "the back-facing double-sided card is not drawn: {count} px"
    );
    assert!(
        share > 0.2 && share < 0.8,
        "the card is not cut out: {share:.2} of its box"
    );
    let (culled, _) = shown(false);
    assert!(
        culled < count / 20,
        "a single-sided card seen from behind drew {culled} px"
    );
}

/// An unlit material is exactly its base colour on the deferred path too, whatever the light does.
#[test]
fn an_unlit_material_ignores_the_light_in_deferred() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let centre = |name: &str| {
        let mut renderer = ForwardRenderer::new(&gpu, W, H);
        renderer.upload_scene(&gpu, &scene(name));
        let mut window = |light: Vec4| {
            renderer.light_dir_ambient = light;
            let frame = frame(
                &gpu,
                &mut renderer,
                RasterPath::Deferred,
                &OrbitCamera::default(),
            );
            let rows = (H / 2 - 12)..(H / 2 + 12);
            rows.flat_map(|y| {
                let start = ((y * W + W / 2 - 12) * 4) as usize;
                frame[start..start + 24 * 4].to_vec()
            })
            .collect::<Vec<u8>>()
        };
        let lit_from_above = window(Vec4::new(1.0, 1.0, 0.5, 0.05));
        (lit_from_above, window(Vec4::new(-1.0, -1.0, -0.5, 0.0)))
    };
    let (a, b) = centre("cube_unlit.gltf");
    assert_eq!(
        a, b,
        "an unlit material must be identical under any lighting"
    );
    let (a, b) = centre("cube.gltf");
    assert_ne!(a, b, "control: a lit material must change with the light");
}

fn max_frame_diff(a: &[u8], b: &[u8]) -> u8 {
    pixels(a)
        .iter()
        .zip(pixels(b))
        .map(|(a, b)| max_channel_diff(a, b))
        .max()
        .unwrap_or(0)
}

/// The editor's switch, both ways, and a resize while deferred: each path renders its frame again.
#[test]
fn switching_paths_and_back_repeats_each_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_on_plane.gltf"));
    low_sun(&gpu, &mut renderer);
    let camera = top_down(6.0, 55.0);
    let forward = frame(&gpu, &mut renderer, RasterPath::Forward, &camera);
    let deferred = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    assert!(
        max_frame_diff(&forward, &deferred) > VISIBLE,
        "the switch changed nothing"
    );
    let forward_again = frame(&gpu, &mut renderer, RasterPath::Forward, &camera);
    assert!(
        max_frame_diff(&forward, &forward_again) <= 1,
        "forward drifted after a deferred frame"
    );
    let small = renderer
        .render_to_pixels(&gpu, 160, 120, &camera)
        .expect("a smaller forward frame");
    assert_eq!(small.len(), 160 * 120 * 4);
    renderer.raster_path = RasterPath::Deferred;
    let small = renderer
        .render_to_pixels(&gpu, 160, 120, &camera)
        .expect("a smaller deferred frame rebuilds the G-buffer");
    assert!(mean_luma(&small) > 50.0, "the resized G-buffer lit nothing");
    let deferred_again = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    assert!(
        max_frame_diff(&deferred, &deferred_again) <= 1,
        "deferred drifted across a resize"
    );
}

/// The G-buffer pass writes the single-sample depth, so SSAO darkens and occlusion queries see it.
#[test]
fn ssao_and_occlusion_queries_read_the_gbuffer_depth() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_alpha.gltf"));
    renderer.bloom_strength = 0.0;
    let camera = top_down(5.0, 60.0);
    let energy = |frame: &[u8]| frame.iter().map(|&b| u64::from(b)).sum::<u64>();
    renderer.ssao_strength = 0.0;
    let without = energy(&frame(&gpu, &mut renderer, RasterPath::Deferred, &camera));
    renderer.ssao_strength = 1.0;
    let with = energy(&frame(&gpu, &mut renderer, RasterPath::Deferred, &camera));
    assert!(
        with + 50_000 < without,
        "SSAO removed no energy: {without} -> {with}"
    );

    renderer.occlusion_queries_enabled = true;
    for _ in 0..16 {
        frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    }
    let visibility = renderer.occlusion_visibility();
    assert_eq!(
        visibility.len(),
        4,
        "one entry per primitive: {visibility:?}"
    );
    assert!(
        visibility.iter().all(|&v| v),
        "nothing here is hidden: {visibility:?}"
    );
    let (drawn, considered) = renderer.occlusion_cull_stats();
    assert!(
        considered >= 2 && drawn == considered,
        "drawn {drawn} of {considered}"
    );
}

/// A cube fully behind a large one is hidden from both cullers by the G-buffer pass's depth.
#[test]
fn a_hidden_cube_is_culled_on_the_deferred_path() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let cube = scene("cube.gltf").primitives[0].clone();
    let placed = |transform: glam::Mat4| CpuPrimitive {
        transform,
        ..cube.clone()
    };
    let occluder = placed(
        glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 2.0))
            * glam::Mat4::from_scale(glam::Vec3::splat(4.0)),
    );
    let hidden = placed(glam::Mat4::from_translation(glam::Vec3::new(
        0.0, 0.0, -3.0,
    )));
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    let scene = CpuScene {
        primitives: vec![occluder, hidden],
        ..CpuScene::default()
    };
    renderer.upload_scene(&gpu, &scene);
    renderer.occlusion_queries_enabled = true;
    let camera = OrbitCamera {
        radius: 8.0,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    for _ in 0..16 {
        frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    }
    let samples = renderer.occlusion_samples();
    eprintln!("occluder and hidden cube samples: {samples:?}");
    assert_eq!(
        renderer.occlusion_visibility(),
        [true, false],
        "samples {samples:?}"
    );
    renderer.gpu_culling_enabled = true;
    for _ in 0..16 {
        frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    }
    assert_eq!(renderer.gpu_culling_visibility(), [true, false]);
    assert_eq!(
        renderer.occlusion_cull_stats(),
        (1, 2),
        "the G-buffer pass skips the hidden cube"
    );
}

/// A reload rebuilds the deferred pipelines in its error scope, so a module missing their entry points is refused.
#[test]
fn a_shader_reload_rebuilds_the_deferred_pipelines_or_keeps_them() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let shipped = ShaderSources::shipped();
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_on_plane.gltf"));
    low_sun(&gpu, &mut renderer);
    let camera = top_down(6.0, 55.0);
    let before = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    renderer
        .reload_shaders(&gpu, &shipped)
        .expect("the shipped shaders must reload");
    let reloaded = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    assert!(
        max_frame_diff(&before, &reloaded) <= 1,
        "the reload changed the frame"
    );
    let renamed =
        (shipped.forward).replace("fn fs_deferred_lighting(", "fn fs_deferred_lighting_gone(");
    assert_ne!(
        renamed, shipped.forward,
        "the lighting entry point is no longer named so"
    );
    let broken = ShaderSources {
        forward: &renamed,
        ..shipped
    };
    assert!(
        renderer.reload_shaders(&gpu, &broken).is_err(),
        "a module without the deferred lighting entry point must be refused"
    );
    let kept = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    assert!(
        max_frame_diff(&before, &kept) <= 1,
        "the refused reload changed the frame"
    );
}

/// Deferred builds lazily, yet a reload before its first frame still refuses a module missing its entry points.
#[test]
fn a_reload_before_the_first_deferred_frame_still_checks_the_deferred_entry_points() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let shipped = ShaderSources::shipped();
    let mut renderer = ForwardRenderer::new(&gpu, W, H);
    renderer.upload_scene(&gpu, &scene("cube_on_plane.gltf"));
    let camera = top_down(6.0, 55.0);
    frame(&gpu, &mut renderer, RasterPath::Forward, &camera);
    let renamed =
        (shipped.forward).replace("fn fs_deferred_lighting(", "fn fs_deferred_lighting_gone(");
    let broken = ShaderSources {
        forward: &renamed,
        ..shipped
    };
    assert!(
        renderer.reload_shaders(&gpu, &broken).is_err(),
        "a forward-only renderer must refuse a module without the deferred lighting entry point"
    );
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let deferred = frame(&gpu, &mut renderer, RasterPath::Deferred, &camera);
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "the first deferred frame failed: {error:?}"
    );
    assert!(mean_luma(&deferred) > 0.0, "the deferred frame is black");
}

/// The frame graph's pool holds only the targets the drawn path uses, and a deferred frame's clouds share a G-buffer
/// texture: forward keeps no G-buffer, deferred no MSAA, and switching back reallocates the same bytes.
#[test]
fn each_path_holds_only_its_own_targets_and_deferred_clouds_alias_the_gbuffer() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (w, h) = (64u64, 48u64);
    let mut renderer = ForwardRenderer::new(&gpu, w as u32, h as u32);
    renderer.upload_scene(&gpu, &scene("cube_on_plane.gltf"));
    renderer.clouds.enabled = true;
    let mut bytes_of = |path| {
        renderer.raster_path = path;
        renderer
            .render_to_pixels(&gpu, w as u32, h as u32, &top_down(6.0, 35.0))
            .expect("renders");
        renderer.transient_texture_bytes()
    };
    let forward = bytes_of(RasterPath::Forward);
    let deferred = bytes_of(RasterPath::Deferred);
    // MSAA HDR 32 and depth 16, HDR 8, depth 4, spread 8 and clouds 8 bytes a pixel.
    assert_eq!(forward, 76 * w * h);
    // HDR 8, depth 4, the G-buffer 20; the clouds' 8 reuse the G-buffer normal's texture.
    assert_eq!(deferred, 32 * w * h);
    assert_eq!(bytes_of(RasterPath::Forward), forward);
}
