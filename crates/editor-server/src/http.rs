//! Serving one file: MIME type, caching, validators and byte ranges.
//!
//! Hand-written rather than `tower_http::services::ServeDir` because the same
//! rules must apply to files that live in the binary (rust-embed) and files on
//! disk (`--dir`, the models folder), and because the cache policy depends on
//! where in the app a file sits, which ServeDir does not know.

use std::io::SeekFrom;
use std::time::SystemTime;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// Where a file's bytes come from.
pub enum Content {
    Bytes(Bytes),
    File { file: tokio::fs::File, len: u64 },
}

impl Content {
    pub fn len(&self) -> u64 {
        match self {
            Content::Bytes(b) => b.len() as u64,
            Content::File { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A file ready to send.
pub struct Found {
    pub content: Content,
    /// A strong validator, quoted, e.g. `"abc123"`.
    pub etag: String,
    pub modified: Option<SystemTime>,
}

impl Found {
    /// Open a file on disk, deriving the validator from size and mtime.
    pub async fn open(path: &std::path::Path) -> std::io::Result<Found> {
        let file = tokio::fs::File::open(path).await?;
        let meta = file.metadata().await?;
        let modified = meta.modified().ok();
        let secs = modified.and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        Ok(Found { etag: format!("\"{:x}-{:x}\"", meta.len(), secs), modified, content: Content::File { len: meta.len(), file } })
    }
}

/// How long a browser may keep a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cache {
    /// Content-hashed build output: the name changes whenever the bytes do.
    Immutable,
    /// Keep, but ask every time (cheap with the ETag). index.html, models.
    Revalidate,
    /// Never store. One-time file handoffs.
    NoStore,
}

impl Cache {
    fn header(self) -> &'static str {
        match self {
            Cache::Immutable => "public, max-age=31536000, immutable",
            Cache::Revalidate => "no-cache",
            Cache::NoStore => "no-store",
        }
    }
}

/// The Content-Type for a file name. Explicit for the types the editor
/// depends on; browsers refuse `WebAssembly.instantiateStreaming` and module
/// workers served with the wrong type.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "psd" | "psb" => "image/vnd.adobe.photoshop",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" | "md" => "text/plain; charset=utf-8",
        // onnx, ort, bin, data, safetensors, gguf and anything unknown.
        _ => "application/octet-stream",
    }
}

/// A single satisfiable byte range, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RangeReq {
    /// No usable Range header: send everything.
    Full,
    Partial(ByteRange),
    Unsatisfiable,
}

/// Parse a `Range` header against a file of `len` bytes.
///
/// One range only. A multi-range request is answered with the whole file,
/// which RFC 9110 allows; the model loaders ask for one range at a time.
pub fn parse_range(value: &str, len: u64) -> RangeReq {
    let Some(spec) = value.trim().strip_prefix("bytes=") else { return RangeReq::Full };
    if spec.contains(',') {
        return RangeReq::Full;
    }
    let Some((a, b)) = spec.trim().split_once('-') else { return RangeReq::Full };
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        // Suffix: the last N bytes.
        let Ok(n) = b.parse::<u64>() else { return RangeReq::Full };
        if n == 0 || len == 0 {
            return RangeReq::Unsatisfiable;
        }
        let n = n.min(len);
        return RangeReq::Partial(ByteRange { start: len - n, end: len - 1 });
    }
    let Ok(start) = a.parse::<u64>() else { return RangeReq::Full };
    let end = if b.is_empty() {
        len.saturating_sub(1)
    } else {
        match b.parse::<u64>() {
            Ok(e) if e >= start => e.min(len.saturating_sub(1)),
            _ => return RangeReq::Full,
        }
    };
    if start >= len {
        return RangeReq::Unsatisfiable;
    }
    RangeReq::Partial(ByteRange { start, end })
}

