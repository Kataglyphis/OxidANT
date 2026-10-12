//! The built-in web server: the Flutter web build, `/healthz`, and `/webrtc-ws` forwarded to the
//! signalling server, so one origin serves the page and its WebSocket and no nginx is needed.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{bail, Context as _};
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use crate::status::Status;

/// What the web client's host-relative `signalingServerUrl` resolves to.
pub const SIGNALLING_PATH: &str = "/webrtc-ws";

/// The latest annotated frame as a JPEG.
pub const SNAPSHOT_PATH: &str = "/snapshot.jpg";

/// The frame the last cat arrival was confirmed on, as a JPEG.
pub const ARRIVAL_PATH: &str = "/arrival.jpg";
/// The last arrival as an H.264 clip, from a few seconds before it.
pub const CLIP_PATH: &str = "/arrival.mp4";

/// How long the signalling server may take to answer the WebSocket handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Smaller files go out as they are: gzip's header would eat most of the saving.
const MIN_GZIP_BYTES: u64 = 1024;

type Body = Full<Bytes>;

pub struct Site {
    pub web_root: Option<PathBuf>,
    /// Where `/webrtc-ws` goes: the signalling server's host and port.
    pub signalling: (String, u16),
    pub status: Arc<Status>,
    pub gzip: GzipCache,
}

/// Gzipped copies of the web build's files, made on first request and kept while the file is unchanged.
#[derive(Default)]
pub struct GzipCache(Mutex<HashMap<PathBuf, (String, Bytes)>>);

impl GzipCache {
    /// The gzipped file; `version` is its identity ETag, so a changed file is compressed again.
    async fn get(&self, file: &Path, version: &str) -> std::io::Result<Bytes> {
        let cached = self.0.lock().ok().and_then(|cache| {
            cache
                .get(file)
                .filter(|(v, _)| v == version)
                .map(|(_, gz)| gz.clone())
        });
        if let Some(gz) = cached {
            return Ok(gz);
        }
        let bytes = tokio::fs::read(file).await?;
        // Off the runtime's two workers: the 7 MB CanvasKit takes a while.
        let gz = tokio::task::spawn_blocking(move || gzip(&bytes))
            .await
            .map_err(std::io::Error::other)??;
        if let Ok(mut cache) = self.0.lock() {
            cache.insert(file.to_path_buf(), (version.to_owned(), gz.clone()));
        }
        Ok(gz)
    }
}

fn gzip(data: &[u8]) -> std::io::Result<Bytes> {
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(data.len() / 3),
        flate2::Compression::default(),
    );
    encoder.write_all(data)?;
    encoder.finish().map(Bytes::from)
}

/// Whether the client takes gzip: listed (or `*`) without `q=0`.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|item| {
            let mut parts = item.split(';');
            let coding = parts.next().unwrap_or_default().trim();
            let refused = parts.any(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|q| q.parse::<f32>().ok())
                    .is_some_and(|q| q == 0.0)
            });
            (coding.eq_ignore_ascii_case("gzip") || coding == "*") && !refused
        })
}

