//! The traced scene on the GPU: six storage buffers, a uniform, two texture arrays and two samplers in one bind group.

use wgpu::util::DeviceExt as _;

use super::layers::LayerArray;
use super::limits::{self, TraceLimitError};
use super::scene::{RecordLayout, TraceScene, TraceStats};
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

/// Uploaded scene data, bound as group [`TRACE_SCENE_GROUP`] of a compute pipeline.
pub struct TraceGpuScene {
    records: wgpu::Buffer,
    node_max: wgpu::Buffer,
    positions: wgpu::Buffer,
    vertices: wgpu::Buffer,
    vertex_extras: wgpu::Buffer,
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

    /// Uploads `scene`, or says which limit of this device it exceeds.
    pub fn upload(gpu: &GpuContext, scene: &TraceScene) -> Result<Self, TraceLimitError> {
        limits::check(scene, &gpu.device.limits())?;
        let device = &gpu.device;
        let layout = scene.layout();
        let queue = &gpu.queue;
        let nodes: &[[u32; 8]] = bytemuck::cast_slice(&scene.nodes);
        let records = buffer_desc::storage_dst(
            device,
            "tracer_records",
            (u64::from(layout.records) * 16).max(16),
        );
        write_half(queue, &records, 0, nodes, 0);
        let materials = u64::from(layout.material_base) * 16;
        queue.write_buffer(&records, materials, bytemuck::cast_slice(&scene.materials));
        let half_buffer = |label: &str, entries: &[[u32; 8]], half: usize| {
            let buffer =
                buffer_desc::storage_dst(device, label, (entries.len() as u64 * 16).max(16));
            write_half(queue, &buffer, 0, entries, half);
            buffer
        };
        let node_max = half_buffer("tracer_node_max", nodes, 1);
        let vertices = half_buffer("tracer_vertices", &scene.vertices, 0);
        let vertex_extras = half_buffer("tracer_vertex_extras", &scene.vertices, 1);
        let storage = |label: &str, bytes: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: if bytes.is_empty() { &[0; 16] } else { bytes },
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        let triangles = storage("tracer_triangles", bytemuck::cast_slice(&scene.triangles));
        let positions = storage("tracer_positions", bytemuck::cast_slice(&scene.positions));
        let info = buffer_desc::uniform(
            device,
            "tracer_info",
            std::mem::size_of::<TraceSceneInfo>() as u64,
        );
        gpu.queue
            .write_buffer(&info, 0, bytemuck::bytes_of(&TraceSceneInfo::of(scene)));
        let srgb = upload_layers(
            gpu,
            &scene.srgb,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "tracer_srgb_layers",
        );
        let linear = upload_layers(
            gpu,
            &scene.linear,
            wgpu::TextureFormat::Rgba8Unorm,
            "tracer_linear_layers",
        );
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
        let buffers = [&records, &triangles, &positions, &vertices, &info];
        let extra = [&node_max, &vertex_extras];
        let bind_group = create_bind_group(device, (buffers, extra), [&srgb, &linear], &samplers);
        let uploaded = Self {
            records,
            node_max,
            positions,
            vertices,
            vertex_extras,
            layout,
            bind_group,
            stats: scene.stats.clone(),
        };
        uploaded.write_instances(gpu, scene);
        Ok(uploaded)
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

/// Float4 slots [`write_half`] deinterleaves per call: 1 MiB.
const WRITE_CHUNK: usize = 1 << 16;

/// Writes one float4 half of each entry, a node's or a packed vertex's, from slot `first` on, a chunk at a time.
fn write_half(
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
    layers: [&wgpu::TextureView; 2],
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

/// One array texture with every layer's mip chain written.
fn upload_layers(
    gpu: &GpuContext,
    array: &LayerArray,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::TextureView {
    let size = array.size.max(1);
    let layers = array.layers.len().max(1) as u32;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        // TEXTURE_2D_SHAPE_OK: an array of material layers, not the single-layer shape of create_2d_texture.
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: array.mip_count(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, levels) in array.layers.iter().enumerate() {
        for (level, data) in levels.iter().enumerate() {
            let side = (size >> level).max(1);
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * side),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
