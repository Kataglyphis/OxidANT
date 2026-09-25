use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use log::warn;

use kataglyphis_telemetry::resource_monitor;

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub id: u64,
    pub data: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

/// Convert a (possibly stride-padded) RGBA buffer into a tightly-packed one.
///
/// When the source buffer is already tightly packed (`src.len() == width * height * 4`),
/// the data is copied directly into an `Arc<[u8]>` — one allocation instead of two
/// (`Vec` + `Arc`).  When stride-stripping is needed, a temporary `Vec` is used.
pub(crate) fn rgba_tightly_packed(src: &[u8], width: u32, height: u32) -> Option<Arc<[u8]>> {
    if width == 0 || height == 0 {
        return None;
    }

    let row_bytes = (width as usize).saturating_mul(4);
    let expected_len = row_bytes.saturating_mul(height as usize);

    // Fast path: already tightly packed — copy straight into Arc.
    if src.len() == expected_len {
        return Some(Arc::from(src));
    }

    // Many GStreamer buffers are padded per row (stride). We conservatively try to
    // interpret the buffer as a single RGBA plane with a constant stride.
    if src.len() < expected_len {
        return None;
    }

    let h = height as usize;
    // The largest valid stride is `src.len() / h` (integer division rounds
    // down), so `stride * h <= src.len()` is guaranteed — no loop needed.
    let stride = src.len() / h;
    if stride < row_bytes {
        return None;
    }

    let mut out = vec![0u8; expected_len];
    for y in 0..h {
        let src_start = y.saturating_mul(stride);
        let src_end = src_start.saturating_add(row_bytes).min(src.len());
        let dst_start = y.saturating_mul(row_bytes);
        let dst_end = dst_start.saturating_add(row_bytes).min(out.len());

        if src_end <= src_start || dst_end <= dst_start {
            return None;
        }

        out[dst_start..dst_end].copy_from_slice(&src[src_start..src_end]);
    }

    Some(Arc::from(out))
}

pub(crate) fn build_pipeline(
    frame_tx: std::sync::mpsc::SyncSender<Frame>,
) -> Result<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();

    let frame_id = AtomicU64::new(1);

    let src = gst::ElementFactory::make("mfvideosrc")
        .build()
        .or_else(|_| gst::ElementFactory::make("autovideosrc").build())
        .context("Failed to create a video source")?;

    let convert = gst::ElementFactory::make("videoconvert")
        .build()
        .context("Failed to create videoconvert")?;

    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .build();

    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()
        .context("Failed to create capsfilter")?;

    let sink = gst::ElementFactory::make("appsink")
        .property("emit-signals", true)
        .property("max-buffers", 2u32)
        .property("drop", true)
        .build()
        .context("Failed to create appsink")?;

    pipeline
        .add_many([&src, &convert, &capsfilter, &sink])
        .context("Failed to add elements to pipeline")?;
    gst::Element::link_many([&src, &convert, &capsfilter, &sink])
        .context("Failed to link pipeline elements")?;

    let appsink = sink
        .clone()
        .dynamic_cast::<gst_app::AppSink>()
        .map_err(|_| anyhow::anyhow!("Sink element is not an AppSink"))?;

    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                if let Ok(sample) = sink.pull_sample() {
                    if let Some(buffer) = sample.buffer() {
                        let caps = sample
                            .caps()
                            .and_then(|c| c.structure(0))
                            .map(|s| s.to_owned());
                        if let Some(structure) = caps {
                            let width = structure.get::<i32>("width").unwrap_or_else(|_| {
                                warn!("GStreamer caps missing 'width'; defaulting to 640");
                                640
                            }) as u32;
                            let height = structure.get::<i32>("height").unwrap_or_else(|_| {
                                warn!("GStreamer caps missing 'height'; defaulting to 480");
                                480
                            }) as u32;
                            if let Ok(map) = buffer.map_readable() {
                                if let Some(data) =
                                    rgba_tightly_packed(map.as_slice(), width, height)
                                {
                                    resource_monitor::record_camera_frame();

                                    let id = frame_id.fetch_add(1, Ordering::Relaxed);

                                    if let Err(e) = frame_tx.try_send(Frame {
                                        id,
                                        data,
                                        width,
                                        height,
                                    }) {
                                        warn!("Frame channel full, dropping frame {id}: {e}");
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    Ok(pipeline)
}

/// What [`media_check`] found: the GStreamer version, the plugin directory the exe
/// carries (if any), and each camera-pipeline element with the plugin file behind it.
pub struct MediaReport {
    pub version: String,
    pub bundled_plugins: Option<PathBuf>,
    pub elements: Vec<(String, Option<PathBuf>)>,
}

impl std::fmt::Display for MediaReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.version)?;
        match &self.bundled_plugins {
            Some(dir) => writeln!(f, "plugins beside the exe: {}", dir.display())?,
            None => writeln!(
                f,
                "plugins beside the exe: none, GStreamer's own search path"
            )?,
        }
        for (factory, file) in &self.elements {
            match file {
                Some(file) => writeln!(f, "{factory}: {}", file.display())?,
                None => writeln!(f, "{factory}: built into GStreamer")?,
            }
        }
        Ok(())
    }
}

/// Builds the camera pipeline without starting it, so with no camera and no window,
/// and names the plugin file behind each element: proof that this exe finds every
/// element the GUI creates by name. An exe that carries its own plugins must take
/// every element from them, or a gap in the package would hide behind a host's
/// GStreamer installation.
pub fn media_check() -> Result<MediaReport> {
    kataglyphis_media::ensure_gst_initialized()?;
    let (frame_tx, _frame_rx) = std::sync::mpsc::sync_channel::<Frame>(1);
    let pipeline = build_pipeline(frame_tx).context("Failed to build the camera pipeline")?;
    let bundled_plugins = kataglyphis_media::bundled_plugin_dir();
    // A bin prepends each child it adds, so reversed the list reads source first.
    let mut children = pipeline.children();
    children.reverse();
    let mut elements = Vec::new();
    for element in children {
        let factory = element
            .factory()
            .with_context(|| format!("Pipeline element {} has no factory", element.name()))?;
        let file = factory.plugin().and_then(|plugin| plugin.filename());
        if let (Some(dir), Some(file)) = (&bundled_plugins, &file) {
            if !lies_under(file, dir) {
                anyhow::bail!(
                    "{} loads from {}, outside the plugins beside this exe ({}): the package lacks it, or GST_PLUGIN_PATH names another installation",
                    factory.name(),
                    file.display(),
                    dir.display()
                );
            }
        }
        elements.push((factory.name().to_string(), file));
    }
    Ok(MediaReport {
        version: gst::version_string().to_string(),
        bundled_plugins,
        elements,
    })
}

/// True when `file` lies in `dir` or below it, compared canonically where both
/// resolve (case, `\\?\` prefixes and junctions alike).
fn lies_under(file: &Path, dir: &Path) -> bool {
    match (std::fs::canonicalize(file), std::fs::canonicalize(dir)) {
        (Ok(file), Ok(dir)) => file.starts_with(dir),
        _ => file.starts_with(dir),
    }
}