fn etag_matches(list: &str, etag: &str) -> bool {
    list.split(',').map(str::trim).any(|t| t == "*" || t == etag || t.strip_prefix("W/") == Some(etag))
}

/// Build the response for `found`, honouring conditional and range requests.
pub async fn respond(method: &Method, req: &HeaderMap, found: Found, name: &str, cache: Cache) -> Response {
    let len = found.content.len();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime_for(name)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache.header()));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(v) = HeaderValue::from_str(&found.etag) {
        headers.insert(header::ETAG, v);
    }
    if let Some(m) = found.modified {
        if let Ok(v) = HeaderValue::from_str(&httpdate::fmt_http_date(m)) {
            headers.insert(header::LAST_MODIFIED, v);
        }
    }

    if let Some(inm) = req.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        if etag_matches(inm, &found.etag) {
            return (StatusCode::NOT_MODIFIED, headers).into_response();
        }
    }

    let mut range = match req.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        Some(r) => parse_range(r, len),
        None => RangeReq::Full,
    };
    // If-Range: only honour the range when the client's copy is this one.
    if let Some(ir) = req.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
        if ir.trim() != found.etag {
            range = RangeReq::Full;
        }
    }

    let (status, start, count) = match range {
        RangeReq::Full => (StatusCode::OK, 0, len),
        RangeReq::Partial(r) => {
            let v = format!("bytes {}-{}/{}", r.start, r.end, len);
            headers.insert(header::CONTENT_RANGE, HeaderValue::from_str(&v).expect("ascii"));
            (StatusCode::PARTIAL_CONTENT, r.start, r.end - r.start + 1)
        }
        RangeReq::Unsatisfiable => {
            let v = format!("bytes */{len}");
            headers.insert(header::CONTENT_RANGE, HeaderValue::from_str(&v).expect("ascii"));
            headers.remove(header::CONTENT_TYPE);
            return (StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response();
        }
    };
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(count));

    if method == Method::HEAD {
        return (status, headers).into_response();
    }
    let body = match found.content {
        Content::Bytes(b) => Body::from(b.slice(start as usize..(start + count) as usize)),
        Content::File { mut file, .. } => {
            if start > 0 {
                if let Err(e) = file.seek(SeekFrom::Start(start)).await {
                    tracing::warn!(error = %e, "seek failed");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            Body::from_stream(ReaderStream::with_capacity(file.take(count), 256 * 1024))
        }
    };
    (status, headers, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let p = |s| parse_range(s, 100);
        assert_eq!(p("bytes=0-9"), RangeReq::Partial(ByteRange { start: 0, end: 9 }));
        assert_eq!(p("bytes=90-"), RangeReq::Partial(ByteRange { start: 90, end: 99 }));
        assert_eq!(p("bytes=-10"), RangeReq::Partial(ByteRange { start: 90, end: 99 }));
        assert_eq!(p("bytes=-1000"), RangeReq::Partial(ByteRange { start: 0, end: 99 }));
        assert_eq!(p("bytes=50-5000"), RangeReq::Partial(ByteRange { start: 50, end: 99 }));
        assert_eq!(p("bytes=100-"), RangeReq::Unsatisfiable);
        assert_eq!(p("bytes=-0"), RangeReq::Unsatisfiable);
        assert_eq!(p("bytes=9-3"), RangeReq::Full);
        assert_eq!(p("bytes=0-1,5-6"), RangeReq::Full);
        assert_eq!(p("items=0-1"), RangeReq::Full);
        assert_eq!(p("bytes=x-1"), RangeReq::Full);
    }

    #[test]
    fn mimes() {
        assert_eq!(mime_for("assets/editor_wasm_bg-abc.wasm"), "application/wasm");
        assert_eq!(mime_for("ort/ort-wasm-simd-threaded.jsep.mjs"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("models/u2net.onnx"), "application/octet-stream");
        assert_eq!(mime_for("INDEX.HTML"), "text/html; charset=utf-8");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }
}
