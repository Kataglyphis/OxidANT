//! Camera enumeration via the GStreamer device monitor.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::ensure_gst_initialized;

#[derive(Debug, Clone)]
pub struct CameraInfo {
    /// Enumeration order: the index `CameraSource::Device` takes.
    pub index: u32,
    pub display_name: String,
    /// Provider element class, e.g. `Source/Video`.
    pub device_class: String,
}

/// The video sources the device monitor finds, in the order every index here refers to.
fn video_sources() -> Result<Vec<gst::Device>> {
    ensure_gst_initialized()?;

    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Video/Source"), None);
    monitor
        .start()
        .context("failed to start GStreamer device monitor")?;
    let devices = monitor.devices();
    monitor.stop();
    Ok(devices.into_iter().collect())
}

/// The source element of the `index`-th listed camera, built by the provider that listed it.
pub(crate) fn camera_element(index: u32) -> Result<gst::Element> {
    let devices = video_sources()?;
    let device = devices
        .get(index as usize)
        .with_context(|| format!("no camera with index {index}: {} listed", devices.len()))?;
    device
        .create_element(None)
        .with_context(|| format!("failed to open camera `{}`", device.display_name()))
}

/// Lists video capture devices. Empty inside containers (no camera devices).
pub fn list_cameras() -> Result<Vec<CameraInfo>> {
    let devices = video_sources()?;

    Ok(devices
        .iter()
        .enumerate()
        .map(|(i, device)| CameraInfo {
            index: i as u32,
            display_name: device.display_name().to_string(),
            device_class: device.device_class().to_string(),
        })
        .collect())
}
