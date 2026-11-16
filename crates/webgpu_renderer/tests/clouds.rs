//! The volumetric clouds on a headless adapter: the C++ cloud goldens' structural checks, and the ones they lack.
//! Each test names its box with `CloudSettings::place_box`, as the C++ goldens do with `place_cloud_box`.

use glam::{Mat4, Quat, Vec3};
use kataglyphis_webgpu_renderer::asset::skybox::CubemapFaces;
use kataglyphis_webgpu_renderer::render::clouds::{
    CloudAmbient, CloudSettings, NOISE_VOLUME_EXTENT,
};
use kataglyphis_webgpu_renderer::render::forward::ShaderSources;
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

/// Under the navy, as the C++ goldens: lit by the horizon's own ambient, a cloud barely differs from it.
fn navy_sky_renderer(gpu: &GpuContext) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.skybox_enabled = false;
    renderer
}

#[test]
fn clouds_appear_when_enabled_and_vanish_when_disabled() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = navy_sky_renderer(&gpu);
    let camera = facing_minus_z(20.0);
    let (centre, half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(6.0, 4.0, 4.0));
    // Without powder, which leaves this thin box's sunlight barely above the navy it scatters as ambient.
    renderer.clouds = CloudSettings {
        powder: false,
        ..clouds_in_box(centre, half, 1.0)
    };
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
        // The box Density 2 gave the default Scale while it sized the box.
        scale: Vec3::new(2000.0, 10.0, 2000.0),
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

/// Offset places the box and Scale sizes it; in C++ it was once a sheared slab Offset could not move.
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
            "the box at {centre} drew outside its Scale extent"
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
    let mut renderer = navy_sky_renderer(&gpu);
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

/// The cloud target after one frame of `clouds` from `camera`.
fn marched(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    clouds: CloudSettings,
    camera: &OrbitCamera,
) -> Vec<u16> {
    renderer.clouds = clouds;
    render(gpu, renderer, camera);
    renderer
        .read_cloud_target(gpu)
        .expect("cloud readback must succeed")
        .expect("clouds ran, so their target exists")
}

/// Pixels with any cloud in front of them.
fn clouded(cloud: &[u16]) -> Vec<bool> {
    (0..PIXELS)
        .map(|p| half_to_f32(cloud[p * 4 + 3]) > 0.0)
        .collect()
}

/// Density thins the cloud and Scale sizes the box, each without moving what the other sets.
/// While Density also sized the box, a quarter of it drew a box a quarter as wide.
#[test]
fn density_and_scale_change_apart() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = plain_renderer(&gpu);
    let camera = facing_minus_z(20.0);
    let (centre, half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(4.0, 3.0, 4.0));
    let dense = clouds_in_box(centre, half, 1.0);
    let thin = CloudSettings {
        density: 0.25,
        ..dense
    };
    let wide = CloudSettings {
        scale: dense.scale * Vec3::new(1.5, 1.0, 1.0),
        ..dense
    };
    let [dense_cloud, thin_cloud, wide_cloud] =
        [dense, thin, wide].map(|clouds| marched(&gpu, &mut renderer, clouds, &camera));

    let (dense_mask, thin_mask) = (clouded(&dense_cloud), clouded(&thin_cloud));
    let footprint = dense_mask.iter().filter(|&&on| on).count();
    let moved = (0..PIXELS)
        .filter(|&p| dense_mask[p] != thin_mask[p])
        .count();
    let alpha_sum = |cloud: &[u16]| (0..PIXELS).map(|p| texel(cloud, p)[3]).sum::<f32>();
    let (dense_alpha, thin_alpha) = (alpha_sum(&dense_cloud), alpha_sum(&thin_cloud));
    eprintln!(
        "footprint {footprint} px, {moved} moved at a quarter Density; alpha {dense_alpha:.0} -> {thin_alpha:.0}"
    );
    let rect = PixelRect::of_box(&camera, centre, half);
    assert!(
        footprint > (0..PIXELS).filter(|&p| rect.contains(p, -2.0)).count() / 2,
        "the box drew too little cloud to judge: {footprint} px"
    );
    assert!(
        moved <= footprint / 100,
        "a quarter Density moved {moved} of the box's {footprint} clouded pixels"
    );
    assert!(
        thin_alpha < dense_alpha * 0.8,
        "a quarter Density must thin the cloud: alpha {dense_alpha} -> {thin_alpha}"
    );

    // Inside the back face's outline every ray crosses the box front to back, whatever its width.
    let back = PixelRect::of_box(
        &camera,
        centre - Vec3::Z * half.z,
        Vec3::new(half.x, half.y, 0.0),
    );
    let inner: Vec<usize> = (0..PIXELS).filter(|&p| back.contains(p, -2.0)).collect();
    let differing = inner
        .iter()
        .filter(|&&p| dense_cloud[p * 4 + 3] != wide_cloud[p * 4 + 3])
        .count();
    assert!(inner.len() > PIXELS / 40, "{} inner pixels", inner.len());
    assert_eq!(
        differing,
        0,
        "a wider box changed the cloud along {differing} of {} rays it did not lengthen",
        inner.len()
    );
    let wide_footprint = clouded(&wide_cloud).iter().filter(|&&on| on).count();
    assert!(
        wide_footprint > footprint + footprint / 5,
        "a box 1.5 times as wide must cover more: {footprint} -> {wide_footprint} px"
    );
}

