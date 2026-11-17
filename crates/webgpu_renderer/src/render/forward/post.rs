//! The post chain after the forward pass: bloom, SSAO, histogram exposure and the tonemap.

use std::sync::atomic::{AtomicU64, Ordering};

use super::cloud_pass::placeholder;
use super::frame::FrameView;
use super::{ForwardRenderer, RasterPath, HDR_FORMAT};
use crate::context::GpuContext;
use crate::render::bloom::{BloomInputs, BloomPass, BloomTargets};
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::Resource;
use crate::render::histogram::{ExposureSettings, HistogramPass};
use crate::render::ssao::{SsaoPass, SsaoTargets, AO_FORMAT};
use crate::render::tonemap::TonemapPass;

pub(super) struct PostChain {
    pub(super) bloom: BloomPass,
    pub(super) ssao: SsaoPass,
    pub(super) histogram: HistogramPass,
    /// What the tonemap samples for bloom on a frame without it, whose bloom strength is zero.
    no_bloom: wgpu::TextureView,
    /// What the tonemap samples for AO on a frame without SSAO, whose AO strength is zero.
    no_ambient: wgpu::TextureView,
    /// Bloom and SSAO must rebind to the current targets before the next frame.
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
    /// The histogram binds the HDR target the first frame's graph allocates.
    pub(super) fn new(gpu: &GpuContext) -> Self {
        Self {
            bloom: BloomPass::new(gpu),
            ssao: SsaoPass::new(gpu),
            histogram: HistogramPass::new(gpu),
            no_bloom: placeholder(&gpu.device, "bloom_placeholder", HDR_FORMAT),
            no_ambient: placeholder(&gpu.device, "ssao_placeholder", AO_FORMAT),
            rebuild_needed: true,
            inputs_id: 0,
        }
    }
}

impl ForwardRenderer {
    /// Rebinds bloom and SSAO to this frame's targets after the pool moved any of them, or lets them go without their rows.
    pub(super) fn rebuild_post_inputs(&mut self, gpu: &GpuContext) {
        if !self.post.rebuild_needed {
            return;
        }
        let (post, targets) = (&mut self.post, &self.targets);
        let inputs = BloomInputs {
            hdr: targets.view(Resource::HdrColor),
            exposure: post.histogram.exposure_buffer(),
            cloud: self.cloud_pass.composite(targets.get(Resource::CloudColor)),
        };
        let bloom = targets
            .get(Resource::Bloom)
            .zip(targets.get(Resource::BloomScratch))
            .map(|(output, scratch)| BloomTargets { output, scratch });
        post.bloom.bind(&gpu.device, inputs, bloom);
        let ssao = targets
            .get(Resource::AmbientRaw)
            .zip(targets.get(Resource::Ambient))
            .map(|(raw, blurred)| SsaoTargets {
                depth: targets.view(Resource::Depth),
                raw,
                blurred,
            });
        post.ssao.bind(&gpu.device, ssao);
        post.inputs_id = next_inputs_id();
        post.rebuild_needed = false;
    }

    /// Binds `tonemap` to the current HDR, bloom and AO outputs unless it samples them already, and to the reloaded WGSL.
    /// Per pass, not per rebuild: every pass a host renders with must follow a rebuild.
    pub(super) fn bind_tonemap_inputs(&self, gpu: &GpuContext, tonemap: &mut TonemapPass) {
        tonemap.use_shaders(gpu, &self.tonemap_shaders);
        let post = &self.post;
        if tonemap.bound_inputs == Some(post.inputs_id) {
            return;
        }
        tonemap.set_input(
            gpu,
            self.targets.view(Resource::HdrColor),
            self.targets.get(Resource::Bloom).unwrap_or(&post.no_bloom),
            self.targets
                .get(Resource::Ambient)
                .unwrap_or(&post.no_ambient),
            post.histogram.exposure_buffer(),
            self.cloud_view(),
        );
        tonemap.bound_inputs = Some(post.inputs_id);
    }

