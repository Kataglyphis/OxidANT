//! The scene's share of the frame, and the lens shift that centres the camera on it while every pass stays full-frame.

use glam::{Mat4, Vec2, Vec4};

/// Below this share per axis the rect is degenerate and the whole frame is used, as in the C++ engine.
pub const MIN_VIEWPORT_FRACTION: f32 = 0.05;

/// The free rect between the docks, in fractions of the frame's width and height from its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneViewport {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Default for SceneViewport {
    fn default() -> Self {
        Self::FULL
    }
}

impl SceneViewport {
    pub const FULL: Self = Self {
        x0: 0.0,
        y0: 0.0,
        x1: 1.0,
        y1: 1.0,
    };

    /// The fractions of the frame `rect_min..rect_max` covers, both rects in the same units (egui points).
    pub fn of_rect(rect_min: Vec2, rect_max: Vec2, frame_min: Vec2, frame_max: Vec2) -> Self {
        let size = (frame_max - frame_min).max(Vec2::splat(f32::MIN_POSITIVE));
        let min = (rect_min - frame_min) / size;
        let max = (rect_max - frame_min) / size;
        Self {
            x0: min.x,
            y0: min.y,
            x1: max.x,
            y1: max.y,
        }
        .sanitized()
    }

    /// Edges clamp into the frame (NaN to 0), and a rect under 5 % on either axis becomes the whole frame.
    pub fn sanitized(self) -> Self {
        // `clamp` keeps NaN; the C++ `sanitizeViewportFraction` maps it to 0.
        let unit = |value: f32| if value >= 0.0 { value.min(1.0) } else { 0.0 };
        let rect = Self {
            x0: unit(self.x0),
            y0: unit(self.y0),
            x1: unit(self.x1),
            y1: unit(self.y1),
        };
        let degenerate =
            rect.x1 - rect.x0 < MIN_VIEWPORT_FRACTION || rect.y1 - rect.y0 < MIN_VIEWPORT_FRACTION;
        if degenerate {
            Self::FULL
        } else {
            rect
        }
    }

    pub fn is_full(self) -> bool {
        self == Self::FULL
    }

    /// The rect's aspect ratio, given the whole frame's width over height.
    pub fn aspect(self, frame_aspect: f32) -> f32 {
        frame_aspect * ((self.x1 - self.x0) / (self.y1 - self.y0))
    }

    /// Clip space onto the rect: the C++ `makeViewportRemap`, with Y negated because WebGPU's clip Y points up.
    pub fn remap(self) -> Mat4 {
        Mat4::from_cols(
            Vec4::new(self.x1 - self.x0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, self.y1 - self.y0, 0.0, 0.0),
            Vec4::Z,
            Vec4::new(self.x0 + self.x1 - 1.0, 1.0 - (self.y0 + self.y1), 0.0, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    const DOCKED: SceneViewport = SceneViewport {
        x0: 0.155,
        y0: 0.04,
        x1: 0.79,
        y1: 0.77,
    };

    /// NDC to top-left fractions of the frame, WebGPU's clip Y pointing up.
    fn to_fraction(ndc: Vec3) -> Vec2 {
        Vec2::new((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5)
    }

    fn assert_near(actual: Vec2, expected: Vec2) {
        assert!(
            actual.abs_diff_eq(expected, 1e-5),
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn the_full_rect_is_the_identity() {
        assert_eq!(SceneViewport::FULL.remap(), Mat4::IDENTITY);
        assert_eq!(SceneViewport::default(), SceneViewport::FULL);
        assert!(SceneViewport::FULL.is_full());
        assert_eq!(SceneViewport::FULL.aspect(16.0 / 9.0), 16.0 / 9.0);
    }

    #[test]
    fn the_lens_centre_lands_on_the_rect_centre_and_back() {
        let remap = DOCKED.remap();
        let centre = Vec2::new(DOCKED.x0 + DOCKED.x1, DOCKED.y0 + DOCKED.y1) * 0.5;
        assert_near(to_fraction(remap.project_point3(Vec3::ZERO)), centre);
        let centre_ndc = Vec3::new(centre.x * 2.0 - 1.0, 1.0 - centre.y * 2.0, 0.5);
        let back = remap.inverse().project_point3(centre_ndc);
        assert!(back.truncate().length() < 1e-5, "{back}");
    }

    #[test]
    fn the_clip_edges_land_on_the_rect_edges() {
        let remap = DOCKED.remap();
        let corner = |x: f32, y: f32| to_fraction(remap.project_point3(Vec3::new(x, y, 0.5)));
        assert_near(corner(-1.0, 1.0), Vec2::new(DOCKED.x0, DOCKED.y0));
        assert_near(corner(1.0, 1.0), Vec2::new(DOCKED.x1, DOCKED.y0));
        assert_near(corner(-1.0, -1.0), Vec2::new(DOCKED.x0, DOCKED.y1));
        assert_near(corner(1.0, -1.0), Vec2::new(DOCKED.x1, DOCKED.y1));
    }

    #[test]
    fn depth_and_w_pass_through_untouched() {
        let clip = DOCKED.remap() * Vec4::new(0.3, -0.2, 0.7, 2.0);
        assert_eq!((clip.z, clip.w), (0.7, 2.0));
    }

    #[test]
    fn sanitizing_clamps_maps_nan_to_zero_and_collapses_slivers() {
        let wild = SceneViewport {
            x0: f32::NAN,
            y0: -0.5,
            x1: 1.5,
            y1: 0.6,
        };
        let expected = SceneViewport {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 0.6,
        };
        assert_eq!(wild.sanitized(), expected);
        let sliver = SceneViewport {
            x1: 0.04,
            ..SceneViewport::FULL
        };
        assert_eq!(sliver.sanitized(), SceneViewport::FULL);
        let inverted = SceneViewport {
            x0: 0.8,
            x1: 0.2,
            ..SceneViewport::FULL
        };
        assert_eq!(inverted.sanitized(), SceneViewport::FULL);
    }

    #[test]
    fn the_aspect_is_the_rects_own() {
        let half_width = SceneViewport {
            x1: 0.5,
            ..SceneViewport::FULL
        };
        assert_eq!(half_width.aspect(2.0), 1.0);
    }

    #[test]
    fn a_rect_in_points_becomes_fractions_of_its_frame() {
        let frame = (Vec2::ZERO, Vec2::new(1600.0, 900.0));
        let scene = SceneViewport::of_rect(
            Vec2::new(248.0, 36.0),
            Vec2::new(1264.0, 693.0),
            frame.0,
            frame.1,
        );
        assert_near(Vec2::new(scene.x0, scene.y0), Vec2::new(0.155, 0.04));
        assert_near(Vec2::new(scene.x1, scene.y1), Vec2::new(0.79, 0.77));
        let whole = SceneViewport::of_rect(frame.0, frame.1, frame.0, frame.1);
        assert!(whole.is_full());
    }
}
