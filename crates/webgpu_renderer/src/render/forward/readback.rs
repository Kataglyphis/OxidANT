//! Headless readback of rendered targets into tightly packed rows.

use anyhow::Context as _;

use crate::context::GpuContext;
use crate::render::buffer_desc;

/// The part of a texture [`read_texture`] copies out: one aspect of `layers` whole layers.
pub(super) struct TextureRegion {
    pub(super) aspect: wgpu::TextureAspect,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) layers: u32,
    /// Bytes per texel of `aspect`; the copy's row pitch is derived from it.
    pub(super) texel_bytes: u32,
}

/// Copies an RGBA8 `texture` into a mapped buffer and returns its rows without the row padding.
pub(super) fn read_rgba8(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> anyhow::Result<Vec<u8>> {
    read_texture(
        gpu,
        texture,
        &TextureRegion {
            aspect: wgpu::TextureAspect::All,
            width,
            height,
            layers: 1,
            texel_bytes: 4,
        },
    )
}

/// Copies `region` of `texture` out, layer-major then row-major, without the row padding.
pub(super) fn read_texture(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    region: &TextureRegion,
) -> anyhow::Result<Vec<u8>> {
    let row_bytes = region.width * region.texel_bytes;
    let bytes_per_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let rows = region.height * region.layers;
    let buffer_size = u64::from(bytes_per_row) * u64::from(rows);
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
            aspect: region.aspect,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(region.height),
            },
        },
        wgpu::Extent3d {
            width: region.width,
            height: region.height,
            depth_or_array_layers: region.layers,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    map_packed_rows(gpu, &readback, row_bytes, rows, bytes_per_row)
}

/// Blocks until `readback` maps, then strips each row's padding.
fn map_packed_rows(
    gpu: &GpuContext,
    readback: &wgpu::Buffer,
    row_bytes: u32,
    rows: u32,
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
    let mut pixels = Vec::with_capacity(row_bytes as usize * rows as usize);
    for row in 0..rows as usize {
        let start = row * bytes_per_row as usize;
        pixels.extend_from_slice(&data[start..start + row_bytes as usize]);
    }
    drop(data);
    readback.unmap();
    Ok(pixels)
}
