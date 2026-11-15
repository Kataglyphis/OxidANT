//! The tracer against brute force and against the GPU, over generated scenes and the glTF test assets.

mod cpu;
mod gpu;
mod scenes;

/// A seeded xorshift, so failures reproduce.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub(crate) fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[-1, 1)` on each axis.
    pub(crate) fn signed3(&mut self) -> glam::Vec3 {
        glam::Vec3::new(self.next_f32(), self.next_f32(), self.next_f32()) * 2.0 - 1.0
    }
}
