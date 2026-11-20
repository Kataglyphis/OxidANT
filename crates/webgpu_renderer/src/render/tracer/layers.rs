//! Material textures resampled into the square, mipmapped layers of a texture array (WebGPU has no binding arrays).

use std::sync::{Arc, OnceLock};

use crate::scene::CpuTexture;

/// One array's layers, every one `size` x `size` with a full mip chain, level 0 first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerArray {
    pub size: u32,
    /// `layers[layer][level]`: tightly packed RGBA8 rows.
    pub layers: Vec<Vec<Vec<u8>>>,
}

impl LayerArray {
    pub fn mip_count(&self) -> u32 {
        u32::BITS - self.size.max(1).leading_zeros()
    }

    /// Drops every level but each layer's first.
    pub fn keep_base_level(&mut self) {
        for levels in &mut self.layers {
            levels.truncate(1);
        }
    }

    /// Bytes of all layers and levels.
    pub fn byte_size(&self) -> u64 {
        self.layers
            .iter()
            .flatten()
            .map(|level| level.len() as u64)
            .sum()
    }
}

/// Bytes one `size`-squared RGBA8 layer takes with its mip chain.
pub fn layer_bytes(size: u32) -> u64 {
    let mut total = 0u64;
    let mut side = u64::from(size.max(1));
    loop {
        total += side * side * 4;
        if side == 1 {
            return total;
        }
        side /= 2;
    }
}

/// The layer side for `sources`: their largest side rounded up to a power of two, halved until it fits `cap` and `budget`.
pub fn layer_size(sources: &[Arc<CpuTexture>], cap: u32, budget: u64) -> u32 {
    let largest = sources
        .iter()
        .map(|t| t.width.max(t.height))
        .max()
        .unwrap_or(1);
    let mut size = largest
        .max(1)
        .next_power_of_two()
        .min(cap.max(1).next_power_of_two());
    while size > 1 && sources.len() as u64 * layer_bytes(size) > budget {
        size /= 2;
    }
    size
}

/// Resamples each source to `size` (a power of two) with its mips; sRGB averages in linear light.
pub fn build_layers(sources: &[Arc<CpuTexture>], srgb: bool, size: u32) -> LayerArray {
    LayerArray {
        size,
        layers: sources
            .iter()
            .map(|texture| resample_layer(texture, srgb, size))
            .collect(),
    }
}

/// A white 1x1 array for an empty slot set: a texture binding must exist even with no layers.
pub fn placeholder() -> LayerArray {
    LayerArray {
        size: 1,
        layers: vec![vec![vec![255, 255, 255, 255]]],
    }
}

/// Whether the tracer can read `texture`: RGBA8, or BC blocks the CPU decodes (ETC2 and ASTC only exist on the GPU).
pub fn traceable(texture: &CpuTexture) -> bool {
    texture.width > 0
        && texture.height > 0
        && texture.compressed.as_ref().is_none_or(|compressed| {
            !matches!(
                compressed.format,
                crate::scene::CompressedFormat::Etc2Rgba8Unorm
                    | crate::scene::CompressedFormat::Astc4x4RgbaUnorm
            )
        })
}

/// A compressed texture's smallest mip still at least `size` on both sides (or level 0), decoded to RGBA8.
fn decoded_level(texture: &CpuTexture, size: u32) -> Option<CpuTexture> {
    let compressed = texture.compressed.as_ref()?;
    let extent = |level: usize| {
        (
            (texture.width >> level).max(1),
            (texture.height >> level).max(1),
        )
    };
    let level = (0..compressed.mips.len())
        .take_while(|&level| {
            let (width, height) = extent(level);
            level == 0 || (width >= size && height >= size)
        })
        .last()?;
    let (width, height) = extent(level);
    let rgba8 = crate::asset::bc_decode::decode_level(
        compressed.format,
        width,
        height,
        &compressed.mips[level],
    )?;
    Some(CpuTexture {
        width,
        height,
        rgba8,
        compressed: None,
    })
}

/// One image as a square mip chain: box halvings while it is twice too big, bilinear to the side, then halvings.
/// A BC texture is decoded first, from the level just above the side, one texture per call, as the web's steps take it.
pub fn resample_layer(texture: &CpuTexture, srgb: bool, size: u32) -> Vec<Vec<u8>> {
    if texture.compressed.is_some() {
        if let Some(decoded) = decoded_level(texture, size) {
            return resample_layer(&decoded, srgb, size);
        }
        log::warn!("tracer: a compressed texture did not decode; its layer stays white");
        let opaque_white = CpuTexture {
            width: 1,
            height: 1,
            rgba8: vec![255; 4],
            compressed: None,
        };
        return resample_layer(&opaque_white, srgb, size);
    }
    let codec = Codec::new(srgb);
    let (width, height) = (texture.width.max(1), texture.height.max(1));
    let mut image = if width >= 2 * size || height >= 2 * size {
        halve_bytes(texture, &codec, (width >= 2 * size, height >= 2 * size))
    } else {
        Image {
            width,
            height,
            texels: codec.decode(&texture.rgba8),
        }
    };
    while image.width >= 2 * size || image.height >= 2 * size {
        image = image.halve(image.width >= 2 * size, image.height >= 2 * size);
    }
    if image.width != size || image.height != size {
        image = image.resize(size);
    }
    let mut levels = vec![codec.encode(&image.texels)];
    while image.width > 1 {
        image = image.halve(true, true);
        levels.push(codec.encode(&image.texels));
    }
    levels
}

