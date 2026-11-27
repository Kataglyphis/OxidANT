// The CPU loaders are kataglyphis_assets'; re-exported so `asset::gltf_loader` and friends keep their paths.
/// The OBJ reader moved to kataglyphis_assets; its converter keeps the path `obj2gltf` and the tests import.
#[doc(no_inline)]
pub use kataglyphis_assets::obj as obj_to_gltf;
pub(crate) use kataglyphis_assets::{basis, bc_decode};
// Linked, not copied, in the docs: a copied module carries tracing's dead dispatcher link (docs/ci.md).
#[doc(no_inline)]
pub use kataglyphis_assets::{gltf_loader, ktx2_loader};
pub mod hdr;
pub mod skybox;
