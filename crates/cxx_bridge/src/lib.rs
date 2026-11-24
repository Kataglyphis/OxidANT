//! The cxx bridge BeschleunigerBallett's C++ engine calls; Corrosion's `corrosion_add_cxxbridge` generates its C++ half.
//!
//! One bridge file per area, each in the C++ namespace `kataglyphis::oxidant::<file stem>`: [`gltf`], [`settings`],
//! [`parity`] and [`diagnostics`]. Every bridged function runs inside `guarded`, since the engine builds without
//! exceptions: the loader's calls return their errors as values, the others, which have none, their type's zero value
//! on a panic (`nothing`); a path arrives as UTF-8 bytes. `contract.rs` holds every bridge file to that.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

#[cfg(test)]
mod contract;
pub mod diagnostics;
pub mod gltf;
pub mod parity;
pub mod settings;

// One guard for both C++ bridges: AccelerANTgine's links the same support crate.
pub(crate) use kataglyphis_bridge_support::{guarded, nothing};
