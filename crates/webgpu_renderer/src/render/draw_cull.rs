//! Per-draw culling on the GPU: a compute pass writes every draw's indirect arguments, no instances when culled.
//! The camera's draws are frustum- and occlusion-tested, each cascade's casters frustum-tested; the CPU reads only stats.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DrawIndexedIndirectArgs;

use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::bounds::Frustum;
use crate::render::buffer_desc;
use crate::render::gpu_timing::PassScope;
use crate::render::occlusion::newer_readback;
use crate::render::pipeline_desc;

/// Must match `cs_main`'s `@workgroup_size(64, 1, 1)` in `draw_cull.wgsl`.
pub const DRAW_CULL_WORKGROUP: u32 = 64;
/// Cascades the views uniform holds planes for, the shader's `cascade_planes[18]` over six planes.
pub const MAX_CULL_CASCADES: usize = 3;
/// Bytes of one draw's arguments, the stride between slots in the args buffer.
pub const ARGS_BYTES: u64 = std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
/// Counters in the stats buffer, in [`DrawStats`] field order.
const STAT_COUNT: usize = 5;
const STATS_BYTES: u64 = (STAT_COUNT * 4) as u64;

/// [`DrawInput::flags`]: drawn after the sky, sorted, never occlusion-tested.
pub const DRAW_BLENDED: u32 = 1;
/// [`DrawInput::flags`]: drawn into the shadow cascades.
pub const DRAW_CASTS_SHADOW: u32 = 2;
/// [`DrawInput::flags`]: the primitive's first slot, which alone adds it to the stats.
pub const DRAW_COUNTED: u32 = 4;

const VIEW_FRUSTUM: u32 = 1;
const VIEW_OCCLUSION_VISIBILITY: u32 = 2;
const VIEW_OCCLUSION_SAMPLES: u32 = 4;

