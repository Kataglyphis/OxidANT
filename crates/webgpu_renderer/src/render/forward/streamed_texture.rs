//! One streamed material texture: the levels its GPU texture holds, the CPU chain it steps from, and a step
//! in either direction. Which steps run is `streaming.rs`; the policy is `render/texture_stream.rs`.

use std::sync::Arc;

use crate::context::GpuContext;
use crate::render::texture::{chain_texture, copy_level, material_chain, write_chain, write_level};
use crate::render::texture_stream::{gpu_copy_allowed, initial_base, LevelChain, TextureStreaming};
use crate::scene::{CpuSampler, CpuTexture};

/// One material slot that samples a streamed texture.
pub(super) struct StreamUser {
    pub(super) primitive: usize,
    pub(super) slot: usize,
    pub(super) sampler: CpuSampler,
    /// UV units per local unit of the primitive, its slot's texture transform applied.
    pub(super) uv_per_unit: f32,
}

/// What the steps of one texture have cost since the upload.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct StepCounters {
    pub(super) written: u64,
    pub(super) copied: u64,
    pub(super) evictions: u64,
    pub(super) regenerated: u64,
}

/// The scene image a texture came from, and the colour space it uploads in, to rebuild a dropped chain.
pub(super) struct TextureSource {
    pub(super) image: Arc<CpuTexture>,
    pub(super) srgb: bool,
}

/// One GPU material texture and the slots that sample it.
pub(super) struct StreamedTexture {
    pub(super) texture: wgpu::Texture,
    label: String,
    source: TextureSource,
    format: wgpu::TextureFormat,
    /// Every chain level's size and byte count, kept when the chain itself is dropped.
    sizes: Vec<(u32, u32)>,
    level_bytes: Vec<u64>,
    /// Levels that can be the texture's level 0, finest first.
    pub(super) candidates: Vec<usize>,
    /// The upload's base: eviction never drops past it.
    pub(super) tail: usize,
    /// The chain level the GPU texture's level 0 holds.
    pub(super) base: usize,
    /// The CPU levels, while the budget holds them; a step that needs them rebuilds them from `source`.
    chain: Option<LevelChain>,
    /// This backend copies the format between textures.
    copies_allowed: bool,
    /// Frames in a row this texture has held more than it needs.
    pub(super) over_frames: u32,
    /// The frame counter at its last step, for the CPU budget's order.
    pub(super) last_used: u64,
    pub(super) users: Vec<StreamUser>,
}

impl StreamedTexture {
    /// Uploads `chain` from the tail `streaming` asks for, or whole; `None` if no level fits the device.
    pub(super) fn upload(
        gpu: &GpuContext,
        chain: LevelChain,
        source: TextureSource,
        label: String,
        streaming: &TextureStreaming,
    ) -> Option<Self> {
        let max_side = gpu.device.limits().max_texture_dimension_2d;
        let candidates = chain.base_candidates(max_side);
        let Some(&finest) = candidates.first() else {
            log::warn!("{label}: no mip level fits the device; the slot keeps its default");
            return None;
        };
        if finest > 0 {
            let (w, h) = (chain.levels[0].width, chain.levels[0].height);
            let (fw, fh) = (chain.levels[finest].width, chain.levels[finest].height);
            log::warn!("{label}: {w}x{h} exceeds the device's {max_side}-texel side; uploaded at {fw}x{fh}");
        }
        let sizes = chain.sizes();
        let tail = initial_base(&candidates, &sizes, streaming.tail_side)?;
        let base = if streaming.enabled { tail } else { finest };
        Some(Self {
            texture: write_chain(gpu, &chain, base, Some(&label)),
            label,
            source,
            format: chain.format,
            level_bytes: chain.levels.iter().map(|l| l.data.len() as u64).collect(),
            sizes,
            candidates,
            tail,
            base,
            copies_allowed: gpu_copy_allowed(gpu.adapter_info.backend, chain.format),
            // A texture whose tail is its finest level never steps.
            chain: (tail != finest).then_some(chain),
            over_frames: 0,
            last_used: 0,
            users: Vec::new(),
        })
    }

    pub(super) fn finest(&self) -> usize {
        self.candidates[0]
    }

    pub(super) fn pending(&self) -> bool {
        self.base != self.finest()
    }

    /// The finest chain level's longer side, in texels.
    pub(super) fn side(&self) -> u32 {
        self.sizes[0].0.max(self.sizes[0].1)
    }

