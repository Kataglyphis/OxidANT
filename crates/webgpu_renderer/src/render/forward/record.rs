//! Records one row of the frame graph: every row is one `ForwardRenderer` method, called in the graph's order.

use super::frame::FrameView;
use super::frame_graph::FramePass;
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::tonemap::TonemapPass;

/// What every row of one frame records with.
pub(super) struct FrameRecord<'f> {
    pub(super) gpu: &'f GpuContext,
    pub(super) encoder: &'f mut wgpu::CommandEncoder,
    pub(super) tonemap: &'f mut TonemapPass,
    pub(super) output: &'f wgpu::TextureView,
    pub(super) view: &'f FrameView,
}

impl ForwardRenderer {
    pub(super) fn record_pass(&mut self, pass: FramePass, frame: &mut FrameRecord<'_>) {
        let (gpu, view) = (frame.gpu, frame.view);
        let encoder = &mut *frame.encoder;
        match pass {
            FramePass::ShadowCascades => self.encode_shadow_cascades(gpu, encoder),
            FramePass::Forward => self.encode_forward_pass(encoder, view),
            FramePass::DepthResolve => self.encode_depth_resolve(encoder),
            FramePass::GBuffer => self.encode_gbuffer(encoder, view),
            FramePass::DeferredLighting => self.encode_lighting(encoder, view),
            FramePass::OcclusionCull => self.encode_occlusion(gpu, encoder, view),
            FramePass::RayTrace | FramePass::PathTrace => self.encode_traced(gpu, encoder, view),
            FramePass::TraceResolve => self.encode_trace_resolve(gpu, encoder),
            FramePass::Histogram => self.encode_histogram(encoder, view),
            FramePass::ExposureReduce => self.encode_exposure_reduce(encoder),
            FramePass::CloudNoise => self.encode_cloud_noise(encoder),
            FramePass::Clouds => self.encode_clouds(gpu, encoder, view),
            FramePass::Bloom => self.encode_bloom(encoder),
            FramePass::Ssao => self.encode_ssao(encoder),
            FramePass::Tonemap => self.encode_tonemap(encoder, frame.tonemap, frame.output),
            FramePass::Fxaa => self.encode_fxaa(encoder, frame.tonemap, frame.output),
        }
    }
}