/// Types gzip shrinks; images and woff fonts are compressed already.
fn compressible(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || matches!(
            content_type,
            "application/wasm" | "application/json" | "image/svg+xml" | "font/ttf" | "font/otf"
        )
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
    let mut response = if path == SIGNALLING_PATH {
        match forward_upgrade(req, &site.signalling).await {
            Ok(response) => response,
            Err(err) => {
                log::warn!("{SIGNALLING_PATH}: {err:#}");
                plain(StatusCode::BAD_GATEWAY, "signalling server unreachable")
            }
        }
    } else if req.method() != Method::GET && req.method() != Method::HEAD {
        plain(StatusCode::METHOD_NOT_ALLOWED, "GET or HEAD only")
    } else if path == "/healthz" {
        let mut response = Response::new(Full::new(Bytes::from(site.status.to_json())));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    } else if path == SNAPSHOT_PATH {
        snapshot(site.status.clone()).await
    } else if path == ARRIVAL_PATH {
        match site.status.arrival_jpeg() {
            Some(jpeg) => jpeg_response(jpeg),
            None => plain(StatusCode::NOT_FOUND, "no cat has arrived yet"),
        }
    } else if path == CLIP_PATH {
        match site.status.clip_mp4() {
            Some(mp4) => mp4_response(req.headers(), mp4),
            None => plain(StatusCode::NOT_FOUND, "no arrival clip yet"),
        }
    } else {
        static_file(&req, site.web_root.as_deref(), &site.gzip).await
    };
    let headers = response.headers_mut();
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

fn plain(status: StatusCode, text: &'static str) -> Response<Body> {
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

fn jpeg_response(jpeg: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(jpeg)));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn static_file(
    req: &Request<Incoming>,
    root: Option<&Path>,
    gzip_cache: &GzipCache,
) -> Response<Body> {
    let Some(root) = root else {
        return plain(StatusCode::NOT_FOUND, "no web build configured (web_root)");
    };
    let Some(relative) = sanitize(req.uri().path()) else {
        return plain(StatusCode::BAD_REQUEST, "bad path");
    };
    let Some(file) = resolve(root, &relative) else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let Ok(meta) = tokio::fs::metadata(&file).await else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    // Size and mtime: a redeployed build changes both, so a returning viewer never keeps a stale page.
    let identity = format!("\"{:x}-{:x}\"", meta.len(), modified);
    let kind = content_type(&file);
    let gzipped = compressible(kind) && meta.len() >= MIN_GZIP_BYTES && accepts_gzip(req.headers());
    let etag = if gzipped {
        format!("\"{:x}-{:x}-gz\"", meta.len(), modified)
    } else {
        identity.clone()
    };
    let fresh = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .is_some_and(|tag| tag.as_bytes() == etag.as_bytes());
    // A revalidation, the common request on a return visit, reads nothing.
    let body = if fresh || req.method() == Method::HEAD {
        Ok(Bytes::new())
    } else if gzipped {
        gzip_cache.get(&file, &identity).await
    } else {
        tokio::fs::read(&file).await.map(Bytes::from)
    };
    let Ok(body) = body else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let mut response = Response::new(Full::new(body));
    if fresh {
        *response.status_mut() = StatusCode::NOT_MODIFIED;
    }
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    // Flutter's entry files are not content-hashed: revalidate everything rather than cache stale code.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if let Ok(value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, value);
    }
    if compressible(kind) {
        headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
    if gzipped {
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    response
}

/// The request path as a relative path below the web root, or `None` when it tries to leave it.
pub fn sanitize(path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    // Backslashes and drive letters only mean something on Windows; a colon there is also an NTFS stream.
    if decoded.contains(['\0', '\\', ':']) {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(relative)
}

/// The file to send: the path itself, a directory's `index.html`, or `index.html` for an app route.
fn resolve(root: &Path, relative: &Path) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if candidate.is_file() {
        return Some(candidate);
    }
    let index = candidate.join("index.html");
    if candidate.is_dir() && index.is_file() {
        return Some(index);
    }
    // A missing path without an extension is a route of the single-page app, not a missing asset.
    let is_route = relative
        .file_name()
        .is_some_and(|name| !name.to_string_lossy().contains('.'));
    let app = root.join("index.html");
    (is_route && app.is_file()).then_some(app)
}

/// The MIME type browsers insist on: `compileStreaming` and module scripts refuse anything else.
pub fn content_type(file: &Path) -> &'static str {
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Forwards a WebSocket upgrade byte for byte: the signalling server does the handshake itself.
async fn forward_upgrade(
    req: Request<Incoming>,
    (host, port): &(String, u16),
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

    #[test]
    fn sanitize_keeps_paths_inside_the_root() {
        assert_eq!(sanitize("/"), Some(PathBuf::new()));
        assert_eq!(
            sanitize("/assets/Noto%20Sans.ttf"),
            Some(PathBuf::from("assets").join("Noto Sans.ttf"))
        );
        for evil in [
            "/../etc/passwd",
            "/assets/%2e%2e/%2e%2e/secret",
            "/a\\..\\b",
            "/C:/Windows",
            "/x/file.txt:stream",
            "/%00",
        ] {
            assert_eq!(sanitize(evil), None, "accepted {evil}");
        }
    }

    #[test]
    fn content_types_are_the_ones_the_flutter_boot_needs() {
        assert_eq!(
            content_type(Path::new("main.dart.wasm")),
            "application/wasm"
        );
        assert!(content_type(Path::new("main.dart.mjs")).starts_with("text/javascript"));
        assert!(content_type(Path::new("index.html")).starts_with("text/html"));
        assert_eq!(
            content_type(Path::new("NOTICES")),
            "application/octet-stream"
        );
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

    #[test]
    fn accept_encoding_is_read_with_its_q_values() {
        let accepts = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_str(value).unwrap(),
            );
            accepts_gzip(&headers)
        };
        assert!(accepts("gzip, deflate, br"));
        assert!(accepts("br;q=1.0, GZIP;q=0.5"));
        assert!(accepts("*"));
        assert!(!accepts("gzip;q=0"));
        assert!(!accepts("identity"));
        assert!(!accepts_gzip(&HeaderMap::new()));
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
        });
        tokio::spawn(serve(listener, site));
        assert!(get(addr, ARRIVAL_PATH, "")
            .await
            .starts_with("HTTP/1.1 404"));
        status.set_arrival(vec![0xFF, 0xD8, 0xFF, 0xD9]);
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
}
