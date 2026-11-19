//! CPU-side asset loading shared by the WebGPU renderer and BeschleunigerBallett's C++ engine (through `kataglyphis_cxx_bridge`).

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

mod accessor_check;
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

pub use gltf_loader::{load_gltf, load_gltf_slice};
pub use scene::{CpuMaterial, CpuPrimitive, CpuScene, CpuTexture};
