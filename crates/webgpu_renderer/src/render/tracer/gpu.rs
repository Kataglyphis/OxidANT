//! The traced scene on the GPU: six storage buffers, a uniform, two texture arrays and two samplers in one bind group.

use super::limits::{self, TraceBufferSizes, TraceLimitError};
use super::scene::{RecordLayout, TraceScene, TraceStats};
use super::upload::TraceUpload;
use crate::context::GpuContext;
use crate::render::texture::create_sampler;
use crate::render::{bind_layout, buffer_desc};
use crate::scene::CpuSampler;

/// The bind group index `bvh_traverse.slang` declares the scene at; a kernel keeps group 0 for itself.
pub const TRACE_SCENE_GROUP: u32 = 1;

/// The scene uniform, `TraceSceneInfo` in `bvh_traverse.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TraceSceneInfo {
    pub tlas_root: u32,
    pub instance_base: u32,
    pub material_base: u32,
    pub instance_count: u32,
    pub material_count: u32,
    pub srgb_layer_size: u32,
    pub linear_layer_size: u32,
    pub flags: u32,
}

impl TraceSceneInfo {
    pub fn of(scene: &TraceScene) -> Self {
        let layout = scene.layout();
        Self {
            tlas_root: layout.tlas_root,
            instance_base: layout.instance_base,
            material_base: layout.material_base,
            instance_count: scene.instances.len() as u32,
            material_count: scene.materials.len() as u32,
            srgb_layer_size: scene.srgb.size,
            linear_layer_size: scene.linear.size,
            flags: 0,
        }
    }
}

/// The scene's buffers at their full sizes, made empty and filled by [`TraceUpload`].
pub(super) struct SceneBuffers {
    pub(super) records: wgpu::Buffer,
    pub(super) node_max: wgpu::Buffer,
    pub(super) triangles: wgpu::Buffer,
    pub(super) positions: wgpu::Buffer,
    pub(super) vertices: wgpu::Buffer,
    pub(super) vertex_extras: wgpu::Buffer,
    pub(super) info: wgpu::Buffer,
}

impl SceneBuffers {
    pub(super) fn allocate(device: &wgpu::Device, scene: &TraceScene) -> Self {
        let sizes = TraceBufferSizes::of(scene);
        let storage =
            |label: &str, bytes: u64| buffer_desc::storage_dst(device, label, bytes.max(16));
        Self {
            records: storage("tracer_records", sizes.records),
            node_max: storage("tracer_node_max", sizes.node_max),
            triangles: storage("tracer_triangles", sizes.triangles),
            positions: storage("tracer_positions", sizes.positions),
            vertices: storage("tracer_vertices", sizes.vertices),
            vertex_extras: storage("tracer_vertex_extras", sizes.vertex_extras),
            info: buffer_desc::uniform(
                device,
                "tracer_info",
                std::mem::size_of::<TraceSceneInfo>() as u64,
            ),
        }
    }
}

/// Uploaded scene data, bound as group [`TRACE_SCENE_GROUP`] of a compute pipeline.
pub struct TraceGpuScene {
    records: wgpu::Buffer,
    node_max: wgpu::Buffer,
    triangles: wgpu::Buffer,
    positions: wgpu::Buffer,
    vertices: wgpu::Buffer,
    vertex_extras: wgpu::Buffer,
    info: wgpu::Buffer,
    layers: [wgpu::TextureView; 2],
    samplers: [wgpu::Sampler; 2],
    layout: RecordLayout,
    bind_group: wgpu::BindGroup,
    pub stats: TraceStats,
}

