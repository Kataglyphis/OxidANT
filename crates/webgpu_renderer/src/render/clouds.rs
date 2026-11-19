//! The volumetric clouds' settings and their uniform packing, in the C++ engine's ranges and defaults.
//! The noise, the box and the march are `shaders/common/volumetric_clouds.slang`, shared with that engine.

use std::ops::RangeInclusive;

use glam::Vec3;

use crate::asset::skybox::CubemapFaces;
use crate::render::ibl::{SKY_GROUND, SKY_HORIZON, SKY_ZENITH};
use crate::render::texture::srgb_to_linear;

/// Texels per axis of the noise volume; the shader's `NOISE_VOLUME_EXTENT`.
pub const NOISE_VOLUME_EXTENT: u32 = 128;
/// `cloud_noise.slang`'s `[numthreads]`: 256 invocations, WebGPU's default per-workgroup limit.
pub const NOISE_WORKGROUP_SIZE: [u32; 3] = [8, 8, 4];
/// The noise volume's texel format, the C++ engine's `R16G16B16A16Sfloat`; WebGPU writes it write-only.
pub const NOISE_VOLUME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// March steps along each view ray; the shader clamps to the same bounds.
pub const MARCH_STEPS: RangeInclusive<i32> = 4..=128;
/// March steps from each view sample toward the sun; the shader clamps to the same bounds.
pub const LIGHT_STEPS: RangeInclusive<i32> = 1..=128;
/// The Density slider, the C++ range; it thins the cloud and leaves the box alone.
pub const DENSITY_RANGE: RangeInclusive<f32> = 0.001..=1.0;
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
    /// The settings as the shader reads them at time zero, where the wind has not moved the noise.
    pub fn params(&self) -> CloudParams {
        self.params_at(0.0)
    }

    /// The settings `time_s` seconds into the wind, clamped where the C++ `SceneUboMarshal` clamps.
    pub fn params_at(&self, time_s: f64) -> CloudParams {
        let scale = self.scale.max(Vec3::splat(*SCALE_RANGE.start()));
        let density = self.density.max(*DENSITY_RANGE.start());
        let steps = |value: i32, range: &RangeInclusive<i32>| {
            value.clamp(*range.start(), *range.end()) as f32
        };
        let wind = self.wind_offset(time_s);
        CloudParams {
            mesh_scale: scale.extend(density).to_array(),
            mesh_offset: self.offset.extend(self.coverage).to_array(),
            parameters: [
                self.pillowness,
                self.cirrus,
                if self.powder { 1.0 } else { 0.0 },
                steps(self.march_steps, &MARCH_STEPS),
            ],
            light_march: [
                steps(self.light_steps, &LIGHT_STEPS),
                wind.x,
                wind.y,
                wind.z,
            ],
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

/// `sky.slang`'s `SKYBOX_DISABLED`: the navy the switched-off sky shows.
const SKY_OFF_DISPLAY: Vec3 = Vec3::new(0.0, 0.0, 0.5);
/// Texels sampled per face edge at most when a cube map is averaged.
const CUBEMAP_AVERAGE_EDGE: u32 = 64;

/// The mean radiance of the drawn sky over its upper and its lower hemisphere, `CloudAmbient` in the shader.
/// The march scatters it isotropically, so a cloud lit by it alone is as bright as that sky.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudAmbient {
    pub top: Vec3,
    pub bottom: Vec3,
}

impl CloudAmbient {
    /// The procedural sky: `sky_gradient` averaged per hemisphere over solid angle, the integrals of pow(y, 0.7) and clamp(3y, 0, 1).
    pub fn analytic_sky() -> Self {
        let [horizon, zenith, ground] = [SKY_HORIZON, SKY_ZENITH, SKY_GROUND].map(Vec3::from);
        Self {
            top: horizon.lerp(zenith, 1.0 / 1.7),
            bottom: horizon.lerp(ground, 5.0 / 6.0),
        }
    }

    /// The switched-off sky: the navy, as the sky pass draws it.
    pub fn sky_off() -> Self {
        let navy = SKY_OFF_DISPLAY.to_array().map(untonemapped).into();
        Self {
            top: navy,
            bottom: navy,
        }
    }

    /// The cube map as the sky pass draws it, sRGB-decoded and untonemapped, averaged per hemisphere.
    /// Every face spans one sixth of the sphere; the side faces' upper rows look up.
    pub fn of_cubemap(faces: &CubemapFaces) -> Self {
        let size = faces.size.max(1) as usize;
        let stride = (faces.size / CUBEMAP_AVERAGE_EDGE).max(1) as usize;
        let mut sums = [Vec3::ZERO; 2];
        let mut counts = [0.0f32; 2];
        for face in 0..6 {
            for row in (0..size).step_by(stride) {
                let v = (row as f32 + 0.5) / size as f32;
                // Faces +X, -X, +Y, -Y, +Z, -Z; a side face's row through its centre counts toward both.
                let weights = match face {
                    2 => [1.0, 0.0],
                    3 => [0.0, 1.0],
                    _ if v < 0.5 => [1.0, 0.0],
                    _ if v > 0.5 => [0.0, 1.0],
                    _ => [0.5, 0.5],
                };
                for column in (0..size).step_by(stride) {
                    let texel = (face * size * size + row * size + column) * 4;
                    let radiance: Vec3 = std::array::from_fn(|c| {
                        untonemapped(srgb_to_linear(faces.rgba8[texel + c]))
                    })
                    .into();
                    for ((sum, count), weight) in sums.iter_mut().zip(&mut counts).zip(weights) {
                        *sum += radiance * weight;
                        *count += weight;
                    }
                }
            }
        }
        Self {
            top: sums[0] / counts[0].max(1.0),
            bottom: sums[1] / counts[1].max(1.0),
        }
    }
}

/// `sky.slang`'s `untonemapped`: the radiance whose ACES curve shows `display`.
fn untonemapped(display: f32) -> f32 {
    let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
    let y = display.clamp(0.0, 1.0);
    let (qa, qb) = (a - c * y, b - d * y);
    (-qb + (qb * qb + 4.0 * qa * e * y).sqrt()) / (2.0 * qa)
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

    #[test]
    fn the_noise_dispatch_tiles_the_volume_within_webgpus_workgroup_limit() {
        let invocations: u32 = NOISE_WORKGROUP_SIZE.iter().product();
        let limits = wgpu::Limits::default();
        assert!(invocations <= limits.max_compute_invocations_per_workgroup);
        assert!(NOISE_WORKGROUP_SIZE[2] <= limits.max_compute_workgroup_size_z);
        for size in NOISE_WORKGROUP_SIZE {
            assert_eq!(
                NOISE_VOLUME_EXTENT % size,
                0,
                "the dispatch must cover every texel"
            );
        }
        assert!(NOISE_VOLUME_EXTENT <= limits.max_texture_dimension_3d);
    }

    #[test]
    fn the_analytic_ambient_is_the_procedural_skys_hemisphere_mean() {
        // Rows uniform in latitude, so each is weighted by the solid angle it spans.
        let sky = crate::render::ibl::EquirectImage::sky(1, 2048);
        let mut sums = [Vec3::ZERO; 2];
        let mut weights = [0.0f32; 2];
        for (row, texel) in sky.rgba32f.as_chunks::<4>().0.iter().enumerate() {
            let latitude = (0.5 - (row as f32 + 0.5) / 2048.0) * std::f32::consts::PI;
            let half = usize::from(latitude < 0.0);
            sums[half] += Vec3::new(texel[0], texel[1], texel[2]) * latitude.cos();
            weights[half] += latitude.cos();
        }
        let ambient = CloudAmbient::analytic_sky();
        assert!(ambient.top.abs_diff_eq(sums[0] / weights[0], 2e-3));
        assert!(ambient.bottom.abs_diff_eq(sums[1] / weights[1], 2e-3));
    }

    #[test]
    fn a_solid_cube_maps_ambient_is_its_untonemapped_colour_per_hemisphere() {
        let blue = [0, 0, 255, 255];
        let red = [255, 0, 0, 255];
        let grey = [128, 128, 128, 255];
        let faces = CubemapFaces::solid([grey, grey, blue, red, grey, grey]);
        let ambient = CloudAmbient::of_cubemap(&faces);
        let grey_radiance = untonemapped(srgb_to_linear(128));
        let full = untonemapped(1.0);
        // The side faces' single row lies on the horizon, so it counts half toward each hemisphere.
        let expected_top = (Vec3::new(0.0, 0.0, full) + Vec3::splat(grey_radiance) * 2.0) / 3.0;
        let expected_bottom = (Vec3::new(full, 0.0, 0.0) + Vec3::splat(grey_radiance) * 2.0) / 3.0;
        assert!(ambient.top.abs_diff_eq(expected_top, 1e-4), "{ambient:?}");
        assert!(
            ambient.bottom.abs_diff_eq(expected_bottom, 1e-4),
            "{ambient:?}"
        );
        assert!(
            (untonemapped(0.5) - 0.5).abs() > 0.05,
            "the inverse curve is not the identity"
        );
    }
}