/// The cloud target of `clouds` after `time_s` seconds of wind.
fn marched_at(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    (clouds, time_s): (CloudSettings, f64),
    camera: &OrbitCamera,
) -> Vec<u16> {
    renderer.cloud_time_seconds = time_s;
    marched(gpu, renderer, clouds, camera)
}

/// The wind moves the noise through a box that stays put: a pinned time repeats its frame bit for bit, time 0 is the
/// still cloud, another time moves it, and a time one noise period of travel later draws the same cloud again.
#[test]
fn the_wind_moves_the_clouds_with_time_and_a_pinned_time_repeats() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = plain_renderer(&gpu);
    let camera = facing_minus_z(20.0);
    let (centre, half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(5.0, 3.0, 4.0));
    let windy = CloudSettings {
        wind: Vec3::new(8.0, 0.0, 0.0),
        ..clouds_in_box(centre, half, 1.0)
    };
    let still = CloudSettings {
        wind: Vec3::ZERO,
        ..windy
    };
    let mut at = |clouds, time_s| marched_at(&gpu, &mut renderer, (clouds, time_s), &camera);
    let resting = at(still, 2.5);
    let start = at(windy, 0.0);
    let later = at(windy, 2.5);
    let later_again = at(windy, 2.5);
    // 32 s at 8 units per second is the noise's 256-unit period.
    let a_period_later = at(windy, 34.5);

    assert!(start == resting, "time 0 must draw the still cloud");
    assert!(later == later_again, "a pinned time must repeat its frame");
    assert!(
        later == a_period_later,
        "a whole noise period of travel must draw the same cloud"
    );
    let rect = PixelRect::of_box(&camera, centre, half);
    let inside: Vec<usize> = (0..PIXELS).filter(|&p| rect.contains(p, -2.0)).collect();
    let moved = inside
        .iter()
        .filter(|&&p| (texel(&start, p)[3] - texel(&later, p)[3]).abs() > 0.02)
        .count();
    let outside = (0..PIXELS)
        .filter(|&p| !rect.contains(p, 2.0) && texel(&later, p)[3] > 0.0)
        .count();
    eprintln!(
        "20 units of wind moved the alpha of {moved} of {} pixels in the box",
        inside.len()
    );
    assert!(
        moved > inside.len() / 4,
        "20 units of wind moved only {moved} of {} pixels",
        inside.len()
    );
    assert_eq!(outside, 0, "the wind must not move the box");
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

/// Relative luminance of linear RGB.
fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// One pixel of a half-float RGBA target.
fn texel(target: &[u16], pixel: usize) -> [f32; 4] {
    std::array::from_fn(|c| half_to_f32(target[pixel * 4 + c]))
}

/// A renderer without bloom or AO, so a target's pixels are what the tonemap composes.
fn plain_renderer(gpu: &GpuContext) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.bloom_strength = 0.0;
    renderer.ssao_strength = 0.0;
    renderer
}

