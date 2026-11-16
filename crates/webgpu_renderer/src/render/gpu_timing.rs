//! Per-pass GPU timestamp queries, in the C++ engine's `gpuTimings` shape so the two compare.
//! Stamps at pass boundaries: `write_timestamp` needs a feature WebGPU never exposes.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::render::buffer_desc;

/// The passes we time, in the order they are recorded in a frame.
/// Opaque and blended share one: splitting that render pass to time it would change its cost.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TimedPass {
    /// All shadow cascades, one render pass each.
    ShadowCascades,
    /// Opaque geometry, sky, and blended geometry in one render pass.
    Forward,
    /// The deferred path's opaque geometry into the G-buffer, in place of `Forward`.
    GBuffer,
    /// The deferred path's full-screen lighting, then its sky and blended geometry in the same pass.
    DeferredLighting,
    /// The occlusion-query box pass (only with culling on); its cost is what the cull must beat.
    OcclusionCull,
    /// Histogram clear plus build (two compute passes).
    Histogram,
    /// Histogram -> adapted exposure reduction (one compute pass).
    ExposureReduce,
    /// Brightpass plus separable blur (three render passes).
    Bloom,
    /// Occlusion plus blur (two render passes).
    Ssao,
    /// The volumetric clouds' march to the scene depth (one render pass; only with clouds on).
    Clouds,
    /// ACES tonemap and composite to the output target.
    Tonemap,
    /// The ray-traced mode's kernel, every tile in one compute pass; it replaces the shadow and forward passes.
    RayTrace,
    /// The path-traced mode's kernel, every tile and sample chunk in one compute pass.
    PathTrace,
    /// The traced modes' copy of their accumulation into the HDR target.
    TraceResolve,
    /// FXAA over the tonemapped frame into the output target (deferred frames by default).
    Fxaa,
}

impl TimedPass {
    /// Every pass in record order: a raster frame's skip the traced passes, a traced frame's skip shadows to SSAO.
    /// Iteration order is the reporting order, so either kind of frame lists its passes as it records them.
    pub const ALL: [TimedPass; 15] = [
        TimedPass::ShadowCascades,
        TimedPass::Forward,
        TimedPass::GBuffer,
        TimedPass::DeferredLighting,
        TimedPass::OcclusionCull,
        TimedPass::RayTrace,
        TimedPass::PathTrace,
        TimedPass::TraceResolve,
        TimedPass::Histogram,
        TimedPass::ExposureReduce,
        TimedPass::Bloom,
        TimedPass::Ssao,
        TimedPass::Clouds,
        TimedPass::Tonemap,
        TimedPass::Fxaa,
    ];

    /// Stable identifier for reports and for matching against the C++ engine.
    pub fn name(self) -> &'static str {
        match self {
            TimedPass::ShadowCascades => "ShadowCascades",
            TimedPass::Forward => "Forward",
            TimedPass::OcclusionCull => "OcclusionCull",
            TimedPass::Bloom => "Bloom",
            TimedPass::Ssao => "Ssao",
            TimedPass::Histogram => "Histogram",
            TimedPass::ExposureReduce => "ExposureReduce",
            TimedPass::Tonemap => "Tonemap",
            TimedPass::GBuffer => "GBuffer",
            TimedPass::DeferredLighting => "DeferredLighting",
            TimedPass::Clouds => "Clouds",
            TimedPass::RayTrace => "RayTrace",
            TimedPass::PathTrace => "PathTrace",
            TimedPass::TraceResolve => "TraceResolve",
            TimedPass::Fxaa => "Fxaa",
        }
    }

    /// What the profiler shows; [`Self::name`] stays the identifier reports and scripts parse.
    pub fn label(self) -> &'static str {
        match self {
            TimedPass::ShadowCascades => "Shadow cascades",
            TimedPass::Forward => "Forward + sky",
            TimedPass::OcclusionCull => "Occlusion cull",
            TimedPass::Bloom => "Bloom",
            TimedPass::Ssao => "SSAO",
            TimedPass::Histogram => "Histogram",
            TimedPass::ExposureReduce => "Exposure",
            TimedPass::Tonemap => "Tonemap",
            TimedPass::GBuffer => "G-buffer",
            TimedPass::DeferredLighting => "Lighting + sky",
            TimedPass::Clouds => "Clouds",
            TimedPass::RayTrace => "Ray trace",
            TimedPass::PathTrace => "Path trace",
            TimedPass::TraceResolve => "Trace resolve",
            TimedPass::Fxaa => "FXAA",
        }
    }

    /// Recorded only by a ray- or path-traced frame, which records no shadow, forward, occlusion or SSAO pass.
    pub fn is_traced(self) -> bool {
        matches!(
            self,
            TimedPass::RayTrace | TimedPass::PathTrace | TimedPass::TraceResolve
        )
    }

    fn index(self) -> usize {
        match self {
            TimedPass::ShadowCascades => 0,
            TimedPass::Forward => 1,
            TimedPass::OcclusionCull => 2,
            TimedPass::Bloom => 3,
            TimedPass::Ssao => 4,
            TimedPass::Histogram => 5,
            TimedPass::ExposureReduce => 6,
            TimedPass::Tonemap => 7,
            TimedPass::GBuffer => 8,
            TimedPass::DeferredLighting => 9,
            TimedPass::Clouds => 10,
            TimedPass::RayTrace => 11,
            TimedPass::PathTrace => 12,
            TimedPass::TraceResolve => 13,
            TimedPass::Fxaa => 14,
        }
    }
}

