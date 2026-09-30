//! Radiance HDR (`.hdr`, RGBE) decoding, hand-rolled to keep an image stack out of the wasm download.
//! Only `-Y +X` decodes (other orientations are refused, not flipped); untrusted input never panics.

use crate::render::ibl::EquirectImage;

/// Cap on `width * height` (16384 x 8192), so `pixels * 16` bytes fits a 32-bit wasm `usize`.
const MAX_PIXELS: u64 = 1 << 27;

/// Everything that can be wrong with an `.hdr` file; the decoder reports here instead of panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HdrError {
    BadMagic,
    /// The file ended inside the named structure.
    TruncatedInput {
        context: &'static str,
    },
    MissingFormat,
    /// A declared FORMAT other than rgbe (in the wild: `32-bit_rle_xyze`).
    UnsupportedFormat(String),
    /// A header variable that was recognised but failed to parse.
    BadHeaderValue {
        line: String,
    },
    BadResolution {
        line: String,
    },
    /// A valid orientation this decoder chooses not to implement.
    UnsupportedOrientation {
        orientation: String,
    },
    /// Zero or beyond the internal `MAX_PIXELS` cap.
    BadDimensions {
        width: u64,
        height: u64,
    },
    /// A new-style RLE scanline whose embedded width disagrees with the image.
    ScanlineWidthMismatch {
        declared: usize,
        expected: usize,
    },
    /// A run or repeat extending past the end of its scanline.
    RleOverrun,
    /// A literal span of length zero, which would never advance the decoder.
    ZeroLengthRleLiteral,
    /// An old-style repeat marker as the first pixel of a scanline.
    RepeatBeforeFirstPixel,
}

impl std::fmt::Display for HdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic => {
                write!(f, "not a Radiance HDR file (no #?RADIANCE / #?RGBE magic)")
            }
            Self::TruncatedInput { context } => {
                write!(f, "file ends early while reading {context}")
            }
            Self::MissingFormat => {
                write!(
                    f,
                    "header has no FORMAT line (expected FORMAT=32-bit_rle_rgbe)"
                )
            }
            Self::UnsupportedFormat(format) => {
                write!(
                    f,
                    "unsupported FORMAT '{format}' (only 32-bit_rle_rgbe is supported)"
                )
            }
            Self::BadHeaderValue { line } => write!(f, "malformed header line '{line}'"),
            Self::BadResolution { line } => {
                write!(
                    f,
                    "malformed resolution line '{line}' (expected e.g. '-Y 512 +X 1024')"
                )
            }
            Self::UnsupportedOrientation { orientation } => {
                write!(
                    f,
                    "unsupported orientation '{orientation}' (only '-Y +X', rows top to bottom, is supported)"
                )
            }
            Self::BadDimensions { width, height } => {
                write!(f, "unreasonable image dimensions {width}x{height}")
            }
            Self::ScanlineWidthMismatch { declared, expected } => {
                write!(
                    f,
                    "RLE scanline declares width {declared} but the image is {expected} wide"
                )
            }
            Self::RleOverrun => write!(f, "RLE run extends past the end of its scanline"),
            Self::ZeroLengthRleLiteral => write!(f, "RLE literal span of zero length"),
            Self::RepeatBeforeFirstPixel => {
                write!(
                    f,
                    "old-style repeat marker with no preceding pixel in its scanline"
                )
            }
        }
    }
}

impl std::error::Error for HdrError {}

