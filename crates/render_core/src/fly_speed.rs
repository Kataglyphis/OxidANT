//! How fast the fly camera crosses a scene, which both renderers' framing picks.

use std::ops::RangeInclusive;

use glam::Vec3;

/// The C++ `movement_speed` in world units per second, for a scene without bounds.
pub const DEFAULT_SPEED: f32 = 10.0;
/// The slowest and fastest speed a scene or the Camera tab may set.
pub const SPEED_RANGE: RangeInclusive<f32> = 0.001..=1_000_000.0;
/// A quarter of the diagonal per second: the C++ 10 units/s in a Sponza about 40 units across.
const SPEED_PER_DIAGONAL: f32 = 0.25;

/// The default speed for a scene's bounds: a 1 m cube and an 8 km city both take four seconds to cross.
pub fn speed_for_bounds(bounds: Option<(Vec3, Vec3)>) -> f32 {
    let diagonal = bounds.map_or(f32::NAN, |(min, max)| (max - min).length());
    if diagonal.is_finite() && diagonal > 0.0 {
        (diagonal * SPEED_PER_DIAGONAL).clamp(*SPEED_RANGE.start(), *SPEED_RANGE.end())
    } else {
        DEFAULT_SPEED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speed_follows_the_scene_size() {
        assert_eq!(speed_for_bounds(None), DEFAULT_SPEED);
        let cube = speed_for_bounds(Some((Vec3::ZERO, Vec3::ONE)));
        assert!((cube - 3.0f32.sqrt() / 4.0).abs() < 1e-6, "{cube}");
        let city = speed_for_bounds(Some((Vec3::ZERO, Vec3::new(8000.0, 300.0, 8000.0))));
        assert!((city - 2829.4).abs() < 0.1, "{city}");
        assert_eq!(
            speed_for_bounds(Some((Vec3::ONE, Vec3::ONE))),
            DEFAULT_SPEED
        );
        let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        assert_eq!(speed_for_bounds(Some(empty)), DEFAULT_SPEED);
    }
}
