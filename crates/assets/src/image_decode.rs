//! Image files to RGBA8 for both renderers: PNG, JPEG, BMP, TGA, GIF, PNM, Radiance HDR and WebP through `image`.
//! Channels expand as stb's `STBI_rgb_alpha` does, which the C++ engine decoded with before: grey fills RGB,
//! 16-bit keeps the high byte, HDR is gamma-encoded with 1/2.2 and alpha kept linear; sizes are capped at MAX_PIXELS.
//! An animated WebP is its first frame on a transparent canvas, the frame a browser's `createImageBitmap` takes.

use anyhow::Context as _;
use image::{DynamicImage, ImageFormat};

use crate::scene::CpuTexture;

/// The formats decoded here; PSD and PIC stay with the C++ engine's stb, the rest are not read.
const FORMATS: [ImageFormat; 8] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Bmp,
    ImageFormat::Tga,
    ImageFormat::Gif,
    ImageFormat::Pnm,
    ImageFormat::Hdr,
    ImageFormat::WebP,
];

/// The format `encoded` is in: by its signature, which a wrong MIME type or extension cannot override, else by MIME
/// type, else by the name's extension (TGA has no signature).
pub fn image_format(
    encoded: &[u8],
    mime_type: Option<&str>,
    name: Option<&str>,
) -> Option<ImageFormat> {
    image::guess_format(encoded)
        .ok()
        .filter(|format| FORMATS.contains(format))
        .or_else(|| mime_type.and_then(format_of_mime))
        .or_else(|| name.and_then(format_of_name))
}

