//! The frustum tests both renderers cull with; `tests/vectors/frustum_verdicts.json` holds the verdicts they share.
//! The C++ engine's rules: unknown bounds render, planes are normalised with a tolerance, a normal-less plane is skipped.

use glam::{Mat4, Vec3, Vec4};

/// World units a box may lie outside a normalised plane and still count as inside it.
pub const PLANE_TOLERANCE: f32 = 1e-4;
/// A plane's normal shorter than this is left unnormalised, and then skipped: dividing by it would make NaN.
const MIN_NORMAL_LENGTH: f32 = 1e-8;
/// The skip's squared threshold, the C++ literal rather than a rounded square of [`MIN_NORMAL_LENGTH`].
const MIN_NORMAL_LENGTH_SQUARED: f32 = 1e-16;

/// View frustum from a wgpu (0..1 depth) view-projection; Gribb-Hartmann planes, normals inward and normalised.
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_view_proj(m: &Mat4) -> Self {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);
        Self {
            planes: [
                r3 + r0, // left
                r3 - r0, // right
                r3 + r1, // bottom
                r3 - r1, // top
                r2,      // near (z >= 0 in 0..1 depth)
                r3 - r2, // far
            ]
            .map(normalized),
        }
    }

    /// The six planes, left, right, bottom, top, near, far, for a GPU copy of the tests below.
    pub fn planes(&self) -> [Vec4; 6] {
        self.planes
    }

    /// Positive-vertex test, which `draw_cull.slang` copies: outside when the AABB's most favourable corner is behind any plane.
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        test_planes(min, max, &self.planes)
    }

    /// Caster test without the near plane: casters between light and cascade box still shadow it.
    pub fn intersects_aabb_as_caster(&self, min: Vec3, max: Vec3) -> bool {
        let [left, right, bottom, top, _near, far] = &self.planes;
        test_planes(min, max, &[*left, *right, *bottom, *top, *far])
    }
}

fn normalized(plane: Vec4) -> Vec4 {
    let length = plane.truncate().length();
    if length < MIN_NORMAL_LENGTH {
        plane
    } else {
        plane / length
    }
}

fn test_planes(min: Vec3, max: Vec3, planes: &[Vec4]) -> bool {
    // An inverted or NaN box is unknown bounds, no licence to cull.
    if !min.cmple(max).all() {
        return true;
    }
    for plane in planes {
        let normal = plane.truncate();
        if normal.length_squared() < MIN_NORMAL_LENGTH_SQUARED {
            continue;
        }
        let p = Vec3::new(
            if plane.x >= 0.0 { max.x } else { min.x },
            if plane.y >= 0.0 { max.y } else { min.y },
            if plane.z >= 0.0 { max.z } else { min.z },
        );
        if normal.dot(p) + plane.w < -PLANE_TOLERANCE {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    // glam's `directx` clip space (NDC Z 0..1, Y up) is wgpu's and matches the old `Mat4::*_rh`.
    use glam::camera::rh::proj::directx as clip;
    use glam::camera::rh::view::look_at_mat4;

    use super::*;

    fn light() -> Mat4 {
        clip::orthographic(-1.0, 1.0, -1.0, 1.0, 0.0, 2.0)
            * look_at_mat4(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO, Vec3::Y)
    }

    #[test]
    fn caster_test_ignores_the_near_plane_and_nothing_else() {
        // Between the light and the box only the caster test passes; beside the box both fail.
        let light = light();
        let behind_near = (Vec3::new(-0.1, -0.1, 1.4), Vec3::new(0.1, 0.1, 1.6));
        let frustum = Frustum::from_view_proj(&light);
        assert!(
            !frustum.intersects_aabb(behind_near.0, behind_near.1),
            "the full test must reject a box behind the near plane"
        );
        assert!(
            frustum.intersects_aabb_as_caster(behind_near.0, behind_near.1),
            "a caster between the light and the box still shadows into it"
        );

        let beside = (Vec3::new(5.0, -0.1, 0.0), Vec3::new(5.2, 0.1, 0.2));
        assert!(
            !frustum.intersects_aabb_as_caster(beside.0, beside.1),
            "outside a side plane, the shadow lands outside the map too"
        );

        let inside = (Vec3::splat(-0.2), Vec3::splat(0.2));
        assert!(frustum.intersects_aabb_as_caster(inside.0, inside.1));
        assert_eq!(
            frustum.planes()[4],
            normalized(light.row(2)),
            "the near plane is row 2"
        );
    }

    #[test]
    fn every_plane_is_normalised_and_a_missing_normal_stays_as_it_is() {
        for plane in Frustum::from_view_proj(&light()).planes() {
            assert!((plane.truncate().length() - 1.0).abs() < 1e-6, "{plane}");
        }
        let flat = Vec4::new(0.0, 0.0, 0.0, -1.0);
        assert_eq!(normalized(flat), flat, "no NaN from a zero normal");
    }

    #[test]
    fn a_box_with_a_nan_corner_is_unknown_bounds_and_renders() {
        let frustum = Frustum::from_view_proj(&light());
        let far_away = Vec3::splat(100.0);
        for corner in [
            Vec3::new(f32::NAN, 100.0, 100.0),
            Vec3::new(100.0, 100.0, f32::NAN),
        ] {
            assert!(frustum.intersects_aabb(corner, far_away + 1.0));
            assert!(frustum.intersects_aabb_as_caster(far_away, corner));
        }
        assert!(
            !frustum.intersects_aabb(far_away, far_away + 1.0),
            "the same box with known bounds is culled"
        );
    }
}
