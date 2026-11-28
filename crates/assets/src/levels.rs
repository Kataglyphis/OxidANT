//! A texture's mip chain, built once for both renderers: a 2x2 box filter for a decoded image (sRGB averaged in linear
//! space), Basis levels transcoded straight to RGBA8 or to a block format the consumer samples, BCn levels decoded or
//! kept as stored.

use crate::scene::{CompressedFormat, CompressedTexture, CpuTexture};

/// What a chain's bytes hold: RGBA8 texels, or one block format's 4x4 blocks.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ChainFormat {
    #[default]
    Rgba8,
    Blocks(CompressedFormat),
}

impl ChainFormat {
    /// The bytes of one `width` x `height` level: four per texel, or its whole blocks.
    pub fn level_bytes(self, width: u32, height: u32) -> usize {
        match self {
            ChainFormat::Rgba8 => (width as usize) * (height as usize) * 4,
            ChainFormat::Blocks(format) => {
                width.div_ceil(4) as usize
                    * height.div_ceil(4) as usize
                    * format.block_bytes() as usize
            }
        }
    }

    /// The bytes of `levels` levels from a `width` x `height` base.
    pub fn chain_bytes(self, width: u32, height: u32, levels: u32) -> usize {
        (0..levels)
            .map(|level| {
                let (w, h) = level_size(width, height, level);
                self.level_bytes(w, h)
            })
            .sum()
    }
}

/// A mip chain, level 0 first, packed back to back: level `i` is `max(width >> i, 1)` by `max(height >> i, 1)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureChain {
    pub format: ChainFormat,
    pub width: u32,
    pub height: u32,
    pub level_count: u32,
    pub bytes: Vec<u8>,
}

impl TextureChain {
    /// Each level's width, height and bytes, up to the first the bytes do not hold.
    pub fn levels(&self) -> impl Iterator<Item = (u32, u32, &[u8])> {
        let mut rest = self.bytes.as_slice();
        (0..self.level_count).map_while(move |level| {
            let (w, h) = level_size(self.width, self.height, level);
            let (bytes, tail) = rest.split_at_checked(self.format.level_bytes(w, h))?;
            rest = tail;
            Some((w, h, bytes))
        })
    }

    /// `mips` as stored, each cut to its whole blocks, packed back to back.
    pub fn of_blocks(
        format: CompressedFormat,
        width: u32,
        height: u32,
        mips: &[Vec<u8>],
    ) -> TextureChain {
        let format = ChainFormat::Blocks(format);
        let mut bytes = Vec::with_capacity(format.chain_bytes(width, height, mips.len() as u32));
        for (level, mip) in (0u32..).zip(mips) {
            let (w, h) = level_size(width, height, level);
            bytes.extend_from_slice(&mip[..format.level_bytes(w, h).min(mip.len())]);
        }
        TextureChain {
            format,
            width,
            height,
            level_count: mips.len() as u32,
            bytes,
        }
    }
}

/// The block formats a consumer samples and the longest side it creates (0 for no limit); the default samples none,
/// so every chain arrives as RGBA8.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlockSupport {
    pub bc: bool,
    pub etc2: bool,
    pub astc: bool,
    pub max_side: u32,
}

impl BlockSupport {
    pub fn samples(&self, format: CompressedFormat) -> bool {
        use CompressedFormat as F;
        match format {
            F::Bc1RgbaUnorm | F::Bc3RgbaUnorm | F::Bc5RgUnorm | F::Bc7RgbaUnorm => self.bc,
            F::Etc2Rgba8Unorm => self.etc2,
            F::Astc4x4RgbaUnorm => self.astc,
        }
    }

    /// The format a Basis payload transcodes to, [`crate::basis::basis_target`]'s order; `None` for RGBA8.
    pub fn basis_target(&self) -> Option<CompressedFormat> {
        crate::basis::basis_target(self.bc, self.etc2, self.astc)
    }

    /// Whether `levels` stored levels of `format` upload as they are: the format sampled, a base of whole 4x4 blocks,
    /// and a level to start from within the side limit.
    pub fn uploads_blocks(
        &self,
        format: CompressedFormat,
        width: u32,
        height: u32,
        levels: u32,
    ) -> bool {
        self.samples(format)
            && width.is_multiple_of(4)
            && height.is_multiple_of(4)
            && first_block_base(width, height, levels, self.max_side).is_some()
    }
}

