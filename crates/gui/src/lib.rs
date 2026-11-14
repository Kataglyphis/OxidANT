//! Camera GUIs: wgpu on desktop targets and GTK4 on Linux.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

pub mod gui_wgpu;

#[cfg(target_os = "linux")]
pub mod gui;
