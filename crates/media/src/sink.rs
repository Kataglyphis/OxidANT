//! The one place a frame sink is made, so every pipeline drops stale frames the same way.

use anyhow::{anyhow, Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// The element factory behind [`frame_sink`]; a pipeline report names its plugin through it.
pub const FRAME_SINK_FACTORY: &str = "appsink";

/// A sink holding at most `max_buffers` frames (0: unbounded), dropping the oldest; `sync` paces it to the clock.
pub fn frame_sink(max_buffers: u32, sync: bool) -> Result<gst_app::AppSink> {
    // Made through its factory, unlike AppSink::builder(), so the element reports which plugin file it came from.
    gst::ElementFactory::make(FRAME_SINK_FACTORY)
        .property("max-buffers", max_buffers)
        .property("drop", true)
        .property("sync", sync)
        .build()
        .context(FRAME_SINK_FACTORY)?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow!("{FRAME_SINK_FACTORY} is not an AppSink"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sink_keeps_the_requested_frames_and_drops_the_rest() {
        crate::ensure_gst_initialized().unwrap();
        let sink = frame_sink(2, false).unwrap();
        assert_eq!(sink.max_buffers(), 2);
        assert!(sink.is_drop());
        assert!(!sink.is_sync());
        assert_eq!(sink.factory().unwrap().name(), FRAME_SINK_FACTORY);
    }
}
