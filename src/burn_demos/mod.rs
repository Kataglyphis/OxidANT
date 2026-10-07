use burn::tensor::Device;

pub mod lcg;
pub mod losses;
pub mod onnx_yolov10;
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
