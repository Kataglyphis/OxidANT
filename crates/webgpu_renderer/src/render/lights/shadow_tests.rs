//! The punctual cube shadows' CPU half, as the C++ `PunctualShadowUnit` suite checks its twin.

use glam::{Mat4, Vec3, Vec4};

use super::*;

fn light(kind: CpuLightKind, position: [f32; 3], range: f32) -> CpuLight {
    CpuLight {
        kind,
        color: [1.0; 3],
        intensity: 1.0,
        range,
        position,
        direction: [0.0, -1.0, 0.0],
        node: None,
    }
}

fn spot() -> CpuLightKind {
    CpuLightKind::Spot {
        cos_inner: 0.9,
        cos_outer: 0.5,
    }
}

/// The cube-map selection table: the face a direction picks and its (sc, tc, ma).
fn cube_lookup(r: Vec3) -> (usize, f32, f32, f32) {
    let a = r.abs();
    if a.x >= a.y && a.x >= a.z {
        return if r.x > 0.0 {
            (0, -r.z, -r.y, a.x)
        } else {
            (1, r.z, -r.y, a.x)
        };
    }
    if a.y >= a.z {
        return if r.y > 0.0 {
            (2, r.x, r.z, a.y)
        } else {
            (3, r.x, -r.z, a.y)
        };
    }
    if r.z > 0.0 {
        (4, r.x, -r.y, a.z)
    } else {
        (5, -r.x, -r.y, a.z)
    }
}

fn ndc(m: Mat4, p: Vec3) -> Vec3 {
    let clip = m * p.extend(1.0);
    clip.truncate() / clip.w
}

#[test]
fn a_point_on_each_axis_projects_to_its_faces_centre() {
    let slot = PunctualShadowSlot {
        position: Vec3::new(1.0, 2.0, -3.0),
        near: 0.05,
        far: 40.0,
    };
    let faces = punctual_shadow_face_matrices(&slot);
    let axes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    for (face, axis) in axes.into_iter().enumerate() {
        let p = ndc(faces[face], slot.position + axis * 5.0);
        assert!(p.x.abs() < 1e-5 && p.y.abs() < 1e-5, "face {face}: {p}");
        assert!((0.0..=1.0).contains(&p.z), "face {face}: depth {}", p.z);
        assert_eq!(cube_lookup(axis).0, face, "layer order");
    }
}

/// The shader picks the layer and uv from the table, so each face's NDC must be the table's (sc, tc) / |ma|.
#[test]
fn face_matrices_follow_the_cube_map_selection_table() {
    let slot = PunctualShadowSlot {
        position: Vec3::new(1.0, 2.0, -3.0),
        near: 0.05,
        far: 40.0,
    };
    let faces = punctual_shadow_face_matrices(&slot);
    let directions = [
        Vec3::new(3.0, 0.5, -1.0),
        Vec3::new(-4.0, 1.0, 2.0),
        Vec3::new(0.3, 5.0, -0.7),
        Vec3::new(1.1, -6.0, 0.4),
        Vec3::new(-0.5, 0.9, 2.5),
        Vec3::new(0.8, -1.2, -3.0),
        Vec3::new(10.0, 9.0, 0.0),
        Vec3::new(-2.0, -2.0, -1.9),
        Vec3::new(0.01, 0.02, 30.0),
    ];
    for d in directions {
        let (face, sc, tc, ma) = cube_lookup(d);
        let p = ndc(faces[face], slot.position + d);
        assert!((p.x - sc / ma).abs() < 1e-4, "face {face}: x {p}");
        assert!((p.y - tc / ma).abs() < 1e-4, "face {face}: y {p}");
        let reference = punctual_shadow_depth(ma, slot.near, slot.far);
        assert!(
            (p.z - reference).abs() < 1e-5,
            "the shader's reference depth disagrees with face {face}: {} vs {reference}",
            p.z
        );
    }
}

#[test]
fn the_depth_formula_round_trips_through_the_projection() {
    let (near, far) = (0.02, 25.0);
    assert!(punctual_shadow_depth(near, near, far).abs() < 1e-6);
    assert!((punctual_shadow_depth(far, near, far) - 1.0).abs() < 1e-6);
    let projection =
        glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    for distance in [0.05f32, 0.5, 1.0, 3.7, 12.0, 24.9] {
        let clip = projection * Vec4::new(0.0, 0.0, -distance, 1.0);
        let depth = punctual_shadow_depth(distance, near, far);
        assert!((clip.z / clip.w - depth).abs() < 1e-5, "{distance}");
        // Inverting the stored depth gives the distance back: the formula is the projection's z / w.
        let back = far * near / (far - depth * (far - near));
        assert!(
            (back - distance).abs() < distance * 1e-3,
            "{distance}: {back}"
        );
    }
}

