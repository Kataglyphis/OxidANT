//! Webcam/video capture built on GStreamer.

pub mod capture;
pub mod devices;
mod runtime;

pub use capture::{CameraSource, CaptureConfig, CaptureSession, FrameSlot, VideoFrame};
pub use devices::{list_cameras, CameraInfo};
pub use runtime::{bundled_plugin_dir, ensure_gst_initialized};
