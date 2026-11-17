//! The scene targets' share of the output size, and the Auto controller that steers it toward a frame-time target.

use std::ops::RangeInclusive;

/// The scales the editor offers; the tonemap upsamples the scene to the output.
pub const RENDER_SCALE_RANGE: RangeInclusive<f32> = 0.25..=1.0;
/// Auto moves in steps of this size, so the targets reallocate at a few sizes only.
pub const SCALE_STEP: f32 = 0.05;
/// The frame rates Auto may aim at.
pub const TARGET_FPS_RANGE: RangeInclusive<f32> = 20.0..=144.0;
/// Auto's default aim: 60 FPS, 16.7 ms.
pub const DEFAULT_TARGET_FPS: f32 = 60.0;

/// How large the scene renders against the output, and whether Auto steers it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderScale {
    /// Scene pixels per output pixel along each side, within [`RENDER_SCALE_RANGE`].
    pub scale: f32,
    /// The app moves `scale` toward `target_fps`; the renderer itself only reads `scale`.
    pub auto: bool,
    pub target_fps: f32,
}

impl Default for RenderScale {
    fn default() -> Self {
        Self {
            scale: 1.0,
            auto: false,
            target_fps: DEFAULT_TARGET_FPS,
        }
    }
}

impl RenderScale {
    /// `scale` within its range; a non-finite one renders at full size.
    pub fn clamped(&self) -> f32 {
        if self.scale.is_finite() {
            self.scale
                .clamp(*RENDER_SCALE_RANGE.start(), *RENDER_SCALE_RANGE.end())
        } else {
            1.0
        }
    }

    /// The scene targets' size for an output of `size`: each side scaled and rounded, at least 1.
    pub fn extent(&self, (width, height): (u32, u32)) -> (u32, u32) {
        let scale = self.clamped();
        let side = |side: u32| ((side as f32 * scale).round() as u32).clamp(1, side.max(1));
        (side(width), side(height))
    }

    /// The frame time Auto aims at, in milliseconds.
    pub fn target_ms(&self) -> f32 {
        1000.0
            / self
                .target_fps
                .clamp(*TARGET_FPS_RANGE.start(), *TARGET_FPS_RANGE.end())
    }
}

/// What Auto measures: the GPU's own pass time where timestamps exist, else the frame time, which vsync caps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameCost {
    Gpu(f32),
    Frame(f32),
}

/// Over this share of the target, Auto lowers the scale.
const OVER_BUDGET: f32 = 1.15;
/// Under this share of the GPU-timed target, Auto raises it.
const GPU_HEADROOM: f32 = 0.8;
/// A vsync-capped frame never reads under its refresh interval, so meeting the target counts as headroom.
const FRAME_HEADROOM: f32 = 1.05;
/// Seconds and frames between two changes, so the measurement covers frames at the new scale.
const MIN_INTERVAL_S: f64 = 1.0;
const MIN_FRAMES: u32 = 40;
/// Seconds of headroom before a step up; doubles after a step up that had to be taken back.
const UP_DELAY_S: f64 = 2.0;
const MAX_UP_DELAY_S: f64 = 32.0;
/// A step down this soon after a step up takes it back.
const TAKEN_BACK_S: f64 = 4.0;
/// The largest step down at once: a cost spike from a load must not drop the scale to its floor.
const MAX_STEP_DOWN: f32 = 0.25;

/// Auto's state between frames: when it last changed the scale, and since when the frame has had headroom.
#[derive(Clone, Debug)]
pub struct AutoScaler {
    last_change_s: Option<f64>,
    frames_since_change: u32,
    headroom_since_s: Option<f64>,
    last_up_s: Option<f64>,
    up_delay_s: f64,
}

impl Default for AutoScaler {
    fn default() -> Self {
        Self {
            last_change_s: None,
            frames_since_change: MIN_FRAMES,
            headroom_since_s: None,
            last_up_s: None,
            up_delay_s: UP_DELAY_S,
        }
    }
}