/// The first of `levels` levels a block texture can start from: whole 4x4 blocks with no side over `max_side` (0 for
/// no limit), as the WebGPU renderer's `base_candidates` picks.
pub fn first_block_base(width: u32, height: u32, levels: u32, max_side: u32) -> Option<u32> {
    (0..levels).find(|&level| {
        let (w, h) = level_size(width, height, level);
        w.is_multiple_of(4) && h.is_multiple_of(4) && (max_side == 0 || w.max(h) <= max_side)
    })
}

/// Level `level`'s size, never below 1x1, also past the 32 levels a `u32` side can have.
pub fn level_size(width: u32, height: u32, level: u32) -> (u32, u32) {
    let side = |s: u32| s.checked_shr(level).unwrap_or(0).max(1);
    (side(width), side(height))
}

/// Levels of a full chain down to 1x1: 1 for 1x1, 12 for 2048 on its longer side.
pub fn full_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

/// The bytes of `levels` RGBA8 levels from a `width` x `height` base.
pub fn chain_bytes(width: u32, height: u32, levels: u32) -> usize {
    (0..levels)
        .map(|level| {
            let (w, h) = level_size(width, height, level);
            (w as usize) * (h as usize) * 4
        })
        .sum()
}

pub fn srgb_to_linear(byte: u8) -> f32 {
    let c = f32::from(byte) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(value: f32) -> u8 {
    let c = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// [`srgb_to_linear`] of every byte, built once: four calls per channel otherwise.
fn linear_table() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| std::array::from_fn(|b| srgb_to_linear(b as u8)))
}

/// One 2x2 box-filtered halving of the `pw` x `ph` level `previous` into `next`: an odd edge drops its last row or
/// column, sRGB colour averages in linear space, alpha and data as stored, both rounded half up.
fn halve(previous: &[u8], pw: u32, ph: u32, next: &mut [u8], srgb: bool) {
    let (nw, nh) = ((pw / 2).max(1), (ph / 2).max(1));
    let to_linear = linear_table();
    let texel = |x: u32, y: u32| (y as usize * pw as usize + x as usize) * 4;
    for y in 0..nh {
        for x in 0..nw {
            let (x0, x1) = ((x * 2).min(pw - 1), (x * 2 + 1).min(pw - 1));
            let (y0, y1) = ((y * 2).min(ph - 1), (y * 2 + 1).min(ph - 1));
            let taps = [texel(x0, y0), texel(x1, y0), texel(x0, y1), texel(x1, y1)];
            let out = (y as usize * nw as usize + x as usize) * 4;
            for channel in 0..4 {
                let samples = taps.map(|tap| previous[tap + channel]);
                next[out + channel] = if srgb && channel < 3 {
                    let sum: f32 = samples.iter().map(|&b| to_linear[b as usize]).sum();
                    linear_to_srgb(sum / 4.0)
                } else {
                    // Round half up like the sRGB arm; truncating compounds a bias per level.
                    ((samples.iter().map(|&b| u32::from(b)).sum::<u32>() + 2) / 4) as u8
                };
            }
        }
    }
}

/// `stored` packed levels of a `width` x `height` chain, completed down to 1x1 by halving the last stored level.
pub fn complete_chain(
    mut texels: Vec<u8>,
    width: u32,
    height: u32,
    stored: u32,
    srgb: bool,
) -> TextureChain {
    let level_count = full_level_count(width, height);
    if width == 0
        || height == 0
        || stored == 0
        || texels.len() != chain_bytes(width, height, stored)
    {
        return TextureChain {
            format: ChainFormat::Rgba8,
            width,
            height,
            level_count: stored,
            bytes: texels,
        };
    }
    let stored = stored.min(level_count);
    texels.resize(chain_bytes(width, height, level_count), 0);
    for level in stored..level_count {
        let previous_start = chain_bytes(width, height, level - 1);
        let next_start = chain_bytes(width, height, level);
        let (pw, ph) = level_size(width, height, level - 1);
        let (head, tail) = texels.split_at_mut(next_start);
        let (nw, nh) = level_size(width, height, level);
        halve(
            &head[previous_start..],
            pw,
            ph,
            &mut tail[..(nw as usize) * (nh as usize) * 4],
            srgb,
        );
    }
    TextureChain {
        format: ChainFormat::Rgba8,
        width,
        height,
        level_count,
        bytes: texels,
    }
}

