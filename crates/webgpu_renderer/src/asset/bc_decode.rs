//! CPU decoding of BC1, BC3, BC5 and BC7 blocks, for an adapter without TEXTURE_COMPRESSION_BC.
//! Block layouts follow the D3D11 functional specification's BC sections; BC7's tables are its own.

use crate::scene::CompressedFormat;

/// RGBA8 texels of one `width` x `height` level, or `None` when `data` holds fewer blocks than it covers or is not BCn.
pub(crate) fn decode_level(
    format: CompressedFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Option<Vec<u8>> {
    let (blocks_wide, blocks_high) = (width.div_ceil(4) as usize, height.div_ceil(4) as usize);
    let block_bytes = format.block_bytes() as usize;
    if width == 0 || height == 0 || data.len() < blocks_wide * blocks_high * block_bytes {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    let mut rgba8 = vec![0u8; width * height * 4];
    for (index, block) in data
        .chunks_exact(block_bytes)
        .take(blocks_wide * blocks_high)
        .enumerate()
    {
        let texels = decode_block(format, block)?;
        let (bx, by) = (index % blocks_wide * 4, index / blocks_wide * 4);
        for (i, texel) in texels.iter().enumerate() {
            let (x, y) = (bx + i % 4, by + i / 4);
            // A level below 4x4, or not whole blocks, keeps only the texels inside it.
            if x < width && y < height {
                let at = (y * width + x) * 4;
                rgba8[at..at + 4].copy_from_slice(texel);
            }
        }
    }
    Some(rgba8)
}

/// The 16 texels of one block, row by row; `None` for ETC2 and ASTC, which only exist where the adapter samples them.
fn decode_block(format: CompressedFormat, block: &[u8]) -> Option<[[u8; 4]; 16]> {
    Some(match format {
        CompressedFormat::Bc1RgbaUnorm => bc1_block(block, false),
        CompressedFormat::Bc3RgbaUnorm => {
            let alpha = bc4_block(&block[..8]);
            let mut texels = bc1_block(&block[8..], true);
            for (texel, a) in texels.iter_mut().zip(alpha) {
                texel[3] = a;
            }
            texels
        }
        CompressedFormat::Bc5RgUnorm => {
            let (red, green) = (bc4_block(&block[..8]), bc4_block(&block[8..]));
            // A BC5 texture samples blue as 0 and alpha as 1, which the shaders' z rebuild keys on.
            std::array::from_fn(|i| [red[i], green[i], 0, 255])
        }
        CompressedFormat::Bc7RgbaUnorm => {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(block);
            bc7_block(&bytes)
        }
        CompressedFormat::Etc2Rgba8Unorm | CompressedFormat::Astc4x4RgbaUnorm => return None,
    })
}

fn expand_565(color: u16) -> [u32; 3] {
    let (r, g, b) = (
        u32::from(color >> 11),
        u32::from((color >> 5) & 63),
        u32::from(color & 31),
    );
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// BC1's colour block; BC3's always takes the four-colour palette, whatever the endpoint order.
fn bc1_block(block: &[u8], four_colours: bool) -> [[u8; 4]; 16] {
    let color0 = u16::from_le_bytes([block[0], block[1]]);
    let color1 = u16::from_le_bytes([block[2], block[3]]);
    let (c0, c1) = (expand_565(color0), expand_565(color1));
    let mix = |w0: u32, w1: u32| -> [u8; 4] {
        let total = w0 + w1;
        let channel = |i: usize| ((c0[i] * w0 + c1[i] * w1 + total / 2) / total) as u8;
        [channel(0), channel(1), channel(2), 255]
    };
    let palette = if four_colours || color0 > color1 {
        [mix(1, 0), mix(0, 1), mix(2, 1), mix(1, 2)]
    } else {
        [mix(1, 0), mix(0, 1), mix(1, 1), [0, 0, 0, 0]]
    };
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    std::array::from_fn(|i| palette[((indices >> (2 * i)) & 3) as usize])
}

/// One BC4 channel block: BC3's alpha, and each of BC5's two channels.
fn bc4_block(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u32::from(block[0]), u32::from(block[1]));
    let palette: [u32; 8] = std::array::from_fn(|code| {
        let code = code as u32;
        match code {
            0 => a0,
            1 => a1,
            _ if a0 > a1 => ((8 - code) * a0 + (code - 1) * a1 + 3) / 7,
            2..=5 => ((6 - code) * a0 + (code - 1) * a1 + 2) / 5,
            6 => 0,
            _ => 255,
        }
    });
    let mut bits = [0u8; 8];
    bits[..6].copy_from_slice(&block[2..8]);
    let indices = u64::from_le_bytes(bits);
    std::array::from_fn(|i| palette[((indices >> (3 * i)) & 7) as usize] as u8)
}

/// One BC7 mode's layout, as the specification's mode table lists it.
struct Bc7Mode {
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    selector_bits: u32,
    color_bits: u32,
    alpha_bits: u32,
    endpoint_pbits: bool,
    shared_pbits: bool,
    index_bits: u32,
    index2_bits: u32,
}

const fn mode(fields: [u32; 10]) -> Bc7Mode {
    Bc7Mode {
        subsets: fields[0] as usize,
        partition_bits: fields[1],
        rotation_bits: fields[2],
        selector_bits: fields[3],
        color_bits: fields[4],
        alpha_bits: fields[5],
        endpoint_pbits: fields[6] == 1,
        shared_pbits: fields[7] == 1,
        index_bits: fields[8],
        index2_bits: fields[9],
    }
}

/// Subsets, partition, rotation, selector, colour, alpha bits, endpoint and shared p-bits, both index widths.
const BC7_MODES: [Bc7Mode; 8] = [
    mode([3, 4, 0, 0, 4, 0, 1, 0, 3, 0]),
    mode([2, 6, 0, 0, 6, 0, 0, 1, 3, 0]),
    mode([3, 6, 0, 0, 5, 0, 0, 0, 2, 0]),
    mode([2, 6, 0, 0, 7, 0, 1, 0, 2, 0]),
    mode([1, 0, 2, 1, 5, 6, 0, 0, 2, 3]),
    mode([1, 0, 2, 0, 7, 8, 0, 0, 2, 2]),
    mode([1, 0, 0, 0, 7, 7, 1, 0, 4, 0]),
    mode([2, 6, 0, 0, 5, 5, 1, 0, 2, 0]),
];

const WEIGHTS2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Bit `i` is texel `i`'s subset in each two-subset partition.
const PARTITION2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800,
    0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce,
    0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc,
    0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718,
    0xccf0, 0x0fcc, 0x7744, 0xee22,
];
/// Two bits per texel give its subset in each three-subset partition.
const PARTITION3: [u32; 64] = [
    0xaa685050, 0x6a5a5040, 0x5a5a4200, 0x5450a0a8, 0xa5a50000, 0xa0a05050, 0x5555a0a0, 0x5a5a5050,
    0xaa550000, 0xaa555500, 0xaaaa5500, 0x90909090, 0x94949494, 0xa4a4a4a4, 0xa9a59450, 0x2a0a4250,
    0xa5945040, 0x0a425054, 0xa5a5a500, 0x55a0a0a0, 0xa8a85454, 0x6a6a4040, 0xa4a45000, 0x1a1a0500,
    0x0050a4a4, 0xaaa59090, 0x14696914, 0x69691400, 0xa08585a0, 0xaa821414, 0x50a4a450, 0x6a5a0200,
    0xa9a58000, 0x5090a0a8, 0xa8a09050, 0x24242424, 0x00aa5500, 0x24924924, 0x24499224, 0x50a50a50,
    0x500aa550, 0xaaaa4444, 0x66660000, 0xa5a0a5a0, 0x50a050a0, 0x69286928, 0x44aaaa44, 0x66666600,
    0xaa444444, 0x54a854a8, 0x95809580, 0x96969600, 0xa85454a8, 0x80959580, 0xaa141414, 0x96960000,
    0xaaaa1414, 0xa05050a0, 0xa0a5a5a0, 0x96000000, 0x40804080, 0xa9a8a9a8, 0xaaaaaa44, 0x2a4a5254,
];
/// The second subset's anchor texel per two-subset partition.
const ANCHOR2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];
/// The second and third subsets' anchor texels per three-subset partition.
const ANCHOR3_A: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5,
    15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5,
    10, 8, 13, 15, 12, 3, 3,
];
const ANCHOR3_B: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6,
    10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];