/// Under the default sun and procedural sky, dense cloud shows at least 70 % of the sky behind it.
/// Lit by the sun's single scattering alone it showed 48 %; the sky's ambient and the scattering octaves lift it.
#[test]
fn a_daylit_cloud_is_not_much_darker_than_the_sky_behind_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = plain_renderer(&gpu);
    renderer.clouds = clouds_in_box(Vec3::new(0.0, 0.0, -5.0), Vec3::new(6.0, 4.0, 10.0), 1.5);
    let frame = composite_frame(&gpu, &mut renderer, &facing_minus_z(20.0));

    let (mut dense, mut cloud_sum, mut sky_sum) = (0, 0.0, 0.0);
    for pixel in 0..PIXELS {
        let [r, g, b, alpha] = texel(&frame.cloud, pixel);
        if alpha < 0.5 {
            continue;
        }
        let [sr, sg, sb, _] = texel(&frame.hdr, pixel);
        let shown = [
            r + sr * (1.0 - alpha),
            g + sg * (1.0 - alpha),
            b + sb * (1.0 - alpha),
        ];
        dense += 1;
        cloud_sum += luminance(shown);
        sky_sum += luminance([sr, sg, sb]);
    }
    let ratio = cloud_sum / sky_sum;
    eprintln!("{dense} dense px: cloud luminance {cloud_sum:.1}, sky behind {sky_sum:.1}, ratio {ratio:.3}");
    assert!(
        dense > PIXELS / 20,
        "too little dense cloud to judge: {dense} px"
    );
    assert!(
        ratio > 0.7,
        "a daylit cloud shows {ratio} of the sky's luminance behind it: is the ambient term gone?"
    );
}

/// The clouds' ambient is the sky the frame draws: a solid cube map or the navy, as the sky pass shows it.
#[test]
fn the_cloud_ambient_is_the_drawn_sky() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let camera = facing_minus_z(20.0);
    let mut renderer = plain_renderer(&gpu);
    let faces = CubemapFaces::solid([[200, 120, 40, 255]; 6]);
    renderer.set_skybox(&gpu, &faces);
    let cases = [
        ("cube map", true, CloudAmbient::of_cubemap(&faces)),
        ("navy", false, CloudAmbient::sky_off()),
    ];
    for (name, skybox_enabled, ambient) in cases {
        renderer.skybox_enabled = skybox_enabled;
        render(&gpu, &mut renderer, &camera);
        let hdr = renderer.read_hdr_target(&gpu).expect("HDR readback");
        let sky = Vec3::from_slice(&texel(&hdr, PIXELS / 2)[..3]);
        eprintln!("{name}: sky {sky}, ambient {ambient:?}");
        assert!(
            ambient.top.abs_diff_eq(sky, 2e-2 * sky.max_element())
                && ambient.bottom.abs_diff_eq(sky, 2e-2 * sky.max_element()),
            "the {name} ambient {ambient:?} is not the sky the pass draws, {sky}"
        );
    }
}

/// Mean step between horizontal neighbours over the mean, where there is cloud: of the luminance, or of alpha.
fn neighbour_contrast(cloud: &[u16], value: impl Fn([f32; 4]) -> f32) -> f64 {
    let size = SIZE as usize;
    let (mut steps, mut total) = (0.0, 0.0);
    for y in 0..size {
        for x in 0..size - 1 {
            let (a, b) = (texel(cloud, y * size + x), texel(cloud, y * size + x + 1));
            if a[3] <= 0.0 && b[3] <= 0.0 {
                continue;
            }
            steps += f64::from((value(a) - value(b)).abs());
            total += f64::from(value(a));
        }
    }
    steps / total.max(f64::MIN_POSITIVE)
}

/// The default slab seen from below at 3 to 34 degrees, marched with `march_steps`: its cloud target.
fn grazing_slab(gpu: &GpuContext, march_steps: i32) -> Vec<u16> {
    let mut renderer = plain_renderer(gpu);
    renderer.clouds = CloudSettings {
        enabled: true,
        march_steps,
        offset: Vec3::new(0.0, 367.0, 0.0),
        ..CloudSettings::default()
    };
    let camera = OrbitCamera {
        target: Vec3::new(0.0, 10.0, -100.0),
        radius: 100.0,
        yaw_deg: 90.0,
        pitch_deg: -12.0,
        far: 100_000.0,
        ..OrbitCamera::default()
    };
    render(gpu, &mut renderer, &camera);
    renderer
        .read_cloud_target(gpu)
        .expect("cloud readback must succeed")
        .expect("clouds ran")
}