impl AutoScaler {
    /// One frame's measurement; returns the new scale when Auto changes it.
    pub fn update(&mut self, settings: &RenderScale, cost: FrameCost, now_s: f64) -> Option<f32> {
        self.frames_since_change = self.frames_since_change.saturating_add(1);
        let target = settings.target_ms();
        let (ms, headroom) = match cost {
            FrameCost::Gpu(ms) => (ms, target * GPU_HEADROOM),
            FrameCost::Frame(ms) => (ms, target * FRAME_HEADROOM),
        };
        if !ms.is_finite() || ms <= 0.0 {
            return None;
        }
        if ms >= headroom {
            self.headroom_since_s = None;
        } else {
            self.headroom_since_s.get_or_insert(now_s);
        }
        if !self.settled(now_s) {
            return None;
        }
        let scale = settings.clamped();
        let next = if ms > target * OVER_BUDGET {
            self.step_down(scale, target / ms, now_s)
        } else if self
            .headroom_since_s
            .is_some_and(|since| now_s - since >= self.up_delay_s)
        {
            self.step_up(scale, now_s)
        } else {
            None
        }?;
        self.last_change_s = Some(now_s);
        self.frames_since_change = 0;
        self.headroom_since_s = None;
        Some(next)
    }

    /// Enough time and frames since the last change for the measurement to show it.
    fn settled(&self, now_s: f64) -> bool {
        self.frames_since_change >= MIN_FRAMES
            && self
                .last_change_s
                .is_none_or(|last| now_s - last >= MIN_INTERVAL_S)
    }

    /// Pixels cost about the square of the scale, so the side shrinks by the root of the over-run.
    fn step_down(&mut self, scale: f32, budget_share: f32, now_s: f64) -> Option<f32> {
        let wanted = scale * budget_share.sqrt();
        let next = quantize_down(wanted.clamp(scale - MAX_STEP_DOWN, scale - SCALE_STEP));
        if self.last_up_s.is_some_and(|up| now_s - up < TAKEN_BACK_S) {
            self.up_delay_s = (self.up_delay_s * 2.0).min(MAX_UP_DELAY_S);
        }
        (next < scale).then_some(next)
    }

    fn step_up(&mut self, scale: f32, now_s: f64) -> Option<f32> {
        let next = quantize_down(scale + SCALE_STEP + 1e-4).min(*RENDER_SCALE_RANGE.end());
        self.last_up_s = Some(now_s);
        (next > scale).then_some(next)
    }
}

