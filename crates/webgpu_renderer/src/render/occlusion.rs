//! Occlusion detection: each primitive's AABB drawn in a hardware occlusion query over forward depth.
//! Queries, not Hi-Z, because WebGPU core has them; results drain from a non-blocking ring.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::forward::DEPTH_FORMAT;
use crate::render::pipeline_desc;

/// Frames in flight: two for `desired_maximum_frame_latency: 2`, plus a spare for an empty poll.
const SLOT_COUNT: usize = 3;

/// Bytes per occlusion query result (a `u64` sample count).
const QUERY_BYTES: u64 = 8;

/// Relative half-extent margin, matching `occlusion_bbox.wgsl`'s `0.5 * 0.02` term.
pub(crate) const CONTAINMENT_MARGIN: f32 = 0.02;

/// `p` inside `[min, max]` grown by `margin` plus 1 cm, the same box `occlusion_bbox.wgsl` rasterises.
pub(crate) fn aabb_contains(min: Vec3, max: Vec3, p: Vec3, margin: f32) -> bool {
    let expand = (max - min) * 0.5 * margin + Vec3::splat(0.01);
    p.cmpge(min - expand).all() && p.cmple(max + expand).all()
}

/// Map state of one readback slot, shared with the `map_async` callback.
const MAP_PENDING: u8 = 0;
const MAP_READY: u8 = 1;
const MAP_FAILED: u8 = 2;

/// One primitive's world AABB, an instance-step vertex; primitive `i` draws instances `i..i+1`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BboxInstance {
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
}

impl BboxInstance {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<BboxInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
    };
}

struct Slot {
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// `Some` while a map is outstanding; the callback sets READY/FAILED, `end_frame` drains it.
    map_state: Option<Arc<AtomicU8>>,
    /// Queries actually recorded into this slot (this frame's primitive count).
    count: u32,
    /// Scene generation at record time; a stale one is drained without touching visibility.
    generation: u64,
    /// "Eye inside this AABB" flags from this slot's own eye, since the readback lands late.
    forced_visible: Vec<bool>,
}

/// Per-primitive occlusion detection over the forward depth buffer.
pub struct OcclusionQueries {
    pipeline: wgpu::RenderPipeline,
    view_proj_buffer: wgpu::Buffer,
    view_proj_bind_group: wgpu::BindGroup,
    /// One AABB per primitive, grown with the scene. Rewritten every frame.
    instance_buffer: wgpu::Buffer,
    query_set: wgpu::QuerySet,
    /// Query-set count and the element capacity of every buffer above.
    capacity: u32,
    slots: Vec<Slot>,
    /// Slot the current frame records into.
    current: usize,
    /// True when this frame claimed a free slot and recorded a pass.
    recording: bool,
    /// Latest sample count per primitive, index-aligned to `ForwardRenderer::primitives`.
    samples: Vec<u64>,
    /// `samples[i] > 0`, cached so callers get a `&[bool]` without recomputing.
    visibility: Vec<bool>,
    /// Bumped by [`Self::reset`] so readbacks from a replaced scene are discarded.
    generation: u64,
}

