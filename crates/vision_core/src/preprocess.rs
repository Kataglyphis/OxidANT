//! A strided RGB, BGR or RGBA frame to the model's NCHW float tensor, stretched or letterboxed.

use std::fmt;

/// Standard YOLO letterbox padding gray (114/255 ≈ 0.447).
pub const LETTERBOX_FILL_VALUE: f32 = 114.0 / 255.0;

/// Where the frame went in the model's input: what [`unmap_point`] undoes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageMapping {
    pub src_w: u32,
    pub src_h: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub pad_x: f32,
    pub pad_y: f32,
}

/// The byte order of a frame's pixels; the model reads RGB planes, so BGR is swapped on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    Bgr,
    Rgba,
}

impl PixelFormat {
    /// Bytes per pixel.
    pub fn channels(self) -> usize {
        match self {
            Self::Rgb | Self::Bgr => 3,
            Self::Rgba => 4,
        }
    }

    /// The byte offsets of red and blue inside a pixel.
    fn red_and_blue(self) -> (usize, usize) {
        match self {
            Self::Rgb | Self::Rgba => (0, 2),
            Self::Bgr => (2, 0),
        }
    }
}

/// Stretch fills the whole input; letterbox keeps the aspect ratio and pads with [`LETTERBOX_FILL_VALUE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeMode {
    Stretch,
    Letterbox,
}

/// One frame as a camera or decoder hands it over: rows `row_stride_bytes` apart, from `plane_offset_bytes` on.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub data: &'a [u8],
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    /// 0 means packed rows, `width * channels` bytes apart, as GStreamer's and the C++ detector's frames say.
    pub row_stride_bytes: usize,
    pub plane_offset_bytes: usize,
}

