//! The renderer's traced scene: the CPU scene it builds from, and the build a traced mode starts.

use std::sync::Arc;

use super::ForwardRenderer;
use crate::render::tracer::{TraceStatus, Tracer};
use crate::scene::CpuScene;

impl ForwardRenderer {
    /// The scene the traced modes build from, the one [`Self::upload_scene`] took; kept until the next upload.
    pub fn set_trace_source(&mut self, scene: Arc<CpuScene>) {
        self.tracer.set_source(scene);
    }

    pub fn trace_status(&self) -> TraceStatus {
        self.tracer.status()
    }

    pub fn tracer(&self) -> &Tracer {
        &self.tracer
    }

    pub fn tracer_mut(&mut self) -> &mut Tracer {
        &mut self.tracer
    }
}
