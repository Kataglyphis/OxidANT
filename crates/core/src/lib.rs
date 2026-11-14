//! Core utilities: configuration, detection types, class names and logging.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

pub mod coco;
pub mod config;
pub mod detection;
pub mod logging;
pub mod provenance;

pub use config::*;
pub use detection::*;
pub use logging::*;