/// Paths of 120 to 1200 units through the 63-unit slab: 8 fixed steps showed neighbours 42 % of the luminance
/// apart (128 steps: 31 %), and alpha 1.6 times as rough as at 128 steps.
#[test]
fn a_grazing_view_through_the_slab_is_not_grainy() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let cloud = grazing_slab(&gpu, CloudSettings::default().march_steps);
    let reference = grazing_slab(&gpu, 128);
    let clouded = (0..PIXELS).filter(|&p| texel(&cloud, p)[3] > 0.1).count();
    let shown = neighbour_contrast(&cloud, |t| luminance([t[0], t[1], t[2]]));
    let alpha = neighbour_contrast(&cloud, |t| t[3]);
    let reference_alpha = neighbour_contrast(&reference, |t| t[3]);
    eprintln!("{clouded} clouded px; luminance contrast {shown:.3}, alpha {alpha:.4} against {reference_alpha:.4}");
    assert!(
        clouded > PIXELS / 3,
        "the slab fills too little of the view: {clouded} px"
    );
    assert!(
        shown < 0.15,
        "neighbouring pixels differ by {shown} of the cloud's luminance: the march aliases the noise"
    );
    assert!(
        alpha < reference_alpha * 1.25,
        "the default march's alpha is {alpha} rough, 128 steps' {reference_alpha}: long paths need more steps"
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

/// Mean step between texels `at(k - 1)` and `at(k)` of one channel, over every row along `axis`.
fn mean_step(volume: &[u16], channel: usize, axis: usize, pairs: &[(usize, usize)]) -> f32 {
    let n = NOISE_VOLUME_EXTENT as usize;
    let index = |along: usize, u: usize, v: usize| {
        let mut xyz = [0; 3];
        xyz[axis] = along;
        xyz[(axis + 1) % 3] = u;
        xyz[(axis + 2) % 3] = v;
        ((xyz[2] * n + xyz[1]) * n + xyz[0]) * 4 + channel
    };
    let mut sum = 0.0;
    let mut count = 0;
    for u in (0..n).step_by(3) {
        for v in (0..n).step_by(3) {
            for &(a, b) in pairs {
                sum += (half_to_f32(volume[index(a, u, v)]) - half_to_f32(volume[index(b, u, v)]))
                    .abs();
                count += 1;
            }
        }
    }
    sum / count as f32
}

/// The march wraps box-local coordinates with `frac`, so a channel that does not tile shows a seam at every wrap.
#[test]
fn every_noise_channel_tiles_across_the_volume_faces() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, 16, 16);
    renderer.clouds.enabled = true;
    renderer
        .render_to_pixels(&gpu, 16, 16, &facing_minus_z(20.0))
        .expect("render");
    let volume = noise_volume(&gpu, &renderer);
    let n = NOISE_VOLUME_EXTENT as usize;
    let inner: Vec<(usize, usize)> = (8..n).step_by(8).map(|k| (k - 1, k)).collect();
    let mut seams = Vec::new();
    for channel in 0..4 {
        for axis in 0..3 {
            let wrap = mean_step(&volume, channel, axis, &[(n - 1, 0)]);
            let step = mean_step(&volume, channel, axis, &inner);
            eprintln!("channel {channel}, axis {axis}: wrap step {wrap:.4}, inner step {step:.4}");
            if wrap > 2.0 * step {
                seams.push((channel, axis, wrap, step));
            }
        }
    }
    assert!(
        seams.is_empty(),
        "(channel, axis, wrap step, inner step) where the volume does not continue across its face: {seams:?}"
    );
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

/// Half-float bits of 0.25, which the edited noise and march shaders below write into every texel.
const QUARTER: u16 = 0x3400;

/// `source` with `old` replaced by `new`; fails when the generated WGSL no longer has `old`.
fn edited(source: &str, old: &str, new: &str) -> String {
    let edited = source.replace(old, new);
    assert_ne!(
        edited, source,
        "the generated WGSL no longer contains `{old}`"
    );
    edited
}

/// The shipped cloud shaders with every noise texel and every march output set to 0.25.
fn flat_cloud_shaders() -> (String, String) {
    let shipped = ShaderSources::shipped();
    let noise = edited(
        shipped.cloud_noise,
        "(noise_volume_texel_0(tid_0))",
        "(vec4<f32>(0.25f))",
    );
    let march = edited(
        shipped.cloud_march,
        "pixelOutput_0( ",
        "pixelOutput_0( vec4<f32>(0.25f) + 0.0f * ",
    );
    (noise, march)
}

fn noise_volume(gpu: &GpuContext, renderer: &ForwardRenderer) -> Vec<u16> {
    renderer
        .read_noise_volume(gpu)
        .expect("noise readback must succeed")
        .expect("clouds ran, so the volume exists")
}

