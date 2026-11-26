//! Mip-tail texture streaming: which level a material texture starts from, which level the camera
//! needs, and which finer steps fit one frame's upload budget. Pure CPU; the GPU half is
//! `forward/streaming.rs`.

use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::scene::{CpuTexture, Vertex};

/// How material textures reach the GPU: whole at upload, or a coarse tail first and finer levels as the camera needs them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureStreaming {
    /// Off uploads every level at load; switched off later, the next frame finishes what is pending.
    pub enabled: bool,
    /// An upload writes the levels up to this many texels per side; finer ones wait. Read by `upload_scene`.
    pub tail_side: u32,
    /// Bytes one frame may write into streamed textures; the first step always runs, so a larger level still lands.
    pub bytes_per_frame: u64,
    /// Frames a texture must hold more than one level finer than any user needs before it drops one; 0 never evicts.
    pub evict_after_frames: u32,
    /// Bytes of CPU level chains kept for later steps; the rest regenerate from the scene's source image.
    pub cpu_bytes: u64,
    /// A step copies the levels already resident on the GPU instead of writing them again from the CPU.
    pub gpu_copies: bool,
}

impl TextureStreaming {
    /// The interactive app's choice: a 128-texel tail, 8 MiB a frame, eviction after two seconds at 60 Hz, 256 MiB of chains.
    pub const INTERACTIVE: Self = Self {
        enabled: true,
        tail_side: 128,
        bytes_per_frame: 8 << 20,
        evict_after_frames: 120,
        cpu_bytes: 256 << 20,
        gpu_copies: true,
    };
}

impl TextureStreaming {
    /// Bytes of CPU chains to keep: none while off, since nothing steps then.
    pub(crate) fn chain_budget(&self) -> u64 {
        if self.enabled {
            self.cpu_bytes
        } else {
            0
        }
    }
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
    /// Bytes of level chains held on the CPU for later steps.
    pub cpu_bytes: u64,
    /// Bytes steps have written from the CPU since the upload, its own writes excluded.
    pub step_written_bytes: u64,
    /// Bytes steps have copied between GPU textures since the upload.
    pub step_copied_bytes: u64,
    /// Steps that dropped a level since the upload.
    pub evictions: u64,
    /// Chains rebuilt from the source image since the upload, after the CPU budget dropped them.
    pub regenerated: u64,
}

/// One level of a [`LevelChain`].
pub(crate) struct ChainLevel {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data: LevelBytes,
}

/// A level's bytes: built for the chain, or the scene image's own, which the chain shares instead of copying.
pub(crate) enum LevelBytes {
    Owned(Vec<u8>),
    /// The image's RGBA8 base.
    Base(Arc<CpuTexture>),
    /// Mip `.1` of the image's block payload.
    Stored(Arc<CpuTexture>, usize),
}

impl std::ops::Deref for LevelBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            LevelBytes::Owned(bytes) => bytes,
            LevelBytes::Base(image) => &image.rgba8,
            LevelBytes::Stored(image, level) => image
                .compressed
                .as_ref()
                .and_then(|compressed| compressed.mips.get(*level))
                .map_or(&[], Vec::as_slice),
        }
    }
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

/// Levels a texture may hold finer than its users need before it drops one.
pub(crate) const EVICTION_MARGIN: usize = 1;

/// The next candidate coarser than `resident`, when it holds more than [`EVICTION_MARGIN`] levels finer than `wanted`.
/// Never coarser than `wanted` itself, which would stream it back, nor than the upload's `tail`.
pub(crate) fn coarser_base(
    candidates: &[usize],
    resident: usize,
    wanted: usize,
    tail: usize,
) -> Option<usize> {
    if resident.saturating_add(EVICTION_MARGIN) >= wanted {
        return None;
    }
    candidates
        .iter()
        .copied()
        .find(|&level| level > resident)
        .filter(|&level| level <= wanted && level <= tail)
}

/// Frames in a row a texture has wanted to drop a level; it drops one a frame once `after` is reached.
pub(crate) fn evict_now(over_frames: &mut u32, wants_coarser: bool, after: u32) -> bool {
    if !wants_coarser || after == 0 {
        *over_frames = 0;
        return false;
    }
    *over_frames = over_frames.saturating_add(1);
    *over_frames >= after
}

/// Whether a step may copy resident levels between GPU textures: GL copies no compressed texture.
pub(crate) fn gpu_copy_allowed(backend: wgpu::Backend, format: wgpu::TextureFormat) -> bool {
    !(backend == wgpu::Backend::Gl && format.is_compressed())
}

