//! `kataglyphis_vision_core` for the browser: the preprocess, the decode and the COCO names as wasm-bindgen exports.
//!
//! JS sees `Preprocessor`, `Mapping`, `decode` and `classNames`. A refusal throws an `Error` whose `name` is its kind
//! (`BufferTooShort`, `UnknownPixelFormat`, ...) and whose `message` is the Rust error's text. The module keeps no
//! state but a preprocessor's tensor buffer, and needs neither threads nor cross-origin isolation.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]
// Not forbid: wasm-bindgen's generated export glue is unsafe; this crate writes no unsafe code of its own.
#![deny(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

use std::fmt;

use kataglyphis_vision_core::{
    parse_yolo_like_detections, preprocess_frame_into, Frame, ImageMapping, PixelFormat, ResizeMode,
};
use wasm_bindgen::prelude::*;

/// The hub's shared COCO list as `kataglyphis_core` embeds it: line N names class N-1.
const COCO: &str = include_str!("../../core/src/coco-classes.txt");

/// A refused call: `kind` is the error variant's name, `message` its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub kind: String,
    pub message: String,
}

impl Refusal {
    /// Named from the derived `Debug`, so a new variant of a `#[non_exhaustive]` error is named too.
    fn of<E: fmt::Debug + fmt::Display>(error: &E) -> Self {
        let debug = format!("{error:?}");
        let kind = debug
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        Self {
            kind: kind.to_owned(),
            message: error.to_string(),
        }
    }
}

impl From<Refusal> for JsValue {
    fn from(refusal: Refusal) -> Self {
        let error = js_sys::Error::new(&refusal.message);
        error.set_name(&refusal.kind);
        error.into()
    }
}

/// A name no `vision_core` type has.
#[derive(Debug)]
enum ArgumentError {
    UnknownPixelFormat(String),
    UnknownResizeMode(String),
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPixelFormat(name) => {
                write!(f, "no pixel format {name:?}: RGBA, RGB or BGR")
            }
            Self::UnknownResizeMode(name) => {
                write!(f, "no resize mode {name:?}: stretch or letterbox")
            }
        }
    }
}

fn pixel_format(name: &str) -> Result<PixelFormat, Refusal> {
    match name {
        "RGBA" => Ok(PixelFormat::Rgba),
        "RGB" => Ok(PixelFormat::Rgb),
        "BGR" => Ok(PixelFormat::Bgr),
        other => Err(Refusal::of(&ArgumentError::UnknownPixelFormat(
            other.to_owned(),
        ))),
    }
}

fn resize_mode(name: &str) -> Result<ResizeMode, Refusal> {
    match name {
        "stretch" => Ok(ResizeMode::Stretch),
        "letterbox" => Ok(ResizeMode::Letterbox),
        other => Err(Refusal::of(&ArgumentError::UnknownResizeMode(
            other.to_owned(),
        ))),
    }
}

/// Where the frame went in the model's input; `decode` maps boxes back through it.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    #[wasm_bindgen(readonly, js_name = srcWidth)]
    pub src_width: u32,
    #[wasm_bindgen(readonly, js_name = srcHeight)]
    pub src_height: u32,
    #[wasm_bindgen(readonly, js_name = scaleX)]
    pub scale_x: f32,
    #[wasm_bindgen(readonly, js_name = scaleY)]
    pub scale_y: f32,
    #[wasm_bindgen(readonly, js_name = padX)]
    pub pad_x: f32,
    #[wasm_bindgen(readonly, js_name = padY)]
    pub pad_y: f32,
}

#[wasm_bindgen]
impl Mapping {
    /// A mapping by hand, as ModelANT's decode cases give one.
    #[wasm_bindgen(constructor)]
    pub fn new(
        src_width: u32,
        src_height: u32,
        scale_x: f32,
        scale_y: f32,
        pad_x: f32,
        pad_y: f32,
    ) -> Mapping {
        Self {
            src_width,
            src_height,
            scale_x,
            scale_y,
            pad_x,
            pad_y,
        }
    }
}

impl From<ImageMapping> for Mapping {
    fn from(m: ImageMapping) -> Self {
        Self::new(m.src_w, m.src_h, m.scale_x, m.scale_y, m.pad_x, m.pad_y)
    }
}

impl From<Mapping> for ImageMapping {
    fn from(m: Mapping) -> Self {
        Self {
            src_w: m.src_width,
            src_h: m.src_height,
            scale_x: m.scale_x,
            scale_y: m.scale_y,
            pad_x: m.pad_x,
            pad_y: m.pad_y,
        }
    }
}

