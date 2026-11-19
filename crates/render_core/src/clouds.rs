//! The volumetric clouds' settings, their ranges and their uniform packing, in the C++ engine's ranges and defaults.
//! The noise, the box and the march are `shaders/common/volumetric_clouds.slang`, shared with that engine.

use std::ops::RangeInclusive;

use glam::Vec3;

/// March steps along each view ray; the shader clamps to the same bounds.
pub const MARCH_STEPS: RangeInclusive<i32> = 4..=128;
/// March steps from each view sample toward the sun; the shader clamps to the same bounds.
pub const LIGHT_STEPS: RangeInclusive<i32> = 1..=128;
/// The Density slider, the C++ range; it thins the cloud and leaves the box alone.
pub const DENSITY_RANGE: RangeInclusive<f32> = 0.001..=1.0;
/// The Coverage slider: noise below it is clear sky.
pub const COVERAGE_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// The Pillowness slider.
pub const PILLOWNESS_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// The Cirrus slider.
pub const CIRRUS_RANGE: RangeInclusive<f32> = 0.0..=1.0;
/// The Scale drag; above zero, or a box axis collapses and the slab test divides by zero.
pub const SCALE_RANGE: RangeInclusive<f32> = 0.001..=1000.0;
/// The Offset drag.
pub const OFFSET_RANGE: RangeInclusive<f32> = -200.0..=400.0;
/// World units of box half extent per unit of Scale (`cloud_unpack`'s `* 10.0`).
pub const BOX_EXTENT_PER_SCALE: f32 = 10.0;
/// World units after which the march's noise repeats: `sample_density`'s coarse `/ 256.0`, which its fine `/ 64.0` divides.
pub const NOISE_PERIOD: f64 = 256.0;
/// The Wind drag, world units per second along each axis.
pub const WIND_RANGE: RangeInclusive<f32> = -100.0..=100.0;

/// The C++ editor's CLOUDS section: the switch and its eleven controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudSettings {
    pub enabled: bool,
    /// Samples along each view ray through the cloud box.
    pub march_steps: i32,
    /// Samples from each view sample toward the sun.
    pub light_steps: i32,
    /// Multiplies every noise sample; the box keeps its size.
    pub density: f32,
    /// Noise below this threshold is clear sky.
    pub coverage: f32,
    pub pillowness: f32,
    pub cirrus: f32,
    /// Darkens thin cloud edges, as dense droplets shadow themselves.
    pub powder: bool,
    /// The box's half extent per axis, in tens of world units.
    pub scale: Vec3,
    /// The centre of the box.
    pub offset: Vec3,
    /// World units per second the noise drifts through the box; the renderer's `cloud_time_seconds` drives it.
    pub wind: Vec3,
    /// March a quarter of the pixels and upsample by depth; on by default, as in the C++ engine.
    pub half_resolution: bool,
}

impl Default for CloudSettings {
    /// The C++ `GUISceneSharedVars` defaults: a thin slab far above the scene, switched off.
    /// Scale is the old default times Density, so the slab keeps the 6300 by 31.5 half extents it had.
    fn default() -> Self {
        Self {
            enabled: false,
            march_steps: 8,
            light_steps: 3,
            density: 0.63,
            coverage: 0.493,
            pillowness: 0.966,
            cirrus: 0.034,
            powder: true,
            scale: Vec3::new(630.0, 3.15, 630.0),
            offset: Vec3::new(-0.364, 367.0, -18.351),
            // A fine noise tile (64 units) every 15 s.
            wind: Vec3::new(4.0, 0.0, 1.5),
            half_resolution: true,
        }
    }
}

/// The four `vec4`s `volumetric_clouds.slang`'s `CloudParams` reads, laid out as the C++ `SceneUBO` packs them.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudParams {
    /// Scale (xyz) and Density (w).
    pub mesh_scale: [f32; 4],
    /// Offset (xyz) and Coverage (w).
    pub mesh_offset: [f32; 4],
    /// Pillowness, Cirrus, Powder as 0 or 1, March steps.
    pub parameters: [f32; 4],
    /// Light steps (x) and the wind's noise offset (yzw).
    pub light_march: [f32; 4],
}