/// Decodes a Radiance HDR file into linear radiance, EXPOSURE divided out.
pub fn decode_hdr(bytes: &[u8]) -> Result<EquirectImage, HdrError> {
    let mut cursor = Cursor { bytes, pos: 0 };

    // Radiance writes `#?RADIANCE`; Photosphere and a few others write `#?RGBE`.
    let magic = cursor.read_line("magic line")?;
    if !(magic.starts_with("#?RADIANCE") || magic.starts_with("#?RGBE")) {
        return Err(HdrError::BadMagic);
    }

    let mut format_seen = false;
    let mut exposure = 1.0f64;
    loop {
        let line = cursor.read_line("header")?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("FORMAT=") {
            match value.trim() {
                "32-bit_rle_rgbe" => format_seen = true,
                other => return Err(HdrError::UnsupportedFormat(other.to_string())),
            }
        } else if let Some(value) = line.strip_prefix("EXPOSURE=") {
            let factor: f64 = value.trim().parse().map_err(|_| HdrError::BadHeaderValue {
                line: line.to_string(),
            })?;
            if !factor.is_finite() || factor <= 0.0 {
                return Err(HdrError::BadHeaderValue {
                    line: line.to_string(),
                });
            }
            exposure *= factor;
        }
        // Other variables (GAMMA=, PRIMARIES=, VIEW=, ...) carry nothing the renderer uses.
    }
    // Assuming rgbe without FORMAT would decode an xyze file as plausible wrong colours.
    if !format_seen {
        return Err(HdrError::MissingFormat);
    }

    let resolution = cursor.read_line("resolution line")?;
    let (width, height) = parse_resolution(resolution.trim())?;

    // Grown per scanline, not reserved from the untrusted header, so a lying size never allocates.
    let mut rgba32f = Vec::new();
    let mut scanline = vec![[0u8; 4]; width as usize];
    for _ in 0..height {
        decode_scanline(&mut cursor, &mut scanline)?;
        for rgbe in &scanline {
            let [r, g, b] = rgbe_to_linear(*rgbe, exposure);
            rgba32f.extend_from_slice(&[r, g, b, 1.0]);
        }
    }

    // Built directly: the length is `width * height * 4` by construction, what `EquirectImage::new` checks.
    Ok(EquirectImage {
        width,
        height,
        rgba32f,
    })
}

/// One RGBE quadruple to linear RGB, with the exposure correction folded in.
fn rgbe_to_linear(rgbe: [u8; 4], exposure: f64) -> [f32; 3] {
    let [r, g, b, e] = rgbe;
    // An exponent byte of 0 is the encoding of black, whatever the mantissas.
    if e == 0 {
        return [0.0; 3];
    }
    // -136 is bias 128 plus 8 mantissa-fraction bits (without them 256x too bright); f64 keeps 2^-135 nonzero.
    let scale = f64::exp2(f64::from(e) - 136.0) / exposure;
    [
        (f64::from(r) * scale) as f32,
        (f64::from(g) * scale) as f32,
        (f64::from(b) * scale) as f32,
    ]
}

fn parse_resolution(line: &str) -> Result<(u32, u32), HdrError> {
    let bad = || HdrError::BadResolution {
        line: line.to_string(),
    };
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let &[axis_a, len_a, axis_b, len_b] = tokens.as_slice() else {
        return Err(bad());
    };

    // Tell an exotic-but-valid orientation apart from a broken resolution line.
    let is_axis = |t: &str| matches!(t, "+X" | "-X" | "+Y" | "-Y");
    if !is_axis(axis_a) || !is_axis(axis_b) || axis_a.as_bytes()[1] == axis_b.as_bytes()[1] {
        return Err(bad());
    }
    if (axis_a, axis_b) != ("-Y", "+X") {
        return Err(HdrError::UnsupportedOrientation {
            orientation: format!("{axis_a} {axis_b}"),
        });
    }

    let height: u64 = len_a.parse().map_err(|_| bad())?;
    let width: u64 = len_b.parse().map_err(|_| bad())?;
    let total = width.checked_mul(height);
    if width == 0 || height == 0 || total.is_none_or(|t| t > MAX_PIXELS) {
        return Err(HdrError::BadDimensions { width, height });
    }
    Ok((width as u32, height as u32))
}

/// Decodes one scanline, choosing new-style RLE or flat the way Radiance's `freadcolrs` does.
fn decode_scanline(cursor: &mut Cursor<'_>, scanline: &mut [[u8; 4]]) -> Result<(), HdrError> {
    let width = scanline.len();
    if (8..=0x7fff).contains(&width) {
        if let Some([2, 2, hi, lo]) = cursor.peek4() {
            if hi & 0x80 == 0 {
                cursor.pos += 4;
                let declared = usize::from(hi) << 8 | usize::from(lo);
                if declared != width {
                    return Err(HdrError::ScanlineWidthMismatch {
                        declared,
                        expected: width,
                    });
                }
                return decode_rle_scanline(cursor, scanline);
            }
        }
    }
    decode_flat_scanline(cursor, scanline)
}

