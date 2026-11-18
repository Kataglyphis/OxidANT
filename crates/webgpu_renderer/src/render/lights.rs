//! Packs punctual lights for `punctual_light.slang`, which the raster passes and the path tracer read; change both together.
//! Four vec4 rows per light: [pos, kind], [color * intensity, range], [dir, cos_inner], [cos_outer, slot + 1, near, far].
//! The shadow half is `PunctualLight.ixx`'s twin: the same slots, depth range and face matrices as the C++ engine.

// glam's `directx` projection is glm's perspectiveRH_ZO: NDC Z in [0, 1], Y up.
use glam::camera::rh::proj::directx as clip;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::scene::{CpuLight, CpuLightKind};

pub const MAX_PUNCTUAL_LIGHTS: usize = 256;

pub(crate) fn pack_punctual_lights(
    lights: &[CpuLight],
) -> ([[f32; 4]; MAX_PUNCTUAL_LIGHTS * 4], u32) {
    let mut packed = [[0.0f32; 4]; MAX_PUNCTUAL_LIGHTS * 4];
    let count = lights.len().min(MAX_PUNCTUAL_LIGHTS);
    for (i, light) in lights.iter().take(count).enumerate() {
        let (kind, cos_inner, cos_outer) = match light.kind {
            CpuLightKind::Point => (1.0, 0.0, 0.0),
            CpuLightKind::Spot {
                cos_inner,
                cos_outer,
            } => (2.0, cos_inner, cos_outer),
            CpuLightKind::Directional => (3.0, 0.0, 0.0),
        };
        let base = i * 4;
        packed[base] = [
            light.position[0],
            light.position[1],
            light.position[2],
            kind,
        ];
        packed[base + 1] = [
            light.color[0] * light.intensity,
            light.color[1] * light.intensity,
            light.color[2] * light.intensity,
            light.range,
        ];
        packed[base + 2] = [
            light.direction[0],
            light.direction[1],
            light.direction[2],
            cos_inner,
        ];
        packed[base + 3] = [cos_outer, 0.0, 0.0, 0.0];
    }
    (packed, count as u32)
}

/// Point and spot lights that get a cube shadow map; the rest, and every directional light, stay unshadowed.
pub const MAX_SHADOWED_PUNCTUAL_LIGHTS: usize = 4;

/// One shadowed light's cube: where it sits and the depth range its six faces project.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PunctualShadowSlot {
    pub position: Vec3,
    pub near: f32,
    pub far: f32,
}

/// The far plane of a light without a range: past the farthest corner of the scene, or 100 without geometry.
pub fn unbounded_shadow_far(position: Vec3, bounds: Option<(Vec3, Vec3)>) -> f32 {
    let Some((min, max)) = bounds.filter(|(min, max)| min.cmple(*max).all()) else {
        return 100.0;
    };
    let farthest = (min - position).abs().max((max - position).abs());
    (farthest.length() * 1.05).max(1.0)
}

/// Gives the first [`MAX_SHADOWED_PUNCTUAL_LIGHTS`] point and spot lights of `packed` a cube: row 3 becomes
/// [cos_outer, slot + 1, near, far], and every other light's .yzw zero, meaning unshadowed.
pub fn assign_punctual_shadow_slots(
    packed: &mut [[f32; 4]],
    count: u32,
    bounds: Option<(Vec3, Vec3)>,
) -> Vec<PunctualShadowSlot> {
    let mut slots = Vec::new();
    for light in packed
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .take(count as usize)
    {
        let directional = light[0][3] > 2.5;
        if directional || slots.len() == MAX_SHADOWED_PUNCTUAL_LIGHTS {
            light[3] = [light[3][0], 0.0, 0.0, 0.0];
            continue;
        }
        let position = Vec3::new(light[0][0], light[0][1], light[0][2]);
        let range = light[1][3];
        let far = if range > 0.0 {
            range
        } else {
            unbounded_shadow_far(position, bounds)
        };
        // A thousandth of the far plane keeps Depth32Float's precision at the far end finer than a texel.
        let near = (far * 1.0e-3).clamp(0.01, 0.5);
        light[3] = [light[3][0], (slots.len() + 1) as f32, near, far];
        slots.push(PunctualShadowSlot {
            position,
            near,
            far,
        });
    }
    slots
}

/// Each face's view axis and up vector in cube layer order (+X, -X, +Y, -Y, +Z, -Z), as the C++ engine's.
const FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::NEG_Y),
    (Vec3::NEG_X, Vec3::NEG_Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::NEG_Z),
    (Vec3::Z, Vec3::NEG_Y),
    (Vec3::NEG_Z, Vec3::NEG_Y),
];

