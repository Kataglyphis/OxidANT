//! The volumetric clouds on a headless adapter: the C++ cloud goldens' structural checks, and the ones they lack.
//! Each test names its box with `CloudSettings::place_box`, as the C++ goldens do with `place_cloud_box`.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::render::clouds::{CloudSettings, NOISE_VOLUME_EXTENT};
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: u32 = 128;
const PIXELS: usize = (SIZE * SIZE) as usize;

/// From (0, 0, distance) toward -Z: world +X is screen right and +Y screen up.
fn facing_minus_z(distance: f32) -> OrbitCamera {
    OrbitCamera {
        radius: distance,
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    }
}

/// The C++ goldens' clouds: coverage 0.3, so most of a box is cloud, in the box the test names.
fn clouds_in_box(centre: Vec3, half_extents: Vec3, density: f32) -> CloudSettings {
    let mut clouds = CloudSettings {
        enabled: true,
        coverage: 0.3,
        ..CloudSettings::default()
    };
    clouds.place_box(centre, half_extents, density);
    clouds
}

/// The unit cube of `tests/assets`, scaled to `half_extents` around `centre`: a wall or a card.
fn upload_box(gpu: &GpuContext, renderer: &mut ForwardRenderer, centre: Vec3, half_extents: Vec3) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    renderer.upload_scene(gpu, &load_gltf(path).expect("cube.gltf must load"));
    let placement =
        Mat4::from_scale_rotation_translation(half_extents * 2.0, Quat::IDENTITY, centre);
    renderer.set_instances(gpu, 0, &[placement]);
}

fn render(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

/// Largest colour-channel difference of one pixel between two RGBA8 frames, the C++ `channel_delta`.
fn channel_delta(a: &[u8], b: &[u8], pixel: usize) -> u8 {
    (0..3)
        .map(|c| a[pixel * 4 + c].abs_diff(b[pixel * 4 + c]))
        .max()
        .unwrap_or(0)
}

/// The pixel rect a world-space box covers; every corner must lie in front of the camera.
#[derive(Clone, Copy, Debug)]
struct PixelRect {
    min: glam::Vec2,
    max: glam::Vec2,
}

impl PixelRect {
    fn of_box(camera: &OrbitCamera, centre: Vec3, half_extents: Vec3) -> Self {
        let view_proj = camera.view_projection(1.0);
        let mut rect = Self {
            min: glam::Vec2::splat(f32::INFINITY),
            max: glam::Vec2::splat(f32::NEG_INFINITY),
        };
        for corner in 0..8 {
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let ndc = view_proj.project_point3(centre + sign * half_extents);
            let pixel = glam::Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * SIZE as f32;
            rect.min = rect.min.min(pixel);
            rect.max = rect.max.max(pixel);
        }
        rect
    }

    /// Whether pixel `index`'s centre lies inside, with the rect grown by `margin` pixels (shrunk if negative).
    fn contains(&self, index: usize, margin: f32) -> bool {
        let x = (index % SIZE as usize) as f32 + 0.5;
        let y = (index / SIZE as usize) as f32 + 0.5;
        x >= self.min.x - margin
            && x <= self.max.x + margin
            && y >= self.min.y - margin
            && y <= self.max.y + margin
    }
}

/// Pixels of `frame` that moved more than `threshold` levels from `reference`, as indices.
fn changed(reference: &[u8], frame: &[u8], threshold: u8) -> Vec<usize> {
    (0..PIXELS)
        .filter(|&pixel| channel_delta(reference, frame, pixel) > threshold)
        .collect()
}

#[test]
fn clouds_appear_when_enabled_and_vanish_when_disabled() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    let (centre, half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(6.0, 4.0, 4.0));
    renderer.clouds = clouds_in_box(centre, half, 1.0);
    renderer.clouds.enabled = false;
    let off = render(&gpu, &mut renderer, &camera);
    renderer.clouds.enabled = true;
    let on = render(&gpu, &mut renderer, &camera);
    renderer.clouds.enabled = false;
    let off_again = render(&gpu, &mut renderer, &camera);

    assert!(
        off == off_again,
        "switching the clouds off must restore the frame exactly"
    );
    let rect = PixelRect::of_box(&camera, centre, half);
    let inside = (0..PIXELS).filter(|&p| rect.contains(p, -2.0)).count();
    let moved = changed(&off, &on, 5);
    let moved_inside = moved.iter().filter(|&&p| rect.contains(p, -2.0)).count();
    let moved_outside = moved.iter().filter(|&&p| !rect.contains(p, 2.0)).count();
    eprintln!("box covers {inside} px; {moved_inside} of them changed, {moved_outside} outside it");
    assert!(
        moved_inside * 2 > inside,
        "enabled clouds changed only {moved_inside} of the {inside} pixels their box covers"
    );
    assert_eq!(moved_outside, 0, "clouds drew outside their box");
    let drifted = changed(&off, &on, 0)
        .into_iter()
        .filter(|&p| !rect.contains(p, 2.0))
        .count();
    assert_eq!(
        drifted, 0,
        "a cloud-free pixel must keep its colour when clouds are on"
    );
}

