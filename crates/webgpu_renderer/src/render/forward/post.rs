//! The post chain after the forward pass: bloom, SSAO, histogram exposure and the tonemap.

use std::sync::atomic::{AtomicU64, Ordering};

use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::bloom::BloomPass;
use crate::render::gpu_timing::TimedPass;
use crate::render::histogram::{ExposureSettings, HistogramPass};
use crate::render::ssao::SsaoPass;
use crate::render::tonemap::TonemapPass;

pub(super) struct PostChain {
    pub(super) bloom: BloomPass,
    pub(super) ssao: SsaoPass,
    pub(super) histogram: HistogramPass,
    /// Bloom and SSAO must rebuild on the current HDR and depth targets before the next frame.
    pub(super) rebuild_needed: bool,
    /// Names the current HDR, bloom, AO and exposure set; unique across renderers.
    inputs_id: u64,
}

/// A fresh id per rebuilt input set, so no tonemap mistakes another renderer's set for its own.
fn next_inputs_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl PostChain {
    pub(super) fn new(gpu: &GpuContext, hdr_view: &wgpu::TextureView) -> Self {
        let mut histogram = HistogramPass::new(gpu);
        histogram.set_input(gpu, hdr_view);
        Self {
            bloom: BloomPass::new(gpu),
            ssao: SsaoPass::new(gpu),
            histogram,
            rebuild_needed: true,
            inputs_id: 0,
        }
    }
}

impl ForwardRenderer {
    /// Rebuilds bloom and SSAO on the current targets after those were recreated.
    pub(super) fn rebuild_post_inputs(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if !self.post.rebuild_needed {
            return;
        }
        let post = &mut self.post;
        post.bloom.rebuild(
            gpu,
            width,
            height,
            &self.targets.hdr_view,
            post.histogram.exposure_buffer(),
        );
        post.ssao.rebuild(gpu, width, height, &self.targets.depth);
        post.inputs_id = next_inputs_id();
        post.rebuild_needed = false;
    }

    /// Binds `tonemap` to the current HDR, bloom and AO outputs unless it samples them already.
    /// Per pass, not per rebuild: every pass a host renders with must follow a rebuild.
    pub(super) fn bind_tonemap_inputs(&self, gpu: &GpuContext, tonemap: &mut TonemapPass) {
        let post = &self.post;
        if tonemap.bound_inputs == Some(post.inputs_id) {
            return;
        }
        tonemap.set_input(
            gpu,
            &self.targets.hdr_view,
            post.bloom
                .output()
                .expect("bloom output exists after rebuild"),
            post.ssao
                .output()
                .expect("ssao output exists after rebuild"),
            post.histogram.exposure_buffer(),
        );
        tonemap.bound_inputs = Some(post.inputs_id);
    }

    /// This frame's bloom and SSAO mix, and the exposure mode the reduce pass applies.
    pub(super) fn write_post_settings(&self, gpu: &GpuContext, tonemap: &TonemapPass) {
        tonemap.set_params(&gpu.queue, self.bloom_strength, self.ssao_strength);
        self.post.histogram.set_exposure_settings(
            &gpu.queue,
            ExposureSettings {
                delta_time_seconds: self.frame_delta_seconds.max(0.0),
                speed: self.auto_exposure_speed,
                auto_enabled: self.auto_exposure,
                manual_ev: self.exposure_ev,
            },
        );
    }

    /// At zero strength the tonemap provably ignores the output, so the pass is skipped.
    pub(super) fn encode_bloom(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.bloom_strength > 0.0 {
            self.post
                .bloom
                .encode(encoder, self.gpu_timing.scope(TimedPass::Bloom));
        }
    }

    /// At zero strength the tonemap provably ignores the output, so the pass is skipped.
    pub(super) fn encode_ssao(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.ssao_strength > 0.0 {
            self.post
                .ssao
                .encode(encoder, self.gpu_timing.scope(TimedPass::Ssao));
        }
    }

    /// The histogram build in auto mode, then the reduce into the exposure the tonemap reads.
    pub(super) fn encode_exposure(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
    ) {
        // Manual exposure never reads the bins, so the full-res histogram build is auto-mode only.
        if self.auto_exposure {
            self.post.histogram.encode(
                encoder,
                width,
                height,
                self.gpu_timing.scope(TimedPass::Histogram),
            );
        }
        // Always reduce: manual mode copies its EV into the one buffer the tonemap reads.
        self.post
            .histogram
            .encode_reduce(encoder, self.gpu_timing.scope(TimedPass::ExposureReduce));
    }

    /// ACES over HDR, bloom and AO into `output_view`.
    pub(super) fn encode_tonemap(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        tonemap: &TonemapPass,
        output_view: &wgpu::TextureView,
    ) {
        tonemap.render(
            encoder,
            output_view,
            self.gpu_timing.scope(TimedPass::Tonemap),
        );
    }
}
