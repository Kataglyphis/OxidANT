//! When a frame is worth the model: something moved since the last look, a cat is in view or was
//! just seen, or the idle period ran out on a frame that is not black. A still, empty room then
//! costs one run every few seconds, and a dark one none. A still cat, asleep or a false one such
//! as a drying rack, costs one run every [`still_period`]: leaving is motion, which restores the
//! full rate.

use std::time::{Duration, Instant};

/// Side of the square blocks a thumbnail averages; 640x480 becomes 80x60.
const BLOCK: usize = 8;

/// A block whose mean luma moved by more than this counts as changed (of 255).
const CHANGED_LUMA: u8 = 12;

/// Motion is at least this many changed blocks per 10 000: 24 of the 4 800 at 640x480.
const MOTION_PER_10K: usize = 50;

/// A frame whose blocks all sit at or below this luma is black (limited-range black is 16).
const DARK_LUMA: u8 = 20;

/// Mean of each `BLOCK`x`BLOCK` block of a luma plane, row by row; partial edge blocks are dropped.
pub fn thumbnail(luma: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (cols, rows) = (w / BLOCK, h / BLOCK);
    let mut sums = vec![0u32; cols * rows];
    for y in 0..rows * BLOCK {
        let row = &luma[y * w..y * w + cols * BLOCK];
        let sums_row = &mut sums[(y / BLOCK) * cols..(y / BLOCK + 1) * cols];
        for (x, &value) in row.iter().enumerate() {
            sums_row[x / BLOCK] += u32::from(value);
        }
    }
    let area = (BLOCK * BLOCK) as u32;
    sums.into_iter().map(|s| (s / area) as u8).collect()
}

/// Whether enough blocks changed between two thumbnails of one frame size.
pub fn moved(previous: &[u8], current: &[u8]) -> bool {
    if previous.len() != current.len() {
        return true;
    }
    let changed = previous
        .iter()
        .zip(current)
        .filter(|(a, b)| a.abs_diff(**b) > CHANGED_LUMA)
        .count();
    changed > 0 && changed * 10_000 >= MOTION_PER_10K * current.len()
}

/// A cat in view with nothing moving is looked at this many times less often than the full rate.
const STILL_SLOWDOWN: u32 = 2;

/// The longest time between two runs while a cat is in view; [`crate::presence::Presence`] lingers past it.
pub fn still_period(inference_fps: f32) -> Duration {
    rate_period(inference_fps) * STILL_SLOWDOWN
}

/// Decides which frames go to the model.
#[derive(Debug)]
pub struct Pacer {
    period: Duration,
    /// The least time between runs while nothing moves; `None` turns the motion gate off.
    idle: Option<Duration>,
    next_look: Instant,
    last_run: Option<Instant>,
    previous: Option<Vec<u8>>,
    /// A frame was wanted but the busy model could not take it yet.
    pending: bool,
}

impl Pacer {
    /// The gate is off when the model is uncapped (`inference_fps = 0`) or idles no slower than it runs.
    pub fn new(inference_fps: f32, idle_inference_fps: f32, now: Instant) -> Self {
        let period = rate_period(inference_fps);
        let idle = (inference_fps > 0.0 && idle_inference_fps < inference_fps)
            .then(|| rate_period(idle_inference_fps));
        Self {
            period,
            idle,
            next_look: now,
            last_run: None,
            previous: None,
            pending: false,
        }
    }

    /// Whether this frame goes to the model; called on every frame, it looks at most once a period.
    pub fn wants(
        &mut self,
        now: Instant,
        luma: &[u8],
        width: u32,
        height: u32,
        cat_seen: bool,
    ) -> bool {
        if self.pending {
            return true;
        }
        if now < self.next_look {
            return false;
        }
        self.next_look = now + self.period;
        let Some(idle) = self.idle else {
            self.pending = true;
            return true;
        };
        let current = thumbnail(luma, width, height);
        let motion = self.previous.as_deref().is_none_or(|p| moved(p, &current));
        // A camera without IR sees black at night, where the model can find nothing.
        let dark = current.iter().all(|&v| v <= DARK_LUMA);
        self.previous = Some(current);
        let since_run = self.last_run.map(|t| now.duration_since(t));
        let idle_over = since_run.is_none_or(|d| d >= idle);
        let still_over = since_run.is_none_or(|d| d >= self.period * STILL_SLOWDOWN);
        self.pending = motion || (cat_seen && still_over) || (idle_over && !dark);
        self.pending
    }

    /// The model took the frame [`Self::wants`] asked for.
    pub fn ran(&mut self, now: Instant) {
        self.last_run = Some(now);
        self.pending = false;
    }
}