/// The block's bits, read least significant first.
struct Bits {
    bits: u128,
    at: u32,
}

impl Bits {
    fn read(&mut self, count: u32) -> u32 {
        let value = self.bits.checked_shr(self.at).unwrap_or(0) as u32 & ((1u32 << count) - 1);
        self.at += count;
        value
    }
}

/// Unquantized endpoints; a mode without alpha bits is opaque.
fn bc7_endpoints(mode: &Bc7Mode, bits: &mut Bits) -> [[u32; 4]; 6] {
    let count = mode.subsets * 2;
    let mut endpoints = [[0u32; 4]; 6];
    for channel in 0..3 {
        for endpoint in endpoints.iter_mut().take(count) {
            endpoint[channel] = bits.read(mode.color_bits);
        }
    }
    for endpoint in endpoints.iter_mut().take(count) {
        endpoint[3] = bits.read(mode.alpha_bits);
    }
    let (mut color_bits, mut alpha_bits) = (mode.color_bits, mode.alpha_bits);
    if mode.endpoint_pbits || mode.shared_pbits {
        let pbits: Vec<u32> = if mode.endpoint_pbits {
            (0..count).map(|_| bits.read(1)).collect()
        } else {
            (0..mode.subsets)
                .flat_map(|_| [bits.read(1)].repeat(2))
                .collect()
        };
        for (endpoint, pbit) in endpoints.iter_mut().zip(pbits) {
            for value in endpoint.iter_mut() {
                *value = (*value << 1) | pbit;
            }
        }
        color_bits += 1;
        alpha_bits += u32::from(alpha_bits > 0);
    }
    let expand = |value: u32, width: u32| {
        let value = value << (8 - width);
        value | (value >> width)
    };
    for endpoint in endpoints.iter_mut().take(count) {
        for value in endpoint.iter_mut().take(3) {
            *value = expand(*value, color_bits);
        }
        endpoint[3] = if alpha_bits == 0 {
            255
        } else {
            expand(endpoint[3], alpha_bits)
        };
    }
    endpoints
}

