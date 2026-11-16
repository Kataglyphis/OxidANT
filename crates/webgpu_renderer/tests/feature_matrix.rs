//! The raster settings, the deferred path, the clouds and the traced modes together on a headless adapter: every
//! feature on both raster paths and in both traced modes, checked structurally (masks, centroids, counters) so the
//! checks survive driver differences.

use std::sync::Arc;
use std::time::Duration;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use kataglyphis_webgpu_renderer::asset::skybox::CubemapFaces;
use kataglyphis_webgpu_renderer::render::clouds::CloudSettings;
use kataglyphis_webgpu_renderer::render::forward::{RasterPath, SkyMode};
use kataglyphis_webgpu_renderer::render::fxaa::FxaaMode;
use kataglyphis_webgpu_renderer::render::gpu_timing::TimedPass;
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::render::model_transform::ModelTransform;
use kataglyphis_webgpu_renderer::render::tracer::RenderMode;
use kataglyphis_webgpu_renderer::{
    load_gltf, CpuScene, ForwardRenderer, GpuContext, OrbitCamera, TonemapPass,
};

const SIZE: u32 = 256;
const PIXELS: usize = (SIZE * SIZE) as usize;
/// Darker than this many levels counts as shadowed, as in the C++ goldens' masks.
const SHADOWED: f32 = 40.0;
/// One colour per cube layer, +X, -X, +Y, -Y, +Z, -Z.
const FACES: [[u8; 4]; 6] = [
    [200, 40, 40, 255],
    [40, 200, 40, 255],
    [40, 40, 200, 255],
    [200, 200, 40, 255],
    [200, 40, 200, 255],
    [40, 200, 200, 255],
];

fn scene(name: &str) -> CpuScene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name);
    load_gltf(path).unwrap_or_else(|e| panic!("{name} must load: {e:#}"))
}

/// The proven shadow tests' view: from above, so the plane fills most of the frame.
fn top_down() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    }
}

/// `scene` on `path` without SSAO and bloom, so a pixel is its own shading alone.
fn plain(gpu: &GpuContext, scene: &CpuScene, path: RasterPath) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(gpu, scene);
    renderer.ssao_strength = 0.0;
    renderer.bloom_strength = 0.0;
    renderer.raster_path = path;
    renderer
}

/// Every frame the renderer draws: both raster paths, then both traced modes, picked over the deferred path, which
/// they must not run.
const MODES: [(RasterPath, RenderMode); 4] = [
    (RasterPath::Forward, RenderMode::Raster),
    (RasterPath::Deferred, RenderMode::Raster),
    (RasterPath::Deferred, RenderMode::RayTraced),
    (RasterPath::Deferred, RenderMode::PathTraced),
];

/// Hands `renderer` its scene's traced copy; a traced frame needs [`build`] after it.
fn trace(renderer: &mut ForwardRenderer, scene: &CpuScene) {
    renderer.set_trace_source(Arc::new(scene.clone()));
    // A short path keeps lavapipe's frames quick; the estimator itself is `traced_modes.rs`'s.
    renderer.trace_settings.samples_per_pixel = 2;
    renderer.trace_settings.max_bounces = 2;
}

/// Blocks until the traced scene is on the GPU, so the next frame in a traced mode traces.
fn build(gpu: &GpuContext, renderer: &mut ForwardRenderer) {
    let built = renderer
        .tracer_mut()
        .finish_build(gpu, Duration::from_secs(120));
    assert!(
        built,
        "the tracer did not build: {:?}",
        renderer.trace_status()
    );
}

/// [`plain`] drawing `mode`, its traced scene built when the mode traces.
fn plain_in(
    gpu: &GpuContext,
    scene: &CpuScene,
    (path, mode): (RasterPath, RenderMode),
) -> ForwardRenderer {
    let mut renderer = plain(gpu, scene, path);
    renderer.render_mode = mode;
    if mode.is_traced() {
        trace(&mut renderer, scene);
        build(gpu, &mut renderer);
    }
    renderer
}

/// One frame of `renderer`'s mode; a path-traced one starts its mean over, so equal inputs trace equal samples.
fn render_once(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    renderer.reset_accumulation();
    let frame = render(gpu, renderer, camera);
    assert_eq!(
        renderer.drawn_mode(),
        renderer.render_mode,
        "the frame drew another mode"
    );
    frame
}

/// As [`plain`], under the shadow tests' low sun at full strength, as the C++ shadow goldens measure.
fn shadowed(gpu: &GpuContext, scene: &CpuScene, path: RasterPath) -> ForwardRenderer {
    let mut renderer = plain(gpu, scene, path);
    renderer.light_dir_ambient = Vec4::new(-1.0, 0.7, -0.3, 0.15);
    renderer.shadows.strength = 1.0;
    renderer
}

fn render(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

fn luma(p: &[u8]) -> f32 {
    0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])
}

