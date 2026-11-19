//! The renderers' shared CPU half, without wgpu, winit or egui: `kataglyphis_cxx_bridge` links it for the C++ engine
//! and `kataglyphis_webgpu_renderer` re-exports it under its old paths. A module moves here in the change that bridges it.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

pub mod clouds;
pub mod frustum;
pub mod lens;
pub mod model_transform;
pub mod shadow_settings;
pub mod viewport;
