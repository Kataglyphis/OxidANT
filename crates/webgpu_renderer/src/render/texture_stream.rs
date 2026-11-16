//! Mip-tail texture streaming: which level a material texture starts from, which level the camera
//! needs, and which finer steps fit one frame's upload budget. Pure CPU; the GPU half is
//! `forward/streaming.rs`.

use glam::{Mat4, Vec3};

use crate::scene::Vertex;

/// How material textures reach the GPU: whole at upload, or a coarse tail first and finer levels as the camera needs them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureStreaming {
    /// Off uploads every level at load; switched off later, the next frame finishes what is pending.
    pub enabled: bool,
    /// An upload writes the levels up to this many texels per side; finer ones wait. Read by `upload_scene`.
    pub tail_side: u32,
    /// Bytes one frame may write into streamed textures; the first step always runs, so a larger level still lands.
    pub bytes_per_frame: u64,
}

impl TextureStreaming {
    /// The interactive app's choice: a 128-texel tail and 8 MiB a frame.
    pub const INTERACTIVE: Self = Self {
        enabled: true,
        tail_side: 128,
        bytes_per_frame: 8 << 20,
    };
}

impl Default for TextureStreaming {
    /// Off: a headless render or a test sees every level from its first frame.
    fn default() -> Self {
        Self {
            enabled: false,
            ..Self::INTERACTIVE
        }
    }
}

/// What the material textures hold on the GPU against what their full chains would.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureResidency {
    /// Distinct GPU textures.
    pub textures: usize,
    /// Textures that still lack finer levels.
    pub streaming: usize,
    /// Bytes of the levels uploaded now.
    pub resident_bytes: u64,
    /// Bytes every texture would hold with its finest level resident.
    pub full_bytes: u64,
}

/// One level of a [`LevelChain`].
pub(crate) struct ChainLevel {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data: Vec<u8>,
}

/// A material texture's levels in the format they upload as, finest first.
pub(crate) struct LevelChain {
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) levels: Vec<ChainLevel>,
}

impl LevelChain {
    pub(crate) fn sizes(&self) -> Vec<(u32, u32)> {
        self.levels.iter().map(|l| (l.width, l.height)).collect()
    }

    /// Bytes of the levels from `base` to the coarsest.
    pub(crate) fn bytes_from(&self, base: usize) -> u64 {
        self.levels[base..]
            .iter()
            .map(|l| l.data.len() as u64)
            .sum()
    }

    /// Levels that can be a texture's level 0 on a device with `max_side`, finest first.
    pub(crate) fn base_candidates(&self, max_side: u32) -> Vec<usize> {
        base_candidates(&self.sizes(), self.format.block_dimensions().0, max_side)
    }
}

/// Levels that can be level 0: within the device's side and, for a block format, whole blocks.
pub(crate) fn base_candidates(sizes: &[(u32, u32)], block: u32, max_side: u32) -> Vec<usize> {
    sizes
        .iter()
        .enumerate()
        .filter(|(_, &(w, h))| {
            w <= max_side && h <= max_side && w.is_multiple_of(block) && h.is_multiple_of(block)
        })
        .map(|(level, _)| level)
        .collect()
}

/// The level an upload starts from: the finest candidate whose sides fit `tail_side`, else the coarsest.
pub(crate) fn initial_base(
    candidates: &[usize],
    sizes: &[(u32, u32)],
    tail_side: u32,
) -> Option<usize> {
    candidates
        .iter()
        .copied()
        .find(|&level| sizes[level].0.max(sizes[level].1) <= tail_side)
        .or_else(|| candidates.last().copied())
}

/// The next candidate finer than `resident`, when `wanted` is finer than it; one step, never a jump.
pub(crate) fn next_base(candidates: &[usize], resident: usize, wanted: usize) -> Option<usize> {
    if wanted >= resident {
        return None;
    }
    candidates.iter().copied().rfind(|&level| level < resident)
}

/// Mip level a surface needs at `distance`: log2 of texels per pixel there, 0 when magnified.
/// Head-on, so a grazing surface, which the anisotropic sampler reads finer, is never starved.
pub(crate) fn wanted_level(
    texels_per_unit: f32,
    distance: f32,
    pixels_per_unit_at_one: f32,
) -> usize {
    let texels_per_pixel = texels_per_unit * distance / pixels_per_unit_at_one;
    if !texels_per_pixel.is_finite() || texels_per_pixel <= 1.0 {
        return 0;
    }
    texels_per_pixel.log2().floor() as usize
}