/// F5 rebuilds the march and refills the noise inside the reload's error scope; a refused reload keeps both.
#[test]
fn a_shader_reload_rebuilds_the_cloud_pipelines_or_keeps_them() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let shipped = ShaderSources::shipped();
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    renderer.clouds = clouds_in_box(Vec3::new(0.0, 0.0, -10.0), Vec3::new(6.0, 4.0, 4.0), 1.0);
    let before = render(&gpu, &mut renderer, &camera);
    let noise = noise_volume(&gpu, &renderer);
    let no_march = edited(shipped.cloud_march, "fn fs_main(", "fn fs_main_gone(");
    let no_noise = edited(shipped.cloud_noise, "fn cs_noise(", "fn cs_noise_gone(");
    for broken in [
        ShaderSources {
            cloud_march: &no_march,
            ..shipped
        },
        ShaderSources {
            cloud_noise: &no_noise,
            ..shipped
        },
    ] {
        assert!(
            renderer.reload_shaders(&gpu, &broken).is_err(),
            "a cloud shader without its entry point must be refused"
        );
        let kept = render(&gpu, &mut renderer, &camera);
        assert!(kept == before, "the refused reload changed the clouds");
    }
    assert!(
        noise_volume(&gpu, &renderer) == noise,
        "the refused reload refilled the volume"
    );

    let (flat_noise, flat_march) = flat_cloud_shaders();
    let flat = ShaderSources {
        cloud_march: &flat_march,
        cloud_noise: &flat_noise,
        ..shipped
    };
    renderer
        .reload_shaders(&gpu, &flat)
        .expect("valid cloud shaders must reload");
    render(&gpu, &mut renderer, &camera);
    let refilled = noise_volume(&gpu, &renderer);
    assert!(
        refilled.iter().all(|&h| h == QUARTER),
        "the reload did not refill the volume"
    );
    let marched = renderer
        .read_cloud_target(&gpu)
        .expect("cloud readback must succeed")
        .expect("clouds ran");
    assert!(
        marched.iter().all(|&h| h == QUARTER),
        "the reload did not replace the march"
    );

    renderer
        .reload_shaders(&gpu, &shipped)
        .expect("the shipped shaders must reload");
    let again = render(&gpu, &mut renderer, &camera);
    assert!(
        again == before,
        "the shipped shaders did not bring the clouds back"
    );
    assert!(
        noise_volume(&gpu, &renderer) == noise,
        "the volume did not refill with the shipped noise"
    );
}

/// A reload before clouds ever ran is what their first frame builds on, not the WGSL the crate was built with.
#[test]
fn the_first_cloud_frame_after_a_reload_builds_on_the_reloaded_shaders() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let (flat_noise, _) = flat_cloud_shaders();
    let mut renderer = ForwardRenderer::new(&gpu, 16, 16);
    let reload = ShaderSources {
        cloud_noise: &flat_noise,
        ..ShaderSources::shipped()
    };
    renderer
        .reload_shaders(&gpu, &reload)
        .expect("valid cloud shaders must reload");
    assert!(
        renderer
            .read_noise_volume(&gpu)
            .expect("readback")
            .is_none(),
        "a reload with clouds off makes no volume"
    );
    renderer.clouds.enabled = true;
    renderer
        .render_to_pixels(&gpu, 16, 16, &facing_minus_z(20.0))
        .expect("render");
    assert!(noise_volume(&gpu, &renderer).iter().all(|&h| h == QUARTER));
}

/// A card glowing far past bloom's threshold, at `centre` facing the camera.
fn upload_glowing_card(gpu: &GpuContext, renderer: &mut ForwardRenderer, centre: Vec3, half: Vec3) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
    let mut scene = load_gltf(path).expect("cube.gltf must load");
    scene.primitives[0].material.emissive_factor = [40.0; 3];
    renderer.upload_scene(gpu, &scene);
    let placement = Mat4::from_scale_rotation_translation(half * 2.0, Quat::IDENTITY, centre);
    renderer.set_instances(gpu, 0, &[placement]);
}

