//! Zstandard-supercompressed UASTC KTX2, inflated here so basisu builds without its zstd feature (ruzstd 0.7).

use std::io::Read as _;

use anyhow::Context as _;

const HEADER_BYTES: usize = 80;
const LEVEL_ENTRY_BYTES: usize = 24;
const SUPERCOMPRESSION_OFFSET: usize = 44;
const UASTC_BLOCK_BYTES: u64 = 16;
/// KTX2 aligns uncompressed UASTC levels to lcm(16, 4) bytes.
const LEVEL_ALIGNMENT: usize = 16;
/// All levels together, inflated: a 16384x16384 UASTC chain needs 341 MiB.
pub(crate) const MAX_INFLATED_BYTES: u64 = 512 << 20;
/// The smallest window cap a frame may declare, however small its level.
const MIN_WINDOW_BYTES: u64 = 1 << 20;

/// `reader`'s file with every level inflated to exactly its blocks' size and the scheme set to none.
pub(crate) fn inflate_uastc_levels(reader: &ktx2::Reader<&[u8]>) -> anyhow::Result<Vec<u8>> {
    let bytes = reader.data();
    let header = reader.header();
    let entries = (0..header.level_count.max(1) as usize)
        .map(|level| level_entry(bytes, level))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let wants = level_sizes(header.pixel_width, header.pixel_height, entries.len())?;
    let data_start = entries.iter().map(|entry| entry.0).min().unwrap_or(0);
    anyhow::ensure!(
        data_start >= HEADER_BYTES + entries.len() * LEVEL_ENTRY_BYTES,
        "KTX2 level data overlaps the level index"
    );
    let mut out = bytes
        .get(..data_start)
        .context("KTX2 level data starts past the file")?
        .to_vec();
    out[SUPERCOMPRESSION_OFFSET..SUPERCOMPRESSION_OFFSET + 4].copy_from_slice(&0u32.to_le_bytes());
    for (level, ((offset, length, declared), want)) in entries.into_iter().zip(wants).enumerate() {
        anyhow::ensure!(
            declared == want,
            "KTX2 level {level} declares {declared} uncompressed bytes, its blocks need {want}"
        );
        let compressed = offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
            .with_context(|| format!("KTX2 level {level} lies past the file"))?;
        out.resize(out.len().next_multiple_of(LEVEL_ALIGNMENT), 0);
        let entry = HEADER_BYTES + level * LEVEL_ENTRY_BYTES;
        for (slot, value) in [out.len() as u64, want, want].into_iter().enumerate() {
            out[entry + slot * 8..entry + slot * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        inflate_into(compressed, want, &mut out)
            .with_context(|| format!("KTX2 level {level} Zstandard data"))?;
    }
    Ok(out)
}

/// Byte offset, byte length and uncompressed length of `level` in the level index.
fn level_entry(bytes: &[u8], level: usize) -> anyhow::Result<(usize, usize, u64)> {
    let start = HEADER_BYTES + level * LEVEL_ENTRY_BYTES;
    let entry = bytes
        .get(start..start + LEVEL_ENTRY_BYTES)
        .context("KTX2 level index is truncated")?;
    let word = |index: usize| {
        u64::from_le_bytes(entry[index * 8..index * 8 + 8].try_into().expect("8 bytes"))
    };
    let to_usize = |value: u64| usize::try_from(value).context("KTX2 level offset overflows");
    Ok((to_usize(word(0))?, to_usize(word(1))?, word(2)))
}

/// Each level's UASTC size, whole 4x4 blocks of 16 bytes, refused past `MAX_INFLATED_BYTES` in total.
pub(crate) fn level_sizes(width: u32, height: u32, levels: usize) -> anyhow::Result<Vec<u64>> {
    let sizes: Vec<u64> = (0..levels.min(32) as u32)
        .map(|level| {
            let side = |extent: u32| u64::from((extent >> level).max(1).div_ceil(4));
            side(width)
                .saturating_mul(side(height))
                .saturating_mul(UASTC_BLOCK_BYTES)
        })
        .collect();
    anyhow::ensure!(
        sizes.len() == levels,
        "KTX2 declares {levels} levels, at most 32 fit"
    );
    let total = sizes
        .iter()
        .fold(0u64, |sum, size| sum.saturating_add(*size));
    anyhow::ensure!(
        total <= MAX_INFLATED_BYTES,
        "a {width}x{height} UASTC chain inflates to {total} bytes, more than {MAX_INFLATED_BYTES}"
    );
    Ok(sizes)
}

/// Appends exactly `want` bytes inflated from one Zstandard frame; any other length is an error.
pub(crate) fn inflate_into(compressed: &[u8], want: u64, out: &mut Vec<u8>) -> anyhow::Result<()> {
    let window = want.next_power_of_two().max(MIN_WINDOW_BYTES);
    let decoder = ruzstd::decoding::StreamingDecoder::new_with_max_window_size(compressed, window)
        .map_err(|err| anyhow::anyhow!("not a Zstandard frame: {err}"))?;
    let start = out.len();
    // One byte past `want` proves an oversized frame without inflating the rest of it.
    decoder
        .take(want + 1)
        .read_to_end(out)
        .map_err(|err| anyhow::anyhow!("corrupt Zstandard data: {err}"))?;
    let got = (out.len() - start) as u64;
    anyhow::ensure!(
        got == want,
        "inflated to {} bytes, the level needs {want}",
        if got > want {
            format!("more than {want}")
        } else {
            got.to_string()
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `blocks` RLE blocks of 128 KiB each, with no content size: 4 bytes in per 128 KiB out.
    fn rle_bomb(blocks: u32) -> Vec<u8> {
        // Magic, then a descriptor with no content size and a window byte of 128 KiB (exponent 7).
        let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 7 << 3];
        for index in 0..blocks {
            let last = u32::from(index + 1 == blocks);
            let block_header = last | (1 << 1) | ((128 << 10) << 3);
            frame.extend_from_slice(&block_header.to_le_bytes()[..3]);
            frame.push(0xAB);
        }
        frame
    }

    #[test]
    fn a_frame_round_trips_to_exactly_its_level() {
        let level: Vec<u8> = (0..4096u32).map(|i| (i * 7 % 251) as u8).collect();
        let frame = ruzstd::encoding::compress_to_vec(
            level.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let mut out = vec![1, 2, 3];
        inflate_into(&frame, level.len() as u64, &mut out).expect("a valid frame inflates");
        assert_eq!(&out[..3], &[1, 2, 3]);
        assert_eq!(&out[3..], level.as_slice());
    }

    #[test]
    fn a_bomb_stops_one_byte_past_its_level() {
        // 8 KiB that would inflate to 256 MiB; the level needs 4 KiB.
        let bomb = rle_bomb(2048);
        assert!(bomb.len() < 9 << 10);
        let mut out = Vec::new();
        let err = inflate_into(&bomb, 4096, &mut out).expect_err("a bomb must fail");
        assert!(format!("{err:#}").contains("more than 4096"), "{err:#}");
        assert!(out.len() <= 4097, "inflated {} bytes", out.len());
    }

    #[test]
    fn a_short_frame_and_garbage_fail() {
        let mut out = Vec::new();
        let short = ruzstd::encoding::compress_to_vec(
            [7u8; 100].as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        assert!(inflate_into(&short, 128, &mut out).is_err());
        assert!(inflate_into(b"not zstd at all", 16, &mut Vec::new()).is_err());
        assert!(inflate_into(&[0x28, 0xB5, 0x2F, 0xFD], 16, &mut Vec::new()).is_err());
    }

    #[test]
    fn a_frame_declaring_a_huge_window_is_refused_before_it_allocates() {
        // Window byte exponent 31: a 128 TiB window for a 16-byte level.
        let frame = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 31 << 3, 0x01, 0x00, 0x00];
        assert!(inflate_into(&frame, 16, &mut Vec::new()).is_err());
    }

    #[test]
    fn level_sizes_are_whole_blocks_and_bounded() {
        assert_eq!(level_sizes(6, 6, 3).unwrap(), [64, 16, 16]);
        assert_eq!(level_sizes(16384, 16384, 1).unwrap(), [256 << 20]);
        assert!(level_sizes(32768, 32768, 1).is_err());
        assert!(level_sizes(u32::MAX, u32::MAX, 32).is_err());
    }
}