/// The C++ `EnablingCloudsChangesTheFrameAndAddsDetail`: the slab lowered into view, the upper half measured.
#[test]
fn enabling_clouds_changes_the_frame_and_adds_detail() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    renderer.upload_scene(
        &gpu,
        &load_gltf(path).expect("cube_on_plane.gltf must load"),
    );
    // Just above the plane and tilted up, so the upper half is all sky.
    let camera = OrbitCamera {
        target: Vec3::new(0.0, 1.0, 0.0),
        radius: 8.0,
        pitch_deg: -2.0,
        ..OrbitCamera::default()
    };
    renderer.clouds = CloudSettings {
        coverage: 0.6,
        density: 2.0,
        offset: Vec3::new(-0.364, 6.0, -18.351),
        ..CloudSettings::default()
    };
    let off = render(&gpu, &mut renderer, &camera);
    renderer.clouds.enabled = true;
    let on = render(&gpu, &mut renderer, &camera);

    let upper = PIXELS / 2;
    let swung = changed(&off, &on, 5).iter().filter(|&&p| p < upper).count();
    let swung = swung as f64 / upper as f64;
    let (detail_off, detail_on) = (detail_fraction(&off), detail_fraction(&on));
    eprintln!("swung {swung:.3}, detail {detail_off:.3} -> {detail_on:.3}");
    assert!(
        swung > 0.05,
        "enabling clouds barely changed the sky: {swung}"
    );
    assert!(
        detail_on > detail_off,
        "the clouds added no pixel-to-pixel detail ({detail_off} -> {detail_on}): a flat noise volume?"
    );
}

/// The C++ `detail_fraction` over the upper half: neighbours more than 6 luma levels apart.
fn detail_fraction(frame: &[u8]) -> f64 {
    let luma = |p: usize| {
        0.2126 * f64::from(frame[p * 4])
            + 0.7152 * f64::from(frame[p * 4 + 1])
            + 0.0722 * f64::from(frame[p * 4 + 2])
    };
    let size = SIZE as usize;
    let (mut detailed, mut total) = (0, 0);
    for y in 0..size / 2 {
        for x in 0..size - 1 {
            let p = y * size + x;
            detailed += usize::from((luma(p) - luma(p + 1)).abs() > 6.0);
            total += 1;
        }
    }
    detailed as f64 / total as f64
}

/// Changed pixels per image half, and their mean column.
fn footprint(off: &[u8], on: &[u8]) -> ([usize; 2], f32) {
    let moved = changed(off, on, 5);
    let size = SIZE as usize;
    let mut halves = [0; 2];
    for &p in &moved {
        halves[usize::from(p % size >= size / 2)] += 1;
    }
    let columns: usize = moved.iter().map(|p| p % size).sum();
    (halves, columns as f32 / moved.len().max(1) as f32)
}

/// Offset places the box and Scale x Density sizes it; in C++ it was once a sheared slab Offset could not move.
#[test]
fn offset_moves_the_cloud_volume_and_away_from_the_view_removes_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    let off = render(&gpu, &mut renderer, &camera);
    // Deep and dense, so each box draws plenty; 2.5 wide around x = 4 still keeps to its half of the view.
    let half = Vec3::new(2.5, 2.5, 6.0);
    let mut frame_with_box_at = |centre: Vec3| {
        renderer.clouds = clouds_in_box(centre, half, 2.0);
        render(&gpu, &mut renderer, &camera)
    };
    let left = Vec3::new(-4.0, 0.0, 0.0);
    let right = Vec3::new(4.0, 0.0, 0.0);
    let frames = [frame_with_box_at(left), frame_with_box_at(right)];
    let behind = frame_with_box_at(Vec3::new(0.0, 0.0, 40.0));
    let above = frame_with_box_at(Vec3::new(0.0, 40.0, 0.0));

    for (centre, frame) in [left, right].into_iter().zip(&frames) {
        let (halves, column) = footprint(&off, frame);
        eprintln!("box at {centre}: {halves:?} px per half, mean column {column:.1}");
        let (near, far) = if centre.x < 0.0 { (0, 1) } else { (1, 0) };
        assert!(
            halves[near] > PIXELS / 100,
            "the box at {centre} drew no clouds"
        );
        assert_eq!(halves[far], 0, "the box at {centre} drew in the other half");
        let off_axis = if centre.x < 0.0 {
            column < SIZE as f32 * 0.4
        } else {
            column > SIZE as f32 * 0.6
        };
        assert!(
            off_axis,
            "the box at {centre} does not show on its side: column {column}"
        );
        let rect = PixelRect::of_box(&camera, centre, half);
        let outside = changed(&off, frame, 0)
            .into_iter()
            .filter(|&p| !rect.contains(p, 2.0))
            .count();
        assert_eq!(
            outside, 0,
            "the box at {centre} drew outside its Scale x Density extent"
        );
    }
    assert!(
        behind == off,
        "a box behind the camera must leave the frame untouched"
    );
    assert!(
        above == off,
        "a box above the view must leave the frame untouched"
    );
}

