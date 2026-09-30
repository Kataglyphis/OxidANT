//! Webcam/video capture built on GStreamer.
//! Empty without the `gstreamer` feature, so non-media builds need no GStreamer dev files.

#[cfg(feature = "gstreamer")]
pub mod capture;
#[cfg(feature = "gstreamer")]
pub mod devices;
#[cfg(feature = "gstreamer")]
mod runtime;

#[cfg(feature = "gstreamer")]
pub use capture::{CameraSource, CaptureConfig, CaptureSession, FrameSlot, VideoFrame};
#[cfg(feature = "gstreamer")]
pub use devices::{list_cameras, CameraInfo};
#[cfg(feature = "gstreamer")]
pub use runtime::{bundled_plugin_dir, ensure_gst_initialized};
