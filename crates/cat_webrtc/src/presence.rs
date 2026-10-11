//! Whether a cat is in view. A lone hit at the score threshold is mostly noise, so a cat arrives
//! after [`CONFIRM`] hits in a row and leaves once no hit came for the linger time.

use std::time::{Duration, Instant};

use kataglyphis_core::Detection;

/// Hits in a row before a cat counts as arrived.
pub const CONFIRM: u32 = 2;

/// The shortest linger; slower inference rates linger longer, so one missed run never ends a visit.
const MIN_LINGER: Duration = Duration::from_secs(3);

/// What an inference result changed.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Arrived { count: usize, best: f32 },
    Left { stayed: Duration },
}

#[derive(Debug)]
pub struct Presence {
    linger: Duration,
    streak: u32,
    since: Option<Instant>,
    last_hit: Option<Instant>,
}

impl Presence {
    /// `period` is the least time between two inference runs.
    pub fn new(period: Duration) -> Self {
        Self {
            linger: MIN_LINGER.max(period.mul_f32(2.5)),
            streak: 0,
            since: None,
            last_hit: None,
        }
    }

    pub fn in_view(&self) -> bool {
        self.since.is_some()
    }

    /// Feeds one inference result.
    pub fn update(&mut self, now: Instant, detections: &[Detection]) -> Option<Change> {
        if let Some(best) = best_score(detections) {
            self.streak = self.streak.saturating_add(1);
            self.last_hit = Some(now);
            if self.since.is_none() && self.streak >= CONFIRM {
                self.since = Some(now);
                return Some(Change::Arrived {
                    count: detections.len(),
                    best,
                });
            }
            return None;
        }
        self.streak = 0;
        let since = self.since?;
        let last = self.last_hit.unwrap_or(since);
        if now.duration_since(last) < self.linger {
            return None;
        }
        self.since = None;
        Some(Change::Left {
            stayed: last.duration_since(since),
        })
    }
}

/// The highest score; the model's output order is not a ranking.
pub fn best_score(detections: &[Detection]) -> Option<f32> {
    detections.iter().map(|d| d.score).reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(score: f32) -> Detection {
        Detection {
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 10.0,
            score,
            class_id: 15,
        }
    }

    const HALF: Duration = Duration::from_millis(500);

    #[test]
    fn a_lone_hit_is_not_a_cat() {
        let t = Instant::now();
        let mut p = Presence::new(HALF);
        assert_eq!(p.update(t, &[cat(0.26)]), None);
        assert_eq!(p.update(t + HALF, &[]), None);
        assert_eq!(p.update(t + HALF * 2, &[cat(0.27)]), None);
        assert!(!p.in_view());
    }

    #[test]
    fn two_hits_in_a_row_arrive_and_a_missed_run_does_not_leave() {
        let t = Instant::now();
        let mut p = Presence::new(HALF);
        p.update(t, &[cat(0.6)]);
        assert_eq!(
            p.update(t + HALF, &[cat(0.3), cat(0.7)]),
            Some(Change::Arrived {
                count: 2,
                best: 0.7
            })
        );
        assert_eq!(p.update(t + HALF * 2, &[]), None);
        assert_eq!(p.update(t + HALF * 3, &[cat(0.5)]), None);
        assert!(p.in_view());
    }

    #[test]
    fn it_leaves_after_the_linger_and_reports_the_visit() {
        let t = Instant::now();
        let mut p = Presence::new(HALF);
        p.update(t, &[cat(0.6)]);
        p.update(t + HALF, &[cat(0.6)]);
        p.update(t + Duration::from_secs(10), &[cat(0.6)]);
        let quiet = t + Duration::from_secs(12);
        assert_eq!(p.update(quiet, &[]), None);
        assert_eq!(
            p.update(t + Duration::from_secs(13), &[]),
            Some(Change::Left {
                stayed: Duration::from_millis(9500)
            })
        );
        assert!(!p.in_view());
    }

    #[test]
    fn a_slow_inference_rate_lingers_longer() {
        let t = Instant::now();
        let mut p = Presence::new(Duration::from_secs(4));
        p.update(t, &[cat(0.6)]);
        p.update(t + Duration::from_secs(4), &[cat(0.6)]);
        assert_eq!(p.update(t + Duration::from_secs(12), &[]), None);
        assert!(p.update(t + Duration::from_secs(14), &[]).is_some());
    }
}