/// `scale` down to a multiple of [`SCALE_STEP`], within [`RENDER_SCALE_RANGE`].
fn quantize_down(scale: f32) -> f32 {
    let steps = (scale / SCALE_STEP + 1e-3).floor();
    (steps * SCALE_STEP).clamp(*RENDER_SCALE_RANGE.start(), *RENDER_SCALE_RANGE.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(scale: f32) -> RenderScale {
        RenderScale {
            scale,
            auto: true,
            ..RenderScale::default()
        }
    }

    /// Feeds `cost` once per 1/60 s from `start_s` for `seconds`, applying each change; returns the scale and the clock.
    fn run(
        scaler: &mut AutoScaler,
        settings: &mut RenderScale,
        cost: impl Fn(f32) -> FrameCost,
        (start_s, seconds): (f64, f64),
    ) -> f64 {
        let frames = (seconds * 60.0) as u32;
        let mut now = start_s;
        for _ in 0..frames {
            now += 1.0 / 60.0;
            if let Some(scale) = scaler.update(settings, cost(settings.scale), now) {
                settings.scale = scale;
            }
        }
        now
    }

    #[test]
    fn the_extent_scales_each_side_and_never_reaches_zero() {
        let half = RenderScale {
            scale: 0.5,
            ..RenderScale::default()
        };
        assert_eq!(half.extent((1928, 1207)), (964, 604));
        assert_eq!(RenderScale::default().extent((1928, 1207)), (1928, 1207));
        assert_eq!(half.extent((1, 1)), (1, 1));
        let tiny = RenderScale {
            scale: 0.01,
            ..RenderScale::default()
        };
        assert_eq!(tiny.extent((100, 100)), (25, 25), "clamped to a quarter");
        let broken = RenderScale {
            scale: f32::NAN,
            ..RenderScale::default()
        };
        assert_eq!(broken.extent((640, 360)), (640, 360));
        assert!((RenderScale::default().target_ms() - 16.667).abs() < 0.01);
    }

    #[test]
    fn an_over_budget_frame_lowers_the_scale_by_the_root_of_the_over_run() {
        let mut scaler = AutoScaler::default();
        let mut settings = auto(1.0);
        // 28 ms against 16.7: the side shrinks by sqrt(0.6) = 0.77, down to the step under it.
        let next = scaler.update(&settings, FrameCost::Frame(28.0), 0.0);
        assert_eq!(next, Some(0.75));
        settings.scale = 0.75;
        assert_eq!(
            scaler.update(&settings, FrameCost::Frame(28.0), 0.5),
            None,
            "the next change waits for a second and 40 frames"
        );
        let spike = AutoScaler::default().update(&auto(1.0), FrameCost::Frame(500.0), 10.0);
        assert_eq!(spike, Some(0.75), "a spike drops at most a quarter at once");
    }

    #[test]
    fn a_cost_within_the_band_changes_nothing() {
        let mut scaler = AutoScaler::default();
        let mut settings = auto(0.6);
        // 17.5 ms: over the GPU headroom, under the over-budget line.
        run(
            &mut scaler,
            &mut settings,
            |_| FrameCost::Gpu(17.5),
            (0.0, 20.0),
        );
        assert_eq!(settings.scale, 0.6);
    }

    #[test]
    fn gpu_headroom_raises_the_scale_a_step_at_a_time_up_to_full_size() {
        let mut scaler = AutoScaler::default();
        let mut settings = auto(0.5);
        let now = run(
            &mut scaler,
            &mut settings,
            |_| FrameCost::Gpu(5.0),
            (0.0, 2.5),
        );
        assert!((settings.scale - 0.55).abs() < 1e-4, "{}", settings.scale);
        run(
            &mut scaler,
            &mut settings,
            |_| FrameCost::Gpu(5.0),
            (now, 60.0),
        );
        assert_eq!(settings.scale, 1.0);
    }

    #[test]
    fn a_vsync_capped_frame_that_meets_the_target_counts_as_headroom() {
        let mut scaler = AutoScaler::default();
        let mut settings = auto(0.5);
        run(
            &mut scaler,
            &mut settings,
            |_| FrameCost::Frame(16.7),
            (0.0, 3.0),
        );
        assert!(settings.scale > 0.5, "{}", settings.scale);
        let mut settings = auto(0.5);
        let mut scaler = AutoScaler::default();
        run(
            &mut scaler,
            &mut settings,
            |_| FrameCost::Gpu(16.7),
            (0.0, 30.0),
        );
        assert_eq!(settings.scale, 0.5, "a GPU at its target has no headroom");
    }

    #[test]
    fn a_step_up_that_is_taken_back_waits_longer_before_the_next_and_settles() {
        let mut scaler = AutoScaler::default();
        let mut settings = auto(0.5);
        // Frames fit up to 0.7 and run 25 ms above it, the vsync-capped shape that would oscillate.
        let cost = |scale: f32| FrameCost::Frame(if scale > 0.7 + 1e-4 { 25.0 } else { 16.7 });
        let mut changes = Vec::new();
        let mut now = 0.0;
        for _ in 0..(120 * 60) {
            now += 1.0 / 60.0;
            if let Some(scale) = scaler.update(&settings, cost(settings.scale), now) {
                changes.push((now, scale));
                settings.scale = scale;
            }
        }
        assert!(settings.scale <= 0.75 + 1e-4, "{}", settings.scale);
        let late = changes.iter().filter(|(at, _)| *at > 60.0).count();
        assert!(late <= 4, "it keeps probing: {changes:?}");
        assert!(scaler.up_delay_s > UP_DELAY_S);
    }

    #[test]
    fn a_broken_measurement_changes_nothing() {
        let mut scaler = AutoScaler::default();
        for cost in [
            FrameCost::Gpu(f32::NAN),
            FrameCost::Frame(0.0),
            FrameCost::Gpu(-1.0),
        ] {
            assert_eq!(scaler.update(&auto(0.5), cost, 100.0), None);
        }
    }

    #[test]
    fn quantizing_lands_on_the_step_grid_within_the_range() {
        assert_eq!(quantize_down(0.77), 0.75);
        assert_eq!(quantize_down(0.75), 0.75);
        assert_eq!(quantize_down(0.1), 0.25);
        assert_eq!(quantize_down(1.3), 1.0);
    }
}