/// Number of timed passes.
pub const PASS_COUNT: usize = TimedPass::ALL.len();
const _: () = assert!(
    PASS_COUNT * 2 <= 32,
    "a frame's written-stamp mask holds two bits per pass in a u32"
);
/// Two queries (begin, end) per pass.
const QUERIES_PER_SLOT: u32 = PASS_COUNT as u32 * 2;
/// Frames in flight: latency 2 plus a spare, since a slot frees only once a poll maps it.
const SLOT_COUNT: usize = 3;
/// Samples averaged per reported value: about half a second at 60 Hz, readable yet responsive.
const AVERAGE_WINDOW: usize = 32;

/// Fixed-window mean: an exact frame count a comparison harness can reproduce, unlike a decay.
#[derive(Clone, Debug)]
pub struct RollingAverage {
    samples: [f32; AVERAGE_WINDOW],
    /// Where the next sample lands.
    next: usize,
    /// Samples recorded so far, saturating at the window size.
    filled: usize,
    sum: f32,
}

impl Default for RollingAverage {
    fn default() -> Self {
        Self {
            samples: [0.0; AVERAGE_WINDOW],
            next: 0,
            filled: 0,
            sum: 0.0,
        }
    }
}

impl RollingAverage {
    /// Records one sample, evicting the oldest once the window is full.
    pub fn push(&mut self, value: f32) {
        if self.filled == AVERAGE_WINDOW {
            self.sum -= self.samples[self.next];
        } else {
            self.filled += 1;
        }
        self.samples[self.next] = value;
        self.sum += value;
        self.next = (self.next + 1) % AVERAGE_WINDOW;
    }

    /// Mean of the recorded samples, or `None` before the first sample.
    /// `None`, not 0.0: never having reported is not the same as taking no time.
    pub fn average(&self) -> Option<f32> {
        if self.filled == 0 {
            None
        } else {
            Some(self.sum / self.filled as f32)
        }
    }

    /// Number of samples currently in the window.
    pub fn len(&self) -> usize {
        self.filled
    }

    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    /// Forgets every sample, so the pass reads as not reporting.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Converts a GPU timestamp delta to milliseconds.
/// `period_ns` differs by vendor; the math is `f64` since tick deltas outgrow `f32`'s mantissa.
pub fn ticks_to_ms(ticks: u64, period_ns: f32) -> f32 {
    (ticks as f64 * period_ns as f64 / 1_000_000.0) as f32
}

/// The timestamp writes one pass should hand to its render/compute passes.
/// Safe when timing is off: every accessor then yields `None`, so call sites need no branch.
#[derive(Copy, Clone)]
pub struct PassScope<'a> {
    query_set: Option<&'a wgpu::QuerySet>,
    begin: u32,
    end: u32,
    /// The frame's written-stamp mask and this pass's begin bit; the end bit is the next one up.
    written: Option<(&'a Cell<u32>, u32)>,
}

// Writes borrow the query set (`'a`), not the scope, so an inline temporary scope still works.
impl<'a> PassScope<'a> {
    /// A scope that writes nothing - timing off, or feature unavailable.
    pub fn disabled() -> Self {
        Self {
            query_set: None,
            begin: 0,
            end: 0,
            written: None,
        }
    }

    /// True when this scope will actually write timestamps.
    pub fn is_active(&self) -> bool {
        self.query_set.is_some()
    }