/// Per pixel, how many levels the shadows darken the frame against the same frame with shadows off.
fn darkening(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<f32> {
    let shadowed = render(gpu, renderer, camera);
    let enabled = renderer.shadows.enabled;
    renderer.shadows.enabled = false;
    let lit = render(gpu, renderer, camera);
    renderer.shadows.enabled = enabled;
    lit.as_chunks::<4>()
        .0
        .iter()
        .zip(shadowed.as_chunks::<4>().0)
        .map(|(l, s)| luma(l) - luma(s))
        .collect()
}

/// Partly shadowed pixels: darker than a tenth of the umbra, lighter than nine tenths of it.
fn penumbra(darkening: &[f32]) -> usize {
    let umbra = darkening.iter().copied().fold(0.0, f32::max);
    darkening
        .iter()
        .filter(|d| **d > 0.1 * umbra && **d < 0.9 * umbra)
        .count()
}

fn iou(a: &[bool], b: &[bool]) -> f64 {
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    both as f64 / either.max(1) as f64
}

/// Pixels where any colour channel of `a` and `b` differs by more than `tolerance`.
fn differing(a: &[u8], b: &[u8], tolerance: u8) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > tolerance))
        .count()
}

fn channel_delta(a: &[u8], b: &[u8], pixel: usize) -> u8 {
    (0..3)
        .map(|c| a[pixel * 4 + c].abs_diff(b[pixel * 4 + c]))
        .max()
        .unwrap_or(0)
}

#[test]
fn deferred_shadows_match_forward_at_pcf_radius_5_strength_1_and_one_cascade() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let masks = RasterPath::ALL.map(|path| {
        let mut renderer = shadowed(&gpu, &scene, path);
        renderer.shadows.pcf_radius = 5;
        renderer.shadows.cascade_count = 1;
        let mask: Vec<bool> = darkening(&gpu, &mut renderer, &top_down())
            .iter()
            .map(|d| *d > SHADOWED)
            .collect();
        assert_eq!(renderer.shadow_map_layers(), 1, "{path:?}");
        mask
    });
    let count = masks[0].iter().filter(|m| **m).count();
    let overlap = iou(&masks[0], &masks[1]);
    eprintln!("forward shadow {count} px, deferred IoU {overlap:.4}");
    assert!(
        count > 500,
        "forward casts too little shadow to compare: {count}"
    );
    assert!(
        overlap > 0.95,
        "the deferred shadow is not forward's under the same settings: IoU {overlap}"
    );
}

#[test]
fn the_pcf_radius_widens_the_deferred_penumbra_as_it_widens_forwards() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    for path in RasterPath::ALL {
        let mut renderer = shadowed(&gpu, &scene, path);
        let mut frames = Vec::new();
        let mut counts = Vec::new();
        for radius in [0, 5] {
            renderer.shadows.pcf_radius = radius;
            counts.push(penumbra(&darkening(&gpu, &mut renderer, &top_down())));
            frames.push(render(&gpu, &mut renderer, &top_down()));
        }
        let changed = differing(&frames[0], &frames[1], 2);
        eprintln!("{path:?}: penumbra at radius 0 and 5 {counts:?}, {changed} px changed");
        assert!(
            changed > 100,
            "{path:?}: the radius changed {changed} pixels"
        );
        assert!(
            counts[1] > counts[0] * 2,
            "{path:?}: the penumbra did not widen: {counts:?}"
        );
    }
}

/// The lighting pass binds the shadow map too: after a reallocation it must sample the new one, as a fresh renderer does.
#[test]
fn deferred_shadows_follow_a_new_resolution_and_cascade_count() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let mut switched = shadowed(&gpu, &scene, RasterPath::Deferred);
    // The first deferred frame builds the lighting group over the default 2048-texel, 3-cascade map.
    render(&gpu, &mut switched, &top_down());
    for (resolution, cascades) in [(512, 1), (4096, 2), (1024, 3)] {
        switched.shadows.resolution = resolution;
        switched.shadows.cascade_count = cascades;
        let frame = render(&gpu, &mut switched, &top_down());
        assert_eq!(switched.shadow_map_resolution(), resolution);
        assert_eq!(switched.shadow_map_layers(), cascades as usize);
        let mut fresh = shadowed(&gpu, &scene, RasterPath::Deferred);
        fresh.shadows = switched.shadows;
        let reference = render(&gpu, &mut fresh, &top_down());
        let off = differing(&frame, &reference, 2);
        assert!(
            off < 10,
            "{resolution} texels, {cascades} cascades: {off} pixels differ from a fresh deferred renderer"
        );
    }
}

/// An orbit camera at the origin looking along `yaw` and `pitch` as `OrbitCamera` measures them.
fn looking(yaw_deg: f32, pitch_deg: f32) -> OrbitCamera {
    let probe = OrbitCamera {
        radius: 1.0,
        yaw_deg,
        pitch_deg,
        ..OrbitCamera::default()
    };
    OrbitCamera {
        target: -probe.eye(),
        ..probe
    }
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = (y * SIZE + x) as usize * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    (0..3).all(|c| a[c].abs_diff(b[c]) <= tolerance)
}