/// UV units per local unit over a primitive: the root of its UV area over its surface area.
/// 0 for a primitive without measurable UV area, which then wants the finest level.
pub(crate) fn uv_density(vertices: &[Vertex], indices: &[u32], uv1: bool) -> f32 {
    let (mut uv_area, mut area) = (0.0f64, 0.0f64);
    for tri in indices.as_chunks::<3>().0 {
        let Some([a, b, c]) = tri_vertices(vertices, tri) else {
            continue;
        };
        let (p0, p1, p2) = (
            Vec3::from(a.position),
            Vec3::from(b.position),
            Vec3::from(c.position),
        );
        area += f64::from((p1 - p0).cross(p2 - p0).length());
        let uv = |v: &Vertex| glam::Vec2::from(if uv1 { v.uv1 } else { v.uv });
        let (t0, t1, t2) = (uv(a), uv(b), uv(c));
        uv_area += f64::from((t1 - t0).perp_dot(t2 - t0).abs());
    }
    let density = (uv_area / area).sqrt() as f32;
    if density.is_finite() {
        density
    } else {
        0.0
    }
}

fn tri_vertices<'a>(vertices: &'a [Vertex], tri: &[u32; 3]) -> Option<[&'a Vertex; 3]> {
    Some([
        vertices.get(tri[0] as usize)?,
        vertices.get(tri[1] as usize)?,
        vertices.get(tri[2] as usize)?,
    ])
}

/// How much a KHR_texture_transform stretches UV area, as a length factor.
pub(crate) fn uv_transform_scale(rows: &[[f32; 3]; 2]) -> f32 {
    (rows[0][0] * rows[1][1] - rows[0][1] * rows[1][0])
        .abs()
        .sqrt()
}

/// The largest axis scale of `model`: dividing by it never overstates UV per world unit.
pub(crate) fn max_axis_scale(model: &Mat4) -> f32 {
    model
        .x_axis
        .truncate()
        .length()
        .max(model.y_axis.truncate().length())
        .max(model.z_axis.truncate().length())
}

/// The distance from `eye` to the nearest point of an AABB, 0 inside it.
pub(crate) fn distance_to_aabb(eye: Vec3, min: Vec3, max: Vec3) -> f32 {
    (eye.clamp(min, max) - eye).length()
}

/// One texture's case for a finer level this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StreamRequest {
    pub(crate) id: usize,
    /// A user is in the camera frustum.
    pub(crate) visible: bool,
    /// Levels between the resident base and the wanted one.
    pub(crate) levels_short: usize,
    /// Bytes the step uploads.
    pub(crate) step_bytes: u64,
}

