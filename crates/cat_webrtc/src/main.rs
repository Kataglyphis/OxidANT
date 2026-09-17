//! Cat detector → WebRTC producer.
//!
//! Loops a still image (or a `videotestsrc` pattern) through `jpegdec` into an
//! appsink — or captures from `v4l2src` / `libcamerasrc` — runs YOLO ONNX
//! inference for COCO class 15 (cat) with `kataglyphis_inference` on a worker
//! thread, paints the latest boxes into the RGBA frames and pushes them into
//! `webrtcsink`, which publishes them over the GStreamer signalling protocol
//! OmniAccelerANT's Stream page (`lib/Pages/StreamPage`) consumes.
//!
//! **No path outside this repository is baked in.** A live source needs
//! nothing but a flag; the still-image mode takes its picture from
//! `--image` or `$KATAGLYPHIS_CAT_IMAGE`, because that picture is not
//! tracked here and its checkout layout differs between a workstation, the
//! CI container and a Raspberry Pi:
//!
//! ```text
//! ORT_DYLIB_PATH=/usr/local/lib/onnxruntime-cpu/lib/libonnxruntime.so \
//!   kataglyphis_cat_webrtc --v4l2 /dev/video0
//! ORT_DYLIB_PATH=/usr/local/lib/onnxruntime-cpu/lib/libonnxruntime.so \
//!   kataglyphis_cat_webrtc --libcamera      # Raspberry Pi CSI camera
//! KATAGLYPHIS_CAT_IMAGE=/srv/assets/Thundy.jpg kataglyphis_cat_webrtc
//! ```

use std::thread;

use anyhow::{anyhow, Context as _};
use clap::Parser;
use gstreamer::prelude::*;
use kataglyphis_inference::person_detection::PersonDetector;

/// COCO class id of "cat" in the 80-class YOLO models shipped in this tree.
const COCO_CAT: i64 = 15;

/// Environment variable naming the still image looped as the demo source.
///
/// It replaced a compile-time default that concatenated `CARGO_MANIFEST_DIR`
/// with `/../../../ANThology/assets/images/cats/Thundy.jpg` - a path that
/// reached out of this repository, past its superproject, and into a SIBLING
/// submodule checkout. That resolved only while OxidANT sat at exactly one
/// place inside exactly one superproject: a standalone clone, a different
/// superproject or a Raspberry Pi got a path to nothing, and the failure
/// surfaced as a `multifilesrc` error logged from a worker thread rather
/// than as a bad argument. The location is configurable now, and nothing
/// this crate cannot see is assumed.
const IMAGE_ENV: &str = "KATAGLYPHIS_CAT_IMAGE";

/// Where that picture lives in a full family checkout, quoted in the error
/// below so "which image?" is answered without leaving the terminal. A
/// hint, deliberately NOT a fallback: no code path resolves it.
const DEFAULT_IMAGE_HINT: &str = "<family checkout root>/ANThology/assets/images/cats/Thundy.jpg";

/// Default ONNX model (OxidANT's yolov10m, end-to-end `[1,N,6]` output).
const DEFAULT_MODEL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/models/yolov10m.onnx"
);

#[derive(Parser, Debug)]
#[command(name = "cat-webrtc", about = "Cat detection over WebRTC (GStreamer)")]
struct Args {
    /// Still image to loop as the live source; `--test`, `--v4l2` and
    /// `--libcamera` override it. Falls back to `$KATAGLYPHIS_CAT_IMAGE`,
    /// and with neither set one of the live-source flags is required.
    #[arg(long)]
    image: Option<String>,

    /// Use `videotestsrc pattern=ball` instead of the image.
    #[arg(long)]
    test: bool,

    /// V4L2 capture device, e.g. `/dev/video0` (Linux; overrides image/test).
    #[arg(long)]
    v4l2: Option<String>,

    /// Use the system libcamera source (`libcamerasrc`). Required for the
    /// Raspberry Pi CSI camera, whose V4L2 nodes carry raw Bayer only.
    #[arg(long)]
    libcamera: bool,

