//! Bridge to the ESP32-S3 companion: USB discovery plus a framed, threaded serial link.
//! Desktop only; the parent crate gates it out of Android and wasm builds.

pub mod discovery;
pub mod link;

pub use discovery::{list_devices, DeviceInfo, ESPRESSIF_VID};
pub use kataglyphis_esp32_proto;
pub use link::{Link, LinkEvent};