impl CloudSettings {
    /// What the renderer reads, clamped where the C++ `SceneUboMarshal` clamps: both step counts into their ranges,
    /// Scale and Density onto their floors (a NaN onto the floor too); every other field passes through.
    pub fn sanitized(&self) -> Self {
        Self {
            march_steps: self
                .march_steps
                .clamp(*MARCH_STEPS.start(), *MARCH_STEPS.end()),
            light_steps: self
                .light_steps
                .clamp(*LIGHT_STEPS.start(), *LIGHT_STEPS.end()),
            density: self.density.max(*DENSITY_RANGE.start()),
            scale: self.scale.max(Vec3::splat(*SCALE_RANGE.start())),
            ..*self
        }
    }

    /// The settings as the shader reads them at time zero, where the wind has not moved the noise.
    pub fn params(&self) -> CloudParams {
        self.params_at(0.0)
    }

    /// The [`Self::sanitized`] settings `time_s` seconds into the wind.
    pub fn params_at(&self, time_s: f64) -> CloudParams {
        let clouds = self.sanitized();
        let wind = clouds.wind_offset(time_s);
        CloudParams {
            mesh_scale: clouds.scale.extend(clouds.density).to_array(),
            mesh_offset: clouds.offset.extend(clouds.coverage).to_array(),
            parameters: [
                clouds.pillowness,
                clouds.cirrus,
                if clouds.powder { 1.0 } else { 0.0 },
                clouds.march_steps as f32,
            ],
            light_march: [clouds.light_steps as f32, wind.x, wind.y, wind.z],
        }
    }

    /// How far the wind has moved the noise after `time_s`, wrapped in f64 to its period so hours keep f32 precision.
    pub fn wind_offset(&self, time_s: f64) -> Vec3 {
        let wrap = |velocity: f32| (f64::from(velocity) * time_s).rem_euclid(NOISE_PERIOD) as f32;
        Vec3::new(wrap(self.wind.x), wrap(self.wind.y), wrap(self.wind.z))
    }

    /// The box's world-space half extents, as `cloud_unpack` derives them; Density plays no part.
    pub fn half_extents(&self) -> Vec3 {
        let [x, y, z, _] = self.params().mesh_scale;
        Vec3::new(x, y, z) * BOX_EXTENT_PER_SCALE
    }

