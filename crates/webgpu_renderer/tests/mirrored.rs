//! Mirroring transforms (negative determinant) and back faces shade like their unmirrored originals.

use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};
use kataglyphis_webgpu_renderer::scene::{
    CpuMaterial, CpuPrimitive, CpuSampler, CpuScene, CpuTexture, CpuTextureRef, Vertex,
};
use kataglyphis_webgpu_renderer::{load_gltf, ForwardRenderer, GpuContext, OrbitCamera};

const SIZE: u32 = 128;

fn cube() -> CpuPrimitive {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tests/assets/cube.gltf");
    let mut prim = load_gltf(path).expect("cube.gltf must load").primitives[0].clone();
    prim.node_index = None;
    prim
}

/// Renders `primitives` with each optional instance list, from `camera` under `light`.
fn render(
    gpu: &GpuContext,
    primitives: Vec<CpuPrimitive>,
    instances: &[(usize, Vec<Mat4>)],
    camera: &OrbitCamera,
    light: Vec4,
) -> Vec<u8> {
    let mut renderer = ForwardRenderer::new(gpu, SIZE, SIZE);
    renderer.upload_scene(
        gpu,
        &CpuScene {
            primitives,
            ..CpuScene::default()
        },
    );
    for (primitive, transforms) in instances {
        renderer.set_instances(gpu, *primitive, transforms);
    }
    renderer.light_dir_ambient = light;
    renderer
        .render_to_pixels(gpu, SIZE, SIZE, camera)
        .expect("headless render must succeed")
}

/// Pixels whose summed channel difference exceeds dither and edge noise.
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| {
            let d: i32 = p
                .iter()
                .zip(q.iter())
                .map(|(x, y)| (*x as i32 - *y as i32).abs())
                .sum();
            d > 24
        })
        .count()
}

/// Red-dominant pixels: the cube against the blue sky.
fn cube_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] as i32 > p[2] as i32 + 20)
        .count()
}

/// A cube is symmetric under x -> -x, so mirrored it must render exactly like itself.
#[test]
fn a_mirrored_cube_renders_like_the_original() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let camera = OrbitCamera::default();
    let light = Vec4::new(-0.5, 0.8, 0.3, 0.35);
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    for double_sided in [false, true] {
        let mut original = cube();
        original.material.double_sided = double_sided;
        let reference = render(&gpu, vec![original.clone()], &[], &camera, light);
        assert!(
            cube_pixels(&reference) > 1000,
            "precondition: the reference cube must render"
        );

        let mut node_mirrored = original.clone();
        node_mirrored.transform = mirror * original.transform;
        let by_node = render(&gpu, vec![node_mirrored], &[], &camera, light);
        let by_instance = render(&gpu, vec![original], &[(0, vec![mirror])], &camera, light);
        for (label, pixels) in [("node", &by_node), ("instance", &by_instance)] {
            let diff = differing(&reference, pixels);
            assert!(
                diff < 40,
                "a {label}-mirrored cube (double-sided: {double_sided}) differs from the \
                 original in {diff} pixels: wrong faces culled or normals flipped"
            );
        }
    }
}

/// Green-dominant pixels left and right of the frame's middle column.
fn green_halves(pixels: &[u8]) -> (usize, usize) {
    let mut halves = (0, 0);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let green = i32::from(p[1]);
        if green > i32::from(p[0]) + 40 && green > i32::from(p[2]) + 40 {
            if (i as u32 % SIZE) < SIZE / 2 {
                halves.0 += 1;
            } else {
                halves.1 += 1;
            }
        }
    }
    halves
}

/// Two single-sided cards facing the camera, the left one placed by a joint that mirrors X: both show their fronts.
#[test]
fn a_skin_mirrored_by_its_joints_draws_both_cards_from_the_front() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/tests/assets/skinned_mirrored.gltf");
    let scene = load_gltf(path).expect("skinned_mirrored.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, SIZE, SIZE);
    renderer.upload_scene(&gpu, &scene);
    let camera = OrbitCamera {
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    let pixels = renderer
        .render_to_pixels(&gpu, SIZE, SIZE, &camera)
        .expect("headless render must succeed");
    let (mirrored, upright) = green_halves(&pixels);
    assert!(
        upright > 1000,
        "precondition: the upright card draws ({upright} pixels)"
    );
    assert!(
        mirrored * 10 > upright * 9,
        "the mirrored card shows {mirrored} pixels against the upright one's {upright}: culled as a back face"
    );
}

/// One draw holding mirrored and unmirrored instances renders each with its own winding.
#[test]
fn mixed_mirrored_and_upright_instances_render_alike() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let camera = OrbitCamera {
        radius: 9.0,
        ..OrbitCamera::default()
    };
    let light = Vec4::new(-0.5, 0.8, 0.3, 0.35);
    let left = Mat4::from_translation(Vec3::new(-1.5, 0.0, 0.0));
    let right = Mat4::from_translation(Vec3::new(1.5, 0.0, 0.0));
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));

    let upright = render(
        &gpu,
        vec![cube()],
        &[(0, vec![left, right])],
        &camera,
        light,
    );
    let mixed = render(
        &gpu,
        vec![cube()],
        &[(0, vec![right * mirror, left])],
        &camera,
        light,
    );
    let diff = differing(&upright, &mixed);
    assert!(
        diff < 40,
        "mixed mirrored and upright instances differ from all-upright ones in {diff} pixels"
    );
}

