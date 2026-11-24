//! Where the camera starts on a scene: the camera its author left active, else its first camera, else its bounds.
//! Both renderers frame through here; the C++ engine reaches [`frame_for_cpp`] through the bridge.

use glam::Vec3;

use crate::fly_speed;
use crate::orbit_camera::OrbitCamera;
use crate::shadow_settings::{DEFAULT_SHADOW_DISTANCE, SHADOW_DISTANCE_RANGE};

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
/// The C++ controller's pitch clamp.
const CPP_PITCH_LIMIT_DEG: f32 = 89.0;

/// A camera the scene authors, posed in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneCamera {
    pub eye: Vec3,
    /// The way it looks: its node's -Z.
    pub forward: Vec3,
    /// `None` for an orthographic camera, which keeps the base's field of view.
    pub yfov_rad: Option<f32>,
}

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

/// The start view of a scene with `cameras` and world `bounds`; `active` names a camera by index and by its name.
pub fn frame(
    cameras: &[SceneCamera],
    active: Option<(usize, String)>,
    bounds: Option<(Vec3, Vec3)>,
    base: &OrbitCamera,
) -> Framing {
    let sphere = bounds.and_then(bounding_sphere);
    let bounds = bounds.filter(|_| sphere.is_some());
    let (mut camera, source) = match (from_cameras(cameras, active, sphere, base), bounds) {
        (Some(found), _) => found,
        (None, Some(bounds)) => (fit_bounds(bounds, base), FramingSource::Bounds),
        (None, None) => (base.clone(), FramingSource::Unchanged),
    };
    if let Some((centre, half)) = sphere {
        clip_planes(&mut camera, centre, half);
    }
    finish(camera, bounds, source)
}

fn finish(camera: OrbitCamera, bounds: Option<(Vec3, Vec3)>, source: FramingSource) -> Framing {
    Framing {
        fly_speed: fly_speed(bounds, camera.radius),
        radius_range: radius_range(&camera),
        shadow_distance: shadow_distance(camera.radius),
        camera,
        source,
    }
}

/// The active camera, else the first one, as an orbit camera; `None` when neither has a usable pose.
fn from_cameras(
    cameras: &[SceneCamera],
    active: Option<(usize, String)>,
    sphere: Option<(Vec3, f32)>,
    base: &OrbitCamera,
) -> Option<(OrbitCamera, FramingSource)> {
    let active = active.and_then(|(index, name)| {
        cameras
            .get(index)
            .map(|camera| (camera, FramingSource::ActiveCamera(name)))
    });
    let authored = active.or_else(|| {
        cameras
            .first()
            .map(|camera| (camera, FramingSource::FirstCamera))
    })?;
    let (camera, source) = authored;
    Some((at_camera(camera, sphere, base)?, source))
}

/// The bounds' centre and half diagonal; `None` for empty or non-finite bounds.
fn bounding_sphere((min, max): (Vec3, Vec3)) -> Option<(Vec3, f32)> {
    let half = (max - min).length() * 0.5;
    (min.is_finite() && max.is_finite() && min.cmple(max).all() && half > 0.0)
        .then(|| ((min + max) * 0.5, half))
}