/// The least time between two runs at `fps`; zero for an uncapped rate.
pub fn rate_period(fps: f32) -> Duration {
    if fps > 0.0 {
        Duration::from_secs_f32(1.0 / fps)
    } else {
        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 64;
    const H: u32 = 48;

    fn frame(luma: u8) -> Vec<u8> {
        vec![luma; (W * H) as usize]
    }

    /// `frame(base)` with a `side`x`side` square at the origin painted `luma`.
    fn with_square(base: u8, side: u32, luma: u8) -> Vec<u8> {
        let mut f = frame(base);
        for y in 0..side {
            for x in 0..side {
                f[(y * W + x) as usize] = luma;
            }
        }
        f
    }

    #[test]
    fn a_thumbnail_is_the_mean_luma_of_each_block() {
        let t = thumbnail(&with_square(10, 8, 250), W, H);
        assert_eq!(t.len(), 8 * 6);
        assert_eq!(t[0], 250);
        assert!(t[1..].iter().all(|&v| v == 10));
    }

    #[test]
    fn noise_is_not_motion_and_a_moving_square_is() {
        let still = thumbnail(&frame(100), W, H);
        let noise = thumbnail(&frame(108), W, H);
        assert!(!moved(&still, &noise));
        let square = thumbnail(&with_square(100, 16, 200), W, H);
        assert!(moved(&still, &square));
        assert!(moved(&still, &still[..10]));
    }

    #[test]
    fn a_still_room_runs_the_model_once_an_idle_period() {
        let t = Instant::now();
        let mut p = Pacer::new(2.0, 0.2, t);
        let f = frame(50);
        assert!(p.wants(t, &f, W, H, false));
        p.ran(t);
        let look = |s: f32| t + Duration::from_secs_f32(s);
        assert!(!p.wants(look(0.2), &f, W, H, false));
        assert!(!p.wants(look(0.5), &f, W, H, false));
        assert!(!p.wants(look(4.5), &f, W, H, false));
        assert!(p.wants(look(5.0), &f, W, H, false));
    }

    #[test]
    fn motion_runs_it_at_the_full_rate_and_a_still_cat_at_half_of_it() {
        let t = Instant::now();
        let at = |ms: u64| t + Duration::from_millis(ms);
        let mut p = Pacer::new(2.0, 0.2, t);
        assert!(p.wants(t, &frame(50), W, H, false));
        p.ran(t);
        let cat = with_square(50, 24, 220);
        assert!(p.wants(at(500), &cat, W, H, false), "motion");
        p.ran(at(500));
        // The cat sits still: every other look, once a second.
        assert!(!p.wants(at(1000), &cat, W, H, true));
        assert!(p.wants(at(1500), &cat, W, H, true));
        p.ran(at(1500));
        assert!(!p.wants(at(2000), &cat, W, H, true));
        // It gets up: the full rate again at once.
        assert!(p.wants(at(2500), &frame(50), W, H, true));
        p.ran(at(2500));
        assert!(p.wants(at(3000), &cat, W, H, true));
        assert_eq!(still_period(2.0), Duration::from_secs(1));
    }

    #[test]
    fn a_black_frame_skips_the_idle_run_until_light_or_a_cat_comes_back() {
        let t = Instant::now();
        let at = |s: f32| t + Duration::from_secs_f32(s);
        let mut p = Pacer::new(2.0, 0.2, t);
        assert!(p.wants(t, &frame(16), W, H, false));
        p.ran(t);
        assert!(!p.wants(at(5.0), &frame(16), W, H, false));
        assert!(!p.wants(at(9.5), &frame(18), W, H, false));
        assert!(p.wants(at(10.0), &frame(16), W, H, true));
        p.ran(at(10.0));
        assert!(p.wants(at(10.5), &frame(120), W, H, false));
    }

    #[test]
    fn a_frame_the_busy_model_refused_is_offered_again() {
        let t = Instant::now();
        let mut p = Pacer::new(2.0, 0.2, t);
        assert!(p.wants(t, &frame(50), W, H, false));
        let next = t + Duration::from_millis(33);
        assert!(p.wants(next, &frame(50), W, H, false));
        p.ran(next);
        assert!(!p.wants(next + Duration::from_millis(33), &frame(50), W, H, false));
    }

    #[test]
    fn an_uncapped_model_or_an_idle_rate_at_the_full_one_turns_the_gate_off() {
        let t = Instant::now();
        let f = frame(50);
        let mut uncapped = Pacer::new(0.0, 0.2, t);
        for i in 0..3 {
            let now = t + Duration::from_millis(33 * i);
            assert!(uncapped.wants(now, &f, W, H, false));
            uncapped.ran(now);
        }
        let mut same = Pacer::new(2.0, 2.0, t);
        assert!(same.wants(t, &f, W, H, false));
        same.ran(t);
        assert!(same.wants(t + Duration::from_millis(500), &f, W, H, false));
    }
}
