//! The volumetric clouds' settings and their uniform packing, in the C++ engine's ranges and defaults.
//! The noise, the box and the march are `shaders/common/volumetric_clouds.slang`, shared with that engine.

use std::ops::RangeInclusive;

use glam::Vec3;

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
/// The Density slider; above zero, since it also sizes the box.
pub const DENSITY_RANGE: RangeInclusive<f32> = 0.001..=1.0;
/// The Scale drag; above zero, or a box axis collapses and the slab test divides by zero.
pub const SCALE_RANGE: RangeInclusive<f32> = 0.001..=1000.0;
/// The Offset drag.
pub const OFFSET_RANGE: RangeInclusive<f32> = -200.0..=400.0;
/// World units of box half extent per unit of Scale times Density (`cloud_unpack`'s `* 10.0`).
pub const BOX_EXTENT_PER_SCALE: f32 = 10.0;

/// The C++ editor's CLOUDS section: the switch and its nine controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudSettings {
    pub enabled: bool,
    /// Samples along each view ray through the cloud box.
    pub march_steps: i32,
    /// Samples from each view sample toward the sun.
    pub light_steps: i32,
    /// Multiplies every noise sample, and sizes the box with `scale`.
    pub density: f32,
    /// Noise below this threshold is clear sky.
    pub coverage: f32,
    pub pillowness: f32,
    pub cirrus: f32,
    /// Darkens thin cloud edges, as dense droplets shadow themselves.
    pub powder: bool,
    /// The box's extent per axis, before Density and the factor of ten.
    pub scale: Vec3,
    /// The centre of the box.
    pub offset: Vec3,
}

impl Default for CloudSettings {
    /// The C++ `GUISceneSharedVars` defaults: a thin slab far above the scene, switched off.
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
            scale: Vec3::new(1000.0, 5.0, 1000.0),
            offset: Vec3::new(-0.364, 367.0, -18.351),
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
    /// Light steps (x).
    pub light_march: [f32; 4],
}

impl CloudSettings {
    /// The settings as the shader reads them, clamped where the C++ `SceneUboMarshal` clamps.
    pub fn params(&self) -> CloudParams {
        let scale = self.scale.max(Vec3::splat(*SCALE_RANGE.start()));
        let density = self.density.max(*DENSITY_RANGE.start());
        let steps = |value: i32, range: &RangeInclusive<i32>| {
            value.clamp(*range.start(), *range.end()) as f32
        };
        CloudParams {
            mesh_scale: scale.extend(density).to_array(),
            mesh_offset: self.offset.extend(self.coverage).to_array(),
            parameters: [
                self.pillowness,
                self.cirrus,
                if self.powder { 1.0 } else { 0.0 },
                steps(self.march_steps, &MARCH_STEPS),
            ],
            light_march: [steps(self.light_steps, &LIGHT_STEPS), 0.0, 0.0, 0.0],
        }
    }

    /// The box's world-space half extents, as `cloud_unpack` derives them.
    pub fn half_extents(&self) -> Vec3 {
        let [x, y, z, density] = self.params().mesh_scale;
        Vec3::new(x, y, z) * density * BOX_EXTENT_PER_SCALE
    }

    /// Centres the box on `centre` with `half_extents`, at `density`: the C++ goldens' `place_cloud_box`.
    pub fn place_box(&mut self, centre: Vec3, half_extents: Vec3, density: f32) {
        self.density = density;
        self.offset = centre;
        self.scale = half_extents / (density * BOX_EXTENT_PER_SCALE);
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
        assert!(clouds.powder);
        assert_eq!(clouds.scale, Vec3::new(1000.0, 5.0, 1000.0));
        assert_eq!(clouds.offset, Vec3::new(-0.364, 367.0, -18.351));
        assert!(
            MARCH_STEPS.contains(&clouds.march_steps) && LIGHT_STEPS.contains(&clouds.light_steps)
        );
    }

    #[test]
    fn the_params_pack_like_the_cpp_scene_ubo() {
        let params = CloudSettings::default().params();
        assert_eq!(params.mesh_scale, [1000.0, 5.0, 1000.0, 0.63]);
        assert_eq!(params.mesh_offset, [-0.364, 367.0, -18.351, 0.493]);
        assert_eq!(params.parameters, [0.966, 0.034, 1.0, 8.0]);
        assert_eq!(params.light_march, [3.0, 0.0, 0.0, 0.0]);
        let without_powder = CloudSettings {
            powder: false,
            ..CloudSettings::default()
        };
        assert_eq!(without_powder.params().parameters[2], 0.0);
        assert_eq!(std::mem::size_of::<CloudParams>(), 64);
    }

    #[test]
    fn steps_scale_and_density_are_clamped_where_the_shader_would_divide_by_them() {
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
    fn the_box_is_scale_times_density_times_ten_around_the_offset() {
        let clouds = CloudSettings::default();
        let expected = Vec3::new(1000.0, 5.0, 1000.0) * 0.63 * 10.0;
        assert!(clouds.half_extents().abs_diff_eq(expected, 1e-2));
        let mut placed = CloudSettings::default();
        placed.place_box(Vec3::new(1.0, 6.0, -3.0), Vec3::new(3.0, 2.0, 0.5), 0.25);
        assert_eq!(placed.offset, Vec3::new(1.0, 6.0, -3.0));
        assert!(placed
            .half_extents()
            .abs_diff_eq(Vec3::new(3.0, 2.0, 0.5), 1e-5));
        assert_eq!(placed.params().mesh_scale[3], 0.25);
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
}
