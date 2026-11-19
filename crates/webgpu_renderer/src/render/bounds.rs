//! Pure-CPU bounds and frustum-culling geometry.
//! See `../../docs/renderer-bounds-invariant.md` § The rule

use glam::{Mat4, Vec3};

use crate::scene::{CpuScene, CpuSkin, MorphTarget, Vertex};

// The frustum tests moved to kataglyphis_render_core, which shares its verdicts with the C++ engine.
pub(crate) use kataglyphis_render_core::frustum::Frustum;

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
    for (i, &joint_node) in skin.joints.iter().enumerate() {
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

/// Local-space bounds covering the morph poses its current weights allow; see [`morph_local_aabb`].
pub(crate) fn primitive_local_aabb(prim: &crate::scene::CpuPrimitive) -> (Vec3, Vec3) {
    morph_local_aabb(&prim.vertices, &prim.morph_targets, &prim.morph_weights)
}

/// Bounds over each target's weights from `min(0, w)` to `max(1, w)`, `w` its current weight.
/// Exact over [0, 1]; a default above 1 or a CUBICSPLINE overshoot widens that target's range.
pub(crate) fn morph_local_aabb(
    vertices: &[Vertex],
    targets: &[MorphTarget],
    weights: &[f32],
) -> (Vec3, Vec3) {
    let ranges: Vec<(f32, f32)> = (0..targets.len())
        .map(|t| {
            let w = weights
                .get(t)
                .copied()
                .filter(|w| w.is_finite())
                .unwrap_or(0.0);
            (w.min(0.0), w.max(1.0))
        })
        .collect();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (i, vertex) in vertices.iter().enumerate() {
        let p = Vec3::from_array(vertex.position);
        let (mut lo, mut hi) = (p, p);
        for (target, &(w_lo, w_hi)) in targets.iter().zip(&ranges) {
            if let Some(&d) = target.position_deltas.get(i) {
                lo += (d * w_lo).min(d * w_hi);
                hi += (d * w_lo).max(d * w_hi);
            }
        }
        // One NaN vertex or delta would NaN the scene bounds and every cascade matrix, so skip it.
        if !lo.is_finite() || !hi.is_finite() {
            continue;
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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/tests/assets/cube.gltf");
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
    fn morph_targets_expand_the_culling_bounds() {
        // cube_morph.gltf: the unit cube plus one target lifting every vertex +1.0 in Y.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/tests/assets/cube_morph.gltf");
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
