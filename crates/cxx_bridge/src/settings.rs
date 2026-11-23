//! The settings both renderers share, in the C++ namespace `kataglyphis::oxidant::settings`: their defaults, the
//! editor's ranges and the clamps the renderers apply, all `kataglyphis_render_core`'s.

use kataglyphis_render_core::clouds::{self, CloudSettings};
use kataglyphis_render_core::shadow_settings::{self, ShadowSettings};
use kataglyphis_render_core::sun::{self, SunSettings};

use crate::{guarded, nothing};

#[cxx::bridge(namespace = "kataglyphis::oxidant::settings")]
pub mod ffi {
    /// The CLOUDS section, `kataglyphis_render_core::clouds::CloudSettings` field for field.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct CloudSettings {
        enabled: bool,
        march_steps: i32,
        light_steps: i32,
        density: f32,
        coverage: f32,
        pillowness: f32,
        cirrus: f32,
        powder: bool,
        /// The box's half extent per axis, in tens of world units.
        scale: [f32; 3],
        offset: [f32; 3],
        /// World units per second.
        wind: [f32; 3],
        half_resolution: bool,
    }

    /// The SHADOWS and LIGHT SHADOWS sections; `resolution` counts texels, not the C++ combo's index.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ShadowSettings {
        enabled: bool,
        resolution: u32,
        cascade_count: u32,
        pcf_radius: u32,
        strength: f32,
        distance: f32,
        split_lambda: f32,
        punctual: bool,
    }

    /// The Sun, `kataglyphis_render_core::sun::SunSettings` field for field.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct SunSettings {
        /// Towards the Sun, as the C++ engine's `directional_light_direction`; both editors show the way the light travels.
        direction: [f32; 3],
        color: [f32; 3],
        radiance: f32,
        ambient: f32,
    }

    /// Every shared setting. A C++ value starts zeroed, not at these defaults: begin from `default_scene_settings()`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct SceneSettings {
        clouds: CloudSettings,
        shadows: ShadowSettings,
        sun: SunSettings,
    }

    /// Both ends included.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct FloatRange {
        min: f32,
        max: f32,
    }

    /// Both ends included.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct IntRange {
        min: i32,
        max: i32,
    }

    /// Both ends included.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct UintRange {
        min: u32,
        max: u32,
    }

    /// The CLOUDS controls' ranges; a three-axis control's range holds for each axis.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct CloudRanges {
        march_steps: IntRange,
        light_steps: IntRange,
        density: FloatRange,
        coverage: FloatRange,
        pillowness: FloatRange,
        cirrus: FloatRange,
        scale: FloatRange,
        offset: FloatRange,
        wind: FloatRange,
    }

    /// The SHADOWS controls' ranges.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ShadowRanges {
        /// What the resolution combo lists, smallest first.
        resolutions: [u32; 4],
        cascade_count: UintRange,
        pcf_radius: UintRange,
        strength: FloatRange,
        distance: FloatRange,
        split_lambda: FloatRange,
    }

    /// The Sun's controls' ranges; the direction's holds for each axis of the way the light travels.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct SunRanges {
        direction: FloatRange,
        color: FloatRange,
        radiance: FloatRange,
        ambient: FloatRange,
    }

    /// The editor's range for every shared setting.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct SettingRanges {
        clouds: CloudRanges,
        shadows: ShadowRanges,
        sun: SunRanges,
    }

    extern "Rust" {
        /// The defaults both renderers start from.
        fn default_scene_settings() -> SceneSettings;
        /// The editor's range for each setting.
        fn setting_ranges() -> SettingRanges;
        /// `settings` in the editor's ranges: each field clamped, a NaN or inf at its default.
        fn sanitize(settings: SceneSettings) -> SceneSettings;
    }
}

/// Cannot panic; `guarded` keeps the bridge's contract all the same, as for every call.
pub fn default_scene_settings() -> ffi::SceneSettings {
    guarded(
        || {
            scene(
                CloudSettings::default(),
                ShadowSettings::default(),
                SunSettings::default(),
            )
        },
        nothing,
    )
}

pub fn setting_ranges() -> ffi::SettingRanges {
    guarded(ranges, nothing)
}

pub fn sanitize(settings: ffi::SceneSettings) -> ffi::SceneSettings {
    guarded(
        || {
            scene(
                core_clouds(settings.clouds).sanitized(),
                core_shadows(settings.shadows).sanitized(),
                core_sun(settings.sun).sanitized(),
            )
        },
        nothing,
    )
}

