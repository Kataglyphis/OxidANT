use super::*;

const ASPECT: f32 = 16.0 / 9.0;
const LIGHT: Vec3 = Vec3::new(0.55, 1.0, 0.35);

fn settings(count: u32, distance: f32, lambda: f32) -> ShadowSettings {
    ShadowSettings {
        cascade_count: count,
        distance,
        split_lambda: lambda,
        ..ShadowSettings::default()
    }
}

fn camera() -> OrbitCamera {
    OrbitCamera {
        radius: 6.0,
        pitch_deg: 30.0,
        ..OrbitCamera::default()
    }
}

/// `world` in NDC of `view_proj`.
fn ndc(view_proj: Mat4, world: Vec3) -> Vec3 {
    let clip = view_proj * world.extend(1.0);
    clip.truncate() / clip.w
}

/// `forward_lighting.slang`'s bounds check: inside the map and its depth range.
fn covers(view_proj: Mat4, world: Vec3) -> bool {
    let p = ndc(view_proj, world);
    let (u, v) = (p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) && (0.0..=1.0).contains(&p.z)
}

/// `forward_lighting.slang`'s selection: the first of `count` cascades whose split lies beyond the depth, else the last.
fn select(fit: &CascadeFit, count: usize, view_depth: f32) -> usize {
    let passed = fit.splits[..count - 1]
        .iter()
        .filter(|split| view_depth >= **split)
        .count();
    passed.min(count - 1)
}

/// A world point at view depth `depth` through NDC `(sx, sy)` of `camera`'s lens.
fn through_lens(camera: &OrbitCamera, (sx, sy): (f32, f32), depth: f32) -> Vec3 {
    let tan_half = (camera.fov_y_deg.to_radians() * 0.5).tan();
    let in_view = Vec3::new(
        sx * depth * tan_half * ASPECT,
        sy * depth * tan_half,
        -depth,
    );
    camera.view().inverse().transform_point3(in_view)
}

#[test]
fn splits_are_uniform_at_lambda_0_logarithmic_at_1_and_exact_at_both_ends() {
    let uniform = split_distances(3, (0.1, 1000.0), 60.0, 0.0);
    assert_eq!(uniform[0], 0.1);
    assert_eq!(uniform[3], 60.0, "the last split is exactly the distance");
    assert!(
        (uniform[1] - (0.1 + 59.9 / 3.0)).abs() < 1e-4,
        "{uniform:?}"
    );
    let log = split_distances(3, (0.1, 1000.0), 60.0, 1.0);
    let expected = 0.1 * 600.0_f32.powf(1.0 / 3.0);
    assert!((log[1] - expected).abs() < 1e-3, "{log:?}");
    assert_eq!(log[3], 60.0);
    let blended = split_distances(3, (0.1, 1000.0), 60.0, 0.5);
    assert!(log[1] < blended[1] && blended[1] < uniform[1]);
    // A distance past the far plane stops at it; one cascade spans the whole distance.
    assert_eq!(split_distances(3, (0.1, 40.0), 60.0, 0.0)[3], 40.0);
    let single = split_distances(1, (0.1, 1000.0), 60.0, 0.0);
    assert_eq!((single[0], single[1]), (0.1, 60.0));
    // A near plane beyond half the distance moves in, as the C++ `shadowNear`.
    assert_eq!(split_distances(2, (50.0, 1000.0), 60.0, 0.0)[0], 30.0);
}

#[test]
fn every_visible_point_within_the_distance_lies_in_the_cascade_it_selects() {
    let camera = camera();
    for count in 1..=3 {
        for lambda in [0.0, 0.5, 1.0] {
            let fit = fit_cascades(
                &CascadeLens::of(&camera, ASPECT),
                LIGHT,
                &settings(count, 60.0, lambda),
                None,
            );
            let mut missed = Vec::new();
            for step in 0..=40 {
                let depth = 0.2 + (60.0 - 0.3) * step as f32 / 40.0;
                for (sx, sy) in [(-0.99, -0.99), (0.99, 0.99), (-0.99, 0.99), (0.0, 0.0)] {
                    let world = through_lens(&camera, (sx, sy), depth);
                    let cascade = select(&fit, count as usize, depth);
                    if !covers(fit.matrices[cascade], world) {
                        missed.push((count, lambda, depth, cascade));
                    }
                }
            }
            assert!(missed.is_empty(), "uncovered: {missed:?}");
        }
    }
}

