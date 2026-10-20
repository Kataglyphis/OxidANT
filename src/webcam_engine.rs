//! Webcam → ONNX person-detection engine (native only): frames to the texture, detections out.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use kataglyphis_media::{CameraSource, CaptureConfig, CaptureSession};

use crate::person_detection::PersonDetector;
use crate::Detection;

pub struct EngineEvent {
    pub detections: Vec<Detection>,
    pub frame_sequence: u64,
    pub pts_ms: Option<u64>,
    pub inference_ms: f64,
    pub fps: f32,
}

/// Runs on the inference worker thread, once per inferred frame.
pub type DetectionsHook = Arc<dyn Fn(&[Detection]) + Send + Sync>;

pub struct EngineConfig {
    pub source: CameraSource,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub model_path: String,
    pub score_threshold: f32,
    /// Flutter texture to feed; `None` runs headless (container CI, tests).
    pub texture: Option<TextureTarget>,
    /// Called with every inference result; the ESP32 bridge hooks in here, so a
    /// detection reaches the board without a round trip through Dart.
    pub on_detections: Option<DetectionsHook>,
}

pub struct TextureTarget {
    /// DLL exporting the `knt_*` C ABI (the native inference plugin).
    pub library: String,
    pub texture_id: i64,
}

type PushFrameFn =
    unsafe extern "C" fn(texture_id: i64, rgba: *const u8, width: u32, height: u32) -> i32;

struct TexturePusher {
    /// Keeps the plugin DLL pinned while `push` is callable.
    _lib: libloading::Library,
    push: PushFrameFn,
    texture_id: i64,
}

impl TexturePusher {
    fn new(target: &TextureTarget) -> Result<Self> {
        // The Flutter engine already loaded the plugin DLL; this only bumps its refcount.
        unsafe {
            let lib = libloading::Library::new(&target.library)
                .with_context(|| format!("failed to open texture library `{}`", target.library))?;
            let sym: libloading::Symbol<PushFrameFn> = lib
                .get(b"knt_push_frame\0")
                .context("`knt_push_frame` not exported by texture library")?;
            let push = *sym;
            Ok(Self {
                _lib: lib,
                push,
                texture_id: target.texture_id,
            })
        }
    }

    fn push(&self, rgba: &[u8], width: u32, height: u32) -> bool {
        unsafe { (self.push)(self.texture_id, rgba.as_ptr(), width, height) == 0 }
    }
}

