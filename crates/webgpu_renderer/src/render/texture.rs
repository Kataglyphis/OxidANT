//! Texture, sampler and mip-chain creation, split out of `forward.rs`.

use crate::asset::basis::{basis_target, retarget_basis};
use crate::asset::bc_decode::decode_level;
use crate::context::GpuContext;
use crate::render::texture_stream::{ChainLevel, LevelChain};
use crate::scene::{CompressedFormat, CompressedTexture, CpuSampler, CpuTexture, CpuWrap};

pub(crate) fn create_sampler(device: &wgpu::Device, desc: &CpuSampler) -> wgpu::Sampler {
    let wrap = |mode: CpuWrap| match mode {
        CpuWrap::Repeat => wgpu::AddressMode::Repeat,
        CpuWrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        CpuWrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
    };
    let filter = |nearest: bool| {
        if nearest {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        }
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("material_sampler"),
        address_mode_u: wrap(desc.wrap_u),
        address_mode_v: wrap(desc.wrap_v),
        mag_filter: filter(desc.mag_nearest),
        min_filter: filter(desc.min_nearest),
        mipmap_filter: if desc.mip_nearest {
            wgpu::MipmapFilterMode::Nearest
        } else {
            wgpu::MipmapFilterMode::Linear
        },
        anisotropy_clamp: anisotropy_for(desc),
        ..Default::default()
    })
}

/// Anisotropy for a glTF sampler; wgpu rejects anisotropy above 1 unless every filter is Linear.
/// A mipmap-less filter gets none either: glTF's LINEAR is plain bilinear on level 0.
pub(crate) fn anisotropy_for(desc: &CpuSampler) -> u16 {
    if desc.mag_nearest || desc.min_nearest || desc.mip_nearest || !desc.mipmaps {
        1
    } else {
        16
    }
}

pub(crate) fn srgb_to_linear(byte: u8) -> f32 {
    let c = byte as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn linear_to_srgb(value: f32) -> u8 {
    let c = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Full mip chain via 2x2 box filtering; sRGB averaged in linear space, data maps raw.
pub(crate) fn generate_mips(base: &CpuTexture, srgb: bool) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(base.width, base.height, base.rgba8.clone())];
    if base.width == 0 || base.height == 0 {
        return levels;
    }
    let (mut w, mut h) = (base.width, base.height);

    while w > 1 || h > 1 {
        let (pw, ph, prev) = levels.last().unwrap();
        let (pw, ph) = (*pw, *ph);
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let mut next = Vec::with_capacity((nw * nh * 4) as usize);

        for y in 0..nh {
            for x in 0..nw {
                let x0 = (x * 2).min(pw - 1);
                let x1 = (x * 2 + 1).min(pw - 1);
                let y0 = (y * 2).min(ph - 1);
                let y1 = (y * 2 + 1).min(ph - 1);
                for channel in 0..4usize {
                    let fetch = |px: u32, py: u32| prev[((py * pw + px) * 4) as usize + channel];
                    let samples = [fetch(x0, y0), fetch(x1, y0), fetch(x0, y1), fetch(x1, y1)];
                    // Alpha is linear even for sRGB textures.
                    let value = if srgb && channel < 3 {
                        let sum: f32 = samples.iter().map(|&b| srgb_to_linear(b)).sum();
                        linear_to_srgb(sum / 4.0)
                    } else {
                        // Round half up like the sRGB arm; truncating compounds a bias per level.
                        ((samples.iter().map(|&b| b as u32).sum::<u32>() + 2) / 4) as u8
                    };
                    next.push(value);
                }
            }
        }
        levels.push((nw, nh, next));
        w = nw;
        h = nh;
    }

    levels
}

