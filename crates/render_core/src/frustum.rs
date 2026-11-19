//! The frustum tests both renderers cull with; `tests/vectors/frustum_verdicts.json` holds the verdicts they share.

use glam::{Mat4, Vec3, Vec4};

/// View frustum from a wgpu (0..1 depth) view-projection; Gribb-Hartmann planes, normals inward.
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
            ],
        }
    }

    /// The six planes, left, right, bottom, top, near, far, for a GPU copy of the tests below.
    pub fn planes(&self) -> [Vec4; 6] {
        self.planes
    }

    /// Positive-vertex test, which `draw_cull.slang` copies: outside when the AABB's most favourable corner is behind any plane.
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.test_planes(min, max, &self.planes)
    }

    /// Caster test without the near plane: casters between light and cascade box still shadow it.
    pub fn intersects_aabb_as_caster(&self, min: Vec3, max: Vec3) -> bool {
        let [left, right, bottom, top, _near, far] = &self.planes;
        self.test_planes(min, max, &[*left, *right, *bottom, *top, *far])
    }

    fn test_planes(&self, min: Vec3, max: Vec3, planes: &[Vec4]) -> bool {
        for plane in planes {
            let p = Vec3::new(
                if plane.x >= 0.0 { max.x } else { min.x },
                if plane.y >= 0.0 { max.y } else { min.y },
                if plane.z >= 0.0 { max.z } else { min.z },
            );
            if plane.truncate().dot(p) + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    // glam's `directx` clip space (NDC Z 0..1, Y up) is wgpu's and matches the old `Mat4::*_rh`.
    use glam::camera::rh::proj::directx as clip;
    use glam::camera::rh::view::look_at_mat4;

    use super::*;

    #[test]
    fn caster_test_ignores_the_near_plane_and_nothing_else() {
        // Between the light and the box only the caster test passes; beside the box both fail.
        let light = clip::orthographic(-1.0, 1.0, -1.0, 1.0, 0.0, 2.0)
            * look_at_mat4(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO, Vec3::Y);

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
        assert_eq!(frustum.planes()[4], light.row(2), "the near plane is row 2");
    }
}
