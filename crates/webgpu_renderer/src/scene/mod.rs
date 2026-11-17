//! CPU-side scene produced by the asset loaders and consumed by the render passes.

pub mod camera;
pub mod controller;
pub mod fly;
pub mod lod;
pub mod qem;
pub mod viewport;

// The CPU types live in kataglyphis_assets, which the C++ engine links without wgpu.
pub use kataglyphis_assets::scene::*;

/// A type's wgpu vertex-buffer layout; a trait because `Vertex` is kataglyphis_assets', which has no wgpu.
pub trait VertexLayout {
    const LAYOUT: wgpu::VertexBufferLayout<'static>;
}

impl VertexLayout for Vertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        // Locations are forward.wgsl's; entries stay in field order, which sets the offsets.
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, 4 => Float32x3, 5 => Float32x2, 6 => Float32x4,
            7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 1 => Float32x2
        ],
    };
}

/// A per-instance transform as four vec4 columns, since mat4 has no vertex-attribute format.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
}

impl InstanceRaw {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
        // The whole point: advance once per INSTANCE, not per vertex.
        step_mode: wgpu::VertexStepMode::Instance,
        // forward.wgsl places the four columns at locations 10, 11, 2, 3.
        attributes: &wgpu::vertex_attr_array![10 => Float32x4, 11 => Float32x4, 2 => Float32x4, 3 => Float32x4],
    };

    pub const IDENTITY: Self = Self {
        model: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };
}
