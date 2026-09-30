//! Pure-CPU shadow-cascade fitting, texel-stabilized like `CascadedShadowMapMath.cpp`.
//! Splits are eye distances: `forward.slang:151` selects by `distance(worldPos, camera_position)`.

// glam's `directx` clip convention is NDC Z in [0,1] with Y up, the same as wgpu's.
use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::render::forward::{CASCADE_COUNT, SHADOW_MAP_SIZE};
use crate::scene::camera::OrbitCamera;

pub(crate) struct CascadeFit {
    pub splits: [f32; 2],
    pub matrices: [Mat4; CASCADE_COUNT],
}

/// Fits one ortho light matrix per cascade: camera focus, orbit target, whole scene.
pub(crate) fn fit_cascades(
    camera: &OrbitCamera,
    scene_min: Vec3,
    scene_max: Vec3,
    light_dir: Vec3,
) -> CascadeFit {
    let scene_center = (scene_min + scene_max) * 0.5;
    let mut scene_radius = ((scene_max - scene_min).length() * 0.5).max(1e-3);
    if !scene_radius.is_finite() {
        scene_radius = 1.0;
    }

    // Floored: `viewDepth` is interpolated per vertex, and smaller boxes miss its error.
    let mut raw_dist = (scene_center - camera.eye()).length().max(scene_radius);
    if !raw_dist.is_finite() {
        raw_dist = scene_radius;
    }
    // Quantized: a continuous size re-scales the texel grid every dolly, defeating the snap.
    let mut steps = (raw_dist / scene_radius).log2().ceil().max(0.0);
    if !steps.is_finite() {
        steps = 0.0;
    }
    let dist_to_center = scene_radius * steps.exp2();

    let near_radius = (dist_to_center * 0.35).max(0.5);
    let mid_radius = (dist_to_center * 0.7).max(1.0);
    let splits = [near_radius * 2.0, mid_radius * 2.0];

    let focus_near = camera.target.lerp(camera.eye(), 0.15);
    let cascades = [
        (focus_near, near_radius),
        (camera.target, mid_radius),
        (scene_center, scene_radius),
    ];

    let light_dir = light_dir.normalize_or_zero();
    let light_dir = if light_dir == Vec3::ZERO {
        Vec3::Y
    } else {
        light_dir
    };
    let up = if light_dir.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    // Eye at +light_dir, not -: ours points toward the light, the C++ `lightDir` along the ray.
    let light_basis = look_at_mat4(light_dir, Vec3::ZERO, up);

    let mut matrices = [Mat4::IDENTITY; CASCADE_COUNT];
    for (i, (center, radius)) in cascades.into_iter().enumerate() {
        matrices[i] = stabilized_light_matrix_for(light_basis, center, radius, SHADOW_MAP_SIZE);
    }

    CascadeFit { splits, matrices }
}