/// The full chain of a `width` x `height` RGBA8 image, built in one allocation sized for every level.
pub fn box_filtered_chain(base: Vec<u8>, width: u32, height: u32, srgb: bool) -> TextureChain {
    complete_chain(base, width, height, 1, srgb)
}

/// The WebGPU renderer's levels below `base`'s RGBA8 base, each with its size, down to 1x1; the base stays the
/// caller's, so a chain can share it instead of copying it. None for an empty or short base.
pub fn mips_below_base(base: &CpuTexture, srgb: bool) -> Vec<(u32, u32, Vec<u8>)> {
    halvings_below(base.width, base.height, 0, &base.rgba8, srgb)
}

/// The levels below level `level` of a `width` x `height` RGBA8 chain, `texels`, each halved from the one above down
/// to 1x1 as [`complete_chain`] halves them; none when `texels` is not that level's size.
pub fn halvings_below(
    width: u32,
    height: u32,
    level: u32,
    texels: &[u8],
    srgb: bool,
) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let (lw, lh) = level_size(width, height, level);
    if width == 0 || height == 0 || texels.len() != chain_bytes(lw, lh, 1) {
        return levels;
    }
    for next_level in level + 1..full_level_count(width, height) {
        let (pw, ph) = level_size(width, height, next_level - 1);
        let (nw, nh) = level_size(width, height, next_level);
        let mut next = vec![0; (nw as usize) * (nh as usize) * 4];
        let previous = levels.last().map_or(texels, |(_, _, texels)| texels);
        halve(previous, pw, ph, &mut next, srgb);
        levels.push((nw, nh, next));
    }
    levels
}

/// `stored` RGBA8 levels of a `width` x `height` chain, each with its size, completed down to 1x1 by halving the last:
/// [`complete_chain`]'s levels for a consumer that keeps them apart. A level of the wrong size stops the completion.
pub fn completed_levels(
    width: u32,
    height: u32,
    stored: Vec<Vec<u8>>,
    srgb: bool,
) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = stored
        .into_iter()
        .zip(0..full_level_count(width, height))
        .map(|(texels, level)| {
            let (w, h) = level_size(width, height, level);
            (w, h, texels)
        })
        .collect();
    let whole = levels
        .iter()
        .all(|(w, h, texels)| texels.len() == chain_bytes(*w, *h, 1));
    if let Some((_, _, last)) = levels.last().filter(|_| whole) {
        let below = halvings_below(width, height, levels.len() as u32 - 1, last, srgb);
        levels.extend(below);
    }
    levels
}

/// A KTX2's chain: Basis transcoded to `blocks`' target or straight to RGBA8 (not through BC7), BCn kept as stored where
/// `blocks` samples it or decoded; an RGBA8 chain is completed by halving, a block chain keeps the levels it has. The
/// header's size is capped before anything is transcoded.
pub fn ktx2_chain(bytes: &[u8], srgb: bool, blocks: &BlockSupport) -> anyhow::Result<TextureChain> {
    if let Some(basis) = crate::basis::levels_of_ktx2(bytes, blocks) {
        let (format, (width, height, levels)) = basis?;
        if let ChainFormat::Blocks(format) = format {
            return Ok(TextureChain::of_blocks(format, width, height, &levels));
        }
        let stored = levels.len() as u32;
        return Ok(complete_chain(
            pack(&levels, width, height),
            width,
            height,
            stored,
            srgb,
        ));
    }
    let texture = crate::ktx2_loader::load_ktx2(bytes)?;
    if let Some(compressed) = &texture.compressed {
        let (width, height, levels) = (texture.width, texture.height, compressed.mips.len() as u32);
        if blocks.uploads_blocks(compressed.format, width, height, levels) {
            return Ok(TextureChain::of_blocks(
                compressed.format,
                width,
                height,
                &compressed.mips,
            ));
        }
    }
    rgba8_chain(texture, srgb)
}

