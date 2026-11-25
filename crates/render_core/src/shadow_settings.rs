//! The C++ Sun tab's shadow controls, with BeschleunigerBallett's ranges and defaults (`GUISceneSharedVars.ixx`).

use std::ops::RangeInclusive;

/// The most cascades, `limits.slang`'s; [`ShadowSettings::cascade_count`] renders 1 to this many.
pub use crate::shader_limits::CASCADE_COUNT;
/// The C++ `kShadowMapResolutions`: texels per side of each cascade's map.
pub const SHADOW_MAP_RESOLUTIONS: [u32; 4] = [512, 1024, 2048, 4096];
/// The widest PCF radius, `limits.slang`'s.
pub use crate::shader_limits::MAX_PCF_RADIUS;
/// The C++ `kMinShadowDistance..=kMaxShadowDistance`.
pub const SHADOW_DISTANCE_RANGE: RangeInclusive<f32> = 5.0..=500.0;
/// The C++ default distance, fitted to its Sponza and Dinosaurs scenes.
pub const DEFAULT_SHADOW_DISTANCE: f32 = 60.0;
/// The Strength slider: 0 leaves shadowed areas lit, 1 takes all of the sun away.
pub const STRENGTH_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// The Split lambda slider: 0 splits evenly, 1 logarithmically.
pub const SPLIT_LAMBDA_RANGE: RangeInclusive<f32> = 0.0..=1.0;

/// What the cascaded shadow maps do; [`Self::sanitized`] is what the renderer reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    /// One of [`SHADOW_MAP_RESOLUTIONS`]; a change reallocates the maps.
    pub resolution: u32,
    /// 1 to [`CASCADE_COUNT`]; a change reallocates the maps.
    pub cascade_count: u32,
    /// (2r+1)^2 comparison taps, each a bilinear 2x2 PCF; 0 gives hard edges.
    pub pcf_radius: u32,
    /// How dark shadowed areas get: 0 leaves them lit, 1 takes all of the sun away.
    pub strength: f32,
    /// How far from the camera the cascades reach, along its view axis.
    pub distance: f32,
    /// 0 splits the cascades evenly, 1 logarithmically (Zhang's practical split scheme).
    pub split_lambda: f32,
    /// A cube for each of the first four point and spot lights, the C++ LIGHT SHADOWS switch; independent of `enabled`.
    pub punctual: bool,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            resolution: 2048,
            cascade_count: CASCADE_COUNT as u32,
            pcf_radius: 2,
            strength: 0.65,
            distance: DEFAULT_SHADOW_DISTANCE,
            split_lambda: 0.0,
            punctual: true,
        }
    }
}

impl ShadowSettings {
    /// Every field in its C++ range, a non-finite one at its default; an unlisted resolution takes the nearest listed one.
    pub fn sanitized(&self) -> Self {
        let defaults = Self::default();
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let within =
            |value: f32, range: &RangeInclusive<f32>| value.clamp(*range.start(), *range.end());
        Self {
            enabled: self.enabled,
            resolution: nearest_resolution(self.resolution),
            cascade_count: self.cascade_count.clamp(1, CASCADE_COUNT as u32),
            pcf_radius: self.pcf_radius.min(MAX_PCF_RADIUS),
            strength: within(finite(self.strength, defaults.strength), &STRENGTH_RANGE),
            distance: within(
                finite(self.distance, defaults.distance),
                &SHADOW_DISTANCE_RANGE,
            ),
            split_lambda: within(
                finite(self.split_lambda, defaults.split_lambda),
                &SPLIT_LAMBDA_RANGE,
            ),
            punctual: self.punctual,
        }
    }

    /// Cascades the frame renders and samples: none while shadows are off.
    pub fn active_cascades(&self) -> usize {
        let settings = self.sanitized();
        if settings.enabled {
            settings.cascade_count as usize
        } else {
            0
        }
    }
}

/// The listed resolution closest to `texels`, the C++ index clamp for an unlisted value.
pub fn nearest_resolution(texels: u32) -> u32 {
    SHADOW_MAP_RESOLUTIONS
        .into_iter()
        .min_by_key(|listed| listed.abs_diff(texels))
        .unwrap_or(2048)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_cpp_ones() {
        let settings = ShadowSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.resolution, 2048);
        assert_eq!(settings.cascade_count, 3);
        assert_eq!(settings.pcf_radius, 2);
        assert_eq!(settings.strength, 0.65);
        assert_eq!(settings.distance, 60.0);
        assert_eq!(settings.split_lambda, 0.0);
        assert!(settings.punctual);
        assert_eq!(settings.sanitized(), settings);
        assert_eq!(settings.active_cascades(), 3);
    }

    #[test]
    fn every_field_is_held_to_its_cpp_range() {
        let wild = ShadowSettings {
            enabled: true,
            resolution: 3000,
            cascade_count: 9,
            pcf_radius: 40,
            strength: f32::NAN,
            distance: 0.5,
            split_lambda: 7.0,
            punctual: false,
        }
        .sanitized();
        assert_eq!(wild.resolution, 2048, "3000 is nearer 2048 than 4096");
        assert_eq!(wild.cascade_count, 3);
        assert_eq!(wild.pcf_radius, MAX_PCF_RADIUS);
        assert_eq!(
            wild.strength, 0.65,
            "a NaN takes the default, not the range's top"
        );
        assert_eq!(wild.distance, 5.0);
        assert_eq!(wild.split_lambda, 1.0);
        assert!(!wild.punctual, "the switch passes through");
        let none = ShadowSettings {
            cascade_count: 0,
            resolution: 1,
            distance: 1e9,
            strength: -2.0,
            split_lambda: f32::NAN,
            ..ShadowSettings::default()
        }
        .sanitized();
        assert_eq!(none.cascade_count, 1);
        assert_eq!(none.resolution, 512);
        assert_eq!(none.distance, 500.0);
        assert_eq!((none.strength, none.split_lambda), (0.0, 0.0));
        let off = ShadowSettings {
            enabled: false,
            ..ShadowSettings::default()
        };
        assert_eq!(off.active_cascades(), 0);
    }

    #[test]
    fn a_non_finite_field_falls_back_to_its_default() {
        let defaults = ShadowSettings::default();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let sane = ShadowSettings {
                strength: bad,
                distance: bad,
                split_lambda: bad,
                ..defaults
            }
            .sanitized();
            assert_eq!(sane, defaults, "{bad}");
        }
    }
}