    /// Centres the box on `centre` with `half_extents`, at `density`: the C++ goldens' `place_cloud_box`.
    pub fn place_box(&mut self, centre: Vec3, half_extents: Vec3, density: f32) {
        self.density = density;
        self.offset = centre;
        self.scale = half_extents / BOX_EXTENT_PER_SCALE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_cpp_editors() {
        let clouds = CloudSettings::default();
        assert!(!clouds.enabled);
        assert_eq!((clouds.march_steps, clouds.light_steps), (8, 3));
        assert_eq!(
            [
                clouds.density,
                clouds.coverage,
                clouds.pillowness,
                clouds.cirrus
            ],
            [0.63, 0.493, 0.966, 0.034]
        );
        assert!(clouds.powder && clouds.half_resolution);
        assert_eq!(clouds.scale, Vec3::new(630.0, 3.15, 630.0));
        assert_eq!(clouds.offset, Vec3::new(-0.364, 367.0, -18.351));
        assert!(
            MARCH_STEPS.contains(&clouds.march_steps) && LIGHT_STEPS.contains(&clouds.light_steps)
        );
    }

    #[test]
    fn every_default_lies_in_its_range_and_sanitizing_keeps_it() {
        let clouds = CloudSettings::default();
        assert_eq!(clouds.sanitized(), clouds);
        let scalars = [
            (clouds.density, DENSITY_RANGE),
            (clouds.coverage, COVERAGE_RANGE),
            (clouds.pillowness, PILLOWNESS_RANGE),
            (clouds.cirrus, CIRRUS_RANGE),
        ];
        for (value, range) in scalars {
            assert!(range.contains(&value), "{value} outside {range:?}");
        }
        let axes = [
            (clouds.scale, SCALE_RANGE),
            (clouds.offset, OFFSET_RANGE),
            (clouds.wind, WIND_RANGE),
        ];
        for (value, range) in axes {
            assert!(
                value.to_array().iter().all(|c| range.contains(c)),
                "{value}"
            );
        }
    }

    #[test]
    fn the_params_pack_like_the_cpp_scene_ubo() {
        let params = CloudSettings::default().params();
        assert_eq!(params.mesh_scale, [630.0, 3.15, 630.0, 0.63]);
        assert_eq!(params.mesh_offset, [-0.364, 367.0, -18.351, 0.493]);
        assert_eq!(params.parameters, [0.966, 0.034, 1.0, 8.0]);
        assert_eq!(
            params.light_march,
            [3.0, 0.0, 0.0, 0.0],
            "time zero packs no wind, as the C++ engine"
        );
        let without_powder = CloudSettings {
            powder: false,
            ..CloudSettings::default()
        };
        assert_eq!(without_powder.params().parameters[2], 0.0);
        assert_eq!(std::mem::size_of::<CloudParams>(), 64);
    }

    #[test]
    fn steps_scale_and_density_are_clamped_to_the_cpp_floors() {
        let wild = CloudSettings {
            march_steps: 0,
            light_steps: 1000,
            density: 0.0,
            scale: Vec3::new(0.0, -5.0, 2.0),
            ..CloudSettings::default()
        };
        let params = wild.params();
        assert_eq!(params.parameters[3], 4.0);
        assert_eq!(params.light_march[0], 128.0);
        assert_eq!(params.mesh_scale, [0.001, 0.001, 2.0, 0.001]);
        assert!(wild.half_extents().min_element() > 0.0);
        let sane = wild.sanitized();
        assert_eq!((sane.march_steps, sane.light_steps), (4, 128));
        assert_eq!(
            (sane.density, sane.scale),
            (0.001, Vec3::new(0.001, 0.001, 2.0))
        );
        let nan = CloudSettings {
            density: f32::NAN,
            scale: Vec3::splat(f32::NAN),
            ..wild
        }
        .sanitized();
        assert_eq!((nan.density, nan.scale), (0.001, Vec3::splat(0.001)));
    }

    #[test]
    fn sanitizing_leaves_what_the_marshal_does_not_clamp() {
        let free = CloudSettings {
            enabled: true,
            density: 7.0,
            coverage: -3.0,
            pillowness: 2.0,
            cirrus: f32::INFINITY,
            powder: false,
            scale: Vec3::splat(5000.0),
            offset: Vec3::splat(-999.0),
            wind: Vec3::splat(300.0),
            half_resolution: false,
            ..CloudSettings::default()
        };
        assert_eq!(free.sanitized(), free);
    }

    #[test]
    fn the_box_is_scale_times_ten_around_the_offset_whatever_the_density() {
        let clouds = CloudSettings::default();
        // The extents the default slab had while Density still sized it.
        let expected = Vec3::new(1000.0, 5.0, 1000.0) * 0.63 * 10.0;
        assert!(clouds.half_extents().abs_diff_eq(expected, 1e-2));
        for density in [0.001, 0.25, 1.0] {
            let thinner = CloudSettings { density, ..clouds };
            assert_eq!(thinner.half_extents(), clouds.half_extents());
            assert_eq!(thinner.params().mesh_scale[3], density);
        }
        let mut placed = CloudSettings::default();
        placed.place_box(Vec3::new(1.0, 6.0, -3.0), Vec3::new(3.0, 2.0, 0.5), 0.25);
        assert_eq!(placed.offset, Vec3::new(1.0, 6.0, -3.0));
        assert!(placed
            .half_extents()
            .abs_diff_eq(Vec3::new(3.0, 2.0, 0.5), 1e-5));
        assert_eq!(placed.params().mesh_scale[3], 0.25);
    }

    #[test]
    fn the_wind_offset_is_velocity_times_time_wrapped_to_the_noise_period() {
        let clouds = CloudSettings {
            wind: Vec3::new(4.0, -2.0, 0.5),
            ..CloudSettings::default()
        };
        assert_eq!(clouds.wind_offset(0.0), Vec3::ZERO);
        assert_eq!(clouds.wind_offset(10.0), Vec3::new(40.0, 236.0, 5.0));
        assert_eq!(clouds.params_at(10.0).light_march, [3.0, 40.0, 236.0, 5.0]);
        // A day in: f32 seconds would step 8 ms, but the f64 wrap lands on the same offset as 3.25 s.
        let day = 86_400.0;
        assert_eq!(clouds.wind_offset(day + 3.25).x, clouds.wind_offset(3.25).x);
        let still = CloudSettings {
            wind: Vec3::ZERO,
            ..clouds
        };
        assert_eq!(still.params_at(1.0e6), still.params());
    }
}
