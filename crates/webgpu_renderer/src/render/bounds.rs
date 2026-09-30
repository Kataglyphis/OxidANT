//! Pure-CPU bounds and frustum-culling geometry.
//! See `../../docs/renderer-bounds-invariant.md` § The rule

use glam::{Mat4, Vec3};

use crate::render::forward::MAX_JOINTS;
use crate::scene::{CpuScene, CpuSkin};

/// View frustum from a wgpu (0..1 depth) view-projection; Gribb-Hartmann planes, normals inward.
pub(crate) struct Frustum {
    planes: [glam::Vec4; 6],
}

impl Frustum {
    pub(crate) fn from_view_proj(m: &Mat4) -> Self {
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

    /// Positive-vertex test: outside when the AABB's most favourable corner is behind any plane.
    pub(crate) fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.test_planes(min, max, &self.planes)
    }

    /// Caster test without the near plane: casters between light and cascade box still shadow it.
    /// Only tests call it yet; the cascade pass still culls casters with [`Self::intersects_aabb`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn intersects_aabb_as_caster(&self, min: Vec3, max: Vec3) -> bool {
        let [left, right, bottom, top, _near, far] = &self.planes;
        self.test_planes(min, max, &[*left, *right, *bottom, *top, *far])
    }

    fn test_planes(&self, min: Vec3, max: Vec3, planes: &[glam::Vec4]) -> bool {
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

/// Inverse-transpose of a model matrix, identity when singular.
/// Zero-scale nodes (how exporters hide objects) are common; a NaN normal matrix shades garbage.
pub(crate) fn normal_matrix_of(model: Mat4) -> Mat4 {
    let inv = model.inverse();
    if inv.is_finite() {
        inv.transpose()
    } else {
        Mat4::IDENTITY
    }
}

/// True when `p` is inside the AABB grown by the occlusion proxy's margin (`occlusion_bbox.wgsl`).
/// The GPU-culling path needs it at draw-skip time; `OcclusionQueries::record` has it built in.
pub(crate) fn aabb_contains_point(min: Vec3, max: Vec3, p: Vec3) -> bool {
    crate::render::occlusion::aabb_contains(
        min,
        max,
        p,
        crate::render::occlusion::CONTAINMENT_MARGIN,
    )
}

/// Bounds covering every instance of `pre`; empty `instances` means one identity instance.
/// See `../../docs/renderer-bounds-invariant.md` § How to be conservative correctly
pub(crate) fn instanced_bounds(pre: (Vec3, Vec3), instances: &[Mat4]) -> (Vec3, Vec3) {
    if instances.is_empty() {
        return pre;
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for m in instances {
        let (lo, hi) = transform_aabb(*m, pre.0, pre.1);
        min = min.min(lo);
        max = max.max(hi);
    }
    (min, max)
}

/// Widens world bounds to every pose the skin can reach by unioning the per-joint boxes.
/// The node-derived box stays in the union because zero-weight vertices still use the model matrix.
pub(crate) fn widen_bounds_for_skin(
    bounds: (Vec3, Vec3),
    local_min: Vec3,
    local_max: Vec3,
    skin: &CpuSkin,
    world: &[Mat4],
) -> (Vec3, Vec3) {
    let (mut min, mut max) = bounds;
    for (i, &joint_node) in skin.joints.iter().take(MAX_JOINTS).enumerate() {
        let jw = world.get(joint_node).copied().unwrap_or(Mat4::IDENTITY);
        let ib = skin
            .inverse_bind_matrices
            .get(i)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let (lo, hi) = transform_aabb(jw * ib, local_min, local_max);
        min = min.min(lo);
        max = max.max(hi);
    }
    (min, max)
}

pub(crate) fn primitive_world_aabb(prim: &crate::scene::CpuPrimitive) -> (Vec3, Vec3) {
    // Morphed primitives transform their pose-covering local box; the rest keep a per-vertex fit.
    if !prim.morph_targets.is_empty() {
        let (lo, hi) = primitive_local_aabb(prim);
        return transform_aabb(prim.transform, lo, hi);
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for vertex in &prim.vertices {
        let world = prim
            .transform
            .transform_point3(Vec3::from_array(vertex.position));
        // A non-finite vertex or transform must never reach the cascade fitting.
        if !world.is_finite() {
            continue;
        }
        min = min.min(world);
        max = max.max(world);
    }
    if min.x > max.x {
        (Vec3::ZERO, Vec3::ZERO)
    } else {
        (min, max)
    }
}

/// Local-space bounds covering every morph pose the primitive can reach (exact for weights 0..1).
pub(crate) fn primitive_local_aabb(prim: &crate::scene::CpuPrimitive) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (i, vertex) in prim.vertices.iter().enumerate() {
        let p = Vec3::from_array(vertex.position);
        // One NaN vertex would NaN the scene bounds and every cascade matrix, so skip it.
        if !p.is_finite() {
            continue;
        }
        let (mut lo, mut hi) = (p, p);
        for target in &prim.morph_targets {
            if let Some(d) = target.position_deltas.get(i) {
                lo += d.min(Vec3::ZERO);
                hi += d.max(Vec3::ZERO);
            }
        }
        min = min.min(lo);
        max = max.max(hi);
    }
    if min.x > max.x {
        (Vec3::ZERO, Vec3::ZERO)
    } else {
        (min, max)
    }
}

pub(crate) fn transform_aabb(m: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ];
    let mut out_min = Vec3::splat(f32::INFINITY);
    let mut out_max = Vec3::splat(f32::NEG_INFINITY);
    for corner in corners {
        let p = m.transform_point3(corner);
        out_min = out_min.min(p);
        out_max = out_max.max(p);
    }
    (out_min, out_max)
}

pub(crate) fn compute_world_bounds(scene: &CpuScene) -> Option<(Vec3, Vec3)> {
    // Per-primitive AABBs, not raw vertices, so the cascades fit the morphed pose actually drawn.
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for prim in &scene.primitives {
        if prim.vertices.is_empty() {
            continue;
        }
        let (lo, hi) = primitive_world_aabb(prim);
        bounds = Some(match bounds {
            None => (lo, hi),
            Some((min, max)) => (min.min(lo), max.max(hi)),
        });
    }
    bounds
}

#[cfg(test)]
mod tests {
    // glam's `directx` clip space (NDC Z 0..1, Y up) is wgpu's and matches the old `Mat4::*_rh`.
    use glam::camera::rh::proj::directx as clip;
    use glam::camera::rh::view::look_at_mat4;

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
    }
    use super::*;
    use crate::scene::camera::OrbitCamera;

    #[test]
    fn a_singular_model_matrix_yields_a_finite_normal_matrix() {
        // Zero scale on an axis is how Blender hides an object.
        let squashed = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
        assert!(
            !squashed.inverse().is_finite(),
            "precondition: inverse is not finite"
        );
        assert!(
            normal_matrix_of(squashed).is_finite(),
            "guard must return something finite"
        );
        // A well-formed matrix must still get the real inverse-transpose.
        let ok = Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let expected = ok.inverse().transpose();
        assert!((normal_matrix_of(ok) - expected).abs_diff_eq(Mat4::ZERO, 1e-6));
    }

    #[test]
    fn a_non_finite_vertex_cannot_poison_the_bounds() {
        // One bad vertex would otherwise break shadows for every object in the scene.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf");
        let scene = crate::load_gltf(path).expect("cube.gltf must load");
        let mut prim = scene.primitives[0].clone();
        prim.vertices[0].position = [f32::NAN, 1.0, f32::INFINITY];

        let (min, max) = primitive_local_aabb(&prim);
        assert!(
            min.is_finite() && max.is_finite(),
            "local bounds must stay finite, got {min:?}..{max:?}"
        );
        let (wmin, wmax) = primitive_world_aabb(&prim);
        assert!(
            wmin.is_finite() && wmax.is_finite(),
            "world bounds must stay finite, got {wmin:?}..{wmax:?}"
        );
        // And the surviving vertices must still define a real box.
        assert!(
            max.x > min.x,
            "the good vertices must still bound something"
        );
    }

    #[test]
    fn aabb_contains_point_matches_the_occlusion_proxy_margin() {
        // Must match occlusion_bbox.wgsl's `half * 0.02 + 0.01` growth or it guards another volume.
        let (min, max) = (Vec3::splat(-0.5), Vec3::splat(0.5));
        let margin = 0.5 * 0.02 + 0.01; // half-extent 0.5

        assert!(
            aabb_contains_point(min, max, Vec3::ZERO),
            "centre is inside"
        );
        // Just inside the expanded face.
        let inside = 0.5 + margin - 1e-4;
        assert!(aabb_contains_point(min, max, Vec3::new(inside, 0.0, 0.0)));
        // Just outside it.
        let outside = 0.5 + margin + 1e-3;
        assert!(!aabb_contains_point(min, max, Vec3::new(outside, 0.0, 0.0)));
        // Must hold on every axis, not just x.
        assert!(!aabb_contains_point(min, max, Vec3::new(0.0, 0.0, outside)));
    }

    #[test]
    fn morph_targets_expand_the_culling_bounds() {
        // cube_morph.gltf: the unit cube plus one target lifting every vertex +1.0 in Y.
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube_morph.gltf");
        let scene = crate::load_gltf(path).expect("cube_morph.gltf must load");
        let prim = &scene.primitives[0];
        assert_eq!(prim.morph_targets.len(), 1, "fixture must carry a target");

        let (min, max) = primitive_local_aabb(prim);
        assert!(
            (max.y - 1.5).abs() < 1e-5,
            "bounds must reach the fully-morphed pose (0.5 + 1.0), got max.y={}",
            max.y
        );
        // The +Y target has only positive deltas, so the lower bound must not move.
        assert!(
            (min.y + 0.5).abs() < 1e-5,
            "unmorphed extent must be preserved, got min.y={}",
            min.y
        );
        // X/Z are untouched by the target and must stay exactly the cube's.
        assert!((max.x - 0.5).abs() < 1e-5 && (min.x + 0.5).abs() < 1e-5);
    }

    #[test]
    fn frustum_culls_out_of_view_aabbs() {
        let camera = OrbitCamera::default();
        let frustum = Frustum::from_view_proj(&camera.view_projection(1.0));

        // Cube at the orbit target is visible.
        assert!(frustum.intersects_aabb(Vec3::splat(-0.5), Vec3::splat(0.5)));
        // A cube far off to the side is culled.
        assert!(
            !frustum.intersects_aabb(Vec3::new(1000.0, -0.5, -0.5), Vec3::new(1001.0, 0.5, 0.5))
        );
        // Behind the camera is culled.
        let eye = camera.eye();
        let behind = eye + (eye - Vec3::ZERO).normalize() * 10.0;
        assert!(!frustum.intersects_aabb(behind - Vec3::splat(0.4), behind + Vec3::splat(0.4)));
    }
}