#[test]
fn the_colosseum_ground_does_not_spread_the_near_cascade_over_8_km() {
    let camera = OrbitCamera {
        target: Vec3::new(0.0, 0.0, 0.0),
        radius: 75.0,
        pitch_deg: 25.0,
        far: 9000.0,
        ..OrbitCamera::default()
    };
    let ground = (
        Vec3::new(-4000.0, -11.0, -4000.0),
        Vec3::new(4000.0, 59.0, 4000.0),
    );
    let fit = fit_cascades(
        &CascadeLens::of(&camera, ASPECT),
        LIGHT,
        &settings(3, 150.0, 0.0),
        Some(ground),
    );
    // Texels per world unit along the box's x: the ortho scale times half the resolution.
    let texel_m =
        |m: &Mat4| 2.0 / (Vec3::new(m.x_axis.x, m.y_axis.x, m.z_axis.x).length() * 2048.0);
    assert!(
        texel_m(&fit.matrices[0]) < 0.06,
        "{}",
        texel_m(&fit.matrices[0])
    );
    assert!(
        texel_m(&fit.matrices[2]) < 0.2,
        "{}",
        texel_m(&fit.matrices[2])
    );
    // The arena centre, 75 m ahead on the ground, is shadowed by the cascade it selects.
    let centre = camera.target;
    let depth = (centre - camera.eye()).dot(fit.view_dir);
    assert!(covers(fit.matrices[select(&fit, 3, depth)], centre));
}

#[test]
fn boxes_keep_their_size_and_move_by_whole_texels_under_camera_motion() {
    let at = |target: Vec3, yaw_deg: f32| {
        let camera = OrbitCamera {
            target,
            yaw_deg,
            ..camera()
        };
        fit_cascades(
            &CascadeLens::of(&camera, ASPECT),
            LIGHT,
            &settings(3, 60.0, 0.0),
            None,
        )
    };
    let a = at(Vec3::ZERO, 45.0);
    // Far under one cascade 0 texel (about 2 cm here), and a turn that moves every corner.
    let b = at(Vec3::new(3e-3, 0.0, -2e-3), 45.0);
    let c = at(Vec3::ZERO, 200.0);
    let scale =
        |m: &Mat4, row: usize| Vec3::new(m.x_axis[row], m.y_axis[row], m.z_axis[row]).length();
    let probe = Vec3::new(0.2, 0.3, 0.1);
    for cascade in 0..CASCADE_COUNT {
        for row in 0..2 {
            let size = scale(&a.matrices[cascade], row);
            assert!((size - scale(&b.matrices[cascade], row)).abs() < 1e-6 * size.max(1.0));
            assert!((size - scale(&c.matrices[cascade], row)).abs() < 1e-6 * size.max(1.0));
        }
        let texels = |m: Mat4| ndc(m, probe).truncate() * 1024.0;
        let shift = texels(a.matrices[cascade]) - texels(b.matrices[cascade]);
        assert!(
            (shift - shift.round()).abs().max_element() < 0.05,
            "cascade {cascade} moved by a fractional texel: {shift}"
        );
    }
}

#[test]
fn casters_between_the_light_and_a_slice_are_kept_and_the_bias_keeps_its_world_size() {
    let scene = (Vec3::new(-20.0, 0.0, -20.0), Vec3::new(20.0, 300.0, 20.0));
    let lens = CascadeLens::of(&camera(), ASPECT);
    let fit = fit_cascades(&lens, LIGHT, &settings(3, 60.0, 0.0), Some(scene));
    let unextended = fit_cascades(&lens, LIGHT, &settings(3, 60.0, 0.0), None);
    for cascade in 0..CASCADE_COUNT {
        for corner in 0..8u32 {
            let p = Vec3::select(
                glam::BVec3::new(corner & 1 != 0, corner & 2 != 0, corner & 4 != 0),
                scene.1,
                scene.0,
            );
            let z = ndc(fit.matrices[cascade], p).z;
            assert!(
                z >= 0.0,
                "cascade {cascade} clips the caster corner {p} at z {z}"
            );
        }
        // NDC per world unit along the light is the matrix's z scale; the scaled bias spans the same metres.
        let z_scale = |m: &Mat4| Vec3::new(m.x_axis.z, m.y_axis.z, m.z_axis.z).length();
        let world = |m: &Mat4, scale: f32| 0.0005 * scale / z_scale(m);
        let kept = world(&fit.matrices[cascade], fit.bias_scales[cascade]);
        let cpp = world(&unextended.matrices[cascade], 1.0);
        assert!(
            fit.bias_scales[cascade] < 1.0,
            "the 300 m tower extends cascade {cascade}"
        );
        assert!(
            (kept - cpp).abs() < 1e-3 * cpp,
            "{kept} m against the C++ {cpp} m"
        );
        assert_eq!(unextended.bias_scales[cascade], 1.0);
    }
}

#[test]
fn matrices_stay_finite_for_degenerate_inputs() {
    let nan_scene = (Vec3::splat(f32::NAN), Vec3::ONE);
    let lenses = [
        CascadeLens::of(&camera(), ASPECT),
        CascadeLens {
            fov_y_deg: 0.0,
            aspect: 0.0,
            far: f32::INFINITY,
            ..CascadeLens::of(&camera(), ASPECT)
        },
    ];
    for lens in lenses {
        for light in [Vec3::ZERO, Vec3::Y, Vec3::new(-0.55, -1.0, -0.35)] {
            let fit = fit_cascades(&lens, light, &settings(3, 60.0, 1.0), Some(nan_scene));
            for (i, m) in fit.matrices.iter().enumerate() {
                assert!(m.is_finite(), "cascade {i}: {m:?}");
                assert!(m.determinant().abs() > 1e-12, "cascade {i} collapsed");
            }
            assert!(fit.view_dir.is_finite());
        }
    }
}