#[test]
fn the_cube_skybox_shows_each_face_along_its_axis_in_deferred_as_in_forward() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // 4x4 faces: a view a degree off a face's centre then still filters that face alone.
    let faces = CubemapFaces {
        size: 4,
        rgba8: FACES.iter().flat_map(|colour| colour.repeat(16)).collect(),
    };
    // +X, -X, +Y, -Y, +Z, -Z: the eye sits opposite the view direction.
    let views = [
        (180.0, 0.0),
        (0.0, 0.0),
        (0.0, -89.0),
        (0.0, 89.0),
        (270.0, 0.0),
        (90.0, 0.0),
    ];
    let shown = RasterPath::ALL.map(|path| {
        let mut renderer = plain(&gpu, &CpuScene::default(), path);
        renderer.set_skybox(&gpu, &faces);
        assert_eq!(renderer.sky_mode(), SkyMode::Cubemap);
        views.map(|(yaw, pitch)| {
            let frame = render(&gpu, &mut renderer, &looking(yaw, pitch));
            pixel(&frame, SIZE / 2, SIZE / 2)
        })
    });
    for layer in 0..6 {
        let (forward, deferred) = (shown[0][layer], shown[1][layer]);
        assert!(
            near(deferred, FACES[layer], 2),
            "deferred shows {deferred:?} along cube layer {layer}, not {:?}",
            FACES[layer]
        );
        assert!(
            near(deferred, forward, 1),
            "{deferred:?} against forward's {forward:?}"
        );
    }
}

#[test]
fn the_skybox_switch_shows_the_cpp_navy_in_deferred_and_leaves_the_geometry_alone() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    // Level with the plane, so the top rows are sky and the centre is the cube.
    let side = OrbitCamera {
        radius: 7.0,
        pitch_deg: 5.0,
        target: Vec3::new(0.0, 0.9, 0.0),
        ..OrbitCamera::default()
    };
    let tops = RasterPath::ALL.map(|path| {
        let mut renderer = plain(&gpu, &scene, path);
        renderer.set_skybox(&gpu, &CubemapFaces::solid(FACES));
        let shown = render(&gpu, &mut renderer, &side);
        renderer.skybox_enabled = false;
        assert_eq!(renderer.sky_mode(), SkyMode::Off);
        let off = render(&gpu, &mut renderer, &side);
        let (top, top_shown) = (pixel(&off, 0, 0), pixel(&shown, 0, 0));
        assert!(
            near(top, [0, 0, 188, 255], 2),
            "{path:?}: off shows {top:?}"
        );
        assert!(
            !near(top_shown, top, 20),
            "{path:?}: the cube map showed {top_shown:?}"
        );
        let centre = |frame: &[u8]| pixel(frame, SIZE / 2, SIZE / 2);
        assert_eq!(
            centre(&shown),
            centre(&off),
            "{path:?}: the switch moved the cube"
        );
        top
    });
    assert_eq!(tops[0], tops[1], "the navy differs between the paths");
}

/// From (0, 0, distance) toward -Z: world +X is screen right and +Y screen up.
fn facing_minus_z(distance: f32) -> OrbitCamera {
    OrbitCamera {
        radius: distance,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    }
}

/// The pixel rect of a world-space box, all of whose corners lie in front of the camera.
struct PixelRect {
    min: Vec2,
    max: Vec2,
}

impl PixelRect {
    fn of_box(camera: &OrbitCamera, centre: Vec3, half_extents: Vec3) -> Self {
        let view_proj = camera.view_projection(1.0);
        let (mut min, mut max) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for corner in 0..8 {
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let ndc = view_proj.project_point3(centre + sign * half_extents);
            let at = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * SIZE as f32;
            (min, max) = (min.min(at), max.max(at));
        }
        Self { min, max }
    }

    /// Whether pixel `index`'s centre lies inside the rect grown by `margin` pixels (shrunk if negative).
    fn contains(&self, index: usize, margin: f32) -> bool {
        let at = Vec2::new(
            (index % SIZE as usize) as f32 + 0.5,
            (index / SIZE as usize) as f32 + 0.5,
        );
        at.cmpge(self.min - margin).all() && at.cmple(self.max + margin).all()
    }
}

/// What turning the clouds on changes on one path: wall pixels at all, sky pixels by more than 5 levels.
struct CloudChange {
    wall: usize,
    sky: usize,
    sky_pixels: Vec<usize>,
    /// The march's half-float output, RGBA per pixel.
    march: Vec<u16>,
}

/// The cloud scene's wall and box, and the camera facing them.
const WALL: (Vec3, Vec3) = (Vec3::ZERO, Vec3::new(3.0, 2.0, 0.1));
const CLOUD_BOX: (Vec3, Vec3) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(8.0, 5.0, 3.0));

/// A wall in front of a cloud box in `mode`, clouds still off; a traced mode builds the wall as an explicit instance.
fn cloud_scene(gpu: &GpuContext, (path, mode): (RasterPath, RenderMode)) -> ForwardRenderer {
    let scene = scene("cube.gltf");
    let mut renderer = plain(gpu, &scene, path);
    renderer.render_mode = mode;
    trace(&mut renderer, &scene);
    let wall = Mat4::from_scale_rotation_translation(WALL.1 * 2.0, Quat::IDENTITY, WALL.0);
    renderer.set_instances(gpu, 0, &[wall]);
    if mode.is_traced() {
        build(gpu, &mut renderer);
    }
    // Navy and no powder: over the horizon a cloud lit by that sky's own ambient barely differs from it.
    renderer.skybox_enabled = false;
    renderer.clouds = CloudSettings {
        coverage: 0.3,
        powder: false,
        ..CloudSettings::default()
    };
    renderer.clouds.place_box(CLOUD_BOX.0, CLOUD_BOX.1, 1.0);
    renderer
}

