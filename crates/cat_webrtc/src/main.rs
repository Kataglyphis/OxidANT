//! Cat cam service: picks the camera, burns YOLO cat boxes into the frames, streams them over
//! WebRTC and serves the web page that plays the stream. One process, so one service unit.

mod camera;
mod capture;
mod config;
mod output;
mod status;
mod web;

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use gstreamer as gst;
use gstreamer::prelude::*;
use kataglyphis_inference::person_detection::{resolve_model_path, PersonDetector};

use crate::config::{Config, Inference};

/// Environment variable naming the still image looped as the demo source.
const IMAGE_ENV: &str = "KATAGLYPHIS_CAT_IMAGE";

/// Environment variable naming the Flutter web build to serve.
const WEB_ROOT_ENV: &str = "KATAGLYPHIS_WEB_ROOT";

/// webrtcsink's signalling server logs every SDP and ICE message unless this says otherwise.
const SIGNALLING_LOG_ENV: &str = "WEBRTCSINK_SIGNALLING_SERVER_LOG";

/// `inference = "auto"` skips the model on boards with less memory than this (a Pi Zero 2 W has 512 MB).
const AUTO_INFERENCE_MIN_MIB: u64 = 1024;

#[derive(Parser, Debug)]
#[command(
    name = "kataglyphis_cat_webrtc",
    about = "Cat cam: camera -> YOLO cat boxes -> WebRTC, plus the web page that plays it"
)]
struct Args {
    /// TOML config (default: $KATAGLYPHIS_CATCAM_CONFIG, then /etc/omni-accelerant/catcam.toml).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Print the effective config as TOML and exit.
    #[arg(long)]
    print_config: bool,
    /// auto, rpicam, libcamera, v4l2[:DEVICE], mf, ks, test or image:FILE.
    #[arg(long)]
    camera: Option<String>,
    /// Still image to loop (else $KATAGLYPHIS_CAT_IMAGE); a camera flag overrides it.
    #[arg(long)]
    image: Option<String>,
    /// Same as --camera test.
    #[arg(long)]
    test: bool,
    /// Same as --camera v4l2:DEVICE.
    #[arg(long)]
    v4l2: Option<String>,
    /// Same as --camera libcamera.
    #[arg(long)]
    libcamera: bool,
    /// auto, on or off.
    #[arg(long, value_enum)]
    inference: Option<Inference>,
    /// Same as --inference off.
    #[arg(long)]
    no_inference: bool,
    /// Rotate the stream by 0, 90, 180 or 270 degrees.
    #[arg(long)]
    rotate: Option<u32>,
    /// ONNX model (default: $KATAGLYPHIS_ONNX_MODEL, then the one beside the binary).
    #[arg(long)]
    model: Option<String>,
    /// Detection score threshold.
    #[arg(long)]
    score: Option<f32>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    fps: Option<u32>,
    /// Keep every class instead of only cats.
    #[arg(long)]
    all_classes: bool,
    /// Producer name shown to viewers.
    #[arg(long)]
    name: Option<String>,
    /// Port of the built-in signalling server.
    #[arg(long, alias = "signalling-port")]
    listen_port: Option<u16>,
    /// Address the signalling server listens on (default 127.0.0.1: the web server proxies to it).
    #[arg(long)]
    signalling_host: Option<String>,
    /// TLS certificate (PEM) for the signalling server itself.
    #[arg(long)]
    cert: Option<String>,
    /// TLS private key (PEM) matching --cert.
    #[arg(long)]
    key: Option<String>,
    /// Address the web server listens on.
    #[arg(long)]
    http_host: Option<String>,
    /// Port of the web server; 0 turns it off.
    #[arg(long)]
    http_port: Option<u16>,
    /// The Flutter web build to serve (default: $KATAGLYPHIS_WEB_ROOT, then web/ beside the binary).
    #[arg(long)]
    web_root: Option<PathBuf>,
    /// stun://host:port; an empty value keeps the stream LAN-only.
    #[arg(long)]
    stun_server: Option<String>,
    /// UDP range for WebRTC media as MIN-MAX; 0-0 leaves the ports free.
    #[arg(long)]
    ice_ports: Option<String>,
}