impl TraceGpuScene {
    /// The layout of the scene's group; pipelines may build theirs before any scene exists, as equal layouts are compatible.
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        let compute = wgpu::ShaderStages::COMPUTE;
        let layers = |binding| {
            bind_layout::texture(
                binding,
                compute,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::D2Array,
                false,
            )
        };
        let filtering = wgpu::SamplerBindingType::Filtering;
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tracer_scene_layout"),
            entries: &[
                bind_layout::storage_buffer(0, compute, true),
                bind_layout::storage_buffer(1, compute, true),
                bind_layout::storage_buffer(2, compute, true),
                bind_layout::storage_buffer(3, compute, true),
                bind_layout::uniform(4, compute),
                layers(5),
                layers(6),
                bind_layout::sampler(7, compute, filtering),
                bind_layout::sampler(8, compute, filtering),
                bind_layout::storage_buffer(9, compute, true),
                bind_layout::storage_buffer(10, compute, true),
            ],
        })
    }

    /// Uploads `scene` in one go, or says which limit of this device it exceeds; [`TraceUpload`] spreads it over frames.
    pub fn upload(gpu: &GpuContext, scene: &TraceScene) -> Result<Self, TraceLimitError> {
        let mut upload = TraceUpload::new(gpu, scene)?;
        upload.step(gpu, scene, u64::MAX);
        Ok(upload.finish(gpu, scene))
    }

    /// The scene over buffers [`TraceUpload`] filled, with its top level written now.
    pub(super) fn from_buffers(
        gpu: &GpuContext,
        scene: &TraceScene,
        buffers: SceneBuffers,
        layers: [wgpu::TextureView; 2],
    ) -> Self {
        let device = &gpu.device;
        let samplers = [false, true].map(|nearest| {
            create_sampler(
                device,
                &CpuSampler {
                    mag_nearest: nearest,
                    min_nearest: nearest,
                    mip_nearest: nearest,
                    ..CpuSampler::default()
                },
            )
        });
        let SceneBuffers {
            records,
            node_max,
            triangles,
            positions,
            vertices,
            vertex_extras,
            info,
        } = buffers;
        let main = [&records, &triangles, &positions, &vertices, &info];
        let bind_group = create_bind_group(
            device,
            (main, [&node_max, &vertex_extras]),
            &layers,
            &samplers,
        );
        let uploaded = Self {
            records,
            node_max,
            triangles,
            positions,
            vertices,
            vertex_extras,
            info,
            layers,
            samplers,
            layout: scene.layout(),
            bind_group,
            stats: scene.stats.clone(),
        };
        uploaded.write_top_level(gpu, scene);
        uploaded
    }

    /// Rebuilds the records after a new instance count ([`super::scene::InstanceChange::Recounted`]); the vertex buffers stay.
    pub fn write_records(
        &mut self,
        gpu: &GpuContext,
        scene: &TraceScene,
    ) -> Result<(), TraceLimitError> {
        limits::check_buffers(&TraceBufferSizes::of(scene), &gpu.device.limits())?;
        [self.records, self.node_max] = record_buffers(gpu, scene);
        let buffers = [
            &self.records,
            &self.triangles,
            &self.positions,
            &self.vertices,
            &self.info,
        ];
        let extra = [&self.node_max, &self.vertex_extras];
        self.bind_group =
            create_bind_group(&gpu.device, (buffers, extra), &self.layers, &self.samplers);
        self.layout = scene.layout();
        self.stats = scene.stats.clone();
        self.write_top_level(gpu, scene);
        Ok(())
    }

    /// The top level, the instances and the uniform that counts them, at a new layout.
    fn write_top_level(&self, gpu: &GpuContext, scene: &TraceScene) {
        gpu.queue.write_buffer(
            &self.info,
            0,
            bytemuck::bytes_of(&TraceSceneInfo::of(scene)),
        );
        self.write_instances(gpu, scene);
    }

    /// The record layout the buffers hold.
    pub fn layout(&self) -> RecordLayout {
        self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Rewrites the top level and the instances after [`TraceScene::set_root_transform`]; the layout cannot change.
    pub fn write_instances(&self, gpu: &GpuContext, scene: &TraceScene) {
        debug_assert_eq!(
            scene.layout(),
            self.layout,
            "a top-level rebuild keeps every section in place"
        );
        self.write_nodes(
            gpu,
            scene,
            scene.blas_node_count as usize..scene.nodes.len(),
        );
        let instances = u64::from(self.layout.instance_base) * 16;
        gpu.queue.write_buffer(
            &self.records,
            instances,
            bytemuck::cast_slice(&scene.instances),
        );
    }

    /// Rewrites `meshes`' vertices and nodes after [`TraceScene::pose_deformed`], and the top level over them.
    pub fn write_meshes(&self, gpu: &GpuContext, scene: &TraceScene, meshes: &[u32]) {
        for mesh in meshes.iter().map(|&m| &scene.meshes[m as usize]) {
            let first = mesh.first_vertex as usize;
            let vertices = first..first + mesh.vertex_count as usize;
            gpu.queue.write_buffer(
                &self.positions,
                first as u64 * 16,
                bytemuck::cast_slice(&scene.positions[vertices.clone()]),
            );
            write_half(
                &gpu.queue,
                &self.vertices,
                first,
                &scene.vertices[vertices.clone()],
                0,
            );
            write_half(
                &gpu.queue,
                &self.vertex_extras,
                first,
                &scene.vertices[vertices],
                1,
            );
            let root = mesh.root as usize;
            self.write_nodes(gpu, scene, root..root + mesh.node_count as usize);
        }
        self.write_instances(gpu, scene);
    }

    /// Writes both halves of `range`'s nodes.
    fn write_nodes(&self, gpu: &GpuContext, scene: &TraceScene, range: std::ops::Range<usize>) {
        let nodes: &[[u32; 8]] = bytemuck::cast_slice(&scene.nodes[range.clone()]);
        write_half(&gpu.queue, &self.records, range.start, nodes, 0);
        write_half(&gpu.queue, &self.node_max, range.start, nodes, 1);
    }
}