fn cloud_change(gpu: &GpuContext, mode: (RasterPath, RenderMode)) -> CloudChange {
    let camera = facing_minus_z(20.0);
    let mut renderer = cloud_scene(gpu, mode);
    let off = render_once(gpu, &mut renderer, &camera);
    renderer.clouds.enabled = true;
    let on = render_once(gpu, &mut renderer, &camera);
    let wall_rect = PixelRect::of_box(&camera, WALL.0, WALL.1);
    let box_rect = PixelRect::of_box(&camera, CLOUD_BOX.0, CLOUD_BOX.1);
    let sky_pixels: Vec<usize> = (0..PIXELS)
        .filter(|&p| box_rect.contains(p, -2.0) && !wall_rect.contains(p, 2.0))
        .collect();
    let on_wall = (0..PIXELS).filter(|&p| wall_rect.contains(p, -2.0));
    CloudChange {
        wall: on_wall.filter(|&p| channel_delta(&off, &on, p) > 0).count(),
        sky: sky_pixels
            .iter()
            .filter(|&&p| channel_delta(&off, &on, p) > 5)
            .count(),
        sky_pixels,
        march: renderer
            .read_cloud_target(gpu)
            .expect("cloud readback must succeed")
            .expect("clouds ran"),
    }
}

/// The march reads the single-sample depth every frame leaves: either raster path's, or the traced primary hit's,
/// as the C++ engine binds the traced depth for its clouds; so a wall hides the clouds in every mode alike.
#[test]
fn clouds_change_the_sky_and_stay_behind_a_wall_on_both_paths_and_in_both_traced_modes() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let changes = MODES.map(|mode| cloud_change(&gpu, mode));
    for (mode, change) in MODES.iter().zip(&changes) {
        eprintln!(
            "{mode:?}: {} wall px changed, {} of {} sky px",
            change.wall,
            change.sky,
            change.sky_pixels.len()
        );
        assert_eq!(
            change.wall, 0,
            "{mode:?}: clouds behind the wall drew over it"
        );
        // A path-traced miss is the procedural horizon, which ambient-lit cloud barely changes; its march is checked below.
        assert!(
            mode.1 == RenderMode::PathTraced || change.sky * 4 > change.sky_pixels.len(),
            "{mode:?}: the clouds barely changed the sky"
        );
    }
    let alpha = |march: &[u16], p: usize| half_to_f32(march[p * 4 + 3]);
    let forward = &changes[0];
    for (mode, change) in MODES.iter().zip(&changes).skip(1) {
        let apart = forward
            .sky_pixels
            .iter()
            .filter(|&&p| (alpha(&forward.march, p) - alpha(&change.march, p)).abs() > 1e-3)
            .count();
        assert_eq!(
            apart, 0,
            "{mode:?}: the march over the open sky differs from forward's in {apart} px"
        );
    }
}

/// The centroid of the pixels `pick` keeps.
fn centroid(pixels: &[u8], pick: impl Fn(&[u8]) -> bool) -> Vec2 {
    let (mut sum, mut n) = (Vec2::ZERO, 0.0);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if pick(p) {
            sum += Vec2::new((i as u32 % SIZE) as f32, (i as u32 / SIZE) as f32);
            n += 1.0;
        }
    }
    assert!(n > 50.0, "too few pixels to place a centroid: {n}");
    sum / n
}

#[test]
fn the_model_transform_moves_the_model_alike_on_both_paths() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let red = |p: &[u8]| i32::from(p[0]) - i32::from(p[1]) > 60;
    let transform = ModelTransform {
        position: Vec3::new(0.0, 0.0, 1.0),
        rotation_deg: Vec3::new(0.0, 90.0, 0.0),
    };
    let moves = RasterPath::ALL.map(|path| {
        let mut renderer = shadowed(&gpu, &scene, path);
        let before = centroid(&render(&gpu, &mut renderer, &top_down()), red);
        renderer.model_transform = transform;
        let after = centroid(&render(&gpu, &mut renderer, &top_down()), red);
        (before, after, renderer.scene_bounds().expect("bounds"))
    });
    let [(before_f, after_f, bounds_f), (before_d, after_d, bounds_d)] = moves;
    eprintln!("forward cube {before_f} -> {after_f}, deferred {before_d} -> {after_d}");
    let (shift_f, shift_d) = (after_f - before_f, after_d - before_d);
    assert!(shift_f.length() > 4.0, "the cube did not move");
    // One sample against four shifts the red mask's edge a little; the move itself must match.
    assert!(
        shift_d.distance(shift_f) < 0.5,
        "deferred moved the cube by {shift_d}, forward by {shift_f}"
    );
    assert!(
        after_d.distance(after_f) < 1.0,
        "{after_d} against {after_f}"
    );
    assert_eq!(
        bounds_f, bounds_d,
        "the paths placed the scene's bounds apart"
    );
}

