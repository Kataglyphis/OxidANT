//! Basis Universal (ETC1S, UASTC) KTX2 transcoding, in pure Rust so the wasm32 demo shares it.

use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};

use crate::scene::{CompressedFormat, CompressedTexture, CpuTexture};

/// Whether a KTX2 carries Basis payloads: BasisLZ (ETC1S) or a UASTC data format descriptor.
pub(crate) fn is_basis(reader: &ktx2::Reader<&[u8]>) -> bool {
    reader.header().supercompression_scheme == Some(ktx2::SupercompressionScheme::BasisLZ)
        || matches!(
            reader.color_model(),
            Some(ktx2::ColorModel::ETC1S | ktx2::ColorModel::UASTC)
        )
}

/// Transcodes to a BC7 mip chain, or to RGBA8 when the base is not whole 4x4 blocks.
/// BC7 rather than a per-adapter target: the upload decodes BC7 on the CPU for an adapter without BC.
pub(crate) fn transcode_basis(reader: &ktx2::Reader<&[u8]>) -> anyhow::Result<CpuTexture> {
    let declared_srgb = match reader.transfer_function() {
        Some(ktx2::TransferFunction::SRGB) => Some(true),
        Some(ktx2::TransferFunction::Linear) => Some(false),
        _ => None,
    };
    let transcoder = Transcoder::new(reader.data())
        .map_err(|err| anyhow::anyhow!("not a Basis KTX2: {err:?}"))?;
    let source = transcoder.source_format();
    anyhow::ensure!(
        matches!(source, SourceFormat::Etc1s | SourceFormat::UastcLdr),
        "Basis source {source:?} is not supported, only ETC1S and UASTC LDR"
    );
    anyhow::ensure!(
        !transcoder.is_video(),
        "Basis video textures are not supported"
    );
    let (width, height) = transcoder.base_dimensions();
    let level = |index: u32, target: TargetFormat| {
        transcoder
            .transcode(index, target, DecodeFlags::NONE)
            .map_err(|err| anyhow::anyhow!("Basis level {index} to {target:?}: {err:?}"))
    };
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
        return Ok(CpuTexture {
            width,
            height,
            rgba8: level(0, TargetFormat::Rgba32)?,
            compressed: None,
        });
    }
    let mips = (0..transcoder.level_count())
        .map(|index| level(index, TargetFormat::Bc7Rgba))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let format = CompressedFormat::Bc7RgbaUnorm;
    super::ktx2_loader::validate_mip_chain(width, height, format, &mips)?;
    Ok(CpuTexture {
        width,
        height,
        rgba8: Vec::new(),
        compressed: Some(CompressedTexture {
            format,
            mips,
            declared_srgb,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::bc_decode::decode_level;
    use crate::asset::ktx2_loader::load_ktx2;

    /// A UASTC mode 8 (solid colour) block: the 5-bit mode code 0x17, then R, G, B, A; ETC1 hints stay zero.
    fn solid_uastc_block(rgba: [u8; 4]) -> [u8; 16] {
        let bits = 0x17u64 | (u64::from(u32::from_le_bytes(rgba)) << 5);
        let mut block = [0u8; 16];
        block[..8].copy_from_slice(&bits.to_le_bytes());
        block
    }

    /// An uncompressed UASTC KTX2 with one sRGB RGB sample; `levels` hold whole blocks, level 0 first.
    fn uastc_ktx2(width: u32, height: u32, levels: &[Vec<[u8; 4]>]) -> Vec<u8> {
        let dfd: [u32; 11] = [
            44,
            0,
            2 | (40 << 16),
            166 | (1 << 8) | (2 << 16),
            0x0303,
            16,
            0,
            127 << 16,
            0,
            0,
            u32::MAX,
        ];
        let dfd_offset = 80 + 24 * levels.len();
        let mut offset = dfd_offset + 44;
        let mut out = b"\xABKTX 20\xBB\r\n\x1A\n".to_vec();
        for word in [0, 1, width, height, 0, 0, 1, levels.len() as u32, 0] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for word in [dfd_offset as u32, 44, 0, 0] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&[0u8; 16]);
        for level in levels {
            let length = (level.len() * 16) as u64;
            for word in [offset as u64, length, length] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            offset += level.len() * 16;
        }
        for word in dfd {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for colour in levels.iter().flatten() {
            out.extend_from_slice(&solid_uastc_block(*colour));
        }
        out
    }

    /// The colour of block (`x`, `y`) in a ramp: red along x, green along y, blue across both.
    fn ramp(x: u32, y: u32) -> [u8; 4] {
        [(x * 60) as u8, (y * 60) as u8, ((x + y) * 30) as u8, 255]
    }

    fn ramp_level(blocks_wide: u32, blocks_high: u32) -> Vec<[u8; 4]> {
        (0..blocks_high)
            .flat_map(|y| (0..blocks_wide).map(move |x| ramp(x, y)))
            .collect()
    }

    /// Mean and max absolute channel error of `rgba8` against the ramp it was encoded from.
    fn ramp_error(width: u32, height: u32, rgba8: &[u8]) -> (f64, u8) {
        texel_error(width, height, rgba8, |x, y| ramp(x / 4, y / 4))
    }

    /// Mean and max absolute channel error of `rgba8` against `expected` per texel.
    fn texel_error(
        width: u32,
        height: u32,
        rgba8: &[u8],
        expected: impl Fn(u32, u32) -> [u8; 4],
    ) -> (f64, u8) {
        assert_eq!(rgba8.len(), (width * height * 4) as usize);
        let mut sum = 0.0;
        let mut max = 0u8;
        for (index, texel) in rgba8.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            for (got, want) in texel.iter().zip(expected(x, y)) {
                let diff = got.abs_diff(want);
                sum += f64::from(diff);
                max = max.max(diff);
            }
        }
        (sum / rgba8.len() as f64, max)
    }

    #[test]
    fn uastc_transcodes_to_a_bc7_chain_that_decodes_to_the_source() {
        let file = uastc_ktx2(
            16,
            16,
            &[
                ramp_level(4, 4),
                ramp_level(2, 2),
                ramp_level(1, 1),
                ramp_level(1, 1),
                ramp_level(1, 1),
            ],
        );
        let texture = load_ktx2(&file).expect("a UASTC KTX2 must transcode");
        assert_eq!((texture.width, texture.height), (16, 16));
        let compressed = texture.compressed.expect("a 16x16 base transcodes to BC7");
        assert_eq!(compressed.format, CompressedFormat::Bc7RgbaUnorm);
        assert_eq!(compressed.mips.len(), 5);
        assert_eq!(compressed.declared_srgb, Some(true));
        let rgba8 =
            decode_level(compressed.format, 16, 16, &compressed.mips[0]).expect("BC7 decodes");
        let (mean, max) = ramp_error(16, 16, &rgba8);
        assert!(
            mean < 1.0 && max <= 4,
            "BC7 from UASTC is {mean:.3} off on average, {max} at most"
        );
    }

    #[test]
    fn a_base_that_is_not_whole_blocks_transcodes_to_rgba8() {
        let file = uastc_ktx2(6, 6, &[ramp_level(2, 2)]);
        let texture = load_ktx2(&file).expect("a 6x6 UASTC KTX2 must transcode");
        assert!(texture.compressed.is_none(), "wgpu takes no 6x6 BC texture");
        assert_eq!(texture.rgba8.len(), 6 * 6 * 4);
        let (mean, max) = ramp_error(6, 6, &texture.rgba8);
        assert!(
            mean == 0.0 && max == 0,
            "a solid UASTC block decodes exactly, got {mean:.3} / {max}"
        );
    }

    #[test]
    fn a_truncated_basis_level_errors_instead_of_panicking() {
        let mut file = uastc_ktx2(8, 8, &[ramp_level(2, 2)]);
        // The level index still claims 64 bytes; the reader must refuse the short file.
        file.truncate(file.len() - 16);
        assert!(load_ktx2(&file).is_err());
    }

    #[test]
    fn a_block_with_no_uastc_mode_errors_instead_of_rendering_garbage() {
        let mut file = uastc_ktx2(8, 8, &[ramp_level(2, 2)]);
        let first_block = file.len() - 4 * 16;
        // 0x45 is the reserved mode code past UASTC's 19 modes.
        file[first_block] = 0x45;
        let err = load_ktx2(&file).expect_err("an undecodable UASTC block must fail the load");
        assert!(
            format!("{err:#}").contains("Basis"),
            "the error must name Basis, got: {err:#}"
        );
    }

    #[test]
    fn the_green_uastc_fixture_is_this_builder_s_output() {
        let green = vec![[0, 255, 0, 255]; 4];
        let built = uastc_ktx2(
            8,
            8,
            &[
                green,
                vec![[0, 255, 0, 255]],
                vec![[0, 255, 0, 255]],
                vec![[0, 255, 0, 255]],
            ],
        );
        assert_eq!(built, include_bytes!("../../tests/assets/green_uastc.ktx2"));
    }

    /// The 32x32 source of `ramp_etc1s.ktx2` and `ramp_uastc.ktx2`.
    fn encoded_ramp(x: u32, y: u32) -> [u8; 4] {
        [
            (x * 255 / 31) as u8,
            (y * 255 / 31) as u8,
            (255 - (x + y) * 255 / 62) as u8,
            255,
        ]
    }

    /// Encoder-made files: basis-universal 0.3.1 (basisu 1.16) at default quality, repacked from .basis into KTX2.
    #[test]
    fn encoded_etc1s_and_uastc_transcode_close_to_their_source() {
        let fixtures: [(&str, &[u8], f64); 2] = [
            (
                "ETC1S",
                include_bytes!("../../tests/assets/ramp_etc1s.ktx2"),
                10.0,
            ),
            (
                "UASTC",
                include_bytes!("../../tests/assets/ramp_uastc.ktx2"),
                2.0,
            ),
        ];
        for (name, file, bound) in fixtures {
            let texture = load_ktx2(file).unwrap_or_else(|err| panic!("{name}: {err:#}"));
            let compressed = texture.compressed.expect("a 32x32 base transcodes to BC7");
            assert_eq!(compressed.mips.len(), 6, "{name}: the encoder's full chain");
            let bc7 =
                decode_level(compressed.format, 32, 32, &compressed.mips[0]).expect("BC7 decodes");
            let (bc7_mean, bc7_max) = texel_error(32, 32, &bc7, encoded_ramp);
            let rgba = Transcoder::new(file)
                .and_then(|t| t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE))
                .expect("RGBA32 transcode");
            let (rgba_mean, rgba_max) = texel_error(32, 32, &rgba, encoded_ramp);
            eprintln!("{name}: BC7 mean {bc7_mean:.3} max {bc7_max}, RGBA32 mean {rgba_mean:.3} max {rgba_max}");
            // A negative control: the same texels against the ramp's transpose must miss by far.
            let (swapped, _) = texel_error(32, 32, &bc7, |x, y| encoded_ramp(y, x));
            assert!(
                swapped > 4.0 * bound,
                "{name}: the transposed ramp is only {swapped:.3} off"
            );
            assert!(
                bc7_mean < bound && rgba_mean < bound,
                "{name}: {bc7_mean:.3} / {rgba_mean:.3} over {bound}"
            );
        }
    }
}
