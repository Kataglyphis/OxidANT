//! F5: every pass rebuilt from new WGSL inside one validation scope; a refused reload keeps the old pipelines.

use std::sync::atomic::{AtomicU64, Ordering};

use super::cloud_pass::CloudPipelines;
use super::deferred::DeferredResources;
use super::main_pass::{self, ForwardPipelineSet};
use super::{shadow, sky, ForwardRenderer};
use crate::context::GpuContext;
use crate::render::bloom::BloomPipelines;
use crate::render::fxaa::FxaaPass;
use crate::render::histogram::HistogramPipelines;
use crate::render::ssao::SsaoPipelines;
use crate::render::tonemap::{TonemapPass, TonemapShaders};
use crate::render::tracer::modes::{TracedPasses, TracedSources};

/// The WGSL a shader reload rebuilds every pass of the frame from.
#[derive(Clone, Copy, Debug)]
pub struct ShaderSources<'a> {
    pub forward: &'a str,
    pub sky: &'a str,
    pub cloud_march: &'a str,
    pub cloud_noise: &'a str,
    /// Both traced modes' kernels.
    pub traced: &'a str,
    pub trace_resolve: &'a str,
    /// The tonemap with the cloud composite.
    pub tonemap: &'a str,
    pub fxaa: &'a str,
    pub bloom: &'a str,
    pub ssao: &'a str,
    pub histogram: &'a str,
    pub depth_resolve: &'a str,
}

impl ShaderSources<'static> {
    /// The files under `src/shaders/`, in field order, as a hot reload watches them.
    pub const FILES: [&'static str; 12] = [
        "forward.wgsl",
        "sky.wgsl",
        "cloud_march.wgsl",
        "cloud_noise.wgsl",
        "traced.wgsl",
        "trace_resolve.wgsl",
        "tonemap.wgsl",
        "fxaa.wgsl",
        "bloom.wgsl",
        "ssao.wgsl",
        "histogram.wgsl",
        "depth_resolve.wgsl",
    ];

    /// The WGSL this crate was built with.
    pub fn shipped() -> Self {
        let traced = TracedSources::shipped();
        Self {
            forward: include_str!("../../shaders/forward.wgsl"),
            sky: include_str!("../../shaders/sky.wgsl"),
            cloud_march: include_str!("../../shaders/cloud_march.wgsl"),
            cloud_noise: include_str!("../../shaders/cloud_noise.wgsl"),
            traced: traced.traced,
            trace_resolve: traced.resolve,
            tonemap: include_str!("../../shaders/tonemap.wgsl"),
            fxaa: include_str!("../../shaders/fxaa.wgsl"),
            bloom: include_str!("../../shaders/bloom.wgsl"),
            ssao: include_str!("../../shaders/ssao.wgsl"),
            histogram: include_str!("../../shaders/histogram.wgsl"),
            depth_resolve: include_str!("../../shaders/depth_resolve.wgsl"),
        }
    }
}

impl<'a> ShaderSources<'a> {
    /// The sources of [`ShaderSources::FILES`], read in that order.
    pub fn from_files(files: &'a [String; 12]) -> Self {
        let [forward, sky, cloud_march, cloud_noise, traced, trace_resolve, tonemap, fxaa, bloom, ssao, histogram, depth_resolve] =
            files.each_ref().map(String::as_str);
        Self {
            forward,
            sky,
            cloud_march,
            cloud_noise,
            traced,
            trace_resolve,
            tonemap,
            fxaa,
            bloom,
            ssao,
            histogram,
            depth_resolve,
        }
    }
}

/// A fresh id per accepted tonemap reload; 0 is the shipped pair.
fn next_tonemap_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The scene, shadow, sky, deferred, cloud and traced pipelines of a reload, compiled but not yet in use.
struct SceneReload {
    shader: wgpu::ShaderModule,
    sky_shader: wgpu::ShaderModule,
    upright: ForwardPipelineSet,
    shadow: shadow::ShadowPipelines,
    sky: wgpu::RenderPipeline,
    deferred: DeferredResources,
    clouds: CloudPipelines,
    traced: TracedPasses,
    traced_sky: wgpu::RenderPipeline,
}

/// The post chain's pipelines of a reload: the tonemap pair as WGSL every tonemap pass rebuilds from.
struct PostReload {
    tonemap: TonemapShaders,
    bloom: BloomPipelines,
    ssao: SsaoPipelines,
    histogram: HistogramPipelines,
    depth_resolve: wgpu::RenderPipeline,
}

