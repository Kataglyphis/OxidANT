//! AccelerANTgine's `Test/fuzz/preprocess_fuzz.cpp` as a proptest: no frame geometry reads outside its buffer.

use kataglyphis_vision_core::preprocess::LETTERBOX_FILL_VALUE;
use kataglyphis_vision_core::{preprocess_frame_into, Frame, PixelFormat, ResizeMode};
use proptest::prelude::*;

/// The target size stays small: the answer is 3*w*h floats, and an arbitrary one would only exhaust memory.
const MAX_TARGET: u32 = 48;

/// The slow sampler the fast one must equal; `None` when a sampled pixel lies past the buffer.
fn reference(frame: &Frame, width: u32, height: u32, mode: ResizeMode) -> Option<Vec<f32>> {
    let channels = frame.format.channels();
    let stride = match frame.row_stride_bytes {
        0 => frame.width as usize * channels,
        stride => stride,
    };
    let (mut content_w, mut content_h) = (width, height);
    if mode == ResizeMode::Letterbox {
        let ratio = f64::min(
            width as f64 / frame.width as f64,
            height as f64 / frame.height as f64,
        );
        content_w = ((frame.width as f64 * ratio).round() as u32).clamp(1, width);
        content_h = ((frame.height as f64 * ratio).round() as u32).clamp(1, height);
    }
    let plane = width as usize * height as usize;
    let fill = if mode == ResizeMode::Letterbox {
        LETTERBOX_FILL_VALUE
    } else {
        0.0
    };
    let mut expected = vec![fill; 3 * plane];
    let (pad_x, pad_y) = ((width - content_w) / 2, (height - content_h) / 2);
    let order: [usize; 3] = if frame.format == PixelFormat::Bgr {
        [2, 1, 0]
    } else {
        [0, 1, 2]
    };
    for y in 0..content_h {
        let source_y = y as u64 * frame.height as u64 / content_h as u64;
        for x in 0..content_w {
            let source_x = x as u64 * frame.width as u64 / content_w as u64;
            let at = (source_y as u128 * stride as u128
                + source_x as u128 * channels as u128
                + frame.plane_offset_bytes as u128) as usize;
            if at >= frame.data.len() || frame.data.len() - at < 3 {
                return None;
            }
            let out = (y + pad_y) as usize * width as usize + (x + pad_x) as usize;
            for (channel, &byte) in order.iter().enumerate() {
                expected[channel * plane + out] = frame.data[at + byte] as f32 / 255.0;
            }
        }
    }
    Some(expected)
}

/// A small value most of the time, so geometries fit their buffer, and any value now and then.
fn mostly_small_u32(small: u32) -> impl Strategy<Value = u32> {
    prop_oneof![4 => 0..=small, 1 => any::<u32>()]
}

fn mostly_small_usize(small: usize) -> impl Strategy<Value = usize> {
    prop_oneof![4 => 0..=small, 1 => any::<usize>()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn never_reads_outside_the_buffer(
        format in prop_oneof![Just(PixelFormat::Rgb), Just(PixelFormat::Bgr), Just(PixelFormat::Rgba)],
        frame_width in mostly_small_u32(24),
        frame_height in mostly_small_u32(24),
        stride in mostly_small_usize(128),
        offset in mostly_small_usize(64),
        bytes in prop::collection::vec(any::<u8>(), 0..4096),
        cut in 0usize..8,
        width in 0..=MAX_TARGET,
        height in 0..=MAX_TARGET,
        letterbox in any::<bool>(),
    ) {
        // The C++ buffer may claim fewer bytes than it holds; a shorter slice is that claim here.
        let frame = Frame {
            data: &bytes[..bytes.len().saturating_sub(cut)],
            format,
            width: frame_width,
            height: frame_height,
            row_stride_bytes: stride,
            plane_offset_bytes: offset,
        };
        let mode = if letterbox { ResizeMode::Letterbox } else { ResizeMode::Stretch };
        let mut output = vec![9.0; 5];
        let Ok(mapping) = preprocess_frame_into(&frame, width, height, mode, &mut output) else {
            return Ok(());
        };
        prop_assert_eq!(output.len(), 3 * width as usize * height as usize);
        prop_assert!(output.iter().all(|value| (0.0..=1.0).contains(value)));
        prop_assert_eq!((mapping.src_w, mapping.src_h), (frame_width, frame_height));
        let expected = reference(&frame, width, height, mode);
        prop_assert!(expected.is_some(), "an accepted geometry samples past the buffer");
        let expected = expected.unwrap_or_default();
        let same = output.iter().zip(&expected).all(|(a, b)| a.to_bits() == b.to_bits());
        prop_assert!(same, "the fast sampler parts from the slow one");
    }
}
