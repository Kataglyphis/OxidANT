//! The C++ engine's fly camera (W A S D, Q E, right-drag look) on the [`OrbitCamera`] the orbit controller drives.
//! One state serves both: flying moves the eye and the target together and turns about the eye, orbiting turns
//! about the target, and the wheel changes the radius between them, so an orbit after a flight circles the point
//! the camera was looking at.

use glam::Vec3;
use winit::event::{MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::scene::camera::OrbitCamera;

/// The C++ `movement_speed` in world units per second, for a scene without bounds.
pub const DEFAULT_SPEED: f32 = 10.0;
/// The slowest and fastest speed a scene or the Camera tab may set.
pub const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.001..=1_000_000.0;
/// The C++ `kKeyboardTurnDegreesPerSecond`.
pub const TURN_DEG_PER_S: f32 = 90.0;
/// The C++ `turn_speed`: degrees of look per pixel of mouse travel.
pub const LOOK_DEG_PER_PX: f32 = 0.25;
/// The C++ pitch clamp, which also keeps the view off the poles.
pub const PITCH_LIMIT_DEG: f32 = 89.0;
/// The longest frame one step integrates, so a hitch cannot fling the camera across the scene.
pub const MAX_STEP_S: f32 = 0.1;
/// A quarter of the diagonal per second: the C++ 10 units/s in a Sponza about 40 units across.
const SPEED_PER_DIAGONAL: f32 = 0.25;

/// The default speed for a scene's bounds: a 1 m cube and an 8 km city both take four seconds to cross.
pub fn speed_for_bounds(bounds: Option<(Vec3, Vec3)>) -> f32 {
    let diagonal = bounds.map_or(f32::NAN, |(min, max)| (max - min).length());
    if diagonal.is_finite() && diagonal > 0.0 {
        (diagonal * SPEED_PER_DIAGONAL).clamp(*SPEED_RANGE.start(), *SPEED_RANGE.end())
    } else {
        DEFAULT_SPEED
    }
}

/// The keys the fly camera reads, by physical position like the C++ GLFW codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyKey {
    Forward,
    Back,
    Left,
    Right,
    TurnLeft,
    TurnRight,
}

impl FlyKey {
    pub const ALL: [Self; 6] = [
        Self::Forward,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::TurnLeft,
        Self::TurnRight,
    ];

    pub fn of(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::KeyW => Some(Self::Forward),
            KeyCode::KeyS => Some(Self::Back),
            KeyCode::KeyA => Some(Self::Left),
            KeyCode::KeyD => Some(Self::Right),
            KeyCode::KeyQ => Some(Self::TurnLeft),
            KeyCode::KeyE => Some(Self::TurnRight),
            _ => None,
        }
    }

    /// The key's label on a US layout, as the CONTROLS list writes it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "W",
            Self::Back => "S",
            Self::Left => "A",
            Self::Right => "D",
            Self::TurnLeft => "Q",
            Self::TurnRight => "E",
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// One window event as the fly camera reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlyInput {
    Key {
        key: FlyKey,
        pressed: bool,
    },
    /// The right mouse button.
    Look {
        pressed: bool,
    },
    /// The cursor in physical pixels.
    CursorMoved(f64, f64),
    FocusLost,
}

impl FlyInput {
    /// The fly camera's reading of `event`; winit's synthetic presses of keys held at focus gain are no presses.
    pub fn of(event: &WindowEvent) -> Option<Self> {
        match event {
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return None;
                };
                let pressed = event.state.is_pressed();
                let key = FlyKey::of(code).filter(|_| !(pressed && *is_synthetic))?;
                Some(Self::Key { key, pressed })
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => Some(Self::Look {
                pressed: state.is_pressed(),
            }),
            WindowEvent::CursorMoved { position, .. } => {
                Some(Self::CursorMoved(position.x, position.y))
            }
            WindowEvent::Focused(false) => Some(Self::FocusLost),
            _ => None,
        }
    }
}

/// What the host does with the pointer after an input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerGrab {
    Keep,
    /// A look began: hide the pointer and hold it in the window.
    Hide,
    /// The look ended: show the pointer, back where the look began when that is known, as GLFW does.
    Show {
        at: Option<(f64, f64)>,
    },
}