    /// This frame's bloom and SSAO mix, and the exposure mode the reduce pass applies.
    pub(super) fn write_post_settings(&self, gpu: &GpuContext, tonemap: &TonemapPass) {
        // A traced frame has no depth for SSAO, so it must not darken by a raster frame's occlusion.
        let ssao = if self.drawn_mode().is_traced() {
            0.0
        } else {
            self.ssao_strength
        };
        tonemap.set_params(&gpu.queue, self.bloom_strength, ssao, self.clouds.enabled);
        self.post.bloom.set_clouds(&gpu.queue, self.clouds.enabled);
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

    /// Drops the adapted exposure back to `exposure_ev`; `upload_scene` does it on every scene change.
    pub fn reset_exposure(&self, gpu: &GpuContext) {
        self.post
            .histogram
            .reset_exposure(&gpu.queue, self.exposure_ev);
    }

    /// The exposure state the tonemap reads, as (adapted EV, target EV); stalls, so tests only.
    #[doc(hidden)]
    pub fn read_exposure(&self, gpu: &GpuContext) -> (f32, f32) {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("exposure_readback_encoder"),
            });
        self.post.histogram.encode_exposure_readback(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        self.post.histogram.read_back_exposure(gpu)
    }

    /// The frame graph adds the row only above zero strength: at zero the tonemap provably ignores the output.
    pub(super) fn encode_bloom(&self, encoder: &mut wgpu::CommandEncoder) {
        self.post
            .bloom
            .encode(encoder, self.gpu_timing.scope(TimedPass::Bloom));
    }

    /// The frame graph adds the row only above zero strength on a raster frame, the only one with SSAO's depth.
    pub(super) fn encode_ssao(&self, encoder: &mut wgpu::CommandEncoder) {
        self.post
            .ssao
            .encode(encoder, self.gpu_timing.scope(TimedPass::Ssao));
    }

    /// The full-resolution histogram build, which only auto exposure reads.
    pub(super) fn encode_histogram(&self, encoder: &mut wgpu::CommandEncoder, view: &FrameView) {
        self.post.histogram.encode(
            encoder,
            view.width,
            view.height,
            self.gpu_timing.scope(TimedPass::Histogram),
        );
    }

    /// Every frame: manual mode copies its EV into the one buffer the tonemap reads.
    pub(super) fn encode_exposure_reduce(&self, encoder: &mut wgpu::CommandEncoder) {
        self.post
            .histogram
            .encode_reduce(encoder, self.gpu_timing.scope(TimedPass::ExposureReduce));
    }

    /// ACES over HDR, bloom and AO into `output_view`, or into FXAA's input when the frame's graph backs one.
    pub(super) fn encode_tonemap(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        tonemap: &TonemapPass,
        output_view: &wgpu::TextureView,
    ) {
        let target = self
            .targets
            .get(Resource::Tonemapped)
            .unwrap_or(output_view);
        tonemap.render(encoder, target, self.gpu_timing.scope(TimedPass::Tonemap));
    }

    /// Binds FXAA's input to the pool's intermediate on a frame that filters, and lets it go on one that does not.
    pub(super) fn prepare_fxaa(&self, gpu: &GpuContext, tonemap: &mut TonemapPass) {
        match self.targets.get(Resource::Tonemapped) {
            Some(input) => {
                let (width, height) = self.targets.output_size;
                tonemap.prepare_fxaa(gpu, input, width, height);
            }
            None => tonemap.release_fxaa(),
        }
    }

    /// FXAA over the tonemapped frame into `output_view`.
    pub(super) fn encode_fxaa(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        tonemap: &TonemapPass,
        output_view: &wgpu::TextureView,
    ) {
        tonemap.render_fxaa(encoder, output_view, self.gpu_timing.scope(TimedPass::Fxaa));
    }

    /// Whether the frame [`Self::drawn_mode`] draws runs FXAA.
    pub fn fxaa_runs(&self) -> bool {
        self.fxaa_filters(self.drawn_mode().is_traced())
    }

    /// Whether this frame runs FXAA; `traced` when it was traced rather than rastered.
    pub(super) fn fxaa_filters(&self, traced: bool) -> bool {
        self.fxaa
            .filters(!traced && self.raster_path == RasterPath::Deferred)
    }
}
