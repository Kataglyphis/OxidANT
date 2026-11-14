//! Telemetry and resource monitoring.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

pub mod resource_monitor;

#[cfg(target_os = "windows")]
pub mod gpu_wmi;

pub use resource_monitor::*;
