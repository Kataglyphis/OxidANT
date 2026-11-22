//! CPU-side asset loading shared by the WebGPU renderer and BeschleunigerBallett's C++ engine (through `kataglyphis_cxx_bridge`).

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

mod accessor_check;
mod attributes;
#[doc(hidden)]
pub mod basis;
#[doc(hidden)]
pub mod bc_decode;
mod files;
pub mod flat;
mod gltf_images;
pub mod gltf_loader;
pub mod ktx2_loader;
mod ktx2_zstd;
pub mod scene;

pub use gltf_loader::{
    load_gltf, load_gltf_slice, load_gltf_slice_with, load_gltf_with, LoadOptions,
};
pub use scene::{CpuMaterial, CpuPrimitive, CpuScene, CpuTexture};