/// The camera pass's primitives, the cascades' casters and the occlusion counts, per path.
type Stats = ((u32, u32), (u32, u32), (u32, u32));

fn stats(
    gpu: &GpuContext,
    scene: &CpuScene,
    path: RasterPath,
    culling: bool,
    camera: &OrbitCamera,
) -> Stats {
    let mut renderer = shadowed(gpu, scene, path);
    renderer.frustum_culling = culling;
    render(gpu, &mut renderer, camera);
    (
        renderer.primitive_stats(),
        renderer.shadow_caster_stats(),
        renderer.occlusion_cull_stats(),
    )
}

/// The G-buffer and lighting passes count what they draw as the forward pass does, blended primitives included.
#[test]
fn culling_counts_the_same_on_both_paths_with_frustum_culling_on_and_off() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // Far past the scene, looking away from it.
    let away = OrbitCamera {
        target: Vec3::new(0.0, 0.0, 5000.0),
        yaw_deg: 270.0,
        ..top_down()
    };
    for name in ["cube_on_plane.gltf", "cube_alpha.gltf"] {
        let scene = scene(name);
        for culling in [true, false] {
            for (view, camera) in [("at", top_down()), ("away", away.clone())] {
                let [forward, deferred] =
                    RasterPath::ALL.map(|path| stats(&gpu, &scene, path, culling, &camera));
                eprintln!("{name} culling {culling} looking {view}: {forward:?}");
                assert_eq!(
                    forward, deferred,
                    "{name}, culling {culling}, looking {view}"
                );
            }
        }
    }
    let scene = scene("cube_on_plane.gltf");
    let primitives = |culling, camera: &OrbitCamera| {
        stats(&gpu, &scene, RasterPath::Deferred, culling, camera).0
    };
    assert_eq!(
        primitives(true, &top_down()),
        (2, 2),
        "the camera looks at both"
    );
    assert_eq!(
        primitives(true, &away),
        (0, 2),
        "both lie behind the camera"
    );
    assert_eq!(primitives(false, &away), (2, 2), "off draws everything");
}

/// The passes `mode` records with every optional pass on: a raster path all but the other path's and the traced
/// passes; a traced mode its kernel, the resolve and the post chain without SSAO. FXAA as `fxaa` decides.
fn passes_in((path, mode): (RasterPath, RenderMode), fxaa: FxaaMode) -> Vec<&'static str> {
    let deferred_raster = mode == RenderMode::Raster && path == RasterPath::Deferred;
    let runs = |pass: &TimedPass| match (mode, pass) {
        (_, TimedPass::Fxaa) => fxaa.filters(deferred_raster),
        (RenderMode::Raster, TimedPass::GBuffer | TimedPass::DeferredLighting) => {
            path == RasterPath::Deferred
        }
        (RenderMode::Raster, TimedPass::Forward) => path == RasterPath::Forward,
        (RenderMode::Raster, pass) => !pass.is_traced(),
        (_, TimedPass::RayTrace) => mode == RenderMode::RayTraced,
        (_, TimedPass::PathTrace) => mode == RenderMode::PathTraced,
        (_, pass) => matches!(
            pass,
            TimedPass::TraceResolve
                | TimedPass::Histogram
                | TimedPass::ExposureReduce
                | TimedPass::Bloom
                | TimedPass::Clouds
                | TimedPass::Tonemap
        ),
    };
    TimedPass::ALL
        .into_iter()
        .filter(runs)
        .map(TimedPass::name)
        .collect()
}

/// With every feature on, each path and mode reports exactly the passes it runs; the list keeps `ALL` order.
#[test]
fn each_path_and_mode_reports_exactly_the_passes_it_runs_with_every_feature_on() {
    let mut names: Vec<&str> = TimedPass::ALL.iter().map(|pass| pass.name()).collect();
    let mut labels: Vec<&str> = TimedPass::ALL.iter().map(|pass| pass.label()).collect();
    for list in [&mut names, &mut labels] {
        list.sort_unstable();
        list.dedup();
        assert_eq!(list.len(), TimedPass::ALL.len(), "{list:?}");
    }
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !gpu.supports_timestamps {
        assert!(
            !GpuContext::gpu_required(),
            "the CI adapters offer timestamps"
        );
        return;
    }
    let scene = scene("cube_on_plane.gltf");
    for mode in MODES {
        // Bloom and SSAO at their defaults: at zero strength their passes do not record.
        let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
        renderer.upload_scene(&gpu, &scene);
        renderer.raster_path = mode.0;
        renderer.render_mode = mode.1;
        trace(&mut renderer, &scene);
        if mode.1.is_traced() {
            build(&gpu, &mut renderer);
        }
        renderer.shadows.pcf_radius = 5;
        assert!(renderer.enable_gpu_timing(&gpu));
        renderer.occlusion_queries_enabled = true;
        renderer.auto_exposure = true;
        renderer.clouds.enabled = true;
        renderer.fxaa = FxaaMode::Always;
        renderer.shadows.cascade_count = 2;
        renderer.set_skybox(&gpu, &CubemapFaces::solid(FACES));
        renderer.model_transform.position = Vec3::new(0.5, 0.0, 0.0);
        let reported = timed(&gpu, &mut renderer);
        assert_eq!(reported, passes_in(mode, FxaaMode::Always), "{mode:?}");
    }
}

