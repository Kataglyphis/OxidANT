//! The Sun's defaults and ranges, which the Rust renderer starts from and the bridge reports to the C++ engine.

use std::ops::RangeInclusive;

use glam::{Vec3, Vec4};

/// The Direction drag, per axis of the way the light travels; both editors show that way and store its opposite.
pub const DIRECTION_RANGE: RangeInclusive<f32> = -1.0..=1.0;
/// The Color picker, per channel.
pub const COLOR_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// The Radiance slider.
pub const RADIANCE_RANGE: RangeInclusive<f32> = 0.0..=50.0;
/// The Sky tab's Ambient slider: how strongly the sky lights the scene.
pub const AMBIENT_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// The directional light and the constant ambient beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunSettings {
    /// Towards the Sun, unnormalised, as the C++ engine's `directional_light_direction` holds it.
    pub direction: Vec3,
    /// Linear RGB.
    pub color: Vec3,
    /// Scales `color`; about 5, because the BRDF divides diffuse by PI.
    pub radiance: f32,
    pub ambient: f32,
}

impl Default for SunSettings {
    fn default() -> Self {
        Self {
            direction: Vec3::new(0.5, 0.8, 0.3),
            color: Vec3::new(1.0, 0.97, 0.92),
            radiance: 5.0,
            ambient: 0.35,
        }
    }
}

impl SunSettings {
    /// The direction with the ambient in w, as the renderer's `light_dir_ambient` holds them.
    pub fn light_dir_ambient(&self) -> Vec4 {
        self.direction.extend(self.ambient)
    }

    /// The colour with the radiance in w, as the renderer's `light_color_intensity` holds them.
    pub fn light_color_intensity(&self) -> Vec4 {
        self.color.extend(self.radiance)
    }

    /// Every field in its editor range, a non-finite one at its default, as `CloudSettings`; the bridge's `sanitize`.
    /// A direction that clamps to zero points straight up, where both renderers send a light with no direction.
    pub fn sanitized(&self) -> Self {
        let defaults = Self::default();
        let within = |value: f32, fallback: f32, range: &RangeInclusive<f32>| {
            let finite = if value.is_finite() { value } else { fallback };
            finite.clamp(*range.start(), *range.end())
        };
        let axes = |value: Vec3, fallback: Vec3, range: &RangeInclusive<f32>| {
            Vec3::new(
                within(value.x, fallback.x, range),
                within(value.y, fallback.y, range),
                within(value.z, fallback.z, range),
            )
        };
        let direction = axes(self.direction, defaults.direction, &DIRECTION_RANGE);
        Self {
            direction: if direction.length_squared() < 1e-12 {
                Vec3::Y
            } else {
                direction
            },
            color: axes(self.color, defaults.color, &COLOR_RANGE),
            radiance: within(self.radiance, defaults.radiance, &RADIANCE_RANGE),
            ambient: within(self.ambient, defaults.ambient, &AMBIENT_RANGE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_owners_warm_sun_and_packs_as_the_renderer_reads_it() {
        let sun = SunSettings::default();
        assert_eq!(sun.light_dir_ambient(), Vec4::new(0.5, 0.8, 0.3, 0.35));
        assert_eq!(sun.light_color_intensity(), Vec4::new(1.0, 0.97, 0.92, 5.0));
        assert_eq!(sun.sanitized(), sun, "the defaults are sane");
    }

    #[test]
    fn sanitized_clamps_each_field_and_puts_a_non_finite_one_at_its_default() {
        let wild = SunSettings {
            direction: Vec3::new(f32::NAN, 7.0, -3.0),
            color: Vec3::new(-1.0, f32::INFINITY, 0.5),
            radiance: 80.0,
            ambient: f32::NEG_INFINITY,
        };
        let sane = wild.sanitized();
        assert_eq!(sane.direction, Vec3::new(0.5, 1.0, -1.0));
        assert_eq!(sane.color, Vec3::new(0.0, 0.97, 0.5));
        assert_eq!((sane.radiance, sane.ambient), (50.0, 0.35));
        let low = SunSettings {
            radiance: -2.0,
            ambient: -0.5,
            ..SunSettings::default()
        }
        .sanitized();
        assert_eq!((low.radiance, low.ambient), (0.0, 0.0));
        let high = SunSettings {
            ambient: 9.0,
            ..SunSettings::default()
        };
        assert_eq!(high.sanitized().ambient, 2.0);
    }

    #[test]
    fn a_direction_with_no_length_points_straight_up() {
        let zero = SunSettings {
            direction: Vec3::ZERO,
            ..SunSettings::default()
        };
        assert_eq!(zero.sanitized().direction, Vec3::Y);
    }
}