pub(crate) fn compressed_wgpu_format(format: CompressedFormat, srgb: bool) -> wgpu::TextureFormat {
    use CompressedFormat as F;
    match (format, srgb) {
        (F::Bc1RgbaUnorm, false) => wgpu::TextureFormat::Bc1RgbaUnorm,
        (F::Bc1RgbaUnorm, true) => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        (F::Bc3RgbaUnorm, false) => wgpu::TextureFormat::Bc3RgbaUnorm,
        (F::Bc3RgbaUnorm, true) => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        (F::Bc5RgUnorm, _) => wgpu::TextureFormat::Bc5RgUnorm,
        (F::Bc7RgbaUnorm, false) => wgpu::TextureFormat::Bc7RgbaUnorm,
        (F::Bc7RgbaUnorm, true) => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
        (F::Etc2Rgba8Unorm, false) => wgpu::TextureFormat::Etc2Rgba8Unorm,
        (F::Etc2Rgba8Unorm, true) => wgpu::TextureFormat::Etc2Rgba8UnormSrgb,
        (F::Astc4x4RgbaUnorm, false) => wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        },
        (F::Astc4x4RgbaUnorm, true) => wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::UnormSrgb,
        },
    }
}

/// Warns when the KTX2 container's colour space disagrees with the slot's: usage decides the format.
fn warn_declared_colour_space(compressed: &CompressedTexture, srgb: bool, label: Option<&str>) {
    if let Some(declared) = compressed.declared_srgb {
        if declared != srgb {
            log::warn!(
                "{}: KTX2 container declares {} but the material uses it as {}; \
                 glTF usage decides the GPU format, so this may render with the wrong gamma",
                label.unwrap_or("<unlabeled texture>"),
                if declared { "sRGB" } else { "linear" },
                if srgb { "sRGB" } else { "linear" },
            );
        }
    }
}

/// A block-compressed mip chain as it uploads, its pixels untouched.
fn compressed_chain(
    texture: &CpuTexture,
    format: CompressedFormat,
    mips: Vec<Vec<u8>>,
    srgb: bool,
) -> LevelChain {
    // `compressed_upload_problem` already checked the base and every level's byte count.
    let levels = mips
        .into_iter()
        .enumerate()
        .map(|(level, data)| ChainLevel {
            width: (texture.width >> level).max(1),
            height: (texture.height >> level).max(1),
            data,
        })
        .collect();
    LevelChain {
        format: compressed_wgpu_format(format, srgb),
        levels,
    }
}

/// A texture holding `chain`'s levels from `base` on, `base` as its level 0.
pub(crate) fn write_chain(
    gpu: &GpuContext,
    chain: &LevelChain,
    base: usize,
    label: Option<&str>,
) -> wgpu::Texture {
    let levels = &chain.levels[base..];
    let size = (levels[0].width, levels[0].height);
    let gpu_texture = chain_texture(&gpu.device, chain.format, size, levels.len(), label);
    for (level, data) in levels.iter().enumerate() {
        write_level(gpu, &gpu_texture, level as u32, data);
    }
    gpu_texture
}

/// An empty streamed material texture: sampled, written from the CPU, and the source of the next step's copies.
pub(crate) fn chain_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    (width, height): (u32, u32),
    levels: usize,
    label: Option<&str>,
) -> wgpu::Texture {
    create_2d_texture(
        device,
        label,
        width,
        height,
        format,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        levels as u32,
        1,
    )
}

/// The extent a copy of a `width` x `height` level spans: whole blocks, as wgpu copies them.
fn block_extent(format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::Extent3d {
    let (block_w, block_h) = format.block_dimensions();
    // A 2x2 or 1x1 level of a 4x4-block format is one block, 4x4 physical texels.
    wgpu::Extent3d {
        width: width.div_ceil(block_w) * block_w,
        height: height.div_ceil(block_h) * block_h,
        depth_or_array_layers: 1,
    }
}

/// Writes one chain level into `texture`'s mip `mip_level`.
pub(crate) fn write_level(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    mip_level: u32,
    data: &ChainLevel,
) {
    let format = texture.format();
    let (block_w, block_h) = format.block_dimensions();
    let block_bytes = format.block_copy_size(None).unwrap_or(4);
    let blocks_wide = data.width.div_ceil(block_w);
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data.data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(blocks_wide * block_bytes),
            rows_per_image: Some(data.height.div_ceil(block_h)),
        },
        block_extent(format, data.width, data.height),
    );
}

