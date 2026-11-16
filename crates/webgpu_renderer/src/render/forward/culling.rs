//! Culling: the draw cull writes every draw's indirect arguments on the GPU, from the camera and cascade frusta
//! and last frame's occlusion, which this frame's depth measures for the next.

use std::ops::Range;

use glam::Vec3;

use super::frame::FrameView;
use super::primitive::GpuPrimitive;
use super::ForwardRenderer;
use crate::context::GpuContext;
use crate::render::draw_cull::{
    CullFrame, DrawCull, DrawInput, DrawStats, OcclusionSource, DRAW_BLENDED, DRAW_CASTS_SHADOW,
    DRAW_COUNTED,
};
use crate::render::gpu_occlusion::GpuCulling;
use crate::render::gpu_timing::TimedPass;
use crate::render::graph::Resource;
use crate::render::occlusion::OcclusionQueries;
use crate::scene::InstanceRaw;

/// One indirect draw: a primitive's run of instances that share a winding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct DrawSlot {
    pub(super) first_instance: u32,
    pub(super) clockwise: bool,
}

impl DrawSlot {
    /// Where the run starts in the instance buffer: indirect draws start at instance 0 without `INDIRECT_FIRST_INSTANCE`.
    pub(super) fn instance_offset(self) -> wgpu::BufferAddress {
        u64::from(self.first_instance) * std::mem::size_of::<InstanceRaw>() as u64
    }
}

/// What a frame's occlusion row measured, which the next frame's draw cull reads.
#[derive(Copy, Clone)]
struct Measured {
    /// Hardware query sample counts, else the compute path's visibility.
    queries: bool,
    count: u32,
    eye: Vec3,
}

pub(super) struct Culling {
    /// Hardware occlusion queries over the forward depth; records only when enabled.
    pub(super) occlusion: OcclusionQueries,
    /// Compute-shader occlusion culling, created on first use when enabled.
    pub(super) gpu_culling: Option<GpuCulling>,
    pub(super) draw_cull: DrawCull,
    /// Every primitive's instance runs in primitive order; slot `i`'s arguments sit at `i` in each view.
    pub(super) slots: Vec<DrawSlot>,
    /// Each primitive's range of `slots`.
    pub(super) primitive_slots: Vec<Range<usize>>,
    /// This frame's slot inputs, kept to reuse the allocation.
    inputs: Vec<DrawInput>,
    measured: Option<Measured>,
    /// Opaque primitives drawn / considered by the camera pass, after all culling, as the last stats readback counted.
    pub(super) drawn: u32,
    pub(super) considered: u32,
    /// Every primitive the camera pass drew, and every one uploaded.
    pub(super) primitives: (u32, u32),
    /// (`occlusion_queries_enabled`, `gpu_culling_enabled`) as the last frame saw them.
    modes: (bool, bool),
}

impl Culling {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            occlusion: OcclusionQueries::new(device),
            gpu_culling: None,
            draw_cull: DrawCull::new(device),
            slots: Vec::new(),
            primitive_slots: Vec::new(),
            inputs: Vec::new(),
            measured: None,
            drawn: 0,
            considered: 0,
            primitives: (0, 0),
            modes: (false, false),
        }
    }

    /// Forgets every occlusion measurement: they are per index of a primitive list, or of a mode, that is gone.
    pub(super) fn reset_occlusion(&mut self) {
        self.occlusion.reset();
        if let Some(gpu_cull) = self.gpu_culling.as_mut() {
            gpu_cull.reset();
        }
        self.measured = None;
    }

    /// `primitive`'s slots with their indices.
    pub(super) fn slots_of(
        &self,
        primitive: usize,
    ) -> impl Iterator<Item = (usize, DrawSlot)> + '_ {
        let range = self.primitive_slots[primitive].clone();
        range.clone().zip(self.slots[range].iter().copied())
    }

    /// Rebuilds the slots and their inputs; the camera's index count follows the LOD `eye` selects.
    fn build_slots(&mut self, primitives: &[GpuPrimitive], eye: Vec3) {
        self.slots.clear();
        self.primitive_slots.clear();
        self.inputs.clear();
        for (index, prim) in primitives.iter().enumerate() {
            let start = self.slots.len();
            for (run, (instances, clockwise)) in prim.instance_runs().enumerate() {
                self.slots.push(DrawSlot {
                    first_instance: instances.start,
                    clockwise,
                });
                let counted = if run == 0 { DRAW_COUNTED } else { 0 };
                self.inputs
                    .push(slot_input(prim, index, instances, counted, eye));
            }
            self.primitive_slots.push(start..self.slots.len());
        }
    }
}

/// The GPU's view of one slot.
fn slot_input(
    prim: &GpuPrimitive,
    index: usize,
    instances: Range<u32>,
    counted: u32,
    eye: Vec3,
) -> DrawInput {
    let mut flags = counted;
    if prim.alpha_blend {
        flags |= DRAW_BLENDED;
    }
    if prim.casts_shadow {
        flags |= DRAW_CASTS_SHADOW;
    }
    DrawInput {
        aabb_min: prim.aabb_min.extend(0.0).to_array(),
        aabb_max: prim.aabb_max.extend(0.0).to_array(),
        primitive: index as u32,
        flags,
        camera_index_count: prim.geometry_for(eye).2,
        shadow_index_count: prim.index_count,
        instance_count: instances.len() as u32,
        _pad: [0; 3],
    }
}