fn format_of_mime(mime_type: &str) -> Option<ImageFormat> {
    match mime_type.to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/bmp" | "image/x-bmp" => Some(ImageFormat::Bmp),
        "image/x-tga" | "image/tga" | "image/x-targa" => Some(ImageFormat::Tga),
        "image/gif" => Some(ImageFormat::Gif),
        "image/x-portable-anymap"
        | "image/x-portable-bitmap"
        | "image/x-portable-graymap"
        | "image/x-portable-pixmap"
        | "image/x-portable-arbitrarymap" => Some(ImageFormat::Pnm),
        "image/vnd.radiance" => Some(ImageFormat::Hdr),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

fn format_of_name(name: &str) -> Option<ImageFormat> {
    let extension = name.rsplit_once('.')?.1;
    ImageFormat::from_extension(extension).filter(|format| FORMATS.contains(format))
}

/// The MIME type a packed image of `format` carries, which [`image_format`] reads back.
pub fn mime_type_of(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::Bmp => Some("image/bmp"),
        ImageFormat::Tga => Some("image/x-tga"),
        ImageFormat::Gif => Some("image/gif"),
        ImageFormat::Pnm => Some("image/x-portable-anymap"),
        ImageFormat::Hdr => Some("image/vnd.radiance"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

/// The largest image either renderer decodes, 16384x16384: a hostile header cannot make a load allocate more.
pub const MAX_PIXELS: u64 = 1 << 28;

/// Refuses a `width` x `height` image over [`MAX_PIXELS`] or with a zero side, before anything is allocated for it.
pub fn check_dimensions(width: u32, height: u32) -> anyhow::Result<()> {
    let pixels = u64::from(width) * u64::from(height);
    anyhow::ensure!(
        pixels > 0 && pixels <= MAX_PIXELS,
        "a {width}x{height} image, over the {MAX_PIXELS}-pixel cap or empty"
    );
    Ok(())
}

/// The size a PSD or PIC header names, the two formats only the engine's stb reads; `None` for anything else.
pub fn stb_only_dimensions(encoded: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| {
        Some(u32::from_be_bytes(
            encoded.get(at..at + 4)?.try_into().ok()?,
        ))
    };
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes(
            encoded.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    if encoded.starts_with(b"8BPS") {
        return Some((be32(18)?, be32(14)?));
    }
    if encoded.starts_with(&[0x53, 0x80, 0xf6, 0x34]) {
        return Some((be16(88)?, be16(90)?));
    }
    None
}

/// `encoded` decoded to tightly packed RGBA8; an error names a format this crate does not read or a size over the cap.
pub fn decode_rgba8(
    encoded: &[u8],
    mime_type: Option<&str>,
    name: Option<&str>,
) -> anyhow::Result<CpuTexture> {
    decode_rgba8_into(encoded, mime_type, name, |width, height| {
        (width as usize) * (height as usize) * 4
    })
}

/// [`decode_rgba8`] into a buffer with `capacity(width, height)` bytes, so a mip chain grows without a reallocation.
pub fn decode_rgba8_into(
    encoded: &[u8],
    mime_type: Option<&str>,
    name: Option<&str>,
    capacity: impl Fn(u32, u32) -> usize,
) -> anyhow::Result<CpuTexture> {
    use image::ImageDecoder as _;
    let format = image_format(encoded, mime_type, name).context(
        "not an image format this loader reads (PNG, JPEG, BMP, TGA, GIF, PNM, HDR or WebP)",
    )?;
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(encoded), format)
        .into_dimensions()
        .with_context(|| format!("reading the {format:?} header"))?;
    check_dimensions(width, height)?;
    if format == ImageFormat::WebP && animated_webp(encoded) {
        log::warn!(
            "{}: an animated WebP, which EXT_texture_webp asks not to be; its first frame is the texture",
            name.unwrap_or("an embedded image")
        );
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(encoded), format);
    // The cap above bounds the texels; image's default 512 MiB would refuse a capped 16384x16384 RGBA image.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_PIXELS * 16);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .with_context(|| format!("decoding {format:?}"))?;
    let (width, height) = decoder.dimensions();
    check_dimensions(width, height)?;
    let mut rgba8 =
        Vec::with_capacity(capacity(width, height).max((width as usize) * (height as usize) * 4));
    if decoder.color_type() == image::ColorType::Rgba8 {
        rgba8.resize((width as usize) * (height as usize) * 4, 0);
        decoder
            .read_image(&mut rgba8)
            .with_context(|| format!("decoding {format:?}"))?;
    } else {
        let decoded =
            DynamicImage::from_decoder(decoder).with_context(|| format!("decoding {format:?}"))?;
        expand_to_rgba8(decoded, &mut rgba8);
    }
    Ok(CpuTexture {
        width,
        height,
        rgba8,
        compressed: None,
    })
}

/// Whether `encoded` is an extended WebP whose VP8X chunk sets the animation flag.
fn animated_webp(encoded: &[u8]) -> bool {
    encoded.starts_with(b"RIFF")
        && encoded.get(8..16) == Some(b"WEBPVP8X")
        && encoded.get(20).is_some_and(|flags| flags & 0x02 != 0)
}

/// stb's `stbi__hdr_to_ldr` for a colour channel: gamma 1/2.2, rounded; NaN and negatives read 0.
fn hdr_to_ldr(value: f32) -> u8 {
    (value.max(0.0).powf(1.0 / 2.2) * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// stb's expansion into `out`: L to LLL1, LA to LLLA, RGB to RGB1; 16-bit channels keep their high byte.
fn expand_to_rgba8(image: DynamicImage, out: &mut Vec<u8>) {
    fn expand<T: Copy>(texels: &[T], channels: usize, byte: impl Fn(T) -> u8, out: &mut Vec<u8>) {
        for texel in texels.chunks_exact(channels) {
            let at = |i: usize| byte(texel[i]);
            match channels {
                1 => out.extend_from_slice(&[at(0), at(0), at(0), 255]),
                2 => out.extend_from_slice(&[at(0), at(0), at(0), at(1)]),
                3 => out.extend_from_slice(&[at(0), at(1), at(2), 255]),
                _ => out.extend_from_slice(&[at(0), at(1), at(2), at(3)]),
            }
        }
    }
    let high = |value: u16| (value >> 8) as u8;
    let alpha = |value: f32| (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    match image {
        DynamicImage::ImageRgba8(rgba) => out.extend_from_slice(rgba.as_raw()),
        DynamicImage::ImageRgb8(rgb) => expand(rgb.as_raw(), 3, |v| v, out),
        DynamicImage::ImageLuma8(grey) => expand(grey.as_raw(), 1, |v| v, out),
        DynamicImage::ImageLumaA8(grey) => expand(grey.as_raw(), 2, |v| v, out),
        DynamicImage::ImageRgba16(rgba) => expand(rgba.as_raw(), 4, high, out),
        DynamicImage::ImageRgb16(rgb) => expand(rgb.as_raw(), 3, high, out),
        DynamicImage::ImageLuma16(grey) => expand(grey.as_raw(), 1, high, out),
        DynamicImage::ImageLumaA16(grey) => expand(grey.as_raw(), 2, high, out),
        DynamicImage::ImageRgb32F(rgb) => expand(rgb.as_raw(), 3, hdr_to_ldr, out),
        DynamicImage::ImageRgba32F(rgba) => {
            for texel in rgba.as_raw().as_chunks::<4>().0 {
                let [r, g, b, a] = *texel;
                out.extend_from_slice(&[hdr_to_ldr(r), hdr_to_ldr(g), hdr_to_ldr(b), alpha(a)]);
            }
        }
        other => out.extend_from_slice(other.into_rgba8().as_raw()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn two_texels() -> image::RgbImage {
        image::RgbImage::from_raw(2, 1, vec![200, 10, 20, 30, 40, 250]).unwrap()
    }

    #[test]
    fn every_listed_format_decodes_to_the_same_texels() {
        let rgb = DynamicImage::ImageRgb8(two_texels());
        let expected = [200, 10, 20, 255, 30, 40, 250, 255];
        // image writes WebP lossless.
        for format in [
            ImageFormat::Png,
            ImageFormat::Bmp,
            ImageFormat::Tga,
            ImageFormat::Pnm,
            ImageFormat::WebP,
        ] {
            let bytes = encode(rgb.clone(), format);
            let decoded = decode_rgba8(
                &bytes,
                None,
                Some(&format!("t.{}", format.extensions_str()[0])),
            )
            .unwrap_or_else(|e| panic!("{format:?}: {e:#}"));
            assert_eq!((decoded.width, decoded.height), (2, 1), "{format:?}");
            assert_eq!(decoded.rgba8, expected, "{format:?}");
        }
        // GIF quantizes to a palette and JPEG is lossy: the size and opaque alpha are what they promise.
        for format in [ImageFormat::Gif, ImageFormat::Jpeg] {
            let decoded = decode_rgba8(&encode(rgb.clone(), format), None, None).unwrap();
            assert_eq!(
                (decoded.width, decoded.height, decoded.rgba8.len()),
                (2, 1, 8)
            );
            assert_eq!(
                (decoded.rgba8[3], decoded.rgba8[7]),
                (255, 255),
                "{format:?}"
            );
        }
    }

    #[test]
    fn hdr_is_gamma_encoded_as_stb_does() {
        let hdr = image::Rgb32FImage::from_raw(1, 1, vec![0.5, 4.0, 0.0]).unwrap();
        let bytes = encode(DynamicImage::ImageRgb32F(hdr), ImageFormat::Hdr);
        let decoded = decode_rgba8(&bytes, Some("image/vnd.radiance"), None).unwrap();
        assert_eq!(decoded.rgba8[1..], [255, 0, 255]);
        // 0.5^(1/2.2) * 255 = 186.2; a linear clamp would give 128.
        assert!(
            (185..=187).contains(&decoded.rgba8[0]),
            "{}",
            decoded.rgba8[0]
        );
        assert_eq!(hdr_to_ldr(f32::NAN), 0);
        let alpha = image::Rgba32FImage::from_raw(1, 1, vec![1.0, 1.0, 1.0, 0.5]).unwrap();
        let mut out = Vec::new();
        expand_to_rgba8(DynamicImage::ImageRgba32F(alpha), &mut out);
        assert_eq!(out, [255, 255, 255, 128], "alpha stays linear");
    }

    #[test]
    fn an_image_over_the_pixel_cap_is_refused_from_its_header() {
        // The BMP header claims 32768x16384: 2^29 pixels, twice the cap.
        let mut bmp = encode(DynamicImage::ImageRgb8(two_texels()), ImageFormat::Bmp);
        bmp[18..22].copy_from_slice(&32768u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&16384u32.to_le_bytes());
        let error = decode_rgba8(&bmp, None, None).unwrap_err();
        assert!(format!("{error:#}").contains("cap"), "{error:#}");
        assert!(check_dimensions(16384, 16384).is_ok());
        assert!(check_dimensions(16385, 16384).is_err() && check_dimensions(0, 4).is_err());
    }

    #[test]
    fn psd_and_pic_headers_name_their_size() {
        let mut psd = b"8BPS\0\x01\0\0\0\0\0\0\0\x03".to_vec();
        psd.extend_from_slice(&7u32.to_be_bytes());
        psd.extend_from_slice(&9u32.to_be_bytes());
        assert_eq!(
            stb_only_dimensions(&psd),
            Some((9, 7)),
            "width after height"
        );
        let mut pic = vec![0u8; 92];
        pic[..4].copy_from_slice(&[0x53, 0x80, 0xf6, 0x34]);
        pic[88..90].copy_from_slice(&640u16.to_be_bytes());
        pic[90..92].copy_from_slice(&480u16.to_be_bytes());
        assert_eq!(stb_only_dimensions(&pic), Some((640, 480)));
        assert_eq!(stb_only_dimensions(b"\x89PNG"), None);
        assert_eq!(stb_only_dimensions(b"8BPS"), None, "a truncated header");
    }

    #[test]
    fn grey_and_alpha_expand_as_stb_does() {
        let la = image::GrayAlphaImage::from_raw(1, 1, vec![90, 30]).unwrap();
        let decoded = decode_rgba8(
            &encode(DynamicImage::ImageLumaA8(la), ImageFormat::Png),
            None,
            None,
        )
        .unwrap();
        assert_eq!(decoded.rgba8, [90, 90, 90, 30]);
        let grey16 =
            image::ImageBuffer::<image::Luma<u16>, _>::from_raw(1, 1, vec![0x80ff]).unwrap();
        let decoded = decode_rgba8(
            &encode(DynamicImage::ImageLuma16(grey16), ImageFormat::Png),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            decoded.rgba8,
            [0x80, 0x80, 0x80, 255],
            "the high byte, as stb keeps"
        );
    }

    #[test]
    fn the_format_comes_from_the_bytes_then_the_mime_type_then_the_name() {
        let png = encode(DynamicImage::ImageRgb8(two_texels()), ImageFormat::Png);
        let tga = encode(DynamicImage::ImageRgb8(two_texels()), ImageFormat::Tga);
        assert_eq!(image_format(&png, None, None), Some(ImageFormat::Png));
        assert_eq!(image_format(&tga, None, None), None, "TGA has no signature");
        assert_eq!(
            image_format(&tga, None, Some("a/b.TGA")),
            Some(ImageFormat::Tga)
        );
        assert_eq!(
            image_format(&tga, Some("image/x-tga"), Some("b.png")),
            Some(ImageFormat::Tga)
        );
        assert_eq!(
            image_format(&png, Some("image/jpg"), Some("b.jpg")),
            Some(ImageFormat::Png),
            "the signature wins over a wrong MIME type and extension"
        );
        assert!(decode_rgba8(&tga, None, Some("b.tga")).is_ok());
        let webp = include_bytes!("../tests/assets/ramp.webp");
        assert_eq!(
            image_format(webp, Some("image/png"), Some("b.png")),
            Some(ImageFormat::WebP),
            "RIFF....WEBP wins over a PNG MIME type and extension"
        );
        let unsigned = b"RIFF\0\0\0\0WEBQ";
        assert_eq!(image_format(unsigned, None, None), None);
        assert_eq!(
            image_format(unsigned, Some("image/webp"), Some("b.png")),
            Some(ImageFormat::WebP)
        );
        assert_eq!(
            image_format(unsigned, None, Some("a/b.WEBP")),
            Some(ImageFormat::WebP)
        );
        for format in FORMATS {
            let mime = mime_type_of(format).unwrap();
            assert_eq!(format_of_mime(mime), Some(format), "{mime} reads back");
        }
    }

    /// `ramp.webp` is basis.rs's 32x32 `encoded_ramp`, lossless; `ramp_alpha_lossy.webp` the ramp at quality 80 with
    /// alpha falling along y (libwebp through ffmpeg 8.0.1).
    fn ramp(x: usize, y: usize, alpha: bool) -> [u8; 4] {
        let a = if alpha { 255 - y * 255 / 31 } else { 255 };
        [x * 255 / 31, y * 255 / 31, 255 - (x + y) * 255 / 62, a].map(|v| v as u8)
    }

    /// Mean and max absolute difference of `rgba8` from the 32x32 ramp, colour and alpha apart.
    fn ramp_error(rgba8: &[u8], alpha: bool) -> ([f64; 2], [u8; 2]) {
        let (mut sum, mut max) = ([0.0; 2], [0u8; 2]);
        for (index, texel) in rgba8.as_chunks::<4>().0.iter().enumerate() {
            let expected = ramp(index % 32, index / 32, alpha);
            for (channel, (got, want)) in texel.iter().zip(expected).enumerate() {
                let (kind, diff) = (usize::from(channel == 3), got.abs_diff(want));
                sum[kind] += f64::from(diff);
                max[kind] = max[kind].max(diff);
            }
        }
        ([sum[0] / (32.0 * 32.0 * 3.0), sum[1] / (32.0 * 32.0)], max)
    }

    #[test]
    fn a_lossless_webp_keeps_every_texel_and_a_lossy_one_its_alpha() {
        let decoded =
            decode_rgba8(include_bytes!("../tests/assets/ramp.webp"), None, None).unwrap();
        assert_eq!((decoded.width, decoded.height), (32, 32));
        assert_eq!(
            ramp_error(&decoded.rgba8, false),
            ([0.0; 2], [0; 2]),
            "RGB to RGB1, exactly"
        );
        let rgba =
            image::RgbaImage::from_raw(2, 1, vec![200, 10, 20, 0, 30, 40, 250, 128]).unwrap();
        let decoded = decode_rgba8(
            &encode(DynamicImage::ImageRgba8(rgba), ImageFormat::WebP),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            decoded.rgba8,
            [200, 10, 20, 0, 30, 40, 250, 128],
            "lossless alpha"
        );
        let lossy = include_bytes!("../tests/assets/ramp_alpha_lossy.webp");
        let decoded = decode_rgba8(lossy, Some("image/webp"), None).unwrap();
        assert_eq!((decoded.width, decoded.height), (32, 32));
        let (mean, max) = ramp_error(&decoded.rgba8, true);
        eprintln!(
            "lossy WebP: colour mean {:.3} max {}, alpha mean {:.3} max {}",
            mean[0], max[0], mean[1], max[1]
        );
        // Quality 80 with 4:2:0 chroma on a 32x32 ramp: 3.1 off here, 4.8 through ffmpeg 8.0.1's own decoder.
        assert!(mean[0] < 4.0 && max[1] == 0, "{mean:?} {max:?}");
        let (swapped, _) = ramp_error(&rgba8_transposed(&decoded.rgba8), true);
        assert!(
            swapped[0] > 20.0,
            "the ramp runs along its own axes: {swapped:?}"
        );
    }

    /// `animated_two_frames.webp`: 16x16, lossless, no alpha; a red left and blue right half, then solid green
    /// (ffmpeg 8.0.1's libwebp_anim from raw RGBA frames, `-lossless 1 -loop 0`).
    #[test]
    fn an_animated_webp_decodes_to_its_first_frame() {
        let file = include_bytes!("../tests/assets/animated_two_frames.webp");
        assert!(animated_webp(file));
        assert!(!animated_webp(include_bytes!("../tests/assets/ramp.webp")));
        assert!(!animated_webp(include_bytes!(
            "../tests/assets/ramp_alpha_lossy.webp"
        )));
        let decoded = decode_rgba8(file, Some("image/webp"), Some("animated.webp")).unwrap();
        assert_eq!((decoded.width, decoded.height), (16, 16));
        let first: Vec<u8> = (0..16 * 16)
            .flat_map(|i| {
                if i % 16 < 8 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        assert!(decoded.rgba8 == first, "the first frame, texel for texel");
    }

    fn rgba8_transposed(rgba8: &[u8]) -> Vec<u8> {
        let texels = rgba8.as_chunks::<4>().0;
        (0..32 * 32)
            .flat_map(|i| texels[(i % 32) * 32 + i / 32])
            .collect()
    }

    #[test]
    fn psd_a_broken_webp_and_garbage_are_errors_not_panics() {
        assert!(decode_rgba8(b"8BPS\0\x01", None, Some("x.psd")).is_err());
        let webp = decode_rgba8(b"RIFF\0\0\0\0WEBPVP8 ", Some("image/webp"), None).unwrap_err();
        assert!(
            format!("{webp:#}").contains("reading the WebP header"),
            "{webp:#}"
        );
        assert!(decode_rgba8(b"\x89PNG\r\n\x1a\n broken", None, None).is_err());
        assert!(decode_rgba8(&[], None, Some("x.tga")).is_err());
    }
}
