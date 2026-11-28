//! Texture, sampler and mip-chain creation, split out of `forward.rs`.

use std::sync::Arc;

use crate::asset::basis::{basis_target, retarget_basis, rgba8_levels as basis_rgba8_levels};
use crate::context::GpuContext;
use crate::render::texture_stream::{ChainLevel, LevelBytes, LevelChain};
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

// The chain both renderers build, so the C++ engine's levels are this renderer's.
use kataglyphis_assets::levels::{completed_levels, decoded_levels, mips_below_base, BlockSupport};
pub(crate) use kataglyphis_assets::levels::{linear_to_srgb, srgb_to_linear};

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
    mips: Vec<LevelBytes>,
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
    blocks: &BlockSupport,
    texture: &CpuTexture,
    compressed: &CompressedTexture,
) -> Option<String> {
    use CompressedFormat as F;
    let family = match compressed.format {
        F::Bc1RgbaUnorm | F::Bc3RgbaUnorm | F::Bc5RgUnorm | F::Bc7RgbaUnorm => "BC",
        F::Etc2Rgba8Unorm => "ETC2",
        F::Astc4x4RgbaUnorm => "ASTC",
    };
    if !blocks.samples(compressed.format) {
        return Some(format!("the adapter has no {family} support"));
    }
    // WebGPU requires a block-compressed texture's base to be whole 4x4 blocks.
    if !texture.width.is_multiple_of(4) || !texture.height.is_multiple_of(4) {
        return Some(format!(
            "its {}x{} base is not whole 4x4 blocks",
            texture.width, texture.height
        ));
    }
    if let Err(err) = crate::asset::ktx2_loader::validate_mip_chain(
        texture.width,
        texture.height,
        compressed.format,
        &compressed.mips,
    ) {
        return Some(err.to_string());
    }
    // A finer level past the device's side is skipped, as the C++ engine's upload does; only none to start from decodes.
    let levels = compressed.mips.len() as u32;
    (!blocks.uploads_blocks(compressed.format, texture.width, texture.height, levels)).then(|| {
        format!(
            "none of its {}x{} levels fits the device's {}-texel side as whole blocks",
            texture.width, texture.height, blocks.max_side
        )
    })
}

