//! Basis Universal (ETC1S, UASTC) KTX2 transcoding, in pure Rust so the wasm32 demo shares it.

use std::sync::Arc;

use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};

use crate::scene::{CompressedFormat, CompressedTexture, CpuTexture};

/// Whether a KTX2 carries Basis payloads: BasisLZ (ETC1S) or a UASTC data format descriptor.
pub fn is_basis(reader: &ktx2::Reader<&[u8]>) -> bool {
    reader.header().supercompression_scheme == Some(ktx2::SupercompressionScheme::BasisLZ)
        || matches!(
            reader.color_model(),
            Some(ktx2::ColorModel::ETC1S | ktx2::ColorModel::UASTC)
        )
}

/// Transcodes to BC7, or RGBA8 off whole 4x4 blocks; the adapter is unknown here, so an upload may retarget.
pub fn transcode_basis(reader: &ktx2::Reader<&[u8]>) -> anyhow::Result<CpuTexture> {
    let declared_srgb = match reader.transfer_function() {
        Some(ktx2::TransferFunction::SRGB) => Some(true),
        Some(ktx2::TransferFunction::Linear) => Some(false),
        _ => None,
    };
    let source = basis_source(reader)?;
    let transcoder = basis_transcoder(&source)?;
    let (width, height) = transcoder.base_dimensions();
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
        return Ok(CpuTexture {
            width,
            height,
            rgba8: transcode_level(&transcoder, 0, TargetFormat::Rgba32)?,
            compressed: None,
        });
    }
    let format = CompressedFormat::Bc7RgbaUnorm;
    let mips = transcode_chain(&transcoder, format)?;
    Ok(CpuTexture {
        width,
        height,
        rgba8: Vec::new(),
        compressed: Some(CompressedTexture {
            format,
            mips,
            declared_srgb,
            basis_source: Some(Arc::clone(&source)),
        }),
    })
}

/// The file basisu reads: as is, or with Zstandard-supercompressed UASTC levels inflated here, not in basisu.
fn basis_source(reader: &ktx2::Reader<&[u8]>) -> anyhow::Result<Arc<[u8]>> {
    if reader.header().supercompression_scheme != Some(ktx2::SupercompressionScheme::Zstandard) {
        return Ok(reader.data().into());
    }
    anyhow::ensure!(
        reader.color_model() == Some(ktx2::ColorModel::UASTC),
        "Zstandard KTX2 is supported only for UASTC, not {:?}",
        reader.color_model()
    );
    Ok(super::ktx2_zstd::inflate_uastc_levels(reader)?.into())
}

/// A transcoder over an ETC1S or UASTC LDR still image; other Basis sources are refused by name.
fn basis_transcoder(bytes: &[u8]) -> anyhow::Result<Transcoder<'_>> {
    let transcoder =
        Transcoder::new(bytes).map_err(|err| anyhow::anyhow!("not a Basis KTX2: {err:?}"))?;
    let source = transcoder.source_format();
    anyhow::ensure!(
        matches!(source, SourceFormat::Etc1s | SourceFormat::UastcLdr),
        "Basis source {source:?} is not supported, only ETC1S and UASTC LDR"
    );
    anyhow::ensure!(
        !transcoder.is_video(),
        "Basis video textures are not supported"
    );
    Ok(transcoder)
}

fn transcode_level(
    transcoder: &Transcoder<'_>,
    index: u32,
    target: TargetFormat,
) -> anyhow::Result<Vec<u8>> {
    transcoder
        .transcode(index, target, DecodeFlags::NONE)
        .map_err(|err| anyhow::anyhow!("Basis level {index} to {target:?}: {err:?}"))
}

