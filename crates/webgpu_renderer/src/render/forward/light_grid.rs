//! Group 3 of the forward pipelines: the punctual lights and the per-tile light grid over them.

use super::frame::FrameView;
use super::{ForwardRenderer, MAX_PUNCTUAL_LIGHTS};
use crate::context::GpuContext;
use crate::render::bind_layout;
use crate::render::buffer_desc;
use crate::render::tile_grid::{build_tile_light_grid, TileLightGridScratch, MAX_LIGHTS_PER_TILE};

pub(super) struct LightResources {
    /// Punctual lights (up to `MAX_PUNCTUAL_LIGHTS`), group 3 binding 0 of the forward shader.
    storage_buffer: wgpu::Buffer,
    /// Group 3: the lights and the tile grid below, rebuilt when a tile buffer grows.
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) bind_group: wgpu::BindGroup,
    /// Tile-based light grid: offsets and counts for each screen tile (group 3 bindings 1 and 2).
    tile_grid_buffer: wgpu::Buffer,
    tile_indices_buffer: wgpu::Buffer,
    pub(super) tile_counts: (u32, u32),
    /// Reusable scratch for [`build_tile_light_grid`] — see its doc comment.
    scratch: TileLightGridScratch,
    pub(super) punctual: [[f32; 4]; MAX_PUNCTUAL_LIGHTS * 4],
    pub(super) punctual_count: u32,
}

impl LightResources {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        // Group 3, forward pipelines only: lights (0), per-tile offset + count (1), tile light indices (2).
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light_bind_group_layout"),
            entries: &[
                bind_layout::storage_buffer(0, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::storage_buffer(1, wgpu::ShaderStages::FRAGMENT, true),
                bind_layout::storage_buffer(2, wgpu::ShaderStages::FRAGMENT, true),
            ],
        });
        let storage_buffer = buffer_desc::storage_dst(
            device,
            "light_storage",
            (MAX_PUNCTUAL_LIGHTS as u64) * 16 * 4, // 4 vec4 per light
        );
        // Initial sizes: will be rebuilt on first render if larger needed.
        let max_tiles = 192 * 128; // enough for 3072×2048 ÷ 16²
        let tile_grid_buffer = buffer_desc::storage_dst(
            device,
            "tile_light_grid",
            (max_tiles as u64) * 8, // vec2<u32> per tile
        );
        let tile_indices_buffer = buffer_desc::storage_dst(
            device,
            "tile_light_indices",
            (max_tiles as u64) * (MAX_LIGHTS_PER_TILE as u64) * 4,
        );
        let bind_group = create_light_bind_group(
            device,
            &layout,
            &storage_buffer,
            &tile_grid_buffer,
            &tile_indices_buffer,
        );
        Self {
            storage_buffer,
            layout,
            bind_group,
            tile_grid_buffer,
            tile_indices_buffer,
            tile_counts: (0, 0),
            scratch: TileLightGridScratch::default(),
            punctual: [[0.0; 4]; MAX_PUNCTUAL_LIGHTS * 4],
            punctual_count: 0,
        }
    }

    /// A bind group captures buffer identity, so rebuild it if either buffer is recreated.
    fn grow_tile_buffers(&mut self, device: &wgpu::Device) {
        let total_tiles = (self.tile_counts.0 * self.tile_counts.1) as usize;
        let mut bind_group_dirty = false;
        let needed_grid = (total_tiles as u64) * 8; // vec2<u32> × total_tiles
        if needed_grid > self.tile_grid_buffer.size() {
            self.tile_grid_buffer =
                buffer_desc::storage_dst(device, "tile_light_grid", needed_grid);
            bind_group_dirty = true;
        }
        let needed_idx = (self.scratch.indices.len() as u64) * 4;
        if needed_idx > self.tile_indices_buffer.size() {
            self.tile_indices_buffer =
                buffer_desc::storage_dst(device, "tile_light_indices", needed_idx.max(64));
            bind_group_dirty = true;
        }
        if bind_group_dirty {
            self.bind_group = create_light_bind_group(
                device,
                &self.layout,
                &self.storage_buffer,
                &self.tile_grid_buffer,
                &self.tile_indices_buffer,
            );
        }
    }
}

impl ForwardRenderer {
    /// Uploads the punctual lights, then bins them into this frame's tiles and uploads the grid.
    pub(super) fn write_light_grid(&mut self, gpu: &GpuContext, view: &FrameView) {
        let lights = &mut self.lights;
        // Write punctual lights to storage buffer (group 3, forward-only).
        gpu.queue.write_buffer(
            &lights.storage_buffer,
            0,
            bytemuck::cast_slice(&lights.punctual[..]),
        );
        build_tile_light_grid(
            &mut lights.scratch,
            &lights.punctual,
            lights.punctual_count,
            view.width,
            view.height,
            &view.view_proj,
        );
        lights.grow_tile_buffers(&gpu.device);
        gpu.queue.write_buffer(
            &lights.tile_grid_buffer,
            0,
            bytemuck::cast_slice(&lights.scratch.grid),
        );
        gpu.queue.write_buffer(
            &lights.tile_indices_buffer,
            0,
            bytemuck::cast_slice(&lights.scratch.indices),
        );
    }
}

fn create_light_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    lights: &wgpu::Buffer,
    tile_grid: &wgpu::Buffer,
    tile_indices: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("light_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: lights.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: tile_grid.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: tile_indices.as_entire_binding(),
            },
        ],
    })
}
