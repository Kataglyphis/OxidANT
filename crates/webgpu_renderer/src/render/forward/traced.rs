//! The renderer's traced scene and modes: the CPU scene it builds from, the build a traced mode starts, and a traced frame.

use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::frame::FrameView;
use super::{sky, ForwardRenderer, RasterPath};
use crate::context::GpuContext;
use crate::render::gpu_timing::TimedPass;
use crate::render::tracer::modes::{
    accumulation_bytes, accumulation_limit, plan, Accumulation, AccumulationKey, AfterResolve,
    Kernel, TraceDispatch, TraceFrameUniforms, TraceLights, TracedFrame, TracedPasses,
};
use crate::render::tracer::{RenderMode, TraceSettings, TraceStatus, Tracer};
use crate::scene::CpuScene;

/// The traced modes' GPU half, built on first use, and the path tracer's running mean.
#[derive(Default)]
pub(super) struct TracedState {
    pub(super) passes: Option<TracedPasses>,
    /// The sky at one sample, which the ray-traced mode draws where its primary rays missed.
    pub(super) sky: Option<wgpu::RenderPipeline>,
    accumulation: Accumulation,
    /// The tracer generation and animated transforms its instances were last moved to.
    synced: (u64, Vec<Mat4>),
    /// The output an accumulation was refused for, so the refusal is logged once.
    refused: Option<(u32, u32)>,
    /// The largest accumulation the device binds, in bytes; an output past it rasters.
    accumulation_limit: u64,
    /// The kind of frame the timings last measured: the drawn mode, the raster path while it rasters, and FXAA.
    timed_as: Option<(RenderMode, RasterPath, bool)>,
}

impl TracedState {
    pub(super) fn new(limits: &wgpu::Limits) -> Self {
        Self {
            accumulation_limit: accumulation_limit(limits),
            ..Self::default()
        }
    }
}

/// Whether a frame of `mode` over `path` records `pass` (with its optional passes on); `fxaa` when it filters.
fn records(pass: TimedPass, mode: RenderMode, path: RasterPath, fxaa: bool) -> bool {
    match (mode, pass) {
        (_, TimedPass::Fxaa) => fxaa,
        (RenderMode::Raster, TimedPass::Forward) => path == RasterPath::Forward,
        (RenderMode::Raster, TimedPass::GBuffer | TimedPass::DeferredLighting) => {
            path == RasterPath::Deferred
        }
        (RenderMode::Raster, pass) => !pass.is_traced(),
        (_, TimedPass::RayTrace) => mode == RenderMode::RayTraced,
        (_, TimedPass::PathTrace) => mode == RenderMode::PathTraced,
        (_, pass) => matches!(
            pass,
            TimedPass::TraceResolve
                | TimedPass::Histogram
                | TimedPass::ExposureReduce
                | TimedPass::Bloom
                | TimedPass::Clouds
                | TimedPass::Tonemap
        ),
    }
}

impl ForwardRenderer {
    /// The scene the traced modes build from, the one [`Self::upload_scene`] took; kept until the next upload.
    pub fn set_trace_source(&mut self, scene: Arc<CpuScene>) {
        self.tracer.set_source(scene);
    }

    pub fn trace_status(&self) -> TraceStatus {
        self.tracer.status()
    }

    pub fn tracer(&self) -> &Tracer {
        &self.tracer
    }

    pub fn tracer_mut(&mut self) -> &mut Tracer {
        &mut self.tracer
    }

    /// What a frame draws: the picked traced mode once its scene is on the GPU and its accumulation fits, raster otherwise.
    pub fn drawn_mode(&self) -> RenderMode {
        if self.tracer.gpu_scene().is_some() && self.traced_fallback().is_none() {
            self.render_mode
        } else {
            RenderMode::Raster
        }
    }

    /// Why a traced pick whose scene is ready rasters anyway: this output's accumulation does not fit the device.
    pub fn traced_fallback(&self) -> Option<String> {
        if !self.render_mode.is_traced() || self.tracer.gpu_scene().is_none() {
            return None;
        }
        accumulation_bytes(self.targets.size, self.traced.accumulation_limit).err()
    }

    /// Lowers the accumulation limit below the device's, so a test can refuse an output without a huge one.
    #[doc(hidden)]
    pub fn cap_accumulation_bytes(&mut self, bytes: u64) {
        self.traced.accumulation_limit = self.traced.accumulation_limit.min(bytes);
    }