/// One model input's preprocessing; it keeps its tensor buffer from frame to frame.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Preprocessor {
    input_width: u32,
    input_height: u32,
    mode: ResizeMode,
    tensor: Vec<f32>,
}

#[wasm_bindgen]
impl Preprocessor {
    /// For an `input_width` x `input_height` model input, `mode` `stretch` or `letterbox`.
    #[wasm_bindgen(constructor)]
    pub fn new(input_width: u32, input_height: u32, mode: &str) -> Result<Preprocessor, Refusal> {
        Ok(Self {
            input_width,
            input_height,
            mode: resize_mode(mode)?,
            tensor: Vec::new(),
        })
    }

    /// An `RGBA`, `RGB` or `BGR` frame to the NCHW tensor: rows `row_stride_bytes` apart (0 for packed rows)
    /// from byte `plane_offset_bytes` on. A refused frame leaves the last tensor as it was.
    pub fn run(
        &mut self,
        frame: &[u8],
        format: &str,
        width: u32,
        height: u32,
        row_stride_bytes: usize,
        plane_offset_bytes: usize,
    ) -> Result<Mapping, Refusal> {
        let frame = Frame {
            data: frame,
            format: pixel_format(format)?,
            width,
            height,
            row_stride_bytes,
            plane_offset_bytes,
        };
        let (width, height) = (self.input_width, self.input_height);
        preprocess_frame_into(&frame, width, height, self.mode, &mut self.tensor)
            .map(Mapping::from)
            .map_err(|error| Refusal::of(&error))
    }

    /// A copy of the last tensor, NCHW float32 with values `pixel / 255`; empty before the first `run`.
    #[wasm_bindgen(getter)]
    pub fn tensor(&self) -> Vec<f32> {
        self.tensor.clone()
    }
}

/// The model's `[n, s]` or `[1, n, s]` rows to detections in frame pixels, in row order: six numbers each,
/// x1, y1, x2, y2, score and class. A row under the threshold, or with a class at or past `class_count`, is none.
#[wasm_bindgen]
pub fn decode(
    shape: &[u32],
    data: &[f32],
    score_threshold: f32,
    mapping: &Mapping,
    swap_xy: bool,
    class_count: u32,
) -> Result<Vec<f64>, Refusal> {
    let shape: Vec<usize> = shape.iter().map(|&n| n as usize).collect();
    let mapping = ImageMapping::from(*mapping);
    let found = parse_yolo_like_detections(
        &shape,
        data,
        score_threshold,
        mapping,
        swap_xy,
        class_count as usize,
    )
    .map_err(|error| Refusal::of(&error))?;
    Ok(found
        .iter()
        .flat_map(|d| {
            let box_and_score = [d.x1, d.y1, d.x2, d.y2, d.score].map(f64::from);
            box_and_score.into_iter().chain([d.class_id as f64])
        })
        .collect())
}

