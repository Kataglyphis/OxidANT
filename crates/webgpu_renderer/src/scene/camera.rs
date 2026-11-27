//! The orbit camera, which moved to kataglyphis_render_core so the C++ engine's framing shares it.

// Linked, not copied, in the docs: a copied type carries tracing's dead dispatcher link (docs/ci.md).
#[doc(no_inline)]
pub use kataglyphis_render_core::orbit_camera::OrbitCamera;
