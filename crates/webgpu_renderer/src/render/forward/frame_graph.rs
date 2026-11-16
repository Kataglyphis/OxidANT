//! The renderer's frame as a render graph: the passes this frame's settings record, what each reads and writes,
//! and the size-dependent targets the transient pool backs. Rows are added in a valid record order, which the
//! graph's tie-break keeps, so a frame records its passes as before the graph ordered them.

#[cfg(test)]
mod tests;

use super::cloud_pass::CLOUD_FORMAT;
use super::deferred::{RasterPath, GBUFFER_FORMATS};
use super::targets::DEPTH_SPREAD_FORMAT;
use super::{DEPTH_FORMAT, HDR_FORMAT, MSAA_SAMPLE_COUNT};
#[cfg(test)]
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::{
    CompiledGraph, FrameGraph, GraphError, PassDesc, Resource, TextureDesc,
};
use crate::render::ssao::AO_FORMAT;
use crate::render::tracer::RenderMode;
use Resource::*;

/// One row of the frame graph, each recorded by its own `ForwardRenderer` method.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum FramePass {
    DrawCull,
    ShadowCascades,
    Forward,
    DepthResolve,
    GBuffer,
    DeferredLighting,
    OcclusionCull,
    RayTrace,
    PathTrace,
    TraceResolve,
    Histogram,
    ExposureReduce,
    CloudNoise,
    Clouds,
    Bloom,
    Ssao,
    Tonemap,
    Fxaa,
}

impl FramePass {
    /// The row's name in the graph and its errors.
    pub(super) fn name(self) -> &'static str {
        match self {
            FramePass::DrawCull => "draw_cull",
            FramePass::ShadowCascades => "shadow",
            FramePass::Forward => "forward+sky",
            FramePass::DepthResolve => "depth_resolve",
            FramePass::GBuffer => "gbuffer",
            FramePass::DeferredLighting => "deferred_lighting+sky",
            FramePass::OcclusionCull => "occlusion_cull",
            FramePass::RayTrace => "ray_trace",
            FramePass::PathTrace => "path_trace",
            FramePass::TraceResolve => "trace_resolve",
            FramePass::Histogram => "histogram",
            FramePass::ExposureReduce => "exposure_reduce",
            FramePass::CloudNoise => "cloud_noise",
            FramePass::Clouds => "clouds",
            FramePass::Bloom => "bloom",
            FramePass::Ssao => "ssao",
            FramePass::Tonemap => "tonemap",
            FramePass::Fxaa => "fxaa",
        }
    }

    /// The profiler's pass this row's timestamps feed; the resolve and the one-off noise are untimed.
    #[cfg(test)]
    fn timed(self) -> Option<TimedPass> {
        Some(match self {
            FramePass::DrawCull => TimedPass::DrawCull,
            FramePass::ShadowCascades => TimedPass::ShadowCascades,
            FramePass::Forward => TimedPass::Forward,
            FramePass::GBuffer => TimedPass::GBuffer,
            FramePass::DeferredLighting => TimedPass::DeferredLighting,
            FramePass::OcclusionCull => TimedPass::OcclusionCull,
            FramePass::RayTrace => TimedPass::RayTrace,
            FramePass::PathTrace => TimedPass::PathTrace,
            FramePass::TraceResolve => TimedPass::TraceResolve,
            FramePass::Histogram => TimedPass::Histogram,
            FramePass::ExposureReduce => TimedPass::ExposureReduce,
            FramePass::Clouds => TimedPass::Clouds,
            FramePass::Bloom => TimedPass::Bloom,
            FramePass::Ssao => TimedPass::Ssao,
            FramePass::Tonemap => TimedPass::Tonemap,
            FramePass::Fxaa => TimedPass::Fxaa,
            FramePass::DepthResolve | FramePass::CloudNoise => return None,
        })
    }
}

/// How the clouds run this frame.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct CloudShape {
    /// The march writes a half-resolution target the upsample fills the full one from.
    pub(super) half: bool,
    /// The noise volume still has to be filled.
    pub(super) noise: bool,
}

/// Everything that decides a frame's passes and targets.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct FrameConfig {
    pub(super) size: (u32, u32),
    /// The tonemap's target format, which FXAA's input shares.
    pub(super) output_format: wgpu::TextureFormat,
    /// The drawn mode, which rasters while a traced pick's scene is not ready.
    pub(super) mode: RenderMode,
    /// Read only while the frame rasters.
    pub(super) path: RasterPath,
    pub(super) clouds: Option<CloudShape>,
    pub(super) bloom: bool,
    pub(super) ssao: bool,
    /// Auto exposure builds the histogram; manual exposure only reduces.
    pub(super) histogram: bool,
    pub(super) occlusion: bool,
    pub(super) fxaa: bool,
}

