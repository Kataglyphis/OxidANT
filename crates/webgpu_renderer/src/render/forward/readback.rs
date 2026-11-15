//! Headless readback of a rendered RGBA8 target into tightly packed rows.

use anyhow::Context as _;

use crate::context::GpuContext;
use crate::render::buffer_desc;

/// Copies `texture` into a mapped buffer and returns its rows without the copy's row padding.
pub(crate) fn read_rgba8(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> anyhow::Result<Vec<u8>> {
    let bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
    let readback = buffer_desc::readback(&gpu.device, "readback", buffer_size);

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("readback_encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    map_packed_rows(gpu, &readback, width, height, bytes_per_row)
}

/// Blocks until `readback` maps, then strips each row's padding.
fn map_packed_rows(
    gpu: &GpuContext,
    readback: &wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
) -> anyhow::Result<Vec<u8>> {
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .context("Device poll failed while mapping readback buffer")?;
    rx.recv()
        .context("Readback mapping callback dropped")?
        .context("Failed to map readback buffer")?;

    // Fallible since wgpu 30; this function returns Result, so propagate rather than panic.
    let data = slice
        .get_mapped_range()
        .context("Failed to view the mapped readback buffer")?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    readback.unmap();
    Ok(pixels)
}