/// The orbit camera at `camera`'s pose: its eye, its view direction, a target ahead of it at the scene's depth.
fn at_camera(
    camera: &SceneCamera,
    sphere: Option<(Vec3, f32)>,
    base: &OrbitCamera,
) -> Option<OrbitCamera> {
    let eye = camera.eye;
    let forward = camera.forward.normalize_or_zero();
    if !eye.is_finite() || !forward.is_finite() || forward == Vec3::ZERO {
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
    let fov_y_deg = match camera.yfov_rad {
        Some(yfov_rad) if yfov_rad.is_finite() => yfov_rad
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
    camera.far = needed_far(camera.eye(), centre, half);
    camera.near = camera.far / NEAR_FAR_RATIO;
}

/// The far plane an eye at `eye` needs to see the whole sphere, with room to zoom out.
fn needed_far(eye: Vec3, centre: Vec3, half: f32) -> f32 {
    let reach = (eye - centre).length() + half;
    (reach * FAR_MARGIN).max(1e-2)
}

/// From twice the near plane to half the far, the default camera's 0.2 to 500 m, always holding the start radius.
fn radius_range(camera: &OrbitCamera) -> (f32, f32) {
    let low = (camera.near * 2.0).min(camera.radius);
    let high = (camera.far * 0.5).max(camera.radius);
    (low, high)
}

/// Never under the C++ 60 m its scenes were tuned for, more for a scene viewed from farther away.
pub fn shadow_distance(view_distance: f32) -> f32 {
    let fitted = view_distance * SHADOW_DISTANCE_PER_VIEW_DISTANCE;
    let distance = if fitted.is_finite() {
        fitted.max(DEFAULT_SHADOW_DISTANCE)
    } else {
        DEFAULT_SHADOW_DISTANCE
    };
    distance.clamp(*SHADOW_DISTANCE_RANGE.start(), *SHADOW_DISTANCE_RANGE.end())
}

/// The scene's own speed, [`fly_speed::speed_for_bounds`], at most half the start view's distance per second.
fn fly_speed(bounds: Option<(Vec3, Vec3)>, view_distance: f32) -> f32 {
    let scene = fly_speed::speed_for_bounds(bounds);
    let close = view_distance * FLY_SPEED_PER_VIEW_DISTANCE;
    let speed = if close.is_finite() && close > 0.0 {
        scene.min(close)
    } else {
        scene
    };
    speed.clamp(
        *fly_speed::SPEED_RANGE.start(),
        *fly_speed::SPEED_RANGE.end(),
    )
}

/// The C++ engine's fly camera: an eye and the way it looks, yaw and pitch in degrees, front =
/// (cos pitch cos yaw, sin pitch, cos pitch sin yaw).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CppPose {
    pub eye: Vec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_y_deg: f32,
    pub near: f32,
    pub far: f32,
    pub fly_speed: f32,
    pub shadow_distance: f32,
}

impl CppPose {
    /// The way the C++ camera looks.
    pub fn forward(&self) -> Vec3 {
        let (yaw, pitch) = (self.yaw_deg.to_radians(), self.pitch_deg.to_radians());
        Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.sin(),
            pitch.cos() * yaw.sin(),
        )
    }

    /// An orbit camera at this eye, looking the same way: the orbit's yaw and pitch point back at the eye.
    pub fn orbit(&self) -> OrbitCamera {
        OrbitCamera {
            target: self.eye + self.forward(),
            radius: 1.0,
            yaw_deg: self.yaw_deg - 180.0,
            pitch_deg: -self.pitch_deg,
            fov_y_deg: self.fov_y_deg,
            near: self.near,
            far: self.far,
            ..OrbitCamera::default()
        }
    }

    /// `framing`'s start view as the C++ camera holds it: yaw + 180 and -pitch of the orbit's, clamped like C++'s.
    pub fn of(framing: &Framing) -> Self {
        let camera = &framing.camera;
        let yaw = wrap_degrees(camera.yaw_deg + 180.0);
        let pitch = (-camera.pitch_deg).clamp(-CPP_PITCH_LIMIT_DEG, CPP_PITCH_LIMIT_DEG);
        Self {
            eye: camera.eye(),
            yaw_deg: yaw,
            pitch_deg: pitch,
            fov_y_deg: camera.fov_y_deg,
            near: camera.near,
            far: camera.far,
            fly_speed: framing.fly_speed,
            shadow_distance: framing.shadow_distance,
        }
    }
}