fn main() -> ExitCode {
    if std::env::var_os(SIGNALLING_LOG_ENV).is_none() {
        // Still single-threaded here, so changing the environment cannot race a reader.
        std::env::set_var(SIGNALLING_LOG_ENV, "warn");
    }
    kataglyphis_core::logging::init_logger();
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> anyhow::Result<()> {
    let (mut config, source) = Config::load(args.config.as_deref())?;
    apply_args(&args, &mut config)?;
    config.validate()?;
    if args.print_config {
        print!("{}", toml::to_string(&config)?);
        return Ok(());
    }
    match &source {
        Some(path) => log::info!("config: {}", path.display()),
        None => log::info!("config: built-in defaults"),
    }
    let choice = camera::Choice::parse(&config.camera)?;
    kataglyphis_media::ensure_gst_initialized()?;

    let detector = load_detector(&config)?;
    let status = Arc::new(status::Status::new(&config.name, detector.is_some()));
    let (pipeline, appsrc) = output::build(&config)?;
    pipeline
        .set_state(gst::State::Playing)
        .context("output pipeline to PLAYING")?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("cat-web")
        .enable_all()
        .build()
        .context("tokio runtime")?;
    if config.http_port != 0 {
        start_web(&runtime, &config, status.clone())?;
    }
    let for_signals = pipeline.clone();
    runtime.spawn(async move {
        shutdown_signal().await;
        log::info!("shutting down");
        output::request_shutdown(&for_signals);
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let worker = capture::Worker {
        config: Arc::new(config),
        choice,
        appsrc,
        detector,
        status,
        shutdown: shutdown.clone(),
    }
    .spawn()
    .context("spawn capture worker")?;

    let outcome = output::run(&pipeline);
    shutdown.store(true, Ordering::Relaxed);
    let _ = pipeline.set_state(gst::State::Null);
    let _ = worker.join();
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    outcome
}

/// Flags override the config file; the camera flags keep their old meaning and precedence.
fn apply_args(args: &Args, config: &mut Config) -> anyhow::Result<()> {
    let live = if args.libcamera {
        Some("libcamera".to_owned())
    } else if let Some(device) = &args.v4l2 {
        Some(format!("v4l2:{device}"))
    } else if args.test {
        Some("test".to_owned())
    } else {
        None
    };
    let image = args
        .image
        .clone()
        .or_else(|| std::env::var(IMAGE_ENV).ok())
        .filter(|value| !value.trim().is_empty())
        .map(|path| format!("image:{path}"));
    if let Some(camera) = args.camera.clone().or(live).or(image) {
        config.camera = camera;
    }
    if args.no_inference {
        config.inference = Inference::Off;
    } else if let Some(mode) = args.inference {
        config.inference = mode;
    }
    if args.all_classes {
        config.all_classes = true;
    }
    macro_rules! copied {
        ($($field:ident),*) => { $( if let Some(value) = args.$field { config.$field = value; } )* };
    }
    macro_rules! cloned {
        ($($field:ident),*) => { $( if let Some(value) = &args.$field { config.$field.clone_from(value); } )* };
    }
    copied!(rotate, score, width, height, fps, http_port);
    cloned!(name, signalling_host, http_host);
    if let Some(port) = args.listen_port {
        config.signalling_port = port;
    }
    if args.model.is_some() {
        config.model.clone_from(&args.model);
    }
    if args.cert.is_some() || args.key.is_some() {
        config.cert.clone_from(&args.cert);
        config.key.clone_from(&args.key);
    }
    if args.web_root.is_some() {
        config.web_root.clone_from(&args.web_root);
    }
    if args.stun_server.is_some() {
        config.stun_server.clone_from(&args.stun_server);
    }
    if let Some(range) = &args.ice_ports {
        let (min, max) = range
            .split_once('-')
            .with_context(|| format!("--ice-ports wants MIN-MAX (got {range:?})"))?;
        config.ice_port_min = min.trim().parse().context("--ice-ports MIN")?;
        config.ice_port_max = max.trim().parse().context("--ice-ports MAX")?;
    }
    Ok(())
}

/// `None` when inference is off, or when `auto` finds a small board or a model that does not load.
fn load_detector(config: &Config) -> anyhow::Result<Option<PersonDetector>> {
    let open = || {
        let path = resolve_model_path(config.model.as_deref());
        log::info!("model: {path}");
        PersonDetector::new(&path).with_context(|| format!("load ONNX model {path}"))
    };
    match config.inference {
        Inference::Off => {
            log::info!("inference off: streaming frames without boxes");
            Ok(None)
        }
        Inference::On => open().map(Some),
        Inference::Auto => {
            let mib = total_memory_mib();
            if mib < AUTO_INFERENCE_MIN_MIB {
                log::info!(
                    "inference off: {mib} MiB RAM is below the {AUTO_INFERENCE_MIN_MIB} MiB auto needs"
                );
                return Ok(None);
            }
            match open() {
                Ok(detector) => Ok(Some(detector)),
                Err(err) => {
                    log::warn!("inference off: {err:#}");
                    Ok(None)
                }
            }
        }
    }
}

fn total_memory_mib() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory() / (1024 * 1024)
}

fn start_web(
    runtime: &tokio::runtime::Runtime,
    config: &Config,
    status: Arc<status::Status>,
) -> anyhow::Result<()> {
    let web_root = web_root(config);
    match &web_root {
        Some(root) if !root.join("index.html").is_file() => {
            log::warn!("no index.html in {}: the page will 404", root.display());
        }
        Some(root) => log::info!("web: serving {}", root.display()),
        None => log::warn!(
            "web: no web build (set web_root or ${WEB_ROOT_ENV}); only /webrtc-ws and /healthz answer"
        ),
    }
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind((
            config.http_host.as_str(),
            config.http_port,
        )))
        .with_context(|| {
            format!(
                "bind the web server to {}:{}",
                config.http_host, config.http_port
            )
        })?;
    log::info!(
        "web: http://{}:{}/ (proxying {} to the signalling server on port {})",
        display_host(&config.http_host),
        config.http_port,
        web::SIGNALLING_PATH,
        config.signalling_port
    );
    let site = Arc::new(web::Site {
        web_root,
        signalling: (
            loopback_for(&config.signalling_host),
            config.signalling_port,
        ),
        status,
    });
    runtime.spawn(web::serve(listener, site));
    if config.cert.is_some() {
        log::warn!(
            "the signalling server speaks TLS, so {} cannot proxy to it",
            web::SIGNALLING_PATH
        );
    }
    Ok(())
}

/// The configured web root, else `$KATAGLYPHIS_WEB_ROOT`, else `web/` beside the binary.
fn web_root(config: &Config) -> Option<PathBuf> {
    config
        .web_root
        .clone()
        .or_else(|| std::env::var_os(WEB_ROOT_ENV).map(PathBuf::from))
        .or_else(|| {
            let beside = std::env::current_exe().ok()?.parent()?.join("web");
            beside.join("index.html").is_file().then_some(beside)
        })
}

/// A wildcard listen address is reached through loopback.
fn loopback_for(host: &str) -> String {
    match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_unspecified() => Ipv4Addr::LOCALHOST.to_string(),
        _ => host.to_owned(),
    }
}

