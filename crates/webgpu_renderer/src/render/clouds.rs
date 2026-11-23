//! The volumetric clouds' GPU half: the noise volume's shape and the sky ambient the march scatters.
//! The settings, their ranges and their packing are `kataglyphis_render_core::clouds`, re-exported here.

use glam::Vec3;
pub use kataglyphis_render_core::clouds::{
    CloudParams, CloudSettings, BOX_EXTENT_PER_SCALE, CIRRUS_RANGE, COVERAGE_RANGE, DENSITY_RANGE,
    LIGHT_STEPS, MARCH_STEPS, NOISE_PERIOD, OFFSET_RANGE, PILLOWNESS_RANGE, SCALE_RANGE,
    WIND_RANGE,
};

use crate::asset::skybox::CubemapFaces;
use crate::render::ibl::{SKY_GROUND, SKY_HORIZON, SKY_ZENITH};
use crate::render::texture::srgb_to_linear;
use crate::render::tonemap::aces_tonemap_inverse;

/// Texels per axis of the noise volume; the shader's `NOISE_VOLUME_EXTENT`.
pub const NOISE_VOLUME_EXTENT: u32 = 128;
/// `cloud_noise.slang`'s `[numthreads]`: 256 invocations, WebGPU's default per-workgroup limit.
pub const NOISE_WORKGROUP_SIZE: [u32; 3] = [8, 8, 4];
/// The noise volume's texel format, the C++ engine's `R16G16B16A16Sfloat`; WebGPU writes it write-only.
pub const NOISE_VOLUME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

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
        let navy = SKY_OFF_DISPLAY.to_array().map(aces_tonemap_inverse).into();
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
                        aces_tonemap_inverse(srgb_to_linear(faces.rgba8[texel + c]))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let grey_radiance = aces_tonemap_inverse(srgb_to_linear(128));
        let full = aces_tonemap_inverse(1.0);
        // The side faces' single row lies on the horizon, so it counts half toward each hemisphere.
        let expected_top = (Vec3::new(0.0, 0.0, full) + Vec3::splat(grey_radiance) * 2.0) / 3.0;
        let expected_bottom = (Vec3::new(full, 0.0, 0.0) + Vec3::splat(grey_radiance) * 2.0) / 3.0;
        assert!(ambient.top.abs_diff_eq(expected_top, 1e-4), "{ambient:?}");
        assert!(
            ambient.bottom.abs_diff_eq(expected_bottom, 1e-4),
            "{ambient:?}"
        );
        assert!(
            (aces_tonemap_inverse(0.5) - 0.5).abs() > 0.05,
            "the inverse curve is not the identity"
        );
    }
}
