//! The camera lens: a perspective in clip depth 0..1 whose centre lands on the scene viewport.

// glam's `directx` convention is NDC Z in [0,1], Y up: wgpu's, and the old `Mat4::*_rh` exactly.
use glam::camera::rh::proj::directx as clip;
use glam::Mat4;

use crate::viewport::SceneViewport;

/// A `fov_y_deg` lens from `near` to `far` on `viewport`, whose aspect it takes; `frame_aspect` is the whole frame's.
/// WebGPU's clip space, Y up: the C++ engine's Vulkan projection is this matrix with row 1 negated.
/// A zero-height frame (an infinite or NaN `frame_aspect`) takes aspect 1, as the C++ `viewportAspectRatio`.
pub fn perspective(
    fov_y_deg: f32,
    frame_aspect: f32,
    near: f32,
    far: f32,
    viewport: SceneViewport,
) -> Mat4 {
    let rect = viewport.sanitized();
    let aspect = rect.aspect(frame_aspect);
    let aspect = if aspect.is_finite() {
        aspect.max(1e-6)
    } else {
        1.0
    };
    let lens = clip::perspective(fov_y_deg.to_radians(), aspect, near, far);
    // Skipping the identity keeps a full-frame camera's matrix bit for bit what it was.
    if rect.is_full() {
        lens
    } else {
        rect.remap() * lens
    }
}

#[cfg(test)]
mod tests {
    use glam::{Vec2, Vec3};

    use super::*;

    const DOCKED: SceneViewport = SceneViewport {
        x0: 0.155,
        y0: 0.04,
        x1: 0.79,
        y1: 0.77,
    };

    #[test]
    fn a_full_frame_lens_is_the_plain_perspective_bit_for_bit() {
        let aspect = 1600.0 / 900.0;
        let plain = clip::perspective(45f32.to_radians(), aspect, 0.1, 1000.0);
        assert_eq!(
            perspective(45.0, aspect, 0.1, 1000.0, SceneViewport::FULL),
            plain
        );
    }

    #[test]
    fn the_lens_axis_lands_on_the_rect_centre_with_the_rects_aspect() {
        let lens = perspective(45.0, 1600.0 / 900.0, 0.1, 1000.0, DOCKED);
        let ndc = lens.project_point3(Vec3::new(0.0, 0.0, -5.0));
        let fraction = Vec2::new((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
        let centre = Vec2::new(DOCKED.x0 + DOCKED.x1, DOCKED.y0 + DOCKED.y1) * 0.5;
        assert!(fraction.abs_diff_eq(centre, 1e-5), "{fraction} vs {centre}");
        let unshifted = DOCKED.remap().inverse() * lens;
        let rect_aspect = DOCKED.aspect(1600.0 / 900.0);
        assert!((unshifted.y_axis.y / unshifted.x_axis.x - rect_aspect).abs() < 1e-4);
    }

    #[test]
    fn a_zero_height_frame_has_aspect_one() {
        let square = DOCKED.remap() * clip::perspective(45f32.to_radians(), 1.0, 0.1, 100.0);
        for width in [1600.0f32, 0.0] {
            let zero_height = width / 0.0;
            assert_eq!(
                perspective(45.0, zero_height, 0.1, 100.0, DOCKED),
                square,
                "{width} by 0"
            );
        }
    }

    #[test]
    fn a_degenerate_rect_draws_the_whole_frame() {
        let sliver = SceneViewport {
            x1: 0.01,
            ..SceneViewport::FULL
        };
        assert_eq!(
            perspective(60.0, 2.0, 0.5, 50.0, sliver),
            perspective(60.0, 2.0, 0.5, 50.0, SceneViewport::FULL)
        );
    }
}