/// One right-button look.
#[derive(Clone, Copy, Debug, Default)]
struct Look {
    /// The cursor when the look began, where it returns.
    anchor: Option<(f64, f64)>,
    /// The cursor at the last move; the first move after the press only sets it, the C++ `mouse_first_moved`.
    last: Option<(f64, f64)>,
    /// Raw device motion arrived; cursor moves, which stop at a confined window's edge, no longer turn the view.
    raw: bool,
}

/// The fly camera's input state; [`Self::advance`] applies the held keys once a frame.
#[derive(Clone, Debug)]
pub struct FlyController {
    /// World units per second; [`speed_for_bounds`] fits it to a scene.
    pub speed: f32,
    held: u8,
    look: Option<Look>,
    cursor: Option<(f64, f64)>,
}

impl Default for FlyController {
    fn default() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            held: 0,
            look: None,
            cursor: None,
        }
    }
}

impl FlyController {
    /// A controller at `speed`, such as [`speed_for_bounds`] of the scene.
    pub fn with_speed(speed: f32) -> Self {
        Self {
            speed,
            ..Self::default()
        }
    }

    pub fn is_held(&self, key: FlyKey) -> bool {
        self.held & key.bit() != 0
    }

    pub fn is_looking(&self) -> bool {
        self.look.is_some()
    }

    /// A key is held or a look is on: the user is steering, so an idle spin must stop.
    pub fn is_engaged(&self) -> bool {
        self.held != 0 || self.is_looking()
    }

    /// Applies one input: a press egui consumed is dropped, a release always lands, so no key or look sticks.
    pub fn handle(
        &mut self,
        input: FlyInput,
        consumed: bool,
        camera: &mut OrbitCamera,
    ) -> PointerGrab {
        match input {
            FlyInput::Key { key, pressed } => {
                if !pressed {
                    self.held &= !key.bit();
                } else if !consumed {
                    self.held |= key.bit();
                }
                PointerGrab::Keep
            }
            FlyInput::Look { pressed: true } if !consumed && !self.is_looking() => {
                self.look = Some(Look {
                    anchor: self.cursor,
                    ..Look::default()
                });
                PointerGrab::Hide
            }
            FlyInput::Look { pressed: false } => self.end_look(),
            FlyInput::Look { pressed: true } => PointerGrab::Keep,
            FlyInput::CursorMoved(x, y) => {
                self.cursor_moved(camera, (x, y));
                PointerGrab::Keep
            }
            FlyInput::FocusLost => {
                self.held = 0;
                self.end_look()
            }
        }
    }

    /// Raw device motion during a look; from the first one on it alone turns the view.
    pub fn raw_motion(&mut self, camera: &mut OrbitCamera, (dx, dy): (f64, f64)) {
        if let Some(look) = self.look.as_mut() {
            look.raw = true;
            look_by(camera, dx as f32, dy as f32);
        }
    }

    /// Moves and turns by the held keys over `dt_s`, at most [`MAX_STEP_S`]; true when the camera moved.
    pub fn advance(&self, camera: &mut OrbitCamera, dt_s: f32) -> bool {
        let dt = if dt_s.is_nan() {
            0.0
        } else {
            dt_s.clamp(0.0, MAX_STEP_S)
        };
        let forward = self.axis(FlyKey::Forward, FlyKey::Back);
        let side = self.axis(FlyKey::Right, FlyKey::Left);
        let turn = self.axis(FlyKey::TurnRight, FlyKey::TurnLeft);
        if dt == 0.0 || (forward == 0.0 && side == 0.0 && turn == 0.0) {
            return false;
        }
        // The C++ order: move along the old heading, then turn.
        let front = -direction(camera.yaw_deg, camera.pitch_deg);
        let right = front.cross(Vec3::Y).normalize_or_zero();
        camera.target += (front * forward + right * side) * self.speed * dt;
        if turn != 0.0 {
            turn_about_eye(camera, turn * TURN_DEG_PER_S * dt, 0.0);
        }
        true
    }

    fn axis(&self, plus: FlyKey, minus: FlyKey) -> f32 {
        f32::from(u8::from(self.is_held(plus))) - f32::from(u8::from(self.is_held(minus)))
    }