/// Why a frame or an input size was refused; the C++ detector refuses the same (`yolo_math.cpp`'s `preprocess_frame_into`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreprocessError {
    /// The frame has no pixels.
    ZeroSizeFrame { width: u32, height: u32 },
    /// The model's input has no pixels.
    ZeroSizeInput { width: u32, height: u32 },
    /// A size is beyond what this machine can address.
    SizeOverflow,
    /// The rows overlap: the stride is shorter than one row of pixels.
    StrideShorterThanRow { stride: usize, row_bytes: usize },
    /// The last row ends past the buffer.
    BufferTooShort { needed: usize, got: usize },
    /// A packed RGBA buffer that is not `width * height * 4` bytes long.
    LengthMismatch { got: usize, expected: usize },
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ZeroSizeFrame { width, height } => {
                write!(f, "Image dimensions must be positive, got {width}x{height}")
            }
            Self::ZeroSizeInput { width, height } => {
                write!(
                    f,
                    "Model input dimensions must be positive, got {width}x{height}"
                )
            }
            Self::SizeOverflow => write!(f, "Image or model input dimensions overflow"),
            Self::StrideShorterThanRow { stride, row_bytes } => {
                write!(
                    f,
                    "Row stride {stride} is shorter than a row of {row_bytes} bytes"
                )
            }
            Self::BufferTooShort { needed, got } => {
                write!(
                    f,
                    "Frame buffer too short: got {got} bytes, the last row ends at {needed}"
                )
            }
            Self::LengthMismatch { got, expected } => {
                write!(
                    f,
                    "RGBA buffer length mismatch: got {got}, expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for PreprocessError {}

/// `value / 255` for every byte: a lookup gives the floats the division gave, without dividing per pixel.
static UNIT_FLOATS: [f32; 256] = {
    let mut table = [0.0; 256];
    let mut value = 0;
    while value < 256 {
        table[value] = value as f32 / 255.0;
        value += 1;
    }
    table
};

/// A checked frame: its stride resolved and every row inside the buffer.
struct Checked {
    channels: usize,
    stride: usize,
    row_bytes: usize,
    plane: usize,
}

/// The refusals of `yolo_math.cpp`'s `preprocess_frame_into`, with `checked_mul` for each size.
fn check(frame: &Frame, dst_w: u32, dst_h: u32) -> Result<Checked, PreprocessError> {
    let (width, height) = (frame.width, frame.height);
    if dst_w == 0 || dst_h == 0 {
        return Err(PreprocessError::ZeroSizeInput {
            width: dst_w,
            height: dst_h,
        });
    }
    if width == 0 || height == 0 {
        return Err(PreprocessError::ZeroSizeFrame { width, height });
    }
    let channels = frame.format.channels();
    let row_bytes = (width as usize)
        .checked_mul(channels)
        .ok_or(PreprocessError::SizeOverflow)?;
    let plane = (dst_w as usize)
        .checked_mul(dst_h as usize)
        .ok_or(PreprocessError::SizeOverflow)?;
    plane.checked_mul(3).ok_or(PreprocessError::SizeOverflow)?;
    let stride = match frame.row_stride_bytes {
        0 => row_bytes,
        stride => stride,
    };
    if stride < row_bytes {
        return Err(PreprocessError::StrideShorterThanRow { stride, row_bytes });
    }
    let got = frame.data.len();
    let needed = (height as usize - 1)
        .checked_mul(stride)
        .and_then(|rows| rows.checked_add(row_bytes))
        .and_then(|rows| rows.checked_add(frame.plane_offset_bytes))
        .ok_or(PreprocessError::SizeOverflow)?;
    if needed > got {
        return Err(PreprocessError::BufferTooShort { needed, got });
    }
    Ok(Checked {
        channels,
        stride,
        row_bytes,
        plane,
    })
}

/// The content size and padding of `mode` for a `src` frame in a `dst` input.
fn content_and_padding(
    src: (u32, u32),
    dst: (u32, u32),
    mode: ResizeMode,
) -> ((u32, u32), (u32, u32)) {
    let ((src_w, src_h), (dst_w, dst_h)) = (src, dst);
    let (new_w, new_h) = match mode {
        ResizeMode::Stretch => (dst_w, dst_h),
        ResizeMode::Letterbox => {
            let scale = f64::min(dst_w as f64 / src_w as f64, dst_h as f64 / src_h as f64);
            let new_w = ((src_w as f64 * scale).round() as u32).clamp(1, dst_w);
            let new_h = ((src_h as f64 * scale).round() as u32).clamp(1, dst_h);
            (new_w, new_h)
        }
    };
    ((new_w, new_h), ((dst_w - new_w) / 2, (dst_h - new_h) / 2))
}

/// Preprocesses `frame` into `buf` as the `dst_w` x `dst_h` NCHW tensor; the floats are those of `pixel / 255`.
pub fn preprocess_frame_into(
    frame: &Frame,
    dst_w: u32,
    dst_h: u32,
    mode: ResizeMode,
    buf: &mut Vec<f32>,
) -> Result<ImageMapping, PreprocessError> {
    let checked = check(frame, dst_w, dst_h)?;
    let src = (frame.width, frame.height);
    let ((new_w, new_h), (pad_x, pad_y)) = content_and_padding(src, (dst_w, dst_h), mode);
    let total = 3 * checked.plane;
    if mode == ResizeMode::Letterbox {
        buf.clear();
        buf.resize(total, LETTERBOX_FILL_VALUE);
    } else {
        // Every element is written below, so only a changed size touches the allocation.
        buf.resize(total, 0.0);
    }
    resize_into_planes(frame, &checked, buf, dst_w, (pad_x, pad_y), (new_w, new_h));
    Ok(ImageMapping {
        src_w: frame.width,
        src_h: frame.height,
        scale_x: new_w as f32 / frame.width as f32,
        scale_y: new_h as f32 / frame.height as f32,
        pad_x: pad_x as f32,
        pad_y: pad_y as f32,
    })
}

/// Nearest-neighbour resize of `frame` into the `new_w` x `new_h` block of the three planes at (`pad_x`, `pad_y`).
fn resize_into_planes(
    frame: &Frame,
    checked: &Checked,
    buf: &mut [f32],
    dst_w: u32,
    (pad_x, pad_y): (u32, u32),
    (new_w, new_h): (u32, u32),
) {
    let (src_w, src_h) = (frame.width as u64, frame.height as u64);
    let channels = checked.channels;
    // Every row samples the same columns, so their byte offsets are divided out once per frame.
    let columns: Vec<usize> = (0..new_w as u64)
        .map(|dx| (dx * src_w / new_w as u64) as usize * channels)
        .collect();
    let (red_at, blue_at) = frame.format.red_and_blue();
    let (red, rest) = buf.split_at_mut(checked.plane);
    let (green, blue) = rest.split_at_mut(checked.plane);
    for dy in 0..new_h as usize {
        let sy = (dy as u64 * src_h / new_h as u64) as usize;
        let first = frame.plane_offset_bytes + sy * checked.stride;
        let row = &frame.data[first..first + checked.row_bytes];
        let start = (dy + pad_y as usize) * dst_w as usize + pad_x as usize;
        let end = start + new_w as usize;
        let planes = red[start..end]
            .iter_mut()
            .zip(&mut green[start..end])
            .zip(&mut blue[start..end]);
        for (((r, g), b), &offset) in planes.zip(&columns) {
            *r = UNIT_FLOATS[row[offset + red_at] as usize];
            *g = UNIT_FLOATS[row[offset + 1] as usize];
            *b = UNIT_FLOATS[row[offset + blue_at] as usize];
        }
    }
}

/// A packed RGBA buffer of exactly `src_w * src_h * 4` bytes as a [`Frame`].
fn packed_rgba(rgba: &[u8], src_w: u32, src_h: u32) -> Result<Frame<'_>, PreprocessError> {
    if src_w == 0 || src_h == 0 {
        return Err(PreprocessError::ZeroSizeFrame {
            width: src_w,
            height: src_h,
        });
    }
    let expected = (src_w as usize)
        .checked_mul(src_h as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(PreprocessError::SizeOverflow)?;
    if rgba.len() != expected {
        return Err(PreprocessError::LengthMismatch {
            got: rgba.len(),
            expected,
        });
    }
    Ok(Frame {
        data: rgba,
        format: PixelFormat::Rgba,
        width: src_w,
        height: src_h,
        row_stride_bytes: 0,
        plane_offset_bytes: 0,
    })
}

/// [`preprocess_frame_into`] letterboxed, for a packed RGBA buffer of exactly `src_w * src_h * 4` bytes.
pub fn rgba_to_nchw_f32_letterboxed(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    buf: &mut Vec<f32>,
) -> Result<ImageMapping, PreprocessError> {
    let frame = packed_rgba(rgba, src_w, src_h)?;
    preprocess_frame_into(&frame, dst_w, dst_h, ResizeMode::Letterbox, buf)
}

/// [`preprocess_frame_into`] stretched, for a packed RGBA buffer of exactly `src_w * src_h * 4` bytes.
pub fn rgba_to_nchw_f32_stretched(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    buf: &mut Vec<f32>,
) -> Result<ImageMapping, PreprocessError> {
    let frame = packed_rgba(rgba, src_w, src_h)?;
    preprocess_frame_into(&frame, dst_w, dst_h, ResizeMode::Stretch, buf)
}

/// A point in model-input pixels back in frame pixels.
#[inline]
pub fn unmap_point(x: f32, y: f32, mapping: ImageMapping) -> (f32, f32) {
    let x = (x - mapping.pad_x) / mapping.scale_x.max(1e-6);
    let y = (y - mapping.pad_y) / mapping.scale_y.max(1e-6);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formula the shared contract fixes, one division per pixel and channel.
    fn reference(rgba: &[u8], src: (u32, u32), dst: (u32, u32), mode: ResizeMode) -> Vec<f32> {
        let ((src_w, src_h), (dst_w, dst_h)) = (src, dst);
        let ((new_w, new_h), (pad_x, pad_y)) = content_and_padding(src, dst, mode);
        let fill = if mode == ResizeMode::Letterbox {
            LETTERBOX_FILL_VALUE
        } else {
            0.0
        };
        let plane = (dst_w * dst_h) as usize;
        let mut out = vec![fill; 3 * plane];
        for dy in 0..new_h {
            let sy = (dy as u64 * src_h as u64 / new_h as u64) as usize;
            for dx in 0..new_w {
                let sx = (dx as u64 * src_w as u64 / new_w as u64) as usize;
                let at = ((dy + pad_y) * dst_w + dx + pad_x) as usize;
                for channel in 0..3 {
                    let byte = rgba[(sy * src_w as usize + sx) * 4 + channel];
                    out[channel * plane + at] = byte as f32 / 255.0;
                }
            }
        }
        out
    }

    #[test]
    fn the_tables_give_the_per_pixel_formulas_floats_bit_for_bit() {
        let sizes = [
            ((37, 23), (16, 12)),
            ((8, 40), (32, 32)),
            ((5, 3), (64, 48)),
        ];
        for (src, dst) in sizes {
            let rgba: Vec<u8> = (0..src.0 * src.1 * 4)
                .map(|i| (i * 37 % 256) as u8)
                .collect();
            // A buffer left over from a larger, other frame: nothing of it may survive.
            let mut output = vec![9.0; 7 * (dst.0 * dst.1) as usize];
            for mode in [ResizeMode::Stretch, ResizeMode::Letterbox] {
                let transform = match mode {
                    ResizeMode::Letterbox => rgba_to_nchw_f32_letterboxed,
                    ResizeMode::Stretch => rgba_to_nchw_f32_stretched,
                };
                transform(&rgba, src.0, src.1, dst.0, dst.1, &mut output).unwrap();
                let expected = reference(&rgba, src, dst, mode);
                assert_eq!(output.len(), expected.len(), "{src:?} -> {dst:?}");
                let same = output
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits());
                assert!(same, "{src:?} -> {dst:?}, {mode:?}");
            }
        }
    }

    #[test]
    fn every_byte_normalises_to_its_quotient() {
        for value in 0..=255u8 {
            assert_eq!(
                UNIT_FLOATS[value as usize].to_bits(),
                (value as f32 / 255.0).to_bits()
            );
        }
    }

    #[test]
    fn zero_dimensions_are_errors() {
        let mut output = Vec::new();
        assert_eq!(
            rgba_to_nchw_f32_letterboxed(&[], 0, 1, 2, 2, &mut output),
            Err(PreprocessError::ZeroSizeFrame {
                width: 0,
                height: 1
            })
        );
        assert_eq!(
            rgba_to_nchw_f32_stretched(&[0; 4], 1, 1, 0, 2, &mut output),
            Err(PreprocessError::ZeroSizeInput {
                width: 0,
                height: 2
            })
        );
    }

    #[test]
    fn a_packed_rgba_buffer_of_another_length_is_refused() {
        let mut output = Vec::new();
        for len in [7, 9] {
            assert_eq!(
                rgba_to_nchw_f32_stretched(&vec![0; len], 2, 1, 2, 2, &mut output),
                Err(PreprocessError::LengthMismatch {
                    got: len,
                    expected: 8
                })
            );
        }
        assert_eq!(
            rgba_to_nchw_f32_stretched(&[], u32::MAX, u32::MAX, 2, 2, &mut output),
            Err(PreprocessError::SizeOverflow)
        );
    }

    fn frame(
        data: &[u8],
        format: PixelFormat,
        size: (u32, u32),
        stride: usize,
        offset: usize,
    ) -> Frame<'_> {
        Frame {
            data,
            format,
            width: size.0,
            height: size.1,
            row_stride_bytes: stride,
            plane_offset_bytes: offset,
        }
    }

    #[test]
    fn the_refusals_of_the_cpp_detector_are_typed() {
        let mut out = Vec::new();
        let data = [7_u8; 64];
        let run = |frame: Frame, out: &mut Vec<f32>| {
            preprocess_frame_into(&frame, 4, 4, ResizeMode::Stretch, out)
        };
        assert_eq!(
            run(frame(&data, PixelFormat::Rgb, (0, 2), 0, 0), &mut out),
            Err(PreprocessError::ZeroSizeFrame {
                width: 0,
                height: 2
            })
        );
        assert_eq!(
            run(frame(&data, PixelFormat::Rgb, (4, 2), 11, 0), &mut out),
            Err(PreprocessError::StrideShorterThanRow {
                stride: 11,
                row_bytes: 12
            })
        );
        // Two rows of 16 bytes, 20 apart, from byte 29 on: the last ends at 65, one past the buffer.
        assert_eq!(
            run(frame(&data, PixelFormat::Rgba, (4, 2), 20, 29), &mut out),
            Err(PreprocessError::BufferTooShort {
                needed: 65,
                got: 64
            })
        );
        assert!(run(frame(&data, PixelFormat::Rgba, (4, 2), 20, 28), &mut out).is_ok());
        assert_eq!(
            run(
                frame(&data, PixelFormat::Bgr, (1, 3), usize::MAX, 0),
                &mut out
            ),
            Err(PreprocessError::SizeOverflow)
        );
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            run(
                frame(&data, PixelFormat::Rgb, (u32::MAX, 1), 0, 0),
                &mut out
            ),
            Err(PreprocessError::BufferTooShort {
                needed: u32::MAX as usize * 3,
                got: 64
            })
        );
        let huge = preprocess_frame_into(
            &frame(&data, PixelFormat::Rgb, (1, 1), 0, 0),
            u32::MAX,
            u32::MAX,
            ResizeMode::Stretch,
            &mut out,
        );
        assert_eq!(huge, Err(PreprocessError::SizeOverflow));
    }

    #[test]
    fn packed_stride_zero_reads_rows_one_row_apart_and_bgr_swaps_red_and_blue() {
        // 2x2 BGR, packed: pixel (x, y) holds blue 10y+x, green 100, red 200+x.
        let data: Vec<u8> = (0..2u8)
            .flat_map(|y| (0..2u8).flat_map(move |x| [10 * y + x, 100, 200 + x]))
            .collect();
        let mut out = Vec::new();
        let mapping = preprocess_frame_into(
            &frame(&data, PixelFormat::Bgr, (2, 2), 0, 0),
            2,
            2,
            ResizeMode::Stretch,
            &mut out,
        )
        .unwrap();
        let bytes: Vec<u8> = out.iter().map(|v| (v * 255.0).round() as u8).collect();
        assert_eq!(
            bytes,
            [200, 201, 200, 201, 100, 100, 100, 100, 0, 1, 10, 11]
        );
        assert_eq!(
            (mapping.scale_x, mapping.scale_y, mapping.pad_x),
            (1.0, 1.0, 0.0)
        );
    }

    #[test]
    fn errors_say_what_was_refused() {
        let text = PreprocessError::BufferTooShort {
            needed: 65,
            got: 64,
        }
        .to_string();
        assert!(text.contains("got 64") && text.contains("65"), "{text}");
        let text = PreprocessError::LengthMismatch {
            got: 7,
            expected: 8,
        }
        .to_string();
        assert!(text.contains("RGBA buffer length mismatch"), "{text}");
    }
}
