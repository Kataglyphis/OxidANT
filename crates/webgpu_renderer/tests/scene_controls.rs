//! The C++ inspector's other controls on the GPU: the cube-map skybox and its switch, the model transform,
//! frustum culling and the light colour, as structural pixel and counter checks.

use glam::{Vec3, Vec4};
use kataglyphis_webgpu_renderer::asset::skybox::CubemapFaces;
use kataglyphis_webgpu_renderer::render::forward::SkyMode;
use kataglyphis_webgpu_renderer::render::model_transform::ModelTransform;
use kataglyphis_webgpu_renderer::{load_gltf, CpuScene, ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: u32 = 128;
/// One colour per cube layer, +X, -X, +Y, -Y, +Z, -Z.
const FACES: [[u8; 4]; 6] = [
    [200, 40, 40, 255],
    [40, 200, 40, 255],
    [40, 40, 200, 255],
    [200, 200, 40, 255],
    [200, 40, 200, 255],
    [40, 200, 200, 255],
];

fn cube_on_plane() -> CpuScene {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_on_plane.gltf");
    load_gltf(path).expect("cube_on_plane.gltf must load")
}

fn camera() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 55.0,
        ..OrbitCamera::default()
    }
}

/// No bloom or SSAO, so a pixel is its own shading alone.
fn renderer(gpu: &GpuContext) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.ssao_strength = 0.0;
    renderer.bloom_strength = 0.0;
    renderer
}

fn render(gpu: &GpuContext, renderer: &mut ForwardRenderer, camera: &OrbitCamera) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

fn centre(pixels: &[u8]) -> [u8; 4] {
    let at = ((SIZE / 2) * SIZE + SIZE / 2) as usize * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter()
        .zip(b)
        .take(3)
        .all(|(x, y)| x.abs_diff(y) <= tolerance)
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

#[test]
fn the_cube_map_shows_each_face_along_its_axis_untonemapped() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    assert_eq!(renderer.sky_mode(), SkyMode::Procedural);
    // 4x4 faces: a view a degree off a face's centre then still filters that face alone.
    let faces = CubemapFaces {
        size: 4,
        rgba8: FACES.iter().flat_map(|colour| colour.repeat(16)).collect(),
    };
    renderer.set_skybox(&gpu, &faces);
    assert_eq!(renderer.sky_mode(), SkyMode::Cubemap);
    // +X, -X, +Y, -Y, +Z, -Z: the eye sits opposite the view direction.
    let views = [
        (180.0, 0.0),
        (0.0, 0.0),
        (0.0, -89.0),
        (0.0, 89.0),
        (270.0, 0.0),
        (90.0, 0.0),
    ];
    for (layer, (yaw, pitch)) in views.into_iter().enumerate() {
        let shown = centre(&render(&gpu, &mut renderer, &looking(yaw, pitch)));
        assert!(
            near(shown, FACES[layer], 2),
            "looking along cube layer {layer} shows {shown:?}, not its colour {:?}",
            FACES[layer]
        );
    }
}

#[test]
fn the_switch_shows_the_cpp_navy_and_leaves_the_geometry_alone() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.upload_scene(&gpu, &cube_on_plane());
    renderer.set_skybox(&gpu, &CubemapFaces::solid(FACES));
    // Level with the plane, so the top rows are sky and the centre is the cube.
    let side = OrbitCamera {
        radius: 7.0,
        pitch_deg: 5.0,
        target: Vec3::new(0.0, 0.9, 0.0),
        ..OrbitCamera::default()
    };
    let shown = render(&gpu, &mut renderer, &side);
    renderer.skybox_enabled = false;
    assert_eq!(renderer.sky_mode(), SkyMode::Off);
    let off = render(&gpu, &mut renderer, &side);
    let top = |pixels: &[u8]| [pixels[0], pixels[1], pixels[2], pixels[3]];
    assert!(near(top(&off), [0, 0, 188, 255], 2), "{:?}", top(&off));
    assert!(
        !near(top(&shown), top(&off), 20),
        "the cube map showed {:?}",
        top(&shown)
    );
    assert_eq!(
        centre(&shown),
        centre(&off),
        "the cube in the middle must not change"
    );
    renderer.skybox_enabled = true;
    renderer.clear_skybox(&gpu);
    let procedural = render(&gpu, &mut renderer, &side);
    assert!(
        !near(top(&procedural), top(&off), 20),
        "{:?}",
        top(&procedural)
    );
    assert!(
        !near(top(&procedural), top(&shown), 20),
        "{:?}",
        top(&procedural)
    );
}

/// The centroid of the pixels `pick` keeps, or `None` when none does.
fn centroid(pixels: &[u8], pick: impl Fn(&[u8; 4]) -> bool) -> Option<(f32, f32)> {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if pick(p) {
            sx += (i as u32 % SIZE) as f32;
            sy += (i as u32 / SIZE) as f32;
            n += 1.0;
        }
    }
    (n > 50.0).then(|| (sx / n, sy / n))
}

