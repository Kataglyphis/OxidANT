//! Webcam live-inference API; frames go straight to the texture, only detections cross the bridge.

use crate::frb_generated::StreamSink;

pub struct CameraDesc {
    pub index: u32,
    pub name: String,
}

/// A detection in source-frame pixels; a field-accessible mirror of the opaque core `Detection`.
#[derive(Clone)]
pub struct DetectionBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub score: f32,
    pub class_id: i64,
}

impl From<crate::Detection> for DetectionBox {
    fn from(d: crate::Detection) -> Self {
        Self {
            x1: d.x1,
            y1: d.y1,
            x2: d.x2,
            y2: d.y2,
            score: d.score,
            class_id: d.class_id,
        }
    }
}

pub struct WebcamStreamConfig {
    /// `videotestsrc` instead of a real camera (containers/CI have none).
    pub use_test_source: bool,
    /// Camera enumeration index; `None` = first available.
    pub device_index: Option<u32>,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub model_path: String,
    pub score_threshold: f32,
    /// Class ids to report (see `detector_labels`); empty reports every class.
    pub classes: Vec<i64>,
    /// Texture id from the native plugin's `create` call; `0` = headless.
    pub texture_id: i64,
    /// Override for the DLL exporting `knt_push_frame`.
    pub texture_library: Option<String>,
}

// Clone: the generated `StreamSink<T>` clones only where `T` does, and a failed start reports through a second handle.
#[derive(Clone)]
pub struct DetectionEvent {
    pub detections: Vec<DetectionBox>,
    /// The inference backend: `ort` or `tract`.
    pub backend: String,
    /// The execution provider: `cpu`, or e.g. `xnnpack` or `cuda`.
    pub execution_provider: String,
    pub frame_sequence: u64,
    pub pts_ms: Option<u64>,
    pub inference_ms: f64,
    pub fps: f32,
}

/// Whether this target runs the webcam pipeline (desktop: GStreamer); compile-time, cheap enough for a widget's build.
#[flutter_rust_bridge::frb(sync)]
pub fn webcam_inference_available() -> bool {
    cfg!(desktop)
}

/// Not sync on purpose: the device monitor starts libcamera, about a second the UI thread must not wait out.
pub fn list_cameras() -> Result<Vec<CameraDesc>, String> {
    #[cfg(desktop)]
    {
        kataglyphis_media::list_cameras()
            .map(|cams| {
                cams.into_iter()
                    .map(|c| CameraDesc {
                        index: c.index,
                        name: c.display_name,
                    })
                    .collect()
            })
            .map_err(|e| format!("camera enumeration failed: {e:#}"))
    }
    #[cfg(not(desktop))]
    {
        Err("webcam capture needs a desktop target: Android and the web have no GStreamer".into())
    }
}

/// Streams capture + inference events to `sink` until [`stop_webcam_inference`]; errors if running.
pub fn start_webcam_inference(
    config: WebcamStreamConfig,
    sink: StreamSink<DetectionEvent>,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        // Through the stream, not the return value: Dart's stream is all the caller holds, and a returned error only closes it.
        if let Err(e) = enabled::start(config, sink.clone()) {
            log::error!("start_webcam_inference failed: {e:#}");
            let _ = sink.add_error(anyhow::anyhow!("start_webcam_inference failed: {e:#}"));
        }
        Ok(())
    }
    #[cfg(not(desktop))]
    {
        let (_, _) = (config, sink);
        Err("webcam inference needs a desktop target: Android and the web have no GStreamer".into())
    }
}

/// Stops the running session (no-op when idle). Sync, so it only asks: the session ends on a thread of its own.
#[flutter_rust_bridge::frb(sync)]
pub fn stop_webcam_inference() {
    #[cfg(desktop)]
    enabled::stop();
}

#[cfg(desktop)]
mod enabled {
    use std::sync::Arc;

    use anyhow::Result;
    use kataglyphis_media::CameraSource;

    use super::{DetectionEvent, StreamSink, WebcamStreamConfig};
    use crate::session_slot::SessionSlot;
    use crate::webcam_engine::{EngineConfig, TextureTarget, WebcamEngine};

    static ENGINE: SessionSlot<WebcamEngine> = SessionSlot::new();

    pub(super) fn start(
        config: WebcamStreamConfig,
        sink: StreamSink<DetectionEvent>,
    ) -> Result<()> {
        ENGINE.start(|| open(config, sink))
    }

    fn open(config: WebcamStreamConfig, sink: StreamSink<DetectionEvent>) -> Result<WebcamEngine> {
        let source = if config.use_test_source {
            CameraSource::Test
        } else {
            match config.device_index {
                Some(index) => CameraSource::Device(index),
                None => CameraSource::Auto,
            }
        };

        let texture = (config.texture_id != 0).then(|| TextureTarget {
            library: config
                .texture_library
                .clone()
                .unwrap_or_else(default_texture_library),
            texture_id: config.texture_id,
        });

        WebcamEngine::start(
            EngineConfig {
                source,
                width: config.width,
                height: config.height,
                framerate: config.framerate,
                model_path: config.model_path,
                score_threshold: config.score_threshold,
                classes: config.classes,
                texture,
                on_detections: Some(Arc::new(crate::api::esp32::notify_detections)),
            },
            Box::new(move |event| {
                let _ = match event {
                    Ok(event) => sink.add(DetectionEvent {
                        detections: event.detections.into_iter().map(Into::into).collect(),
                        backend: event.backend.to_owned(),
                        execution_provider: event.execution_provider.to_owned(),
                        frame_sequence: event.frame_sequence,
                        pts_ms: event.pts_ms,
                        inference_ms: event.inference_ms,
                        fps: event.fps,
                    }),
                    Err(reason) => sink.add_error(anyhow::anyhow!(reason)),
                };
            }),
        )
    }

    pub(super) fn stop() {
        ENGINE.stop();
    }

    pub(super) fn default_texture_library() -> String {
        if cfg!(windows) {
            "kataglyphis_native_inference_plugin.dll".into()
        } else {
            "libkataglyphis_native_inference_plugin.so".into()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_probe_answers_for_the_target() {
        assert_eq!(super::webcam_inference_available(), cfg!(desktop));
    }

    #[cfg(desktop)]
    #[test]
    fn cameras_list_and_an_idle_stop_is_a_no_op() {
        // A container has no camera; the enumeration must still succeed.
        let cameras = super::list_cameras().unwrap();
        assert!(cameras.iter().all(|c| !c.name.is_empty()));
        super::stop_webcam_inference();
    }

    #[cfg(desktop)]
    #[test]
    fn the_default_texture_library_is_the_native_plugin() {
        let name = super::enabled::default_texture_library();
        assert!(
            name.contains("kataglyphis_native_inference_plugin"),
            "{name}"
        );
    }
}
