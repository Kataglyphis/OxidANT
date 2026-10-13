//! The built-in web server: the Flutter web build, `/healthz`, and `/webrtc-ws` forwarded to the
//! signalling server, so one origin serves the page and its WebSocket and no nginx is needed.

use std::convert::Infallible;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context as _};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use crate::static_files::{static_file, GzipCache};
use crate::status::{Settings, Status, Viewer};

/// What the web client's host-relative `signalingServerUrl` resolves to.
pub const SIGNALLING_PATH: &str = "/webrtc-ws";

/// What the cat cam runs and what it saw, as JSON.
pub const HEALTHZ_PATH: &str = "/healthz";

/// The latest annotated frame as a JPEG.
pub const SNAPSHOT_PATH: &str = "/snapshot.jpg";

/// The frame the last cat arrival was confirmed on, as a JPEG.
pub const ARRIVAL_PATH: &str = "/arrival.jpg";
/// The last arrival as an H.264 clip, from a few seconds before it.
pub const CLIP_PATH: &str = "/arrival.mp4";
/// `GET` the settings the page can change; `POST` any of them as JSON, e.g. `{"rotate": 90}`.
pub const SETTINGS_PATH: &str = "/settings";

/// How long the signalling server may take to answer the WebSocket handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type Body = Full<Bytes>;

pub struct Site {
    pub web_root: Option<PathBuf>,
    /// Where `/webrtc-ws` goes: the signalling server's host and port.
    pub signalling: (String, u16),
    pub status: Arc<Status>,
    pub gzip: GzipCache,
    /// Host names besides IPs and local names that may open the stream and change settings.
    pub allowed_hosts: Vec<String>,
    /// Browser tabs on the stream at once; 0 means no limit.
    pub max_viewers: usize,
    /// The other cat cams' origins, whose pages may read this one's status and pictures.
    pub peers: Vec<String>,
}

/// `origin` when it is one of `peers`: a page another cat cam serves, for the overview.
fn peer_origin<'a>(origin: Option<&'a HeaderValue>, peers: &[String]) -> Option<&'a HeaderValue> {
    let text = origin?.to_str().ok()?;
    peers
        .iter()
        .any(|peer| peer.trim_end_matches('/').eq_ignore_ascii_case(text))
        .then_some(origin?)
}

/// Names that only resolve inside a home network; a browser that reached the cat cam under a public
/// name was sent there by DNS rebinding.
const LOCAL_SUFFIXES: [&str; 6] = [
    ".local",
    ".lan",
    ".home",
    ".internal",
    ".home.arpa",
    ".fritz.box",
];

/// `Ok` when a request that acts, the signalling socket or a settings change, comes from this cat
/// cam's own page; a tool such as curl sends no `Origin` and passes.
pub(crate) fn own_page(headers: &HeaderMap, allowed_hosts: &[String]) -> Result<(), &'static str> {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if let Some(host) = host {
        let name = host_name(host);
        let local = name.parse::<IpAddr>().is_ok()
            || !name.contains('.')
            || LOCAL_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
            || allowed_hosts.iter().any(|a| a.eq_ignore_ascii_case(&name));
        if !local {
            return Err("this host name is not the cat cam's own; add it to allowed_hosts");
        }
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        let authority = origin.split_once("://").map_or("", |(_, rest)| rest);
        if host.is_none_or(|host| !authority.eq_ignore_ascii_case(host)) {
            return Err("cross-site request refused");
        }
    }
    Ok(())
}

/// `host` without its port, lowercase; `[::1]:8080` is `::1`.
fn host_name(host: &str) -> String {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.to_ascii_lowercase()
}

/// Serves connections until the runtime shuts down.
pub async fn serve(listener: TcpListener, site: Arc<Site>) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(err) => {
                log::warn!("web server accept failed: {err}");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        let site = site.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |req| handle(req, site.clone()));
            let connection = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades();
            if let Err(err) = connection.await {
                log::debug!("connection from {peer}: {err}");
            }
        });
    }
}

