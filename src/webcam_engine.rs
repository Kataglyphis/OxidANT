//! Webcam → ONNX person-detection engine (native only): frames to the texture, detections out.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use kataglyphis_media::{CameraSource, CaptureConfig, CaptureSession};

use crate::person_detection::PersonDetector;
use crate::texture_abi::{ApiVersionFn, PushFrameFn, API_VERSION, OK};
use crate::Detection;

/// Consecutive failed runs after which the engine gives up: a model that fails every frame will not recover.
const MAX_FAILED_RUNS: u32 = 30;

pub struct EngineEvent {
    pub detections: Vec<Detection>,
    pub backend: &'static str,
    pub execution_provider: &'static str,
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
    /// Class ids to keep; empty keeps every class.
    pub classes: Vec<i64>,
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
            let version: libloading::Symbol<ApiVersionFn> = lib
                .get(b"knt_api_version\0")
                .context("`knt_api_version` not exported by texture library")?;
            let actual = version();
            anyhow::ensure!(
                actual == API_VERSION,
                "texture ABI version {actual}, expected {API_VERSION}"
            );
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
        unsafe { (self.push)(self.texture_id, rgba.as_ptr(), width, height) == OK }
    }

    /// Pushes every captured frame, so the preview keeps the camera's rate while the model is busy with one.
    fn into_observer(self) -> kataglyphis_media::FrameObserver {
        let mut rejected = false;
        Box::new(move |frame| {
            let pushed = self.push(&frame.rgba, frame.width, frame.height);
            // Once per streak: a texture that is gone rejects every frame.
            if !pushed && !rejected {
                log::warn!("texture push rejected frame {}", frame.sequence);
            }
            rejected = !pushed;
        })
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
    /// Starts capture and inference; `on_event` gets every result, and once an `Err` when the session ends by itself.
    pub fn start(
        config: EngineConfig,
        on_event: Box<dyn Fn(Result<EngineEvent, String>) + Send + 'static>,
    ) -> Result<Self> {
        let model_path = crate::person_detection::resolve_model_path(Some(&config.model_path));
        let mut detector = PersonDetector::new(&model_path)?;

        let pusher = config
            .texture
            .as_ref()
            .map(TexturePusher::new)
            .transpose()?;

        let capture = CaptureSession::start_observed(
            CaptureConfig {
                source: config.source,
                width: config.width,
                height: config.height,
                framerate: config.framerate,
            },
            pusher.map(TexturePusher::into_observer),
        )?;
        let frames = capture.frames();

        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let score_threshold = config.score_threshold;
        let classes = config.classes;
        let on_detections = config.on_detections;
        let (backend, execution_provider) =
            (detector.backend_name(), detector.execution_provider());

        let worker = std::thread::Builder::new()
            .name("webcam-inference".into())
            .spawn(move || {
                let mut recent: VecDeque<Instant> = VecDeque::with_capacity(32);
                let mut failed_runs = 0u32;
                while !stop_worker.load(Ordering::Acquire) {
                    let Some(frame) = frames.take_latest(Duration::from_millis(200)) else {
                        if frames.is_closed() {
                            // Closed by stop() there is nothing to say; closed by the pipeline, the viewer must hear why.
                            if !stop_worker.load(Ordering::Acquire) {
                                let reason = frames.error();
                                on_event(Err(
                                    reason.unwrap_or_else(|| "the camera stream ended".into())
                                ));
                            }
                            break;
                        }
                        continue;
                    };

                    let started = Instant::now();
                    let filter = (!classes.is_empty()).then_some(classes.as_slice());
                    let detections = match detector.infer_rgba(
                        &frame.rgba,
                        frame.width,
                        frame.height,
                        score_threshold,
                        filter,
                    ) {
                        Ok(d) => d,
                        Err(e) => {
                            failed_runs += 1;
                            // The first of a streak: every frame of a broken model would say the same.
                            if failed_runs == 1 {
                                log::error!("inference failed on frame {}: {e:#}", frame.sequence);
                            }
                            if failed_runs >= MAX_FAILED_RUNS {
                                on_event(Err(format!("inference keeps failing: {e:#}")));
                                break;
                            }
                            continue;
                        }
                    };
                    failed_runs = 0;
                    let inference_ms = started.elapsed().as_secs_f64() * 1000.0;

                    if let Some(on_detections) = &on_detections {
                        on_detections(&detections);
                    }

                    recent.push_back(Instant::now());
                    while recent.len() > 30 {
                        recent.pop_front();
                    }
                    let fps = rolling_fps(&recent);

                    on_event(Ok(EngineEvent {
                        detections,
                        backend,
                        execution_provider,
                        frame_sequence: frame.sequence,
                        pts_ms: frame.pts_ms,
                        inference_ms,
                        fps,
                    }));
                }
            })
            .context("failed to spawn webcam inference worker")?;

        Ok(Self {
            capture,
            stop,
            worker: Some(worker),
        })
    }

    /// Whether the session ended by itself: its worker left after a capture or model failure.
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
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
            classes: vec![0],
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
        let target = TextureTarget {
            library: "libc.so.6".into(),
            texture_id: 1,
        };
        let err = match TexturePusher::new(&target) {
            Ok(_) => panic!("accepted a library without the texture ABI"),
            Err(err) => format!("{err:#}"),
        };
        assert!(err.contains("`knt_api_version` not exported"), "{err}");
    }

    /// `source` compiled into a shared library in `directory`: a stand-in for the native plugin.
    #[cfg(target_os = "linux")]
    fn texture_stub(directory: &std::path::Path, source: &str) -> String {
        let (file, library) = (directory.join("stub.c"), directory.join("stub.so"));
        std::fs::write(&file, source).unwrap();
        let cross_compiler = if cfg!(target_arch = "riscv64") {
            std::env::var_os("CC_riscv64gc_unknown_linux_gnu")
        } else {
            None
        };
        let compiler = cross_compiler
            .or_else(|| std::env::var_os("CC"))
            .unwrap_or_else(|| "cc".into());
        assert!(std::process::Command::new(compiler)
            .args(["-shared", "-fPIC"])
            .arg(&file)
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success());
        library.to_string_lossy().into_owned()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_wrong_texture_version_is_refused_before_loading_the_callback() {
        let directory = tempfile::tempdir().unwrap();
        let source = format!(
            "int knt_api_version(void) {{ return {}; }}",
            API_VERSION + 1
        );
        let library = texture_stub(directory.path(), &source);
        let target = TextureTarget {
            library,
            texture_id: 1,
        };
        let err = match TexturePusher::new(&target) {
            Ok(_) => panic!("accepted an incompatible texture ABI"),
            Err(err) => format!("{err:#}"),
        };
        assert!(
            err.contains(&format!(
                "texture ABI version {}, expected {API_VERSION}",
                API_VERSION + 1
            )),
            "{err}"
        );
        assert!(!err.contains("`knt_push_frame` not exported"), "{err}");
    }

    #[test]
    fn headless_on_the_test_source_reports_frames_in_order_until_stopped() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let (tx, rx) = mpsc::channel();
        let mut engine = WebcamEngine::start(
            config(model, None),
            Box::new(move |event| {
                let _ = tx.send(event.expect("the session ended by itself"));
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

    #[test]
    fn events_name_the_backend_and_the_cpu_provider_and_keep_only_the_classes() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let (tx, rx) = mpsc::channel();
        let mut engine = WebcamEngine::start(
            EngineConfig {
                classes: vec![15, 16],
                ..config(model, None)
            },
            Box::new(move |event| {
                let _ = tx.send(event.expect("the session ended by itself"));
            }),
        )
        .unwrap();
        let event = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("no event within 60 s");
        engine.stop();
        let primary = if cfg!(any(target_os = "linux", windows)) {
            "ort"
        } else {
            "tract"
        };
        assert_eq!((event.backend, event.execution_provider), (primary, "cpu"));
        assert!(event
            .detections
            .iter()
            .all(|d| [15, 16].contains(&d.class_id)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_texture_gets_every_captured_frame_not_only_the_inferred_ones() {
        let Some(model) = model() else { return };
        let _turn = crate::model_turn();
        let directory = tempfile::tempdir().unwrap();
        // Counts the frames of texture 7 at the asked size; anything else is refused.
        let source = format!(
            "static int pushed;\n\
             int knt_api_version(void) {{ return {API_VERSION}; }}\n\
             int knt_push_frame(long long id, const unsigned char *rgba, unsigned w, unsigned h) {{\n\
                 if (id != 7 || !rgba || w != 320 || h != 240) return -1;\n\
                 pushed++;\n\
                 return 0;\n\
             }}\n\
             int knt_pushed(void) {{ return pushed; }}"
        );
        let library = texture_stub(directory.path(), &source);
        // The same handle the engine opens, so the counter is the one its pushes move.
        let stub = unsafe { libloading::Library::new(&library).unwrap() };
        let pushed: libloading::Symbol<unsafe extern "C" fn() -> i32> =
            unsafe { stub.get(b"knt_pushed\0").unwrap() };

        let (tx, rx) = mpsc::channel();
        let target = TextureTarget {
            library: library.clone(),
            texture_id: 7,
        };
        let mut engine = WebcamEngine::start(
            config(model, Some(target)),
            Box::new(move |event| {
                let _ = tx.send(event.map(|event| event.frame_sequence));
            }),
        )
        .unwrap();
        let inferred: Vec<u64> = (0..3)
            .map(|_| {
                rx.recv_timeout(Duration::from_secs(60))
                    .expect("no event within 60 s")
                    .expect("the session ended by itself")
            })
            .collect();
        assert!(!engine.is_finished());
        engine.stop();
        assert!(engine.is_finished());

        // The model skips frames while it is busy; the preview must not.
        let last = *inferred.last().unwrap();
        let pushed = unsafe { pushed() } as u64;
        assert!(pushed >= last, "{pushed} pushed, frame {last} inferred");
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
                let event = event.expect("the session ended by itself");
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