/// The pass names `renderer` reports after enough frames for the timestamp readback.
fn timed(gpu: &GpuContext, renderer: &mut ForwardRenderer) -> Vec<&'static str> {
    timed_after(gpu, renderer, 16)
}

/// The pass names `renderer` reports after `frames` frames, each waited for so its readback lands.
fn timed_after(gpu: &GpuContext, renderer: &mut ForwardRenderer, frames: u32) -> Vec<&'static str> {
    let mut tonemap = TonemapPass::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("feature_matrix_target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    for _ in 0..frames {
        renderer.render_tonemapped(gpu, &mut tonemap, &view, SIZE, SIZE, &top_down());
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }
    renderer
        .gpu_timings_ms()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Raster after a trip through both traced modes draws what it drew before on either path, and the traced frames
/// count no primitives or casters, as the C++ engine's traced frames zero its visibility counts.
#[test]
fn raster_frames_are_bit_identical_after_a_trip_through_both_traced_modes() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    renderer.upload_scene(&gpu, &scene);
    trace(&mut renderer, &scene);
    build(&gpu, &mut renderer);
    renderer.clouds.enabled = true;
    renderer.occlusion_queries_enabled = true;
    let camera = top_down();
    let raster = |renderer: &mut ForwardRenderer, path| {
        renderer.render_mode = RenderMode::Raster;
        renderer.raster_path = path;
        let frame = render(&gpu, renderer, &camera);
        let counts = (renderer.primitive_stats(), renderer.shadow_caster_stats());
        (frame, counts)
    };
    let before = RasterPath::ALL.map(|path| raster(&mut renderer, path));
    for mode in [RenderMode::RayTraced, RenderMode::PathTraced] {
        renderer.render_mode = mode;
        let traced = render(&gpu, &mut renderer, &camera);
        assert_eq!(renderer.drawn_mode(), mode);
        assert_ne!(traced, before[0].0, "{mode:?} drew the raster frame");
        assert_eq!(renderer.primitive_stats(), (0, 0), "{mode:?}");
        assert_eq!(renderer.shadow_caster_stats(), (0, 0), "{mode:?}");
        assert_eq!(renderer.occlusion_cull_stats(), (0, 0), "{mode:?}");
    }
    for path in [RasterPath::Deferred, RasterPath::Forward] {
        let (frame, counts) = raster(&mut renderer, path);
        let index = RasterPath::ALL.iter().position(|p| *p == path);
        let (expected, expected_counts) = &before[index.expect("a listed path")];
        let off = differing(&frame, expected, 0);
        assert_eq!(
            off, 0,
            "{path:?}: {off} pixels changed after the traced modes"
        );
        assert_eq!(counts, *expected_counts, "{path:?}");
        assert!(counts.0 .0 > 0, "{path:?} drew nothing");
    }
}

/// Where the traced depth and `reference` both see geometry, how far apart they are at most, and how many pixels
/// one calls sky and the other not.
fn depth_gap(traced: &[f32], reference: &[f32]) -> (f32, usize) {
    let mut worst = 0.0f32;
    let mut coverage = 0;
    for (t, r) in traced.iter().zip(reference) {
        match (*t < 1.0, *r < 1.0) {
            (true, true) => worst = worst.max((t - r).abs()),
            (false, false) => {}
            _ => coverage += 1,
        }
    }
    (worst, coverage)
}

/// The traced kernels write the primary hit's NDC depth as raytrace.rgen.slang and path_tracing.slang do: the depth
/// raster writes for the same surface, and 1 where nothing is hit.
#[test]
fn the_traced_depth_is_the_raster_depth_on_a_simple_scene() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let side = OrbitCamera {
        radius: 7.0,
        pitch_deg: 20.0,
        target: Vec3::new(0.0, 0.5, 0.0),
        ..OrbitCamera::default()
    };
    let mut renderer = plain_in(&gpu, &scene, MODES[0]);
    trace(&mut renderer, &scene);
    build(&gpu, &mut renderer);
    let depths = MODES.map(|(path, mode)| {
        renderer.raster_path = path;
        renderer.render_mode = mode;
        render_once(&gpu, &mut renderer, &side);
        renderer.read_depth_target(&gpu).expect("depth readback")
    });
    let sky = depths[0].iter().filter(|d| **d == 1.0).count();
    assert!(
        sky > PIXELS / 10 && sky < PIXELS * 9 / 10,
        "the view must hold sky and geometry: {sky} sky px"
    );
    // Deferred writes one sample at the pixel centre, as both kernels trace; forward resolves the nearest of four.
    for (i, label) in [(2, "ray traced"), (3, "path traced")] {
        let (worst, coverage) = depth_gap(&depths[i], &depths[1]);
        let (worst_f, coverage_f) = depth_gap(&depths[i], &depths[0]);
        eprintln!(
            "{label}: deferred gap {worst:e}, {coverage} px coverage; forward gap {worst_f:e}, {coverage_f} px"
        );
        assert!(worst < 1e-4, "{label}: {worst} from the deferred depth");
        assert!(coverage < 40, "{label}: {coverage} px disagree on coverage");
        assert!(coverage_f < 400, "{label}: {coverage_f} px against forward");
    }
}