/// Every level as `format`'s blocks, checked against the base's dimensions.
fn transcode_chain(
    transcoder: &Transcoder<'_>,
    format: CompressedFormat,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let target = match format {
        CompressedFormat::Bc7RgbaUnorm => TargetFormat::Bc7Rgba,
        CompressedFormat::Etc2Rgba8Unorm => TargetFormat::Etc2Rgba,
        CompressedFormat::Astc4x4RgbaUnorm => TargetFormat::Astc4x4Rgba,
        other => anyhow::bail!("Basis does not transcode to {other:?} here"),
    };
    let mips = (0..transcoder.level_count())
        .map(|index| transcode_level(transcoder, index, target))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let (width, height) = transcoder.base_dimensions();
    super::ktx2_loader::validate_mip_chain(width, height, format, &mips)?;
    Ok(mips)
}

/// The Basis target for an adapter: BC7, then ETC2, then ASTC 4x4; `None` decodes BC7 to RGBA8 on the CPU.
pub fn basis_target(bc: bool, etc2: bool, astc: bool) -> Option<CompressedFormat> {
    if bc {
        Some(CompressedFormat::Bc7RgbaUnorm)
    } else if etc2 {
        Some(CompressedFormat::Etc2Rgba8Unorm)
    } else if astc {
        Some(CompressedFormat::Astc4x4RgbaUnorm)
    } else {
        None
    }
}

