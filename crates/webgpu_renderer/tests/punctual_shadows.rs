//! Point and spot light cube shadows on a real adapter, the C++ `PunctualLightShadowsTheCardOntoThePlane` goldens' twin:
//! a card between a point light and the ground darkens it, the light shadows switch takes that away, and nothing else dims.

use glam::{Mat4, Vec2, Vec3};
use kataglyphis_webgpu_renderer::render::forward::RasterPath;
use kataglyphis_webgpu_renderer::render::ibl::half_to_f32;
use kataglyphis_webgpu_renderer::scene::{
    CpuLight, CpuLightKind, CpuMaterial, CpuPrimitive, CpuScene, Vertex,
};
use kataglyphis_webgpu_renderer::{ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: u32 = 128;

fn vertex(position: Vec3, normal: Vec3) -> Vertex {
    Vertex {
        position: position.to_array(),
        normal: normal.to_array(),
        uv: [0.0, 0.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0.0; 4],
        weights: [0.0; 4],
        color: [1.0; 4],
        uv1: [0.0, 0.0],
    }
}

/// A rough white quad from four counter-clockwise corners, its normal theirs.
fn quad(corners: [Vec3; 4]) -> CpuPrimitive {
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize();
    CpuPrimitive {
        vertices: corners.iter().map(|&c| vertex(c, normal)).collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            metallic_factor: 0.0,
            roughness_factor: 1.0,
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// An upward quad of half-width `half` around `centre`.
fn flat(centre: Vec3, half: f32) -> CpuPrimitive {
    quad([
        centre + Vec3::new(-half, 0.0, -half),
        centre + Vec3::new(-half, 0.0, half),
        centre + Vec3::new(half, 0.0, half),
        centre + Vec3::new(half, 0.0, -half),
    ])
}

fn light(kind: CpuLightKind, position: Vec3, intensity: f32, range: f32) -> CpuLight {
    CpuLight {
        kind,
        color: [1.0; 3],
        intensity,
        range,
        position: position.to_array(),
        direction: [0.0, -1.0, 0.0],
        node: None,
    }
}

/// The light 2 above the ground at the origin; the card halfway down throws a shadow twice its offset and size.
/// Off both axes, so a face's flipped uv would put the shadow where the test finds light.
const CARD: Vec3 = Vec3::new(-0.6, 1.0, 0.4);
const SHADOW: Vec2 = Vec2::new(-1.2, 0.8);
/// The shadow mirrored through the light: the same irradiance, nothing between.
const MIRROR: Vec2 = Vec2::new(1.2, -0.8);

/// An unbounded point light over the card and the ground, and a ranged spot light whose pool nothing occludes.
fn scene() -> CpuScene {
    let spot = CpuLightKind::Spot {
        cos_inner: 0.95,
        cos_outer: 0.85,
    };
    CpuScene {
        primitives: vec![flat(Vec3::ZERO, 10.0), flat(CARD, 0.2)],
        lights: vec![
            light(CpuLightKind::Point, Vec3::new(0.0, 2.0, 0.0), 24.0, 0.0),
            light(spot, Vec3::new(1.5, 2.0, 2.5), 24.0, 6.0),
        ],
        ..CpuScene::default()
    }
}

/// Straight down from 10 up: the frame shows (x, z) within about 4 of the origin.
fn from_above() -> OrbitCamera {
    OrbitCamera {
        radius: 10.0,
        pitch_deg: 89.0,
        yaw_deg: 90.0,
        ..OrbitCamera::default()
    }
}

/// Where each pixel centre's camera ray meets the ground (x, z).
fn ground(camera: &OrbitCamera) -> Vec<Vec2> {
    let inverse = camera.view_projection(1.0).inverse();
    (0..SIZE * SIZE)
        .map(|i| {
            let (px, py) = ((i % SIZE) as f32 + 0.5, (i / SIZE) as f32 + 0.5);
            let ndc = Vec2::new(px / SIZE as f32 * 2.0 - 1.0, 1.0 - py / SIZE as f32 * 2.0);
            let near = inverse.project_point3(ndc.extend(0.0));
            let far = inverse.project_point3(ndc.extend(1.0));
            let hit = near + (far - near) * (near.y / (near.y - far.y));
            Vec2::new(hit.x, hit.z)
        })
        .collect()
}

/// Lit by the punctual lights alone: no sun, no ambient, no post.
fn renderer(gpu: &GpuContext, path: RasterPath) -> ForwardRenderer {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(gpu, &scene());
    renderer.raster_path = path;
    renderer.bloom_strength = 0.0;
    renderer.ssao_strength = 0.0;
    renderer.light_color_intensity.w = 0.0;
    renderer.light_dir_ambient.w = 0.0;
    renderer
}

/// The red HDR radiance of a frame with the light shadows `on`.
fn radiance(gpu: &GpuContext, renderer: &mut ForwardRenderer, on: bool) -> Vec<f32> {
    renderer.shadows.punctual = on;
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, &from_above())
        .expect("renders");
    let bits = renderer.read_hdr_target(gpu).expect("reads back");
    bits.as_chunks::<4>()
        .0
        .iter()
        .map(|p| half_to_f32(p[0]))
        .collect()
}

fn mean_near(frame: &[f32], ground: &[Vec2], centre: Vec2) -> f32 {
    let patch: Vec<f32> = (0..ground.len())
        .filter(|&i| ground[i].distance(centre) < 0.25)
        .map(|i| frame[i])
        .collect();
    assert!(!patch.is_empty(), "no pixel sees {centre}");
    patch.iter().sum::<f32>() / patch.len() as f32
}

fn expect_the_card_shadows_the_ground(path: RasterPath) {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, path);
    let off = radiance(&gpu, &mut renderer, false);
    assert_eq!(renderer.punctual_shadow_cubes(), 0, "the switch draws none");
    let on = radiance(&gpu, &mut renderer, true);
    assert_eq!(
        renderer.punctual_shadow_cubes(),
        2,
        "the point and the spot light each get a cube"
    );
    let ground = ground(&from_above());
    let (shadow_on, shadow_off) = (
        mean_near(&on, &ground, SHADOW),
        mean_near(&off, &ground, SHADOW),
    );
    let (mirror_on, mirror_off) = (
        mean_near(&on, &ground, MIRROR),
        mean_near(&off, &ground, MIRROR),
    );
    eprintln!(
        "{path:?}: under the card {shadow_on:.4} (off {shadow_off:.4}), its mirror {mirror_on:.4} (off {mirror_off:.4})"
    );
    assert!(
        shadow_off > 0.5 * mirror_off,
        "without shadows the card's footprint is lit like its mirror: {shadow_off} against {mirror_off}"
    );
    assert!(
        shadow_on < 0.15 * shadow_off,
        "the card casts no shadow from the point light: {shadow_on} against {shadow_off}"
    );
    // Outside the quadrant of the card and its shadow every pixel keeps its light, the spot's pool included: no acne.
    let open: Vec<usize> = (0..ground.len())
        .filter(|&i| !(ground[i].x < 0.0 && ground[i].y > 0.0) && off[i] > 0.01)
        .collect();
    let dimmed = open.iter().filter(|&&i| on[i] < 0.97 * off[i]).count();
    eprintln!("{path:?}: {dimmed} of {} open pixels dimmed", open.len());
    assert!(open.len() > 1000, "too little of the ground is lit");
    assert!(
        dimmed * 200 < open.len(),
        "{dimmed} of {} unoccluded pixels shadow themselves (acne)",
        open.len()
    );
}

#[test]
fn a_card_shadows_the_point_light_onto_the_ground_in_forward() {
    expect_the_card_shadows_the_ground(RasterPath::Forward);
}

#[test]
fn a_card_shadows_the_point_light_onto_the_ground_in_deferred() {
    expect_the_card_shadows_the_ground(RasterPath::Deferred);
}

/// Each face covers its axis: the card straight below the point light lands on -Y, where the shader looks for it.
#[test]
fn the_card_lands_on_the_down_face_where_the_shader_samples_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut renderer = renderer(&gpu, RasterPath::Forward);
    radiance(&gpu, &mut renderer, true);
    let depth = renderer.read_punctual_shadow_map(&gpu).expect("reads back");
    let layer = depth.len() / 24;
    let side = (layer as f64).sqrt() as usize;
    let face = |index: usize| &depth[index * layer..(index + 1) * layer];
    let covered = |index: usize| face(index).iter().filter(|d| **d < 1.0).count();
    let faces: Vec<usize> = (0..6).map(covered).collect();
    eprintln!("texels nearer than the far plane per face of cube 0: {faces:?}");
    assert_eq!(faces[2], 0, "nothing lies above the light");
    assert_eq!(faces[3], layer, "the ground fills the down face");
    // On -Y, NDC is (x, -z) over the depth below; uv flips NDC y as the shader's punctual_shadow_tap does.
    let texel = |offset: Vec2| {
        let ndc = Vec2::new(offset.x, -offset.y) / (CARD.y - 2.0).abs();
        let (u, v) = (ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        face(3)[(v * side as f32) as usize * side + (u * side as f32) as usize]
    };
    let card = Vec2::new(CARD.x, CARD.z);
    let (on_card, flipped, mirrored) =
        (texel(card), texel(Vec2::new(card.x, -card.y)), texel(-card));
    eprintln!("-Y depth: card {on_card}, its z mirror {flipped}, its mirror {mirrored}");
    assert!(
        on_card < flipped && on_card < mirrored,
        "the card is not where the shader samples it on -Y"
    );
}
