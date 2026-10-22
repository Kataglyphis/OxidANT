//! Main entry point for Kataglyphis.

#![doc = include_str!("../README.md")]
#![doc(html_logo_url = "https://rust.jonasheinle.de/logo.png")]

pub mod api;
pub use kataglyphis_core::config;
pub use kataglyphis_core::detection::Detection;
pub use kataglyphis_core::logging;
pub use kataglyphis_telemetry::resource_monitor;
pub mod platform;
pub mod texture_abi;
pub mod utils;

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub(crate) use kataglyphis_telemetry::gpu_wmi;

#[cfg(any(target_os = "linux", windows))]
pub use kataglyphis_inference::ort_ext;
#[cfg(any(target_os = "linux", windows))]
pub use kataglyphis_inference::ort_runtime;

mod frb_generated;
#[cfg(not(target_arch = "wasm32"))]
pub use kataglyphis_inference::person_detection;

#[cfg(desktop)]
mod webcam_engine;

/// The real model's tests take turns: under QEMU on four cores three at once outran their 60 s waits.
#[cfg(test)]
pub(crate) fn model_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// C FFI demo stub — returns a fixed integer to verify `extern "C"` linkage works.
#[unsafe(no_mangle)]
pub extern "C" fn rusty_extern_c_integer() -> i32 {
    322
}
