//! A compute pass that traces a buffer of rays through `trace_debug.wgsl` and reads the hits back: the GPU side of the tests.

use anyhow::Context as _;
use glam::{Vec2, Vec3};
use wgpu::util::DeviceExt as _;

use super::gpu::{TraceGpuScene, TRACE_SCENE_GROUP};
use super::query::{TraceRay, NO_HIT};
use crate::context::GpuContext;
use crate::render::{bind_layout, buffer_desc};

const WORKGROUP: u32 = 64;
/// Float4 records `cs_trace_closest` writes per ray.
const HIT_RECORDS: usize = 4;

/// What `cs_trace_closest` reports for one ray: the hit, and the surface and material it fetched there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugHit {
    pub t: f32,
    /// [`NO_HIT`] for a miss, when the rest is zero.
    pub instance: u32,
    pub triangle: u32,
    pub barycentrics: Vec2,
    pub material: u32,
    pub front_face: bool,
    /// The world tangent frame's bitangent sign.
    pub handedness: f32,
    pub normal: Vec3,
    pub vertex_alpha: f32,
    pub uv0: Vec2,
    /// The base colour slot's red and alpha at level 0.
    pub base_red: f32,
    pub base_alpha: f32,
}

impl DebugHit {
    fn from_records(r: &[[f32; 4]]) -> Self {
        Self {
            t: r[0][0],
            instance: r[0][1].to_bits(),
            triangle: r[0][2].to_bits(),
            barycentrics: Vec2::new(r[0][3], r[1][0]),
            material: r[1][1].to_bits(),
            front_face: r[1][2] > 0.5,
            handedness: r[1][3],
            normal: Vec3::new(r[2][0], r[2][1], r[2][2]),
            vertex_alpha: r[2][3],
            uv0: Vec2::new(r[3][0], r[3][1]),
            base_red: r[3][2],
            base_alpha: r[3][3],
        }
    }

    pub fn is_hit(&self) -> bool {
        self.instance != NO_HIT
    }
}

/// The two debug kernels and their own bind group layout (group 0); the scene is group 1.
pub struct TraceDebugPass {
    closest: wgpu::ComputePipeline,
    any: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl TraceDebugPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("trace_debug_layout"),
            entries: &[
                bind_layout::uniform(0, wgpu::ShaderStages::COMPUTE),
                bind_layout::storage_buffer(1, wgpu::ShaderStages::COMPUTE, true),
                bind_layout::storage_buffer(2, wgpu::ShaderStages::COMPUTE, false),
            ],
        });
        let scene_layout = TraceGpuScene::bind_group_layout(device);
        debug_assert_eq!(
            TRACE_SCENE_GROUP, 1,
            "the pipeline layout lists the scene second"
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("trace_debug_pipeline_layout"),
            bind_group_layouts: &[Some(&layout), Some(&scene_layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("trace_debug"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/trace_debug.wgsl").into()),
        });
        let pipeline = |entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        Self {
            closest: pipeline("cs_trace_closest"),
            any: pipeline("cs_trace_any"),
            layout,
        }
    }

    /// The closest hit of each ray, with its surface.
    pub fn trace_closest(
        &self,
        gpu: &GpuContext,
        scene: &TraceGpuScene,
        rays: &[TraceRay],
    ) -> anyhow::Result<Vec<DebugHit>> {
        let records = self.run(gpu, scene, &self.closest, rays)?;
        Ok(records
            .as_chunks::<HIT_RECORDS>()
            .0
            .iter()
            .map(|r| DebugHit::from_records(r))
            .collect())
    }

    /// Whether anything blocks each ray.
    pub fn trace_any(
        &self,
        gpu: &GpuContext,
        scene: &TraceGpuScene,
        rays: &[TraceRay],
    ) -> anyhow::Result<Vec<bool>> {
        let records = self.run(gpu, scene, &self.any, rays)?;
        Ok(records
            .as_chunks::<HIT_RECORDS>()
            .0
            .iter()
            .map(|r| r[0][0] > 0.5)
            .collect())
    }

    fn run(
        &self,
        gpu: &GpuContext,
        scene: &TraceGpuScene,
        pipeline: &wgpu::ComputePipeline,
        rays: &[TraceRay],
    ) -> anyhow::Result<Vec<[f32; 4]>> {
        let device = &gpu.device;
        let packed: Vec<[f32; 4]> = rays
            .iter()
            .flat_map(|r| {
                [
                    r.origin.extend(r.tmin).to_array(),
                    r.direction.extend(r.tmax).to_array(),
                ]
            })
            .collect();
        let count = rays.len().max(1);
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("trace_debug_params"),
            contents: bytemuck::cast_slice(&[rays.len() as u32, 0, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("trace_debug_rays"),
            contents: bytemuck::cast_slice(if packed.is_empty() {
                &[[0.0; 4]; 2]
            } else {
                &packed[..]
            }),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bytes = (count * HIT_RECORDS * 16) as u64;
        let output = buffer_desc::storage_src(device, "trace_debug_hits", bytes);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("trace_debug_group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let readback = buffer_desc::readback(device, "trace_debug_readback", bytes);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("trace_debug"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_bind_group(TRACE_SCENE_GROUP, scene.bind_group(), &[]);
            pass.dispatch_workgroups((count as u32).div_ceil(WORKGROUP), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        gpu.queue.submit(Some(encoder.finish()));
        let data = read_mapped(gpu, &readback)?;
        Ok(bytemuck::cast_slice::<u8, [f32; 4]>(&data)[..rays.len() * HIT_RECORDS].to_vec())
    }
}

/// Blocks until `buffer` maps and copies it out.
fn read_mapped(gpu: &GpuContext, buffer: &wgpu::Buffer) -> anyhow::Result<Vec<u8>> {
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .context("device poll failed while tracing")?;
    receiver
        .recv()
        .context("the map callback was dropped")?
        .context("could not map the hit buffer")?;
    let data = slice
        .get_mapped_range()
        .context("could not view the hit buffer")?
        .to_vec();
    buffer.unmap();
    Ok(data)
}