async fn handle(req: Request<Incoming>, site: Arc<Site>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_owned();
    let origin = req.headers().get(header::ORIGIN).cloned();
    let reads = [HEALTHZ_PATH, SNAPSHOT_PATH, ARRIVAL_PATH, CLIP_PATH].contains(&path.as_str());
    let acts = path == SIGNALLING_PATH || (path == SETTINGS_PATH && req.method() == Method::POST);
    let refused = acts
        .then(|| own_page(req.headers(), &site.allowed_hosts).err())
        .flatten();
    let mut response = if let Some(why) = refused {
        log::warn!("{path}: {why}");
        plain(StatusCode::FORBIDDEN, why)
    } else if path == SIGNALLING_PATH {
        match site.status.viewer(site.max_viewers) {
            None => {
                log::warn!(
                    "{SIGNALLING_PATH}: {} viewers already, refused one",
                    site.max_viewers
                );
                plain(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "too many viewers: close another tab, or raise max_viewers",
                )
            }
            Some(viewer) => match forward_upgrade(req, &site.signalling, viewer).await {
                Ok(response) => response,
                Err(err) => {
                    log::warn!("{SIGNALLING_PATH}: {err:#}");
                    plain(StatusCode::BAD_GATEWAY, "signalling server unreachable")
                }
            },
        }
    } else if path == SETTINGS_PATH {
        settings(req, &site.status).await
    } else if req.method() != Method::GET && req.method() != Method::HEAD {
        plain(StatusCode::METHOD_NOT_ALLOWED, "GET or HEAD only")
    } else if path == HEALTHZ_PATH {
        let mut response = Response::new(Full::new(Bytes::from(site.status.to_json())));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    } else if path == SNAPSHOT_PATH {
        snapshot(site.status.clone()).await
    } else if path == ARRIVAL_PATH {
        // ?t= names the arrival, so a notification's picture is never a later, false visit's.
        let at = query_u64(req.uri().query(), "t");
        match (site.status.arrival_jpeg(at), at) {
            (Some(jpeg), _) => jpeg_response(jpeg),
            (None, None) => plain(StatusCode::NOT_FOUND, "no cat has arrived yet"),
            (None, Some(_)) => plain(
                StatusCode::NOT_FOUND,
                "that arrival's picture is no longer kept",
            ),
        }
    } else if path == CLIP_PATH {
        // ?t= names the arrival, so a link in an older notification never plays a newer visit.
        let at = query_u64(req.uri().query(), "t");
        match (site.status.clip_mp4(at), at) {
            (Some(mp4), _) => mp4_response(req.headers(), mp4),
            (None, None) => plain(StatusCode::NOT_FOUND, "no arrival clip yet"),
            (None, Some(_)) => plain(
                StatusCode::NOT_FOUND,
                "that arrival's clip is no longer kept",
            ),
        }
    } else {
        static_file(&req, site.web_root.as_deref(), &site.gzip).await
    };
    let headers = response.headers_mut();
    if let Some(peer) = peer_origin(origin.as_ref(), &site.peers).filter(|_| reads) {
        // Another cat cam's page reads this one for its overview; under COEP its pictures need CORP too.
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, peer.clone());
        headers.insert(
            header::HeaderName::from_static("cross-origin-resource-policy"),
            HeaderValue::from_static("cross-origin"),
        );
    }
    if reads {
        headers.append(header::VARY, HeaderValue::from_static("origin"));
    }
    // Isolation lets Firefox and Safari load the Rust core's shared-memory wasm; the stream needs neither.
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-embedder-policy",
        HeaderValue::from_static("require-corp"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

/// The settings page's endpoint. A change is JSON only: another site's form cannot send it, and a
/// script needs a CORS preflight this server never grants.
async fn settings(req: Request<Incoming>, status: &Status) -> Response<Body> {
    if req.method() == Method::POST {
        let json = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        if !json {
            return plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, "application/json only");
        }
        let Ok(body) = Limited::new(req.into_body(), 1024).collect().await else {
            return plain(StatusCode::BAD_REQUEST, "body too large or unreadable");
        };
        let applied = serde_json::from_slice::<Settings>(&body.to_bytes())
            .map_err(anyhow::Error::from)
            .and_then(|update| status.apply(update));
        if let Err(err) = applied {
            log::warn!("{SETTINGS_PATH}: {err:#}");
            let mut response = Response::new(Full::new(Bytes::from(format!("{err:#}"))));
            *response.status_mut() = StatusCode::BAD_REQUEST;
            return response;
        }
    } else if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain(StatusCode::METHOD_NOT_ALLOWED, "GET or POST only");
    }
    let mut response = Response::new(Full::new(Bytes::from(status.settings_json())));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

pub(crate) fn plain(status: StatusCode, text: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from_static(text.as_bytes())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

async fn snapshot(status: Arc<Status>) -> Response<Body> {
    // Off the runtime's two workers, so a snapshot cannot stall a signalling connection.
    let jpeg = match tokio::task::spawn_blocking(move || status.snapshot_jpeg()).await {
        Ok(Ok(Some(jpeg))) => jpeg,
        Ok(Ok(None)) => return plain(StatusCode::SERVICE_UNAVAILABLE, "no frame yet"),
        Ok(Err(err)) => {
            log::warn!("{SNAPSHOT_PATH}: {err:#}");
            return plain(StatusCode::INTERNAL_SERVER_ERROR, "snapshot failed");
        }
        Err(_) => return plain(StatusCode::INTERNAL_SERVER_ERROR, "snapshot failed"),
    };
    jpeg_response(jpeg)
}

/// The clip, whole or the byte range asked for: Safari plays a `<video>` only from a server that answers ranges.
fn mp4_response(headers: &HeaderMap, mp4: Bytes) -> Response<Body> {
    let len = mp4.len() as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|v| byte_range(v, len));
    let mut response = match range {
        None | Some(Range::Ignored) => Response::new(Full::new(mp4)),
        Some(Range::Satisfiable(start, end)) => {
            let mut partial = Response::new(Full::new(mp4.slice(start as usize..=end as usize)));
            *partial.status_mut() = StatusCode::PARTIAL_CONTENT;
            if let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{len}")) {
                partial.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            partial
        }
        Some(Range::Unsatisfiable) => {
            let mut refused = Response::new(Full::new(Bytes::new()));
            *refused.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{len}")) {
                refused.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            refused
        }
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The number `key=` carries in a query string, if any.
fn query_u64(query: Option<&str>, key: &str) -> Option<u64> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.parse().ok())
}