/// One cascade's ortho light matrix for a sphere, snapped to whole texels in `light_basis`.
/// The basis must be world-fixed: one anchored at a moving center absorbs motion no snap undoes.
fn stabilized_light_matrix_for(
    light_basis: Mat4,
    center: Vec3,
    radius: f32,
    shadow_map_size: u32,
) -> Mat4 {
    // Pad by one projected texel so snap and projection share a grid; size <= 2 cannot be padded.
    let (half_extent, texel_world) = if shadow_map_size > 2 {
        let half_extent = radius * shadow_map_size as f32 / (shadow_map_size as f32 - 2.0);
        let texel_world = (2.0 * half_extent) / shadow_map_size as f32;
        (half_extent, texel_world)
    } else {
        (radius, 0.0)
    };
    let center_ls = light_basis.transform_point3(center);
    let (snapped_x, snapped_y) = if texel_world > 0.0 {
        (
            (center_ls.x / texel_world).floor() * texel_world,
            (center_ls.y / texel_world).floor() * texel_world,
        )
    } else {
        (center_ls.x, center_ls.y)
    };

    // Not sphere-tight: every caster reaches every cascade's pass, so a tight range clips some.
    let depth_pad = radius * 4.0;
    let near = -center_ls.z - depth_pad;
    let mut far = -center_ls.z + depth_pad;
    if far <= near {
        far = near + 1.0;
    }

    let projection = clip::orthographic(
        snapped_x - half_extent,
        snapped_x + half_extent,
        snapped_y - half_extent,
        snapped_y + half_extent,
        near,
        far,
    );
    projection * light_basis
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoomed_out_camera() -> OrbitCamera {
        OrbitCamera {
            radius: 200.0,
            ..OrbitCamera::default()
        }
    }

    #[test]
    fn splits_track_the_camera_instead_of_staying_scene_fixed() {
        // Scene-fixed splits leave a far camera's every fragment in the last cascade.
        let scene_min = Vec3::splat(-1.0);
        let scene_max = Vec3::splat(1.0);
        let light_dir = Vec3::Y;

        let near_fit = fit_cascades(&OrbitCamera::default(), scene_min, scene_max, light_dir);
        let far_fit = fit_cascades(&zoomed_out_camera(), scene_min, scene_max, light_dir);

        assert!(
            far_fit.splits[0] > near_fit.splits[0] * 10.0,
            "far camera's first split ({}) must grow with eye distance, not stay near the close camera's ({})",
            far_fit.splits[0],
            near_fit.splits[0]
        );
        assert!(
            far_fit.splits[1] > far_fit.splits[0],
            "splits must stay ordered: {:?}",
            far_fit.splits
        );
    }

    #[test]
    fn a_camera_inside_the_scene_radius_matches_the_original_scene_radius_derived_split() {
        // The GPU golden tests' cameras sit inside the scene radius and rely on this exact sizing.
        let scene_min = Vec3::new(-4.0, 0.0, -4.0);
        let scene_max = Vec3::new(4.0, 1.4, 4.0);
        let camera = OrbitCamera {
            radius: 6.0,
            pitch_deg: 55.0,
            ..OrbitCamera::default()
        };
        let scene_center = (scene_min + scene_max) * 0.5;
        let scene_radius = (scene_max - scene_min).length() * 0.5;
        assert!(
            (scene_center - camera.eye()).length() < scene_radius,
            "precondition: this camera must sit inside the scene's own radius"
        );

        let fit = fit_cascades(&camera, scene_min, scene_max, Vec3::new(-1.0, 0.7, -0.3));

        let expected_near = (scene_radius * 0.35).max(0.5);
        let expected_mid = (scene_radius * 0.7).max(1.0);
        assert!(
            (fit.splits[0] - expected_near * 2.0).abs() < 1e-3,
            "got {}, expected {}",
            fit.splits[0],
            expected_near * 2.0
        );
        assert!(
            (fit.splits[1] - expected_mid * 2.0).abs() < 1e-3,
            "got {}, expected {}",
            fit.splits[1],
            expected_mid * 2.0
        );
    }

    #[test]
    fn cascade_matrices_stay_finite_and_non_degenerate() {
        let scene_min = Vec3::splat(-1.0);
        let scene_max = Vec3::splat(1.0);
        for camera in [OrbitCamera::default(), zoomed_out_camera()] {
            let fit = fit_cascades(&camera, scene_min, scene_max, Vec3::new(-0.55, -1.0, -0.35));
            for (i, m) in fit.matrices.iter().enumerate() {
                assert!(m.is_finite(), "cascade {i} matrix is not finite: {m:?}");
                assert!(
                    m.determinant().abs() > 1e-12,
                    "cascade {i} matrix collapsed"
                );
            }
        }
    }

    /// `world` in `view_proj`'s texel space, where one shadow-map texel is exactly 1.0.
    fn texel_space(view_proj: Mat4, world: Vec3) -> (f32, f32) {
        let clip = view_proj * world.extend(1.0);
        (
            (clip.x / clip.w) * (SHADOW_MAP_SIZE as f32 / 2.0),
            (clip.y / clip.w) * (SHADOW_MAP_SIZE as f32 / 2.0),
        )
    }

    #[test]
    fn cascades_shift_by_whole_texels_under_camera_motion() {
        // Shimmer guard: a static point may only shift by whole texels as the camera moves.
        let scene_min = Vec3::splat(-1.0);
        let scene_max = Vec3::splat(1.0);
        let light_dir = Vec3::new(-0.4, -1.0, -0.2);
        let probe = Vec3::new(0.2, 0.3, 0.1);

        let camera_a = OrbitCamera::default();
        // Well under one near-cascade texel (~1.4e-3 world units here).
        let camera_b = OrbitCamera {
            target: camera_a.target + Vec3::new(4e-4, 0.0, 0.0),
            ..camera_a.clone()
        };

        let fit_a = fit_cascades(&camera_a, scene_min, scene_max, light_dir);
        let fit_b = fit_cascades(&camera_b, scene_min, scene_max, light_dir);

        let (ax, ay) = texel_space(fit_a.matrices[0], probe);
        let (bx, by) = texel_space(fit_b.matrices[0], probe);
        let (dx, dy) = (ax - bx, ay - by);
        assert!(
            (dx - dx.round()).abs() < 5e-2,
            "x moved by a fractional texel: {dx}"
        );
        assert!(
            (dy - dy.round()).abs() < 5e-2,
            "y moved by a fractional texel: {dy}"
        );
    }

    #[test]
    fn cascades_stay_texel_aligned_over_long_camera_travel() {
        // Far travel at fixed radius: an unpadded texel_world would drift past the tolerance.
        let light_dir = Vec3::new(-0.4, -1.0, -0.2).normalize();
        let up = if light_dir.dot(Vec3::Y).abs() > 0.99 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let light_basis = look_at_mat4(light_dir, Vec3::ZERO, up);
        let radius = 1.4_f32;
        let probe = Vec3::new(0.2, 0.3, 0.1);

        let center_a = Vec3::ZERO;
        let center_b = center_a + Vec3::new(0.5, 0.0, 0.0);

        let matrix_a = stabilized_light_matrix_for(light_basis, center_a, radius, SHADOW_MAP_SIZE);
        let matrix_b = stabilized_light_matrix_for(light_basis, center_b, radius, SHADOW_MAP_SIZE);

        let (ax, ay) = texel_space(matrix_a, probe);
        let (bx, by) = texel_space(matrix_b, probe);
        let (dx, dy) = (ax - bx, ay - by);
        assert!(
            (dx - dx.round()).abs() < 5e-2,
            "x drifted off whole-texel alignment: {dx}"
        );
        assert!(
            (dy - dy.round()).abs() < 5e-2,
            "y drifted off whole-texel alignment: {dy}"
        );
    }

    /// Whether `world` lands inside the box, per `forward.slang:212`'s `uv`/`proj.z` check.
    fn shader_covers(view_proj: Mat4, world: Vec3) -> bool {
        let clip = view_proj * world.extend(1.0);
        if clip.w.abs() < 1e-6 {
            return false;
        }
        let proj = clip.truncate() / clip.w;
        let u = proj.x * 0.5 + 0.5;
        let v = 0.5 - proj.y * 0.5;
        (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) && (0.0..=1.0).contains(&proj.z)
    }

    /// `forward.slang:198-203`'s cascade selection rule, in isolation.
    fn select_cascade(eye_distance: f32, splits: [f32; 2]) -> usize {
        let mut cascade = 0usize;
        if eye_distance > splits[0] {
            cascade = 1;
        }
        if eye_distance > splits[1] {
            cascade = 2;
        }
        cascade.min(CASCADE_COUNT - 1)
    }

    #[test]
    fn every_selectable_eye_distance_lands_inside_the_cascade_it_selects() {
        // Near-eye points can miss cascade 0 (it hugs the focus); the shader retries coarser ones.
        let scene_min = Vec3::new(-4.0, 0.0, -4.0);
        let scene_max = Vec3::new(4.0, 1.4, 4.0);
        let light_dir = Vec3::new(-1.0, -0.3, -1.0);
        let camera = OrbitCamera {
            radius: 6.0,
            pitch_deg: 5.0,
            ..OrbitCamera::default()
        };

        let fit = fit_cascades(&camera, scene_min, scene_max, light_dir);
        let eye = camera.eye();
        let view_dir = (camera.target - eye).normalize();

        let lo = 0.05 * fit.splits[0];
        let hi = 1.5 * fit.splits[1];
        const STEPS: u32 = 64;

        let mut uncovered = Vec::new();
        for i in 0..=STEPS {
            let t = i as f32 / STEPS as f32;
            let eye_distance = lo + t * (hi - lo);
            let world = eye + view_dir * eye_distance;
            let selected = select_cascade(eye_distance, fit.splits);

            let covered_by_fallback =
                (selected..CASCADE_COUNT).any(|c| shader_covers(fit.matrices[c], world));
            if !covered_by_fallback {
                uncovered.push((eye_distance, selected));
            }
        }

        assert!(
            uncovered.is_empty(),
            "no cascade from the selected one through {} covers the fragment for {} of {} \
             sampled eye distances (eye_distance, selected cascade): {:?}",
            CASCADE_COUNT - 1,
            uncovered.len(),
            STEPS + 1,
            uncovered
        );
    }

    #[test]
    fn cascade_box_size_is_invariant_under_camera_motion() {
        // Turning inside the scene radius must not resize the box, or the texel size breathes.
        let scene_min = Vec3::splat(-50.0);
        let scene_max = Vec3::splat(50.0);
        let light_dir = Vec3::new(-0.3, -1.0, 0.2);

        let camera_a = OrbitCamera {
            radius: 5.0,
            yaw_deg: 10.0,
            pitch_deg: 15.0,
            ..OrbitCamera::default()
        };
        let camera_b = OrbitCamera {
            radius: 5.0,
            yaw_deg: 200.0,
            pitch_deg: 60.0,
            ..OrbitCamera::default()
        };

        let fit_a = fit_cascades(&camera_a, scene_min, scene_max, light_dir);
        let fit_b = fit_cascades(&camera_b, scene_min, scene_max, light_dir);

        // Upper-3x3 row norms are the ortho scales, since light_basis is a pure rotation.
        let row_norm =
            |m: &Mat4, row: usize| Vec3::new(m.x_axis[row], m.y_axis[row], m.z_axis[row]).length();

        for i in 0..CASCADE_COUNT {
            for row in 0..2 {
                let a = row_norm(&fit_a.matrices[i], row);
                let b = row_norm(&fit_b.matrices[i], row);
                assert!(
                    (a - b).abs() < 1e-4,
                    "cascade {i} row {row} scale breathes with camera motion: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn cascade_box_size_is_invariant_under_small_camera_dolly() {
        // A small dolly outside the floor must stay on one quantized zoom step.
        let scene_min = Vec3::splat(-1.0);
        let scene_max = Vec3::splat(1.0);
        let light_dir = Vec3::new(-0.3, -1.0, 0.2);

        let camera_a = OrbitCamera {
            radius: 50.0,
            ..OrbitCamera::default()
        };
        let camera_b = OrbitCamera {
            radius: 50.3,
            ..OrbitCamera::default()
        };

        let fit_a = fit_cascades(&camera_a, scene_min, scene_max, light_dir);
        let fit_b = fit_cascades(&camera_b, scene_min, scene_max, light_dir);

        let row_norm =
            |m: &Mat4, row: usize| Vec3::new(m.x_axis[row], m.y_axis[row], m.z_axis[row]).length();

        for i in 0..CASCADE_COUNT {
            for row in 0..2 {
                let a = row_norm(&fit_a.matrices[i], row);
                let b = row_norm(&fit_b.matrices[i], row);
                assert!(
                    (a - b).abs() < 1e-4,
                    "cascade {i} row {row} scale breathes with a small camera dolly: {a} vs {b}"
                );
            }
        }
    }
}