    /// Timestamp writes for sub-pass `sub` of `count` in this scope.
    /// Begin rides the first sub-pass and end the last, so the span covers the gaps too.
    pub fn render_writes(
        &self,
        sub: usize,
        count: usize,
    ) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
        let (query_set, beginning, end) = self.indices(sub, count)?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: beginning,
            end_of_pass_write_index: end,
        })
    }

    /// As [`Self::render_writes`], for compute passes.
    pub fn compute_writes(
        &self,
        sub: usize,
        count: usize,
    ) -> Option<wgpu::ComputePassTimestampWrites<'a>> {
        let (query_set, beginning, end) = self.indices(sub, count)?;
        Some(wgpu::ComputePassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: beginning,
            end_of_pass_write_index: end,
        })
    }

    /// The indices sub-pass `sub` writes, noting each stamp it hands out in the frame's mask.
    fn indices(
        &self,
        sub: usize,
        count: usize,
    ) -> Option<(&'a wgpu::QuerySet, Option<u32>, Option<u32>)> {
        let query_set = self.query_set?;
        let (beginning, end) = sub_pass_write_indices(sub, count, self.begin, self.end)?;
        if let Some((mask, begin_bit)) = self.written {
            let stamped = u32::from(beginning.is_some()) | (u32::from(end.is_some()) << 1);
            mask.set(mask.get() | (stamped << begin_bit));
        }
        Some((query_set, beginning, end))
    }
}

/// Which of a scope's two queries sub-pass `sub` of `count` should write.
/// `None` means no descriptor at all: wgpu rejects one with both indices unset.
fn sub_pass_write_indices(
    sub: usize,
    count: usize,
    begin: u32,
    end: u32,
) -> Option<(Option<u32>, Option<u32>)> {
    let beginning = (sub == 0).then_some(begin);
    let last = (count > 0 && sub + 1 == count).then_some(end);
    if beginning.is_none() && last.is_none() {
        return None;
    }
    Some((beginning, last))
}

/// Map state of one readback slot, shared with the `map_async` callback.
const MAP_PENDING: u8 = 0;
const MAP_READY: u8 = 1;
const MAP_FAILED: u8 = 2;

struct Slot {
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// `Some` while a map is outstanding; the callback sets READY/FAILED for `end_frame` to drain.
    map_state: Option<Arc<AtomicU8>>,
    /// Which stamps this slot's frame wrote, two bits per pass; an unwritten query resolves stale.
    written: u32,
}

struct Resources {
    query_set: wgpu::QuerySet,
    slots: Vec<Slot>,
    /// Slot the current frame writes into.
    current: usize,
    period_ns: f32,
}

/// Per-pass GPU timings, or an inert stand-in when timestamps are unavailable.
pub struct GpuTiming {
    /// `None` when `TIMESTAMP_QUERY` is absent or timing was never enabled.
    resources: Option<Resources>,
    /// Set by `begin_frame` when this frame found a free slot to record into.
    recording: bool,
    averages: Vec<RollingAverage>,
    /// The stamps this frame's passes were handed, two bits per pass (begin, end).
    written: Cell<u32>,
    /// Measured frames in a row each pass has not run in; a full window clears its average.
    frames_without: [usize; PASS_COUNT],
}

impl GpuTiming {
    /// Inert timing with no queries; takes no device so GPU-less unit tests can reach it.
    pub fn unavailable() -> Self {
        Self {
            resources: None,
            recording: false,
            averages: vec![RollingAverage::default(); PASS_COUNT],
            written: Cell::new(0),
            frames_without: [0; PASS_COUNT],
        }
    }