    /// Stream frames without loading the ONNX model or running detection.
    /// For bring-up and for hosts too weak to run inference.
    #[arg(long)]
    no_inference: bool,

    /// Rotate the stream by this many degrees: 0, 90, 180 or 270. Use 180
    /// for a camera that is mounted upside down.
    #[arg(long, default_value_t = 0)]
    rotate: u32,

    /// ONNX model (default: OxidANT's yolov10m, end-to-end [1,N,6] output).
    #[arg(long, default_value = DEFAULT_MODEL)]
    model: String,

    /// Port for webrtcsink's built-in signalling server.
    #[arg(long, default_value_t = 8443)]
    listen_port: u32,

    /// TLS certificate (PEM) for the built-in signalling server — enables WSS,
    /// which a phone needs because an HTTPS page cannot open `ws://`.
    #[arg(long)]
    cert: Option<String>,

    /// TLS private key (PEM) matching `--cert`.
    #[arg(long)]
    key: Option<String>,

    /// Producer name shown to consumers.
    #[arg(long, default_value = "Trouble Tabbls Cat Cam")]
    name: String,

    /// Detection score threshold.
    #[arg(long, default_value_t = 0.25)]
    score: f32,

    /// Frame size pushed to webrtcsink.
    #[arg(long, default_value_t = 640)]
    width: u32,
    #[arg(long, default_value_t = 480)]
    height: u32,
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Keep every class instead of only cats.
    #[arg(long)]
    all_classes: bool,
}

/// `--image`, else `$KATAGLYPHIS_CAT_IMAGE`, else nothing. A blank
/// environment value counts as unset: exporting the variable empty to
/// "clear" it otherwise built a `multifilesrc` for the empty path.
fn resolve_image(flag: Option<String>) -> Option<String> {
    flag.or_else(|| std::env::var(IMAGE_ENV).ok())
        .filter(|value| !value.trim().is_empty())
}

fn missing_source_error() -> anyhow::Error {
    anyhow!(
        "no video source. Pass one of --test, --v4l2 <DEVICE> or --libcamera, or point the still-image mode at a file with --image <FILE> or ${IMAGE_ENV}. That picture is not tracked in this repository; in a full family checkout it is {DEFAULT_IMAGE_HINT}."
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let image = resolve_image(args.image.clone());
    if image.is_none() && !args.test && args.v4l2.is_none() && !args.libcamera {
        return Err(missing_source_error());
    }

    gstreamer::init().context("gstreamer::init")?;

    let (out_pipeline, appsrc) = build_output_pipeline(&args)?;
    out_pipeline
        .set_state(gstreamer::State::Playing)
        .context("output pipeline Playing")?;

    let worker = {
        let mut worker_args = WorkerArgs {
            image: image.clone(),
            test: args.test,
            v4l2: args.v4l2.clone(),
            libcamera: args.libcamera,
            no_inference: args.no_inference,
            rotate: args.rotate,
            model: args.model.clone(),
            score: args.score,
            width: args.width,
            height: args.height,
            fps: args.fps,
            all_classes: args.all_classes,
        };
        let appsrc = appsrc;
        thread::Builder::new()
            .name("cat-infer".into())
            .spawn(move || {
                if let Err(err) = run_worker(&mut worker_args, &appsrc) {
                    log::error!("worker stopped: {err:#}");
                }
            })
            .context("spawn worker")?
    };

    let bus = out_pipeline
        .bus()
        .ok_or_else(|| anyhow!("output pipeline has no bus"))?;
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        use gstreamer::MessageView;
        match msg.view() {
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                log::error!(
                    "pipeline error: {} ({:?})",
                    err.error(),
                    err.debug().unwrap_or_default()
                );
                break;
            }
            MessageView::StateChanged(state) if state.src().is_some_and(|s| s.name() == "ws") => {
                log::info!("webrtcsink state: {:?}", state.current());
            }
            _ => (),
        }
    }

    let _ = out_pipeline.set_state(gstreamer::State::Null);
    let _ = worker.join();
    Ok(())
}

