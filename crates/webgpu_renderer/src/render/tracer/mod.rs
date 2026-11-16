//! The compute-shader ray tracer's core: BVHs built on the CPU, traversed by `shaders/tracer/bvh_traverse.slang`.
//! Layout, limits, build times and the kernel API: `../../../docs/tracer.md`.

pub mod build;
pub mod bvh;
pub mod debug;
pub mod deform;
pub mod geometry;
pub mod gpu;
pub mod job;
pub mod layers;
pub mod limits;
pub mod material;
pub mod modes;
pub mod query;
pub mod scene;

pub use build::{TraceBuildOptions, TraceProgress, TraceSceneBuilder, TraceStage};
pub use debug::{DebugHit, TraceDebugPass};
pub use gpu::{TraceGpuScene, TraceSceneInfo, TRACE_SCENE_GROUP};
pub use job::{RenderMode, TraceStatus, Tracer};
pub use limits::TraceLimitError;
pub use modes::{FurnaceSettings, TraceSettings};
pub use query::{TraceHit, TraceRay, NO_HIT};
pub use scene::{TraceScene, TraceStats};

#[cfg(test)]
mod tests;