fn bc7_subset(subsets: usize, partition: usize, texel: usize) -> usize {
    match subsets {
        2 => usize::from((PARTITION2[partition] >> texel) & 1),
        3 => ((PARTITION3[partition] >> (2 * texel)) & 3) as usize,
        _ => 0,
    }
}

/// The index sets; each subset's anchor texel drops its top bit, which the encoder keeps zero.
fn bc7_indices(mode: &Bc7Mode, partition: usize, bits: &mut Bits) -> [[u32; 16]; 2] {
    let anchors = match mode.subsets {
        2 => [0, usize::from(ANCHOR2[partition]), 0],
        3 => [
            0,
            usize::from(ANCHOR3_A[partition]),
            usize::from(ANCHOR3_B[partition]),
        ],
        _ => [0; 3],
    };
    let mut indices = [[0u32; 16]; 2];
    for (texel, index) in indices[0].iter_mut().enumerate() {
        *index = bits.read(mode.index_bits - u32::from(anchors.contains(&texel)));
    }
    if mode.index2_bits > 0 {
        for (texel, index) in indices[1].iter_mut().enumerate() {
            *index = bits.read(mode.index2_bits - u32::from(texel == 0));
        }
    }
    indices
}

fn bc7_interpolate(e0: u32, e1: u32, index: u32, width: u32) -> u8 {
    let weight = match width {
        2 => WEIGHTS2[index as usize],
        3 => WEIGHTS3[index as usize],
        _ => WEIGHTS4[index as usize],
    };
    ((e0 * (64 - weight) + e1 * weight + 32) >> 6) as u8
}