/// The cube far off the axes, so a view from the origin along any axis sees only sky.
fn sky_only(gpu: &GpuContext, mode: (RasterPath, RenderMode)) -> ForwardRenderer {
    let scene = scene("cube.gltf");
    let mut renderer = plain(gpu, &scene, mode.0);
    renderer.render_mode = mode.1;
    trace(&mut renderer, &scene);
    let away = Mat4::from_scale_rotation_translation(
        Vec3::splat(0.01),
        Quat::IDENTITY,
        Vec3::splat(500.0),
    );
    renderer.set_instances(gpu, 0, &[away]);
    if mode.1.is_traced() {
        build(gpu, &mut renderer);
    }
    renderer
}

/// The C++ ray-traced mode draws its sky pass where a primary ray misses: the cube map, or the navy when switched off,
/// as raster does. The path tracer keeps its own sky term, the procedural gradient, as the C++ one keeps its own.
#[test]
fn ray_traced_misses_show_the_sky_pass_and_path_traced_ones_the_tracers_sky() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let faces = CubemapFaces {
        size: 4,
        rgba8: FACES.iter().flat_map(|colour| colour.repeat(16)).collect(),
    };
    let views = [
        (180.0, 0.0),
        (0.0, 0.0),
        (0.0, -89.0),
        (0.0, 89.0),
        (270.0, 0.0),
        (90.0, 0.0),
    ];
    // Per mode: each view's centre with the cube map, then with the skybox off, then the procedural sky.
    let shown = MODES.map(|mode| {
        let mut renderer = sky_only(&gpu, mode);
        renderer.set_skybox(&gpu, &faces);
        let centre = |renderer: &mut ForwardRenderer, (yaw, pitch)| {
            pixel(
                &render_once(&gpu, renderer, &looking(yaw, pitch)),
                SIZE / 2,
                SIZE / 2,
            )
        };
        let cube = views.map(|view| centre(&mut renderer, view));
        renderer.skybox_enabled = false;
        let navy = centre(&mut renderer, views[1]);
        renderer.skybox_enabled = true;
        renderer.clear_skybox(&gpu);
        let procedural = views.map(|view| centre(&mut renderer, view));
        (cube, navy, procedural)
    });
    let (forward, ray, path) = (&shown[0], &shown[2], &shown[3]);
    for (layer, face) in FACES.into_iter().enumerate() {
        assert!(
            near(ray.0[layer], face, 2),
            "ray traced shows {:?} along cube layer {layer}, not {:?}",
            ray.0[layer],
            face
        );
        assert!(near(ray.0[layer], forward.0[layer], 1));
        let (procedural, raster) = (ray.2[layer], forward.2[layer]);
        assert!(
            near(procedural, raster, 1),
            "{procedural:?} against {raster:?}"
        );
        assert!(
            near(path.0[layer], path.2[layer], 2) && !near(path.0[layer], face, 20),
            "the path tracer's sky along layer {layer} is {:?} with the cube map, {:?} without",
            path.0[layer],
            path.2[layer]
        );
        let (jittered, raster) = (path.2[layer], forward.2[layer]);
        assert!(near(jittered, raster, 3), "{jittered:?} against {raster:?}");
    }
    assert!(
        near(ray.1, [0, 0, 188, 255], 2),
        "ray traced off: {:?}",
        ray.1
    );
    assert_eq!(ray.1, forward.1, "the navy differs from raster's");
}

/// The red cube's centroid in one frame of `mode`.
fn red_centroid(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    mode: RenderMode,
    camera: &OrbitCamera,
) -> Vec2 {
    renderer.render_mode = mode;
    let red = |p: &[u8]| i32::from(p[0]) - i32::from(p[1]) > 60;
    centroid(&render_once(gpu, renderer, camera), red)
}

/// The model transform is one root matrix over the traced instances too: set before the build it holds after it, a
/// change moves the traced cube as it moves the raster one, and the path tracer's mean starts over.
#[test]
fn the_model_transform_moves_the_traced_scene_with_raster_and_restarts_the_mean() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube_on_plane.gltf");
    let camera = top_down();
    let mut renderer = plain(&gpu, &scene, RasterPath::Forward);
    trace(&mut renderer, &scene);
    let raster_home = red_centroid(&gpu, &mut renderer, RenderMode::Raster, &camera);
    let moved = ModelTransform {
        position: Vec3::new(1.5, 0.0, 0.5),
        rotation_deg: Vec3::new(0.0, 90.0, 0.0),
    };
    renderer.model_transform = moved;
    let raster_moved = red_centroid(&gpu, &mut renderer, RenderMode::Raster, &camera);
    build(&gpu, &mut renderer);
    let traced_moved = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    renderer.model_transform = ModelTransform::default();
    let traced_home = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    let (raster_shift, traced_shift) = (raster_moved - raster_home, traced_moved - traced_home);
    eprintln!("raster {raster_home} -> {raster_moved}, ray traced {traced_home} -> {traced_moved}");
    assert!(
        raster_shift.length() > 10.0,
        "the cube barely moved: {raster_shift}"
    );
    assert!(
        traced_shift.distance(raster_shift) < 1.0,
        "the traced cube moved by {traced_shift}, raster by {raster_shift}"
    );
    assert!(
        traced_moved.distance(raster_moved) < 3.0,
        "{traced_moved} against {raster_moved}"
    );
    renderer.render_mode = RenderMode::PathTraced;
    renderer.reset_accumulation();
    for _ in 0..2 {
        render(&gpu, &mut renderer, &camera);
    }
    assert_eq!(renderer.accumulated_samples(), 4);
    renderer.model_transform = moved;
    render(&gpu, &mut renderer, &camera);
    assert_eq!(
        renderer.accumulated_samples(),
        2,
        "the move must start the mean over"
    );
}