fn build_output_pipeline(
    args: &Args,
) -> anyhow::Result<(gstreamer::Pipeline, gstreamer_app::AppSrc)> {
    let pipeline = gstreamer::Pipeline::new();

    let caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", args.width as i32)
        .field("height", args.height as i32)
        .field("framerate", gstreamer::Fraction::new(args.fps as i32, 1))
        .build();

    let appsrc = gstreamer::ElementFactory::make("appsrc")
        .name("frames")
        .build()
        .context("appsrc")?
        .downcast::<gstreamer_app::AppSrc>()
        .map_err(|_| anyhow!("element 'appsrc' is not an AppSrc"))?;
    appsrc.set_is_live(true);
    appsrc.set_do_timestamp(true);
    appsrc.set_format(gstreamer::Format::Time);
    appsrc.set_caps(Some(&caps));

    let queue = gstreamer::ElementFactory::make("queue")
        .property("max-size-buffers", 1u32)
        .property_from_str("leaky", "downstream")
        .build()
        .context("queue")?;
    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("out-convert")
        .build()
        .context("videoconvert")?;

    let webrtc = gstreamer::ElementFactory::make("webrtcsink")
        .name("ws")
        .build()
        .context("webrtcsink (is the rswebrtc plugin available?)")?;
    // webrtcsink can run the signalling server itself; that keeps this demo to
    // one process and the same GStreamer signalling protocol the web client
    // speaks. (Setting the separate signaller's `uri` is not possible from a
    // plain Element: `signaller` is NULL until a child property is written.)
    webrtc.set_property("run-signalling-server", true);
    webrtc.set_property("signalling-server-host", "0.0.0.0");
    webrtc.set_property("signalling-server-port", args.listen_port);
    if let (Some(cert), Some(key)) = (&args.cert, &args.key) {
        webrtc.set_property("signalling-server-cert", cert.as_str());
        webrtc.set_property("signalling-server-key", key.as_str());
    }
    let meta = gstreamer::Structure::builder("meta")
        .field("name", args.name.as_str())
        .build();
    webrtc.set_property("meta", &meta);

    pipeline
        .add_many([appsrc.upcast_ref(), &queue, &convert, &webrtc])
        .context("add elements")?;
    gstreamer::Element::link_many([appsrc.upcast_ref(), &queue, &convert, &webrtc])
        .context("link output pipeline")?;

    Ok((pipeline, appsrc))
}

struct WorkerArgs {
    image: Option<String>,
    test: bool,
    v4l2: Option<String>,
    libcamera: bool,
    no_inference: bool,
    rotate: u32,
    model: String,
    score: f32,
    width: u32,
    height: u32,
    fps: u32,
    all_classes: bool,
}

