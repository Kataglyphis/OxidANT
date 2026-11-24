//! The detector's shared CPU half without ORT, tract or environment reads: frames to the NCHW tensor, rows to detections.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]
// A bridge may link this crate for the C++ detector, where its sanitizers and FuzzTest cannot reach.
#![forbid(unsafe_code)]
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

pub mod detection;
pub mod postprocess;
pub mod preprocess;

pub use detection::Detection;
pub use postprocess::{parse_yolo_like_detections, DecodeError};
pub use preprocess::{
    preprocess_frame_into, Frame, ImageMapping, PixelFormat, PreprocessError, ResizeMode,
};
