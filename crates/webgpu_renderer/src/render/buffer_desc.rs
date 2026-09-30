//! `wgpu::BufferDescriptor` constructors, one per usage shape, so label and size stay visible.
//! The two odd shapes (`histogram.rs`'s exposure buffer, `occlusion.rs`'s instances) stay literals.

/// A uniform buffer: `UNIFORM | COPY_DST`.
pub fn uniform(device: &wgpu::Device, label: &str, size: wgpu::BufferAddress) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A host-written storage buffer: `STORAGE | COPY_DST`.
pub fn storage_dst(device: &wgpu::Device, label: &str, size: wgpu::BufferAddress) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A GPU-written storage buffer that is later copied out: `STORAGE | COPY_SRC`.
pub fn storage_src(device: &wgpu::Device, label: &str, size: wgpu::BufferAddress) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// A host-mappable readback buffer: `COPY_DST | MAP_READ`.
pub fn readback(device: &wgpu::Device, label: &str, size: wgpu::BufferAddress) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// A timestamp/occlusion query resolve target: `QUERY_RESOLVE | COPY_SRC`.
pub fn query_resolve(
    device: &wgpu::Device,
    label: &str,
    size: wgpu::BufferAddress,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}