/// An uncompressed texture with its whole mip chain behind the default view.
pub(crate) fn create_material_texture(
    gpu: &GpuContext,
    texture: CpuTexture,
    srgb: bool,
    label: Option<&str>,
) -> wgpu::TextureView {
    let chain = rgba8_chain(&Arc::new(texture), srgb);
    let max_side = gpu.device.limits().max_texture_dimension_2d;
    let base = chain
        .base_candidates(max_side)
        .first()
        .copied()
        .unwrap_or(chain.levels.len().saturating_sub(1));
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

/// The block formats `gpu` samples and its longest texture side, the C++ engine's `BlockSupport` for this adapter.
fn block_support(gpu: &GpuContext) -> BlockSupport {
    BlockSupport {
        bc: gpu.supports_bc,
        etc2: gpu.supports_etc2,
        astc: gpu.supports_astc,
        max_side: gpu.device.limits().max_texture_dimension_2d,
    }
}

/// A material texture's chain as this adapter uploads it, decoding a BCn payload the device refuses; `None` if even that fails.
/// Its levels share the image's base or stored blocks rather than copy them.
pub(crate) fn material_chain(
    gpu: &GpuContext,
    texture: &Arc<CpuTexture>,
    srgb: bool,
    label: Option<&str>,
) -> Option<LevelChain> {
    material_chain_for(&block_support(gpu), texture, srgb, label)
}

/// [`material_chain`] for an adapter that samples `blocks`: the levels `levels::ktx2_chain` hands the C++ engine.
fn material_chain_for(
    blocks: &BlockSupport,
    texture: &Arc<CpuTexture>,
    srgb: bool,
    label: Option<&str>,
) -> Option<LevelChain> {
    let Some(compressed) = texture.compressed.as_ref() else {
        return Some(rgba8_chain(texture, srgb));
    };
    if let Some(chain) = retargeted_basis_chain(blocks, texture, compressed, srgb, label) {
        return Some(chain);
    }
    let Some(problem) = compressed_upload_problem(blocks, texture, compressed) else {
        warn_declared_colour_space(compressed, srgb, label);
        let mips = (0..compressed.mips.len())
            .map(|level| LevelBytes::Stored(Arc::clone(texture), level))
            .collect();
        return Some(compressed_chain(texture, compressed.format, mips, srgb));
    };
    let name = label.unwrap_or("<unlabeled texture>");
    if let Some(chain) = straight_basis_chain(texture, compressed, srgb) {
        log::info!("{name}: Basis transcoded straight to RGBA8: {problem}");
        return Some(chain);
    }
    let levels = decoded_levels(texture.width, texture.height, compressed);
    if levels.is_empty() {
        log::warn!("{name}: block-compressed texture ({problem}) does not decode; the slot keeps its default");
        return None;
    }
    log::info!("{name}: block-compressed texture decoded on the CPU: {problem}");
    // BC5 has no sRGB variant, so the adapter's own decode would read it linear whatever the slot.
    let srgb = compressed_wgpu_format(compressed.format, srgb).is_srgb();
    // The levels that decode, completed to 1x1: a level past the device's side is no base candidate.
    Some(rgba8_levels(
        completed_levels(texture.width, texture.height, levels, srgb),
        srgb,
    ))
}

/// A Basis payload no block format of this adapter takes, its own levels transcoded straight to RGBA8 and completed to
/// 1x1 as the C++ engine's loader does, not decoded from its BC7; `None` for any other payload or a failed transcode.
fn straight_basis_chain(
    texture: &CpuTexture,
    compressed: &CompressedTexture,
    srgb: bool,
) -> Option<LevelChain> {
    let levels = basis_rgba8_levels(compressed.basis_source.as_ref()?).ok()?;
    Some(rgba8_levels(
        completed_levels(texture.width, texture.height, levels, srgb),
        srgb,
    ))
}

/// A Basis payload retargeted for an adapter with ETC2 or ASTC but no BC; `None` leaves the BC7 path to decide.
fn retargeted_basis_chain(
    blocks: &BlockSupport,
    texture: &CpuTexture,
    compressed: &CompressedTexture,
    srgb: bool,
    label: Option<&str>,
) -> Option<LevelChain> {
    let target = basis_target(blocks.bc, blocks.etc2, blocks.astc)?;
    // A base of partial blocks, or none within the side, goes straight to RGBA8 as in the C++ engine's loader.
    let levels = compressed.mips.len() as u32;
    if !blocks.uploads_blocks(target, texture.width, texture.height, levels) {
        return None;
    }
    let name = label.unwrap_or("<unlabeled texture>");
    let retargeted = match retarget_basis(compressed, target)? {
        Ok(retargeted) => retargeted,
        Err(err) => {
            log::warn!("{name}: Basis to {target:?} failed ({err:#}); falling back to BC7");
            return None;
        }
    };
    if let Some(problem) = compressed_upload_problem(blocks, texture, &retargeted) {
        log::warn!("{name}: Basis as {target:?} cannot upload ({problem}); falling back to BC7");
        return None;
    }
    log::info!("{name}: Basis transcoded to {target:?} for this adapter");
    warn_declared_colour_space(&retargeted, srgb, label);
    let mips = retargeted.mips.into_iter().map(LevelBytes::Owned).collect();
    Some(compressed_chain(texture, retargeted.format, mips, srgb))
}

/// The CPU box-filtered chain of an RGBA8 image, down to 1x1; level 0 is the image's own base.
pub(crate) fn rgba8_chain(texture: &Arc<CpuTexture>, srgb: bool) -> LevelChain {
    let base = ChainLevel {
        width: texture.width,
        height: texture.height,
        data: LevelBytes::Base(Arc::clone(texture)),
    };
    let mut chain = rgba8_levels(mips_below_base(texture, srgb), srgb);
    chain.levels.insert(0, base);
    chain
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
                data: LevelBytes::Owned(data),
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
    use kataglyphis_assets::levels::{ktx2_chain, ChainFormat};

    /// `rgba8_chain`'s levels as sizes and bytes.
    fn generate_mips(base: CpuTexture, srgb: bool) -> Vec<(u32, u32, Vec<u8>)> {
        rgba8_chain(&Arc::new(base), srgb)
            .levels
            .iter()
            .map(|level| (level.width, level.height, level.data.to_vec()))
            .collect()
    }

    #[test]
    fn a_chain_shares_the_image_s_base_and_stored_blocks_instead_of_copying_them() {
        let image = Arc::new(CpuTexture {
            width: 4,
            height: 4,
            rgba8: vec![9; 64],
            compressed: None,
        });
        let chain = rgba8_chain(&image, true);
        assert_eq!(chain.levels.len(), 3);
        assert!(
            std::ptr::eq(chain.levels[0].data.as_ptr(), image.rgba8.as_ptr()),
            "level 0 is the image's own base"
        );
        assert!(matches!(chain.levels[1].data, LevelBytes::Owned(_)));
        let blocks = Arc::new(CpuTexture {
            width: 4,
            height: 4,
            rgba8: Vec::new(),
            compressed: Some(CompressedTexture {
                format: CompressedFormat::Bc1RgbaUnorm,
                mips: vec![vec![1; 8], vec![2; 8], vec![3; 8]],
                declared_srgb: None,
                basis_source: None,
            }),
        });
        let mips = (0..3)
            .map(|level| LevelBytes::Stored(Arc::clone(&blocks), level))
            .collect();
        let chain = compressed_chain(&blocks, CompressedFormat::Bc1RgbaUnorm, mips, false);
        let stored = &blocks.compressed.as_ref().unwrap().mips;
        for (level, mip) in chain.levels.iter().zip(stored) {
            assert!(std::ptr::eq(level.data.as_ptr(), mip.as_ptr()));
        }
        let sizes: Vec<_> = chain.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(4, 4), (2, 2), (1, 1)]);
        assert_eq!(
            &*LevelBytes::Stored(Arc::clone(&blocks), 7),
            &[] as &[u8],
            "a missing level is empty"
        );
    }

    #[test]
    fn a_basis_payload_no_block_format_takes_transcodes_straight_to_rgba8() {
        let file = include_bytes!("../../../assets/tests/assets/ramp_uastc.ktx2");
        let texture = crate::asset::ktx2_loader::load_ktx2(file).unwrap();
        let compressed = texture.compressed.as_ref().unwrap();
        let chain = straight_basis_chain(&texture, compressed, true).unwrap();
        assert_eq!(chain.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        let want = basis_rgba8_levels(file).unwrap();
        let got: Vec<Vec<u8>> = chain.levels.iter().map(|l| l.data.to_vec()).collect();
        assert_eq!(got, want, "basisu's own RGBA32 levels, not BC7 decoded");
        let sizes: Vec<_> = chain.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(32, 32), (16, 16), (8, 8), (4, 4), (2, 2), (1, 1)]);
        let stored_bc1 = crate::asset::ktx2_loader::load_ktx2(include_bytes!(
            "../../../assets/tests/assets/red_bc1.ktx2"
        ))
        .unwrap();
        assert!(
            straight_basis_chain(&stored_bc1, stored_bc1.compressed.as_ref().unwrap(), true)
                .is_none()
        );
    }

    #[test]
    fn a_chain_owns_only_the_levels_it_built() {
        let image = Arc::new(CpuTexture {
            width: 8,
            height: 8,
            rgba8: vec![9; 8 * 8 * 4],
            compressed: None,
        });
        let chain = rgba8_chain(&image, true);
        assert_eq!(chain.bytes_from(0), 256 + 64 + 16 + 4);
        assert_eq!(chain.owned_bytes(), 64 + 16 + 4, "the base is the image's");
        let file = include_bytes!("../../../assets/tests/assets/green_uastc.ktx2");
        let blocks = Arc::new(crate::asset::ktx2_loader::load_ktx2(file).unwrap());
        let bc = BlockSupport {
            bc: true,
            ..BlockSupport::default()
        };
        let stored = material_chain_for(&bc, &blocks, true, None).unwrap();
        assert!(stored.format.is_compressed());
        assert_eq!(stored.owned_bytes(), 0, "every level is the image's");
        let etc2 = BlockSupport {
            etc2: true,
            ..BlockSupport::default()
        };
        let retargeted = material_chain_for(&etc2, &blocks, true, None).unwrap();
        assert_eq!(retargeted.owned_bytes(), retargeted.bytes_from(0));
    }

    /// The support sets a chain is checked under: none, each family, all, and BC whose side fits no whole block.
    fn support_sets() -> Vec<BlockSupport> {
        let none = BlockSupport::default();
        vec![
            none,
            BlockSupport { bc: true, ..none },
            BlockSupport { etc2: true, ..none },
            BlockSupport { astc: true, ..none },
            BlockSupport {
                bc: true,
                etc2: true,
                astc: true,
                max_side: 0,
            },
            BlockSupport {
                bc: true,
                max_side: 2,
                ..none
            },
        ]
    }

    /// `chain`'s format as the C++ engine's chain names it, and each level's size and whole-block bytes.
    fn as_texture_chain(chain: &LevelChain) -> (ChainFormat, Vec<(u32, u32, Vec<u8>)>) {
        use CompressedFormat as F;
        let format = if chain.format.is_compressed() {
            let all = [
                F::Bc1RgbaUnorm,
                F::Bc3RgbaUnorm,
                F::Bc5RgUnorm,
                F::Bc7RgbaUnorm,
                F::Etc2Rgba8Unorm,
                F::Astc4x4RgbaUnorm,
            ];
            let same = |f: &F| {
                compressed_wgpu_format(*f, false).remove_srgb_suffix()
                    == chain.format.remove_srgb_suffix()
            };
            ChainFormat::Blocks(all.into_iter().find(same).unwrap())
        } else {
            ChainFormat::Rgba8
        };
        let levels = chain
            .levels
            .iter()
            .map(|l| {
                let bytes = format.level_bytes(l.width, l.height);
                (l.width, l.height, l.data[..bytes].to_vec())
            })
            .collect();
        (format, levels)
    }

    /// Every KTX2 fixture, under every support set: this renderer's chain is the one the C++ engine's loader hands over.
    #[test]
    fn a_ktx2_chain_is_the_one_the_cpp_engine_s_loader_builds() {
        let fixtures: [(&str, &[u8]); 5] = [
            (
                "red_bc1",
                include_bytes!("../../../assets/tests/assets/red_bc1.ktx2"),
            ),
            (
                "green_uastc",
                include_bytes!("../../../assets/tests/assets/green_uastc.ktx2"),
            ),
            (
                "ramp_uastc",
                include_bytes!("../../../assets/tests/assets/ramp_uastc.ktx2"),
            ),
            (
                "ramp_etc1s",
                include_bytes!("../../../assets/tests/assets/ramp_etc1s.ktx2"),
            ),
            (
                "rgb_levels_6x6_uastc",
                include_bytes!("../../../assets/tests/assets/rgb_levels_6x6_uastc.ktx2"),
            ),
        ];
        for (name, file) in fixtures {
            let image = Arc::new(crate::asset::ktx2_loader::load_ktx2(file).unwrap());
            for blocks in support_sets() {
                for srgb in [false, true] {
                    let ours = material_chain_for(&blocks, &image, srgb, Some(name)).unwrap();
                    let theirs = ktx2_chain(file, srgb, &blocks).unwrap();
                    let theirs_levels: Vec<_> = theirs
                        .levels()
                        .map(|(w, h, t)| (w, h, t.to_vec()))
                        .collect();
                    assert!(
                        as_texture_chain(&ours) == (theirs.format, theirs_levels),
                        "{name} under {blocks:?}, srgb {srgb}"
                    );
                }
            }
        }
    }

    /// A short BC chain an adapter without BC decodes gets its missing levels by halving, down to 1x1.
    #[test]
    fn a_short_decoded_chain_is_completed_to_one_by_one() {
        let file = include_bytes!("../../../assets/tests/assets/red_bc1.ktx2");
        let image = Arc::new(crate::asset::ktx2_loader::load_ktx2(file).unwrap());
        assert_eq!(image.compressed.as_ref().unwrap().mips.len(), 1);
        let decoded = material_chain_for(&BlockSupport::default(), &image, true, None).unwrap();
        let sizes: Vec<_> = decoded.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(8, 8), (4, 4), (2, 2), (1, 1)]);
        let base = &decoded.levels[0].data[..4];
        assert!(decoded.levels.iter().all(|l| &l.data[..4] == base));
        let bc = BlockSupport {
            bc: true,
            ..BlockSupport::default()
        };
        let stored = material_chain_for(&bc, &image, true, None).unwrap();
        assert_eq!(stored.levels.len(), 1, "blocks keep the file's one level");
    }

    /// A Basis base of partial blocks keeps the file's red, green and blue levels on every adapter, no box filter of red.
    #[test]
    fn a_partial_block_basis_base_keeps_the_file_s_levels() {
        let file = include_bytes!("../../../assets/tests/assets/rgb_levels_6x6_uastc.ktx2");
        let image = Arc::new(crate::asset::ktx2_loader::load_ktx2(file).unwrap());
        for blocks in support_sets() {
            let chain = material_chain_for(&blocks, &image, true, None).unwrap();
            assert_eq!(
                chain.format,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                "{blocks:?}"
            );
            let firsts: Vec<_> = chain.levels.iter().map(|l| l.data[..4].to_vec()).collect();
            assert_eq!(
                firsts,
                [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
                "{blocks:?}"
            );
        }
    }

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
        let levels = generate_mips(base, false);
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
        let levels = generate_mips(base, true);
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

        let levels = generate_mips(base, false);
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
        let levels = generate_mips(base, false);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].0, 0);
        assert_eq!(levels[0].1, 4);
    }
}