    fn cursor_moved(&mut self, camera: &mut OrbitCamera, at: (f64, f64)) {
        self.cursor = Some(at);
        let Some(look) = self.look.as_mut() else {
            return;
        };
        if let Some((x, y)) = look.last.replace(at) {
            if !look.raw {
                look_by(camera, (at.0 - x) as f32, (at.1 - y) as f32);
            }
        }
    }

    fn end_look(&mut self) -> PointerGrab {
        match self.look.take() {
            Some(look) => PointerGrab::Show { at: look.anchor },
            None => PointerGrab::Keep,
        }
    }
}

/// The unit vector from the target to the eye: [`OrbitCamera::eye`]'s offset, against the C++ `front`.
fn direction(yaw_deg: f32, pitch_deg: f32) -> Vec3 {
    let yaw = yaw_deg.to_radians();
    let pitch = pitch_deg
        .clamp(-PITCH_LIMIT_DEG, PITCH_LIMIT_DEG)
        .to_radians();
    Vec3::new(
        pitch.cos() * yaw.cos(),
        pitch.sin(),
        pitch.cos() * yaw.sin(),
    )
}

/// Mouse travel in pixels turns the view; down on the screen looks down, as the C++ `last_y - y` does.
fn look_by(camera: &mut OrbitCamera, dx_px: f32, dy_px: f32) {
    turn_about_eye(camera, dx_px * LOOK_DEG_PER_PX, dy_px * LOOK_DEG_PER_PX);
}