/// Summed colour levels bloom adds to the pixels outside `rect` (grown by 3 pixels).
fn halo_outside(
    gpu: &GpuContext,
    renderer: &mut ForwardRenderer,
    camera: &OrbitCamera,
    rect: PixelRect,
) -> u64 {
    renderer.bloom_strength = 0.0;
    let plain = render(gpu, renderer, camera);
    renderer.bloom_strength = 0.6;
    let bloomed = render(gpu, renderer, camera);
    (0..PIXELS)
        .filter(|&p| !rect.contains(p, 3.0))
        .flat_map(|p| (0..3).map(move |c| p * 4 + c))
        .map(|i| u64::from(bloomed[i].saturating_sub(plain[i])))
        .sum()
}

/// A glowing card hidden in a dense box: bloom thresholds what the tonemap shows, so no halo spills past the box.
#[test]
fn a_light_the_clouds_hide_does_not_bloom_around_them() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // Far off, so the deep box is barely wider than the card on screen and the card's halo would reach past it.
    let camera = facing_minus_z(60.0);
    let (card_centre, card_half) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(6.0, 6.0, 0.1));
    let (box_centre, box_half) = (Vec3::new(0.0, 0.0, -5.5), Vec3::new(6.6, 6.6, 4.0));
    let rect = PixelRect::of_box(&camera, box_centre, box_half);
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    upload_glowing_card(&gpu, &mut renderer, card_centre, card_half);
    // A dim sun, so the sunlit cloud itself stays below the threshold.
    renderer.light_color_intensity.w = 0.5;
    renderer.clouds = clouds_in_box(box_centre, box_half, 12.0);
    renderer.clouds.enabled = false;
    let clear = halo_outside(&gpu, &mut renderer, &camera, rect);
    renderer.clouds.enabled = true;
    let hidden = halo_outside(&gpu, &mut renderer, &camera, rect);
    eprintln!("bloom outside the box: {clear} levels without clouds, {hidden} with them");
    assert!(
        clear > 5000,
        "the bare card must glow past the box's outline, or nothing is tested"
    );
    assert!(
        hidden * 4 < clear,
        "the clouds hide the card, yet its halo still spills past them: {hidden} of {clear} levels"
    );
}

/// A wall in front of the box: the pixels its left and right silhouettes half cover still show the cloud behind.
#[test]
fn silhouette_pixels_in_front_of_a_cloud_are_clouded() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    let camera = facing_minus_z(20.0);
    let (wall_centre, wall_half) = (Vec3::ZERO, Vec3::new(3.0, 2.0, 0.1));
    upload_box(&gpu, &mut renderer, wall_centre, wall_half);
    renderer.clouds = clouds_in_box(Vec3::new(0.0, 0.0, -10.0), Vec3::new(8.0, 5.0, 3.0), 1.0);
    render(&gpu, &mut renderer, &camera);
    let cloud = renderer
        .read_cloud_target(&gpu)
        .expect("cloud readback must succeed")
        .expect("clouds ran");
    let alpha = |x: usize, y: usize| half_to_f32(cloud[(y * SIZE as usize + x) * 4 + 3]);

    let wall = PixelRect::of_box(&camera, wall_centre, wall_half);
    let (left, right) = (wall.min.x, wall.max.x);
    for edge in [left, right] {
        let share = edge.fract();
        assert!(
            (0.05..0.95).contains(&share),
            "the silhouette at x = {edge} must cut a pixel, not run between two"
        );
    }
    // Each edge column, and the sky column two pixels further out.
    let columns = [
        (left as usize, left as usize - 2),
        (right as usize, right as usize + 2),
    ];
    let rows = (wall.min.y as usize + 2)..(wall.max.y as usize - 2);
    let (mut edge_sum, mut sky_sum, mut bare) = (0.0, 0.0, 0);
    for y in rows.clone() {
        for &(edge, sky) in &columns {
            edge_sum += alpha(edge, y);
            sky_sum += alpha(sky, y);
            bare += usize::from(alpha(edge, y) == 0.0);
        }
    }
    let count = (rows.len() * columns.len()) as f32;
    let (edge_mean, sky_mean) = (edge_sum / count, sky_sum / count);
    eprintln!("cloud alpha on the silhouette {edge_mean:.3}, beside it {sky_mean:.3}, {bare} bare");
    assert!(sky_mean > 0.1, "too little cloud beside the wall to judge");
    assert_eq!(
        bare, 0,
        "silhouette pixels got no cloud: the march stopped at the nearest sample"
    );
    assert!(
        edge_mean > sky_mean * 0.4,
        "the silhouette shows {edge_mean} alpha against {sky_mean} beside it"
    );
}
