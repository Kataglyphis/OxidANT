//! Occlusion culling: this frame's depth tested against every primitive's box, read back next frame.

use glam::Vec3;

use super::frame::FrameView;
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::gpu_occlusion::GpuCulling;
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::Resource;
use crate::render::occlusion::OcclusionQueries;

pub(super) struct Culling {
    /// Hardware occlusion queries over the forward depth; records only when enabled.
    pub(super) occlusion: OcclusionQueries,
    /// Compute-shader occlusion culling, created on first use when enabled.
    pub(super) gpu_culling: Option<GpuCulling>,
    /// Opaque primitives drawn / considered by the camera pass last frame, after all culling.
    pub(super) drawn: u32,
    pub(super) considered: u32,
    /// Every primitive the camera pass drew last frame, and every one uploaded.
    pub(super) primitives: (u32, u32),
    /// (`occlusion_queries_enabled`, `gpu_culling_enabled`) as the last frame saw them.
    modes: (bool, bool),
}

impl Culling {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            occlusion: OcclusionQueries::new(device),
            gpu_culling: None,
            drawn: 0,
            considered: 0,
            primitives: (0, 0),
            modes: (false, false),
        }
    }
}

impl ForwardRenderer {
    /// Primitives the camera pass drew last frame and primitives uploaded: the C++ Visibility tab's meshes.
    pub fn primitive_stats(&self) -> (u32, u32) {
        self.culling.primitives
    }

    /// A switched culling mode forgets every readback: they describe a camera from before the switch.
    pub(super) fn reset_culling_on_mode_change(&mut self) {
        let modes = (self.occlusion_queries_enabled, self.gpu_culling_enabled);
        if modes == self.culling.modes {
            return;
        }
        self.culling.modes = modes;
        self.culling.occlusion.reset();
        if let Some(gpu_cull) = self.culling.gpu_culling.as_mut() {
            gpu_cull.reset();
        }
    }

    /// Last frame's result; unmeasured primitives count as visible, so nothing pops.
    pub(super) fn occluded_last_frame(&self, primitive_index: usize) -> bool {
        if self.gpu_culling_enabled {
            self.culling
                .gpu_culling
                .as_ref()
                .is_some_and(|c| !c.visible(primitive_index))
        } else if self.occlusion_queries_enabled {
            !self.culling.occlusion.visible(primitive_index)
        } else {
            false
        }
    }

    /// Occlusion detection: AABB proxies vs this frame's depth, by compute or hardware queries.
    pub(super) fn encode_occlusion(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) {
        if self.primitives.is_empty() {
            return;
        }
        let aabbs: Vec<(Vec3, Vec3)> = self
            .primitives
            .iter()
            .map(|p| (p.aabb_min, p.aabb_max))
            .collect();

        if self.gpu_culling_enabled {
            self.encode_gpu_culling(gpu, encoder, view, &aabbs);
        } else if self.occlusion_queries_enabled {
            // Hardware occlusion query path
            self.culling.occlusion.record(
                &gpu.device,
                &gpu.queue,
                encoder,
                self.targets.view(Resource::Depth),
                view.view_proj,
                &aabbs,
                view.eye,
                self.gpu_timing.scope(TimedPass::OcclusionCull),
            );
        }
    }

    /// The compute-shader path, against the resolved single-sample depth.
    fn encode_gpu_culling(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
        aabbs: &[(Vec3, Vec3)],
    ) {
        let primitive_count = self.primitives.len();
        let gpu_cull = self
            .culling
            .gpu_culling
            .get_or_insert_with(|| GpuCulling::new(gpu, primitive_count.max(1024)));
        let (width, height) = self.targets.size;
        let inv_view_proj = view.view_proj.inverse();
        gpu_cull.cull(
            gpu,
            encoder,
            self.targets.view(Resource::Depth),
            width,
            height,
            bytemuck::cast_ref(&view.view_proj),
            bytemuck::cast_ref(&inv_view_proj),
            aabbs,
        );
    }
}