/// Events per second over the kept instants: intervals over their span, 0 until two exist.
fn rolling_fps(recent: &VecDeque<Instant>) -> f32 {
    match (recent.front(), recent.back()) {
        (Some(first), Some(last)) if recent.len() >= 2 => {
            let span = last.duration_since(*first).as_secs_f32();
            if span > 0.0 {
                (recent.len() - 1) as f32 / span
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

pub struct WebcamEngine {
    capture: CaptureSession,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl WebcamEngine {
    pub fn start(
        config: EngineConfig,
        on_event: Box<dyn Fn(EngineEvent) + Send + 'static>,
    ) -> Result<Self> {
        let model_path = crate::person_detection::resolve_model_path(Some(&config.model_path));
        let mut detector = PersonDetector::new(&model_path)?;

        let pusher = config
            .texture
            .as_ref()
            .map(TexturePusher::new)
            .transpose()?;

        let capture = CaptureSession::start(CaptureConfig {
            source: config.source,
            width: config.width,
            height: config.height,
            framerate: config.framerate,
        })?;
        let frames = capture.frames();

        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let score_threshold = config.score_threshold;
        let on_detections = config.on_detections;

        let worker = std::thread::Builder::new()
            .name("webcam-inference".into())
            .spawn(move || {
                let mut recent: VecDeque<Instant> = VecDeque::with_capacity(32);
                while !stop_worker.load(Ordering::Acquire) {
                    let Some(frame) = frames.take_latest(Duration::from_millis(200)) else {
                        if frames.is_closed() {
                            break;
                        }
                        continue;
                    };

                    if let Some(pusher) = &pusher {
                        if !pusher.push(&frame.rgba, frame.width, frame.height) {
                            log::warn!("texture push rejected frame {}", frame.sequence);
                        }
                    }

                    let started = Instant::now();
                    let detections = match detector.infer_persons_rgba(
                        &frame.rgba,
                        frame.width,
                        frame.height,
                        score_threshold,
                    ) {
                        Ok(d) => d,
                        Err(e) => {
                            log::error!("inference failed on frame {}: {e:#}", frame.sequence);
                            continue;
                        }
                    };
                    let inference_ms = started.elapsed().as_secs_f64() * 1000.0;

                    if let Some(on_detections) = &on_detections {
                        on_detections(&detections);
                    }

                    recent.push_back(Instant::now());
                    while recent.len() > 30 {
                        recent.pop_front();
                    }
                    let fps = rolling_fps(&recent);

                    on_event(EngineEvent {
                        detections,
                        frame_sequence: frame.sequence,
                        pts_ms: frame.pts_ms,
                        inference_ms,
                        fps,
                    });
                }
            })
            .context("failed to spawn webcam inference worker")?;

        Ok(Self {
            capture,
            stop,
            worker: Some(worker),
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.capture.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for WebcamEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    /// The model `scripts/linux/fetch-models.sh` puts in the checkout; `None` skips, unless CI demands it.
    fn model() -> Option<String> {
        let path = crate::person_detection::resolve_model_path(None);
        if std::path::Path::new(&path).is_file() {
            return Some(path);
        }
        let required = std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() == Ok("1");
        assert!(
            !required,
            "no model at {path}: scripts/linux/fetch-models.sh fetches it"
        );
        eprintln!("SKIP: no model at {path}");
        None
    }

    fn config(model_path: String, texture: Option<TextureTarget>) -> EngineConfig {
        EngineConfig {
            source: CameraSource::Test,
            width: 320,
            height: 240,
            framerate: 15,
            model_path,
            score_threshold: 0.5,
            texture,
            on_detections: None,
        }
    }

    fn start_err(config: EngineConfig) -> String {
        match WebcamEngine::start(config, Box::new(|_| {})) {
            Ok(_) => panic!("the engine started"),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn fps_needs_two_instants_and_a_span() {
        let t = Instant::now();
        assert_eq!(rolling_fps(&VecDeque::new()), 0.0);
        assert_eq!(rolling_fps(&VecDeque::from([t])), 0.0);
        assert_eq!(rolling_fps(&VecDeque::from([t, t])), 0.0);
    }

    #[test]
    fn fps_counts_intervals_over_the_span() {
        let t = Instant::now();
        let recent: VecDeque<Instant> = (0..31)
            .map(|i| t + Duration::from_millis(100 * i))
            .collect();
        assert!(
            (rolling_fps(&recent) - 10.0).abs() < 1e-3,
            "{}",
            rolling_fps(&recent)
        );
    }

    #[test]
    fn a_missing_model_fails_the_start() {
        let err = start_err(config("/nonexistent/model.onnx".into(), None));
        assert_eq!(err, "no ONNX model at /nonexistent/model.onnx");
    }

    #[test]
    fn a_missing_texture_library_names_itself() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let target = TextureTarget {
            library: "/nonexistent/libplugin.so".into(),
            texture_id: 1,
        };
        let err = start_err(config(model, Some(target)));
        assert!(
            err.contains("failed to open texture library `/nonexistent/libplugin.so`"),
            "{err}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_library_without_the_texture_abi_is_refused() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let target = TextureTarget {
            library: "libc.so.6".into(),
            texture_id: 1,
        };
        let err = start_err(config(model, Some(target)));
        assert!(err.contains("`knt_push_frame` not exported"), "{err}");
    }

    #[test]
    fn headless_on_the_test_source_reports_frames_in_order_until_stopped() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let (tx, rx) = mpsc::channel();
        let mut engine = WebcamEngine::start(
            config(model, None),
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .unwrap();

        let events: Vec<EngineEvent> = (0..5)
            .map(|_| {
                rx.recv_timeout(Duration::from_secs(60))
                    .expect("no event within 60 s")
            })
            .collect();
        engine.stop();

        for pair in events.windows(2) {
            assert!(
                pair[1].frame_sequence > pair[0].frame_sequence,
                "sequence not rising"
            );
        }
        assert!(events.iter().all(|e| e.inference_ms > 0.0));
        assert!(events.last().unwrap().fps > 0.0);
        // Joined: the worker dropped its callback, so after the backlog the channel reports closed.
        let _backlog = rx.try_iter().count();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).err(),
            Some(mpsc::RecvTimeoutError::Disconnected),
            "the worker outlived stop()"
        );
    }

    #[derive(Debug)]
    enum Seen {
        Hook(Vec<Detection>),
        Event(Vec<Detection>),
    }

    #[test]
    fn the_detections_hook_sees_each_frames_detections_before_its_event() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let (tx, rx) = mpsc::channel();
        let hook_tx = tx.clone();
        let hook: DetectionsHook = Arc::new(move |detections: &[Detection]| {
            let _ = hook_tx.send(Seen::Hook(detections.to_vec()));
        });
        let mut engine = WebcamEngine::start(
            EngineConfig {
                on_detections: Some(hook),
                ..config(model, None)
            },
            Box::new(move |event| {
                let _ = tx.send(Seen::Event(event.detections));
            }),
        )
        .unwrap();

        let seen: Vec<Seen> = (0..6)
            .map(|_| {
                rx.recv_timeout(Duration::from_secs(60))
                    .expect("nothing within 60 s")
            })
            .collect();
        engine.stop();

        for pair in seen.chunks(2) {
            let [Seen::Hook(hooked), Seen::Event(reported)] = pair else {
                panic!("the hook did not run once before each event: {seen:?}");
            };
            assert_eq!(hooked, reported);
        }
    }
}
