//! Cat cam service: picks the camera, burns YOLO cat boxes into the frames, streams them over
//! WebRTC and serves the web page that plays the stream. One process, so one service unit.

mod camera;
mod capture;
mod clip;
mod config;
mod frame;
mod install;
mod motion;
mod notify;
mod output;
mod precompress;
mod presence;
mod static_files;
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

/// ORT's idle workers spin-wait for the next run unless this is `0`; between capped runs that is waste.
const ORT_SPIN_ENV: &str = "KATAGLYPHIS_ORT_SPIN";

/// ORT's intra-op thread count, read once when the session is built.
const ORT_THREADS_ENV: &str = "KATAGLYPHIS_ORT_THREADS";

/// systemd's `StateDirectory=` for the unit; the cat's visits are kept there across restarts.
const STATE_DIR_ENV: &str = "STATE_DIRECTORY";

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
    /// Detections per second at most (default 2); 0 runs the model on every frame it can take.
    #[arg(long)]
    inference_fps: Option<f32>,
    /// ONNX Runtime threads for the model (default 2); 0 takes one per core.
    #[arg(long)]
    inference_threads: Option<u32>,
    /// The model's rate while nothing moves and no cat is in view (default 0.2).
    #[arg(long)]
    idle_inference_fps: Option<f32>,
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
    /// An http(s) URL to POST a JSON event to when a cat arrives or leaves.
    #[arg(long)]
    notify_url: Option<String>,
    /// UDP range for WebRTC media as MIN-MAX; 0-0 leaves the ports free.
    #[arg(long)]
    ice_ports: Option<String>,
    /// Write a zopfli `<file>.gz` beside each file of this web build the server gzips, and exit.
    #[arg(long, value_name = "DIR")]
    precompress: Option<PathBuf>,
}