/// `compressed` transcoded again from its Basis source to `format`; `None` when it has no source or is already `format`.
pub fn retarget_basis(
    compressed: &CompressedTexture,
    format: CompressedFormat,
) -> Option<anyhow::Result<CompressedTexture>> {
    let source = compressed.basis_source.as_ref()?;
    if compressed.format == format {
        return None;
    }
    Some(
        basis_transcoder(source)
            .and_then(|transcoder| transcode_chain(&transcoder, format))
            .map(|mips| CompressedTexture {
                format,
                mips,
                declared_srgb: compressed.declared_srgb,
                basis_source: Some(Arc::clone(source)),
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bc_decode::decode_level;
    use crate::ktx2_loader::load_ktx2;

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

    /// `file` with every level replaced by `compress(level)` and the Zstandard scheme declared.
    fn zstd_levels(file: &[u8], compress: impl Fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
        let word = |at: usize| u64::from_le_bytes(file[at..at + 8].try_into().unwrap()) as usize;
        let levels = u32::from_le_bytes(file[40..44].try_into().unwrap()) as usize;
        let data_start = (0..levels)
            .map(|level| word(80 + level * 24))
            .min()
            .unwrap();
        let mut out = file[..data_start].to_vec();
        out[44..48].copy_from_slice(&2u32.to_le_bytes());
        for level in 0..levels {
            let (offset, length) = (word(80 + level * 24), word(88 + level * 24));
            let frame = compress(&file[offset..offset + length]);
            let entry = 80 + level * 24;
            let at = out.len() as u64;
            out[entry..entry + 8].copy_from_slice(&at.to_le_bytes());
            out[entry + 8..entry + 16].copy_from_slice(&(frame.len() as u64).to_le_bytes());
            out.extend_from_slice(&frame);
        }
        out
    }

    fn zstd(level: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(level, ruzstd::encoding::CompressionLevel::Fastest)
    }

    #[test]
    fn zstd_uastc_transcodes_like_the_uncompressed_file_and_retargets() {
        let plain = uastc_ktx2(
            16,
            16,
            &[ramp_level(4, 4), ramp_level(2, 2), ramp_level(1, 1)],
        );
        let packed = zstd_levels(&plain, zstd);
        assert_eq!(u32::from_le_bytes(packed[44..48].try_into().unwrap()), 2);
        let want = load_ktx2(&plain).unwrap().compressed.unwrap();
        let got = load_ktx2(&packed)
            .expect("a Zstandard UASTC KTX2 must transcode")
            .compressed
            .unwrap();
        assert_eq!(got.mips, want.mips);
        assert_eq!(got.declared_srgb, Some(true));
        let etc2 = retarget_basis(&got, CompressedFormat::Etc2Rgba8Unorm)
            .expect("the inflated file is kept to retarget")
            .unwrap();
        let etc2_plain = retarget_basis(&want, CompressedFormat::Etc2Rgba8Unorm)
            .unwrap()
            .unwrap();
        assert_eq!(etc2.mips, etc2_plain.mips);
    }

    #[test]
    fn a_zstd_level_that_inflates_past_its_blocks_fails_the_load() {
        let plain = uastc_ktx2(8, 8, &[ramp_level(2, 2)]);
        // 64 bytes of blocks, but the frame holds 64 KiB.
        let packed = zstd_levels(&plain, |_| zstd(&[0x17; 64 << 10]));
        let err = load_ktx2(&packed).expect_err("an oversized level must fail");
        assert!(format!("{err:#}").contains("more than 64"), "{err:#}");
    }

    #[test]
    fn a_zstd_level_declaring_a_size_its_blocks_do_not_have_fails_the_load() {
        let plain = uastc_ktx2(8, 8, &[ramp_level(2, 2)]);
        let mut packed = zstd_levels(&plain, zstd);
        // The level index's uncompressed length: a 1 GiB claim for 64 bytes of blocks.
        packed[96..104].copy_from_slice(&(1u64 << 30).to_le_bytes());
        let err = load_ktx2(&packed).expect_err("a wrong uncompressed length must fail");
        assert!(format!("{err:#}").contains("its blocks need 64"), "{err:#}");
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
        assert_eq!(
            built,
            include_bytes!("../../webgpu_renderer/tests/assets/green_uastc.ktx2")
        );
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
                include_bytes!("../../webgpu_renderer/tests/assets/ramp_etc1s.ktx2"),
                10.0,
            ),
            (
                "UASTC",
                include_bytes!("../../webgpu_renderer/tests/assets/ramp_uastc.ktx2"),
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

    #[test]
    fn the_basis_target_prefers_bc7_then_etc2_then_astc_then_the_cpu() {
        use CompressedFormat as F;
        let cases = [
            ((true, true, true), Some(F::Bc7RgbaUnorm)),
            ((true, false, false), Some(F::Bc7RgbaUnorm)),
            ((false, true, true), Some(F::Etc2Rgba8Unorm)),
            ((false, true, false), Some(F::Etc2Rgba8Unorm)),
            ((false, false, true), Some(F::Astc4x4RgbaUnorm)),
            ((false, false, false), None),
        ];
        for ((bc, etc2, astc), expected) in cases {
            assert_eq!(
                basis_target(bc, etc2, astc),
                expected,
                "bc {bc} etc2 {etc2} astc {astc}"
            );
        }
    }

    /// ETC1 intensity modifiers per table: the small and the large step.
    const ETC1_MODIFIERS: [[i32; 2]; 8] = [
        [2, 8],
        [5, 17],
        [9, 29],
        [13, 42],
        [18, 60],
        [24, 80],
        [33, 106],
        [47, 183],
    ];

    /// EAC alpha modifiers per table, from the ETC2 specification.
    const EAC_MODIFIERS: [[i32; 8]; 16] = [
        [-3, -6, -9, -15, 2, 5, 8, 14],
        [-3, -7, -10, -13, 2, 6, 9, 12],
        [-2, -5, -8, -13, 1, 4, 7, 12],
        [-2, -4, -6, -13, 1, 3, 5, 12],
        [-3, -6, -8, -12, 2, 5, 7, 11],
        [-3, -7, -9, -11, 2, 6, 8, 10],
        [-4, -7, -8, -11, 3, 6, 7, 10],
        [-3, -5, -8, -11, 2, 4, 7, 10],
        [-2, -6, -8, -10, 1, 5, 7, 9],
        [-2, -5, -8, -10, 1, 4, 7, 9],
        [-2, -4, -8, -10, 1, 3, 7, 9],
        [-2, -5, -7, -10, 1, 4, 6, 9],
        [-3, -4, -7, -10, 2, 3, 6, 9],
        [-1, -2, -3, -10, 0, 1, 2, 9],
        [-4, -6, -8, -9, 3, 5, 7, 8],
        [-3, -5, -7, -9, 2, 4, 6, 8],
    ];

    /// One ETC2 RGBA8 block, row by row; panics on the T, H and planar modes, which Basis never emits.
    fn etc2_rgba8_block(block: &[u8; 16]) -> [[u8; 4]; 16] {
        let alpha = u64::from_be_bytes(block[..8].try_into().unwrap());
        let color = u64::from_be_bytes(block[8..].try_into().unwrap());
        let bits = |hi: u32, count: u32| ((color >> (hi + 1 - count)) & ((1 << count) - 1)) as i32;
        let differential = bits(33, 1) == 1;
        let flip = bits(32, 1) == 1;
        let bases: [[i32; 3]; 2] = if differential {
            let channel = |hi: u32| {
                let base = bits(hi, 5);
                let delta = (bits(hi - 5, 3) << 29) >> 29;
                assert!(
                    (0..32).contains(&(base + delta)),
                    "an ETC2 T, H or planar block"
                );
                let expand = |v: i32| (v << 3) | (v >> 2);
                [expand(base), expand(base + delta)]
            };
            let (r, g, b) = (channel(63), channel(55), channel(47));
            [[r[0], g[0], b[0]], [r[1], g[1], b[1]]]
        } else {
            let nibble = |hi: u32| bits(hi, 4) * 17;
            [
                [nibble(63), nibble(55), nibble(47)],
                [nibble(59), nibble(51), nibble(43)],
            ]
        };
        let tables = [bits(39, 3) as usize, bits(36, 3) as usize];
        let (alpha_base, multiplier, alpha_table) = (
            (alpha >> 56) as i32,
            ((alpha >> 52) & 15) as i32,
            ((alpha >> 48) & 15) as usize,
        );
        std::array::from_fn(|texel| {
            let (x, y) = (texel % 4, texel / 4);
            // Both formats index texels column by column.
            let column_major = (x * 4 + y) as u32;
            let sub = usize::from(if flip { y >= 2 } else { x >= 2 });
            let msb = (color >> (16 + column_major)) & 1;
            let lsb = (color >> column_major) & 1;
            let [small, large] = ETC1_MODIFIERS[tables[sub]];
            let modifier = match (msb, lsb) {
                (0, 0) => small,
                (0, _) => large,
                (_, 0) => -small,
                _ => -large,
            };
            let rgb = bases[sub].map(|c| (c + modifier).clamp(0, 255) as u8);
            let index = ((alpha >> (45 - 3 * column_major)) & 7) as usize;
            let a = alpha_base + EAC_MODIFIERS[alpha_table][index] * multiplier;
            [rgb[0], rgb[1], rgb[2], a.clamp(0, 255) as u8]
        })
    }

    /// One ASTC 4x4 LDR block through basisu's own decoder, independent of its UASTC and ETC1S transcoders.
    fn astc_4x4_block(block: &[u8; 16]) -> [[u8; 4]; 16] {
        let log = basisu::astc::unpack::unpack_block(block, 4, 4).expect("a valid ASTC 4x4 block");
        let mut texels = [[0u8; 4]; 16];
        basisu::astc::decode::decode_block_ldr8(&log, 4, 4, false, &mut texels)
            .expect("an LDR ASTC block");
        texels
    }

    type Decode = fn(&[u8; 16]) -> [[u8; 4]; 16];
    /// A target format, its decoder, and mean bounds against the ramp and against basisu's RGBA32.
    type Target = (CompressedFormat, Decode, f64, f64);

    fn decode_blocks(width: u32, height: u32, data: &[u8], block: Decode) -> Vec<u8> {
        let blocks_wide = width.div_ceil(4) as usize;
        let mut rgba8 = vec![0u8; (width * height * 4) as usize];
        for (index, bytes) in data.as_chunks::<16>().0.iter().enumerate() {
            for (i, texel) in block(bytes).iter().enumerate() {
                let (x, y) = (
                    index % blocks_wide * 4 + i % 4,
                    index / blocks_wide * 4 + i / 4,
                );
                if x < width as usize && y < height as usize {
                    let at = (y * width as usize + x) * 4;
                    rgba8[at..at + 4].copy_from_slice(texel);
                }
            }
        }
        rgba8
    }

    /// Retargets decode to the ramp; ETC1S is ETC1 and UASTC is ASTC, so those two match RGBA32 exactly.
    #[test]
    fn encoded_basis_retargets_to_etc2_and_astc_close_to_its_source() {
        let etc2 = (CompressedFormat::Etc2Rgba8Unorm, etc2_rgba8_block as Decode);
        let astc = (CompressedFormat::Astc4x4RgbaUnorm, astc_4x4_block as Decode);
        let cases: [(&str, &[u8], [Target; 2]); 2] = [
            (
                "ETC1S",
                include_bytes!("../../webgpu_renderer/tests/assets/ramp_etc1s.ktx2"),
                [(etc2.0, etc2.1, 10.0, 0.0), (astc.0, astc.1, 10.0, 1.0)],
            ),
            (
                "UASTC",
                include_bytes!("../../webgpu_renderer/tests/assets/ramp_uastc.ktx2"),
                [(etc2.0, etc2.1, 6.0, 6.0), (astc.0, astc.1, 2.0, 0.0)],
            ),
        ];
        for (name, file, targets) in cases {
            let bc7 = load_ktx2(file)
                .unwrap_or_else(|err| panic!("{name}: {err:#}"))
                .compressed
                .expect("a 32x32 base transcodes to BC7");
            assert!(
                retarget_basis(&bc7, CompressedFormat::Bc7RgbaUnorm).is_none(),
                "{name}: BC7 is already the payload"
            );
            let rgba = Transcoder::new(file)
                .and_then(|t| t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE))
                .expect("RGBA32 transcode");
            for (format, block, bound, rgba_bound) in targets {
                let retargeted = retarget_basis(&bc7, format)
                    .expect("a Basis payload retargets")
                    .unwrap_or_else(|err| panic!("{name} to {format:?}: {err:#}"));
                assert_eq!(retargeted.format, format);
                assert_eq!(
                    retargeted.mips.len(),
                    6,
                    "{name} {format:?}: the full chain"
                );
                assert_eq!(retargeted.declared_srgb, bc7.declared_srgb);
                let rgba8 = decode_blocks(32, 32, &retargeted.mips[0], block);
                let (mean, max) = texel_error(32, 32, &rgba8, encoded_ramp);
                let (to_rgba, _) = texel_error(32, 32, &rgba8, |x, y| {
                    let at = ((y * 32 + x) * 4) as usize;
                    rgba[at..at + 4].try_into().unwrap()
                });
                eprintln!("{name} as {format:?}: mean {mean:.3} max {max}, {to_rgba:.3} from basisu's RGBA32");
                let (swapped, _) = texel_error(32, 32, &rgba8, |x, y| encoded_ramp(y, x));
                assert!(
                    mean < bound && swapped > 4.0 * bound && to_rgba <= rgba_bound,
                    "{name} as {format:?}: {mean:.3} off, transposed {swapped:.3}, {to_rgba:.3} from RGBA32"
                );
                let last = decode_blocks(1, 1, &retargeted.mips[5], block);
                assert!(
                    last[0].abs_diff(128) < 24 && last[1].abs_diff(128) < 24,
                    "{name} as {format:?}: the 1x1 level averages the ramp, got {:?}",
                    &last[..4]
                );
            }
        }
    }

    #[test]
    fn a_bcn_payload_has_no_basis_source_to_retarget() {
        let texture = load_ktx2(include_bytes!(
            "../../webgpu_renderer/tests/assets/red_bc1.ktx2"
        ))
        .unwrap();
        let compressed = texture.compressed.unwrap();
        assert!(compressed.basis_source.is_none());
        assert!(retarget_basis(&compressed, CompressedFormat::Etc2Rgba8Unorm).is_none());
    }
}
