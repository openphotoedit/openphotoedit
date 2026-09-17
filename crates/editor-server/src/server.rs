//! The HTTP surface: routes, the headers every response carries, binding.
//!
//! | Route | What |
//! |---|---|
//! | `GET /api/health` | `{app, version, native, api, web, features}` |
//! | `GET /api/models` | the models manifest with `installed` per entry |
//! | `GET /api/file/<token>` | the bytes of a file named by `openphotoedit open`, once |
//! | `GET /models/<path>` | a model file: per-user folder first, then the web build |
//! | `GET /<path>` | the web app; unknown extensionless paths get `index.html` |

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::http::{self as h, Cache, Found};
use crate::paths::{self, BadPath};
use crate::tokens::{TakeError, Tokens};
use crate::web::WebSource;

pub struct AppState {
    /// The web app. `None` only in a build without `embed` run without `--dir`.
    pub web: Option<WebSource>,
    pub models_dir: PathBuf,
    pub tokens: Arc<Tokens>,
}

impl AppState {
    pub fn new(web: Option<WebSource>, models_dir: PathBuf) -> AppState {
        AppState { web, models_dir, tokens: Arc::new(Tokens::new()) }
    }
}

type St = State<Arc<AppState>>;

/// Features the UI can switch on when it sees them in `/api/health`.
pub const FEATURES: &[&str] = &["models", "models-manifest", "open-file", "range"];

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/models", get(models_listing))
        .route("/api/file/{token}", get(file_handoff))
        .route("/api", get(api_not_found))
        .route("/api/{*rest}", get(api_not_found))
        .route("/models/{*rest}", get(model_file))
        .fallback(web_file)
        .with_state(state)
        .layer(middleware::from_fn(log_request))
        .layer(middleware::from_fn(common_headers))
        .layer(middleware::from_fn(loopback_host_only))
}

// ---------------------------------------------------------------- middleware

/// Refuse any request whose Host is not a loopback name.
///
/// Binding to 127.0.0.1 stops other machines. It does not stop DNS
/// rebinding: a hostile page can point its own name at 127.0.0.1 and then
/// make same-origin requests to this server from the person's browser. Those
/// requests carry the hostile name in `Host`, so checking it closes the gap.
async fn loopback_host_only(req: Request, next: Next) -> Response {
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).or_else(|| req.uri().host());
    let ok = host.is_some_and(|h| {
        let name = if let Some(rest) = h.strip_prefix('[') {
            rest.split(']').next().map(|n| format!("[{n}]")).unwrap_or_default()
        } else {
            h.rsplit_once(':').map(|(n, _)| n).unwrap_or(h).to_string()
        };
        matches!(name.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "[::1]")
    });
    if !ok {
        tracing::warn!(host = ?host, path = %req.uri().path(), "refused a request for a non-loopback host");
        return (StatusCode::MISDIRECTED_REQUEST, "This server only answers on 127.0.0.1 or localhost.").into_response();
    }
    next.run(req).await
}

/// Headers on every response, errors included.
///
/// COOP + COEP make the page cross-origin isolated, which onnxruntime-web
/// needs for `SharedArrayBuffer` threads. Everything is same-origin, so
/// `require-corp` costs nothing.
async fn common_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let hs = res.headers_mut();
    hs.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    hs.insert("cross-origin-embedder-policy", HeaderValue::from_static("require-corp"));
    hs.insert("cross-origin-resource-policy", HeaderValue::from_static("same-origin"));
    hs.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    hs.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    res
}

async fn log_request(req: Request, next: Next) -> Response {
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let start = Instant::now();
    let res = next.run(req).await;
    let status = res.status().as_u16();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    if res.status().is_server_error() {
        tracing::error!(%method, %path, status, ms, "request");
    } else {
        tracing::info!(%method, %path, status, ms, "request");
    }
    res
}

// ---------------------------------------------------------------- handlers

async fn health(State(st): St) -> impl IntoResponse {
    let web = st.web.as_ref().map(WebSource::describe).unwrap_or("none");
    let res = Json(json!({
        "app": crate::APP,
        "version": crate::VERSION,
        "native": true,
        "api": 1,
        "web": web,
        "features": FEATURES,
    }));
    ([(header::CACHE_CONTROL, "no-store")], res)
}

async fn models_listing(State(st): St) -> Response {
    match crate::models::load_manifest(None, &st.models_dir, st.web.as_ref()).await {
        Ok((m, source)) => {
            let mut v = crate::models::listing(&m, &st.models_dir);
            v["source"] = json!(source);
            v["dir"] = json!(st.models_dir.display().to_string());
            ([(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "models manifest unusable");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("{e:#}") }))).into_response()
        }
    }
}

async fn api_not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "no such API" }))).into_response()
}