/// One draw slot: a primitive's instance run, as `DrawInput` in `draw_cull.slang`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct DrawInput {
    pub aabb_min: [f32; 4],
    pub aabb_max: [f32; 4],
    pub primitive: u32,
    pub flags: u32,
    /// The camera's LOD-selected index count.
    pub camera_index_count: u32,
    /// The full mesh's, which the shadow casters draw.
    pub shadow_index_count: u32,
    pub instance_count: u32,
    pub _pad: [u32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CullViews {
    camera_planes: [[f32; 4]; 6],
    cascade_planes: [[f32; 4]; 6 * MAX_CULL_CASCADES],
    eye: [f32; 4],
    prev_eye: [f32; 4],
    slot_count: u32,
    cascade_count: u32,
    flags: u32,
    occlusion_count: u32,
    slot_stride: u32,
    _pad: [u32; 3],
}

/// Where last frame's occlusion measurement lives on the GPU, and how many primitives it covers.
#[derive(Copy, Clone)]
pub enum OcclusionSource<'a> {
    None,
    /// One u32 per primitive, non-zero where visible: the compute culling path.
    Visibility(&'a wgpu::Buffer, u32),
    /// One u64 sample count per primitive: the hardware query path.
    Samples(&'a wgpu::Buffer, u32),
}

/// Everything one dispatch tests against.
pub struct CullFrame<'a> {
    pub camera: Mat4,
    /// The active cascades' view-projections; at most [`MAX_CULL_CASCADES`].
    pub cascades: &'a [Mat4],
    pub eye: Vec3,
    /// The eye the occlusion source measured from, last frame's.
    pub occlusion_eye: Vec3,
    pub frustum_culling: bool,
    pub occlusion: OcclusionSource<'a>,
}

/// What one frame's dispatch counted, per primitive rather than per slot.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
#[repr(C)]
pub struct DrawStats {
    /// Opaque primitives inside the camera frustum.
    pub opaque_considered: u32,
    /// Of those, the ones occlusion left drawn.
    pub opaque_drawn: u32,
    pub blended_drawn: u32,
    /// Casters times active cascades.
    pub casters_considered: u32,
    /// Caster draws inside their cascade, summed over cascades.
    pub casters_drawn: u32,
}

pub struct DrawCull {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    views: wgpu::Buffer,
    inputs: wgpu::Buffer,
    args: wgpu::Buffer,
    stats: wgpu::Buffer,
    /// Bound in place of an occlusion source while there is none.
    no_occlusion: wgpu::Buffer,
    /// Slots `inputs` holds; `args` holds one camera and [`MAX_CULL_CASCADES`] cascade arguments per slot.
    capacity: u32,
    slot_count: u32,
    /// The inputs last written, so an unchanged frame uploads nothing.
    uploaded: Vec<DrawInput>,
    /// Bumped whenever `args` is replaced, so recorded bundles know they point at a dropped buffer.
    generation: u64,
    readback: StatsReadback,
}

impl DrawCull {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw_cull_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/draw_cull.wgsl").into()),
        });
        let compute = wgpu::ShaderStages::COMPUTE;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw_cull_bgl"),
            entries: &[
                bind_layout::uniform(0, compute),
                bind_layout::storage_buffer(1, compute, true),
                bind_layout::storage_buffer(2, compute, true),
                bind_layout::storage_buffer(3, compute, false),
                bind_layout::storage_buffer(4, compute, false),
            ],
        });
        let pipeline_layout = pipeline_desc::single_layout(device, "draw_cull_layout", &layout);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("draw_cull_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let capacity = 64;
        Self {
            pipeline,
            layout,
            views: buffer_desc::uniform(
                device,
                "draw_cull_views",
                std::mem::size_of::<CullViews>() as u64,
            ),
            inputs: inputs_buffer(device, capacity),
            args: args_buffer(device, capacity),
            stats: buffer_desc::storage_counters(device, "draw_cull_stats", STATS_BYTES),
            no_occlusion: buffer_desc::storage_dst(device, "draw_cull_no_occlusion", 16),
            capacity,
            slot_count: 0,
            uploaded: Vec::new(),
            generation: 0,
            readback: StatsReadback::new(device),
        }
    }

    /// The arguments every indirect draw reads: slot `s` of view `v` (0 the camera, then each cascade) at [`Self::args_offset`].
    pub fn args(&self) -> &wgpu::Buffer {
        &self.args
    }

    /// Byte offset of `slot`'s arguments for `view`.
    pub fn args_offset(&self, view: usize, slot: usize) -> wgpu::BufferAddress {
        (view as u64 * u64::from(self.capacity) + slot as u64) * ARGS_BYTES
    }

    /// Changes whenever [`Self::args`] is a new buffer.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Writes this frame's slots, growing the buffers when they no longer fit.
    pub fn upload(&mut self, gpu: &GpuContext, inputs: &[DrawInput]) {
        let needed = inputs.len() as u32;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two();
            self.inputs = inputs_buffer(&gpu.device, self.capacity);
            self.args = args_buffer(&gpu.device, self.capacity);
            self.generation += 1;
            self.uploaded.clear();
        }
        self.slot_count = needed;
        if self.uploaded != inputs {
            gpu.queue
                .write_buffer(&self.inputs, 0, bytemuck::cast_slice(inputs));
            self.uploaded.clear();
            self.uploaded.extend_from_slice(inputs);
        }
    }

    /// Clears the stats and writes every slot's arguments for `frame`; copies the stats out when a slot is free.
    pub fn encode(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        frame: &CullFrame<'_>,
        timing: PassScope<'_>,
    ) {
        gpu.queue
            .write_buffer(&self.views, 0, bytemuck::bytes_of(&self.views_for(frame)));
        encoder.clear_buffer(&self.stats, 0, None);
        if self.slot_count > 0 {
            let bind_group = self.bind_group(&gpu.device, frame.occlusion);
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("draw_cull"),
                timestamp_writes: timing.compute_writes(0, 1),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(self.slot_count.div_ceil(DRAW_CULL_WORKGROUP), 1, 1);
        }
        self.readback.record(encoder, &self.stats);
    }

    fn views_for(&self, frame: &CullFrame<'_>) -> CullViews {
        let planes = |m: &Mat4| Frustum::from_view_proj(m).planes().map(|p| p.to_array());
        let mut cascade_planes = [[0.0; 4]; 6 * MAX_CULL_CASCADES];
        let cascades = &frame.cascades[..frame.cascades.len().min(MAX_CULL_CASCADES)];
        for (chunk, cascade) in cascade_planes.chunks_mut(6).zip(cascades) {
            chunk.copy_from_slice(&planes(cascade));
        }
        let (occlusion_flag, occlusion_count) = match frame.occlusion {
            OcclusionSource::None => (0, 0),
            OcclusionSource::Visibility(_, count) => (VIEW_OCCLUSION_VISIBILITY, count),
            OcclusionSource::Samples(_, count) => (VIEW_OCCLUSION_SAMPLES, count),
        };
        let frustum_flag = if frame.frustum_culling {
            VIEW_FRUSTUM
        } else {
            0
        };
        CullViews {
            camera_planes: planes(&frame.camera),
            cascade_planes,
            eye: frame.eye.extend(0.0).to_array(),
            prev_eye: frame.occlusion_eye.extend(0.0).to_array(),
            slot_count: self.slot_count,
            cascade_count: cascades.len() as u32,
            flags: frustum_flag | occlusion_flag,
            occlusion_count,
            slot_stride: self.capacity,
            _pad: [0; 3],
        }
    }

    fn bind_group(&self, device: &wgpu::Device, occlusion: OcclusionSource<'_>) -> wgpu::BindGroup {
        let occlusion = match occlusion {
            OcclusionSource::None => &self.no_occlusion,
            OcclusionSource::Visibility(buffer, _) | OcclusionSource::Samples(buffer, _) => buffer,
        };
        let buffers = [
            &self.views,
            &self.inputs,
            occlusion,
            &self.args,
            &self.stats,
        ];
        let entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
            .iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw_cull_bg"),
            layout: &self.layout,
            entries: &entries,
        })
    }

    /// Starts this frame's stats readback and takes any that landed; never blocks, call after submit.
    pub fn end_frame(&mut self, device: &wgpu::Device) {
        self.readback.end_frame(device);
    }

    /// The newest stats that landed, `None` until one does after [`Self::forget`].
    pub fn stats(&self) -> Option<DrawStats> {
        self.readback.latest
    }

    /// Drops every stats readback in flight, for a frame that culled nothing.
    pub fn forget(&mut self) {
        self.readback.generation += 1;
        self.readback.latest = None;
    }

    /// Every slot's arguments for `view` as the last submitted frame wrote them; stalls, so tests only.
    pub fn read_args(
        &self,
        gpu: &GpuContext,
        view: usize,
    ) -> anyhow::Result<Vec<DrawIndexedIndirectArgs>> {
        let bytes = u64::from(self.slot_count) * ARGS_BYTES;
        if bytes == 0 {
            return Ok(Vec::new());
        }
        let staging = buffer_desc::readback(&gpu.device, "draw_cull_args_readback", bytes);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("draw_cull_args_readback"),
            });
        encoder.copy_buffer_to_buffer(&self.args, self.args_offset(view, 0), &staging, 0, bytes);
        gpu.queue.submit(Some(encoder.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        let view = slice.get_mapped_range()?;
        let args = bytemuck::pod_collect_to_vec::<u8, u32>(&view)
            .as_chunks::<5>()
            .0
            .iter()
            .map(|w| DrawIndexedIndirectArgs {
                index_count: w[0],
                instance_count: w[1],
                first_index: w[2],
                base_vertex: w[3] as i32,
                first_instance: w[4],
            })
            .collect();
        Ok(args)
    }
}