impl OcclusionQueries {
    /// Initial per-buffer capacity; grown on the first frame that needs more.
    const INITIAL_CAPACITY: u32 = 32;

    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("occlusion_bbox_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/occlusion_bbox.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("occlusion_bind_group_layout"),
            entries: &[bind_layout::uniform(0, wgpu::ShaderStages::VERTEX)],
        });

        let pipeline_layout =
            pipeline_desc::single_layout(device, "occlusion_pipeline_layout", &bind_group_layout);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("occlusion_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(BboxInstance::LAYOUT)],
                compilation_options: Default::default(),
            },
            // No color targets, but WebGPU still wants a fragment stage for the pipeline.
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[],
                compilation_options: Default::default(),
            }),
            // Cull nothing: box winding is arbitrary and the camera may sit inside a box.
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // Test the forward depth but never write it: SSAO reads it next.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let view_proj_buffer = buffer_desc::uniform(
            device,
            "occlusion_view_proj",
            std::mem::size_of::<[[f32; 4]; 4]>() as wgpu::BufferAddress,
        );
        let view_proj_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occlusion_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_proj_buffer.as_entire_binding(),
            }],
        });

        let capacity = Self::INITIAL_CAPACITY;
        Self {
            pipeline,
            view_proj_buffer,
            view_proj_bind_group,
            instance_buffer: create_instance_buffer(device, capacity),
            query_set: create_query_set(device, capacity),
            capacity,
            slots: create_slots(device, capacity),
            current: 0,
            recording: false,
            samples: Vec::new(),
            visibility: Vec::new(),
            generation: 0,
        }
    }

    /// Records and resolves the pass in the forward encoder; call [`Self::end_frame`] after submit.
    /// A box containing `eye` reads 0 samples, so it is forced visible, with the mask kept per slot.
    // Eight distinct inputs; a wrapper struct would be artificial.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        view_proj: Mat4,
        aabbs: &[(Vec3, Vec3)],
        eye: Vec3,
        timing: crate::render::gpu_timing::PassScope<'_>,
    ) {
        self.recording = false;
        let count = aabbs.len() as u32;
        if count == 0 {
            return;
        }
        self.ensure_capacity(device, count);

        // Every slot still mapping: skip this frame rather than stall.
        let Some(slot_index) = self.free_slot() else {
            return;
        };
        self.current = slot_index;

        let forced_visible: Vec<bool> = aabbs
            .iter()
            .map(|(min, max)| aabb_contains(*min, *max, eye, CONTAINMENT_MARGIN))
            .collect();

        queue.write_buffer(
            &self.view_proj_buffer,
            0,
            bytemuck::bytes_of(&view_proj.to_cols_array_2d()),
        );
        let instances: Vec<BboxInstance> = aabbs
            .iter()
            .map(|(min, max)| BboxInstance {
                aabb_min: min.to_array(),
                aabb_max: max.to_array(),
            })
            .collect();
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("occlusion_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    // Store only preserves the untouched forward depth for SSAO.
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timing.render_writes(0, 1),
                occlusion_query_set: Some(&self.query_set),
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view_proj_bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            // One draw per query, indices dense in 0..count, all in this one pass.
            for i in 0..count {
                pass.begin_occlusion_query(i);
                pass.draw(0..36, i..i + 1);
                pass.end_occlusion_query();
            }
        }

        let slot = &mut self.slots[slot_index];
        slot.count = count;
        slot.forced_visible = forced_visible;
        let bytes = u64::from(count) * QUERY_BYTES;
        encoder.resolve_query_set(&self.query_set, 0..count, &slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, bytes);
        self.recording = true;
    }

    /// Starts this frame's readback and consumes completed slots; never blocks, call after submit.
    pub fn end_frame(&mut self, device: &wgpu::Device) {
        if self.recording {
            let state = Arc::new(AtomicU8::new(MAP_PENDING));
            let callback_state = Arc::clone(&state);
            let bytes = u64::from(self.slots[self.current].count) * QUERY_BYTES;
            self.slots[self.current].readback.slice(..bytes).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let code = if result.is_ok() {
                        MAP_READY
                    } else {
                        MAP_FAILED
                    };
                    callback_state.store(code, Ordering::Release);
                },
            );
            self.slots[self.current].map_state = Some(state);
            self.slots[self.current].generation = self.generation;
            self.current = (self.current + 1) % SLOT_COUNT;
            self.recording = false;
        }

        // Non-blocking: a waiting poll would bring back the stall the ring avoids.
        let _ = device.poll(wgpu::PollType::Poll);

        for index in 0..self.slots.len() {
            let Some(state) = self.slots[index].map_state.as_ref() else {
                continue;
            };
            match state.load(Ordering::Acquire) {
                MAP_READY => {
                    let count = self.slots[index].count as usize;
                    let samples = read_samples(&self.slots[index].readback, count);
                    self.slots[index].readback.unmap();
                    self.slots[index].map_state = None;
                    // A stale generation is drained but must not cull the new scene.
                    if self.slots[index].generation == self.generation {
                        let forced = &self.slots[index].forced_visible;
                        self.visibility = samples
                            .iter()
                            .enumerate()
                            .map(|(i, &s)| s > 0 || forced.get(i).copied().unwrap_or(false))
                            .collect();
                        self.samples = samples;
                    }
                }
                MAP_FAILED => {
                    self.slots[index].map_state = None;
                }
                _ => {}
            }
        }
    }

    /// Latest per-primitive visibility (samples > 0 or eye inside); empty until a readback lands.
    pub fn visibility(&self) -> &[bool] {
        &self.visibility
    }

    /// Raw sample counts behind [`Self::visibility`], for tests and diagnostics.
    pub fn samples(&self) -> &[u64] {
        &self.samples
    }

    /// Whether primitive `i` should be drawn; uncovered indices default to visible, the safe side.
    pub fn visible(&self, i: usize) -> bool {
        self.visibility.get(i).copied().unwrap_or(true)
    }

    /// Forgets all readback state on a scene change, since visibility is per index of the old list.
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.visibility.clear();
        self.samples.clear();
        // The generation already blocks stale masks; clearing means no slot even holds one.
        for slot in &mut self.slots {
            slot.forced_visible.clear();
        }
    }

    /// A slot with no outstanding map, preferring `current` so the ring advances in order.
    fn free_slot(&self) -> Option<usize> {
        (0..SLOT_COUNT)
            .map(|offset| (self.current + offset) % SLOT_COUNT)
            .find(|&candidate| self.slots[candidate].map_state.is_none())
    }

    /// Recreates the query set and buffers for `count` queries, dropping in-flight readbacks.
    fn ensure_capacity(&mut self, device: &wgpu::Device, count: u32) {
        if count <= self.capacity {
            return;
        }
        // Double, so a slowly growing scene does not reallocate every frame.
        let capacity = count.max(self.capacity * 2);
        self.instance_buffer = create_instance_buffer(device, capacity);
        self.query_set = create_query_set(device, capacity);
        self.slots = create_slots(device, capacity);
        self.capacity = capacity;
        self.current = 0;
        self.recording = false;
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("occlusion_instances"),
        size: u64::from(capacity) * std::mem::size_of::<BboxInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_query_set(device: &wgpu::Device, capacity: u32) -> wgpu::QuerySet {
    device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("occlusion_queries"),
        ty: wgpu::QueryType::Occlusion,
        count: capacity,
    })
}