async fn file_handoff(State(st): St, axum::extract::Path(token): axum::extract::Path<String>, headers: HeaderMap) -> Response {
    // A token URL is only ever fetched by our own page. A cross-site fetch
    // cannot read the response anyway (CORP), but it could spend the token.
    if headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") {
        return (StatusCode::FORBIDDEN, "cross-site").into_response();
    }
    let handoff = match st.tokens.take(&token) {
        Ok(h) => h,
        Err(TakeError::Unknown) => return (StatusCode::NOT_FOUND, "This link is not valid.").into_response(),
        Err(TakeError::Gone) => {
            return (StatusCode::GONE, "This link was already used or has expired. Run `openphotoedit open` again.").into_response()
        }
    };
    let found = match Found::open(&handoff.path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, path = %handoff.path.display(), "handoff file unreadable");
            return (StatusCode::NOT_FOUND, "The file could not be read. It may have been moved or deleted.").into_response();
        }
    };
    let mut res = h::respond(&Method::GET, &HeaderMap::new(), found, &handoff.name, Cache::NoStore).await;
    let encoded = percent_encoding::utf8_percent_encode(&handoff.name, percent_encoding::NON_ALPHANUMERIC).to_string();
    let ascii: String = handoff.name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let hs = res.headers_mut();
    hs.remove(header::ETAG);
    hs.remove(header::LAST_MODIFIED);
    hs.remove(header::ACCEPT_RANGES);
    if let Ok(v) = HeaderValue::from_str(&format!("inline; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")) {
        hs.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Ok(v) = HeaderValue::from_str(&encoded) {
        hs.insert("x-file-name", v);
    }
    res
}

fn bad_path(e: BadPath) -> Response {
    let msg = match e {
        BadPath::Encoding => "The path is not valid.",
        BadPath::Forbidden => "The path is not allowed.",
    };
    (StatusCode::BAD_REQUEST, msg).into_response()
}

async fn model_file(State(st): St, method: Method, uri: Uri, headers: HeaderMap) -> Response {
    let raw = uri.path().strip_prefix("/models/").unwrap_or("");
    let segs = match paths::clean(raw) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return bad_path(e),
    };
    let name = segs.last().cloned().unwrap_or_default();
    if let Some(p) = paths::resolve_under(&st.models_dir, &segs) {
        if let Ok(found) = Found::open(&p).await {
            return h::respond(&method, &headers, found, &name, Cache::Revalidate).await;
        }
    }
    if let Some(web) = &st.web {
        let mut key = vec!["models".to_string()];
        key.extend(segs);
        if let Some(found) = web.get(&key).await {
            return h::respond(&method, &headers, found, &name, Cache::Revalidate).await;
        }
    }
    (StatusCode::NOT_FOUND, "This model is not installed. Run `openphotoedit models list`.").into_response()
}

async fn web_file(State(st): St, method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "GET, HEAD")]).into_response();
    }
    let segs = match paths::clean(uri.path()) {
        Ok(s) => s,
        Err(e) => return bad_path(e),
    };
    let Some(web) = &st.web else {
        return (StatusCode::NOT_FOUND, "This build has no web app. Run with --dir apps/web/dist.").into_response();
    };
    let is_index = segs.is_empty() || segs == ["index.html"];
    if !is_index {
        if let Some(found) = web.get(&segs).await {
            let cache = if segs.first().map(String::as_str) == Some("assets") { Cache::Immutable } else { Cache::Revalidate };
            let name = segs.last().cloned().unwrap_or_default();
            return h::respond(&method, &headers, found, &name, cache).await;
        }
        // A missing file with an extension, or anything under a build folder,
        // is a real 404. Answering index.html there turns a missing script
        // into a baffling "unexpected token <" in the console.
        let looks_like_file = segs.last().is_some_and(|s| s.contains('.'));
        let build_folder = matches!(segs[0].as_str(), "assets" | "ort" | "models" | "api");
        if looks_like_file || build_folder {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
    }
    match web.get(&["index.html".to_string()]).await {
        // The shell must always be revalidated: it names the hashed assets.
        Some(found) => h::respond(&method, &headers, found, "index.html", Cache::Revalidate).await,
        None => (StatusCode::NOT_FOUND, Body::from("index.html is missing from the web build.")).into_response(),
    }
}

// ---------------------------------------------------------------- binding

/// The port tried first when none is given.
pub const DEFAULT_PORT: u16 = 8765;

/// Bind on loopback. An explicit port is strict (a test or a script asked
/// for exactly that one); the default walks a few ports and then lets the OS
/// choose, because a downloaded app that dies when something else holds 8765
/// is an app that "does nothing" when double-clicked.
///
/// Loopback only, and not configurable: this server hands out local files
/// through tokens and must never be reachable from another machine.
pub async fn bind(port: Option<u16>) -> anyhow::Result<tokio::net::TcpListener> {
    if let Some(p) = port {
        let addr = SocketAddr::from(([127, 0, 0, 1], p));
        return tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| anyhow::anyhow!("could not listen on {addr}: {e}. Is another copy running? Omit --port to pick a free one."));
    }
    let mut last = String::new();
    for p in (DEFAULT_PORT..DEFAULT_PORT + 10).chain(std::iter::once(0)) {
        let addr = SocketAddr::from(([127, 0, 0, 1], p));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => return Ok(l),
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    anyhow::bail!("could not open a loopback port to run on. Last try was {last}.")
}

/// Resolves on Ctrl-C, or SIGTERM on Unix.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
