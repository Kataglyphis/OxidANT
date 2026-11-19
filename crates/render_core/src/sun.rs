//! The Sun's defaults, which the Rust renderer starts from and the bridge reports to the C++ engine.

use glam::{Vec3, Vec4};

/// The directional light and the constant ambient beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunSettings {
    /// Towards the Sun, unnormalised; the C++ editor's `directional_light_direction` points the way the light travels.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_owners_warm_sun_and_packs_as_the_renderer_reads_it() {
        let sun = SunSettings::default();
        assert_eq!(sun.light_dir_ambient(), Vec4::new(0.5, 0.8, 0.3, 0.35));
        assert_eq!(sun.light_color_intensity(), Vec4::new(1.0, 0.97, 0.92, 5.0));
    }
}