/// `set_instances` before the build is traced as explicit instances; after it, a move or a new count re-instances it.
#[test]
fn set_instances_reaches_the_traced_scene_moved_or_recounted() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube.gltf");
    let camera = facing_minus_z(12.0);
    let at = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
    let mut renderer = plain(&gpu, &scene, RasterPath::Forward);
    trace(&mut renderer, &scene);
    renderer.set_instances(&gpu, 0, &[at(-2.0)]);
    build(&gpu, &mut renderer);
    let left = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    let raster_left = red_centroid(&gpu, &mut renderer, RenderMode::Raster, &camera);
    let generation = renderer.tracer().generation();
    renderer.set_instances(&gpu, 0, &[at(2.0)]);
    assert!(
        renderer.tracer().generation() > generation,
        "the move did not reach the tracer"
    );
    assert!(renderer.tracer().instances_current());
    let right = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    let raster_right = red_centroid(&gpu, &mut renderer, RenderMode::Raster, &camera);
    eprintln!("ray traced {left} -> {right}, raster {raster_left} -> {raster_right}");
    assert!(
        (right - left).x > 20.0,
        "the traced cube stayed: {left} -> {right}"
    );
    assert!(
        (right - left).distance(raster_right - raster_left) < 1.0,
        "{right} - {left} against raster {raster_right} - {raster_left}"
    );
    renderer.set_instances(&gpu, 0, &[at(-2.0), at(2.0)]);
    assert!(
        renderer.tracer().instances_current(),
        "a second instance joins the built scene"
    );
    let both = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    let raster_both = red_centroid(&gpu, &mut renderer, RenderMode::Raster, &camera);
    eprintln!("two cubes: ray traced {both}, raster {raster_both}");
    assert!(
        (both - right).distance(raster_both - raster_right) < 1.0,
        "{both} - {right} against raster {raster_both} - {raster_right}"
    );
    assert!(
        (both - right).x < -10.0,
        "the left cube did not join: {both}"
    );
    renderer.set_instances(&gpu, 0, &[at(2.0)]);
    let back = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    assert!(
        back.distance(right) < 0.5,
        "one cube again: {back} against {right}"
    );
}

/// A primitive merged into the world-space mesh cannot be re-instanced: it keeps its place, flagged, until an upload.
#[test]
fn set_instances_on_a_merged_primitive_says_it_waits() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let scene = scene("cube.gltf");
    let camera = facing_minus_z(12.0);
    let mut renderer = plain(&gpu, &scene, RasterPath::Forward);
    trace(&mut renderer, &scene);
    build(&gpu, &mut renderer);
    let home = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    renderer.set_instances(&gpu, 0, &[Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0))]);
    assert!(!renderer.tracer().instances_current());
    let kept = red_centroid(&gpu, &mut renderer, RenderMode::RayTraced, &camera);
    assert!(kept.distance(home) < 0.5, "the merged cube moved: {kept}");
}

/// A switch of mode, raster path or FXAA drops the passes the new frames do not record at once, so a traced frame lists its
/// kernel and resolve in record order instead of the raster rows a 32-frame average would keep.
#[test]
fn the_timings_follow_every_switch_of_mode_and_path_within_four_frames() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    if !gpu.supports_timestamps {
        assert!(
            !GpuContext::gpu_required(),
            "the CI adapters offer timestamps"
        );
        return;
    }
    let scene = scene("cube_on_plane.gltf");
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    renderer.upload_scene(&gpu, &scene);
    trace(&mut renderer, &scene);
    build(&gpu, &mut renderer);
    assert!(renderer.enable_gpu_timing(&gpu));
    renderer.occlusion_queries_enabled = true;
    renderer.auto_exposure = true;
    renderer.clouds.enabled = true;
    for mode in [MODES[0], MODES[2], MODES[3], MODES[1], MODES[2], MODES[0]] {
        (renderer.raster_path, renderer.render_mode) = mode;
        let reported = timed_after(&gpu, &mut renderer, 4);
        let fxaa = renderer.fxaa;
        assert_eq!(
            reported,
            passes_in(mode, fxaa),
            "after switching to {mode:?}"
        );
    }
    // FXAA switched off on a deferred frame drops its row as fast.
    (renderer.raster_path, renderer.render_mode) = MODES[1];
    renderer.fxaa = FxaaMode::Off;
    let reported = timed_after(&gpu, &mut renderer, 4);
    assert_eq!(reported, passes_in(MODES[1], FxaaMode::Off), "FXAA off");
}