fn display_host(host: &str) -> &str {
    match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_unspecified() => "<this-host>",
        _ => host,
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(err) => {
                log::warn!("no SIGTERM handler ({err}); only Ctrl-C stops the service cleanly");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::try_parse_from(std::iter::once("cat").chain(list.iter().copied())).unwrap()
    }

    fn applied(list: &[&str]) -> Config {
        let mut config = Config::default();
        apply_args(&args(list), &mut config).unwrap();
        config
    }

    #[test]
    fn the_pi_runner_flags_keep_working() {
        let config = applied(&[
            "--libcamera",
            "--listen-port",
            "8443",
            "--name",
            "Pi Cam",
            "--width",
            "1280",
            "--height",
            "720",
            "--fps",
            "15",
            "--rotate",
            "180",
        ]);
        assert_eq!(config.camera, "libcamera");
        assert_eq!(config.signalling_port, 8443);
        assert_eq!(config.name, "Pi Cam");
        assert_eq!((config.width, config.height, config.fps), (1280, 720, 15));
        assert_eq!(config.rotate, 180);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn camera_flags_override_the_still_image() {
        assert_eq!(applied(&["--image", "cat.jpg", "--test"]).camera, "test");
        assert_eq!(applied(&["--image", "cat.jpg"]).camera, "image:cat.jpg");
        assert_eq!(
            applied(&["--v4l2", "/dev/video2", "--camera", "rpicam"]).camera,
            "rpicam"
        );
    }

    #[test]
    fn no_inference_wins_and_ice_ports_parse() {
        let config = applied(&[
            "--inference",
            "on",
            "--no-inference",
            "--ice-ports",
            "50000-50010",
        ]);
        assert_eq!(config.inference, Inference::Off);
        assert_eq!((config.ice_port_min, config.ice_port_max), (50000, 50010));
        let mut config = Config::default();
        assert!(apply_args(&args(&["--ice-ports", "50000"]), &mut config).is_err());
    }

    #[test]
    fn a_wildcard_signalling_address_is_reached_through_loopback() {
        assert_eq!(loopback_for("0.0.0.0"), "127.0.0.1");
        assert_eq!(loopback_for("::"), "127.0.0.1");
        assert_eq!(loopback_for("192.168.188.98"), "192.168.188.98");
    }
}