fn create_slots(device: &wgpu::Device, capacity: u32) -> Vec<Slot> {
    let bytes = u64::from(capacity) * QUERY_BYTES;
    (0..SLOT_COUNT)
        .map(|i| Slot {
            resolve: buffer_desc::query_resolve(device, &format!("occlusion_resolve_{i}"), bytes),
            readback: buffer_desc::readback(device, &format!("occlusion_readback_{i}"), bytes),
            map_state: None,
            count: 0,
            generation: 0,
            forced_visible: Vec::new(),
        })
        .collect()
}

/// Reads `count` `u64` sample counts out of a mapped readback buffer.
fn read_samples(buffer: &wgpu::Buffer, count: usize) -> Vec<u64> {
    let bytes = count as u64 * QUERY_BYTES;
    let view = buffer
        .slice(..bytes)
        .get_mapped_range()
        .expect("caller maps the buffer before calling this");
    let samples = view
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().expect("chunks_exact(8) yields 8 bytes")))
        .collect();
    drop(view);
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_contains_point_strictly_inside() {
        assert!(aabb_contains(
            Vec3::splat(-1.0),
            Vec3::splat(1.0),
            Vec3::ZERO,
            CONTAINMENT_MARGIN
        ));
    }

    #[test]
    fn aabb_contains_point_strictly_outside() {
        assert!(!aabb_contains(
            Vec3::splat(-1.0),
            Vec3::splat(1.0),
            Vec3::new(5.0, 0.0, 0.0),
            CONTAINMENT_MARGIN
        ));
    }

    #[test]
    fn aabb_contains_point_exactly_on_a_face() {
        assert!(aabb_contains(
            Vec3::splat(-1.0),
            Vec3::splat(1.0),
            Vec3::new(1.0, 0.0, 0.0),
            CONTAINMENT_MARGIN
        ));
    }

    #[test]
    fn aabb_contains_point_just_outside_but_within_the_margin() {
        // Half-extent 1.0, so margin = 1.0 * 0.02 + 0.01 = 0.03 on every axis.
        let (min, max) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(
            aabb_contains(min, max, Vec3::new(1.02, 0.0, 0.0), CONTAINMENT_MARGIN),
            "0.02 past the face is still inside the 0.03 margin"
        );
        assert!(
            !aabb_contains(min, max, Vec3::new(1.04, 0.0, 0.0), CONTAINMENT_MARGIN),
            "0.04 past the face is outside the 0.03 margin"
        );
    }

    #[test]
    fn aabb_contains_point_degenerate_zero_extent_box() {
        // A relative-only margin vanishes here; the fixed 1 cm floor keeps the point inside.
        let p = Vec3::new(3.0, -2.0, 0.5);
        assert!(
            aabb_contains(p, p, p, CONTAINMENT_MARGIN),
            "the box's own point must be inside itself"
        );
        assert!(
            aabb_contains(p, p, p + Vec3::new(0.005, 0.0, 0.0), CONTAINMENT_MARGIN),
            "within the fixed 1cm floor"
        );
        assert!(
            !aabb_contains(p, p, p + Vec3::new(0.02, 0.0, 0.0), CONTAINMENT_MARGIN),
            "outside the fixed 1cm floor"
        );
    }
}