    pub(super) fn bytes_from(&self, level: usize) -> u64 {
        self.level_bytes[level..].iter().sum()
    }

    /// Bytes of the CPU chain this texture holds.
    pub(super) fn cpu_bytes(&self) -> u64 {
        self.chain.as_ref().map_or(0, |chain| chain.bytes_from(0))
    }

    pub(super) fn drop_chain(&mut self) {
        self.chain = None;
    }

    /// Whether a step to `base` copies the resident levels rather than writing them.
    fn copies(&self, gpu_copies: bool) -> bool {
        gpu_copies && self.copies_allowed
    }

    /// Bytes a step to `base` writes into the new texture, plus the chain it rebuilds first when it needs one.
    pub(super) fn step_bytes(&self, base: usize, gpu_copies: bool) -> u64 {
        let needs_chain = !self.copies(gpu_copies) || base < self.base;
        let rebuild = if needs_chain && self.chain.is_none() {
            self.bytes_from(0)
        } else {
            0
        };
        self.bytes_from(base) + rebuild
    }

    /// The CPU chain, rebuilt from the source image when the budget dropped it; `None` if that fails.
    fn ensure_chain(
        &mut self,
        gpu: &GpuContext,
        counters: &mut StepCounters,
    ) -> Option<&LevelChain> {
        if self.chain.is_none() {
            let rebuilt =
                material_chain(gpu, &self.source.image, self.source.srgb, Some(&self.label))
                    .filter(|chain| chain.format == self.format && chain.sizes() == self.sizes);
            if rebuilt.is_none() {
                log::warn!(
                    "{}: its chain does not rebuild as uploaded; it keeps its levels",
                    self.label
                );
                // Frozen where it is, so the step is not retried every frame.
                let base = self.base;
                self.candidates.retain(|&level| level >= base);
                self.tail = base;
                return None;
            }
            counters.regenerated += 1;
            self.chain = rebuilt;
        }
        self.chain.as_ref()
    }

    /// Replaces the GPU texture with one whose level 0 is chain level `base`, finer or coarser.
    /// Levels the old texture holds are copied on the GPU when `gpu_copies` and the backend allow it.
    pub(super) fn step(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        base: usize,
        (gpu_copies, counters): (bool, &mut StepCounters),
    ) -> bool {
        let texture = if self.copies(gpu_copies) {
            self.copied_step(gpu, encoder, base, counters)
        } else {
            self.written_step(gpu, base, counters)
        };
        let Some(texture) = texture else {
            return false;
        };
        counters.evictions += u64::from(base > self.base);
        self.texture = texture;
        self.base = base;
        true
    }

    /// Writes only the levels finer than the resident ones; copies the rest from the current texture.
    fn copied_step(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        base: usize,
        counters: &mut StepCounters,
    ) -> Option<wgpu::Texture> {
        let resident = self.base;
        let levels = self.sizes.len() - base;
        let label = Some(self.label.as_str());
        let texture = chain_texture(&gpu.device, self.format, self.sizes[base], levels, label);
        if base < resident {
            let chain = self.ensure_chain(gpu, counters)?;
            for level in base..resident {
                write_level(gpu, &texture, (level - base) as u32, &chain.levels[level]);
                counters.written += chain.levels[level].data.len() as u64;
            }
        }
        self.texture_copied_from(encoder, &texture, (base.max(resident), base), counters);
        Some(texture)
    }

    /// Writes every level from `base` on from the CPU chain.
    fn written_step(
        &mut self,
        gpu: &GpuContext,
        base: usize,
        counters: &mut StepCounters,
    ) -> Option<wgpu::Texture> {
        let label = self.label.clone();
        let chain = self.ensure_chain(gpu, counters)?;
        counters.written += chain.bytes_from(base);
        Some(write_chain(gpu, chain, base, Some(&label)))
    }

    /// Copies chain levels `from..` of the current texture into `texture`, whose level 0 is chain level `new_base`.
    fn texture_copied_from(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        (from, new_base): (usize, usize),
        counters: &mut StepCounters,
    ) {
        for level in from..self.sizes.len() {
            let src = (&self.texture, (level - self.base) as u32);
            copy_level(
                encoder,
                src,
                (texture, (level - new_base) as u32),
                self.sizes[level],
            );
            counters.copied += self.level_bytes[level];
        }
    }
}