impl FrameConfig {
    /// The forward path's full-resolution march reads the MSAA depth spread; no other frame has one.
    fn spread(&self) -> bool {
        !self.mode.is_traced()
            && self.path == RasterPath::Forward
            && self.clouds.is_some_and(|clouds| !clouds.half)
    }
}

/// The frame's passes in record order and the compiled graph behind them.
pub(super) struct FramePlan {
    pub(super) passes: Vec<FramePass>,
    pub(super) graph: CompiledGraph,
}

impl FramePlan {
    pub(super) fn runs(&self, pass: FramePass) -> bool {
        self.passes.contains(&pass)
    }
}

type Row = (FramePass, Vec<Resource>, Vec<Resource>);

/// Compiles `config`'s frame: the rows it records, ordered, and the transient slots behind its targets.
pub(super) fn plan(config: &FrameConfig) -> Result<FramePlan, GraphError> {
    let rows = rows(config);
    let mut graph = FrameGraph::new();
    for (pass, reads, writes) in &rows {
        graph.add_pass(PassDesc {
            name: pass.name(),
            reads,
            writes,
        });
    }
    declare_resources(&mut graph, config.size, config.output_format);
    let compiled = graph.compile()?;
    Ok(FramePlan {
        passes: compiled.order.iter().map(|&row| rows[row].0).collect(),
        graph: compiled,
    })
}

/// Every row `config` records, in record order.
fn rows(config: &FrameConfig) -> Vec<Row> {
    let mut rows = if config.mode.is_traced() {
        traced_rows(config.mode)
    } else {
        raster_rows(config)
    };
    if config.histogram {
        rows.push((FramePass::Histogram, vec![HdrColor], vec![Histogram]));
    }
    let bins = if config.histogram {
        vec![Histogram]
    } else {
        vec![]
    };
    rows.push((FramePass::ExposureReduce, bins, vec![Exposure]));
    if let Some(clouds) = config.clouds {
        rows.extend(cloud_rows(clouds, config.spread()));
    }
    rows.extend(post_rows(config));
    rows
}

/// The draw cull, the shadow cascades, the path's scene passes, and the occlusion cull against their depth.
fn raster_rows(config: &FrameConfig) -> Vec<Row> {
    let mut rows = vec![
        (FramePass::DrawCull, vec![], vec![DrawArgs]),
        (FramePass::ShadowCascades, vec![DrawArgs], vec![ShadowMap]),
    ];
    if config.path == RasterPath::Deferred {
        let gbuffer = [GBufferNormal, GBufferAlbedo, GBufferMaterial];
        rows.push((
            FramePass::GBuffer,
            vec![DrawArgs],
            [&gbuffer[..], &[Depth]].concat(),
        ));
        let reads = [&gbuffer[..], &[Depth, ShadowMap, DrawArgs]].concat();
        rows.push((FramePass::DeferredLighting, reads, vec![HdrColor]));
    } else {
        let writes = vec![HdrMsaa, HdrColor, DepthMsaa];
        rows.push((FramePass::Forward, vec![ShadowMap, DrawArgs], writes));
        let writes = vec![Depth, DepthSpread];
        rows.push((FramePass::DepthResolve, vec![DepthMsaa], writes));
    }
    if config.occlusion {
        rows.push((FramePass::OcclusionCull, vec![Depth], vec![Visibility]));
    }
    rows
}

/// The kernel into the accumulation, and the resolve of it into the HDR target and the primary hit's depth.
fn traced_rows(mode: RenderMode) -> Vec<Row> {
    let kernel = if mode == RenderMode::PathTraced {
        FramePass::PathTrace
    } else {
        FramePass::RayTrace
    };
    vec![
        (kernel, vec![], vec![Accumulation]),
        (
            FramePass::TraceResolve,
            vec![Accumulation],
            vec![HdrColor, Depth],
        ),
    ]
}