/// New-style RLE: four component planes, each of runs (`code > 128`) and literals (`code <= 128`).
fn decode_rle_scanline(cursor: &mut Cursor<'_>, scanline: &mut [[u8; 4]]) -> Result<(), HdrError> {
    for component in 0..4 {
        let mut x = 0usize;
        while x < scanline.len() {
            let code = cursor.take(1, "RLE code")?[0];
            if code > 128 {
                let run = usize::from(code) - 128;
                let value = cursor.take(1, "RLE run value")?[0];
                let span = scanline.get_mut(x..x + run).ok_or(HdrError::RleOverrun)?;
                for pixel in span {
                    pixel[component] = value;
                }
                x += run;
            } else {
                let count = usize::from(code);
                // A zero-length literal never advances, so a crafted file could loop forever.
                if count == 0 {
                    return Err(HdrError::ZeroLengthRleLiteral);
                }
                let bytes = cursor.take(count, "RLE literal")?;
                let span = scanline.get_mut(x..x + count).ok_or(HdrError::RleOverrun)?;
                for (pixel, &byte) in span.iter_mut().zip(bytes) {
                    pixel[component] = byte;
                }
                x += count;
            }
        }
    }
    Ok(())
}

/// Flat RGBE with old-style RLE: `(1, 1, 1, n)` repeats the previous pixel, each consecutive marker shifting 8 more.
/// Like Radiance's `oldreadcolrs`, repeats never cross scanlines.
fn decode_flat_scanline(cursor: &mut Cursor<'_>, scanline: &mut [[u8; 4]]) -> Result<(), HdrError> {
    let mut shift = 0u32;
    let mut x = 0usize;
    while x < scanline.len() {
        let bytes = cursor.take(4, "flat pixel")?;
        let quad = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if quad[0] == 1 && quad[1] == 1 && quad[2] == 1 {
            let Some(previous_index) = x.checked_sub(1) else {
                return Err(HdrError::RepeatBeforeFirstPixel);
            };
            // A fifth consecutive marker would shift by 32, a run longer than MAX_PIXELS.
            if shift > 24 {
                return Err(HdrError::RleOverrun);
            }
            let count = usize::from(quad[3]) << shift;
            // `count` can reach 255 << 24, which wraps a 32-bit wasm `usize` when added to `x`.
            let end = x.checked_add(count).ok_or(HdrError::RleOverrun)?;
            let previous = scanline[previous_index];
            let span = scanline.get_mut(x..end).ok_or(HdrError::RleOverrun)?;
            for pixel in span {
                *pixel = previous;
            }
            x = end;
            shift += 8;
        } else {
            scanline[x] = quad;
            x += 1;
            shift = 0;
        }
    }
    Ok(())
}

