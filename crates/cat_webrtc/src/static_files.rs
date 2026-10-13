//! The web build's files: a request path kept below the web root, the type a browser insists on,
//! and gzip, from a packaged `.gz` beside the file or compressed once and cached.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};

use crate::web::{plain, Body};

/// Smaller files go out as they are: gzip's header would eat most of the saving.
pub const MIN_GZIP_BYTES: u64 = 1024;

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

/// The `<file>.gz` packaged beside `file`, unless it is older: zopfli-made at package time, so
/// smaller than [`gzip`]'s, and neither compressed nor held in memory here.
async fn precompressed(file: &Path, modified: Option<SystemTime>) -> Option<Bytes> {
    let mut gz = file.as_os_str().to_owned();
    gz.push(".gz");
    let meta = tokio::fs::metadata(&gz).await.ok()?;
    if meta.modified().ok()? < modified? {
        return None;
    }
    tokio::fs::read(&gz).await.ok().map(Bytes::from)
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
pub fn compressible(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || matches!(
            content_type,
            "application/wasm" | "application/json" | "image/svg+xml" | "font/ttf" | "font/otf"
        )
}

pub(crate) async fn static_file(
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
        match precompressed(&file, meta.modified().ok()).await {
            Some(gz) => Ok(gz),
            None => gzip_cache.get(&file, &identity).await,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