fn inputs_buffer(device: &wgpu::Device, slots: u32) -> wgpu::Buffer {
    let bytes = u64::from(slots) * std::mem::size_of::<DrawInput>() as u64;
    buffer_desc::storage_dst(device, "draw_cull_inputs", bytes)
}

fn args_buffer(device: &wgpu::Device, slots: u32) -> wgpu::Buffer {
    let views = 1 + MAX_CULL_CASCADES as u64;
    buffer_desc::indirect_args(
        device,
        "draw_cull_args",
        views * u64::from(slots) * ARGS_BYTES,
    )
}

/// Map state of one readback slot, shared with the `map_async` callback.
const MAP_PENDING: u8 = 0;
const MAP_READY: u8 = 1;
const MAP_FAILED: u8 = 2;
/// Frame latency 2, plus a spare so an empty poll drops no frame.
const READBACK_SLOTS: usize = 3;

struct ReadbackSlot {
    buffer: wgpu::Buffer,
    map_state: Option<Arc<AtomicU8>>,
    generation: u64,
    sequence: u64,
}

/// The stats' non-blocking readback ring, as the occlusion paths' rings.
struct StatsReadback {
    slots: Vec<ReadbackSlot>,
    /// The slot this frame copied into, if one was free.
    recorded: Option<usize>,
    generation: u64,
    sequence: u64,
    applied_sequence: u64,
    latest: Option<DrawStats>,
}