/// `texture`'s full RGBA8 chain: a decoded image box-filtered, a Basis payload transcoded level by level, BCn decoded up
/// to its first level too short to decode, the rest halved from the last; an error when not even level 0 decodes.
pub fn rgba8_chain(texture: CpuTexture, srgb: bool) -> anyhow::Result<TextureChain> {
    let CpuTexture {
        width,
        height,
        rgba8,
        compressed,
    } = texture;
    let Some(compressed) = compressed else {
        anyhow::ensure!(
            width > 0 && height > 0 && rgba8.len() == chain_bytes(width, height, 1),
            "a {width}x{height} image with {} RGBA8 bytes",
            rgba8.len()
        );
        return Ok(box_filtered_chain(rgba8, width, height, srgb));
    };
    let levels = match &compressed.basis_source {
        Some(source) => crate::basis::rgba8_levels(source)?,
        None => decoded_levels(width, height, &compressed),
    };
    anyhow::ensure!(
        !levels.is_empty(),
        "level 0 of the {width}x{height} {:?} texture does not decode",
        compressed.format
    );
    let stored = levels.len() as u32;
    Ok(complete_chain(
        pack(&levels, width, height),
        width,
        height,
        stored,
        srgb,
    ))
}

/// `levels` back to back in a buffer with room for the full chain, so completing it does not reallocate.
fn pack(levels: &[Vec<u8>], width: u32, height: u32) -> Vec<u8> {
    let mut texels =
        Vec::with_capacity(chain_bytes(width, height, full_level_count(width, height)));
    for level in levels {
        texels.extend_from_slice(level);
    }
    texels
}