/// The first halving straight from the bytes: a large image is never decoded whole.
fn halve_bytes(texture: &CpuTexture, codec: &Codec, (x, y): (bool, bool)) -> Image {
    let (width, height) = (texture.width, texture.height);
    let (sx, sy) = (1 + u32::from(x), 1 + u32::from(y));
    let (half_width, half_height) = ((width / sx).max(1), (height / sy).max(1));
    let rows: Vec<Image> = (0..half_height)
        .map(|ty| {
            let row = |dy: u32| {
                let at = ((ty * sy + dy).min(height - 1) * width * 4) as usize;
                Image {
                    width,
                    height: 1,
                    texels: codec.decode(&texture.rgba8[at..at + width as usize * 4]),
                }
            };
            let (top, bottom) = (row(0), row(sy - 1));
            let pair = Image {
                width,
                height: 2,
                texels: [top.texels, bottom.texels].concat(),
            };
            pair.halve(x, true)
        })
        .collect();
    Image {
        width: half_width,
        height: half_height,
        texels: rows.into_iter().flat_map(|row| row.texels).collect(),
    }
}

/// Linear-light RGBA in f32, so averaging an sRGB image does not darken it.
struct Image {
    width: u32,
    height: u32,
    texels: Vec<[f32; 4]>,
}

impl Image {
    fn at(&self, x: u32, y: u32) -> [f32; 4] {
        self.texels[(y.min(self.height - 1) * self.width + x.min(self.width - 1)) as usize]
    }

    /// Averages 2x2 (or 2x1, 1x2) blocks along the requested axes.
    fn halve(&self, x: bool, y: bool) -> Self {
        let (sx, sy) = (1 + u32::from(x), 1 + u32::from(y));
        let (width, height) = ((self.width / sx).max(1), (self.height / sy).max(1));
        let mut texels = Vec::with_capacity((width * height) as usize);
        for ty in 0..height {
            for tx in 0..width {
                let mut mean = [0.0f32; 4];
                // Four taps on a 2x1 block read each texel twice, so the mean stays exact.
                for (dx, dy) in [(0, 0), (sx - 1, 0), (0, sy - 1), (sx - 1, sy - 1)] {
                    let texel = self.at(tx * sx + dx, ty * sy + dy);
                    for c in 0..4 {
                        mean[c] += texel[c] * 0.25;
                    }
                }
                texels.push(mean);
            }
        }
        Self {
            width,
            height,
            texels,
        }
    }

    /// Bilinear resample to `size` square, texel centres aligned, edges clamped.
    fn resize(&self, size: u32) -> Self {
        let (fx, fy) = (
            self.width as f32 / size as f32,
            self.height as f32 / size as f32,
        );
        let mut texels = Vec::with_capacity((size * size) as usize);
        for ty in 0..size {
            let v = ((ty as f32 + 0.5) * fy - 0.5).max(0.0);
            let (y0, wy) = (v.floor() as u32, v.fract());
            for tx in 0..size {
                let u = ((tx as f32 + 0.5) * fx - 0.5).max(0.0);
                let (x0, wx) = (u.floor() as u32, u.fract());
                let lerp = |a: [f32; 4], b: [f32; 4], t: f32| {
                    std::array::from_fn(|c| a[c] + (b[c] - a[c]) * t)
                };
                let top = lerp(self.at(x0, y0), self.at(x0 + 1, y0), wx);
                let bottom = lerp(self.at(x0, y0 + 1), self.at(x0 + 1, y0 + 1), wx);
                texels.push(lerp(top, bottom, wy));
            }
        }
        Self {
            width: size,
            height: size,
            texels,
        }
    }
}

/// Byte to linear light and back; alpha and data textures map straight.
struct Codec {
    srgb: bool,
}

/// sRGB byte to linear, computed once.
fn srgb_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| std::array::from_fn(|b| crate::render::texture::srgb_to_linear(b as u8)))
}

/// Linear light quantised to 16 bits, to sRGB byte: fine enough that the darkest sRGB steps survive.
fn linear_table() -> &'static [u8] {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=u16::MAX)
            .map(|v| crate::render::texture::linear_to_srgb(f32::from(v) / f32::from(u16::MAX)))
            .collect()
    })
}