fn scene(clouds: CloudSettings, shadows: ShadowSettings, sun: SunSettings) -> ffi::SceneSettings {
    ffi::SceneSettings {
        clouds: ffi::CloudSettings {
            enabled: clouds.enabled,
            march_steps: clouds.march_steps,
            light_steps: clouds.light_steps,
            density: clouds.density,
            coverage: clouds.coverage,
            pillowness: clouds.pillowness,
            cirrus: clouds.cirrus,
            powder: clouds.powder,
            scale: clouds.scale.into(),
            offset: clouds.offset.into(),
            wind: clouds.wind.into(),
            half_resolution: clouds.half_resolution,
        },
        shadows: ffi::ShadowSettings {
            enabled: shadows.enabled,
            resolution: shadows.resolution,
            cascade_count: shadows.cascade_count,
            pcf_radius: shadows.pcf_radius,
            strength: shadows.strength,
            distance: shadows.distance,
            split_lambda: shadows.split_lambda,
            punctual: shadows.punctual,
        },
        sun: ffi::SunSettings {
            direction: sun.direction.into(),
            color: sun.color.into(),
            radiance: sun.radiance,
            ambient: sun.ambient,
        },
    }
}

fn core_clouds(c: ffi::CloudSettings) -> CloudSettings {
    CloudSettings {
        enabled: c.enabled,
        march_steps: c.march_steps,
        light_steps: c.light_steps,
        density: c.density,
        coverage: c.coverage,
        pillowness: c.pillowness,
        cirrus: c.cirrus,
        powder: c.powder,
        scale: c.scale.into(),
        offset: c.offset.into(),
        wind: c.wind.into(),
        half_resolution: c.half_resolution,
    }
}

fn core_shadows(s: ffi::ShadowSettings) -> ShadowSettings {
    ShadowSettings {
        enabled: s.enabled,
        resolution: s.resolution,
        cascade_count: s.cascade_count,
        pcf_radius: s.pcf_radius,
        strength: s.strength,
        distance: s.distance,
        split_lambda: s.split_lambda,
        punctual: s.punctual,
    }
}

fn core_sun(s: ffi::SunSettings) -> SunSettings {
    SunSettings {
        direction: s.direction.into(),
        color: s.color.into(),
        radiance: s.radiance,
        ambient: s.ambient,
    }
}

fn floats(range: std::ops::RangeInclusive<f32>) -> ffi::FloatRange {
    ffi::FloatRange {
        min: *range.start(),
        max: *range.end(),
    }
}

fn ints(range: std::ops::RangeInclusive<i32>) -> ffi::IntRange {
    ffi::IntRange {
        min: *range.start(),
        max: *range.end(),
    }
}