/// A wall in front of the box hides it: the box shows around the wall, never on it.
#[test]
fn geometry_in_front_of_the_clouds_hides_them() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    let (wall_centre, wall_half) = (Vec3::ZERO, Vec3::new(3.0, 2.0, 0.1));
    upload_box(&gpu, &mut renderer, wall_centre, wall_half);
    let (box_centre, box_half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(8.0, 5.0, 3.0));
    renderer.clouds = clouds_in_box(box_centre, box_half, 1.0);
    renderer.clouds.enabled = false;
    let off = render(&gpu, &mut renderer, &camera);
    renderer.clouds.enabled = true;
    let on = render(&gpu, &mut renderer, &camera);

    let wall = PixelRect::of_box(&camera, wall_centre, wall_half);
    let clouds = PixelRect::of_box(&camera, box_centre, box_half);
    let wall_pixels: Vec<usize> = (0..PIXELS).filter(|&p| wall.contains(p, -2.0)).collect();
    let sky_pixels: Vec<usize> = (0..PIXELS)
        .filter(|&p| clouds.contains(p, -2.0) && !wall.contains(p, 2.0))
        .collect();
    let wall_changed = wall_pixels
        .iter()
        .filter(|&&p| channel_delta(&off, &on, p) > 0)
        .count();
    let sky_changed = sky_pixels
        .iter()
        .filter(|&&p| channel_delta(&off, &on, p) > 5)
        .count();
    eprintln!(
        "{wall_changed} of {} wall px changed, {sky_changed} of {} sky px",
        wall_pixels.len(),
        sky_pixels.len()
    );
    assert!(
        wall_pixels.len() > PIXELS / 20,
        "the wall fills too little of the frame to judge"
    );
    assert!(
        sky_changed * 4 > sky_pixels.len(),
        "the box behind the wall drew no clouds around it, so nothing was there to hide"
    );
    assert_eq!(
        wall_changed, 0,
        "clouds behind the wall drew over it: the march ignores the depth"
    );
}

/// A box around the wall: the march stops at the wall, so less cloud covers it than the sky beside it.
#[test]
fn geometry_inside_the_box_cuts_the_march_short() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    let (wall_centre, wall_half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(3.0, 2.0, 0.1));
    upload_box(&gpu, &mut renderer, wall_centre, wall_half);
    let box_half = Vec3::new(8.0, 5.0, 4.0);
    renderer.clouds = clouds_in_box(Vec3::new(0.0, 0.0, -10.0), box_half, 1.0);
    render(&gpu, &mut renderer, &camera);
    let cloud = renderer
        .read_cloud_target(&gpu)
        .expect("cloud readback must succeed")
        .expect("clouds ran, so their target exists");

    let wall = PixelRect::of_box(&camera, wall_centre, wall_half);
    let beside = PixelRect::of_box(&camera, wall_centre, Vec3::new(5.0, 2.0, 0.1));
    let mean_alpha = |pixels: &mut dyn Iterator<Item = usize>| {
        let alphas: Vec<f32> = pixels.map(|p| half_to_f32(cloud[p * 4 + 3])).collect();
        alphas.iter().sum::<f32>() / alphas.len().max(1) as f32
    };
    let over_wall = mean_alpha(&mut (0..PIXELS).filter(|&p| wall.contains(p, -2.0)));
    let over_sky = mean_alpha(
        &mut (0..PIXELS).filter(|&p| beside.contains(p, -2.0) && !wall.contains(p, 2.0)),
    );
    eprintln!("cloud alpha over the wall {over_wall:.3}, beside it {over_sky:.3}");
    assert!(
        over_wall > 0.01,
        "the half of the box in front of the wall must still draw"
    );
    assert!(
        over_wall < over_sky * 0.8,
        "the march ran past the wall: alpha {over_wall} over it, {over_sky} beside it"
    );
}

