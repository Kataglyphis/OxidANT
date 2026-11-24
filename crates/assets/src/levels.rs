//! A texture's RGBA8 mip chain, built once for both renderers: a 2x2 box filter for a decoded image (sRGB averaged in
//! linear space), Basis levels transcoded straight to RGBA8, and BCn levels decoded.

use crate::scene::{CompressedTexture, CpuTexture};

/// A mip chain, level 0 first, packed back to back: level `i` is `max(width >> i, 1)` by `max(height >> i, 1)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rgba8Chain {
    pub width: u32,
    pub height: u32,
    pub level_count: u32,
    pub texels: Vec<u8>,
}

impl Rgba8Chain {
    /// Each level's width, height and texels, up to the first the texels do not hold.
    pub fn levels(&self) -> impl Iterator<Item = (u32, u32, &[u8])> {
        let mut rest = self.texels.as_slice();
        (0..self.level_count).map_while(move |level| {
            let (w, h) = level_size(self.width, self.height, level);
            let (texels, tail) = rest.split_at_checked((w as usize) * (h as usize) * 4)?;
            rest = tail;
            Some((w, h, texels))
        })
    }
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
) -> Rgba8Chain {
    let level_count = full_level_count(width, height);
    if width == 0
        || height == 0
        || stored == 0
        || texels.len() != chain_bytes(width, height, stored)
    {
        return Rgba8Chain {
            width,
            height,
            level_count: stored,
            texels,
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
    Rgba8Chain {
        width,
        height,
        level_count,
        texels,
    }
}

/// The full chain of a `width` x `height` RGBA8 image, built in one allocation sized for every level.
pub fn box_filtered_chain(base: Vec<u8>, width: u32, height: u32, srgb: bool) -> Rgba8Chain {
    complete_chain(base, width, height, 1, srgb)
}

/// `generate_mips`' shape for the WebGPU renderer: each level's size and its own texels, the base copied once.
pub fn generate_mips(base: &CpuTexture, srgb: bool) -> Vec<(u32, u32, Vec<u8>)> {
    let (width, height) = (base.width, base.height);
    let mut levels = vec![(width, height, base.rgba8.clone())];
    if width == 0 || height == 0 || base.rgba8.len() != chain_bytes(width, height, 1) {
        return levels;
    }
    for level in 1..full_level_count(width, height) {
        let (pw, ph) = level_size(width, height, level - 1);
        let (nw, nh) = level_size(width, height, level);
        let mut next = vec![0; (nw as usize) * (nh as usize) * 4];
        if let Some((_, _, previous)) = levels.last() {
            halve(previous, pw, ph, &mut next, srgb);
        }
        levels.push((nw, nh, next));
    }
    levels
}

/// A KTX2's full RGBA8 chain: Basis transcoded straight to RGBA8 (not through BC7), BCn decoded, either completed by
/// halving; the header's size is capped before anything is transcoded.
pub fn ktx2_chain(bytes: &[u8], srgb: bool) -> anyhow::Result<Rgba8Chain> {
    if let Some(basis) = crate::basis::rgba8_chain_of_ktx2(bytes) {
        let (width, height, levels) = basis?;
        let stored = levels.len() as u32;
        return Ok(complete_chain(
            pack(&levels, width, height),
            width,
            height,
            stored,
            srgb,
        ));
    }
    rgba8_chain(crate::ktx2_loader::load_ktx2(bytes)?, srgb)
}

/// `texture`'s full RGBA8 chain: a decoded image box-filtered, a Basis payload transcoded level by level, BCn decoded up
/// to its first level too short to decode, the rest halved from the last; an error when not even level 0 decodes.
pub fn rgba8_chain(texture: CpuTexture, srgb: bool) -> anyhow::Result<Rgba8Chain> {
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
fn decoded_levels(width: u32, height: u32, compressed: &CompressedTexture) -> Vec<Vec<u8>> {
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
        assert_eq!(chain.texels.len(), 64 + 16 + 8 + 4);
        assert!(chain.texels.iter().all(|&b| b == 255));
    }

    #[test]
    fn srgb_averages_in_linear_space_and_alpha_and_data_round_half_up() {
        let base = vec![0, 0, 0, 0, 255, 255, 255, 1, 0, 0, 0, 0, 255, 255, 255, 1];
        let srgb = box_filtered_chain(base.clone(), 2, 2, true);
        let top = &srgb.texels[16..];
        // Half of linear white is 0.5, which sRGB encodes as 188.
        assert_eq!(top, [188, 188, 188, 1]);
        let data = box_filtered_chain(base, 2, 2, false);
        assert_eq!(&data.texels[16..], [128, 128, 128, 1]);
    }

    #[test]
    fn generate_mips_matches_the_packed_chain() {
        let base = image(3, 3, (0..36).collect());
        let levels = generate_mips(&base, true);
        let chain = box_filtered_chain(base.rgba8.clone(), 3, 3, true);
        let packed: Vec<u8> = levels.iter().flat_map(|(_, _, t)| t.clone()).collect();
        assert_eq!(packed, chain.texels);
        assert_eq!(levels.len(), 2);
        assert_eq!((levels[1].0, levels[1].1), (1, 1));
    }

    #[test]
    fn an_empty_or_short_image_is_an_error_not_a_panic() {
        assert!(rgba8_chain(image(0, 4, Vec::new()), false).is_err());
        assert!(rgba8_chain(image(2, 2, vec![0; 15]), false).is_err());
        let one = box_filtered_chain(Vec::new(), 0, 0, false);
        assert_eq!((one.level_count, one.texels.len()), (1, 0));
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
        assert_eq!(chain.texels.len(), (64 + 16 + 4 + 1) * 4);
        assert!(chain.texels.iter().all(|&b| b == 255));
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
        assert_eq!(chain.texels.len(), (16 + 4 + 1) * 4);
        assert_eq!(chain.texels[80..], [128, 128, 128, 128]);
        let full = complete_chain(vec![7; 4], 1, 1, 1, true);
        assert_eq!(
            (full.level_count, full.texels.len()),
            (1, 4),
            "a 1x1 base is already full"
        );
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