/// Records a GPU copy of `src`'s mip `src_level` into `dst`'s mip `dst_level`, both `width` x `height`.
pub(crate) fn copy_level(
    encoder: &mut wgpu::CommandEncoder,
    (src, src_level): (&wgpu::Texture, u32),
    (dst, dst_level): (&wgpu::Texture, u32),
    (width, height): (u32, u32),
) {
    let at = |texture, mip_level| wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    };
    encoder.copy_texture_to_texture(
        at(src, src_level),
        at(dst, dst_level),
        block_extent(dst.format(), width, height),
    );
}

/// Why a block-compressed payload cannot become a valid wgpu texture, or `None` when it can.
fn compressed_upload_problem(
    gpu: &GpuContext,
    texture: &CpuTexture,
    compressed: &CompressedTexture,
) -> Option<String> {
    use CompressedFormat as F;
    let (sampled, family) = match compressed.format {
        F::Bc1RgbaUnorm | F::Bc3RgbaUnorm | F::Bc5RgUnorm | F::Bc7RgbaUnorm => {
            (gpu.supports_bc, "BC")
        }
        F::Etc2Rgba8Unorm => (gpu.supports_etc2, "ETC2"),
        F::Astc4x4RgbaUnorm => (gpu.supports_astc, "ASTC"),
    };
    if !sampled {
        return Some(format!("the adapter has no {family} support"));
    }
    let max_side = gpu.device.limits().max_texture_dimension_2d;
    if texture.width > max_side || texture.height > max_side {
        return Some(format!(
            "its {}x{} base exceeds the device's {max_side}-texel side",
            texture.width, texture.height
        ));
    }
    // WebGPU requires a block-compressed texture's base to be whole 4x4 blocks.
    if !texture.width.is_multiple_of(4) || !texture.height.is_multiple_of(4) {
        return Some(format!(
            "its {}x{} base is not whole 4x4 blocks",
            texture.width, texture.height
        ));
    }
    crate::asset::ktx2_loader::validate_mip_chain(
        texture.width,
        texture.height,
        compressed.format,
        &compressed.mips,
    )
    .err()
    .map(|err| err.to_string())
}

/// An uncompressed texture with its whole mip chain behind the default view.
pub(crate) fn create_material_texture(
    gpu: &GpuContext,
    texture: &CpuTexture,
    srgb: bool,
    label: Option<&str>,
) -> wgpu::TextureView {
    let chain = rgba8_chain(texture, srgb);
    let max_side = gpu.device.limits().max_texture_dimension_2d;
    let base = chain
        .base_candidates(max_side)
        .first()
        .copied()
        .unwrap_or(chain.levels.len() - 1);
    write_chain(gpu, &chain, base, label).create_view(&wgpu::TextureViewDescriptor::default())
}

/// A view of a material texture for `sampler`: glTF's mipmap-less filters see level 0 alone.
/// A one-level view, not a zero LOD clamp: Vulkan treats a clamped LOD of 0 as magnified.
pub(crate) fn material_view(texture: &wgpu::Texture, sampler: &CpuSampler) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        mip_level_count: if sampler.mipmaps { None } else { Some(1) },
        ..Default::default()
    })
}