impl ForwardRenderer {
    /// Primitives the camera pass drew last frame and primitives uploaded: the C++ Visibility tab's meshes.
    pub fn primitive_stats(&self) -> (u32, u32) {
        self.culling.primitives
    }

    /// Every slot's indirect arguments for `view` (0 the camera, then each cascade) as the last frame wrote them.
    /// Stalls on a readback, so tests and diagnostics only.
    #[doc(hidden)]
    pub fn read_draw_args(
        &self,
        gpu: &GpuContext,
        view: usize,
    ) -> anyhow::Result<Vec<wgpu::util::DrawIndexedIndirectArgs>> {
        self.culling.draw_cull.read_args(gpu, view)
    }

    /// Each primitive's slots, in the order [`Self::read_draw_args`] returns their arguments.
    #[doc(hidden)]
    pub fn draw_slots_of(&self, primitive_index: usize) -> Range<usize> {
        self.culling
            .primitive_slots
            .get(primitive_index)
            .cloned()
            .unwrap_or(0..0)
    }

    /// A switched culling mode forgets every measurement: they describe a camera from before the switch.
    pub(super) fn reset_culling_on_mode_change(&mut self) {
        let modes = (self.occlusion_queries_enabled, self.gpu_culling_enabled);
        if modes == self.culling.modes {
            return;
        }
        self.culling.modes = modes;
        self.culling.reset_occlusion();
    }

    /// This frame's slots, uploaded before the encoder exists.
    pub(super) fn prepare_draw_cull(&mut self, gpu: &GpuContext, view: &FrameView) {
        self.culling.build_slots(&self.primitives, view.eye);
        let culling = &mut self.culling;
        culling.draw_cull.upload(gpu, &culling.inputs);
    }

    /// Writes every slot's arguments for the camera and the active cascades.
    pub(super) fn encode_draw_cull(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
    ) {
        let measured = self.culling.measured.take();
        let cascades = self
            .shadows
            .active_cascades()
            .min(self.shadow.layer_views.len());
        let occlusion = match measured {
            Some(m) if m.queries => {
                OcclusionSource::Samples(self.culling.occlusion.samples_buffer(), m.count)
            }
            Some(m) => match self.culling.gpu_culling.as_ref() {
                Some(gpu_cull) => {
                    OcclusionSource::Visibility(gpu_cull.visibility_buffer(), m.count)
                }
                None => OcclusionSource::None,
            },
            None => OcclusionSource::None,
        };
        let frame = CullFrame {
            camera: view.view_proj,
            cascades: &self.shadow.cascade_matrices[..cascades],
            eye: view.eye,
            occlusion_eye: measured.map_or(view.eye, |m| m.eye),
            frustum_culling: self.frustum_culling,
            occlusion,
        };
        let timing = self.gpu_timing.scope(TimedPass::DrawCull);
        self.culling.draw_cull.encode(gpu, encoder, &frame, timing);
    }

    /// A frame that rasters nothing reports no draws, and leaves no measurement for a later raster frame.
    pub(super) fn forget_raster_culling(&mut self) {
        (self.culling.drawn, self.culling.considered) = (0, 0);
        self.culling.primitives = (0, 0);
        (self.shadow.casters_drawn, self.shadow.casters_considered) = (0, 0);
        self.culling.draw_cull.forget();
        self.culling.measured = None;
    }

    /// Takes the newest stats readback into what the stats getters report.
    pub(super) fn apply_draw_stats(&mut self) {
        let Some(stats) = self.culling.draw_cull.stats() else {
            return;
        };
        let DrawStats {
            opaque_considered,
            opaque_drawn,
            blended_drawn,
            casters_considered,
            casters_drawn,
        } = stats;
        self.culling.drawn = opaque_drawn;
        self.culling.considered = opaque_considered;
        self.culling.primitives = (opaque_drawn + blended_drawn, self.primitives.len() as u32);
        self.shadow.casters_drawn = casters_drawn;
        self.shadow.casters_considered = casters_considered;
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

        let (queries, count) = if self.gpu_culling_enabled {
            (false, self.encode_gpu_culling(gpu, encoder, view, &aabbs))
        } else if self.occlusion_queries_enabled {
            let count = self.culling.occlusion.record(
                &gpu.device,
                &gpu.queue,
                encoder,
                self.targets.view(Resource::Depth),
                view.view_proj,
                &aabbs,
                view.eye,
                self.gpu_timing.scope(TimedPass::OcclusionCull),
            );
            (true, count)
        } else {
            return;
        };
        self.culling.measured = (count > 0).then_some(Measured {
            queries,
            count,
            eye: view.eye,
        });
    }

    /// The compute-shader path, against the resolved single-sample depth; returns the primitives tested.
    fn encode_gpu_culling(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &FrameView,
        aabbs: &[(Vec3, Vec3)],
    ) -> u32 {
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
        )
    }
}