impl StatsReadback {
    fn new(device: &wgpu::Device) -> Self {
        let slots = (0..READBACK_SLOTS)
            .map(|i| ReadbackSlot {
                buffer: buffer_desc::readback(device, &format!("draw_cull_stats_{i}"), STATS_BYTES),
                map_state: None,
                generation: 0,
                sequence: 0,
            })
            .collect();
        Self {
            slots,
            recorded: None,
            generation: 0,
            sequence: 0,
            applied_sequence: 0,
            latest: None,
        }
    }

    fn record(&mut self, encoder: &mut wgpu::CommandEncoder, stats: &wgpu::Buffer) {
        self.recorded = self.slots.iter().position(|slot| slot.map_state.is_none());
        if let Some(index) = self.recorded {
            encoder.copy_buffer_to_buffer(stats, 0, &self.slots[index].buffer, 0, STATS_BYTES);
        }
    }

    fn end_frame(&mut self, device: &wgpu::Device) {
        if let Some(index) = self.recorded.take() {
            let state = Arc::new(AtomicU8::new(MAP_PENDING));
            let callback_state = Arc::clone(&state);
            self.slots[index]
                .buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let code = if result.is_ok() {
                        MAP_READY
                    } else {
                        MAP_FAILED
                    };
                    callback_state.store(code, Ordering::Release);
                });
            self.sequence += 1;
            let slot = &mut self.slots[index];
            slot.map_state = Some(state);
            slot.generation = self.generation;
            slot.sequence = self.sequence;
        }
        // Never a waiting poll: the stats are advisory and must not stall the frame.
        let _ = device.poll(wgpu::PollType::Poll);
        for index in 0..self.slots.len() {
            self.drain(index);
        }
    }

    fn drain(&mut self, index: usize) {
        let slot = &mut self.slots[index];
        let Some(state) = slot.map_state.as_ref() else {
            return;
        };
        match state.load(Ordering::Acquire) {
            MAP_READY => {
                let stats = slot
                    .buffer
                    .slice(..)
                    .get_mapped_range()
                    .map(|view| *bytemuck::from_bytes::<DrawStats>(&view))
                    .ok();
                slot.buffer.unmap();
                slot.map_state = None;
                let current = slot.generation == self.generation;
                if current && newer_readback(slot.sequence, &mut self.applied_sequence) {
                    self.latest = stats.or(self.latest);
                }
            }
            MAP_FAILED => slot.map_state = None,
            _ => {}
        }
    }
}
