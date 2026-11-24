//! Orbit camera producing wgpu-convention (depth 0..1) view-projection matrices; the framing and both renderers use it.

use crate::lens;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::viewport::SceneViewport;

#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub radius: f32,
    /// Degrees around the Y axis.
    pub yaw_deg: f32,
    /// Degrees above the horizon, clamped to (-89, 89).
    pub pitch_deg: f32,
    pub fov_y_deg: f32,
    pub near: f32,
    pub far: f32,
    /// The part of the frame the docks leave free; the projection centres the camera on it.
    pub viewport: SceneViewport,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            radius: 4.0,
            yaw_deg: 45.0,
            pitch_deg: 25.0,
            fov_y_deg: 45.0,
            near: 0.1,
            far: 1000.0,
            viewport: SceneViewport::FULL,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let yaw = self.yaw_deg.to_radians();
        let pitch = self.pitch_deg.clamp(-89.0, 89.0).to_radians();
        self.target
            + self.radius
                * Vec3::new(
                    pitch.cos() * yaw.cos(),
                    pitch.sin(),
                    pitch.cos() * yaw.sin(),
                )
    }

    pub fn view(&self) -> Mat4 {
        look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    /// Projection with wgpu/WebGPU clip space (depth 0..1); `aspect` is the whole frame's.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        lens::perspective(self.fov_y_deg, aspect, self.near, self.far, self.viewport)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }
}

#[cfg(test)]
mod tests {
    // glam's `directx` convention is NDC Z in [0,1], Y up: wgpu's, and the old `Mat4::*_rh` exactly.
    use glam::camera::rh::proj::directx as clip;
    use glam::Vec2;

    use super::*;

    const DOCKED: SceneViewport = SceneViewport {
        x0: 0.155,
        y0: 0.04,
        x1: 0.79,
        y1: 0.77,
    };

    #[test]
    fn eye_orbits_at_radius() {
        let cam = OrbitCamera::default();
        assert!((cam.eye().distance(cam.target) - cam.radius).abs() < 1e-4);
    }

    #[test]
    fn view_maps_eye_to_origin() {
        let cam = OrbitCamera::default();
        let origin = cam.view().transform_point3(cam.eye());
        assert!(origin.length() < 1e-4);
    }

    #[test]
    fn a_full_frame_projection_is_bit_identical_to_the_plain_lens() {
        let cam = OrbitCamera::default();
        let aspect = 1600.0 / 900.0;
        let lens = clip::perspective(cam.fov_y_deg.to_radians(), aspect, cam.near, cam.far);
        assert_eq!(cam.projection(aspect), lens);
    }

    #[test]
    fn the_target_projects_to_the_centre_of_the_free_rect() {
        let cam = OrbitCamera {
            viewport: DOCKED,
            ..OrbitCamera::default()
        };
        let ndc = cam
            .view_projection(1600.0 / 900.0)
            .project_point3(cam.target);
        let fraction = Vec2::new((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
        let centre = Vec2::new(DOCKED.x0 + DOCKED.x1, DOCKED.y0 + DOCKED.y1) * 0.5;
        assert!(fraction.abs_diff_eq(centre, 1e-5), "{fraction} vs {centre}");
    }

    #[test]
    fn a_square_facing_the_camera_stays_square_in_the_free_rect() {
        let (width, height) = (1600.0, 900.0);
        let cam = OrbitCamera {
            viewport: DOCKED,
            ..OrbitCamera::default()
        };
        let view_proj = cam.view_projection(width / height);
        let camera_to_world = cam.view().inverse();
        let pixel = |camera_space: Vec3| {
            let ndc = view_proj.project_point3(camera_to_world.transform_point3(camera_space));
            Vec2::new((ndc.x + 1.0) * 0.5 * width, (1.0 - ndc.y) * 0.5 * height)
        };
        let depth = -cam.radius;
        let span_x = pixel(Vec3::new(0.5, 0.0, depth)).x - pixel(Vec3::new(-0.5, 0.0, depth)).x;
        let span_y = pixel(Vec3::new(0.0, -0.5, depth)).y - pixel(Vec3::new(0.0, 0.5, depth)).y;
        assert!(
            span_x > 10.0,
            "the square must be visible, spans {span_x} px"
        );
        assert!((span_x - span_y).abs() < 1e-2, "{span_x} vs {span_y} px");
    }
}
