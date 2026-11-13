//! Machine learning and inference models.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

#[cfg(any(target_os = "linux", windows))]
pub mod ort_ext;
#[cfg(target_os = "linux")]
pub mod ort_plugin;
#[cfg(any(target_os = "linux", windows))]
pub mod ort_runtime;

pub mod person_detection;
pub mod provider;
