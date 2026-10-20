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

    // GStreamer often pads rows; assume a single RGBA plane with a constant stride.
    if src.len() < expected_len {
        return None;
    }

    let h = height as usize;
    // Integer division guarantees `stride * h <= src.len()`.
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

/// What [`media_check`] found: GStreamer version, bundled plugin dir, each element's plugin.
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

/// Builds the camera pipeline without starting it and names the plugin file behind each element.
/// Fails when a bundled exe takes an element from elsewhere, so a package gap cannot hide.
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

/// True when `file` lies in `dir` or below it, compared canonically where both resolve.
fn lies_under(file: &Path, dir: &Path) -> bool {
    match (std::fs::canonicalize(file), std::fs::canonicalize(dir)) {
        (Ok(file), Ok(dir)) => file.starts_with(dir),
        _ => file.starts_with(dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_frame_passes_unchanged() {
        let src: Vec<u8> = (0..2 * 2 * 4).map(|i| i as u8).collect();
        assert_eq!(rgba_tightly_packed(&src, 2, 2).as_deref(), Some(&src[..]));
    }

    #[test]
    fn row_padding_is_dropped() {
        // Two 1-pixel rows, each padded to 8 bytes.
        let src = [1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0];
        let packed = rgba_tightly_packed(&src, 1, 2).unwrap();
        assert_eq!(&packed[..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn an_empty_or_short_frame_is_refused() {
        assert!(rgba_tightly_packed(&[0; 16], 0, 4).is_none());
        assert!(rgba_tightly_packed(&[0; 16], 4, 0).is_none());
        assert!(rgba_tightly_packed(&[0; 15], 2, 2).is_none());
    }

    #[test]
    fn a_plugin_counts_as_bundled_only_inside_the_dir() {
        let dir = std::env::temp_dir().join(format!("oxidant-gui-plugins-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inside = dir.join("libgstapp.so");
        std::fs::write(&inside, b"").unwrap();
        let outside =
            std::env::temp_dir().join(format!("oxidant-gui-elsewhere-{}.so", std::process::id()));
        std::fs::write(&outside, b"").unwrap();
        let (inside_ok, outside_ok) = (lies_under(&inside, &dir), lies_under(&outside, &dir));
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_file(&outside).unwrap();
        assert!(inside_ok);
        assert!(!outside_ok);
    }

    #[test]
    fn the_report_names_each_element_and_its_plugin() {
        let report = MediaReport {
            version: "GStreamer 1.29.2".into(),
            bundled_plugins: None,
            elements: vec![
                ("appsink".into(), Some(PathBuf::from("/p/libgstapp.so"))),
                ("bin".into(), None),
            ],
        };
        assert_eq!(
            report.to_string(),
            "GStreamer 1.29.2\nplugins beside the exe: none, GStreamer's own search path\n\
             appsink: /p/libgstapp.so\nbin: built into GStreamer\n"
        );
    }

    #[test]
    fn the_camera_pipeline_builds_without_a_camera() {
        let report = media_check().expect("the GUI's pipeline must build");
        let names: Vec<&str> = report.elements.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names
                .first()
                .is_some_and(|n| *n == "mfvideosrc" || *n == "autovideosrc"),
            "{names:?}"
        );
        assert_eq!(&names[1..], ["videoconvert", "capsfilter", "appsink"]);
    }
}
