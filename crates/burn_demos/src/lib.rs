//! Burn demos of common PyTorch tasks, and the family's detector run through ONNX Runtime.

use burn::tensor::Device;

pub mod lcg;
pub mod losses;
#[cfg(any(target_os = "linux", windows))]
pub mod onnx_detector;
pub mod plot;
pub mod simple;
pub mod two_moons;
pub mod yolo;

/// The demos' CPU device. Since burn 0.22 the backend is a runtime `Device`, and Flex replaced the deprecated NdArray.
pub fn inference_device() -> Device {
    Device::flex()
}

/// [`inference_device`] with autodiff enabled, for the training demos.
pub fn training_device() -> Device {
    inference_device().autodiff()
}
