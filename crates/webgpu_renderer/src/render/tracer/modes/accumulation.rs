//! When the path tracer's running mean starts over: the C++ `PathTracingHistoryKey`, plus what the C++ resets elsewhere.

use glam::{Mat4, Vec3, Vec4};

use super::settings::TraceSettings;

/// Everything a path-traced sample depends on; a frame whose key differs from the last one's starts over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccumulationKey {
    /// The camera: view, projection and the viewport's lens shift in one matrix.
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// The output in pixels; the accumulation holds one mean per pixel.
    pub size: (u32, u32),
    /// Towards the sun, as the renderer holds it.
    pub light_dir: Vec3,
    /// Colour and intensity: the sun's radiance and the sky's sun disk.
    pub light_color_intensity: Vec4,
    /// How strongly the sky lights a path: the raster's ambient strength times its environment intensity.
    pub sky_light: f32,
    pub settings: TraceSettings,
    /// The traced scene's generation: a new build, or instances moved (the model transform, animation).
    pub scene: u64,
    /// A fingerprint of the punctual lights as packed for the GPU: any light added, moved or recoloured.
    pub lights: u64,
    /// The bound environment's generation: one set or cleared.
    pub environment: u64,
}

/// How many samples the accumulation holds, and the key they were traced under.
#[derive(Clone, Debug, Default)]
pub struct Accumulation {
    key: Option<AccumulationKey>,
    samples: u64,
}

impl Accumulation {
    /// Opens a frame under `key` and returns the samples it adds to: none after a change or a reset.
    pub fn begin(&mut self, key: AccumulationKey) -> u64 {
        if self.key != Some(key) {
            self.key = Some(key);
            self.samples = 0;
        }
        self.samples
    }

    /// Counts the samples a frame added.
    pub fn add(&mut self, samples: u64) {
        self.samples += samples;
    }

    /// Forgets the mean: the next frame starts over, whatever its key.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Samples per pixel in the mean.
    pub fn samples(&self) -> u64 {
        self.samples
    }
}

#[cfg(test)]
mod tests {
    use kataglyphis_render_core::sun::SunSettings;

    use super::*;
    use crate::render::tracer::modes::settings::FurnaceSettings;

    fn key() -> AccumulationKey {
        let sun = SunSettings::default();
        AccumulationKey {
            view_proj: Mat4::IDENTITY,
            eye: Vec3::new(0.0, 1.0, 4.0),
            size: (64, 48),
            light_dir: sun.direction,
            light_color_intensity: sun.light_color_intensity(),
            sky_light: sun.ambient,
            settings: TraceSettings::default(),
            scene: 1,
            lights: 7,
            environment: 0,
        }
    }

    /// The accumulation after one frame under `first` and the samples a second frame under `second` adds to.
    fn after(first: AccumulationKey, second: AccumulationKey) -> u64 {
        let mut accumulation = Accumulation::default();
        assert_eq!(accumulation.begin(first), 0);
        accumulation.add(8);
        accumulation.begin(second)
    }

    #[test]
    fn an_unchanged_key_keeps_accumulating() {
        let mut accumulation = Accumulation::default();
        for frame in 0..3u64 {
            assert_eq!(accumulation.begin(key()), frame * 8);
            accumulation.add(8);
        }
        assert_eq!(accumulation.samples(), 24);
    }

    #[test]
    fn every_premise_of_the_mean_starts_it_over() {
        let edits: [fn(&mut AccumulationKey); 12] = [
            |k| k.view_proj = Mat4::from_translation(Vec3::X * 1e-3),
            |k| k.eye.x += 1e-3,
            |k| k.size.0 += 1,
            |k| k.light_dir.z += 0.01,
            |k| k.light_color_intensity.w -= 1.0,
            |k| k.sky_light = 1.0,
            |k| k.settings.samples_per_pixel = 4,
            |k| k.settings.max_bounces = 1,
            |k| k.settings.furnace = Some(FurnaceSettings::default()),
            |k| k.scene += 1,
            |k| k.lights ^= 1,
            |k| k.environment += 1,
        ];
        let base = key();
        for edit in edits {
            let mut changed = base;
            edit(&mut changed);
            assert_eq!(after(base, changed), 0, "{changed:?}");
        }
        assert_eq!(after(base, base), 8);
    }

    #[test]
    fn a_reset_starts_over_under_the_same_key() {
        let mut accumulation = Accumulation::default();
        accumulation.begin(key());
        accumulation.add(8);
        accumulation.reset();
        assert_eq!(accumulation.samples(), 0);
        assert_eq!(accumulation.begin(key()), 0);
    }
}