/// Byte reader over the input; the only place raw indexing happens.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, context: &'static str) -> Result<&'a [u8], HdrError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(HdrError::TruncatedInput { context })?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn peek4(&self) -> Option<[u8; 4]> {
        let end = self.pos.checked_add(4)?;
        self.bytes.get(self.pos..end)?.try_into().ok()
    }

    /// Next line without its `\n`/`\r`; lossy UTF-8 so a stray high byte in a comment is not fatal.
    fn read_line(&mut self, context: &'static str) -> Result<String, HdrError> {
        let rest = &self.bytes[self.pos..];
        let newline = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or(HdrError::TruncatedInput { context })?;
        let mut line = &rest[..newline];
        self.pos += newline + 1;
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        Ok(String::from_utf8_lossy(line).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inverse of [`rgbe_to_linear`], truncating like Radiance's `setcolr`.
    fn encode_rgbe([r, g, b]: [f32; 3]) -> [u8; 4] {
        let max = r.max(g).max(b);
        if max < 1e-32 {
            return [0; 4];
        }
        // frexp by hand: max = v * 2^e with v in [0.5, 1).
        let mut e = 0i32;
        let mut v = max;
        while v >= 1.0 {
            v *= 0.5;
            e += 1;
        }
        while v < 0.5 {
            v *= 2.0;
            e -= 1;
        }
        let scale = f64::from(v) * 256.0 / f64::from(max); // = 2^(8 - e)
        [
            (f64::from(r) * scale) as u8,
            (f64::from(g) * scale) as u8,
            (f64::from(b) * scale) as u8,
            (e + 128) as u8,
        ]
    }

    fn header(width: usize, height: usize, extra: &[&str]) -> Vec<u8> {
        let mut text = String::from("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n");
        for line in extra {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str(&format!("\n-Y {height} +X {width}\n"));
        text.into_bytes()
    }

    fn encode_flat(width: usize, height: usize, pixels: &[[f32; 3]], extra: &[&str]) -> Vec<u8> {
        assert_eq!(pixels.len(), width * height);
        let mut out = header(width, height, extra);
        for pixel in pixels {
            out.extend_from_slice(&encode_rgbe(*pixel));
        }
        out
    }

    fn encode_rle(width: usize, height: usize, pixels: &[[f32; 3]]) -> Vec<u8> {
        assert_eq!(pixels.len(), width * height);
        assert!((8..=0x7fff).contains(&width));
        let mut out = header(width, height, &[]);
        for row in pixels.chunks(width) {
            let rgbe: Vec<[u8; 4]> = row.iter().map(|p| encode_rgbe(*p)).collect();
            out.extend_from_slice(&[2, 2, (width >> 8) as u8, (width & 0xff) as u8]);
            for component in 0..4 {
                let plane: Vec<u8> = rgbe.iter().map(|q| q[component]).collect();
                emit_plane(&plane, &mut out);
            }
        }
        out
    }

    /// Runs of >= 3 as runs, everything else as literals, so both decoder branches run.
    fn emit_plane(plane: &[u8], out: &mut Vec<u8>) {
        let mut i = 0;
        while i < plane.len() {
            let mut run = 1;
            while i + run < plane.len() && plane[i + run] == plane[i] && run < 127 {
                run += 1;
            }
            if run >= 3 {
                out.extend_from_slice(&[128 + run as u8, plane[i]]);
                i += run;
            } else {
                let start = i;
                let mut end = start + 1;
                while end < plane.len() && end - start < 128 {
                    let mut ahead = 1;
                    while end + ahead < plane.len() && plane[end + ahead] == plane[end] && ahead < 3
                    {
                        ahead += 1;
                    }
                    if ahead >= 3 {
                        break;
                    }
                    end += 1;
                }
                out.push((end - start) as u8);
                out.extend_from_slice(&plane[start..end]);
                i = end;
            }
        }
    }

    /// Worst per-channel error relative to the brightest channel, the quantity a shared exponent bounds.
    fn worst_relative_error(decoded: &EquirectImage, expected: &[[f32; 3]]) -> f32 {
        let mut worst = 0.0f32;
        for (texel, want) in decoded.rgba32f.chunks_exact(4).zip(expected) {
            let max = want[0].max(want[1]).max(want[2]);
            if max <= 0.0 {
                assert_eq!(&texel[..3], &[0.0; 3], "black must decode to exact black");
                continue;
            }
            for channel in 0..3 {
                worst = worst.max((texel[channel] - want[channel]).abs() / max);
            }
        }
        worst
    }

    #[test]
    fn flat_pixels_round_trip_across_six_orders_of_magnitude() {
        // Width 4 is below the new-RLE minimum of 8, so this also pins flat decoding for narrow images.
        let pixels = [
            [0.001, 0.002, 0.0015],
            [0.03, 0.01, 0.02],
            [0.5, 0.25, 0.125],
            [1.0, 1.0, 1.0],
            [2.0, 4.0, 8.0],
            [100.0, 50.0, 25.0],
            [1000.0, 999.0, 512.0],
            [0.0, 0.0, 0.0],
        ];
        let decoded = decode_hdr(&encode_flat(4, 2, &pixels, &[])).expect("well-formed file");
        assert_eq!((decoded.width, decoded.height), (4, 2));

        // Truncated 8-bit mantissas bound the error at 1/128 of the brightest channel.
        let worst = worst_relative_error(&decoded, &pixels);
        eprintln!("flat round-trip worst relative error: {worst:.6}");
        assert!(
            worst < 1.0 / 128.0,
            "round-trip error {worst} exceeds the RGBE quantum"
        );
    }

    #[test]
    fn one_decodes_to_one_not_two_hundred_fifty_six() {
        // 1.0 encodes as (128, 128, 128, 129); without the -8 fraction shift it decodes as 256.0.
        assert_eq!(encode_rgbe([1.0, 1.0, 1.0]), [128, 128, 128, 129]);

        let mut file = header(1, 1, &[]);
        file.extend_from_slice(&[128, 128, 128, 129]);
        let decoded = decode_hdr(&file).expect("well-formed file");
        assert_eq!(&decoded.rgba32f[..3], &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn new_style_rle_with_runs_and_literals_matches_the_flat_encoding() {
        // A constant half (runs) and a ramp half (literals), so both plane-decoder branches run.
        let width = 32;
        let mut pixels = Vec::new();
        for y in 0..2 {
            for x in 0..width {
                pixels.push(if x < width / 2 {
                    [0.5, 0.25, 0.75]
                } else {
                    let t = (x + y) as f32;
                    [t * 0.1, t * 0.2, t * 0.05]
                });
            }
        }

        let rle = decode_hdr(&encode_rle(width, 2, &pixels)).expect("well-formed RLE file");
        let flat = decode_hdr(&encode_flat(width, 2, &pixels, &[])).expect("well-formed flat file");
        assert_eq!(
            rle.rgba32f, flat.rgba32f,
            "the two encodings must decode identically"
        );

        let worst = worst_relative_error(&rle, &pixels);
        eprintln!("RLE round-trip worst relative error: {worst:.6}");
        assert!(worst < 1.0 / 128.0);
    }

    #[test]
    fn old_style_repeat_markers_expand_and_compose_their_shifts() {
        // One literal pixel, then (1,1,1,7): eight identical pixels.
        let mut file = header(8, 1, &[]);
        file.extend_from_slice(&encode_rgbe([0.5, 0.5, 0.5]));
        file.extend_from_slice(&[1, 1, 1, 7]);
        let decoded = decode_hdr(&file).expect("old-style repeat");
        for texel in decoded.rgba32f.chunks_exact(4) {
            assert_eq!(&texel[..3], &[0.5, 0.5, 0.5]);
        }

        // Consecutive markers shift 8 bits each: pixel + repeat(2) + repeat(1 << 8) = 259 pixels.
        let width = 259;
        let mut file = header(width, 1, &[]);
        file.extend_from_slice(&encode_rgbe([0.25, 0.25, 0.25]));
        file.extend_from_slice(&[1, 1, 1, 2]);
        file.extend_from_slice(&[1, 1, 1, 1]);
        let decoded = decode_hdr(&file).expect("composed repeat");
        assert_eq!(decoded.rgba32f.len(), width * 4);
        for texel in decoded.rgba32f.chunks_exact(4) {
            assert_eq!(&texel[..3], &[0.25, 0.25, 0.25]);
        }
    }

    #[test]
    fn widths_beyond_the_rle_maximum_decode_flat() {
        // 40000 > 0x7fff, so even a scanline starting with two 2s must decode flat.
        let width = 40000;
        let pixels = vec![[0.25, 0.5, 0.75]; width];
        let decoded = decode_hdr(&encode_flat(width, 1, &pixels, &[])).expect("wide flat file");
        assert_eq!(decoded.width as usize, width);
        let worst = worst_relative_error(&decoded, &pixels);
        assert!(worst < 1.0 / 128.0);
    }

    #[test]
    fn exposure_is_divided_out_and_composes_multiplicatively() {
        // EXPOSURE is already applied to the stored pixels, so EXPOSURE=2 means half the radiance.
        let pixels = [[1.0, 1.0, 1.0]];
        let plain = decode_hdr(&encode_flat(1, 1, &pixels, &[])).expect("plain");
        let doubled = decode_hdr(&encode_flat(1, 1, &pixels, &["EXPOSURE=2.0"])).expect("exposed");
        assert_eq!(&plain.rgba32f[..3], &[1.0, 1.0, 1.0]);
        assert_eq!(&doubled.rgba32f[..3], &[0.5, 0.5, 0.5]);

        // Two lines compose into 2.0 * 0.5 = 1.0, i.e. cancel exactly.
        let cancelled = decode_hdr(&encode_flat(
            1,
            1,
            &pixels,
            &["EXPOSURE=2.0", "EXPOSURE=0.5"],
        ))
        .expect("cancelling exposures");
        assert_eq!(cancelled.rgba32f, plain.rgba32f);
    }

    #[test]
    fn every_malformed_input_is_a_typed_error_not_a_panic() {
        // Bad magic.
        assert_eq!(
            decode_hdr(b"\x89PNG\r\n\x1a\n").err(),
            Some(HdrError::BadMagic)
        );

        // A FORMAT this decoder does not speak.
        let xyze = b"#?RADIANCE\nFORMAT=32-bit_rle_xyze\n\n-Y 1 +X 1\n\0\0\0\0";
        assert!(
            matches!(decode_hdr(xyze), Err(HdrError::UnsupportedFormat(f)) if f == "32-bit_rle_xyze")
        );

        // No FORMAT at all.
        let no_format = b"#?RADIANCE\n\n-Y 1 +X 1\n\0\0\0\0";
        assert_eq!(decode_hdr(no_format).err(), Some(HdrError::MissingFormat));

        // Header ends at EOF before any resolution line.
        let no_resolution = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n";
        assert!(matches!(
            decode_hdr(no_resolution),
            Err(HdrError::TruncatedInput { .. })
        ));

        // A real orientation, refused by name rather than decoded flipped.
        let flipped = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n+Y 1 +X 1\n\0\0\0\0";
        assert!(matches!(
            decode_hdr(flipped),
            Err(HdrError::UnsupportedOrientation { orientation }) if orientation == "+Y +X"
        ));

        // Not a resolution line at all.
        let garbage = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\nfour score and seven\n";
        assert!(matches!(
            decode_hdr(garbage),
            Err(HdrError::BadResolution { .. })
        ));

        // Dimensions of zero, and dimensions whose product blows the cap.
        let zero = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 0 +X 4\n";
        assert!(matches!(
            decode_hdr(zero),
            Err(HdrError::BadDimensions { .. })
        ));
        let huge = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 65535 +X 65535\n";
        assert!(matches!(
            decode_hdr(huge),
            Err(HdrError::BadDimensions { .. })
        ));

        // Truncated mid-scanline: a 2x1 flat file with one pixel missing.
        let mut truncated = header(2, 1, &[]);
        truncated.extend_from_slice(&encode_rgbe([1.0, 1.0, 1.0]));
        assert!(matches!(
            decode_hdr(&truncated),
            Err(HdrError::TruncatedInput { .. })
        ));

        // New-style scanline declaring the wrong width.
        let mut mismatch = header(8, 1, &[]);
        mismatch.extend_from_slice(&[2, 2, 0, 9]);
        assert_eq!(
            decode_hdr(&mismatch).err(),
            Some(HdrError::ScanlineWidthMismatch {
                declared: 9,
                expected: 8
            })
        );

        // A run longer than the scanline.
        let mut overrun = header(8, 1, &[]);
        overrun.extend_from_slice(&[2, 2, 0, 8, 128 + 9, 0xaa]);
        assert_eq!(decode_hdr(&overrun).err(), Some(HdrError::RleOverrun));

        // A zero-length literal, which would never advance.
        let mut stuck = header(8, 1, &[]);
        stuck.extend_from_slice(&[2, 2, 0, 8, 0]);
        assert_eq!(
            decode_hdr(&stuck).err(),
            Some(HdrError::ZeroLengthRleLiteral)
        );

        // An old-style repeat with nothing to repeat.
        let mut orphan = header(8, 1, &[]);
        orphan.extend_from_slice(&[1, 1, 1, 3]);
        assert_eq!(
            decode_hdr(&orphan).err(),
            Some(HdrError::RepeatBeforeFirstPixel)
        );

        // An old-style repeat overrunning its scanline.
        let mut long = header(8, 1, &[]);
        long.extend_from_slice(&encode_rgbe([0.5, 0.5, 0.5]));
        long.extend_from_slice(&[1, 1, 1, 200]);
        assert_eq!(decode_hdr(&long).err(), Some(HdrError::RleOverrun));

        // A malformed EXPOSURE value.
        let bad_exposure = encode_flat(1, 1, &[[1.0, 1.0, 1.0]], &["EXPOSURE=zero"]);
        assert!(matches!(
            decode_hdr(&bad_exposure),
            Err(HdrError::BadHeaderValue { .. })
        ));
        let negative = encode_flat(1, 1, &[[1.0, 1.0, 1.0]], &["EXPOSURE=-2.0"]);
        assert!(matches!(
            decode_hdr(&negative),
            Err(HdrError::BadHeaderValue { .. })
        ));
    }

    #[test]
    fn the_rgbe_magic_and_comments_and_crlf_are_accepted() {
        let file = b"#?RGBE\r\n# made by a tool that loves carriage returns\r\nFORMAT=32-bit_rle_rgbe\r\n\r\n-Y 1 +X 1\r\n\x80\x80\x80\x81";
        let decoded = decode_hdr(file).expect("RGBE magic with CRLF header");
        assert_eq!(&decoded.rgba32f[..3], &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn exponent_zero_is_black_regardless_of_mantissas() {
        let mut file = header(1, 1, &[]);
        file.extend_from_slice(&[200, 150, 100, 0]);
        let decoded = decode_hdr(&file).expect("well-formed file");
        assert_eq!(&decoded.rgba32f[..3], &[0.0, 0.0, 0.0]);
    }
}