/// Which held chains to drop so the rest fit `budget`: least recently stepped first, then the earliest uploaded.
/// `held` is (texture, last frame it stepped, chain bytes).
pub(crate) fn chains_over_budget(mut held: Vec<(usize, u64, u64)>, budget: u64) -> Vec<usize> {
    let mut total: u64 = held.iter().map(|&(_, _, bytes)| bytes).sum();
    held.sort_by_key(|&(id, last_used, _)| (last_used, id));
    let mut dropped = Vec::new();
    for (id, _, bytes) in held {
        if total <= budget {
            break;
        }
        total -= bytes;
        dropped.push(id);
    }
    dropped
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

/// One texture's case for a finer, or with `finer` false a coarser, level this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StreamRequest {
    pub(crate) id: usize,
    /// A finer step; an eviction otherwise, which runs after every finer step.
    pub(crate) finer: bool,
    /// A user is in the camera frustum.
    pub(crate) visible: bool,
    /// Levels between the resident base and the wanted one.
    pub(crate) levels_short: usize,
    /// Bytes the step writes into the new texture, plus the chain it regenerates first.
    pub(crate) step_bytes: u64,
}

/// The requests one frame runs: finer steps before evictions, visible first, then the furthest off, then the cheapest.
/// Greedy within `budget`; the first always runs, so no level is too large to ever land.
pub(crate) fn plan_steps(mut requests: Vec<StreamRequest>, budget: u64) -> Vec<usize> {
    requests.sort_by(|a, b| {
        b.finer
            .cmp(&a.finer)
            .then(b.visible.cmp(&a.visible))
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
            finer: true,
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
    fn evictions_run_after_every_finer_step() {
        let evict = |id, bytes| StreamRequest {
            finer: false,
            ..request(id, false, 3, bytes)
        };
        let requests = vec![evict(0, 5), request(1, false, 1, 40), evict(2, 1)];
        assert_eq!(plan_steps(requests.clone(), 46), vec![1, 2, 0]);
        assert_eq!(plan_steps(requests.clone(), 41), vec![1, 2]);
        // The first runs whatever its kind, so a frame with only evictions still drops one.
        assert_eq!(plan_steps(vec![evict(4, 1 << 30)], 1), vec![4]);
    }

    #[test]
    fn a_texture_drops_a_level_only_past_the_margin() {
        let candidates = [0, 1, 2, 3, 4];
        // One level finer than needed is within the margin: no eviction, no thrash at a boundary.
        assert_eq!(coarser_base(&candidates, 1, 2, 3), None);
        assert_eq!(coarser_base(&candidates, 1, 1, 3), None);
        assert_eq!(coarser_base(&candidates, 1, 0, 3), None);
        assert_eq!(coarser_base(&candidates, 1, 3, 3), Some(2));
        assert_eq!(coarser_base(&candidates, 0, usize::MAX, 3), Some(1));
        // The upload's tail stays: never coarser than it.
        assert_eq!(coarser_base(&candidates, 3, usize::MAX, 3), None);
        // A gap that would cross the wanted level is not taken: the texture would stream straight back.
        assert_eq!(coarser_base(&[0, 3], 0, 2, 3), None);
        assert_eq!(coarser_base(&[0, 3], 0, 3, 3), Some(3));
    }

    #[test]
    fn an_eviction_waits_for_its_frames_and_resets_when_the_need_returns() {
        let mut over = 0;
        let run: Vec<bool> = [true, true, true, true]
            .iter()
            .map(|&w| evict_now(&mut over, w, 3))
            .collect();
        assert_eq!(run, [false, false, true, true]);
        assert!(!evict_now(&mut over, false, 3));
        assert_eq!(over, 0);
        assert!(!evict_now(&mut over, true, 3), "the wait starts over");
        assert!(!evict_now(&mut over, true, 0), "0 never evicts");
        assert_eq!(over, 0);
        // Alternating need never reaches the wait: a camera hovering at a boundary does not thrash.
        let mut over = 0;
        assert!((0..100).all(|i| !evict_now(&mut over, i % 2 == 0, 2)));
    }

    #[test]
    fn gl_keeps_the_cpu_path_for_compressed_textures_only() {
        use wgpu::{Backend, TextureFormat as F};
        assert!(!gpu_copy_allowed(Backend::Gl, F::Bc1RgbaUnormSrgb));
        assert!(!gpu_copy_allowed(Backend::Gl, F::Etc2Rgba8Unorm));
        assert!(gpu_copy_allowed(Backend::Gl, F::Rgba8UnormSrgb));
        for backend in [
            Backend::Vulkan,
            Backend::Metal,
            Backend::Dx12,
            Backend::BrowserWebGpu,
        ] {
            assert!(gpu_copy_allowed(backend, F::Bc7RgbaUnorm));
        }
    }

    #[test]
    fn the_cpu_budget_drops_the_least_recently_stepped_chains() {
        let held = vec![(0, 5, 100), (1, 2, 100), (2, 9, 100), (3, 2, 50)];
        assert_eq!(chains_over_budget(held.clone(), 350), Vec::<usize>::new());
        assert_eq!(chains_over_budget(held.clone(), 300), vec![1]);
        assert_eq!(chains_over_budget(held.clone(), 120), vec![1, 3, 0]);
        assert_eq!(chains_over_budget(held, 0), vec![1, 3, 0, 2]);
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