impl ForwardRenderer {
    /// Rebuilds every pass from new WGSL; invalid shaders keep the old pipelines, the tonemap's included.
    pub fn reload_shaders(
        &mut self,
        gpu: &GpuContext,
        sources: &ShaderSources<'_>,
    ) -> anyhow::Result<()> {
        let error_scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let scene = self.compile_scene(&gpu.device, sources);
        let post = self.compile_post(gpu, sources);
        if let Some(err) = pollster::block_on(error_scope.pop()) {
            anyhow::bail!("shader reload rejected: {err}");
        }
        self.install_scene(&gpu.device, scene);
        self.install_post(post);
        Ok(())
    }

    fn compile_scene(&self, device: &wgpu::Device, sources: &ShaderSources<'_>) -> SceneReload {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("forward_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sources.forward.into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader_reloaded"),
            source: wgpu::ShaderSource::Wgsl(sources.sky.into()),
        });
        let upright = main_pass::create_forward_pipeline_set(
            device,
            &shader,
            &self.main.layout,
            wgpu::FrontFace::Ccw,
        );
        let shadow = shadow::ShadowPipelines::new(
            device,
            &shader,
            &self.shadow.pipeline_layout,
            &self.shadow.masked_pipeline_layout,
        );
        let sky = sky::create_sky_pipeline(device, &sky_shader, &self.sky.pipeline_layout);
        // Even before deferred first runs: its pipelines are built lazily, outside any error scope.
        let deferred = self.build_deferred(device, &shader, &sky_shader);
        // Even with clouds off: a cloud shader that cannot build is refused now, not on the first cloud frame.
        let clouds = CloudPipelines::new(device, sources.cloud_march, sources.cloud_noise);
        // Both kernels now, as for deferred: each otherwise compiles on its mode's first frame, outside any scope.
        let traced = TracedPasses::compiled(
            device,
            TracedSources {
                traced: sources.traced,
                resolve: sources.trace_resolve,
            },
        );
        let traced_sky = sky::sky_pipeline(device, &sky_shader, &self.sky.pipeline_layout, 1);
        SceneReload {
            shader,
            sky_shader,
            upright,
            shadow,
            sky,
            deferred,
            clouds,
            traced,
            traced_sky,
        }
    }

    fn compile_post(&self, gpu: &GpuContext, sources: &ShaderSources<'_>) -> PostReload {
        let device = &gpu.device;
        let tonemap = TonemapShaders {
            tonemap: sources.tonemap.into(),
            fxaa: sources.fxaa.into(),
            id: next_tonemap_id(),
        };
        // Built and dropped here, inside the scope: the host's passes rebuild from these sources outside any.
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        drop(TonemapPass::with_shaders(gpu, format, tonemap.clone()));
        drop(FxaaPass::new(gpu, format, sources.fxaa));
        PostReload {
            tonemap,
            bloom: self.post.bloom.compile(device, sources.bloom),
            ssao: self.post.ssao.compile(device, sources.ssao),
            histogram: self.post.histogram.compile(device, sources.histogram),
            depth_resolve: self.depth_resolve.compile(device, sources.depth_resolve),
        }
    }

    fn install_scene(&mut self, device: &wgpu::Device, scene: SceneReload) {
        self.main.upright = scene.upright;
        // The mirrored sets are rebuilt from the new module the next frame one is drawn.
        self.main.mirrored = None;
        self.main.shader = scene.shader;
        self.shadow.pipelines = scene.shadow;
        // The cached bundles still hold the old shadow pipelines.
        self.shadow.caster_bundles = None;
        self.sky.pipeline = scene.sky;
        self.sky.shader = scene.sky_shader;
        if self.deferred.is_some() {
            self.deferred = Some(scene.deferred);
        }
        self.cloud_pass.replace_pipelines(device, scene.clouds);
        self.replace_traced_pipelines(scene.traced, scene.traced_sky);
    }

    fn install_post(&mut self, post: PostReload) {
        // Every tonemap pass, the window's and a screenshot's, rebuilds from these before its next frame.
        self.tonemap_shaders = post.tonemap;
        self.post.bloom.replace_pipelines(post.bloom);
        self.post.ssao.replace_pipelines(post.ssao);
        self.post.histogram.replace_pipelines(post.histogram);
        self.depth_resolve.replace_pipeline(post.depth_resolve);
    }
}