    /// Allocates the queries and readbacks, or goes inert without `TIMESTAMP_QUERY`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            log::info!("TIMESTAMP_QUERY unavailable; per-pass GPU timings disabled");
            return Self::unavailable();
        }

        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_timing_queries"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERIES_PER_SLOT * SLOT_COUNT as u32,
        });
        let bytes = u64::from(QUERIES_PER_SLOT) * 8;
        let slots = (0..SLOT_COUNT)
            .map(|i| Slot {
                resolve: buffer_desc::query_resolve(
                    device,
                    &format!("gpu_timing_resolve_{i}"),
                    bytes,
                ),
                readback: buffer_desc::readback(device, &format!("gpu_timing_readback_{i}"), bytes),
                map_state: None,
                written: 0,
            })
            .collect();

        Self {
            resources: Some(Resources {
                query_set,
                slots,
                current: 0,
                period_ns: queue.get_timestamp_period(),
            }),
            recording: false,
            averages: vec![RollingAverage::default(); PASS_COUNT],
            written: Cell::new(0),
            frames_without: [0; PASS_COUNT],
        }
    }

    /// True when timestamps are being collected.
    pub fn is_available(&self) -> bool {
        self.resources.is_some()
    }

    /// Nanoseconds per timestamp tick, as reported by the queue.
    pub fn timestamp_period_ns(&self) -> Option<f32> {
        self.resources.as_ref().map(|r| r.period_ns)
    }

    /// Averaged duration of each pass that has reported at least once.
    /// A pass with no sample yet is absent, which a caller must not read as zero.
    pub fn timings_ms(&self) -> Vec<(&'static str, f32)> {
        TimedPass::ALL
            .iter()
            .filter_map(|pass| {
                self.averages[pass.index()]
                    .average()
                    .map(|ms| (pass.name(), ms))
            })
            .collect()
    }

    /// Drops every pass `keep` rejects: its average, and its stamps in readbacks still in flight, so a pass the
    /// frame stopped running leaves the report now rather than a full window later.
    pub fn forget(&mut self, keep: impl Fn(TimedPass) -> bool) {
        for pass in TimedPass::ALL.into_iter().filter(|pass| !keep(*pass)) {
            let i = pass.index();
            self.averages[i].clear();
            self.frames_without[i] = AVERAGE_WINDOW;
            if let Some(resources) = self.resources.as_mut() {
                for slot in &mut resources.slots {
                    slot.written &= !(0b11 << (i * 2));
                }
            }
        }
    }

    /// Opens the frame: claims a slot if one is free.
    /// With every slot still mapping the frame goes untimed: dropping a sample beats stalling.
    pub fn begin_frame(&mut self) {
        self.recording = false;
        self.written.set(0);
        let Some(resources) = self.resources.as_mut() else {
            return;
        };
        for offset in 0..SLOT_COUNT {
            let candidate = (resources.current + offset) % SLOT_COUNT;
            if resources.slots[candidate].map_state.is_none() {
                resources.current = candidate;
                self.recording = true;
                return;
            }
        }
    }

    /// The scope `pass` should stamp into this frame.
    pub fn scope(&self, pass: TimedPass) -> PassScope<'_> {
        if !self.recording {
            return PassScope::disabled();
        }
        let Some(resources) = self.resources.as_ref() else {
            return PassScope::disabled();
        };
        let base = resources.current as u32 * QUERIES_PER_SLOT + pass.index() as u32 * 2;
        PassScope {
            query_set: Some(&resources.query_set),
            begin: base,
            end: base + 1,
            written: Some((&self.written, pass.index() as u32 * 2)),
        }
    }

    /// Resolves this frame's queries; record into the passes' encoder before submit.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.recording {
            return;
        }
        let Some(resources) = self.resources.as_mut() else {
            return;
        };
        let slot = &resources.slots[resources.current];
        let first = resources.current as u32 * QUERIES_PER_SLOT;
        encoder.resolve_query_set(
            &resources.query_set,
            first..first + QUERIES_PER_SLOT,
            &slot.resolve,
            0,
        );
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, slot.resolve.size());
    }

    /// Closes the frame: maps this slot and consumes any finished readbacks, without blocking.
    pub fn end_frame(&mut self, device: &wgpu::Device) {
        let Some(resources) = self.resources.as_mut() else {
            return;
        };
        if self.recording {
            let state = Arc::new(AtomicU8::new(MAP_PENDING));
            let callback_state = Arc::clone(&state);
            resources.slots[resources.current]
                .readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let code = if result.is_ok() {
                        MAP_READY
                    } else {
                        MAP_FAILED
                    };
                    callback_state.store(code, Ordering::Release);
                });
            resources.slots[resources.current].map_state = Some(state);
            resources.slots[resources.current].written = self.written.get();
            resources.current = (resources.current + 1) % SLOT_COUNT;
            self.recording = false;
        }

        // Poll, never wait: a blocking poll would bring back the stall the ring exists to avoid.
        let _ = device.poll(wgpu::PollType::Poll);

        for index in 0..resources.slots.len() {
            let Some(state) = resources.slots[index].map_state.as_ref() else {
                continue;
            };
            match state.load(Ordering::Acquire) {
                MAP_READY => {
                    let ticks = read_ticks(&resources.slots[index].readback);
                    resources.slots[index].readback.unmap();
                    resources.slots[index].map_state = None;
                    let mut history = PassHistory {
                        averages: &mut self.averages,
                        frames_without: &mut self.frames_without,
                    };
                    history.record(&ticks, resources.slots[index].written, resources.period_ns);
                }
                MAP_FAILED => {
                    resources.slots[index].map_state = None;
                }
                _ => {}
            }
        }
    }
}

/// The per-pass samples and run streaks `end_frame` updates from one measured frame.
struct PassHistory<'a> {
    averages: &'a mut [RollingAverage],
    frames_without: &'a mut [usize; PASS_COUNT],
}