/// Turns the view about the eye, which stays put: the target swings round it at the same radius.
fn turn_about_eye(camera: &mut OrbitCamera, yaw_deg: f32, pitch_deg: f32) {
    let eye = camera.eye();
    camera.yaw_deg = (camera.yaw_deg + yaw_deg).rem_euclid(360.0);
    camera.pitch_deg = (camera.pitch_deg + pitch_deg).clamp(-PITCH_LIMIT_DEG, PITCH_LIMIT_DEG);
    camera.target = eye - camera.radius * direction(camera.yaw_deg, camera.pitch_deg);
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceId, ElementState};

    use super::*;
    use crate::scene::controller::OrbitController;

    /// Looking down -Z from 4 units out, level.
    fn level_camera() -> OrbitCamera {
        OrbitCamera {
            yaw_deg: 90.0,
            pitch_deg: 0.0,
            ..OrbitCamera::default()
        }
    }

    fn front(camera: &OrbitCamera) -> Vec3 {
        (camera.target - camera.eye()).normalize()
    }

    fn press(fly: &mut FlyController, camera: &mut OrbitCamera, key: FlyKey) {
        fly.handle(FlyInput::Key { key, pressed: true }, false, camera);
    }

    fn assert_close(a: Vec3, b: Vec3) {
        assert!(a.abs_diff_eq(b, 1e-4), "{a} is not {b}");
    }

    #[test]
    fn the_offset_is_the_eye_the_camera_computes() {
        for (yaw, pitch) in [(0.0, 0.0), (45.0, 25.0), (-130.0, -60.0), (90.0, 120.0)] {
            let camera = OrbitCamera {
                target: Vec3::new(1.0, -2.0, 3.0),
                radius: 7.5,
                yaw_deg: yaw,
                pitch_deg: pitch,
                ..OrbitCamera::default()
            };
            let eye = camera.target + camera.radius * direction(yaw, pitch);
            assert_close(eye, camera.eye());
        }
    }

    #[test]
    fn w_flies_eye_and_target_along_the_view_at_the_speed() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let (eye, target, radius) = (camera.eye(), camera.target, camera.radius);
        press(&mut fly, &mut camera, FlyKey::Forward);
        assert!(fly.advance(&mut camera, 0.05));
        let step = Vec3::NEG_Z * DEFAULT_SPEED * 0.05;
        assert_close(camera.eye(), eye + step);
        assert_close(camera.target, target + step);
        assert_eq!(camera.radius, radius);
    }

    #[test]
    fn d_flies_right_s_back_and_opposite_keys_cancel() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let eye = camera.eye();
        press(&mut fly, &mut camera, FlyKey::Right);
        press(&mut fly, &mut camera, FlyKey::Back);
        fly.advance(&mut camera, 0.1);
        assert_close(
            camera.eye(),
            eye + Vec3::new(1.0, 0.0, 1.0) * DEFAULT_SPEED * 0.1,
        );
        press(&mut fly, &mut camera, FlyKey::Left);
        press(&mut fly, &mut camera, FlyKey::Forward);
        assert!(!fly.advance(&mut camera, 0.1), "held opposites cancel");
    }

    #[test]
    fn a_long_frame_moves_a_tenth_of_a_second_and_a_bad_one_none() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let eye = camera.eye();
        press(&mut fly, &mut camera, FlyKey::Forward);
        fly.advance(&mut camera, 5.0);
        assert!((camera.eye().distance(eye) - DEFAULT_SPEED * MAX_STEP_S).abs() < 1e-4);
        let eye = camera.eye();
        assert!(!fly.advance(&mut camera, f32::NAN));
        assert!(!fly.advance(&mut camera, -1.0));
        assert_eq!(camera.eye(), eye);
    }

    #[test]
    fn e_turns_right_about_the_eye_at_90_degrees_a_second() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let eye = camera.eye();
        press(&mut fly, &mut camera, FlyKey::TurnRight);
        fly.advance(&mut camera, 0.1);
        assert!((camera.yaw_deg - 99.0).abs() < 1e-4, "{}", camera.yaw_deg);
        assert_close(camera.eye(), eye);
        assert!(front(&camera).x > 0.0, "looking down -Z, right is +X");
        fly.handle(
            FlyInput::Key {
                key: FlyKey::TurnRight,
                pressed: false,
            },
            false,
            &mut camera,
        );
        press(&mut fly, &mut camera, FlyKey::TurnLeft);
        fly.advance(&mut camera, 0.1);
        assert!((camera.yaw_deg - 90.0).abs() < 1e-3, "{}", camera.yaw_deg);
    }

    #[test]
    fn a_look_turns_a_quarter_degree_per_pixel_about_the_eye() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let eye = camera.eye();
        let grab = fly.handle(FlyInput::Look { pressed: true }, false, &mut camera);
        assert_eq!(grab, PointerGrab::Hide);
        fly.handle(FlyInput::CursorMoved(100.0, 100.0), false, &mut camera);
        assert_eq!(camera.yaw_deg, 90.0, "the first move only seeds the cursor");
        fly.handle(FlyInput::CursorMoved(140.0, 60.0), false, &mut camera);
        assert!((camera.yaw_deg - 100.0).abs() < 1e-4, "{}", camera.yaw_deg);
        assert!(front(&camera).y > 0.0, "moving the mouse up looks up");
        assert_close(camera.eye(), eye);
        fly.handle(FlyInput::CursorMoved(140.0, 100_000.0), false, &mut camera);
        assert_eq!(
            camera.pitch_deg, PITCH_LIMIT_DEG,
            "the pitch stops short of the pole"
        );
    }

    #[test]
    fn raw_motion_takes_over_from_cursor_moves() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        fly.raw_motion(&mut camera, (400.0, 0.0));
        assert_eq!(camera.yaw_deg, 90.0, "no look, no turn");
        fly.handle(FlyInput::Look { pressed: true }, false, &mut camera);
        fly.handle(FlyInput::CursorMoved(10.0, 10.0), false, &mut camera);
        fly.raw_motion(&mut camera, (8.0, 0.0));
        assert!((camera.yaw_deg - 92.0).abs() < 1e-4);
        fly.handle(FlyInput::CursorMoved(500.0, 10.0), false, &mut camera);
        assert!(
            (camera.yaw_deg - 92.0).abs() < 1e-4,
            "a cursor move after raw motion counts twice"
        );
    }

    #[test]
    fn a_consumed_press_is_dropped_but_every_release_lands() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        let w = |pressed| FlyInput::Key {
            key: FlyKey::Forward,
            pressed,
        };
        fly.handle(w(true), true, &mut camera);
        assert!(!fly.is_held(FlyKey::Forward), "a W typed into a field");
        fly.handle(w(true), false, &mut camera);
        // A field took focus while W was down: egui consumes the release, which must still stop the camera.
        fly.handle(w(false), true, &mut camera);
        assert!(!fly.advance(&mut camera, 0.1));
        let look = |pressed| FlyInput::Look { pressed };
        assert_eq!(fly.handle(look(true), true, &mut camera), PointerGrab::Keep);
        assert!(!fly.is_looking(), "a right press on a panel");
        fly.handle(FlyInput::CursorMoved(5.0, 6.0), true, &mut camera);
        fly.handle(look(true), false, &mut camera);
        let shown = fly.handle(look(false), true, &mut camera);
        assert_eq!(
            shown,
            PointerGrab::Show {
                at: Some((5.0, 6.0))
            }
        );
        assert_eq!(
            fly.handle(look(false), false, &mut camera),
            PointerGrab::Keep
        );
    }

    #[test]
    fn losing_focus_releases_every_key_and_ends_the_look() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        for key in FlyKey::ALL {
            press(&mut fly, &mut camera, key);
        }
        fly.handle(FlyInput::Look { pressed: true }, false, &mut camera);
        assert!(fly.is_engaged());
        let grab = fly.handle(FlyInput::FocusLost, false, &mut camera);
        assert_eq!(grab, PointerGrab::Show { at: None });
        assert!(!fly.is_engaged());
        assert!(!fly.advance(&mut camera, 0.1));
    }

    #[test]
    fn an_orbit_after_a_flight_circles_the_point_ahead() {
        let mut fly = FlyController::default();
        let mut camera = level_camera();
        press(&mut fly, &mut camera, FlyKey::Forward);
        fly.advance(&mut camera, 0.1);
        let (target, radius) = (camera.target, camera.radius);
        OrbitController::default().apply_drag(&mut camera, 200.0, 40.0);
        assert_eq!(camera.target, target);
        assert!((camera.eye().distance(target) - radius).abs() < 1e-4);
    }

    #[test]
    fn the_speed_follows_the_scene_size() {
        assert_eq!(speed_for_bounds(None), DEFAULT_SPEED);
        let cube = speed_for_bounds(Some((Vec3::ZERO, Vec3::ONE)));
        assert!((cube - 3.0f32.sqrt() / 4.0).abs() < 1e-6, "{cube}");
        let city = speed_for_bounds(Some((Vec3::ZERO, Vec3::new(8000.0, 300.0, 8000.0))));
        assert!((city - 2829.4).abs() < 0.1, "{city}");
        assert_eq!(
            speed_for_bounds(Some((Vec3::ONE, Vec3::ONE))),
            DEFAULT_SPEED
        );
        let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        assert_eq!(speed_for_bounds(Some(empty)), DEFAULT_SPEED);
    }

    #[test]
    fn window_events_read_as_fly_input() {
        let device_id = DeviceId::dummy();
        let right = |state| WindowEvent::MouseInput {
            device_id,
            state,
            button: MouseButton::Right,
        };
        assert_eq!(
            FlyInput::of(&right(ElementState::Pressed)),
            Some(FlyInput::Look { pressed: true })
        );
        assert_eq!(
            FlyInput::of(&right(ElementState::Released)),
            Some(FlyInput::Look { pressed: false })
        );
        let left = WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: MouseButton::Left,
        };
        assert_eq!(FlyInput::of(&left), None);
        let moved = WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(3.0, 4.0),
        };
        assert_eq!(FlyInput::of(&moved), Some(FlyInput::CursorMoved(3.0, 4.0)));
        assert_eq!(
            FlyInput::of(&WindowEvent::Focused(false)),
            Some(FlyInput::FocusLost)
        );
        assert_eq!(FlyInput::of(&WindowEvent::Focused(true)), None);
        let codes = [
            KeyCode::KeyW,
            KeyCode::KeyS,
            KeyCode::KeyA,
            KeyCode::KeyD,
            KeyCode::KeyQ,
            KeyCode::KeyE,
        ];
        assert_eq!(codes.map(FlyKey::of), FlyKey::ALL.map(Some));
        assert_eq!(FlyKey::of(KeyCode::KeyR), None);
    }
}
