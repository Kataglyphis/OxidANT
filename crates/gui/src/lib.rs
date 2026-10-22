//! Camera GUIs: wgpu on desktop targets and GTK4 on Linux.

pub mod gui_wgpu;

#[cfg(target_os = "linux")]
pub mod gui;