/// The 80 COCO class names by class id, so a page labels boxes without the Rust core.
#[wasm_bindgen(js_name = classNames)]
pub fn class_names() -> Vec<String> {
    COCO.lines().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|v| v.to_bits()).collect()
    }

    #[test]
    fn the_tensor_and_the_mapping_are_vision_cores() {
        let rgba: Vec<u8> = (0..37 * 23 * 4).map(|i| (i * 37 % 256) as u8).collect();
        for (name, mode) in [
            ("stretch", ResizeMode::Stretch),
            ("letterbox", ResizeMode::Letterbox),
        ] {
            let mut pre = Preprocessor::new(16, 12, name).unwrap();
            assert!(pre.tensor().is_empty());
            let mapping = pre.run(&rgba, "RGBA", 37, 23, 0, 0).unwrap();
            let frame = Frame {
                data: &rgba,
                format: PixelFormat::Rgba,
                width: 37,
                height: 23,
                row_stride_bytes: 0,
                plane_offset_bytes: 0,
            };
            let mut expected = Vec::new();
            let direct = preprocess_frame_into(&frame, 16, 12, mode, &mut expected).unwrap();
            assert_eq!(mapping, Mapping::from(direct), "{name}");
            assert_eq!(bits(&pre.tensor()), bits(&expected), "{name}");
        }
    }

    #[test]
    fn each_format_name_reads_its_byte_order_and_the_stride_and_offset_count() {
        let unit = |v: u8| v as f32 / 255.0;
        let mut pre = Preprocessor::new(1, 1, "stretch").unwrap();
        pre.run(&[10, 20, 30], "RGB", 1, 1, 0, 0).unwrap();
        assert_eq!(pre.tensor(), [unit(10), unit(20), unit(30)]);
        pre.run(&[10, 20, 30], "BGR", 1, 1, 0, 0).unwrap();
        assert_eq!(pre.tensor(), [unit(30), unit(20), unit(10)]);
        pre.run(&[10, 20, 30, 40], "RGBA", 1, 1, 0, 0).unwrap();
        assert_eq!(pre.tensor(), [unit(10), unit(20), unit(30)]);
        // Two rows of one RGB pixel, 5 bytes apart, from byte 2 on: the second row's pixel is 40, 50, 60.
        let mut tall = Preprocessor::new(1, 2, "stretch").unwrap();
        tall.run(&[0, 0, 7, 8, 9, 0, 0, 40, 50, 60], "RGB", 1, 2, 5, 2)
            .unwrap();
        assert_eq!(
            tall.tensor(),
            [unit(7), unit(40), unit(8), unit(50), unit(9), unit(60)]
        );
    }

    #[test]
    fn refusals_carry_the_variants_name_and_its_text() {
        let mut pre = Preprocessor::new(4, 4, "stretch").unwrap();
        pre.run(&[7; 64], "RGBA", 4, 2, 20, 28).unwrap();
        let kept = pre.tensor();
        let refusal = pre.run(&[7; 64], "RGBA", 4, 2, 20, 29).unwrap_err();
        assert_eq!(refusal.kind, "BufferTooShort");
        assert_eq!(
            refusal.message,
            "Frame buffer too short: got 64 bytes, the last row ends at 65"
        );
        assert_eq!(bits(&pre.tensor()), bits(&kept));
        let format = pre.run(&[7; 64], "YUV", 4, 2, 0, 0).unwrap_err();
        assert_eq!(format.kind, "UnknownPixelFormat");
        assert!(format.message.contains("\"YUV\""), "{}", format.message);
        assert_eq!(
            pre.run(&[], "RGB", 0, 2, 0, 0).unwrap_err().kind,
            "ZeroSizeFrame"
        );
        assert_eq!(
            Preprocessor::new(4, 4, "fit").unwrap_err().kind,
            "UnknownResizeMode"
        );
        let unit = Refusal::of(&kataglyphis_vision_core::PreprocessError::SizeOverflow);
        assert_eq!(unit.kind, "SizeOverflow");
    }

    #[test]
    fn decode_gives_six_numbers_per_detection_in_row_order() {
        // A 200x100 frame letterboxed into 100x100: scale 0.5, 25 rows of padding above.
        let mapping = Mapping::new(200, 100, 0.5, 0.5, 0.0, 25.0);
        let rows = [
            10.0, 35.0, 60.0, 60.0, 0.9, 15.0, 1.0, 1.0, 2.0, 2.0, 0.1, 3.0, -5.0, 0.0, 120.0,
            99.0, 0.8, 0.0,
        ];
        let found = decode(&[1, 3, 6], &rows, 0.5, &mapping, false, 80).unwrap();
        let (high, low) = (f64::from(0.9_f32), f64::from(0.8_f32));
        assert_eq!(
            found,
            [20.0, 20.0, 120.0, 70.0, high, 15.0, 0.0, 0.0, 200.0, 100.0, low, 0.0]
        );
    }

    #[test]
    fn decode_takes_the_class_count_and_the_swap_per_call() {
        let mapping = Mapping::new(100, 100, 1.0, 1.0, 0.0, 0.0);
        let rows = [10.0, 20.0, 30.0, 40.0, 0.9, 1.0];
        assert!(decode(&[1, 6], &rows, 0.5, &mapping, false, 1)
            .unwrap()
            .is_empty());
        let swapped = decode(&[1, 6], &rows, 0.5, &mapping, true, 2).unwrap();
        assert_eq!(swapped, [20.0, 10.0, 40.0, 30.0, f64::from(0.9_f32), 1.0]);
    }

    #[test]
    fn decode_refusals_are_named() {
        let mapping = Mapping::new(100, 100, 1.0, 1.0, 0.0, 0.0);
        let kind = |shape: &[u32], data: &[f32]| {
            decode(shape, data, 0.5, &mapping, false, 80)
                .unwrap_err()
                .kind
        };
        assert_eq!(kind(&[1, 4], &[0.0; 4]), "TooFewColumns");
        assert_eq!(kind(&[2, 1, 6], &[0.0; 12]), "BatchNotOne");
        assert_eq!(kind(&[6], &[0.0; 6]), "UnsupportedRank");
        assert_eq!(kind(&[2, 6], &[0.0; 6]), "ShorterThanShape");
    }

    #[test]
    fn the_class_names_are_the_native_detectors() {
        let names = class_names();
        assert_eq!(names, kataglyphis_core::coco::NAMES);
        assert_eq!(names[15], "cat");
    }
}