fn main() -> ExitCode {
    // Still single-threaded here, so changing the environment cannot race a reader.
    install::adopt();
    if std::env::var_os(SIGNALLING_LOG_ENV).is_none() {
        std::env::set_var(SIGNALLING_LOG_ENV, "warn");
    }
    if std::env::var_os(ORT_SPIN_ENV).is_none() {
        std::env::set_var(ORT_SPIN_ENV, "0");
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
    if let Some(dir) = &args.precompress {
        let done = precompress::precompress(dir)?;
        println!(
            "precompressed {} files in {}: {} -> {} bytes",
            done.files,
            dir.display(),
            done.raw_bytes,
            done.gz_bytes
        );
        return Ok(());
    }
    let (mut config, source) = Config::load(args.config.as_deref())?;
    apply_args(&args, &mut config)?;
    config.validate()?;
    if args.print_config {
        print!("{}", toml::to_string(&config)?);
        return Ok(());
    }
    // Still single-threaded here, so changing the environment cannot race a reader.
    std::env::set_var(ORT_THREADS_ENV, config.inference_threads.to_string());
    match &source {
        Some(path) => log::info!("config: {} (cat cam {})", path.display(), status::VERSION),
        None => log::info!("config: built-in defaults (cat cam {})", status::VERSION),
    }
    let choice = camera::Choice::parse(&config.camera)?;
    kataglyphis_media::ensure_gst_initialized()?;

    let detector = load_detector(&config)?;
    let status = status::Status::new(
        &config.name,
        detector.is_some(),
        config.width,
        config.height,
    );
    let status = status
        .rotated(config.rotate)
        .scored(config.score)
        .at_fps(config.fps)
        .peered(config.peers.clone());
    let status = Arc::new(match state_dir() {
        Some(dir) => {
            log::info!("state: {}", dir.display());
            status.remember_in(dir)
        }
        None => status,
    });
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
    let notifier = start_notifier(&config, detector.is_some(), status.clone());
    // Only the model sees arrivals, so without it there is nothing to record.
    let clip = if detector.is_some() {
        clip::Clip::spawn(
            config.clip_seconds,
            (config.width, config.height, config.fps),
            status.clone(),
            notifier.clone(),
        )
    } else {
        None
    };
    let at_exit = status.clone();
    let worker = capture::Worker {
        config: Arc::new(config),
        choice,
        appsrc,
        detector,
        status,
        shutdown: shutdown.clone(),
        notifier,
        clip,
    }
    .spawn()
    .context("spawn capture worker")?;

    let outcome = output::run(&pipeline);
    shutdown.store(true, Ordering::Relaxed);
    let _ = pipeline.set_state(gst::State::Null);
    let _ = worker.join();
    // The last sighting before a stop, so the next start closes the visit the stop cut short there.
    at_exit.persist();
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
    copied!(
        rotate,
        score,
        inference_fps,
        idle_inference_fps,
        inference_threads,
        width,
        height,
        fps,
        http_port
    );
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
    if args.notify_url.is_some() {
        config.notify_url.clone_from(&args.notify_url);
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

/// The webhook's delivery thread when `notify_url` is set; logs name only the URL's origin.
fn start_notifier(
    config: &Config,
    inference: bool,
    status: Arc<status::Status>,
) -> Option<notify::Notifier> {
    let url = config.notify()?;
    if !inference {
        log::warn!("notify_url is set, but with the model off no cat is ever seen");
        return None;
    }
    match notify::Notifier::spawn(url, status) {
        Ok(notifier) => {
            log::info!("notify: cat events go to {}", notify::origin(url));
            Some(notifier)
        }
        Err(err) => {
            log::warn!("notify: no delivery thread: {err}");
            None
        }
    }
}

/// The first of `$STATE_DIRECTORY`'s colon-separated directories, if systemd set one.
fn state_dir() -> Option<PathBuf> {
    first_dir(&std::env::var(STATE_DIR_ENV).ok()?)
}

fn first_dir(list: &str) -> Option<PathBuf> {
    let first = list.split(':').next()?.trim();
    (!first.is_empty()).then(|| PathBuf::from(first))
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
            "web: no web build (set web_root or ${WEB_ROOT_ENV}); only /webrtc-ws, /healthz and /snapshot.jpg answer"
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
        gzip: static_files::GzipCache::default(),
        allowed_hosts: config.allowed_hosts.clone(),
        max_viewers: config.max_viewers as usize,
        peers: config.peers.clone(),
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
            "--inference-threads",
            "1",
        ]);
        assert_eq!(config.inference, Inference::Off);
        assert_eq!(config.inference_threads, 1);
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

    #[test]
    fn every_other_flag_reaches_the_config() {
        let config = applied(&[
            "--all-classes",
            "--inference",
            "auto",
            "--model",
            "cat.onnx",
            "--score",
            "0.6",
            "--inference-fps",
            "1",
            "--idle-inference-fps",
            "0.1",
            "--cert",
            "cert.pem",
            "--key",
            "key.pem",
            "--signalling-host",
            "0.0.0.0",
            "--http-host",
            "0.0.0.0",
            "--http-port",
            "9090",
            "--web-root",
            "/srv/web",
            "--stun-server",
            "stun://stun.example:3478",
            "--notify-url",
            "http://ha.local:8123/api/webhook/catcam",
        ]);
        assert!(config.all_classes);
        assert_eq!(config.inference, Inference::Auto);
        assert_eq!(config.model.as_deref(), Some("cat.onnx"));
        assert_eq!(
            (
                config.score,
                config.inference_fps,
                config.idle_inference_fps
            ),
            (0.6, 1.0, 0.1)
        );
        assert_eq!(
            (config.cert.as_deref(), config.key.as_deref()),
            (Some("cert.pem"), Some("key.pem"))
        );
        assert_eq!(
            (config.signalling_host.as_str(), config.http_host.as_str()),
            ("0.0.0.0", "0.0.0.0")
        );
        assert_eq!(config.http_port, 9090);
        assert_eq!(
            config.web_root.as_deref(),
            Some(std::path::Path::new("/srv/web"))
        );
        assert_eq!(
            config.stun_server.as_deref(),
            Some("stun://stun.example:3478")
        );
        assert_eq!(
            config.notify(),
            Some("http://ha.local:8123/api/webhook/catcam")
        );
        // The config file's web root wins over the one beside the binary.
        assert_eq!(web_root(&config), Some(PathBuf::from("/srv/web")));
    }

    #[test]
    fn a_wildcard_address_is_shown_as_this_host() {
        assert_eq!(display_host("0.0.0.0"), "<this-host>");
        assert_eq!(display_host("::"), "<this-host>");
        assert_eq!(display_host("192.168.188.98"), "192.168.188.98");
        assert_eq!(display_host("catcam.local"), "catcam.local");
    }

    #[test]
    fn with_the_model_off_there_is_no_detector_and_no_notifier() {
        let mut config = applied(&["--no-inference", "--notify-url", "http://ha.local/hook"]);
        assert!(load_detector(&config).unwrap().is_none());
        let status = Arc::new(status::Status::new("Cam", false, 4, 4));
        // Without the model no cat is ever seen, so no delivery thread is started.
        assert!(start_notifier(&config, false, status.clone()).is_none());
        config.notify_url = None;
        assert!(start_notifier(&config, true, status).is_none());
    }

    #[test]
    fn a_missing_model_stops_on_and_turns_auto_off() {
        let mut config = applied(&["--inference", "on"]);
        config.model = Some("/nonexistent/yolo.onnx".into());
        let err = load_detector(&config)
            .err()
            .expect("on must fail without its model");
        assert!(
            format!("{err:#}").contains("/nonexistent/yolo.onnx"),
            "{err:#}"
        );
        config.inference = Inference::Auto;
        assert!(load_detector(&config).unwrap().is_none());
    }

    #[test]
    fn a_notify_url_with_the_model_on_starts_the_delivery_thread() {
        let config = applied(&["--notify-url", "http://ha.local/api/webhook/x"]);
        let status = Arc::new(status::Status::new("Cam", true, 4, 4));
        assert!(start_notifier(&config, true, status).is_some());
    }

    #[test]
    fn systemd_names_the_state_directory_first() {
        assert_eq!(
            first_dir("/var/lib/cat:/var/lib/other"),
            Some(PathBuf::from("/var/lib/cat"))
        );
        assert_eq!(
            first_dir(" /var/lib/cat "),
            Some(PathBuf::from("/var/lib/cat"))
        );
        assert_eq!(first_dir(""), None);
        assert_eq!(first_dir(":/var/lib/other"), None);
    }

    #[test]
    fn the_web_server_answers_on_the_configured_address() {
        use std::io::{Read as _, Write as _};
        let web = tempfile::tempdir().unwrap();
        std::fs::write(
            web.path().join("index.html"),
            "<!doctype html><title>cat</title>",
        )
        .unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut config = applied(&["--http-host", "127.0.0.1", "--http-port", &port.to_string()]);
        config.web_root = Some(web.path().to_path_buf());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let status = Arc::new(status::Status::new("Web Cam", false, 4, 4));
        start_web(&runtime, &config, status).unwrap();
        let get = |path: &str| {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                stream,
                "GET {path} HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\n\r\n"
            )
            .unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        let health = get("/healthz");
        assert!(
            health.starts_with("HTTP/1.1 200") && health.contains("\"name\":\"Web Cam\""),
            "{health}"
        );
        assert!(get("/").contains("<title>cat</title>"));
        // The port is taken now, so a second server on it fails and says where.
        let status = Arc::new(status::Status::new("Second", false, 4, 4));
        let err = start_web(&runtime, &config, status).unwrap_err();
        assert!(
            format!("{err:#}").contains(&format!("127.0.0.1:{port}")),
            "{err:#}"
        );
    }
}
