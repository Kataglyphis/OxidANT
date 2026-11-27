//! Where the camera starts on a scene: kataglyphis_render_core's framing over the scene's cameras and metadata.

use glam::{Mat4, Vec3};

use super::scene_meta::SceneMeta;
use crate::scene::camera::OrbitCamera;
use crate::scene::{CpuCameraProjection, CpuScene};
// Linked, not copied, in the docs: a copied type carries tracing's dead dispatcher link (docs/ci.md).
#[doc(no_inline)]
pub use kataglyphis_render_core::framing::{Framing, FramingSource, SceneCamera, NEAR_FAR_RATIO};

/// The start view of `scene` whose world bounds are `bounds`; `base` lends a bounds fit its direction and field of view.
pub fn frame(
    scene: &CpuScene,
    meta: &SceneMeta,
    bounds: Option<(Vec3, Vec3)>,
    base: &OrbitCamera,
) -> Framing {
    let world = CpuScene::compute_world_transforms(&scene.nodes);
    let cameras = scene_cameras(scene, &world);
    let active = meta.active_camera_node.and_then(|node| {
        let index = scene
            .cameras
            .iter()
            .position(|camera| camera.node == node)?;
        Some((index, meta.active_camera.clone().unwrap_or_default()))
    });
    kataglyphis_render_core::framing::frame(&cameras, active, bounds, base)
}