/// Pixels the shadows darken by over 40 levels: the frame with shadows on against the same one off.
fn shadow_mask(gpu: &GpuContext, renderer: &mut ForwardRenderer) -> Vec<u8> {
    let shadowed = render(gpu, renderer, &camera());
    renderer.shadows.enabled = false;
    let lit = render(gpu, renderer, &camera());
    renderer.shadows.enabled = true;
    let luma =
        |p: &[u8; 4]| 0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]);
    let lit = lit
        .as_chunks::<4>()
        .0
        .iter()
        .map(luma)
        .collect::<Vec<f32>>();
    shadowed
        .as_chunks::<4>()
        .0
        .iter()
        .zip(lit)
        .flat_map(|(p, l)| if l - luma(p) > 40.0 { [255; 4] } else { [0; 4] })
        .collect()
}

#[test]
fn the_model_transform_moves_the_model_and_its_shadow_and_a_load_resets_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.upload_scene(&gpu, &cube_on_plane());
    renderer.light_dir_ambient = Vec4::new(-1.0, 0.7, -0.3, 0.15);
    renderer.shadows.strength = 1.0;
    let red = |p: &[u8; 4]| i32::from(p[0]) - i32::from(p[1]) > 60;
    let marked = |p: &[u8; 4]| p[0] == 255;
    let before = render(&gpu, &mut renderer, &camera());
    let shadow_before = shadow_mask(&gpu, &mut renderer);
    let bounds = renderer
        .scene_bounds()
        .expect("an uploaded scene has bounds");
    // A quarter turn maps the square scene onto its own bounds, so only the translation shows there.
    renderer.model_transform = ModelTransform {
        position: Vec3::new(0.0, 0.0, 1.0),
        rotation_deg: Vec3::new(0.0, 90.0, 0.0),
    };
    let moved = render(&gpu, &mut renderer, &camera());
    let shadow_moved = shadow_mask(&gpu, &mut renderer);
    let moved_bounds = renderer.scene_bounds().expect("bounds");
    assert!(
        (moved_bounds.0.z - (bounds.0.z + 1.0)).abs() < 1e-4,
        "{moved_bounds:?}"
    );
    let shift = |a: &[u8], b: &[u8], pick: &dyn Fn(&[u8; 4]) -> bool| {
        let (ax, ay) = centroid(a, pick).expect("before");
        let (bx, by) = centroid(b, pick).expect("after");
        (bx - ax, by - ay)
    };
    let cube = shift(&before, &moved, &red);
    let shadow = shift(&shadow_before, &shadow_moved, &marked);
    eprintln!("cube moved by {cube:?} pixels, its shadow by {shadow:?}");
    assert!(
        cube.0.hypot(cube.1) > 4.0,
        "the cube did not move: {cube:?}"
    );
    // Perspective shifts the plane and the cube a little differently; a shadow left behind does not shift at all.
    let lag = (shadow.0 - cube.0).hypot(shadow.1 - cube.1);
    assert!(
        lag < 0.4 * cube.0.hypot(cube.1),
        "the shadow stayed: {shadow:?}"
    );
    renderer.model_transform = ModelTransform::default();
    assert_eq!(
        render(&gpu, &mut renderer, &camera()),
        before,
        "Reset transform restores the frame"
    );
    renderer.model_transform.position = Vec3::splat(3.0);
    renderer.upload_scene(&gpu, &cube_on_plane());
    assert_eq!(
        renderer.model_transform,
        ModelTransform::default(),
        "a load resets it"
    );
}

#[test]
fn frustum_culling_drops_offscreen_primitives_only_and_switches_off() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.upload_scene(&gpu, &cube_on_plane());
    render(&gpu, &mut renderer, &camera());
    assert_eq!(
        renderer.primitive_stats(),
        (2, 2),
        "the camera looks at both"
    );
    // Far past the scene, looking away from it.
    let away = OrbitCamera {
        target: Vec3::new(0.0, 0.0, 5000.0),
        yaw_deg: 270.0,
        ..camera()
    };
    render(&gpu, &mut renderer, &away);
    assert_eq!(renderer.primitive_stats(), (0, 2));
    renderer.frustum_culling = false;
    render(&gpu, &mut renderer, &away);
    assert_eq!(renderer.primitive_stats(), (2, 2), "off draws everything");
}

#[test]
fn the_light_colour_tints_the_lit_plane() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu);
    renderer.upload_scene(&gpu, &cube_on_plane());
    renderer.light_dir_ambient = Vec4::new(0.2, 1.0, 0.1, 0.05);
    let corner = |pixels: &[u8]| {
        let at = ((SIZE - 8) * SIZE + 8) as usize * 4;
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    };
    let white = corner(&render(&gpu, &mut renderer, &camera()));
    renderer.light_color_intensity = Vec4::new(1.0, 0.1, 0.1, 5.0);
    let red = corner(&render(&gpu, &mut renderer, &camera()));
    eprintln!("lit plane under a white light {white:?}, under a red one {red:?}");
    assert!(white[0].abs_diff(white[1]) < 25, "{white:?}");
    assert!(red[0] > red[1] + 60 && red[0] > red[2] + 60, "{red:?}");
}
