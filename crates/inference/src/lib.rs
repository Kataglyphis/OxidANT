//! Machine learning and inference models.

#[cfg(any(target_os = "linux", windows))]
pub mod ort_ext;
#[cfg(any(target_os = "linux", windows))]
pub mod ort_runtime;

pub mod person_detection;