impl PassHistory<'_> {
    /// One measured frame: a sample per pass it stamped; a pass it skipped a full window running
    /// stops reporting, since an unwritten query resolves an old frame's stamps.
    fn record(&mut self, ticks: &[u64], written: u32, period_ns: f32) {
        for pass in TimedPass::ALL {
            let i = pass.index();
            let (begin, end) = (ticks[i * 2], ticks[i * 2 + 1]);
            let stamped = (written >> (i * 2)) & 0b11 == 0b11;
            if stamped && begin != 0 && end >= begin {
                self.averages[i].push(ticks_to_ms(end - begin, period_ns));
                self.frames_without[i] = 0;
                continue;
            }
            self.frames_without[i] += 1;
            if self.frames_without[i] >= AVERAGE_WINDOW {
                self.averages[i].clear();
            }
        }
    }
}

/// Reads `QUERIES_PER_SLOT` u64 ticks out of a mapped readback buffer.
fn read_ticks(buffer: &wgpu::Buffer) -> Vec<u64> {
    let view = buffer
        .slice(..)
        .get_mapped_range()
        .expect("caller maps the buffer before calling this");
    let ticks = view
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect();
    drop(view);
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_reports_nothing_and_hands_out_inert_scopes() {
        let mut timing = GpuTiming::unavailable();
        assert!(!timing.is_available());
        assert_eq!(timing.timestamp_period_ns(), None);
        timing.begin_frame();
        for pass in TimedPass::ALL {
            let scope = timing.scope(pass);
            assert!(!scope.is_active());
            assert!(scope.render_writes(0, 1).is_none());
            assert!(scope.compute_writes(0, 1).is_none());
        }
        assert!(timing.timings_ms().is_empty());
    }

    #[test]
    fn ticks_convert_with_the_reported_period() {
        // 1 000 000 ticks at 1 ns each is exactly one millisecond.
        assert_eq!(ticks_to_ms(1_000_000, 1.0), 1.0);
        // A coarse timer (some AMD parts report ~40 ns) scales accordingly.
        assert_eq!(ticks_to_ms(25_000, 40.0), 1.0);
        assert_eq!(ticks_to_ms(0, 1.0), 0.0);
    }

    #[test]
    fn tick_conversion_keeps_precision_on_a_large_timestamp_delta() {
        // Past f32's 24-bit mantissa: f32 math would round this to an even tick count.
        let ticks = 16_777_217u64;
        assert_eq!(ticks_to_ms(ticks, 1.0), 16.777217);
    }

    #[test]
    fn rolling_average_starts_empty_then_means_its_samples() {
        let mut average = RollingAverage::default();
        assert_eq!(average.average(), None);
        assert!(average.is_empty());
        average.push(2.0);
        average.push(4.0);
        assert_eq!(average.len(), 2);
        assert_eq!(average.average(), Some(3.0));
    }

    #[test]
    fn rolling_average_evicts_beyond_its_window() {
        let mut average = RollingAverage::default();
        for _ in 0..AVERAGE_WINDOW {
            average.push(10.0);
        }
        assert_eq!(average.average(), Some(10.0));
        // One full window of a new value must displace the old one entirely.
        for _ in 0..AVERAGE_WINDOW {
            average.push(20.0);
        }
        assert_eq!(average.len(), AVERAGE_WINDOW);
        assert_eq!(average.average(), Some(20.0));
    }

    #[test]
    fn pass_names_are_unique_and_indices_are_dense() {
        let mut indices: Vec<usize> = TimedPass::ALL.iter().map(|p| p.index()).collect();
        indices.sort_unstable();
        assert_eq!(indices, (0..PASS_COUNT).collect::<Vec<_>>());
        let mut names: Vec<&str> = TimedPass::ALL.iter().map(|p| p.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PASS_COUNT);
        let mut labels: Vec<&str> = TimedPass::ALL.iter().map(|p| p.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(
            labels.len(),
            PASS_COUNT,
            "two passes share a profiler label"
        );
    }

    #[test]
    fn a_single_sub_pass_scope_stamps_both_ends() {
        assert_eq!(sub_pass_write_indices(0, 1, 4, 5), Some((Some(4), Some(5))));
    }

    #[test]
    fn a_multi_sub_pass_scope_stamps_only_its_outer_passes() {
        // Bloom's middle blur pass gets no descriptor, not one with both indices unset.
        assert_eq!(sub_pass_write_indices(0, 3, 4, 5), Some((Some(4), None)));
        assert_eq!(sub_pass_write_indices(1, 3, 4, 5), None);
        assert_eq!(sub_pass_write_indices(2, 3, 4, 5), Some((None, Some(5))));
    }
}
