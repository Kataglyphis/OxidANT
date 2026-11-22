//! CPU binning of punctual lights into the screen tiles their `range` covers, for tiled lighting.
//! An empty grid (`tileW == 0`) is the shader's all-lights fallback.

use glam::{DVec2, Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};

use crate::render::forward::MAX_PUNCTUAL_LIGHTS;

/// Tile edge in pixels; each fragment iterates only its tile's lights.
pub const TILE_SIZE: u32 = 16;
/// Most lights one tile lists; a tile more lights overlap lists none, and the shader takes them all.
pub const MAX_LIGHTS_PER_TILE: u32 = 32;

/// An inclusive tile rect `(min_tx, min_ty, max_tx, max_ty)`.
type TileRect = (u32, u32, u32, u32);

/// Scratch for [`build_tile_light_grid`], kept on `ForwardRenderer` to avoid per-frame allocations.
#[derive(Default)]
pub struct TileLightGridScratch {
    pub(crate) per_tile_counts: Vec<u32>,
    pub(crate) grid: Vec<u32>,
    pub(crate) indices: Vec<u32>,
    pub(crate) write_positions: Vec<u32>,
    /// Each light's tile rect, `None` when it reaches no tile.
    rects: Vec<Option<TileRect>>,
}

/// Tiles across and down for a `width` x `height` target.
#[derive(Copy, Clone)]
struct TileGrid {
    tiles_x: u32,
    tiles_y: u32,
    width: u32,
    height: u32,
}

impl TileGrid {
    fn total(self) -> usize {
        (self.tiles_x * self.tiles_y) as usize
    }

    fn whole(self) -> TileRect {
        (0, 0, self.tiles_x - 1, self.tiles_y - 1)
    }

    /// Flat tile indices inside `rect`.
    fn tiles_in(self, (min_tx, min_ty, max_tx, max_ty): TileRect) -> impl Iterator<Item = usize> {
        (min_ty..=max_ty)
            .flat_map(move |ty| (min_tx..=max_tx).map(move |tx| (ty * self.tiles_x + tx) as usize))
    }

    /// The tiles an NDC rect covers, or `None` when it lies wholly off screen.
    fn ndc_rect(self, min: Vec2, max: Vec2) -> Option<TileRect> {
        let (w, h) = (self.width as f32, self.height as f32);
        // Framebuffer pixels, y down; the pixel of padding absorbs rounding, a sample off the pixel centre included.
        let (px0, px1) = ((min.x * 0.5 + 0.5) * w - 1.0, (max.x * 0.5 + 0.5) * w + 1.0);
        let (py0, py1) = ((0.5 - max.y * 0.5) * h - 1.0, (0.5 - min.y * 0.5) * h + 1.0);
        if px1 < 0.0 || py1 < 0.0 || px0 >= w || py0 >= h {
            return None;
        }
        let tile = |p: f32, extent: u32| (p.clamp(0.0, (extent - 1) as f32) as u32) / TILE_SIZE;
        Some((
            tile(px0, self.width).min(self.tiles_x - 1),
            tile(py0, self.height).min(self.tiles_y - 1),
            tile(px1, self.width).min(self.tiles_x - 1),
            tile(py1, self.height).min(self.tiles_y - 1),
        ))
    }
}

/// Where a light's sphere lands on screen.
enum Footprint {
    /// Wholly behind the eye.
    Hidden,
    /// Reaching the eye plane, or not boundable by this projection.
    Unbounded,
    /// The NDC rect `min..max` bounding its silhouette.
    Ndc(Vec2, Vec2),
}

/// The slopes `x / depth` of the two tangents from the eye to a circle at lateral `c`, `depth` and radius `r`.
fn tangent_slopes(c: f64, depth: f64, r: f64) -> (f64, f64) {
    // Roots of s²(d² - r²) - 2cds + c² - r² = 0, the second from the product so neither cancels.
    let a = depth * depth - r * r;
    let q = c * depth + c.signum() * r * (c * c + a).sqrt();
    let (s0, s1) = (q / a, (c * c - r * r) / q);
    (s0.min(s1), s0.max(s1))
}

/// The NDC bound of a view-space sphere's perspective silhouette (Mara & McGuire 2013, one axis at a time).
fn sphere_footprint(center: Vec3, radius: f32, proj: &Mat4) -> Footprint {
    let (depth, r) = (f64::from(-center.z), f64::from(radius));
    if depth <= -r {
        return Footprint::Hidden;
    }
    // The slope rect maps to NDC only when clip w is a positive multiple of view depth.
    let w_row = proj.row(3);
    let perspective = w_row.x == 0.0 && w_row.y == 0.0 && w_row.w == 0.0 && w_row.z < 0.0;
    if !perspective || depth * depth - r * r <= 1e-6 * depth * depth {
        return Footprint::Unbounded;
    }
    let (x0, x1) = tangent_slopes(f64::from(center.x), depth, r);
    let (y0, y1) = tangent_slopes(f64::from(center.y), depth, r);
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    // At depth 1 a slope is the view-space offset; NDC is affine in it, so the corners bound the rect's image.
    for s in [
        DVec2::new(x0, y0),
        DVec2::new(x0, y1),
        DVec2::new(x1, y0),
        DVec2::new(x1, y1),
    ] {
        let clip = *proj * Vec4::new(s.x as f32, s.y as f32, -1.0, 1.0);
        let ndc = clip.xy() / clip.w;
        min = min.min(ndc);
        max = max.max(ndc);
    }
    if min.is_finite() && max.is_finite() {
        Footprint::Ndc(min, max)
    } else {
        Footprint::Unbounded
    }
}

