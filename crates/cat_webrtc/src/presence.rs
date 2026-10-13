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

/// A box of the picture as fractions of its width and height: x1, y1, x2, y2.
pub type Zone = [f32; 4];

/// A detection that overlaps a spot marked "not a cat" by more than this, as intersection over
/// union, is dropped: the same object jitters far less, a cat in front of it differs in size.
const IGNORE_IOU: f32 = 0.5;

/// `detections` without those over a spot in `zones`, fractions of the `width` x `height` frame.
pub fn outside(
    detections: Vec<Detection>,
    zones: &[Zone],
    width: u32,
    height: u32,
) -> Vec<Detection> {
    if zones.is_empty() {
        return detections;
    }
    detections
        .into_iter()
        .filter(|d| {
            let zone = zone_of(d, width, height);
            !zones.iter().any(|z| iou(&zone, z) > IGNORE_IOU)
        })
        .collect()
}

/// Where `detection` lies, as fractions of the `width` x `height` frame.
pub fn zone_of(detection: &Detection, width: u32, height: u32) -> Zone {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    [
        detection.x1 / w,
        detection.y1 / h,
        detection.x2 / w,
        detection.y2 / h,
    ]
}

fn iou(a: &Zone, b: &Zone) -> f32 {
    let area = |z: &Zone| (z[2] - z[0]).max(0.0) * (z[3] - z[1]).max(0.0);
    let overlap =
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let union = area(a) + area(b) - overlap;
    if union > 0.0 {
        overlap / union
    } else {
        0.0
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

    fn at(x1: f32, y1: f32, x2: f32, y2: f32, score: f32) -> Detection {
        Detection {
            x1,
            y1,
            x2,
            y2,
            score,
            class_id: 15,
        }
    }

    #[test]
    fn a_detection_over_an_ignored_spot_is_dropped_and_a_cat_beside_or_before_it_is_not() {
        // The drying rack at the bottom edge of a 640x480 frame, marked "not a cat".
        let rack = [0.25, 0.75, 0.75, 1.0];
        let zones = [rack];
        let detections = vec![
            // The rack again, a few pixels off: dropped.
            at(165.0, 362.0, 478.0, 479.0, 0.34),
            // A cat sitting in front of it, a much smaller box: kept.
            at(280.0, 380.0, 360.0, 470.0, 0.81),
            // A cat elsewhere: kept.
            at(20.0, 20.0, 120.0, 140.0, 0.77),
        ];
        let kept = outside(detections.clone(), &zones, 640, 480);
        assert_eq!(kept, detections[1..]);
        assert_eq!(outside(detections.clone(), &[], 640, 480), detections);
        assert_eq!(
            zone_of(&at(160.0, 360.0, 480.0, 480.0, 0.5), 640, 480),
            rack
        );
        assert_eq!(iou(&rack, &rack), 1.0);
        assert_eq!(iou(&rack, &[0.0, 0.0, 0.1, 0.1]), 0.0);
        assert_eq!(iou(&[0.5, 0.5, 0.5, 0.5], &[0.5, 0.5, 0.5, 0.5]), 0.0);
    }
}
