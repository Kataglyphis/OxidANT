//! A traced frame as dispatches short enough for a browser's GPU watchdog: tiles of the image, samples in chunks.

/// A tile's side in pixels.
pub const TILE_SIZE: u32 = 256;
/// Path segments (a closest-hit ray and a shadow ray each) one dispatch traces at most: milliseconds on a desktop GPU.
pub const DISPATCH_SEGMENTS: u64 = 1 << 22;

/// One dispatch: a tile and the samples it adds to each of its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceDispatch {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The first of this dispatch's samples among the frame's.
    pub first_sample: u32,
    pub samples: u32,
    /// Samples each of the tile's pixels held before this dispatch; 0 overwrites the pixel.
    pub accumulated: u32,
}

/// Samples one dispatch of a full tile takes, so it traces at most [`DISPATCH_SEGMENTS`].
pub fn samples_per_dispatch(samples: u32, segments_per_sample: u32) -> u32 {
    let tile = u64::from(TILE_SIZE * TILE_SIZE) * u64::from(segments_per_sample.max(1));
    let fit = (DISPATCH_SEGMENTS / tile).clamp(1, u64::from(u32::MAX)) as u32;
    fit.min(samples.max(1))
}

/// The frame's dispatches: every pixel of `size` gets `samples` more after `accumulated`, chunk by chunk, each chunk over every tile.
pub fn plan(
    size: (u32, u32),
    samples: u32,
    segments_per_sample: u32,
    accumulated: u64,
) -> Vec<TraceDispatch> {
    let (width, height) = (size.0.max(1), size.1.max(1));
    let chunk = samples_per_dispatch(samples, segments_per_sample);
    let mut dispatches = Vec::new();
    let mut first_sample = 0;
    while first_sample < samples.max(1) {
        let count = chunk.min(samples.max(1) - first_sample);
        let before = accumulated.saturating_add(u64::from(first_sample));
        for y in (0..height).step_by(TILE_SIZE as usize) {
            for x in (0..width).step_by(TILE_SIZE as usize) {
                dispatches.push(TraceDispatch {
                    x,
                    y,
                    width: TILE_SIZE.min(width - x),
                    height: TILE_SIZE.min(height - y),
                    first_sample,
                    samples: count,
                    accumulated: u32::try_from(before).unwrap_or(u32::MAX),
                });
            }
        }
        first_sample += count;
    }
    dispatches
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples each pixel gets over `dispatches`, row-major.
    fn coverage(size: (u32, u32), dispatches: &[TraceDispatch]) -> Vec<u32> {
        let mut samples = vec![0; (size.0 * size.1) as usize];
        for d in dispatches {
            for y in d.y..d.y + d.height {
                for x in d.x..d.x + d.width {
                    samples[(y * size.0 + x) as usize] += d.samples;
                }
            }
        }
        samples
    }

    #[test]
    fn every_pixel_gets_every_sample_exactly_once() {
        for (size, samples, bounces) in [((640, 360), 8, 8), ((300, 1), 64, 16), ((1, 1), 1, 1)] {
            let dispatches = plan(size, samples, bounces, 0);
            assert!(
                coverage(size, &dispatches).iter().all(|&n| n == samples),
                "{size:?} at {samples} samples"
            );
        }
    }

    #[test]
    fn no_dispatch_traces_past_the_budget() {
        for (samples, bounces) in [(64, 16), (8, 8), (1, 16), (64, 1)] {
            for d in plan((1920, 1080), samples, bounces, 0) {
                let segments = u64::from(d.width * d.height * d.samples * bounces);
                assert!(segments <= DISPATCH_SEGMENTS, "{d:?} at {bounces} bounces");
            }
        }
        assert_eq!(samples_per_dispatch(8, 8), 8);
        assert_eq!(samples_per_dispatch(64, 16), 4);
    }

    #[test]
    fn a_chunk_adds_to_the_samples_the_earlier_ones_left() {
        let dispatches = plan((300, 200), 64, 16, 1000);
        let firsts: Vec<(u32, u32)> = dispatches
            .iter()
            .filter(|d| (d.x, d.y) == (0, 0))
            .map(|d| (d.first_sample, d.accumulated))
            .collect();
        let expected: Vec<(u32, u32)> =
            (0..16).map(|chunk| (chunk * 4, 1000 + chunk * 4)).collect();
        assert_eq!(firsts, expected);
        assert_eq!(dispatches.len(), 16 * 2);
    }
}