/// The requests one frame runs: visible first, then the furthest short, then the cheapest.
/// Greedy within `budget`; the first always runs, so no level is too large to ever land.
pub(crate) fn plan_steps(mut requests: Vec<StreamRequest>, budget: u64) -> Vec<usize> {
    requests.sort_by(|a, b| {
        b.visible
            .cmp(&a.visible)
            .then(b.levels_short.cmp(&a.levels_short))
            .then(a.step_bytes.cmp(&b.step_bytes))
            .then(a.id.cmp(&b.id))
    });
    let mut spent = 0u64;
    let mut chosen = Vec::new();
    for request in requests {
        if chosen.is_empty() || spent + request.step_bytes <= budget {
            spent += request.step_bytes;
            chosen.push(request.id);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_sizes(mut w: u32, mut h: u32) -> Vec<(u32, u32)> {
        let mut sizes = vec![(w, h)];
        while w > 1 || h > 1 {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            sizes.push((w, h));
        }
        sizes
    }

    #[test]
    fn candidates_skip_levels_over_the_side_limit() {
        let sizes = chain_sizes(1024, 256);
        assert_eq!(base_candidates(&sizes, 1, 256)[0], 2);
        assert_eq!(base_candidates(&sizes, 1, 256).len(), sizes.len() - 2);
    }

    #[test]
    fn block_candidates_are_whole_blocks() {
        // 24x12 -> 12x6 -> 6x3: only the base is whole 4x4 blocks.
        let sizes = chain_sizes(24, 12);
        assert_eq!(base_candidates(&sizes, 4, 8192), vec![0]);
        let sizes = chain_sizes(64, 64);
        assert_eq!(base_candidates(&sizes, 4, 8192), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_initial_base_is_the_finest_level_within_the_tail() {
        let sizes = chain_sizes(1024, 512);
        let candidates = base_candidates(&sizes, 1, 8192);
        assert_eq!(initial_base(&candidates, &sizes, 128), Some(3));
        assert_eq!(initial_base(&candidates, &sizes, 4096), Some(0));
        // A block chain whose aligned levels all exceed the tail starts from the coarsest of them.
        let sizes = chain_sizes(1000, 600);
        let candidates = base_candidates(&sizes, 4, 8192);
        assert_eq!(candidates, vec![0, 1]);
        assert_eq!(initial_base(&candidates, &sizes, 128), Some(1));
        assert_eq!(initial_base(&[], &sizes, 128), None);
    }

    #[test]
    fn next_base_steps_one_candidate_toward_the_wanted_level() {
        let candidates = [0, 1, 2, 3, 4];
        assert_eq!(next_base(&candidates, 3, 0), Some(2));
        assert_eq!(next_base(&candidates, 3, 2), Some(2));
        assert_eq!(next_base(&candidates, 3, 3), None);
        assert_eq!(next_base(&candidates, 3, 5), None);
        // A gap in the candidates is crossed whole: 1 is wanted, 0 is the only finer one.
        assert_eq!(next_base(&[0, 3], 3, 1), Some(0));
        assert_eq!(next_base(&[0, 3], 0, 0), None);
    }

    #[test]
    fn the_wanted_level_is_log2_of_texels_per_pixel() {
        // 1024 texels over one unit, 512 px per unit at distance 1: 2 texels a pixel, level 1.
        assert_eq!(wanted_level(1024.0, 1.0, 512.0), 1);
        assert_eq!(wanted_level(1024.0, 0.5, 512.0), 0);
        assert_eq!(wanted_level(1024.0, 8.0, 512.0), 4);
        assert_eq!(wanted_level(1024.0, 0.0, 512.0), 0);
        assert_eq!(wanted_level(f32::NAN, 1.0, 512.0), 0);
        assert_eq!(wanted_level(1024.0, 1.0, 0.0), 0);
        // Further away never wants a finer level.
        let mut last = 0;
        for d in 1..200 {
            let level = wanted_level(2048.0, d as f32 * 0.25, 700.0);
            assert!(level >= last);
            last = level;
        }
    }

    fn quad(size: f32, uv_size: f32) -> (Vec<Vertex>, Vec<u32>) {
        let v = |x: f32, y: f32| Vertex {
            position: [x * size, y * size, 0.0],
            uv: [x * uv_size, y * uv_size],
            uv1: [x, y],
            ..bytemuck::Zeroable::zeroed()
        };
        (
            vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
            vec![0, 1, 2, 0, 2, 3],
        )
    }

    #[test]
    fn uv_density_is_uv_length_per_surface_length() {
        let (vertices, indices) = quad(2.0, 4.0);
        assert!((uv_density(&vertices, &indices, false) - 2.0).abs() < 1e-5);
        assert!((uv_density(&vertices, &indices, true) - 0.5).abs() < 1e-5);
        assert_eq!(uv_density(&vertices, &[], false), 0.0);
        // Out-of-range indices are skipped, not a panic.
        assert_eq!(uv_density(&vertices, &[0, 1, 99], false), 0.0);
        let (flat, indices) = quad(0.0, 1.0);
        assert_eq!(uv_density(&flat, &indices, false), 0.0);
    }

    #[test]
    fn transform_and_model_scales_are_length_factors() {
        assert_eq!(uv_transform_scale(&[[4.0, 0.0, 0.5], [0.0, 4.0, 0.5]]), 4.0);
        assert_eq!(
            uv_transform_scale(&[[0.0, -2.0, 0.0], [2.0, 0.0, 0.0]]),
            2.0
        );
        let model = Mat4::from_scale(Vec3::new(1.0, 3.0, 2.0));
        assert_eq!(max_axis_scale(&model), 3.0);
        assert_eq!(
            distance_to_aabb(Vec3::new(3.0, 0.5, 4.5), Vec3::ZERO, Vec3::ONE),
            (4.0f32 + 12.25).sqrt()
        );
        assert_eq!(
            distance_to_aabb(Vec3::splat(0.5), Vec3::ZERO, Vec3::ONE),
            0.0
        );
    }

    fn request(id: usize, visible: bool, levels_short: usize, step_bytes: u64) -> StreamRequest {
        StreamRequest {
            id,
            visible,
            levels_short,
            step_bytes,
        }
    }

    #[test]
    fn the_plan_runs_visible_textures_first_within_the_budget() {
        let requests = vec![
            request(0, false, 5, 10),
            request(1, true, 1, 40),
            request(2, true, 3, 50),
            request(3, true, 3, 30),
        ];
        assert_eq!(plan_steps(requests.clone(), 85), vec![3, 2]);
        assert_eq!(plan_steps(requests.clone(), 130), vec![3, 2, 1, 0]);
        // A smaller request later in the order still fills what the budget leaves.
        assert_eq!(plan_steps(requests, 100), vec![3, 2, 0]);
    }

    #[test]
    fn the_first_step_runs_even_over_the_budget() {
        let requests = vec![request(7, true, 1, 1 << 30), request(8, true, 1, 1 << 30)];
        assert_eq!(plan_steps(requests, 1 << 20), vec![7]);
        assert!(plan_steps(Vec::new(), 1 << 20).is_empty());
    }

    #[test]
    fn streaming_is_off_by_default_and_on_for_the_app() {
        assert!(!TextureStreaming::default().enabled);
        let on = TextureStreaming {
            enabled: true,
            ..TextureStreaming::default()
        };
        assert_eq!(on, TextureStreaming::INTERACTIVE);
    }
}
