//! WebGPU renderer with glTF loading; the plan it implements is `../docs/webgpu-gltf-rust-plan.md`.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

pub mod app;
pub mod asset;
pub mod context;
pub mod render;
pub mod scene;
mod surface_recovery;
pub mod ui;
#[cfg(target_arch = "wasm32")]
pub mod wasm_demo;

pub use app::{AppSignal, HostCaps, Notice, OffscreenFrame, StudioApp};
pub use asset::gltf_loader::load_gltf;
pub use asset::hdr::{decode_hdr, HdrError};
pub use context::GpuContext;
pub use render::forward::ForwardRenderer;
pub use render::frame_clock::FrameClock;
pub use render::ibl::{BrdfLut, EquirectImage, IblEnvironment};
pub use render::overlay::Overlay;
pub use render::texture_stream::{TextureResidency, TextureStreaming};
pub use render::tonemap::TonemapPass;
pub use scene::camera::OrbitCamera;
pub use scene::controller::OrbitController;
pub use scene::lod::{
    build_lod_chain, build_lod_chain_with, select_lod, select_lod_by_distance, simplify_primitive,
    Lod, Simplifier,
};
pub use scene::qem::simplify_primitive_qem;
pub use scene::viewport::SceneViewport;
pub use scene::{CpuMaterial, CpuPrimitive, CpuScene, CpuTexture};
