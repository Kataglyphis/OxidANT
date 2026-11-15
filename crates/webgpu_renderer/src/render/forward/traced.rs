//! The renderer's traced scene and modes: the CPU scene it builds from, the build a traced mode starts, and a traced frame.

use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::frame::FrameView;
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::gpu_timing::TimedPass;
use crate::render::tracer::modes::{
    plan, Accumulation, AccumulationKey, Kernel, TraceDispatch, TraceFrameUniforms, TracedFrame,
    TracedPasses,
};
use crate::render::tracer::{RenderMode, TraceSettings, TraceStatus, Tracer};
use crate::scene::CpuScene;

/// The traced modes' GPU half, built on first use, and the path tracer's running mean.
#[derive(Default)]
pub(super) struct TracedState {
    passes: Option<TracedPasses>,
    accumulation: Accumulation,
    /// The tracer generation and animated transforms its instances were last moved to.
    synced: (u64, Vec<Mat4>),
    /// The output an accumulation was refused for, so the refusal is logged once.
    refused: Option<(u32, u32)>,
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

    /// What a frame draws: the picked traced mode once its scene is on the GPU, raster until then.
    pub fn drawn_mode(&self) -> RenderMode {
        if self.tracer.gpu_scene().is_some() {
            self.render_mode
        } else {
            RenderMode::Raster
        }
    }

    /// Samples per pixel in the path tracer's running mean; 0 in any other mode.
    pub fn accumulated_samples(&self) -> u64 {
        self.traced.accumulation.samples()
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

    /// The camera, the sun and the furnace as both kernels read them.
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
        let passes = self
            .traced
            .passes
            .get_or_insert_with(|| TracedPasses::new(&gpu.device));
        let frame = TracedFrame {
            kernel,
            scene,
            uniforms,
            dispatches,
            hdr_view: &self.targets.hdr_view,
        };
        let scopes = (
            self.gpu_timing.scope(timed),
            self.gpu_timing.scope(TimedPass::TraceResolve),
        );
        let Err(why) = passes.encode(gpu, encoder, &frame, scopes) else {
            return true;
        };
        let size = (uniforms.size[0], uniforms.size[1]);
        if self.traced.refused.replace(size) != Some(size) {
            log::warn!("tracer: {why}; the frame rasters");
        }
        false
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