    /// Samples per pixel in the path tracer's running mean; 0 in any other mode.
    pub fn accumulated_samples(&self) -> u64 {
        self.traced.accumulation.samples()
    }

    /// A shader reload's kernels, resolve and sky; the mean starts over, since it was traced by the old kernel.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn replace_traced_pipelines(
        &mut self,
        passes: TracedPasses,
        sky: wgpu::RenderPipeline,
    ) {
        self.traced.passes = Some(passes);
        self.traced.sky = Some(sky);
        self.traced.accumulation.reset();
    }

    /// Whether the traced kernel for `mode` has been built, for the reload tests.
    #[doc(hidden)]
    pub fn traced_kernel_compiled(&self, mode: RenderMode) -> bool {
        let kernel = match mode {
            RenderMode::PathTraced => Kernel::Path,
            _ => Kernel::Ray,
        };
        self.traced
            .passes
            .as_ref()
            .is_some_and(|passes| passes.is_compiled(kernel))
    }

    /// On a switch of mode or raster path, the profiler drops the passes the new frames do not record, at once.
    pub(super) fn forget_timings_of_another_frame(&mut self) {
        let mode = self.drawn_mode();
        let path = if mode.is_traced() {
            RasterPath::Forward
        } else {
            self.raster_path
        };
        let fxaa = self.fxaa_filters(mode.is_traced());
        if self.traced.timed_as.replace((mode, path, fxaa)) == Some((mode, path, fxaa)) {
            return;
        }
        self.gpu_timing
            .forget(|pass| records(pass, mode, path, fxaa));
    }

    /// Starts the path tracer's mean over: the hook for anything that changes what it traces in place.
    pub fn reset_accumulation(&mut self) {
        self.traced.accumulation.reset();
    }

    /// Traces this frame into the HDR target once a traced mode is ready; false leaves the frame to raster.
    pub(super) fn encode_traced(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) -> bool {
        let mode = self.drawn_mode();
        if mode != RenderMode::PathTraced {
            // The ray tracer writes the same buffer, and a raster frame may outlive the scene it held.
            self.traced.accumulation.reset();
        }
        if !mode.is_traced() {
            self.log_traced_fallback();
            return false;
        }
        self.sync_animated_instances(gpu);
        let settings = self.trace_settings.clamped();
        let (kernel, timed, dispatches) = self.plan_traced_frame(mode, settings, view);
        let uniforms = self.trace_uniforms(view, settings);
        if !self.encode_traced_passes(gpu, encoder, (kernel, timed), uniforms, &dispatches) {
            return false;
        }
        if kernel == Kernel::Path {
            self.traced
                .accumulation
                .add(u64::from(settings.samples_per_pixel));
        }
        // No raster pass ran, so none reports what it drew, as in the C++ engine's traced frames.
        (self.culling.drawn, self.culling.considered) = (0, 0);
        self.culling.primitives = (0, 0);
        (self.shadow.casters_drawn, self.shadow.casters_considered) = (0, 0);
        true
    }

    /// The kernel, its timed pass and its dispatches; a path-traced frame adds to the samples its history kept.
    fn plan_traced_frame(
        &mut self,
        mode: RenderMode,
        settings: TraceSettings,
        view: &FrameView,
    ) -> (Kernel, TimedPass, Vec<TraceDispatch>) {
        let size = (view.width, view.height);
        if mode == RenderMode::RayTraced {
            return (Kernel::Ray, TimedPass::RayTrace, plan(size, 1, 1, 0));
        }
        let key = AccumulationKey {
            view_proj: view.view_proj,
            eye: view.eye,
            size,
            light_dir: self.light_dir_ambient.truncate(),
            light_color_intensity: self.light_color_intensity,
            sky_light: self.sky_light(),
            settings,
            scene: self.tracer.generation(),
            lights: self.lights.fingerprint(),
            environment: self.ibl.generation,
        };
        let accumulated = self.traced.accumulation.begin(key);
        let dispatches = plan(
            size,
            settings.samples_per_pixel,
            settings.max_bounces,
            accumulated,
        );
        (Kernel::Path, TimedPass::PathTrace, dispatches)
    }

    /// The camera, the sun, the punctual lights, the environment and the furnace as both kernels read them.
    fn trace_uniforms(&self, view: &FrameView, settings: TraceSettings) -> TraceFrameUniforms {
        let light = self.light_dir_ambient.truncate().normalize_or(Vec3::Y);
        let colour = self.light_color_intensity;
        TraceFrameUniforms {
            size: [view.width, view.height, settings.max_bounces, 0],
            inv_view_proj: view.view_proj.inverse().to_cols_array_2d(),
            light_dir: [light.x, light.y, light.z, colour.w],
            light_radiance: (colour.truncate() * colour.w)
                .extend(self.sky_light())
                .to_array(),
            furnace: settings.furnace.map_or([0.0; 4], |furnace| {
                [furnace.radiance, furnace.metallic, furnace.roughness, 1.0]
            }),
            view_proj: view.view_proj.to_cols_array_2d(),
            lights: [
                self.lights.punctual_count,
                u32::from(self.environment_enabled()),
                0,
                0,
            ],
        }
    }

    /// The raster's ambient scale for the sky's light, so a path is lit as the raster's ambient term is.
    fn sky_light(&self) -> f32 {
        let environment = if self.environment_enabled() {
            self.ibl_intensity
        } else {
            1.0
        };
        self.light_dir_ambient.w * environment
    }

    /// Records the kernel and the resolve; false when this output's accumulation does not fit the device.
    fn encode_traced_passes(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        (kernel, timed): (Kernel, TimedPass),
        uniforms: TraceFrameUniforms,
        dispatches: &[TraceDispatch],
    ) -> bool {
        let Some(scene) = self.tracer.gpu_scene() else {
            return false;
        };
        let device = &gpu.device;
        let traced = &mut self.traced;
        let passes = traced
            .passes
            .get_or_insert_with(|| TracedPasses::new(device));
        let sky_res = &self.sky;
        // The C++ ray-traced mode shows the sky pass where its rays miss; the path tracer keeps its own sky, as in C++.
        let sky_pipeline = (kernel == Kernel::Ray).then(|| {
            &*traced.sky.get_or_insert_with(|| {
                sky::sky_pipeline(device, &sky_res.shader, &sky_res.pipeline_layout, 1)
            })
        });
        let draw_sky = sky_pipeline.map(|pipeline| {
            move |pass: &mut wgpu::RenderPass<'_>| sky_res.draw_with(pass, pipeline)
        });
        let lights = TraceLights {
            punctual: &self.lights.storage_buffer,
            environment: self
                .ibl
                .environment
                .as_ref()
                .map_or(&self.ibl.fallback.prefiltered, |environment| {
                    environment.environment_view()
                }),
        };
        let frame = TracedFrame {
            kernel,
            scene,
            lights,
            uniforms,
            dispatches,
            hdr_view: &self.targets.hdr_view,
            depth_view: &self.targets.depth,
            after_resolve: draw_sky.as_ref().map(|draw| draw as AfterResolve<'_>),
        };
        let scopes = (
            self.gpu_timing.scope(timed),
            self.gpu_timing.scope(TimedPass::TraceResolve),
        );
        if let Err(why) = passes.encode(gpu, encoder, &frame, scopes) {
            log::error!("tracer: {why}, although the drawn mode said it fits");
            return false;
        }
        true
    }

    /// Logs once per output size that a traced pick rasters because its accumulation does not fit.
    fn log_traced_fallback(&mut self) {
        let Some(why) = self.traced_fallback() else {
            return;
        };
        let size = self.targets.size;
        if self.traced.refused.replace(size) != Some(size) {
            log::warn!("tracer: {why}; the frame rasters");
        }
    }

    /// Moves the traced scene's animated instances to the poses the raster draws, rebuilding the top level on a change.
    fn sync_animated_instances(&mut self, gpu: &GpuContext) {
        if !self.has_animations() {
            return;
        }
        let transforms: Vec<Mat4> = self.primitives.iter().map(|p| p.model).collect();
        let (generation, synced) = &self.traced.synced;
        if *generation == self.tracer.generation() && *synced == transforms {
            return;
        }
        self.tracer.set_primitive_transforms(gpu, &transforms);
        self.traced.synced = (self.tracer.generation(), transforms);
    }
}
