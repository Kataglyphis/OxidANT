//! ObservANT's parts, which `main.rs` wires into one service; benches and integration tests reach them here.

pub mod camera;
pub mod capture;
pub mod clip;
pub mod coco;
pub mod config;
pub mod frame;
pub mod install;
pub mod motion;
pub mod notify;
pub mod output;
pub mod precompress;
pub mod presence;
pub mod static_files;
pub mod stats;
pub mod status;
pub mod web;
