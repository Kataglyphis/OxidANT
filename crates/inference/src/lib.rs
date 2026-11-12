//! Machine learning and inference models.

#[cfg(any(target_os = "linux", windows))]
pub mod ort_ext;
#[cfg(target_os = "linux")]
pub mod ort_plugin;
#[cfg(any(target_os = "linux", windows))]
pub mod ort_runtime;

pub mod person_detection;
pub mod provider;