/// A unit quad in the XY plane facing +Z, its tangent +X and bitangent +Y, with one normal map texel.
fn normal_mapped_quad(normal_texel: [u8; 4], double_sided: bool) -> CpuPrimitive {
    let vertices = [[-1.0f32, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .iter()
        .map(|[x, y]| Vertex {
            position: [*x, *y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [(x + 1.0) * 0.5, (1.0 - y) * 0.5],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0.0; 4],
            weights: [0.0; 4],
            color: [1.0; 4],
            uv1: [0.0, 0.0],
        })
        .collect();
    let normal_map = CpuTextureRef {
        texture: Arc::new(CpuTexture {
            width: 1,
            height: 1,
            rgba8: normal_texel.to_vec(),
            compressed: None,
        }),
        sampler: CpuSampler::default(),
        srgb: false,
    };
    CpuPrimitive {
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
        transform: Mat4::IDENTITY,
        node_index: None,
        skin_index: None,
        material: CpuMaterial {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.9,
            double_sided,
            normal_texture: Some(normal_map),
            ..CpuMaterial::default()
        },
        morph_targets: Vec::new(),
        morph_weights: Vec::new(),
    }
}

/// Mean luminance of the frame's central quarter, where the quad sits.
fn centre_luma(pixels: &[u8]) -> f64 {
    let (lo, hi) = (SIZE / 4, SIZE * 3 / 4);
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in lo..hi {
        for x in lo..hi {
            let i = ((y * SIZE + x) * 4) as usize;
            sum += 0.2126 * pixels[i] as f64
                + 0.7152 * pixels[i + 1] as f64
                + 0.0722 * pixels[i + 2] as f64;
            n += 1.0;
        }
    }
    sum / n
}

/// Under a mirror the bitangent still follows the surface, so a normal map tilted toward it shades alike.
#[test]
fn a_mirrored_normal_mapped_quad_tilts_its_normals_like_the_original() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // Tilted toward +bitangent (+Y), lit from +Y: the lit side only if the bitangent stays +Y.
    let tilted_up = [128, 230, 166, 255];
    let camera = OrbitCamera {
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    let light = Vec4::new(0.0, 0.8, 0.6, 0.05);
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));

    let reference = render(
        &gpu,
        vec![normal_mapped_quad(tilted_up, false)],
        &[],
        &camera,
        light,
    );
    let mut node_mirrored = normal_mapped_quad(tilted_up, false);
    node_mirrored.transform = mirror;
    let by_node = render(&gpu, vec![node_mirrored], &[], &camera, light);
    let by_instance = render(
        &gpu,
        vec![normal_mapped_quad(tilted_up, false)],
        &[(0, vec![mirror])],
        &camera,
        light,
    );
    let expected = centre_luma(&reference);
    for (label, pixels) in [("node", &by_node), ("instance", &by_instance)] {
        let luma = centre_luma(pixels);
        assert!(
            (luma - expected).abs() < expected * 0.05,
            "a {label}-mirrored normal-mapped quad has luma {luma:.1} where the original has \
             {expected:.1}: its tangent frame lost its handedness"
        );
    }
}

/// Seen from behind, a double-sided normal-mapped sheet is the front reflected through its origin.
#[test]
fn a_double_sided_back_face_flips_its_whole_tangent_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // Tilted toward +tangent (+X): the front normal is (0.6, 0, 0.8), the back one its negation.
    let tilted_along_tangent = [204, 128, 230, 255];
    let front_camera = OrbitCamera {
        yaw_deg: 90.0,
        pitch_deg: 0.0,
        ..OrbitCamera::default()
    };
    let back_camera = OrbitCamera {
        yaw_deg: -90.0,
        ..front_camera.clone()
    };
    let front = render(
        &gpu,
        vec![normal_mapped_quad(tilted_along_tangent, true)],
        &[],
        &front_camera,
        Vec4::new(0.6, 0.0, 0.8, 0.05),
    );
    let back = render(
        &gpu,
        vec![normal_mapped_quad(tilted_along_tangent, true)],
        &[],
        &back_camera,
        Vec4::new(-0.6, 0.0, -0.8, 0.05),
    );
    let (front_luma, back_luma) = (centre_luma(&front), centre_luma(&back));
    assert!(
        (front_luma - back_luma).abs() < front_luma * 0.05,
        "the back face (luma {back_luma:.1}) must shade like the front (luma {front_luma:.1}) \
         under the point-reflected light and camera: its tangent stayed unflipped"
    );
}