/// One BC7 block; a block with no mode bit is reserved and decodes to transparent black.
fn bc7_block(block: &[u8; 16]) -> [[u8; 4]; 16] {
    let raw = u128::from_le_bytes(*block);
    let mode_index = raw.trailing_zeros();
    let Some(mode) = BC7_MODES.get(mode_index as usize) else {
        return [[0; 4]; 16];
    };
    let mut bits = Bits {
        bits: raw,
        at: mode_index + 1,
    };
    let partition = bits.read(mode.partition_bits) as usize;
    let rotation = bits.read(mode.rotation_bits);
    let selector = bits.read(mode.selector_bits);
    let endpoints = bc7_endpoints(mode, &mut bits);
    let indices = bc7_indices(mode, partition, &mut bits);
    // With two index sets the selector picks which one colour takes; alpha takes the other.
    let (color_set, alpha_set) = match (mode.index2_bits, selector) {
        (0, _) => ((0, mode.index_bits), (0, mode.index_bits)),
        (_, 0) => ((0, mode.index_bits), (1, mode.index2_bits)),
        _ => ((1, mode.index2_bits), (0, mode.index_bits)),
    };
    std::array::from_fn(|texel| {
        let subset = bc7_subset(mode.subsets, partition, texel);
        let (e0, e1) = (endpoints[2 * subset], endpoints[2 * subset + 1]);
        let color = |channel: usize| {
            bc7_interpolate(
                e0[channel],
                e1[channel],
                indices[color_set.0][texel],
                color_set.1,
            )
        };
        let alpha = bc7_interpolate(e0[3], e1[3], indices[alpha_set.0][texel], alpha_set.1);
        let mut rgba = [color(0), color(1), color(2), alpha];
        if rotation > 0 {
            rgba.swap(3, rotation as usize - 1);
        }
        rgba
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bc1_block_decodes_its_endpoints_and_its_punch_through_texel() {
        // Red over green, so color0 > color1: four colours, all opaque.
        let four = bc1_block(&[0x00, 0xf8, 0xe0, 0x07, 0b1110_0100, 0, 0, 0], false);
        assert_eq!(four, {
            let mut texels = [[255, 0, 0, 255]; 16];
            texels[..4].copy_from_slice(&[
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [170, 85, 0, 255],
                [85, 170, 0, 255],
            ]);
            texels
        });
        // Swapped endpoints select three colours and transparent black for index 3.
        let three = bc1_block(&[0xe0, 0x07, 0x00, 0xf8, 0b1110_0100, 0, 0, 0], false);
        assert_eq!(three[2], [128, 128, 0, 255]);
        assert_eq!(three[3], [0, 0, 0, 0]);
        let bc3 = bc1_block(&[0xe0, 0x07, 0x00, 0xf8, 0b1100_0000, 0, 0, 0], true);
        assert_eq!(
            bc3[3],
            [170, 85, 0, 255],
            "BC3 colour ignores the endpoint order"
        );
    }

    #[test]
    fn a_bc4_channel_takes_both_palettes() {
        // Codes 0, 1, 2, 7 in the first four texels: 3 bits each, least significant first.
        let codes = 0b111_010_001_000u64.to_le_bytes();
        let eight = bc4_block(&[200, 60, codes[0], codes[1], 0, 0, 0, 0]);
        assert_eq!(&eight[..4], &[200, 60, 180, 80]);
        let six = bc4_block(&[60, 200, codes[0], codes[1], 0, 0, 0, 0]);
        assert_eq!(&six[..4], &[60, 200, 88, 255]);
    }

    #[test]
    fn a_bc5_block_keeps_the_normal_and_samples_blue_as_zero() {
        let block = [220, 220, 0, 0, 0, 0, 0, 0, 40, 40, 0, 0, 0, 0, 0, 0];
        let texels = decode_level(CompressedFormat::Bc5RgUnorm, 4, 4, &block).unwrap();
        assert_eq!(&texels[..4], &[220, 40, 0, 255]);
        assert!(texels.chunks(4).all(|t| t == [220, 40, 0, 255]));
    }

    #[test]
    fn a_level_crops_to_its_size_and_refuses_too_few_blocks() {
        let blocks = [0xe0u8, 0x07, 0xe0, 0x07, 0, 0, 0, 0].repeat(4);
        let six = decode_level(CompressedFormat::Bc1RgbaUnorm, 6, 6, &blocks).unwrap();
        assert_eq!(six.len(), 6 * 6 * 4);
        assert!(six.chunks(4).all(|t| t == [0, 255, 0, 255]));
        assert_eq!(
            decode_level(CompressedFormat::Bc1RgbaUnorm, 1, 1, &blocks[..8])
                .unwrap()
                .len(),
            4
        );
        assert!(decode_level(CompressedFormat::Bc1RgbaUnorm, 8, 8, &blocks[..24]).is_none());
        assert!(decode_level(CompressedFormat::Bc7RgbaUnorm, 4, 4, &[0x40; 15]).is_none());
    }

    #[test]
    fn a_bc7_mode_6_block_reaches_both_endpoints() {
        // Mode 6: first endpoints 127 with p-bit 1, second ones 0 with p-bit 0.
        let mut raw: u128 = 1 << 6;
        let mut at = 7;
        for _ in 0..4 {
            raw |= 127 << at;
            at += 14;
        }
        raw |= 1 << 63;
        raw |= u128::MAX << 65;
        let texels = bc7_block(&raw.to_le_bytes());
        assert_eq!(texels[1], [0, 0, 0, 0], "index 15 is the second endpoint");
        raw &= !(u128::MAX << 65);
        let texels = bc7_block(&raw.to_le_bytes());
        assert_eq!(
            texels[5],
            [255, 255, 255, 255],
            "index 0 is the first endpoint"
        );
        assert_eq!(
            bc7_block(&[0; 16])[0],
            [0, 0, 0, 0],
            "a reserved block is transparent black"
        );
    }

    /// FNV-1a over 4096 xorshift blocks per mode, against basisu's `unpack_bc7` (KTX-Software lib/basisu).
    #[test]
    fn every_bc7_mode_decodes_like_the_basis_universal_reference() {
        const REFERENCE: [u32; 8] = [
            0x9f12_6f1a,
            0xe693_888a,
            0xfe95_c437,
            0x03bb_de1b,
            0x12cd_299a,
            0x909e_7ad7,
            0x5ae1_3610,
            0xae41_d8cd,
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for (mode, expected) in REFERENCE.iter().enumerate() {
            let mut hash = 2_166_136_261u32;
            for _ in 0..4096 {
                let mut block = [0u8; 16];
                block[..8].copy_from_slice(&next().to_le_bytes());
                block[8..].copy_from_slice(&next().to_le_bytes());
                block[0] = (block[0] & !((2u32 << mode) - 1) as u8) | (1u32 << mode) as u8;
                for byte in bc7_block(&block).as_flattened() {
                    hash = (hash ^ u32::from(*byte)).wrapping_mul(16_777_619);
                }
            }
            assert_eq!(hash, *expected, "mode {mode}");
        }
    }
}