/// Each camera at its node's world pose; a camera whose node is missing looks nowhere, so the framing skips it.
fn scene_cameras(scene: &CpuScene, world: &[Mat4]) -> Vec<SceneCamera> {
    scene
        .cameras
        .iter()
        .map(|camera| {
            let pose = world.get(camera.node).copied().unwrap_or(Mat4::NAN);
            SceneCamera {
                eye: pose.transform_point3(Vec3::ZERO),
                forward: pose.transform_vector3(Vec3::NEG_Z),
                yfov_rad: match camera.projection {
                    CpuCameraProjection::Perspective { yfov_rad, .. } => Some(yfov_rad),
                    CpuCameraProjection::Orthographic { .. } => None,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec4};

    use super::*;
    use crate::scene::controller::{OrbitController, DEFAULT_RADIUS_RANGE};
    use crate::scene::{CpuCamera, CpuNode};
    use kataglyphis_render_core::framing::shadow_distance;
    use kataglyphis_render_core::shadow_settings::{
        DEFAULT_SHADOW_DISTANCE, SHADOW_DISTANCE_RANGE,
    };

    fn node(translation: Vec3, rotation: Quat) -> CpuNode {
        CpuNode {
            parent: None,
            translation,
            rotation,
            scale: Vec3::ONE,
        }
    }

    fn camera(name: &str, node: usize) -> CpuCamera {
        CpuCamera {
            name: Some(name.to_owned()),
            node,
            projection: CpuCameraProjection::Perspective {
                yfov_rad: 0.7427,
                znear: 0.1,
                zfar: Some(3000.0),
            },
        }
    }

    /// The Colosseum's shape: an 8 km ground and a 190 m building, its first camera aside and `Kamera_Arena` in the stands.
    fn colosseum() -> (CpuScene, SceneMeta, (Vec3, Vec3)) {
        let arena = Quat::from_xyzw(-0.1795, -0.4802, -0.1011, 0.8526).normalize();
        let scene = CpuScene {
            nodes: vec![
                node(Vec3::new(23.0, 2.2, 1.5), Quat::IDENTITY),
                node(Vec3::new(-58.0, 34.0, 36.0), arena),
            ],
            cameras: vec![camera("Kamera", 0), camera("Kamera_Arena", 1)],
            ..CpuScene::default()
        };
        let meta = SceneMeta {
            active_camera_node: Some(1),
            active_camera: Some("Kamera_Arena".to_owned()),
            ..SceneMeta::default()
        };
        let bounds = (
            Vec3::new(-4000.0, -11.25, -4000.0),
            Vec3::new(4000.0, 58.6, 4000.0),
        );
        (scene, meta, bounds)
    }

    #[test]
    fn a_scene_starts_at_the_camera_its_author_left_active() {
        let (scene, meta, bounds) = colosseum();
        let framing = frame(&scene, &meta, Some(bounds), &OrbitCamera::default());
        assert_eq!(
            framing.source,
            FramingSource::ActiveCamera("Kamera_Arena".into())
        );
        assert!(!framing.auto_orbit());
        let camera = &framing.camera;
        let eye = camera.eye();
        assert!(eye.abs_diff_eq(Vec3::new(-58.0, 34.0, 36.0), 1e-3), "{eye}");
        let forward = (camera.target - eye).normalize();
        assert!(
            forward.abs_diff_eq(Vec3::new(0.783, -0.403, -0.474), 2e-3),
            "{forward}"
        );
        assert!((camera.fov_y_deg - 0.7427_f32.to_degrees()).abs() < 1e-3);
        // The arena floor, about where the view meets the scene's centre, is what an orbit circles.
        assert!(
            camera.target.abs_diff_eq(Vec3::new(-5.8, 7.2, 4.4), 1.0),
            "{}",
            camera.target
        );
        assert!(
            framing.fly_speed > 10.0 && framing.fly_speed < 50.0,
            "{}",
            framing.fly_speed
        );
        let first = frame(
            &scene,
            &SceneMeta::default(),
            Some(bounds),
            &OrbitCamera::default(),
        );
        assert_eq!(first.source, FramingSource::FirstCamera);
        assert!(first
            .camera
            .eye()
            .abs_diff_eq(Vec3::new(23.0, 2.2, 1.5), 1e-3));
    }

    #[test]
    fn an_eight_kilometre_scene_reaches_its_horizon_without_z_fighting_at_the_arena() {
        let (scene, meta, bounds) = colosseum();
        let camera = frame(&scene, &meta, Some(bounds), &OrbitCamera::default()).camera;
        let eye = camera.eye();
        let farthest = (0..8)
            .map(|corner| {
                let pick =
                    |bit: usize, low: f32, high: f32| if corner & bit == 0 { low } else { high };
                Vec3::new(
                    pick(1, bounds.0.x, bounds.1.x),
                    pick(2, bounds.0.y, bounds.1.y),
                    pick(4, bounds.0.z, bounds.1.z),
                )
            })
            .map(|corner| corner.distance(eye))
            .fold(0.0, f32::max);
        assert!(camera.far >= farthest, "far {} < {farthest}", camera.far);
        assert!(camera.far / camera.near <= NEAR_FAR_RATIO * 1.001);
        // Two surfaces 5 cm apart, 150 m into the arena, keep distinct 32-bit depths.
        let view_projection = camera.view_projection(16.0 / 9.0);
        let forward = (camera.target - eye).normalize();
        let depth = |distance: f32| {
            let clip = view_projection * Vec4::from((eye + forward * distance, 1.0));
            clip.z / clip.w
        };
        let (near, far) = (depth(150.0), depth(150.05));
        let ulps = (far.to_bits() as i64 - near.to_bits() as i64).abs();
        assert!(ulps >= 4, "{ulps} ulps between surfaces 5 cm apart");
    }

    #[test]
    fn a_scene_without_a_camera_is_fitted_to_its_bounds() {
        let scene = CpuScene::default();
        let bounds = (Vec3::new(-4.0, 0.0, -4.0), Vec3::new(4.0, 1.4, 4.0));
        let framing = frame(
            &scene,
            &SceneMeta::default(),
            Some(bounds),
            &OrbitCamera::default(),
        );
        assert_eq!(framing.source, FramingSource::Bounds);
        assert!(framing.auto_orbit());
        let camera = &framing.camera;
        assert!(camera.target.abs_diff_eq(Vec3::new(0.0, 0.7, 0.0), 1e-5));
        assert_eq!(
            (camera.yaw_deg, camera.pitch_deg),
            (45.0, 25.0),
            "the base's direction"
        );
        let view_projection = camera.view_projection(1.0);
        for corner in 0..8 {
            let pick = |bit: usize, low: f32, high: f32| if corner & bit == 0 { low } else { high };
            let point = Vec3::new(pick(1, -4.0, 4.0), pick(2, 0.0, 1.4), pick(4, -4.0, 4.0));
            let ndc = view_projection.project_point3(point);
            assert!(
                ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0,
                "corner {point} at {ndc}"
            );
            assert!(
                (0.0..1.0).contains(&ndc.z),
                "corner {point} clipped at depth {}",
                ndc.z
            );
        }
        let point = (Vec3::ONE, Vec3::ONE);
        for bounds in [None, Some(point)] {
            let unbounded = frame(
                &scene,
                &SceneMeta::default(),
                bounds,
                &OrbitCamera::default(),
            );
            assert_eq!(unbounded.source, FramingSource::Unchanged);
            assert_eq!(unbounded.camera.radius, OrbitCamera::default().radius);
        }
    }

    #[test]
    fn a_bounds_fitted_eight_kilometre_scene_zooms_from_its_start_radius() {
        let (mut scene, meta, bounds) = colosseum();
        let arena = frame(&scene, &meta, Some(bounds), &OrbitCamera::default());
        scene.cameras.clear();
        let fitted = frame(
            &scene,
            &SceneMeta::default(),
            Some(bounds),
            &OrbitCamera::default(),
        );
        assert_eq!(fitted.source, FramingSource::Bounds);
        let start = fitted.camera.radius;
        assert!(start > 10_000.0, "{start}");
        // The fixed 0.2 to 500 m range snapped a 15 km fit to 500 m on the first wheel notch.
        let mut snapped = fitted.camera.clone();
        OrbitController::default().apply_zoom(&mut snapped, 1.0);
        assert_eq!(snapped.radius, DEFAULT_RADIUS_RANGE.1);
        for framing in [&fitted, &arena] {
            let mut controller = OrbitController::default();
            controller.radius_range = framing.radius_range;
            let mut camera = framing.camera.clone();
            let start = camera.radius;
            controller.apply_zoom(&mut camera, 1.0);
            assert!(
                (camera.radius / start - 0.9).abs() < 1e-4,
                "{} from {start}",
                camera.radius
            );
            // All the way in the target stays past the near plane; all the way out the scene stays inside the far.
            controller.apply_zoom(&mut camera, 500.0);
            assert_eq!(camera.radius, 2.0 * camera.near);
            controller.apply_zoom(&mut camera, -500.0);
            assert_eq!(camera.radius, 0.5 * camera.far);
            let (centre, half) = (
                (bounds.0 + bounds.1) * 0.5,
                (bounds.1 - bounds.0).length() * 0.5,
            );
            let farthest = camera.target.distance(centre) + camera.radius + half;
            assert!(
                farthest <= camera.far * 1.0001,
                "{farthest} past {}",
                camera.far
            );
        }
        assert!(arena.radius_range.1 > 5_000.0, "{:?}", arena.radius_range);
        let unchanged = frame(&scene, &SceneMeta::default(), None, &OrbitCamera::default());
        assert_eq!(unchanged.radius_range, DEFAULT_RADIUS_RANGE);
    }

    #[test]
    fn the_shadows_reach_twice_the_start_view_never_under_the_cpp_distance() {
        let (scene, meta, bounds) = colosseum();
        let framing = frame(&scene, &meta, Some(bounds), &OrbitCamera::default());
        // From Kamera_Arena about 130 m: the arena and the stands behind it, not 60 m short of its far side.
        let expected = 2.0 * framing.camera.radius;
        assert!(
            (framing.shadow_distance - expected).abs() < 1e-3,
            "{}",
            framing.shadow_distance
        );
        assert!(
            framing.shadow_distance > 120.0,
            "{}",
            framing.shadow_distance
        );
        assert_eq!(shadow_distance(4.0), DEFAULT_SHADOW_DISTANCE);
        assert_eq!(shadow_distance(1e6), *SHADOW_DISTANCE_RANGE.end());
        assert_eq!(shadow_distance(f32::NAN), DEFAULT_SHADOW_DISTANCE);
    }
}
