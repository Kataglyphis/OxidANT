//! Clamped frame deltas for time-dependent effects such as auto-exposure adaptation.
//! Callers inject the timestamp because `std::time::Instant` panics on wasm.

/// Delta cap, so a suspended tab or debugger pause cannot slam auto-exposure with a huge step.
pub const MAX_DELTA_SECONDS: f32 = 0.25;

/// Delta reported before the first real tick.
pub const NOMINAL_DELTA_SECONDS: f32 = 1.0 / 60.0;

/// Turns monotonic seconds-since-epoch readings into clamped frame-to-frame deltas.
#[derive(Default)]
pub struct FrameClock {
    last_seconds: Option<f64>,
}

impl FrameClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clamped delta since the last tick; `now_seconds` comes from a monotonic source.
    /// A backwards or repeated reading yields `0.0`, never a negative delta.
    pub fn tick_at(&mut self, now_seconds: f64) -> f32 {
        let delta = match self.last_seconds {
            None => NOMINAL_DELTA_SECONDS,
            Some(last) => ((now_seconds - last) as f32).clamp(0.0, MAX_DELTA_SECONDS),
        };
        self.last_seconds = Some(now_seconds);
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tick_is_nominal() {
        let mut clock = FrameClock::new();
        assert_eq!(clock.tick_at(1_234.0), NOMINAL_DELTA_SECONDS);
    }

    #[test]
    fn consecutive_ticks_match_the_injected_gap() {
        let mut clock = FrameClock::new();
        clock.tick_at(10.0);
        assert!((clock.tick_at(10.016) - 0.016).abs() < 1e-6);
        assert!((clock.tick_at(10.1) - (10.1 - 10.016)).abs() < 1e-6);
    }

    #[test]
    fn a_five_second_gap_clamps_instead_of_slamming_the_caller() {
        let mut clock = FrameClock::new();
        clock.tick_at(0.0);
        assert_eq!(clock.tick_at(5.0), MAX_DELTA_SECONDS);
    }

    #[test]
    fn a_backwards_or_repeated_reading_clamps_to_zero_not_negative() {
        // A coarse or glitching timer must not hand adapt_exposure_ev a negative delta.
        let mut clock = FrameClock::new();
        clock.tick_at(10.0);
        assert_eq!(clock.tick_at(9.5), 0.0);
        assert_eq!(clock.tick_at(9.5), 0.0);
    }
}