/// ACES (Narkowicz) and the exact sRGB encode, as `aces.slang` and an sRGB target apply them.
fn displayed(linear: f32) -> f32 {
    let x = linear.min(1e4);
    let mapped = ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0);
    let encoded = if mapped < 0.003_130_8 {
        mapped * 12.92
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

/// What the tonemap shows for `cloud` over `hdr`, without bloom, AO or exposure: premultiplied-over in linear HDR.
fn composited(cloud: &[u16], hdr: &[u16], pixel: usize) -> [f32; 3] {
    let alpha = half_to_f32(cloud[pixel * 4 + 3]);
    std::array::from_fn(|c| {
        let over =
            half_to_f32(cloud[pixel * 4 + c]) + half_to_f32(hdr[pixel * 4 + c]) * (1.0 - alpha);
        displayed(over)
    })
}

/// Pixels whose shown colour is more than 2 levels from `expected`'s.
fn off_by_more_than_two(
    frame: &[u8],
    pixels: &[usize],
    expected: impl Fn(usize) -> [f32; 3],
) -> usize {
    pixels
        .iter()
        .filter(|&&p| {
            let want = expected(p);
            (0..3).any(|c| (f32::from(frame[p * 4 + c]) - want[c]).abs() > 2.0)
        })
        .count()
}

/// A frame of the box over whatever stands behind it: its display, cloud and HDR targets.
struct Composite {
    shown: Vec<u8>,
    cloud: Vec<u16>,
    hdr: Vec<u16>,
}

fn composite_frame(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    camera: &OrbitCamera,
) -> Composite {
    let shown = render(gpu, renderer, camera);
    let cloud = renderer
        .read_cloud_target(gpu)
        .expect("cloud readback")
        .expect("clouds ran");
    let hdr = renderer.read_hdr_target(gpu).expect("HDR readback");
    Composite { shown, cloud, hdr }
}

/// The same cloud over the sky and over a card behind it: the march matches, and both get one composite.
/// The C++ golden compares one red channel over navy sky and black card; here every pixel is predicted.
#[test]
fn a_cloud_composites_over_the_sky_as_over_geometry() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let camera = facing_minus_z(20.0);
    let new_renderer = || {
        let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
        // No bloom, AO or exposure, which differ between backgrounds; the tonemap's own math stays.
        renderer.bloom_strength = 0.0;
        renderer.ssao_strength = 0.0;
        // Deep enough to be half opaque, without powder, which darkens thin cloud to almost nothing.
        renderer.clouds = CloudSettings {
            powder: false,
            ..clouds_in_box(Vec3::new(0.0, 0.0, -5.0), Vec3::new(6.0, 4.0, 10.0), 1.0)
        };
        renderer
    };
    let mut over_sky = new_renderer();
    let sky = composite_frame(&gpu, &mut over_sky, &camera);
    let mut over_card = new_renderer();
    let (card_centre, card_half) = (Vec3::new(0.0, 0.0, -20.0), Vec3::new(8.0, 6.0, 0.1));
    upload_box(&gpu, &mut over_card, card_centre, card_half);
    let card = composite_frame(&gpu, &mut over_card, &camera);

    let card_rect = PixelRect::of_box(&camera, card_centre, card_half);
    let region: Vec<usize> = (0..PIXELS)
        .filter(|&p| card_rect.contains(p, -2.0))
        .collect();
    let alpha = |frame: &Composite, p: usize| half_to_f32(frame.cloud[p * 4 + 3]);
    let clouded = region.iter().filter(|&&p| alpha(&sky, p) > 0.2).count();
    let march_mismatch = region
        .iter()
        .filter(|&&p| (0..4).any(|c| sky.cloud[p * 4 + c] != card.cloud[p * 4 + c]))
        .count();
    let differs = region
        .iter()
        .filter(|&&p| channel_delta(&sky.shown, &card.shown, p) > 5)
        .count();
    eprintln!(
        "{} px of card, {clouded} clouded, card visible in {differs}",
        region.len()
    );
    assert!(
        clouded * 3 > region.len(),
        "too little cloud in front of the card to compare"
    );
    assert!(
        differs * 2 > region.len(),
        "the card must show behind the cloud, or both backgrounds are one"
    );
    assert_eq!(
        march_mismatch, 0,
        "the card behind the box changed the cloud's own colour or alpha"
    );

    let all: Vec<usize> = (0..PIXELS).collect();
    for (name, frame) in [("sky", &sky), ("card", &card)] {
        let wrong = off_by_more_than_two(&frame.shown, &all, |p| {
            composited(&frame.cloud, &frame.hdr, p)
        });
        assert_eq!(
            wrong, 0,
            "over the {name}, {wrong} pixels are not cloud + HDR x (1 - alpha)"
        );
    }
    let crossed = off_by_more_than_two(&card.shown, &region, |p| {
        composited(&sky.cloud, &card.hdr, p)
    });
    assert_eq!(
        crossed, 0,
        "the cloud measured over the sky does not composite over the card as shown"
    );
}