/// The tiles a light's sphere covers, or `None` when no on-screen point lies inside it.
/// See crates/webgpu_renderer/docs/renderer-bounds-invariant.md § The tile-light grid is conservative.
fn tile_rect_for_light(
    light: &[[f32; 4]; 4],
    grid: TileGrid,
    view: &Mat4,
    proj: &Mat4,
) -> Option<TileRect> {
    let (kind, range) = (light[0][3], light[1][3]);
    // Range 0 is glTF's "unbounded" (Blender exports it by default), so it reaches every tile.
    if kind == 3.0 || !(range > 0.0 && range.is_finite()) {
        return Some(grid.whole());
    }
    let center = view.transform_point3(Vec3::from_slice(&light[0][..3]));
    match sphere_footprint(center, range, proj) {
        Footprint::Hidden => None,
        Footprint::Unbounded => Some(grid.whole()),
        Footprint::Ndc(min, max) => grid.ndc_rect(min, max),
    }
}

/// Bins lights per tile into `scratch.grid` ([count, offset] per tile) and flat `scratch.indices`.
/// `view` must be rigid, and `proj` a perspective projection for a bound tighter than the whole grid.
pub fn build_tile_light_grid(
    scratch: &mut TileLightGridScratch,
    packed_lights: &[[f32; 4]; MAX_PUNCTUAL_LIGHTS * 4],
    light_count: u32,
    (width, height): (u32, u32),
    view: &Mat4,
    proj: &Mat4,
) {
    let (width, height) = (width.max(1), height.max(1));
    let grid = TileGrid {
        tiles_x: width.div_ceil(TILE_SIZE),
        tiles_y: height.div_ceil(TILE_SIZE),
        width,
        height,
    };
    scratch.rects.clear();
    for light in packed_lights
        .as_chunks::<4>()
        .0
        .iter()
        .take(light_count as usize)
    {
        scratch
            .rects
            .push(tile_rect_for_light(light, grid, view, proj));
    }
    count_lights_per_tile(scratch, grid);
    assign_tile_offsets(scratch, grid);
    write_tile_indices(scratch, grid);
}

fn count_lights_per_tile(scratch: &mut TileLightGridScratch, grid: TileGrid) {
    scratch.per_tile_counts.clear();
    scratch.per_tile_counts.resize(grid.total(), 0);
    for rect in scratch.rects.iter().flatten() {
        for idx in grid.tiles_in(*rect) {
            scratch.per_tile_counts[idx] = scratch.per_tile_counts[idx].saturating_add(1);
        }
    }
}

/// How many of a tile's `count` lights it lists: all of them, or none once they overflow it.
fn listed(count: u32) -> u32 {
    if count > MAX_LIGHTS_PER_TILE {
        0
    } else {
        count
    }
}

/// Writes each tile's `[count, offset]`; a count past `MAX_LIGHTS_PER_TILE` lists nothing at its offset.
fn assign_tile_offsets(scratch: &mut TileLightGridScratch, grid: TileGrid) {
    scratch.grid.clear();
    scratch.grid.resize(grid.total() * 2, 0);
    let mut running_offset = 0u32;
    for t in 0..grid.total() {
        let count = scratch.per_tile_counts[t];
        scratch.grid[t * 2] = count;
        scratch.grid[t * 2 + 1] = running_offset;
        running_offset += listed(count);
    }
    scratch.indices.clear();
    scratch.indices.resize((running_offset as usize).max(1), 0);
}

fn write_tile_indices(scratch: &mut TileLightGridScratch, grid: TileGrid) {
    scratch.write_positions.clear();
    scratch.write_positions.extend(
        scratch
            .grid
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[_, offset]| *offset),
    );
    for (i, rect) in scratch.rects.iter().enumerate() {
        let Some(rect) = rect else {
            continue;
        };
        for idx in grid.tiles_in(*rect) {
            // An overflowing tile lists nothing, so it never writes.
            if listed(scratch.grid[idx * 2]) == 0 {
                continue;
            }
            let wp = scratch.write_positions[idx];
            scratch.indices[wp as usize] = i as u32;
            scratch.write_positions[idx] = wp + 1;
        }
    }
}

#[cfg(test)]
mod tests;