fn run_worker(args: &mut WorkerArgs, appsrc: &gstreamer_app::AppSrc) -> anyhow::Result<()> {
    let mut detector = if args.no_inference {
        log::info!("inference disabled (--no-inference): publishing frames unannotated");
        None
    } else {
        Some(
            PersonDetector::new(&args.model)
                .with_context(|| format!("load ONNX model {}", args.model))?,
        )
    };

    let pipeline = gstreamer::Pipeline::new();
    let source: gstreamer::Element = if args.libcamera {
        gstreamer::ElementFactory::make("libcamerasrc")
            .build()
            .context("libcamerasrc (is the GStreamer libcamera plugin available?)")?
    } else if let Some(device) = &args.v4l2 {
        gstreamer::ElementFactory::make("v4l2src")
            .property("device", device.as_str())
            .build()
            .with_context(|| format!("v4l2src {device}"))?
    } else if args.test {
        gstreamer::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .property_from_str("pattern", "ball")
            .build()
            .context("videotestsrc")?
    } else {
        // main() rejects this combination before the pipeline is built. The
        // check is repeated rather than unwrapped so a second caller of
        // run_worker cannot reintroduce a panic here.
        let image = args.image.as_deref().ok_or_else(missing_source_error)?;
        let caps = gstreamer::Caps::builder("image/jpeg")
            .field("framerate", gstreamer::Fraction::new(args.fps as i32, 1))
            .build();
        gstreamer::ElementFactory::make("multifilesrc")
            .property("location", image)
            .property("loop", true)
            .property("caps", &caps)
            .build()
            .with_context(|| format!("multifilesrc for {image}"))?
    };

    let decoder = if args.test || args.v4l2.is_some() || args.libcamera {
        None
    } else {
        Some(
            gstreamer::ElementFactory::make("jpegdec")
                .build()
                .context("jpegdec")?,
        )
    };
    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("cap-convert")
        .build()
        .context("videoconvert")?;
    let flip = match args.rotate {
        0 => None,
        90 | 180 | 270 => {
            let method = match args.rotate {
                90 => "clockwise",
                180 => "rotate-180",
                _ => "counterclockwise",
            };
            Some(
                gstreamer::ElementFactory::make("videoflip")
                    .property_from_str("method", method)
                    .build()
                    .with_context(|| format!("videoflip for --rotate {}", args.rotate))?,
            )
        }
        other => return Err(anyhow!("--rotate must be 0, 90, 180 or 270 (got {other})")),
    };
    let scale = gstreamer::ElementFactory::make("videoscale")
        .build()
        .context("videoscale")?;
    let caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", args.width as i32)
        .field("height", args.height as i32)
        .field("framerate", gstreamer::Fraction::new(args.fps as i32, 1))
        .build();
    let capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()
        .context("capsfilter")?;
    let sink = gstreamer::ElementFactory::make("appsink")
        .property("max-buffers", 1u32)
        .property("drop", true)
        // sync=true paces the non-live image loop at the requested framerate.
        .property("sync", true)
        .build()
        .context("appsink")?
        .downcast::<gstreamer_app::AppSink>()
        .map_err(|_| anyhow!("element 'appsink' is not an AppSink"))?;

    let mut elements: Vec<gstreamer::Element> = vec![source.clone()];
    if args.libcamera {
        // Ask libcamera for the stream size up front: the sensor's default
        // mode is far larger than the inference input, and the ISP's scaler
        // is much cheaper than doing it later in videoscale. The format must
        // be a processed one (`RGB`), otherwise the element hands back raw
        // Bayer and `videoconvert` cannot negotiate.
        let caps = gstreamer::Caps::builder("video/x-raw")
            .field("format", "RGB")
            .field("width", args.width as i32)
            .field("height", args.height as i32)
            .field("framerate", gstreamer::Fraction::new(args.fps as i32, 1))
            .build();
        elements.push(
            gstreamer::ElementFactory::make("capsfilter")
                .property("caps", &caps)
                .build()
                .context("libcamera capsfilter")?,
        );
    }
    if args.v4l2.is_some() {
        // Force a raw format out of v4l2src: the C920 happily negotiates MJPG,
        // which videoconvert cannot decode. A bare video/x-raw capsfilter makes
        // v4l2src pick YUYV instead.
        let raw = gstreamer::ElementFactory::make("capsfilter")
            .property("caps", gstreamer::Caps::builder("video/x-raw").build())
            .build()
            .context("raw capsfilter")?;
        elements.push(raw);
    }
    elements.extend(decoder.clone());
    elements.push(convert.clone());
    elements.extend(flip.clone());
    elements.push(scale.clone());
    elements.push(capsfilter.clone());
    elements.push(sink.clone().upcast());
    pipeline
        .add_many(elements.iter().collect::<Vec<_>>())
        .context("add capture elements")?;
    gstreamer::Element::link_many(elements.iter().collect::<Vec<_>>())
        .context("link capture pipeline")?;

    pipeline
        .set_state(gstreamer::State::Playing)
        .context("capture pipeline Playing")?;

    let wanted: Option<Vec<i64>> = if args.all_classes {
        None
    } else {
        Some(vec![COCO_CAT])
    };

    // Inference is seconds per frame on a Raspberry Pi while capture runs at
    // camera rate. Running it inline would throttle the WebRTC stream to the
    // model's rate, so frames go to an inference thread and the render loop
    // keeps drawing the latest boxes: the stream stays smooth and the boxes
    // lag by one inference.
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
    let boxes = std::sync::Arc::new(std::sync::Mutex::new(
        Vec::<kataglyphis_core::Detection>::new(),
    ));
    let infer_boxes = boxes.clone();
    let (infer_width, infer_height, infer_score) = (args.width, args.height, args.score);
    let infer_thread = match detector.take() {
        Some(mut detector) => Some(
            thread::Builder::new()
                .name("cat-infer".into())
                .spawn(move || {
                    let mut runs: u64 = 0;
                    while let Ok(rgba) = frame_rx.recv() {
                        match detector.infer_rgba(
                            &rgba,
                            infer_width,
                            infer_height,
                            infer_score,
                            wanted.as_deref(),
                        ) {
                            Ok(detections) => {
                                runs += 1;
                                if let Some(first) = detections.first() {
                                    log::info!(
                                        "inference {runs}: {} detection(s), first class={} score={:.2} box=({:.0},{:.0})-({:.0},{:.0})",
                                        detections.len(),
                                        first.class_id,
                                        first.score,
                                        first.x1,
                                        first.y1,
                                        first.x2,
                                        first.y2,
                                    );
                                }
                                if let Ok(mut current) = infer_boxes.lock() {
                                    *current = detections;
                                }
                            }
                            Err(err) => log::warn!("inference failed: {err:#}"),
                        }
                    }
                })
                .context("spawn inference thread")?,
        ),
        None => None,
    };

    loop {
        let sample = match sink.pull_sample() {
            Ok(sample) => sample,
            Err(err) => {
                log::warn!("appsink pull failed (stream ended?): {err}");
                break;
            }
        };
        let Some(buffer) = sample.buffer() else {
            continue;
        };
        let Ok(map) = buffer.map_readable() else {
            continue;
        };
        let rgba = map.as_slice();

        let mut annotated = rgba.to_vec();
        if let Ok(current) = boxes.lock() {
            for detection in current.iter() {
                draw_rect(
                    &mut annotated,
                    args.width,
                    args.height,
                    [detection.x1, detection.y1, detection.x2, detection.y2],
                    [0, 255, 0, 255],
                    4,
                );
            }
        }

        // Queue the freshest frame only while the inference thread is idle: a
        // frame of lag is the point, a backlog is not.
        if infer_thread.is_some() {
            let _ = frame_tx.try_send(rgba.to_vec());
        }

        let buffer = gstreamer::Buffer::from_slice(annotated);
        appsrc
            .push_buffer(buffer)
            .map_err(|err| anyhow!("appsrc push: {err:?}"))?;
    }

    drop(frame_tx);
    if let Some(infer_thread) = infer_thread {
        let _ = infer_thread.join();
    }
    let _ = pipeline.set_state(gstreamer::State::Null);
    Ok(())
}

fn draw_rect(
    frame: &mut [u8],
    width: u32,
    height: u32,
    rect: [f32; 4],
    color: [u8; 4],
    thickness: i32,
) {
    let w = width as i32;
    let h = height as i32;
    let [x1, y1, x2, y2] = rect;
    let clamp = |v: f32, max: i32| v.round().max(0.0).min((max - 1) as f32) as i32;
    let (x1, y1, x2, y2) = (clamp(x1, w), clamp(y1, h), clamp(x2, w), clamp(y2, h));
    for t in 0..thickness {
        for x in x1..=x2 {
            put(frame, w, x, (y1 + t).min(h - 1), color);
            put(frame, w, x, (y2 - t).max(0), color);
        }
        for y in y1..=y2 {
            put(frame, w, (x1 + t).min(w - 1), y, color);
            put(frame, w, (x2 - t).max(0), y, color);
        }
    }
}

fn put(frame: &mut [u8], width: i32, x: i32, y: i32, color: [u8; 4]) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 4 <= frame.len() {
        frame[idx..idx + 4].copy_from_slice(&color);
    }
}