#[test]
fn the_noise_volume_fills_every_channel_the_march_reads() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, 16, 16);
    let camera = facing_minus_z(20.0);
    assert!(
        renderer
            .read_noise_volume(&gpu)
            .expect("readback")
            .is_none(),
        "the volume is made the first frame clouds are on, not before"
    );
    renderer.clouds.enabled = true;
    renderer
        .render_to_pixels(&gpu, 16, 16, &camera)
        .expect("render");
    let volume = renderer
        .read_noise_volume(&gpu)
        .expect("noise readback must succeed")
        .expect("clouds ran, so the volume exists");
    let extent = NOISE_VOLUME_EXTENT as usize;
    assert_eq!(volume.len(), extent * extent * extent * 4);

    let channel = |c: usize| -> Vec<f32> {
        volume
            .iter()
            .skip(c)
            .step_by(4)
            .map(|&h| half_to_f32(h))
            .collect()
    };
    let channels: Vec<Vec<f32>> = (0..4).map(channel).collect();
    for (c, values) in channels.iter().enumerate() {
        let stats = Stats::of(values);
        let slice = extent * extent;
        let along_z = values
            .windows(slice + 1)
            .step_by(97)
            .map(|w| (w[slice] - w[0]).abs())
            .sum::<f32>();
        eprintln!("channel {c}: {stats:?}, z variation {along_z:.2}");
        assert!(
            values
                .iter()
                .all(|v| v.is_finite() && (-0.001..=1.001).contains(v)),
            "channel {c} leaves [0, 1]"
        );
        assert!(stats.std > 0.03, "channel {c} is flat: {stats:?}");
        assert!(
            stats.max - stats.min > 0.3,
            "channel {c} spans too little: {stats:?}"
        );
        assert!(
            along_z > 1.0,
            "channel {c} does not vary along z: the dispatch misses a dimension"
        );
    }
    for (worley, cells) in [(0, 4), (2, 8)] {
        let stats = Stats::of(&channels[worley]);
        assert!(
            stats.min < 0.1,
            "Worley ({cells} cells) never comes near a feature point: {stats:?}"
        );
    }
    let mean_gap = |a: usize, b: usize| {
        channels[a]
            .iter()
            .zip(&channels[b])
            .map(|(x, y)| (x - y).abs())
            .sum::<f32>()
            / channels[a].len() as f32
    };
    assert!(
        mean_gap(0, 2) > 0.02,
        "the coarse and fine Worley channels are one"
    );
    assert!(mean_gap(1, 3) > 0.02, "the two fBm channels are one");
}

#[derive(Debug)]
struct Stats {
    min: f32,
    max: f32,
    std: f32,
}

impl Stats {
    fn of(values: &[f32]) -> Self {
        let n = values.len() as f64;
        let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
        let var = values
            .iter()
            .map(|&v| (f64::from(v) - mean).powi(2))
            .sum::<f64>()
            / n;
        Self {
            min: values.iter().copied().fold(f32::INFINITY, f32::min),
            max: values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            std: var.sqrt() as f32,
        }
    }
}

/// The C++ `CloudsAcrossManyFramesDoesNotLoseTheDevice`, with resizes, which recreate the target and its depth.
#[test]
fn clouds_keep_rendering_across_many_frames_and_resizes() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    let camera = facing_minus_z(20.0);
    renderer.clouds = clouds_in_box(Vec3::new(0.0, 0.0, -10.0), Vec3::new(6.0, 4.0, 4.0), 1.0);
    for (width, height) in [(64, 64), (96, 48), (64, 64)] {
        let mut last = Vec::new();
        for _ in 0..10 {
            last = renderer
                .render_to_pixels(&gpu, width, height, &camera)
                .expect("render");
        }
        renderer.clouds.enabled = false;
        let off = renderer
            .render_to_pixels(&gpu, width, height, &camera)
            .expect("render");
        renderer.clouds.enabled = true;
        let moved = (0..(width * height) as usize)
            .filter(|&p| channel_delta(&off, &last, p) > 5)
            .count();
        assert!(moved > 0, "at {width}x{height} the clouds stopped drawing");
    }
}