/// The BCn levels as RGBA8, up to the first one too short to decode or past a full chain.
pub fn decoded_levels(width: u32, height: u32, compressed: &CompressedTexture) -> Vec<Vec<u8>> {
    compressed
        .mips
        .iter()
        .zip(0..full_level_count(width, height))
        .map_while(|(data, level)| {
            let (w, h) = level_size(width, height, level);
            crate::bc_decode::decode_level(compressed.format, w, h, data)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::CompressedFormat;

    fn image(width: u32, height: u32, rgba8: Vec<u8>) -> CpuTexture {
        CpuTexture {
            width,
            height,
            rgba8,
            compressed: None,
        }
    }

    #[test]
    fn a_chain_halves_down_to_one_by_one_and_packs_every_level() {
        let chain = box_filtered_chain(vec![255; 8 * 2 * 4], 8, 2, false);
        assert_eq!(chain.level_count, 4);
        let sizes: Vec<_> = chain.levels().map(|(w, h, t)| (w, h, t.len())).collect();
        assert_eq!(sizes, [(8, 2, 64), (4, 1, 16), (2, 1, 8), (1, 1, 4)]);
        assert_eq!(chain.bytes.len(), 64 + 16 + 8 + 4);
        assert!(chain.bytes.iter().all(|&b| b == 255));
    }

    #[test]
    fn srgb_averages_in_linear_space_and_alpha_and_data_round_half_up() {
        let base = vec![0, 0, 0, 0, 255, 255, 255, 1, 0, 0, 0, 0, 255, 255, 255, 1];
        let srgb = box_filtered_chain(base.clone(), 2, 2, true);
        let top = &srgb.bytes[16..];
        // Half of linear white is 0.5, which sRGB encodes as 188.
        assert_eq!(top, [188, 188, 188, 1]);
        let data = box_filtered_chain(base, 2, 2, false);
        assert_eq!(&data.bytes[16..], [128, 128, 128, 1]);
    }

    #[test]
    fn the_mips_below_a_base_match_the_packed_chain() {
        let base = image(3, 3, (0..36).collect());
        let below = mips_below_base(&base, true);
        let chain = box_filtered_chain(base.rgba8.clone(), 3, 3, true);
        let packed: Vec<u8> = base
            .rgba8
            .iter()
            .copied()
            .chain(below.iter().flat_map(|(_, _, t)| t.clone()))
            .collect();
        assert_eq!(packed, chain.bytes);
        assert_eq!(below.len(), 1, "3x3 then 1x1, the base not among them");
        assert_eq!((below[0].0, below[0].1), (1, 1));
        assert!(mips_below_base(&image(1, 1, vec![0; 4]), true).is_empty());
        assert!(mips_below_base(&image(2, 2, vec![0; 3]), true).is_empty());
    }

    #[test]
    fn a_block_level_holds_its_whole_blocks() {
        let bc7 = ChainFormat::Blocks(CompressedFormat::Bc7RgbaUnorm);
        let bc1 = ChainFormat::Blocks(CompressedFormat::Bc1RgbaUnorm);
        assert_eq!(ChainFormat::Rgba8.level_bytes(3, 2), 24);
        assert_eq!(bc7.level_bytes(8, 4), 32);
        assert_eq!(bc7.level_bytes(2, 1), 16, "a level below 4x4 is one block");
        assert_eq!(bc1.level_bytes(6, 6), 32);
        assert_eq!(bc7.chain_bytes(32, 32, 6), 1024 + 256 + 64 + 16 + 16 + 16);
        let chain = TextureChain::of_blocks(
            CompressedFormat::Bc1RgbaUnorm,
            8,
            4,
            &[vec![1; 16], vec![2; 9], vec![3; 8]],
        );
        let levels: Vec<_> = chain.levels().map(|(w, h, b)| (w, h, b.to_vec())).collect();
        assert_eq!(
            levels,
            [(8, 4, vec![1; 16]), (4, 2, vec![2; 8]), (2, 1, vec![3; 8])],
            "a level's bytes past its blocks are cut"
        );
        assert_eq!(chain.format, bc1);
    }

    #[test]
    fn blocks_upload_as_stored_only_where_sampled_whole_and_within_the_side() {
        use CompressedFormat as F;
        let bc = BlockSupport {
            bc: true,
            ..BlockSupport::default()
        };
        assert!(bc.uploads_blocks(F::Bc7RgbaUnorm, 32, 32, 6));
        assert!(bc.uploads_blocks(F::Bc5RgUnorm, 8, 4, 1));
        assert!(
            !bc.uploads_blocks(F::Etc2Rgba8Unorm, 32, 32, 6),
            "ETC2 not sampled"
        );
        assert!(
            !bc.uploads_blocks(F::Bc7RgbaUnorm, 6, 8, 2),
            "a base of partial blocks"
        );
        assert!(
            !bc.uploads_blocks(F::Bc7RgbaUnorm, 18, 16, 2),
            "18 wide is no whole block, though its 9x8 level would not be either"
        );
        let limited = BlockSupport { max_side: 16, ..bc };
        assert!(
            limited.uploads_blocks(F::Bc7RgbaUnorm, 64, 64, 3),
            "64, 32, then 16 fits"
        );
        assert!(
            !limited.uploads_blocks(F::Bc7RgbaUnorm, 64, 64, 2),
            "64 and 32 only"
        );
        assert_eq!(first_block_base(64, 64, 7, 16), Some(2));
        assert_eq!(
            first_block_base(64, 64, 7, 2),
            None,
            "4x4 is the last whole block"
        );
        assert_eq!(first_block_base(64, 64, 7, 0), Some(0), "0 is no limit");
        assert!(!BlockSupport::default().uploads_blocks(F::Bc7RgbaUnorm, 32, 32, 6));
        assert_eq!(BlockSupport::default().basis_target(), None);
        let all = BlockSupport {
            bc: true,
            etc2: true,
            astc: true,
            max_side: 0,
        };
        assert_eq!(all.basis_target(), Some(F::Bc7RgbaUnorm));
        assert!(all.samples(F::Astc4x4RgbaUnorm) && all.samples(F::Bc1RgbaUnorm));
    }

    #[test]
    fn a_stored_bcn_ktx2_keeps_its_blocks_where_sampled_and_decodes_elsewhere() {
        let file = include_bytes!("../tests/assets/red_bc1.ktx2");
        let bc = BlockSupport {
            bc: true,
            ..BlockSupport::default()
        };
        let stored = ktx2_chain(file, true, &bc).unwrap();
        let loaded = crate::ktx2_loader::load_ktx2(file)
            .unwrap()
            .compressed
            .unwrap();
        assert_eq!(
            stored.format,
            ChainFormat::Blocks(CompressedFormat::Bc1RgbaUnorm)
        );
        assert_eq!(
            (stored.width, stored.height, stored.level_count),
            (8, 8, 1),
            "its one level, not halved"
        );
        assert_eq!(stored.bytes, loaded.mips[0][..32]);
        let decoded = ktx2_chain(file, true, &BlockSupport::default()).unwrap();
        assert_eq!(decoded.format, ChainFormat::Rgba8);
        assert_eq!(decoded.level_count, 4, "decoded, then halved down to 1x1");
        let etc2_only = BlockSupport {
            etc2: true,
            ..BlockSupport::default()
        };
        assert_eq!(
            ktx2_chain(file, true, &etc2_only).unwrap().format,
            ChainFormat::Rgba8
        );
    }

    #[test]
    fn an_empty_or_short_image_is_an_error_not_a_panic() {
        assert!(rgba8_chain(image(0, 4, Vec::new()), false).is_err());
        assert!(rgba8_chain(image(2, 2, vec![0; 15]), false).is_err());
        let one = box_filtered_chain(Vec::new(), 0, 0, false);
        assert_eq!((one.level_count, one.bytes.len()), (1, 0));
    }

    #[test]
    fn bc_levels_decode_until_one_is_too_short_and_the_rest_are_halved() {
        // BC1 with both endpoints white: every texel opaque white.
        let block = [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0];
        let compressed = CompressedTexture {
            format: CompressedFormat::Bc1RgbaUnorm,
            mips: vec![block.repeat(4), block.to_vec(), Vec::new()],
            declared_srgb: None,
            basis_source: None,
        };
        let texture = CpuTexture {
            width: 8,
            height: 8,
            rgba8: Vec::new(),
            compressed: Some(compressed),
        };
        let chain = rgba8_chain(texture, true).unwrap();
        assert_eq!(
            chain.level_count, 4,
            "two decoded levels, then 2x2 and 1x1 halved"
        );
        assert_eq!(chain.bytes.len(), (64 + 16 + 4 + 1) * 4);
        assert!(chain.bytes.iter().all(|&b| b == 255));
    }

    #[test]
    fn a_short_chain_is_completed_from_its_last_stored_level() {
        // A 4x4 base of 0 and a 2x2 level of (0, 255) per row: the 1x1 halving must come from the 2x2, not the base.
        let mut texels = vec![0u8; 64];
        texels.extend([
            0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255,
        ]);
        let chain = complete_chain(texels, 4, 4, 2, false);
        assert_eq!(chain.level_count, 3);
        assert_eq!(chain.bytes.len(), (16 + 4 + 1) * 4);
        assert_eq!(chain.bytes[80..], [128, 128, 128, 128]);
        let full = complete_chain(vec![7; 4], 1, 1, 1, true);
        assert_eq!(
            (full.level_count, full.bytes.len()),
            (1, 4),
            "a 1x1 base is already full"
        );
    }

    #[test]
    fn completed_levels_are_the_completed_chain_s_levels_kept_apart() {
        let base: Vec<u8> = (0..12 * 6 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let second: Vec<u8> = (0..6 * 3 * 4).map(|i| (i * 13 % 256) as u8).collect();
        for srgb in [false, true] {
            let packed = complete_chain([base.clone(), second.clone()].concat(), 12, 6, 2, srgb);
            let apart = completed_levels(12, 6, vec![base.clone(), second.clone()], srgb);
            let packed: Vec<_> = packed
                .levels()
                .map(|(w, h, t)| (w, h, t.to_vec()))
                .collect();
            assert_eq!(apart, packed, "srgb {srgb}");
            assert_eq!(apart.len(), 4, "12x6, 6x3, then 3x1 and 1x1 halved");
        }
        let short = completed_levels(8, 8, vec![vec![0; 8 * 8 * 4], vec![0; 3]], false);
        assert_eq!(
            short.iter().map(|(w, h, _)| (*w, *h)).collect::<Vec<_>>(),
            [(8, 8), (4, 4)],
            "a level of the wrong size is not halved from"
        );
        let past_full = completed_levels(2, 2, vec![vec![1; 16], vec![2; 4], vec![3; 4]], false);
        assert_eq!(past_full.len(), 2, "no level past 1x1");
        let below = halvings_below(8, 4, 1, &[255; 4 * 2 * 4], true);
        assert_eq!(
            below.iter().map(|(w, h, _)| (*w, *h)).collect::<Vec<_>>(),
            [(2, 1), (1, 1)]
        );
        assert!(halvings_below(8, 4, 1, &[255; 3], true).is_empty());
    }

    #[test]
    fn sizes_past_thirty_two_levels_do_not_overflow_the_shift() {
        assert_eq!(level_size(5, 3, 40), (1, 1));
        assert_eq!(chain_bytes(2, 2, 40), 16 + 4 * 39);
    }

    #[test]
    fn full_level_count_counts_the_longer_side() {
        assert_eq!(full_level_count(1, 1), 1);
        assert_eq!(full_level_count(2048, 16), 12);
        assert_eq!(full_level_count(5, 3), 3);
        assert_eq!(full_level_count(0, 0), 1);
    }
}