fn ranges() -> ffi::SettingRanges {
    ffi::SettingRanges {
        clouds: ffi::CloudRanges {
            march_steps: ints(clouds::MARCH_STEPS),
            light_steps: ints(clouds::LIGHT_STEPS),
            density: floats(clouds::DENSITY_RANGE),
            coverage: floats(clouds::COVERAGE_RANGE),
            pillowness: floats(clouds::PILLOWNESS_RANGE),
            cirrus: floats(clouds::CIRRUS_RANGE),
            scale: floats(clouds::SCALE_RANGE),
            offset: floats(clouds::OFFSET_RANGE),
            wind: floats(clouds::WIND_RANGE),
        },
        shadows: ffi::ShadowRanges {
            resolutions: shadow_settings::SHADOW_MAP_RESOLUTIONS,
            cascade_count: ffi::UintRange {
                min: 1,
                max: shadow_settings::CASCADE_COUNT as u32,
            },
            pcf_radius: ffi::UintRange {
                min: 0,
                max: shadow_settings::MAX_PCF_RADIUS,
            },
            strength: floats(shadow_settings::STRENGTH_RANGE),
            distance: floats(shadow_settings::SHADOW_DISTANCE_RANGE),
            split_lambda: floats(shadow_settings::SPLIT_LAMBDA_RANGE),
        },
        sun: ffi::SunRanges {
            direction: floats(sun::DIRECTION_RANGE),
            color: floats(sun::COLOR_RANGE),
            radiance: floats(sun::RADIANCE_RANGE),
            ambient: floats(sun::AMBIENT_RANGE),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_cross_the_bridge_field_for_field() {
        let settings = default_scene_settings();
        assert_eq!(core_clouds(settings.clouds), CloudSettings::default());
        assert_eq!(core_shadows(settings.shadows), ShadowSettings::default());
        assert_eq!(core_sun(settings.sun), SunSettings::default());
        assert_eq!(settings.clouds.wind, [4.0, 0.0, 1.5]);
        assert_eq!(settings.clouds.scale, [630.0, 3.15, 630.0]);
        assert_eq!(settings.clouds.offset, [-0.364, 367.0, -18.351]);
        assert_eq!(
            (settings.shadows.resolution, settings.shadows.cascade_count),
            (2048, 3)
        );
        let sun = settings.sun;
        assert_eq!(
            (sun.direction, sun.color),
            ([0.5, 0.8, 0.3], [1.0, 0.97, 0.92])
        );
        assert_eq!((sun.radiance, sun.ambient), (5.0, 0.35), "the owner's Sun");
        assert_eq!(sanitize(settings), settings, "the defaults are sane");
    }

    #[test]
    fn every_range_is_render_cores() {
        let ranges = setting_ranges();
        let clouds = ranges.clouds;
        assert_eq!(
            (clouds.march_steps, clouds.light_steps),
            (
                ffi::IntRange { min: 4, max: 128 },
                ffi::IntRange { min: 1, max: 128 }
            )
        );
        let range = |min, max| ffi::FloatRange { min, max };
        assert_eq!(clouds.density, range(0.001, 1.0));
        let unit = range(0.0, 1.0);
        assert_eq!(
            [clouds.coverage, clouds.pillowness, clouds.cirrus],
            [unit; 3]
        );
        assert_eq!(clouds.scale, range(0.001, 1000.0));
        assert_eq!(clouds.offset, range(-200.0, 400.0));
        assert_eq!(clouds.wind, range(-100.0, 100.0), "the owner's ±100");
        let shadows = ranges.shadows;
        assert_eq!(shadows.resolutions, [512, 1024, 2048, 4096]);
        assert_eq!(shadows.cascade_count, ffi::UintRange { min: 1, max: 3 });
        assert_eq!(shadows.pcf_radius, ffi::UintRange { min: 0, max: 5 });
        assert_eq!([shadows.strength, shadows.split_lambda], [unit; 2]);
        assert_eq!(shadows.distance, range(5.0, 500.0));
        let sun = ranges.sun;
        assert_eq!(sun.direction, range(-1.0, 1.0));
        assert_eq!(sun.color, unit);
        assert_eq!(sun.radiance, range(0.0, 50.0));
        assert_eq!(sun.ambient, range(0.0, 2.0));
    }

    #[test]
    fn sanitize_applies_render_cores_clamps_to_every_section() {
        let mut wild = default_scene_settings();
        wild.clouds.march_steps = 0;
        wild.clouds.light_steps = 1000;
        wild.clouds.density = 0.0;
        wild.clouds.scale = [0.0, -5.0, 2.0];
        wild.clouds.coverage = 4.0;
        wild.clouds.offset = [f32::NAN, 500.0, -500.0];
        wild.clouds.wind = [300.0, 0.0, 0.0];
        wild.sun.radiance = 80.0;
        wild.sun.ambient = f32::NAN;
        wild.sun.direction = [0.0, 3.0, -2.0];
        wild.shadows.resolution = 3000;
        wild.shadows.cascade_count = 9;
        wild.shadows.pcf_radius = 40;
        wild.shadows.strength = f32::NAN;
        wild.shadows.distance = 0.5;
        wild.shadows.split_lambda = 7.0;
        let sane = sanitize(wild);
        assert_eq!((sane.clouds.march_steps, sane.clouds.light_steps), (4, 128));
        assert_eq!(sane.clouds.density, 0.001);
        assert_eq!(sane.clouds.scale, [0.001, 0.001, 2.0]);
        assert_eq!(sane.clouds.coverage, 1.0);
        assert_eq!(
            sane.clouds.offset,
            [-0.364, 400.0, -200.0],
            "a NaN takes the default"
        );
        assert_eq!(sane.clouds.wind, [100.0, 0.0, 0.0]);
        let expected_sun = ffi::SunSettings {
            direction: [0.0, 1.0, -1.0],
            radiance: 50.0,
            ambient: 0.35,
            ..wild.sun
        };
        assert_eq!(sane.sun, expected_sun, "a NaN takes the default");
        let expected = ffi::ShadowSettings {
            resolution: 2048,
            cascade_count: 3,
            pcf_radius: 5,
            strength: 0.65,
            distance: 5.0,
            split_lambda: 1.0,
            ..wild.shadows
        };
        assert_eq!(sane.shadows, expected);
    }
}