#[test]
fn the_first_point_and_spot_lights_get_cubes_and_directional_ones_none() {
    let lights = [
        light(CpuLightKind::Directional, [0.0; 3], 0.0),
        light(CpuLightKind::Point, [1.0, 0.0, 0.0], 5.0),
        light(spot(), [0.0, 3.0, 0.0], 0.0),
        light(CpuLightKind::Point, [2.0, 0.0, 0.0], 6.0),
        light(CpuLightKind::Point, [3.0, 0.0, 0.0], 7.0),
        light(CpuLightKind::Point, [4.0, 0.0, 0.0], 8.0),
    ];
    let (mut packed, count) = pack_punctual_lights(&lights);
    let bounds = Some((Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 1.0, 10.0)));
    let slots = assign_punctual_shadow_slots(&mut packed, count, bounds);

    assert_eq!(slots.len(), MAX_SHADOWED_PUNCTUAL_LIGHTS);
    let row = |light: usize| packed[light * 4 + 3];
    assert_eq!(row(0)[1], 0.0, "a directional light has no cube");
    for light in 1..=4 {
        assert_eq!(
            row(light)[1],
            light as f32,
            "light {light} holds slot {}",
            light - 1
        );
        assert_eq!(row(light)[2], slots[light - 1].near);
        assert_eq!(row(light)[3], slots[light - 1].far);
        assert!(slots[light - 1].near < slots[light - 1].far);
    }
    assert_eq!(row(2)[0], 0.5, "the slot must not clobber cos_outer");
    assert_eq!(row(5)[1], 0.0, "past the cap a light stays unshadowed");
    assert_eq!(slots[0].far, 5.0, "a ranged light's cube ends at its range");
    assert_eq!(
        slots[0].near, 0.01,
        "5e-3 clamps up to the nearest near plane"
    );
    assert_eq!(slots[0].position, Vec3::X);
    // The spot light at y 3 has no range: its far plane must reach the scene's farthest corner.
    assert!(slots[1].far >= Vec3::new(10.0, 4.0, 10.0).length());
}

#[test]
fn near_is_a_thousandth_of_far_within_its_clamp() {
    let lights = [
        light(CpuLightKind::Point, [0.0; 3], 100.0),
        light(CpuLightKind::Point, [0.0; 3], 2000.0),
    ];
    let (mut packed, count) = pack_punctual_lights(&lights);
    let slots = assign_punctual_shadow_slots(&mut packed, count, None);
    assert!((slots[0].near - 0.1).abs() < 1e-6);
    assert_eq!(slots[1].near, 0.5, "the clamp's top");
}

#[test]
fn assignment_clears_a_stale_slot_past_the_cap() {
    let lights: Vec<CpuLight> = (0..6)
        .map(|i| light(CpuLightKind::Point, [i as f32, 0.0, 0.0], 3.0))
        .collect();
    let (mut packed, count) = pack_punctual_lights(&lights);
    packed[5 * 4 + 3] = [0.0, 9.0, 1.0, 2.0];
    assign_punctual_shadow_slots(&mut packed, count, None);
    assert_eq!(packed[5 * 4 + 3], [0.0; 4], "a stale slot must not survive");
}

#[test]
fn an_unbounded_light_without_geometry_still_gets_a_depth_range() {
    assert_eq!(unbounded_shadow_far(Vec3::ZERO, None), 100.0);
    assert_eq!(
        unbounded_shadow_far(Vec3::ZERO, Some((Vec3::ONE, Vec3::NEG_ONE))),
        100.0,
        "an inverted box is no geometry"
    );
    assert!(unbounded_shadow_far(Vec3::ZERO, Some((Vec3::ZERO, Vec3::splat(0.1)))) >= 1.0);
    let far = unbounded_shadow_far(Vec3::ZERO, Some((Vec3::splat(-2.0), Vec3::splat(4.0))));
    assert!((far - Vec3::splat(4.0).length() * 1.05).abs() < 1e-5);
}