/// The noise on the first cloud frame, then the march to the scene depth.
fn cloud_rows(clouds: CloudShape, spread: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    if clouds.noise {
        rows.push((FramePass::CloudNoise, vec![], vec![NoiseVolume]));
    }
    let mut reads = vec![Depth, NoiseVolume];
    if spread {
        reads.push(DepthSpread);
    }
    let mut writes = vec![CloudColor];
    if clouds.half {
        writes.push(CloudHalf);
    }
    rows.push((FramePass::Clouds, reads, writes));
    rows
}

/// Bloom, SSAO, and the tonemap of everything before them, through FXAA when the frame filters.
fn post_rows(config: &FrameConfig) -> Vec<Row> {
    let clouds: &[Resource] = if config.clouds.is_some() {
        &[CloudColor]
    } else {
        &[]
    };
    let mut rows = Vec::new();
    let mut tonemap_reads = [&[HdrColor, Exposure][..], clouds].concat();
    if config.bloom {
        let reads = [&[HdrColor, Exposure][..], clouds].concat();
        rows.push((FramePass::Bloom, reads, vec![Bloom, BloomScratch]));
        tonemap_reads.push(Bloom);
    }
    // A traced frame never ran SSAO, so its tonemap reads no ambient term.
    if config.ssao && !config.mode.is_traced() {
        rows.push((FramePass::Ssao, vec![Depth], vec![AmbientRaw, Ambient]));
        tonemap_reads.push(Ambient);
    }
    let tonemapped = if config.fxaa { Tonemapped } else { Output };
    rows.push((FramePass::Tonemap, tonemap_reads, vec![tonemapped]));
    if config.fxaa {
        rows.push((FramePass::Fxaa, vec![Tonemapped], vec![Output]));
    }
    rows
}

/// What the frame keeps or reads back afterwards, and the transient targets at the output `size`.
fn declare_resources(graph: &mut FrameGraph<'_>, size: (u32, u32), output: wgpu::TextureFormat) {
    graph.import(NoiseVolume);
    // The test and diagnostic readbacks read HDR, depth and clouds after the frame.
    for kept in [
        Output,
        Visibility,
        Accumulation,
        NoiseVolume,
        HdrColor,
        Depth,
        CloudColor,
    ] {
        graph.export(kept);
    }
    for (resource, desc) in transient_targets(size, output) {
        graph.transient(resource, desc);
    }
}

/// Every target the pool may back, by resource; a frame allocates only those its running passes use.
pub(super) fn transient_targets(
    size: (u32, u32),
    output: wgpu::TextureFormat,
) -> [(Resource, TextureDesc); 15] {
    use wgpu::TextureUsages as U;
    let sampled = U::RENDER_ATTACHMENT | U::TEXTURE_BINDING;
    // One usage for every single-sample colour target, so a G-buffer target and the clouds can share a texture.
    let read_back = sampled | U::COPY_SRC;
    let target = |format, sample_count, usage| TextureDesc {
        size,
        format,
        sample_count,
        usage,
    };
    let half = TextureDesc {
        size: (size.0.div_ceil(2), size.1.div_ceil(2)),
        ..target(CLOUD_FORMAT, 1, read_back)
    };
    // Bloom and SSAO round down, so an even output's bloom chain can take the cloud march's half target.
    let post_half = |format, usage| TextureDesc {
        size: ((size.0 / 2).max(1), (size.1 / 2).max(1)),
        ..target(format, 1, usage)
    };
    [
        (HdrMsaa, target(HDR_FORMAT, MSAA_SAMPLE_COUNT, sampled)),
        (DepthMsaa, target(DEPTH_FORMAT, MSAA_SAMPLE_COUNT, sampled)),
        (HdrColor, target(HDR_FORMAT, 1, read_back)),
        (Depth, target(DEPTH_FORMAT, 1, read_back)),
        (DepthSpread, target(DEPTH_SPREAD_FORMAT, 1, sampled)),
        (GBufferNormal, target(GBUFFER_FORMATS[0], 1, read_back)),
        (GBufferAlbedo, target(GBUFFER_FORMATS[1], 1, read_back)),
        (GBufferMaterial, target(GBUFFER_FORMATS[2], 1, read_back)),
        (CloudColor, target(CLOUD_FORMAT, 1, read_back)),
        (CloudHalf, half),
        (Bloom, post_half(HDR_FORMAT, read_back)),
        (BloomScratch, post_half(HDR_FORMAT, read_back)),
        (AmbientRaw, post_half(AO_FORMAT, sampled)),
        (Ambient, post_half(AO_FORMAT, sampled)),
        (Tonemapped, target(output, 1, sampled)),
    ]
}
