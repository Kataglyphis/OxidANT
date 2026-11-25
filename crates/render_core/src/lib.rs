//! The renderers' shared CPU half, without wgpu, winit or egui: `kataglyphis_cxx_bridge` links it for the C++ engine
//! and `kataglyphis_webgpu_renderer` re-exports it under its old paths. A module moves here in the change that bridges it.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]
// The C++ engine links this crate through the bridge, where its sanitizers and FuzzTest cannot reach.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod clouds;
pub mod fly_speed;
pub mod framing;
pub mod frustum;
pub mod lens;
pub mod model_transform;
pub mod orbit_camera;
pub mod shader_limits;
pub mod shadow_settings;
pub mod sun;
pub mod viewport;
