//! The ray-traced and path-traced modes over the tracer's scene: settings, accumulation, dispatch plan and GPU passes.
//! What each mode computes and how it differs from the C++ engine: `../../../../docs/tracer.md` § The modes.

pub mod accumulation;
pub mod dispatch;
pub mod passes;
pub mod settings;

pub use accumulation::{Accumulation, AccumulationKey};
pub use dispatch::{plan, TraceDispatch, DISPATCH_SEGMENTS, TILE_SIZE};
pub use passes::{
    accumulation_bytes, accumulation_limit, AfterResolve, Kernel, TraceFrameUniforms,
    TraceTileUniforms, TracedFrame, TracedPasses, TracedSources, KERNEL_GROUP_SIZE,
};
pub use settings::{FurnaceSettings, TraceSettings, MAX_BOUNCES, SAMPLES_PER_PIXEL};