/// `degrees` in (-180, 180].
fn wrap_degrees(degrees: f32) -> f32 {
    let wrapped = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if wrapped <= -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// The C++ engine's start view, or `None` to keep its fixed pose: only a scene with its own camera, or one the
/// fixed pose sees only through a far plane past [`NEAR_FAR_RATIO`] times its near plane, is framed.
pub fn frame_for_cpp(
    cameras: &[SceneCamera],
    active: Option<(usize, String)>,
    bounds: Option<(Vec3, Vec3)>,
    fixed: &CppPose,
) -> Option<(CppPose, FramingSource)> {
    let base = fixed.orbit();
    let framing = frame(cameras, active, bounds, &base);
    let framed = match framing.source {
        FramingSource::ActiveCamera(_) | FramingSource::FirstCamera => true,
        FramingSource::Bounds => bounds
            .and_then(bounding_sphere)
            .is_some_and(|(centre, half)| fixed_pose_far_too_large(fixed, centre, half)),
        FramingSource::Unchanged => false,
    };
    framed.then(|| (CppPose::of(&framing), framing.source))
}

/// The fixed pose would need a far plane whose depth precision z-fights at its own near plane.
fn fixed_pose_far_too_large(fixed: &CppPose, centre: Vec3, half: f32) -> bool {
    let usable = fixed.near.is_finite() && fixed.near > 0.0 && fixed.eye.is_finite();
    !usable || needed_far(fixed.eye, centre, half) > fixed.near * NEAR_FAR_RATIO
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C++ Release start pose: Sponza's.
    fn release_pose() -> CppPose {
        CppPose {
            eye: Vec3::new(0.0, 2.0, 0.0),
            yaw_deg: -90.0,
            pitch_deg: 0.0,
            fov_y_deg: 45.0,
            near: 0.1,
            far: 4000.0,
            fly_speed: 10.0,
            shadow_distance: 60.0,
        }
    }

    fn arena_camera() -> SceneCamera {
        SceneCamera {
            eye: Vec3::new(-58.0, 34.0, 36.0),
            forward: Vec3::new(0.783, -0.403, -0.474),
            yfov_rad: Some(0.7427),
        }
    }

    const COLOSSEUM: (Vec3, Vec3) = (
        Vec3::new(-4000.0, -11.25, -4000.0),
        Vec3::new(4000.0, 58.6, 4000.0),
    );

    #[test]
    fn the_cpp_pose_looks_where_the_orbit_camera_looks() {
        for (yaw, pitch) in [(45.0, 25.0), (-170.0, -60.0), (90.0, 0.0), (0.0, 88.0)] {
            let framing = finish(
                OrbitCamera {
                    target: Vec3::new(1.0, 2.0, 3.0),
                    radius: 5.0,
                    yaw_deg: yaw,
                    pitch_deg: pitch,
                    ..OrbitCamera::default()
                },
                None,
                FramingSource::Bounds,
            );
            let pose = CppPose::of(&framing);
            let camera = &framing.camera;
            assert!(pose.eye.abs_diff_eq(camera.eye(), 1e-4));
            let looks = (camera.target - camera.eye()).normalize();
            assert!(pose.forward().abs_diff_eq(looks, 1e-4), "{yaw} {pitch}");
            assert_eq!(pose.pitch_deg, -pitch);
            assert!((wrap_degrees(pose.yaw_deg - yaw - 180.0)).abs() < 1e-3);
            assert!(pose.yaw_deg > -180.0 && pose.yaw_deg <= 180.0);
        }
    }

    #[test]
    fn the_cpp_pose_round_trips_through_an_orbit_camera() {
        let pose = release_pose();
        let orbit = pose.orbit();
        assert!(orbit.eye().abs_diff_eq(pose.eye, 1e-5));
        let back = CppPose::of(&finish(orbit, None, FramingSource::Bounds));
        assert!(back.forward().abs_diff_eq(pose.forward(), 1e-5));
        assert!((back.yaw_deg - pose.yaw_deg).abs() < 1e-3);
    }

    #[test]
    fn a_scene_with_its_own_camera_is_framed_at_it() {
        let (pose, source) = frame_for_cpp(
            &[
                SceneCamera {
                    eye: Vec3::new(23.0, 2.2, 1.5),
                    forward: Vec3::NEG_Z,
                    yfov_rad: None,
                },
                arena_camera(),
            ],
            Some((1, "Kamera_Arena".into())),
            Some(COLOSSEUM),
            &release_pose(),
        )
        .expect("an authored camera frames the scene");
        assert_eq!(source, FramingSource::ActiveCamera("Kamera_Arena".into()));
        assert!(pose.eye.abs_diff_eq(arena_camera().eye, 1e-3));
        assert!(pose
            .forward()
            .abs_diff_eq(arena_camera().forward.normalize(), 1e-3));
        assert!((pose.fov_y_deg - 0.7427_f32.to_degrees()).abs() < 1e-3);
        assert!(pose.far >= 8000.0 && pose.far / pose.near <= NEAR_FAR_RATIO * 1.001);
        assert!(pose.shadow_distance > 120.0, "{}", pose.shadow_distance);
        assert!(pose.fly_speed > 10.0 && pose.fly_speed < 50.0);
        // Without the active camera the first one frames it.
        let first = frame_for_cpp(&[arena_camera()], None, Some(COLOSSEUM), &release_pose());
        assert_eq!(
            first.map(|(_, source)| source),
            Some(FramingSource::FirstCamera)
        );
    }

    #[test]
    fn a_scene_the_fixed_pose_sees_keeps_the_pose() {
        // Sponza's bounds, about 40 m across.
        let sponza = (Vec3::new(-19.2, -1.3, -11.9), Vec3::new(18.0, 15.4, 11.2));
        assert_eq!(
            frame_for_cpp(&[], None, Some(sponza), &release_pose()),
            None
        );
        assert_eq!(frame_for_cpp(&[], None, None, &release_pose()), None);
    }

    #[test]
    fn a_scene_past_the_fixed_poses_depth_range_is_fitted_to_its_bounds() {
        let (pose, source) = frame_for_cpp(&[], None, Some(COLOSSEUM), &release_pose())
            .expect("an 8 km scene needs more than 50 000 near planes");
        assert_eq!(source, FramingSource::Bounds);
        // The fixed pose's direction and field of view, moved back over the whole scene.
        assert!(pose.forward().abs_diff_eq(release_pose().forward(), 1e-4));
        assert_eq!(pose.fov_y_deg, 45.0);
        assert!(pose.eye.z > 4000.0, "{}", pose.eye);
        assert!(pose.far / pose.near <= NEAR_FAR_RATIO * 1.001);
        // A cube on the eye needs FAR_MARGIN half diagonals of far plane: just inside it stays, just past it frames.
        let reach = |far: f32| {
            let half = far / FAR_MARGIN / 3.0_f32.sqrt();
            (
                Vec3::new(-half, 2.0 - half, -half),
                Vec3::new(half, 2.0 + half, half),
            )
        };
        let limit = 0.1 * NEAR_FAR_RATIO;
        assert_eq!(
            frame_for_cpp(&[], None, Some(reach(limit * 0.99)), &release_pose()),
            None
        );
        assert!(frame_for_cpp(&[], None, Some(reach(limit * 1.01)), &release_pose()).is_some());
    }

    #[test]
    fn a_camera_without_a_usable_pose_falls_back() {
        let broken = SceneCamera {
            eye: Vec3::NAN,
            forward: Vec3::NEG_Z,
            yfov_rad: Some(0.8),
        };
        let unseen = frame_for_cpp(&[broken], Some((0, "Broken".into())), None, &release_pose());
        assert_eq!(unseen, None);
        let zero = SceneCamera {
            forward: Vec3::ZERO,
            ..arena_camera()
        };
        assert_eq!(
            frame(&[zero], None, None, &OrbitCamera::default()).source,
            FramingSource::Unchanged
        );
    }

    #[test]
    fn the_shadows_reach_twice_the_start_view_never_under_the_cpp_distance() {
        assert_eq!(shadow_distance(4.0), DEFAULT_SHADOW_DISTANCE);
        assert_eq!(shadow_distance(100.0), 200.0);
        assert_eq!(shadow_distance(1e6), *SHADOW_DISTANCE_RANGE.end());
        assert_eq!(shadow_distance(f32::NAN), DEFAULT_SHADOW_DISTANCE);
    }
}