/// A material texture's chain as this adapter uploads it, decoding a BCn payload the device refuses; `None` if even that fails.
pub(crate) fn material_chain(
    gpu: &GpuContext,
    texture: &CpuTexture,
    srgb: bool,
    label: Option<&str>,
) -> Option<LevelChain> {
    let Some(compressed) = texture.compressed.as_ref() else {
        return Some(rgba8_chain(texture, srgb));
    };
    if let Some(chain) = retargeted_basis_chain(gpu, texture, compressed, srgb, label) {
        return Some(chain);
    }
    let Some(problem) = compressed_upload_problem(gpu, texture, compressed) else {
        warn_declared_colour_space(compressed, srgb, label);
        let mips = compressed.mips.clone();
        return Some(compressed_chain(texture, compressed.format, mips, srgb));
    };
    let name = label.unwrap_or("<unlabeled texture>");
    let Some(levels) = decoded_levels(texture, compressed) else {
        log::warn!("{name}: block-compressed texture ({problem}) does not decode; the slot keeps its default");
        return None;
    };
    log::info!("{name}: block-compressed texture decoded on the CPU: {problem}");
    // BC5 has no sRGB variant, so the adapter's own decode would read it linear whatever the slot.
    let srgb = compressed_wgpu_format(compressed.format, srgb).is_srgb();
    let max_side = gpu.device.limits().max_texture_dimension_2d;
    if texture.width > max_side || texture.height > max_side {
        // The decoded chain may stop short of the side limit; the base's own mips reach 1x1.
        let base = levels.into_iter().next().map(|(_, _, rgba8)| rgba8)?;
        let decoded = CpuTexture {
            width: texture.width,
            height: texture.height,
            rgba8: base,
            compressed: None,
        };
        return Some(rgba8_chain(&decoded, srgb));
    }
    Some(rgba8_levels(levels, srgb))
}

/// A Basis payload retargeted for an adapter with ETC2 or ASTC but no BC; `None` leaves the BC7 path to decide.
fn retargeted_basis_chain(
    gpu: &GpuContext,
    texture: &CpuTexture,
    compressed: &CompressedTexture,
    srgb: bool,
    label: Option<&str>,
) -> Option<LevelChain> {
    let target = basis_target(gpu.supports_bc, gpu.supports_etc2, gpu.supports_astc)?;
    let name = label.unwrap_or("<unlabeled texture>");
    let retargeted = match retarget_basis(compressed, target)? {
        Ok(retargeted) => retargeted,
        Err(err) => {
            log::warn!("{name}: Basis to {target:?} failed ({err:#}); falling back to BC7");
            return None;
        }
    };
    if let Some(problem) = compressed_upload_problem(gpu, texture, &retargeted) {
        log::warn!("{name}: Basis as {target:?} cannot upload ({problem}); falling back to BC7");
        return None;
    }
    log::info!("{name}: Basis transcoded to {target:?} for this adapter");
    warn_declared_colour_space(&retargeted, srgb, label);
    Some(compressed_chain(
        texture,
        retargeted.format,
        retargeted.mips,
        srgb,
    ))
}

/// The payload's mip chain as RGBA8, up to its first level too short to decode; `None` if that is the base.
fn decoded_levels(
    texture: &CpuTexture,
    compressed: &CompressedTexture,
) -> Option<Vec<(u32, u32, Vec<u8>)>> {
    let levels: Vec<_> = compressed
        .mips
        .iter()
        .enumerate()
        .map_while(|(level, data)| {
            let (w, h) = (
                (texture.width >> level).max(1),
                (texture.height >> level).max(1),
            );
            decode_level(compressed.format, w, h, data).map(|rgba8| (w, h, rgba8))
        })
        .collect();
    (!levels.is_empty()).then_some(levels)
}

/// The CPU box-filtered chain of an RGBA8 image, down to 1x1.
pub(crate) fn rgba8_chain(texture: &CpuTexture, srgb: bool) -> LevelChain {
    rgba8_levels(generate_mips(texture, srgb), srgb)
}

fn rgba8_levels(levels: Vec<(u32, u32, Vec<u8>)>, srgb: bool) -> LevelChain {
    LevelChain {
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        levels: levels
            .into_iter()
            .map(|(width, height, data)| ChainLevel {
                width,
                height,
                data,
            })
            .collect(),
    }
}

/// The crate's single-layer 2D texture; clamps `width`/`height` to 1, as zero is a wgpu error.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_2d_texture(
    device: &wgpu::Device,
    label: Option<&str>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    mip_level_count: u32,
    sample_count: u32,
) -> wgpu::Texture {
    let shape = Shape2d {
        size: (width, height),
        mip_level_count,
        sample_count,
    };
    create_2d(device, label, shape, (format, usage), &[])
}

