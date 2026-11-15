use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use kataglyphis_media::{CameraSource, CaptureConfig, CaptureSession, VideoFrame};

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub id: u64,
    pub data: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

impl From<VideoFrame> for Frame {
    fn from(frame: VideoFrame) -> Self {
        Self {
            id: frame.sequence,
            data: Arc::from(frame.rgba),
            width: frame.width,
            height: frame.height,
        }
    }
}

/// The GUI's camera: the first webcam at its own size and rate, through the media crate's session.
pub(crate) fn camera_config() -> CaptureConfig {
    CaptureConfig {
        source: CameraSource::Auto,
        width: 0,
        height: 0,
        framerate: 0,
    }
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
    let pipeline = CaptureSession::unstarted_pipeline(&camera_config())
        .context("Failed to build the camera pipeline")?;
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
    fn a_session_frame_keeps_its_number_size_and_pixels() {
        let frame = Frame::from(VideoFrame {
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
            width: 2,
            height: 1,
            pts_ms: Some(40),
            sequence: 7,
        });
        assert_eq!((frame.id, frame.width, frame.height), (7, 2, 1));
        assert_eq!(&frame.data[..], &[1, 2, 3, 4, 5, 6, 7, 8]);
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
                (
                    "videoconvert".into(),
                    Some(PathBuf::from("/p/libgstvideoconvertscale.so")),
                ),
                ("bin".into(), None),
            ],
        };
        assert_eq!(
            report.to_string(),
            "GStreamer 1.29.2\nplugins beside the exe: none, GStreamer's own search path\n\
             videoconvert: /p/libgstvideoconvertscale.so\nbin: built into GStreamer\n"
        );
    }

    #[test]
    fn the_camera_pipeline_builds_without_a_camera() {
        let report = media_check().expect("the GUI's pipeline must build");
        let names: Vec<&str> = report.elements.iter().map(|(n, _)| n.as_str()).collect();
        // The media crate's session, which reads its bus and notices a stall: no third camera pipeline.
        assert!(
            names.first().is_some_and(|n| {
                ["mfvideosrc", "ksvideosrc", "autovideosrc", "v4l2src"].contains(n)
            }),
            "{names:?}"
        );
        assert_eq!(
            &names[1..],
            [
                "videoconvert",
                "videoscale",
                kataglyphis_media::FRAME_SINK_FACTORY
            ]
        );
    }
}