/// View-projections of a cube's faces in layer order, each a 90-degree perspective whose NDC xy is the face's
/// (sc, tc) / |ma| of the cube-map selection table, depth zero to one.
pub fn punctual_shadow_face_matrices(slot: &PunctualShadowSlot) -> [Mat4; 6] {
    let projection = clip::perspective(std::f32::consts::FRAC_PI_2, 1.0, slot.near, slot.far);
    FACES.map(|(axis, up)| projection * look_at_mat4(slot.position, slot.position + axis, up))
}

/// The depth a face stores for a point `major_axis` along its axis; `punctual_shadow.slang`'s twin.
pub fn punctual_shadow_depth(major_axis: f32, near: f32, far: f32) -> f32 {
    far * (major_axis - near) / ((far - near) * major_axis)
}

#[cfg(test)]
mod shadow_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn point_light() -> CpuLight {
        CpuLight {
            kind: CpuLightKind::Point,
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
            range: 10.0,
            position: [1.0, 2.0, 3.0],
            direction: [0.0, -1.0, 0.0],
            node: None,
        }
    }

    fn spot_light(cos_inner: f32, cos_outer: f32) -> CpuLight {
        CpuLight {
            kind: CpuLightKind::Spot {
                cos_inner,
                cos_outer,
            },
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            range: 5.0,
            position: [0.0, 1.0, 0.0],
            direction: [0.0, -1.0, 0.0],
            node: None,
        }
    }

    fn directional_light() -> CpuLight {
        CpuLight {
            kind: CpuLightKind::Directional,
            color: [1.0, 1.0, 1.0],
            intensity: 3.0,
            range: 0.0,
            position: [0.0, 0.0, 0.0],
            direction: [1.0, 0.0, 0.0],
            node: None,
        }
    }

    #[test]
    fn point_spot_and_directional_pack_their_kind_discriminant() {
        let lights = [point_light(), spot_light(0.9, 0.8), directional_light()];
        let (packed, count) = pack_punctual_lights(&lights);
        assert_eq!(count, 3);

        // punctual_light.slang: kind > 2.5 directional, > 1.5 spot, else point; test both thresholds.
        let kind_of = |light_index: usize| packed[light_index * 4][3];
        let point_kind = kind_of(0);
        let spot_kind = kind_of(1);
        let directional_kind = kind_of(2);

        assert_eq!(point_kind, 1.0);
        assert!(point_kind <= 1.5, "point must not be read as spot");

        assert_eq!(spot_kind, 2.0);
        assert!(spot_kind > 1.5, "spot must be read as spot, not point");
        assert!(spot_kind <= 2.5, "spot must not be read as directional");

        assert_eq!(directional_kind, 3.0);
        assert!(
            directional_kind > 2.5,
            "directional must be read as directional"
        );
    }

    #[test]
    fn intensity_is_premultiplied_into_the_colour_row() {
        let lights = [point_light()];
        let (packed, _) = pack_punctual_lights(&lights);
        let row1 = packed[1];
        assert_eq!(row1[0], 1.0 * 2.0);
        assert_eq!(row1[1], 0.5 * 2.0);
        assert_eq!(row1[2], 0.25 * 2.0);
        assert_eq!(row1[3], 10.0, "range lives in row 1's .w");
    }

    #[test]
    fn lights_beyond_the_cap_are_dropped_not_wrapped() {
        let lights: Vec<CpuLight> = (0..MAX_PUNCTUAL_LIGHTS + 5)
            .map(|i| CpuLight {
                position: [i as f32, 0.0, 0.0],
                ..point_light()
            })
            .collect();
        let (packed, count) = pack_punctual_lights(&lights);
        assert_eq!(count as usize, MAX_PUNCTUAL_LIGHTS);

        let last_base = (MAX_PUNCTUAL_LIGHTS - 1) * 4;
        assert_eq!(
            packed[last_base][0],
            (MAX_PUNCTUAL_LIGHTS - 1) as f32,
            "last packed record must be light index MAX_PUNCTUAL_LIGHTS - 1, not a wraparound over the front"
        );

        // Nothing beyond the cap was ever written.
        assert_eq!(packed[MAX_PUNCTUAL_LIGHTS * 4 - 4][3], 1.0);
    }

    #[test]
    fn spot_cone_angles_land_in_the_slots_smoothstep_reads() {
        let lights = [spot_light(0.9, 0.7)];
        let (packed, _) = pack_punctual_lights(&lights);
        // punctual_light.slang: `smoothstep(cos_outer, cos_inner, cosAngle)`, cos_outer row base+3 .x, cos_inner base+2 .w.
        let cvec = packed[2];
        let dvec = packed[3];
        assert_eq!(cvec[3], 0.9, "cos_inner must be in packed[base+2][3]");
        assert_eq!(dvec[0], 0.7, "cos_outer must be in packed[base+3][0]");
    }
}
