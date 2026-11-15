//! Where the camera starts on a scene: the camera its author left active, else its first camera, else its bounds.

use glam::{Mat4, Vec3};

use super::scene_meta::SceneMeta;
use crate::render::shadow_settings::{DEFAULT_SHADOW_DISTANCE, SHADOW_DISTANCE_RANGE};
use crate::scene::camera::OrbitCamera;
use crate::scene::fly;
use crate::scene::{CpuCamera, CpuCameraProjection, CpuScene};

/// Far over near: more and a 32-bit depth buffer z-fights at an 8 km scene's arena.
pub const NEAR_FAR_RATIO: f32 = 50_000.0;
/// The far plane's room beyond the scene's farthest point, so zooming out does not clip it at once.
const FAR_MARGIN: f32 = 2.0;
/// The bounds fit's room around the scene.
const FIT_MARGIN: f32 = 1.1;
/// The field-of-view slider's range, which an authored camera is held to.
const FOV_RANGE_DEG: (f32, f32) = (20.0, 110.0);
/// At most half the start view's distance to its target per second, so a close start view flies slowly.
const FLY_SPEED_PER_VIEW_DISTANCE: f32 = 0.5;
/// Shadows reach twice the start view's distance to its target, so the subject and what lies behind it are shadowed.
const SHADOW_DISTANCE_PER_VIEW_DISTANCE: f32 = 2.0;

/// Which framing a scene got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FramingSource {
    /// `scenes[scene].extras.active_camera`.
    ActiveCamera(String),
    /// The first camera the file authors.
    FirstCamera,
    Bounds,
    /// No bounds: the shell's own camera.
    Unchanged,
}

/// A scene's start view and the speed the fly camera crosses it at.
#[derive(Clone, Debug)]
pub struct Framing {
    pub camera: OrbitCamera,
    /// World units per second.
    pub fly_speed: f32,
    /// The orbit radius the wheel and a pinch keep to.
    pub radius_range: (f32, f32),
    /// How far the shadow cascades reach.
    pub shadow_distance: f32,
    pub source: FramingSource,
}

impl Framing {
    /// An authored camera is a deliberate view, so only a bounds fit may spin until the first drag.
    pub fn auto_orbit(&self) -> bool {
        matches!(
            self.source,
            FramingSource::Bounds | FramingSource::Unchanged
        )
    }
}

/// The start view of `scene` whose world bounds are `bounds`; `base` lends a bounds fit its direction and field of view.
pub fn frame(
    scene: &CpuScene,
    meta: &SceneMeta,
    bounds: Option<(Vec3, Vec3)>,
    base: &OrbitCamera,
) -> Framing {
    let sphere = bounds.and_then(bounding_sphere);
    let bounds = bounds.filter(|_| sphere.is_some());
    let active = meta
        .active_camera_node
        .and_then(|node| scene.cameras.iter().find(|camera| camera.node == node));
    let authored = active
        .map(|camera| (camera, active_source(meta)))
        .or_else(|| {
            scene
                .cameras
                .first()
                .map(|camera| (camera, FramingSource::FirstCamera))
        });
    let from_camera = authored
        .and_then(|(camera, source)| Some((at_camera(scene, camera, sphere, base)?, source)));
    let (mut camera, source) = match (from_camera, bounds) {
        (Some(found), _) => found,
        (None, Some(bounds)) => (fit_bounds(bounds, base), FramingSource::Bounds),
        (None, None) => (base.clone(), FramingSource::Unchanged),
    };
    if let Some((centre, half)) = sphere {
        clip_planes(&mut camera, centre, half);
    }
    let fly_speed = fly_speed(bounds, camera.radius);
    Framing {
        radius_range: radius_range(&camera),
        shadow_distance: shadow_distance(camera.radius),
        camera,
        fly_speed,
        source,
    }
}

fn active_source(meta: &SceneMeta) -> FramingSource {
    FramingSource::ActiveCamera(meta.active_camera.clone().unwrap_or_default())
}

/// The bounds' centre and half diagonal; `None` for empty or non-finite bounds.
fn bounding_sphere((min, max): (Vec3, Vec3)) -> Option<(Vec3, f32)> {
    let half = (max - min).length() * 0.5;
    (min.is_finite() && max.is_finite() && min.cmple(max).all() && half > 0.0)
        .then(|| ((min + max) * 0.5, half))
}