#[derive(Debug, PartialEq)]
enum Range {
    /// The whole file goes out: not a single `bytes=` range this server serves.
    Ignored,
    /// Inclusive first and last byte.
    Satisfiable(u64, u64),
    Unsatisfiable,
}

/// One `bytes=a-b`, `bytes=a-` or `bytes=-n` range against `len` bytes; several ranges get the whole file.
fn byte_range(value: &str, len: u64) -> Range {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return Range::Ignored;
    };
    if spec.contains(',') {
        return Range::Ignored;
    }
    let Some((first, last)) = spec.split_once('-') else {
        return Range::Ignored;
    };
    let (first, last) = (first.trim(), last.trim());
    let parsed = match (first.is_empty(), last.is_empty()) {
        (true, true) => return Range::Ignored,
        (true, false) => match last.parse::<u64>() {
            Ok(0) => return Range::Unsatisfiable,
            Ok(n) => Some((len.saturating_sub(n), len.saturating_sub(1))),
            Err(_) => None,
        },
        (false, _) => match (
            first.parse::<u64>(),
            if last.is_empty() {
                Ok(u64::MAX)
            } else {
                last.parse::<u64>()
            },
        ) {
            (Ok(a), Ok(b)) if a <= b => Some((a, b.min(len.saturating_sub(1)))),
            _ => None,
        },
    };
    match parsed {
        None => Range::Ignored,
        Some((a, _)) if len == 0 || a >= len => Range::Unsatisfiable,
        Some((a, b)) => Range::Satisfiable(a, b),
    }
}

fn jpeg_response(jpeg: impl Into<Bytes>) -> Response<Body> {
    let mut response = Response::new(Full::new(jpeg.into()));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Forwards a WebSocket upgrade byte for byte: the signalling server does the handshake itself.
async fn forward_upgrade(
    req: Request<Incoming>,
    (host, port): &(String, u16),
    viewer: Viewer,
) -> anyhow::Result<Response<Body>> {
    let mut upstream = TcpStream::connect((host.as_str(), *port))
        .await
        .with_context(|| format!("connect to {host}:{port}"))?;
    let target = req
        .uri()
        .path_and_query()
        .map_or(SIGNALLING_PATH, |pq| pq.as_str());
    let mut head = format!(
        "{} {target} HTTP/1.1\r\nhost: {host}:{port}\r\n",
        req.method()
    );
    for (name, value) in req.headers() {
        if name == header::HOST {
            continue;
        }
        if let Ok(value) = value.to_str() {
            head.push_str(name.as_str());
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
    }
    head.push_str("\r\n");
    upstream.write_all(head.as_bytes()).await?;

    let (status, headers, leftover) =
        tokio::time::timeout(HANDSHAKE_TIMEOUT, read_response_head(&mut upstream))
            .await
            .context("signalling handshake timed out")??;
    let mut builder = Response::builder().status(status);
    for (name, value) in &headers {
        let hop = name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("transfer-encoding");
        if status != StatusCode::SWITCHING_PROTOCOLS && hop {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_slice());
    }
    if status != StatusCode::SWITCHING_PROTOCOLS {
        return Ok(builder.body(Full::new(Bytes::from(leftover)))?);
    }
    tokio::spawn(async move {
        // Held while the tab stays connected.
        let _viewer = viewer;
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let mut client = TokioIo::new(upgraded);
                if !leftover.is_empty() && client.write_all(&leftover).await.is_err() {
                    return;
                }
                if let Err(err) = tokio::io::copy_bidirectional(&mut client, &mut upstream).await {
                    log::debug!("{SIGNALLING_PATH} closed: {err}");
                }
            }
            Err(err) => log::warn!("{SIGNALLING_PATH} upgrade failed: {err}"),
        }
    });
    Ok(builder.body(Full::new(Bytes::new()))?)
}

