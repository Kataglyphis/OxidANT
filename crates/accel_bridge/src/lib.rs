//! The cxx bridge AccelerANTgine's C++ engine calls; Corrosion's `corrosion_add_cxxbridge` generates its C++ half.
//!
//! One bridge file per area, each in the C++ namespace `kataglyphis::oxidant::accel::<file stem>`: [`text`], the chain
//! GenAI's generation pulled piece by piece, and [`diagnostics`], OxidANT's version and each call's log records. Every
//! bridged function is one `guarded` call, errors are values and text from C++ arrives as UTF-8 bytes, since the engine
//! builds without exceptions; `contract.rs` checks each file. The crate holds no unsafe code of its own: the glue cxx
//! generates for a bridge is its one allowance. The crates it links may hold unsafe FFI, `kataglyphis_text`'s GenAI
//! binding among them (owner D3).

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]
// Not forbid: cxx's generated glue is unsafe, so each bridge module allows it and the contract test holds it there.
#![deny(unsafe_code)]
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

#[cfg(test)]
mod contract;
pub mod diagnostics;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod text;

use kataglyphis_bridge_support::{guarded, nothing};