/// One mip, one sample, viewable as each of `view_formats` too: an output egui paints through its linear twin.
pub(crate) fn create_viewable_2d_texture(
    device: &wgpu::Device,
    label: Option<&str>,
    size: (u32, u32),
    (format, usage): (wgpu::TextureFormat, wgpu::TextureUsages),
    view_formats: &[wgpu::TextureFormat],
) -> wgpu::Texture {
    let shape = Shape2d {
        size,
        mip_level_count: 1,
        sample_count: 1,
    };
    create_2d(device, label, shape, (format, usage), view_formats)
}

struct Shape2d {
    size: (u32, u32),
    mip_level_count: u32,
    sample_count: u32,
}

fn create_2d(
    device: &wgpu::Device,
    label: Option<&str>,
    shape: Shape2d,
    (format, usage): (wgpu::TextureFormat, wgpu::TextureUsages),
    view_formats: &[wgpu::TextureFormat],
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width: shape.size.0.max(1),
            height: shape.size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: shape.mip_level_count,
        sample_count: shape.sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

/// As [`create_2d_texture`], but returns the default view for callers that never touch the texture.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_2d_view(
    device: &wgpu::Device,
    label: Option<&str>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    mip_level_count: u32,
    sample_count: u32,
) -> wgpu::TextureView {
    create_2d_texture(
        device,
        label,
        width,
        height,
        format,
        usage,
        mip_level_count,
        sample_count,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::CpuTexture;

    #[test]
    fn anisotropy_is_requested_only_when_every_filter_is_linear() {
        // wgpu rejects anisotropy > 1 with any Nearest filter; all-linear must still get 16x.
        let linear = CpuSampler::default();
        assert_eq!(
            anisotropy_for(&linear),
            16,
            "all-linear sampler should get 16x"
        );

        for (label, s) in [
            (
                "mag",
                CpuSampler {
                    mag_nearest: true,
                    ..Default::default()
                },
            ),
            (
                "min",
                CpuSampler {
                    min_nearest: true,
                    ..Default::default()
                },
            ),
            (
                "mip",
                CpuSampler {
                    mip_nearest: true,
                    ..Default::default()
                },
            ),
            (
                "mipmap-less",
                CpuSampler {
                    mipmaps: false,
                    ..Default::default()
                },
            ),
        ] {
            assert_eq!(
                anisotropy_for(&s),
                1,
                "{label}-nearest sampler must stay at 1x or wgpu rejects it"
            );
        }
    }

    #[test]
    fn compressed_wgpu_format_is_decided_by_usage_not_by_the_container() {
        use CompressedFormat as F;
        // Only the material's `srgb` usage picks the format, never the KTX2 declared colour space.
        let cases = [
            (F::Bc1RgbaUnorm, false, wgpu::TextureFormat::Bc1RgbaUnorm),
            (F::Bc1RgbaUnorm, true, wgpu::TextureFormat::Bc1RgbaUnormSrgb),
            (F::Bc3RgbaUnorm, false, wgpu::TextureFormat::Bc3RgbaUnorm),
            (F::Bc3RgbaUnorm, true, wgpu::TextureFormat::Bc3RgbaUnormSrgb),
            (F::Bc5RgUnorm, false, wgpu::TextureFormat::Bc5RgUnorm),
            (F::Bc7RgbaUnorm, false, wgpu::TextureFormat::Bc7RgbaUnorm),
            (F::Bc7RgbaUnorm, true, wgpu::TextureFormat::Bc7RgbaUnormSrgb),
            (
                F::Etc2Rgba8Unorm,
                false,
                wgpu::TextureFormat::Etc2Rgba8Unorm,
            ),
            (
                F::Etc2Rgba8Unorm,
                true,
                wgpu::TextureFormat::Etc2Rgba8UnormSrgb,
            ),
            (
                F::Astc4x4RgbaUnorm,
                true,
                wgpu::TextureFormat::Astc {
                    block: wgpu::AstcBlock::B4x4,
                    channel: wgpu::AstcChannel::UnormSrgb,
                },
            ),
        ];
        for (format, srgb, expected) in cases {
            assert_eq!(compressed_wgpu_format(format, srgb), expected);
            // The upload's bytes-per-row comes from `block_bytes`; it must match wgpu's own block size.
            assert_eq!(
                expected.block_copy_size(None),
                Some(format.block_bytes()),
                "{format:?}"
            );
            assert_eq!(expected.block_dimensions(), (4, 4), "{format:?}");
        }
    }

    #[test]
    fn srgb_round_trip_is_stable_for_every_byte() {
        for b in 0..=255u8 {
            assert_eq!(
                linear_to_srgb(srgb_to_linear(b)),
                b,
                "sRGB byte {b} must round-trip through linear space exactly"
            );
        }
    }

    #[test]
    fn generate_mips_halves_down_to_one_by_one() {
        let base = CpuTexture {
            width: 4,
            height: 4,
            rgba8: vec![128u8; 4 * 4 * 4],
            compressed: None,
        };
        let levels = generate_mips(&base, false);
        let dims: Vec<(u32, u32)> = levels.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(dims, vec![(4, 4), (2, 2), (1, 1)]);
        for (w, h, data) in &levels {
            assert_eq!(data.len(), (*w as usize) * (*h as usize) * 4);
        }
    }

    #[test]
    fn generate_mips_averages_srgb_in_linear_space() {
        // Black and white texels: a raw byte average gives ~127, a linear-space one differs.
        let base = CpuTexture {
            width: 2,
            height: 1,
            rgba8: vec![0, 0, 0, 0, 255, 255, 255, 255],
            compressed: None,
        };
        let levels = generate_mips(&base, true);
        let (w, h, data) = &levels[1];
        assert_eq!((*w, *h), (1, 1));

        let expected_rgb = linear_to_srgb((srgb_to_linear(0) + srgb_to_linear(255)) / 2.0);
        for &channel in &data[0..3] {
            assert_eq!(
                channel, expected_rgb,
                "sRGB channels must be averaged in linear space, not as raw bytes"
            );
            assert_ne!(
                channel, 127,
                "a raw byte average would incorrectly give ~127/128"
            );
        }
        // Alpha is averaged raw: the mean of two 0s and two 255s is 127.5, rounded up to 128.
        assert_eq!(data[3], 128);
    }

    #[test]
    fn generate_mips_does_not_drift_darker_over_a_long_chain() {
        const SIZE: u32 = 64;
        let mut rgba8 = vec![0u8; (SIZE * SIZE * 4) as usize];
        for y in 0..SIZE {
            for x in 0..SIZE {
                for channel in 0..4u32 {
                    let value = ((x * 7 + y * 13 + channel) % 256) as u8;
                    rgba8[((y * SIZE + x) * 4 + channel) as usize] = value;
                }
            }
        }
        let base = CpuTexture {
            width: SIZE,
            height: SIZE,
            rgba8,
            compressed: None,
        };

        let mut sums = [0f64; 4];
        for y in 0..SIZE {
            for x in 0..SIZE {
                for (channel, sum) in sums.iter_mut().enumerate() {
                    *sum += base.rgba8[((y * SIZE + x) * 4 + channel as u32) as usize] as f64;
                }
            }
        }
        let means: Vec<f64> = sums.iter().map(|&s| s / (SIZE * SIZE) as f64).collect();

        let levels = generate_mips(&base, false);
        let (w, h, data) = levels.last().unwrap();
        assert_eq!((*w, *h), (1, 1));
        for channel in 0..4usize {
            let value = data[channel] as f64;
            let mean = means[channel].round();
            assert!(
                (value - mean).abs() <= 1.0,
                "channel {channel}: mip value {value} drifted too far from base mean {mean}"
            );
        }
    }

    #[test]
    fn generate_mips_on_a_zero_sized_texture_returns_only_the_base() {
        let base = CpuTexture {
            width: 0,
            height: 4,
            rgba8: Vec::new(),
            compressed: None,
        };
        let levels = generate_mips(&base, false);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].0, 0);
        assert_eq!(levels[0].1, 4);
    }
}