/// The records and node-maxima buffers at `scene`'s layout, with the mesh nodes and the materials; the top level follows.
fn record_buffers(gpu: &GpuContext, scene: &TraceScene) -> [wgpu::Buffer; 2] {
    let layout = scene.layout();
    let meshes: &[[u32; 8]] = bytemuck::cast_slice(&scene.nodes[..scene.blas_node_count as usize]);
    let records = buffer_desc::storage_dst(
        &gpu.device,
        "tracer_records",
        (u64::from(layout.records) * 16).max(16),
    );
    write_half(&gpu.queue, &records, 0, meshes, 0);
    let materials = u64::from(layout.material_base) * 16;
    gpu.queue
        .write_buffer(&records, materials, bytemuck::cast_slice(&scene.materials));
    let node_max = buffer_desc::storage_dst(
        &gpu.device,
        "tracer_node_max",
        (scene.nodes.len() as u64 * 16).max(16),
    );
    write_half(&gpu.queue, &node_max, 0, meshes, 1);
    [records, node_max]
}

/// Float4 slots [`write_half`] deinterleaves per call: 1 MiB.
pub(super) const WRITE_CHUNK: usize = 1 << 16;

/// Writes one float4 half of each entry, a node's or a packed vertex's, from slot `first` on, a chunk at a time.
pub(super) fn write_half(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    first: usize,
    entries: &[[u32; 8]],
    half: usize,
) {
    for (index, chunk) in entries.chunks(WRITE_CHUNK).enumerate() {
        let slots: Vec<[u32; 4]> = chunk
            .iter()
            .map(|entry| entry.as_chunks::<4>().0[half])
            .collect();
        let offset = (first + index * WRITE_CHUNK) as u64 * 16;
        queue.write_buffer(buffer, offset, bytemuck::cast_slice(&slots));
    }
}

/// Bindings 0-4 are `buffers`, 9 and 10 `extra`: the node maxima and the vertex extras.
fn create_bind_group(
    device: &wgpu::Device,
    (buffers, extra): ([&wgpu::Buffer; 5], [&wgpu::Buffer; 2]),
    layers: &[wgpu::TextureView; 2],
    samplers: &[wgpu::Sampler; 2],
) -> wgpu::BindGroup {
    let bindings = (0..5).chain(9..11);
    let mut entries: Vec<wgpu::BindGroupEntry> = bindings
        .zip(buffers.iter().chain(&extra))
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    for (offset, view) in layers.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: 5 + offset as u32,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    for (offset, sampler) in samplers.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: 7 + offset as u32,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tracer_scene"),
        layout: &TraceGpuScene::bind_group_layout(device),
        entries: &entries,
    })
}
