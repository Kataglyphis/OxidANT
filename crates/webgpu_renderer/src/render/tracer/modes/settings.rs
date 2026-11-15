//! The path tracer's quality controls and its white-furnace mode, with the C++ editor's ranges and defaults.

use std::ops::RangeInclusive;

/// The C++ editor's "Samples / pixel" range.
pub const SAMPLES_PER_PIXEL: RangeInclusive<u32> = 1..=64;
/// The C++ editor's "Max bounces" range.
pub const MAX_BOUNCES: RangeInclusive<u32> = 1..=16;

/// One white surface everywhere under a uniform sky: each pixel converges to the sky times that surface's directional albedo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnaceSettings {
    pub radiance: f32,
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for FurnaceSettings {
    /// The C++ furnace golden's white dielectric under a sky of radiance 1.
    fn default() -> Self {
        Self {
            radiance: 1.0,
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

/// What the path-traced mode traces; a change starts its accumulation over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSettings {
    /// Paths per pixel and frame.
    pub samples_per_pixel: u32,
    /// Segments per path.
    pub max_bounces: u32,
    /// The white furnace in place of the scene's materials, sky and sun.
    pub furnace: Option<FurnaceSettings>,
}

impl Default for TraceSettings {
    /// The C++ editor's defaults.
    fn default() -> Self {
        Self {
            samples_per_pixel: 8,
            max_bounces: 8,
            furnace: None,
        }
    }
}

impl TraceSettings {
    /// Within the C++ ranges, and a furnace with finite values in [0, 1] where the BRDF needs them.
    pub fn clamped(self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let furnace = self.furnace.map(|f| FurnaceSettings {
            radiance: finite(f.radiance, 1.0).max(0.0),
            metallic: finite(f.metallic, 0.0).clamp(0.0, 1.0),
            roughness: finite(f.roughness, 0.5).clamp(0.0, 1.0),
        });
        Self {
            samples_per_pixel: self
                .samples_per_pixel
                .clamp(*SAMPLES_PER_PIXEL.start(), *SAMPLES_PER_PIXEL.end()),
            max_bounces: self
                .max_bounces
                .clamp(*MAX_BOUNCES.start(), *MAX_BOUNCES.end()),
            furnace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_and_ranges_are_the_c_plus_plus_editors() {
        let settings = TraceSettings::default();
        assert_eq!((settings.samples_per_pixel, settings.max_bounces), (8, 8));
        assert_eq!(settings.furnace, None);
        assert_eq!((SAMPLES_PER_PIXEL, MAX_BOUNCES), (1..=64, 1..=16));
        assert_eq!(settings.clamped(), settings);
    }

    #[test]
    fn out_of_range_values_clamp_and_a_broken_furnace_heals() {
        let wild = TraceSettings {
            samples_per_pixel: 0,
            max_bounces: 99,
            furnace: Some(FurnaceSettings {
                radiance: f32::NAN,
                metallic: 2.0,
                roughness: -1.0,
            }),
        }
        .clamped();
        assert_eq!((wild.samples_per_pixel, wild.max_bounces), (1, 16));
        let furnace = wild.furnace.expect("the furnace stays on");
        assert_eq!(
            (furnace.radiance, furnace.metallic, furnace.roughness),
            (1.0, 1.0, 0.0)
        );
    }
}
