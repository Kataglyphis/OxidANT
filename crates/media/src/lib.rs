//! Webcam/video capture built on GStreamer.

pub mod capture;
pub mod devices;
mod runtime;
pub mod sink;

pub use capture::{
    CameraSource, CaptureConfig, CaptureSession, FrameObserver, FrameSlot, VideoFrame,
};
pub use devices::{list_cameras, CameraInfo};
pub use runtime::{bundled_plugin_dir, ensure_gst_initialized};
pub use sink::{frame_sink, FRAME_SINK_FACTORY};