/// The orbit camera at `camera`'s pose: its eye, its view direction, a target ahead of it at the scene's depth.
fn at_camera(
    scene: &CpuScene,
    camera: &CpuCamera,
    sphere: Option<(Vec3, f32)>,
    base: &OrbitCamera,
) -> Option<OrbitCamera> {
    let world: Mat4 = *CpuScene::compute_world_transforms(&scene.nodes).get(camera.node)?;
    let eye = world.transform_point3(Vec3::ZERO);
    let forward = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
    if !eye.is_finite() || forward == Vec3::ZERO {
        return None;
    }
    let centre = sphere.map_or(eye + forward, |(centre, _)| centre);
    let ahead = (centre - eye).dot(forward);
    // A camera that looks away from the scene's centre orbits a point as far ahead as the centre is away.
    let distance = if ahead > 1e-3 {
        ahead
    } else {
        (centre - eye).length().max(1.0)
    };
    let toward_eye = -forward;
    let fov_y_deg = match camera.projection {
        CpuCameraProjection::Perspective { yfov_rad, .. } if yfov_rad.is_finite() => yfov_rad
            .to_degrees()
            .clamp(FOV_RANGE_DEG.0, FOV_RANGE_DEG.1),
        _ => base.fov_y_deg,
    };
    Some(OrbitCamera {
        target: eye + forward * distance,
        radius: distance,
        yaw_deg: toward_eye.z.atan2(toward_eye.x).to_degrees(),
        pitch_deg: toward_eye.y.clamp(-1.0, 1.0).asin().to_degrees(),
        fov_y_deg,
        ..base.clone()
    })
}

/// `base` turned on the bounds' centre and moved back until every corner is in view, the narrower axis vertical.
fn fit_bounds((min, max): (Vec3, Vec3), base: &OrbitCamera) -> OrbitCamera {
    let centre = (min + max) * 0.5;
    let probe = OrbitCamera {
        target: Vec3::ZERO,
        radius: 1.0,
        ..base.clone()
    };
    let toward_eye = probe.eye().normalize_or_zero();
    let view = probe.view();
    let tan_half = (base.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
    let mut radius: f32 = 0.0;
    for corner in 0..8 {
        let pick = |bit: usize, low: f32, high: f32| if corner & bit == 0 { low } else { high };
        let offset = Vec3::new(
            pick(1, min.x, max.x),
            pick(2, min.y, max.y),
            pick(4, min.z, max.z),
        ) - centre;
        let in_view = view.transform_vector3(offset);
        let spread = in_view.x.abs().max(in_view.y.abs());
        radius = radius.max(offset.dot(toward_eye) + spread / tan_half);
    }
    OrbitCamera {
        target: centre,
        radius: (radius * FIT_MARGIN).max(1e-3),
        ..base.clone()
    }
}

/// Far past the scene's farthest point, near a fixed ratio closer, so depth precision holds at every scale.
fn clip_planes(camera: &mut OrbitCamera, centre: Vec3, half: f32) {
    let reach = (camera.eye() - centre).length() + half;
    camera.far = (reach * FAR_MARGIN).max(1e-2);
    camera.near = camera.far / NEAR_FAR_RATIO;
}

/// From twice the near plane to half the far, the default camera's 0.2 to 500 m, always holding the start radius.
fn radius_range(camera: &OrbitCamera) -> (f32, f32) {
    let low = (camera.near * 2.0).min(camera.radius);
    let high = (camera.far * 0.5).max(camera.radius);
    (low, high)
}

/// Never under the C++ 60 m its scenes were tuned for, more for a scene viewed from farther away.
fn shadow_distance(view_distance: f32) -> f32 {
    let fitted = view_distance * SHADOW_DISTANCE_PER_VIEW_DISTANCE;
    let distance = if fitted.is_finite() {
        fitted.max(DEFAULT_SHADOW_DISTANCE)
    } else {
        DEFAULT_SHADOW_DISTANCE
    };
    distance.clamp(*SHADOW_DISTANCE_RANGE.start(), *SHADOW_DISTANCE_RANGE.end())
}

/// The scene's own speed, [`fly::speed_for_bounds`], at most half the start view's distance per second.
fn fly_speed(bounds: Option<(Vec3, Vec3)>, view_distance: f32) -> f32 {
    let scene = fly::speed_for_bounds(bounds);
    let close = view_distance * FLY_SPEED_PER_VIEW_DISTANCE;
    let speed = if close.is_finite() && close > 0.0 {
        scene.min(close)
    } else {
        scene
    };
    speed.clamp(*fly::SPEED_RANGE.start(), *fly::SPEED_RANGE.end())
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec4};

    use super::*;
    use crate::scene::controller::{OrbitController, DEFAULT_RADIUS_RANGE};
    use crate::scene::CpuNode;

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