/// The status, headers and any bytes past the head of the upstream's response.
async fn read_response_head(
    stream: &mut TcpStream,
) -> anyhow::Result<(StatusCode, Vec<(String, Vec<u8>)>, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            bail!("signalling server closed the connection during the handshake");
        }
        buf.extend_from_slice(&chunk[..n]);
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut response = httparse::Response::new(&mut headers);
        if let httparse::Status::Complete(len) = response.parse(&buf)? {
            let status = StatusCode::from_u16(response.code.unwrap_or(502))?;
            let headers = response
                .headers
                .iter()
                .map(|h| (h.name.to_owned(), h.value.to_vec()))
                .collect();
            return Ok((status, headers, buf[len..].to_vec()));
        }
        if buf.len() > 16 * 1024 {
            bail!("signalling response head larger than 16 KiB");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn the_arrival_time_comes_from_the_query_and_nothing_else() {
        assert_eq!(query_u64(Some("t=1791763273"), "t"), Some(1_791_763_273));
        assert_eq!(query_u64(Some("visit=3&t=42"), "t"), Some(42));
        assert_eq!(query_u64(Some("visit=3"), "t"), None);
        assert_eq!(query_u64(Some("t=soon"), "t"), None);
        assert_eq!(query_u64(Some("at=5"), "t"), None);
        assert_eq!(query_u64(None, "t"), None);
    }

    #[test]
    fn a_byte_range_is_served_as_asked_and_clamped_to_the_clip() {
        assert_eq!(byte_range("bytes=0-1", 100), Range::Satisfiable(0, 1));
        assert_eq!(byte_range("bytes=0-", 100), Range::Satisfiable(0, 99));
        assert_eq!(byte_range("bytes=90-500", 100), Range::Satisfiable(90, 99));
        assert_eq!(byte_range("bytes=-10", 100), Range::Satisfiable(90, 99));
        assert_eq!(byte_range("bytes=-500", 100), Range::Satisfiable(0, 99));
    }

    #[test]
    fn a_range_past_the_end_is_refused_and_anything_else_gets_the_whole_clip() {
        assert_eq!(byte_range("bytes=100-", 100), Range::Unsatisfiable);
        assert_eq!(byte_range("bytes=-0", 100), Range::Unsatisfiable);
        assert_eq!(byte_range("bytes=0-1,5-6", 100), Range::Ignored);
        assert_eq!(byte_range("bytes=5-2", 100), Range::Ignored);
        assert_eq!(byte_range("items=0-1", 100), Range::Ignored);
        assert_eq!(byte_range("bytes=x-1", 100), Range::Ignored);
        assert_eq!(byte_range("bytes=5", 100), Range::Ignored);
        assert_eq!(byte_range("bytes=-", 100), Range::Ignored);
        assert_eq!(byte_range("bytes=-x", 100), Range::Ignored);
    }

    #[test]
    fn the_clip_answers_a_range_with_206_and_says_it_takes_ranges() {
        let mp4 = Bytes::from_static(b"0123456789");
        let mut headers = HeaderMap::new();
        let whole = mp4_response(&headers, mp4.clone());
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.headers()[header::ACCEPT_RANGES], "bytes");
        assert_eq!(whole.headers()[header::CONTENT_TYPE], "video/mp4");
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=2-4"));
        let part = mp4_response(&headers, mp4.clone());
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.headers()[header::CONTENT_RANGE], "bytes 2-4/10");
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=10-"));
        let refused = mp4_response(&headers, mp4);
        assert_eq!(refused.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(refused.headers()[header::CONTENT_RANGE], "bytes */10");
    }

    /// One raw HTTP/1.1 exchange; returns the whole response as text.
    async fn get(addr: std::net::SocketAddr, path: &str, extra: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\n{extra}\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8_lossy(&response).into_owned()
    }

    async fn start(web_root: Option<PathBuf>, signalling_port: u16) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let site = Arc::new(Site {
            web_root,
            signalling: ("127.0.0.1".into(), signalling_port),
            status: Arc::new(Status::new("Test Cam", false, 16, 8)),
            gzip: GzipCache::default(),
            allowed_hosts: Vec::new(),
            max_viewers: 0,
            peers: Vec::new(),
        });
        tokio::spawn(serve(listener, site));
        addr
    }

    fn web_build() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>cat cam</html>").unwrap();
        std::fs::write(dir.path().join("main.dart.wasm"), b"\0asm").unwrap();
        std::fs::create_dir(dir.path().join("pkg")).unwrap();
        std::fs::write(dir.path().join("pkg").join("oxidant.js"), "export {}").unwrap();
        dir
    }

    #[tokio::test]
    async fn serves_the_build_with_isolation_headers_and_the_right_types() {
        let build = web_build();
        let addr = start(Some(build.path().to_path_buf()), 9).await;

        let index = get(addr, "/", "").await;
        assert!(index.starts_with("HTTP/1.1 200"), "{index}");
        assert!(index.contains("cross-origin-opener-policy: same-origin"));
        assert!(index.contains("cross-origin-embedder-policy: require-corp"));
        assert!(index.contains("content-type: text/html"));
        assert!(index.ends_with("<html>cat cam</html>"));

        let wasm = get(addr, "/main.dart.wasm", "").await;
        assert!(wasm.contains("content-type: application/wasm"), "{wasm}");
        let module = get(addr, "/pkg/oxidant.js", "").await;
        assert!(module.contains("content-type: text/javascript"), "{module}");
    }

    #[tokio::test]
    async fn app_routes_fall_back_to_the_app_and_missing_assets_do_not() {
        let build = web_build();
        let addr = start(Some(build.path().to_path_buf()), 9).await;
        assert!(get(addr, "/stream", "")
            .await
            .ends_with("<html>cat cam</html>"));
        assert!(get(addr, "/missing.js", "")
            .await
            .starts_with("HTTP/1.1 404"));
        assert!(get(addr, "/../secret", "")
            .await
            .starts_with("HTTP/1.1 400"));
    }

    /// One raw HTTP/1.1 exchange; returns the response head as text and the body as bytes.
    async fn get_raw(addr: std::net::SocketAddr, path: &str, extra: &str) -> (String, Vec<u8>) {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\n{extra}\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&response[..split]).into_owned();
        (head, response[split + 4..].to_vec())
    }

    async fn post(
        addr: std::net::SocketAddr,
        path: &str,
        content_type: &str,
        body: &str,
    ) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "POST {path} HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8_lossy(&response).into_owned()
    }

    fn body_json(response: &str) -> serde_json::Value {
        serde_json::from_str(&response[response.find("\r\n\r\n").unwrap() + 4..]).unwrap()
    }

    #[tokio::test]
    async fn the_settings_page_reads_and_changes_name_rotation_and_score() {
        let addr = start(None, 9).await;
        let before = body_json(&get(addr, "/settings", "").await);
        assert_eq!(before["rotate"], 0);
        assert_eq!(before["name"], "Test Cam");
        // Only what the post names changes.
        let reply = post(
            addr,
            "/settings",
            "application/json",
            r#"{"rotate": 270, "score": 0.5}"#,
        )
        .await;
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        let after = body_json(&reply);
        assert_eq!(
            (after["rotate"].clone(), after["score"].clone()),
            (270.into(), 0.5.into())
        );
        assert_eq!(after["name"], "Test Cam");
        post(
            addr,
            "/settings",
            "application/json",
            r#"{"name": "  Futtertrog  "}"#,
        )
        .await;
        let health = body_json(&get(addr, "/healthz", "").await);
        assert_eq!(health["name"], "Futtertrog");
        assert_eq!(health["rotate"], 270);
        assert_eq!(health["score"], 0.5);
    }

    #[tokio::test]
    async fn settings_take_only_json_and_a_bad_value_changes_nothing() {
        let addr = start(None, 9).await;
        // A form post is what another site could send without a preflight.
        let form = post(
            addr,
            "/settings",
            "application/x-www-form-urlencoded",
            "rotate=90",
        )
        .await;
        assert!(form.starts_with("HTTP/1.1 415"), "{form}");
        // One bad value refuses the whole post, the good one beside it included.
        let mixed = post(
            addr,
            "/settings",
            "application/json",
            r#"{"rotate": 90, "score": 2}"#,
        )
        .await;
        assert!(
            mixed.starts_with("HTTP/1.1 400") && mixed.contains("score"),
            "{mixed}"
        );
        for bad in [
            r#"{"rotate": 45}"#,
            r#"{"name": "   "}"#,
            r#"{"colour": "red"}"#,
            "turn it",
        ] {
            let reply = post(addr, "/settings", "application/json", bad).await;
            assert!(reply.starts_with("HTTP/1.1 400"), "{bad}: {reply}");
        }
        assert!(post(addr, "/settings", "application/json", r#"{}"#)
            .await
            .starts_with("HTTP/1.1 200"));
        let now = body_json(&get(addr, "/settings", "").await);
        assert_eq!(
            (now["rotate"].clone(), now["name"].clone()),
            (0.into(), "Test Cam".into())
        );
    }

    fn etag_of(head: &str) -> String {
        head.lines()
            .find_map(|l| l.strip_prefix("etag: "))
            .expect("etag header")
            .to_owned()
    }

    #[tokio::test]
    async fn big_assets_go_out_gzipped_to_a_client_that_asks_and_plain_to_one_that_does_not() {
        use std::io::Read as _;
        let build = web_build();
        let script = "let cat = 'Thundy';\n".repeat(4000);
        std::fs::write(build.path().join("main.dart.js"), &script).unwrap();
        let addr = start(Some(build.path().to_path_buf()), 9).await;

        let (head, body) = get_raw(
            addr,
            "/main.dart.js",
            "accept-encoding: gzip, deflate, br\r\n",
        )
        .await;
        assert!(head.contains("content-encoding: gzip"), "{head}");
        assert!(head.contains("vary: accept-encoding"), "{head}");
        assert!(etag_of(&head).ends_with("-gz\""), "{head}");
        assert!(body.len() < script.len() / 10, "{} bytes", body.len());
        let mut unzipped = String::new();
        flate2::read::GzDecoder::new(&body[..])
            .read_to_string(&mut unzipped)
            .unwrap();
        assert_eq!(unzipped, script);

        let (head, body) = get_raw(addr, "/main.dart.js", "").await;
        assert!(!head.contains("content-encoding"), "{head}");
        assert_eq!(body, script.as_bytes());
        let (head, _) = get_raw(addr, "/index.html", "accept-encoding: gzip\r\n").await;
        assert!(
            !head.contains("content-encoding"),
            "a 20-byte page went out gzipped: {head}"
        );
    }

    #[tokio::test]
    async fn a_packaged_gz_beside_a_file_is_sent_unless_the_file_is_newer() {
        let build = web_build();
        let script = build.path().join("main.dart.js");
        std::fs::write(&script, "1;".repeat(2000)).unwrap();
        // Not a real gzip of the file: the body proves which one went out.
        std::fs::write(build.path().join("main.dart.js.gz"), b"packaged").unwrap();
        let addr = start(Some(build.path().to_path_buf()), 9).await;
        let gzip = "accept-encoding: gzip\r\n";

        let (head, body) = get_raw(addr, "/main.dart.js", gzip).await;
        assert!(head.contains("content-encoding: gzip"), "{head}");
        assert_eq!(body, b"packaged");
        let (_, body) = get_raw(addr, "/main.dart.js", "").await;
        assert_eq!(body, "1;".repeat(2000).as_bytes());

        // A file edited after packaging is compressed here again rather than sent stale.
        let later = SystemTime::now() + Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&script)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let (_, body) = get_raw(addr, "/main.dart.js", gzip).await;
        assert_ne!(body, b"packaged");
        assert!(body.starts_with(&[0x1f, 0x8b]), "not gzip: {body:?}");
    }

    #[tokio::test]
    async fn a_gzipped_file_revalidates_against_its_own_etag() {
        let build = web_build();
        std::fs::write(build.path().join("main.dart.js"), "1;".repeat(2000)).unwrap();
        let addr = start(Some(build.path().to_path_buf()), 9).await;
        let gzip = "accept-encoding: gzip\r\n";
        let gz_tag = etag_of(&get_raw(addr, "/main.dart.js", gzip).await.0);
        let plain_tag = etag_of(&get_raw(addr, "/main.dart.js", "").await.0);
        assert_ne!(gz_tag, plain_tag);
        let revalidate = |tag: &str| format!("{gzip}if-none-match: {tag}\r\n");
        let (head, body) = get_raw(addr, "/main.dart.js", &revalidate(&gz_tag)).await;
        assert!(head.starts_with("HTTP/1.1 304"), "{head}");
        assert!(body.is_empty());
        let (head, _) = get_raw(addr, "/main.dart.js", &revalidate(&plain_tag)).await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    }

    #[tokio::test]
    async fn an_unchanged_file_revalidates_as_304() {
        let build = web_build();
        let addr = start(Some(build.path().to_path_buf()), 9).await;
        let first = get(addr, "/index.html", "").await;
        let etag = first
            .lines()
            .find_map(|l| l.strip_prefix("etag: "))
            .expect("etag header")
            .to_owned();
        let again = get(addr, "/index.html", &format!("if-none-match: {etag}\r\n")).await;
        assert!(again.starts_with("HTTP/1.1 304"), "{again}");
    }

    #[tokio::test]
    async fn the_snapshot_waits_for_a_frame_then_serves_a_jpeg() {
        gstreamer::init().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let status = Arc::new(Status::new("Test Cam", false, 16, 8));
        let site = Arc::new(Site {
            web_root: None,
            signalling: ("127.0.0.1".into(), 9),
            status: status.clone(),
            gzip: GzipCache::default(),
            allowed_hosts: Vec::new(),
            max_viewers: 0,
            peers: Vec::new(),
        });
        tokio::spawn(serve(listener, site));
        assert!(get(addr, SNAPSHOT_PATH, "")
            .await
            .starts_with("HTTP/1.1 503"));
        status.set_frame(&gstreamer::Buffer::from_mut_slice(vec![
            90u8;
            crate::frame::size(
                16, 8
            )
        ]));
        let jpeg = get(addr, SNAPSHOT_PATH, "").await;
        assert!(jpeg.starts_with("HTTP/1.1 200"), "{jpeg}");
        assert!(jpeg.contains("content-type: image/jpeg"), "{jpeg}");
        assert!(jpeg.contains("cache-control: no-store"), "{jpeg}");
    }

    #[tokio::test]
    async fn the_arrival_is_a_404_until_a_cat_came_then_its_jpeg() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let status = Arc::new(Status::new("Test Cam", false, 16, 8));
        let site = Arc::new(Site {
            web_root: None,
            signalling: ("127.0.0.1".into(), 9),
            status: status.clone(),
            gzip: GzipCache::default(),
            allowed_hosts: Vec::new(),
            max_viewers: 0,
            peers: Vec::new(),
        });
        tokio::spawn(serve(listener, site));
        assert!(get(addr, ARRIVAL_PATH, "")
            .await
            .starts_with("HTTP/1.1 404"));
        status.set_arrival(vec![0xFF, 0xD8, 0xFF, 0xD9], 1000);
        status.set_arrival(vec![0xFF, 0xD8, 0x01, 0xFF, 0xD9], 2000);
        // Each arrival keeps its own picture: a later, false visit does not replace the one a notification names.
        let (_, first) = get_raw(addr, &format!("{ARRIVAL_PATH}?t=1000"), "").await;
        assert_eq!(first, [0xFF, 0xD8, 0xFF, 0xD9]);
        assert!(get(addr, &format!("{ARRIVAL_PATH}?t=5000"), "")
            .await
            .starts_with("HTTP/1.1 404"));
        status.set_arrival(vec![0xFF, 0xD8, 0xFF, 0xD9], 3000);
        let (head, body) = get_raw(addr, ARRIVAL_PATH, "").await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.contains("content-type: image/jpeg"), "{head}");
        assert_eq!(body, [0xFF, 0xD8, 0xFF, 0xD9]);
        // The Stream page keys the picture by visit (`?visit=3`); the route must match the path alone.
        let (head, body) = get_raw(addr, &format!("{ARRIVAL_PATH}?visit=3"), "").await;
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(body, [0xFF, 0xD8, 0xFF, 0xD9]);
    }

    #[tokio::test]
    async fn healthz_reports_the_status() {
        let addr = start(None, 9).await;
        let health = get(addr, "/healthz", "").await;
        assert!(health.contains("application/json"), "{health}");
        assert!(health.contains("\"name\":\"Test Cam\""), "{health}");
    }

    #[tokio::test]
    async fn the_signalling_path_is_forwarded_as_a_raw_upgrade() {
        // A stand-in signalling server: answers 101, then echoes whatever arrives.
        let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_port = backend.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 512];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut chunk).await.unwrap();
                seen.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&seen).to_lowercase();
            assert!(head.starts_with("get /webrtc-ws http/1.1"), "{head}");
            assert!(head.contains("upgrade: websocket"), "{head}");
            stream
                .write_all(b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: test\r\n\r\n")
                .await
                .unwrap();
            let n = stream.read(&mut chunk).await.unwrap();
            stream.write_all(&chunk[..n]).await.unwrap();
        });

        let addr = start(None, backend_port).await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET /webrtc-ws HTTP/1.1\r\nhost: cat.local\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: not-a-secret\r\nsec-websocket-version: 13\r\n\r\n")
            .await
            .unwrap();
        let mut head = Vec::new();
        let mut chunk = [0u8; 512];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = client.read(&mut chunk).await.unwrap();
            head.extend_from_slice(&chunk[..n]);
        }
        let head = String::from_utf8_lossy(&head).to_lowercase();
        assert!(head.starts_with("http/1.1 101"), "{head}");
        assert!(head.contains("sec-websocket-accept: test"), "{head}");

        client.write_all(b"welcome?").await.unwrap();
        let mut echo = [0u8; 8];
        client.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"welcome?");
    }

    #[tokio::test]
    async fn an_unreachable_signalling_server_is_a_502() {
        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = unused.local_addr().unwrap().port();
        drop(unused);
        let addr = start(None, port).await;
        let response = get(addr, "/webrtc-ws", "upgrade: websocket\r\n").await;
        assert!(response.starts_with("HTTP/1.1 502"), "{response}");
    }

    /// A stand-in signalling server that reads one request head, then answers with `reply` and hangs up.
    async fn one_shot_backend(reply: &'static [u8]) -> u16 {
        let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = backend.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 512];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut chunk).await.unwrap();
                seen.extend_from_slice(&chunk[..n]);
            }
            stream.write_all(reply).await.unwrap();
        });
        port
    }

    #[tokio::test]
    async fn a_signalling_server_that_refuses_the_upgrade_is_passed_through() {
        let port = one_shot_backend(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nX-Reason: room\r\n\r\nnope",
        )
        .await;
        let addr = start(None, port).await;
        let response = get(addr, "/webrtc-ws", "upgrade: websocket\r\n").await;
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
        assert!(
            response.to_lowercase().contains("x-reason: room"),
            "{response}"
        );
        assert!(response.ends_with("nope"), "{response}");
    }

    #[tokio::test]
    async fn a_signalling_server_that_hangs_up_mid_handshake_is_a_502() {
        let addr = start(None, one_shot_backend(b"").await).await;
        let response = get(addr, "/webrtc-ws", "upgrade: websocket\r\n").await;
        assert!(response.starts_with("HTTP/1.1 502"), "{response}");
    }

    #[tokio::test]
    async fn only_settings_take_a_post_and_no_clip_is_a_404() {
        let addr = start(None, 9).await;
        let post = post(addr, "/healthz", "application/json", "{}").await;
        assert!(post.starts_with("HTTP/1.1 405"), "{post}");
        let clip = get(addr, "/arrival.mp4", "").await;
        assert!(
            clip.starts_with("HTTP/1.1 404") && clip.ends_with("no arrival clip yet"),
            "{clip}"
        );
        // An arrival's own clip that is not (or no longer) kept is a 404 too, never a newer one.
        assert!(get(addr, "/arrival.mp4?t=1791763273", "")
            .await
            .starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn without_a_web_build_the_page_says_so_and_the_api_still_answers() {
        let addr = start(None, 9).await;
        let page = get(addr, "/", "").await;
        assert!(
            page.starts_with("HTTP/1.1 404") && page.contains("no web build configured"),
            "{page}"
        );
        assert!(get(addr, "/healthz", "").await.starts_with("HTTP/1.1 200"));
    }

    fn headers(list: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in list {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn only_the_cat_cams_own_page_may_act() {
        let none: &[String] = &[];
        // The page itself, under an IP, a bare name or a local suffix, port and all.
        for host in [
            "192.168.188.98:8080",
            "himbeere2:8080",
            "himbeere2.fritz.box:8080",
            "[fd46::1]:8080",
            "cat.local",
        ] {
            let page = headers(&[("host", host), ("origin", &format!("http://{host}"))]);
            assert_eq!(own_page(&page, none), Ok(()), "{host}");
        }
        // A tool without a browser sends no Origin.
        assert_eq!(
            own_page(&headers(&[("host", "192.168.188.98:8080")]), none),
            Ok(())
        );
        // Another site's page, a sandboxed one and DNS rebinding under a public name are refused.
        let foreign = headers(&[
            ("host", "192.168.188.98:8080"),
            ("origin", "https://evil.example"),
        ]);
        assert!(own_page(&foreign, none).is_err());
        let sandboxed = headers(&[("host", "192.168.188.98:8080"), ("origin", "null")]);
        assert!(own_page(&sandboxed, none).is_err());
        let rebound = headers(&[
            ("host", "cat.evil.example:8080"),
            ("origin", "http://cat.evil.example:8080"),
        ]);
        assert!(own_page(&rebound, none).is_err());
        // Unless that name is the owner's own reverse proxy.
        assert_eq!(own_page(&rebound, &["Cat.Evil.Example".to_owned()]), Ok(()));
    }

    async fn post_with(addr: std::net::SocketAddr, path: &str, extra: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "POST {path} HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{extra}\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8_lossy(&response).into_owned()
    }

    #[tokio::test]
    async fn another_sites_page_can_neither_change_settings_nor_open_the_stream() {
        let addr = start(None, 9).await;
        let foreign = post_with(
            addr,
            "/settings",
            "origin: https://evil.example\r\n",
            r#"{"rotate": 90}"#,
        )
        .await;
        assert!(foreign.starts_with("HTTP/1.1 403"), "{foreign}");
        let own = post_with(
            addr,
            "/settings",
            "origin: http://cat.local\r\n",
            r#"{"rotate": 90}"#,
        )
        .await;
        assert!(own.starts_with("HTTP/1.1 200"), "{own}");
        // Reading stays open to anyone on the LAN; only acting is guarded.
        assert!(get(addr, "/settings", "origin: https://evil.example\r\n")
            .await
            .starts_with("HTTP/1.1 200"));
        let socket = get(
            addr,
            SIGNALLING_PATH,
            "origin: https://evil.example\r\nupgrade: websocket\r\n",
        )
        .await;
        assert!(socket.starts_with("HTTP/1.1 403"), "{socket}");
    }

    async fn start_with(site: Site) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, Arc::new(site)));
        addr
    }

    fn site(max_viewers: usize, peers: Vec<String>, signalling_port: u16) -> Site {
        Site {
            web_root: None,
            signalling: ("127.0.0.1".into(), signalling_port),
            status: Arc::new(Status::new("Cam", false, 4, 4)),
            gzip: GzipCache::default(),
            allowed_hosts: Vec::new(),
            max_viewers,
            peers,
        }
    }

    #[tokio::test]
    async fn a_viewer_past_the_limit_is_refused_until_one_leaves() {
        // A stand-in signalling server that upgrades every socket and keeps it open.
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = upstream.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut head = [0u8; 1024];
                    let _ = socket.read(&mut head).await;
                    let reply = "HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: upgrade\r\n\r\n";
                    socket.write_all(reply.as_bytes()).await.unwrap();
                    let mut rest = [0u8; 16];
                    while socket.read(&mut rest).await.is_ok_and(|n| n > 0) {}
                });
            }
        });
        let status_site = site(1, Vec::new(), port);
        let status = status_site.status.clone();
        let addr = start_with(status_site).await;
        let open = |addr| async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            let request = format!("GET {SIGNALLING_PATH} HTTP/1.1\r\nhost: cat.local\r\nupgrade: websocket\r\nconnection: upgrade\r\n\r\n");
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut head = [0u8; 64];
            let n = stream.read(&mut head).await.unwrap();
            (stream, String::from_utf8_lossy(&head[..n]).into_owned())
        };
        let (first, reply) = open(addr).await;
        assert!(reply.starts_with("HTTP/1.1 101"), "{reply}");
        let (_, refused) = open(addr).await;
        assert!(refused.starts_with("HTTP/1.1 503"), "{refused}");
        drop(first);
        // The closed tab frees its place once the proxy notices.
        for _ in 0..50 {
            if status.viewer(1).is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the closed tab never freed its place");
    }

    #[tokio::test]
    async fn another_cat_cams_page_may_read_this_one_and_no_other_page_may() {
        let addr = start_with(site(0, vec!["http://10.0.0.2:8080/".into()], 9)).await;
        let peer = get(addr, "/healthz", "origin: http://10.0.0.2:8080\r\n").await;
        assert!(
            peer.contains("access-control-allow-origin: http://10.0.0.2:8080"),
            "{peer}"
        );
        assert!(
            peer.contains("cross-origin-resource-policy: cross-origin"),
            "{peer}"
        );
        let stranger = get(addr, "/healthz", "origin: http://10.0.0.3:8080\r\n").await;
        assert!(
            !stranger.contains("access-control-allow-origin"),
            "{stranger}"
        );
        // Only reads: the page itself and the settings stay its own.
        let settings = get(addr, "/settings", "origin: http://10.0.0.2:8080\r\n").await;
        assert!(
            !settings.contains("access-control-allow-origin"),
            "{settings}"
        );
    }
}
