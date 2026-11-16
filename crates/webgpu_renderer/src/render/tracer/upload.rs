//! The traced scene's first upload, spread over frames: every buffer and texture made at once, then filled a budget at a time.

use super::gpu::{write_half, SceneBuffers, TraceGpuScene, WRITE_CHUNK};
use super::layers::LayerArray;
use super::limits::{self, TraceLimitError};
use super::scene::TraceScene;
use crate::context::GpuContext;

/// Bytes a frame uploads by default: the full Colosseum's half gigabyte over about eight frames.
pub const UPLOAD_BYTES_PER_FRAME: u64 = 64 << 20;

/// One write of the upload, at most [`WRITE_CHUNK`] entries.
#[derive(Clone, Copy, Debug)]
enum Piece {
    /// Float4 half `half` of the mesh nodes `first..first + count`: the records' (0) or the node maxima's (1).
    MeshNodes {
        half: usize,
        first: usize,
        count: usize,
    },
    Materials,
    Triangles {
        first: usize,
        count: usize,
    },
    Positions {
        first: usize,
        count: usize,
    },
    /// Float4 half `half` of the packed vertices `first..first + count`: the vertices (0) or their extras (1).
    Vertices {
        half: usize,
        first: usize,
        count: usize,
    },
    /// One mip level of one layer of the sRGB (0) or the linear (1) array.
    Level {
        array: usize,
        layer: usize,
        level: usize,
    },
}

/// An upload in flight: the GPU objects at their full sizes and the pieces not yet written.
pub struct TraceUpload {
    buffers: SceneBuffers,
    layers: [wgpu::Texture; 2],
    pieces: Vec<(Piece, u64)>,
    next: usize,
    written: u64,
    total: u64,
}

impl TraceUpload {
    /// Makes every buffer and texture `scene` needs, or says which limit of this device it exceeds.
    pub fn new(gpu: &GpuContext, scene: &TraceScene) -> Result<Self, TraceLimitError> {
        limits::check(scene, &gpu.device.limits())?;
        let device = &gpu.device;
        let layers = [
            (
                &scene.srgb,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                "tracer_srgb_layers",
            ),
            (
                &scene.linear,
                wgpu::TextureFormat::Rgba8Unorm,
                "tracer_linear_layers",
            ),
        ]
        .map(|(array, format, label)| create_layers(device, array, format, label));
        let pieces = pieces_of(scene);
        let total = pieces.iter().map(|(_, bytes)| bytes).sum();
        Ok(Self {
            buffers: SceneBuffers::allocate(device, scene),
            layers,
            pieces,
            next: 0,
            written: 0,
            total,
        })
    }

    /// Writes pieces of `scene` up to `budget` bytes, at least one; true once every piece is written.
    pub fn step(&mut self, gpu: &GpuContext, scene: &TraceScene, budget: u64) -> bool {
        let mut spent = 0u64;
        while let Some(&(piece, bytes)) = self.pieces.get(self.next) {
            if spent > 0 && spent.saturating_add(bytes) > budget {
                break;
            }
            self.write(&gpu.queue, scene, piece);
            spent += bytes;
            self.written += bytes;
            self.next += 1;
        }
        self.is_done()
    }

    pub fn is_done(&self) -> bool {
        self.next == self.pieces.len()
    }

    /// Bytes written over all bytes, 1 for an empty scene.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (self.written as f64 / self.total as f64) as f32
        }
    }

    /// The uploaded scene, its top level written now; call once [`Self::step`] returned true.
    pub fn finish(self, gpu: &GpuContext, scene: &TraceScene) -> TraceGpuScene {
        debug_assert!(self.is_done(), "a scene finished before its upload");
        let views = self.layers.map(|texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        });
        TraceGpuScene::from_buffers(gpu, scene, self.buffers, views)
    }

    fn write(&self, queue: &wgpu::Queue, scene: &TraceScene, piece: Piece) {
        let b = &self.buffers;
        let halves = |half: usize, first, second| if half == 0 { first } else { second };
        match piece {
            Piece::MeshNodes { half, first, count } => {
                let nodes: &[[u32; 8]] = bytemuck::cast_slice(&scene.nodes[first..first + count]);
                write_half(
                    queue,
                    halves(half, &b.records, &b.node_max),
                    first,
                    nodes,
                    half,
                );
            }
            Piece::Materials => {
                let offset = u64::from(scene.layout().material_base) * 16;
                queue.write_buffer(&b.records, offset, bytemuck::cast_slice(&scene.materials));
            }
            Piece::Triangles { first, count } => {
                let data = bytemuck::cast_slice(&scene.triangles[first..first + count]);
                queue.write_buffer(&b.triangles, first as u64 * 16, data);
            }
            Piece::Positions { first, count } => {
                let data = bytemuck::cast_slice(&scene.positions[first..first + count]);
                queue.write_buffer(&b.positions, first as u64 * 16, data);
            }
            Piece::Vertices { half, first, count } => {
                let target = halves(half, &b.vertices, &b.vertex_extras);
                write_half(
                    queue,
                    target,
                    first,
                    &scene.vertices[first..first + count],
                    half,
                );
            }
            Piece::Level {
                array,
                layer,
                level,
            } => {
                let source = [&scene.srgb, &scene.linear][array];
                write_level(queue, &self.layers[array], source, layer, level);
            }
        }
    }
}

/// Every write `scene` needs with its bytes, in the order the upload goes.
fn pieces_of(scene: &TraceScene) -> Vec<(Piece, u64)> {
    let chunks = |len: usize| {
        (0..len)
            .step_by(WRITE_CHUNK)
            .map(move |first| (first, WRITE_CHUNK.min(len - first)))
    };
    let mut pieces = Vec::new();
    for half in 0..2 {
        for (first, count) in chunks(scene.blas_node_count as usize) {
            pieces.push((Piece::MeshNodes { half, first, count }, count as u64 * 16));
        }
    }
    if !scene.materials.is_empty() {
        let bytes = std::mem::size_of_val(scene.materials.as_slice()) as u64;
        pieces.push((Piece::Materials, bytes));
    }
    for (first, count) in chunks(scene.triangles.len()) {
        pieces.push((Piece::Triangles { first, count }, count as u64 * 16));
    }
    for (first, count) in chunks(scene.positions.len()) {
        pieces.push((Piece::Positions { first, count }, count as u64 * 16));
    }
    for half in 0..2 {
        for (first, count) in chunks(scene.vertices.len()) {
            pieces.push((Piece::Vertices { half, first, count }, count as u64 * 16));
        }
    }
    for (array, source) in [&scene.srgb, &scene.linear].into_iter().enumerate() {
        for (layer, levels) in source.layers.iter().enumerate() {
            for (level, data) in levels.iter().enumerate() {
                pieces.push((
                    Piece::Level {
                        array,
                        layer,
                        level,
                    },
                    data.len() as u64,
                ));
            }
        }
    }
    pieces
}

/// One array texture of `array`'s size, layers and mips, written by [`write_level`].
fn create_layers(
    device: &wgpu::Device,
    array: &LayerArray,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    let size = array.size.max(1);
    device.create_texture(&wgpu::TextureDescriptor {
        // TEXTURE_2D_SHAPE_OK: an array of material layers, not the single-layer shape of create_2d_texture.
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: array.layers.len().max(1) as u32,
        },
        mip_level_count: array.mip_count(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_level(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    array: &LayerArray,
    layer: usize,
    level: usize,
) {
    let side = (array.size.max(1) >> level).max(1);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level as u32,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer as u32,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &array.layers[layer][level],
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