impl Codec {
    fn new(srgb: bool) -> Self {
        Self { srgb }
    }

    fn decode(&self, rgba8: &[u8]) -> Vec<[f32; 4]> {
        let table = srgb_table();
        rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .map(|t| {
                let colour = |b: u8| {
                    if self.srgb {
                        table[b as usize]
                    } else {
                        f32::from(b) / 255.0
                    }
                };
                [
                    colour(t[0]),
                    colour(t[1]),
                    colour(t[2]),
                    f32::from(t[3]) / 255.0,
                ]
            })
            .collect()
    }

    fn encode(&self, texels: &[[f32; 4]]) -> Vec<u8> {
        let table = linear_table();
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let colour = |v: f32| {
            if self.srgb {
                table[(v.clamp(0.0, 1.0) * f32::from(u16::MAX) + 0.5) as usize]
            } else {
                byte(v)
            }
        };
        texels
            .iter()
            .flat_map(|t| [colour(t[0]), colour(t[1]), colour(t[2]), byte(t[3])])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> CpuTexture {
        CpuTexture {
            width,
            height,
            rgba8: rgba.repeat((width * height) as usize),
            compressed: None,
        }
    }

    /// One BC1 block of pure red (RGB565 0xF800) per 4x4; the 4x4 level is the one just above a side of 4.
    #[test]
    fn a_bc1_texture_decodes_from_the_level_above_the_side() {
        use crate::scene::{CompressedFormat, CompressedTexture};
        let red = [0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        let texture = CpuTexture {
            width: 8,
            height: 8,
            rgba8: Vec::new(),
            compressed: Some(CompressedTexture {
                format: CompressedFormat::Bc1RgbaUnorm,
                mips: vec![red.repeat(4), red.to_vec()],
                declared_srgb: None,
                basis_source: None,
            }),
        };
        assert!(traceable(&texture));
        assert_eq!(
            decoded_level(&texture, 4).map(|t| (t.width, t.height)),
            Some((4, 4))
        );
        assert_eq!(
            decoded_level(&texture, 8).map(|t| (t.width, t.height)),
            Some((8, 8))
        );
        let level = &resample_layer(&texture, false, 4)[0];
        assert_eq!(&level[..4], &[255, 0, 0, 255]);
        let mut etc2 = texture.clone();
        if let Some(compressed) = etc2.compressed.as_mut() {
            compressed.format = CompressedFormat::Etc2Rgba8Unorm;
        }
        assert!(
            !traceable(&etc2),
            "ETC2 only exists where the adapter samples it"
        );
    }

    #[test]
    fn a_layer_has_a_full_chain_of_square_levels() {
        let levels = resample_layer(&solid(300, 70, [10, 200, 30, 128]), false, 64);
        assert_eq!(levels.len(), 7);
        for (level, data) in levels.iter().enumerate() {
            let side = 64usize >> level;
            assert_eq!(data.len(), side * side * 4, "level {level}");
            assert_eq!(&data[..4], &[10, 200, 30, 128], "a solid image stays solid");
        }
    }

    #[test]
    fn halving_an_srgb_checker_averages_in_linear_light() {
        let mut texture = solid(2, 2, [0, 0, 0, 255]);
        texture.rgba8[..4].copy_from_slice(&[255, 255, 255, 255]);
        texture.rgba8[12..].copy_from_slice(&[255, 255, 255, 255]);
        let levels = resample_layer(&texture, true, 1);
        // Half white in linear light is sRGB 188, not the byte average 128.
        assert_eq!(levels[0][0], 188);
        assert_eq!(resample_layer(&texture, false, 1)[0][0], 128);
    }

    #[test]
    fn an_upsampled_layer_keeps_its_corners() {
        let mut texture = solid(2, 1, [0, 0, 0, 255]);
        texture.rgba8[4..].copy_from_slice(&[200, 100, 50, 255]);
        let level = &resample_layer(&texture, false, 8)[0];
        assert_eq!(&level[..4], &[0, 0, 0, 255]);
        assert_eq!(&level[7 * 4..8 * 4], &[200, 100, 50, 255]);
    }

    #[test]
    fn the_layer_side_follows_the_sources_the_cap_and_the_budget() {
        let small = Arc::new(solid(100, 30, [0; 4]));
        let big = Arc::new(solid(3000, 2048, [0; 4]));
        assert_eq!(layer_size(&[Arc::clone(&small)], 1024, u64::MAX), 128);
        assert_eq!(
            layer_size(&[Arc::clone(&small), Arc::clone(&big)], 1024, u64::MAX),
            1024
        );
        let budget = 2 * layer_bytes(256);
        assert_eq!(layer_size(&[small, big], 1024, budget), 256);
        assert_eq!(layer_bytes(2), 2 * 2 * 4 + 4);
    }
}
