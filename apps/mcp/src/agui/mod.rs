//! The gauntlet, served: AG-UI over HTTP, and the page that watches it.
//!
//! ```text
//! GET  /                     apps/mcp/ui/gauntlet/index.html
//! GET  /<anything>           the rest of that directory, read from disk
//! POST /agui                 the AG-UI binding: JSON in, text/event-stream out
//! GET  /agui                 the same run for an EventSource or a curl
//! GET  /gauntlet/stream      ┐
//! GET  /gauntlet/events      │ aliases of GET /agui, because the page probes
//! GET  /agui/gauntlet        │ for its stream and any of these may be the one
//! GET  /agui/stream          │ it tries first
//! GET  /events               ┘
//! GET  /health               is a run going, and against which corpus
//! ```
//!
//! **Loopback only.** The listener is bound to 127.0.0.1 and nothing else.
//! This serves a directory of files and runs a CPU-saturating job on request;
//! neither belongs on a network interface.
//!
//! `POST /agui` is the binding the AG-UI spec defines — a `RunAgentInput` body,
//! `Accept: text/event-stream`, one protocol event per `data:` frame. `GET
//! /agui` exists because the browser's `EventSource` cannot issue a POST with a
//! body, and a demo that can be opened with `curl` is a demo that can be
//! debugged. Both produce byte-identical event streams.
//!
//! Both take `?pace=<ms>` and `?filter=<substring>`; a POST may send the same
//! two in `forwardedProps` instead, which is where a conforming AG-UI client
//! puts things the protocol has no field for. The query string wins.
//!
//! ## Two runs at once
//!
//! Every connection gets its own [`GauntletAgent`], its own `Progress` and its
//! own walk of the corpus, which is only ever read. A second viewer therefore
//! sees a second run from the beginning rather than joining the first one
//! halfway through, and closing either tab cancels only that one: the response
//! body owns the run, so hyper dropping it trips the run's cancellation token.

pub mod agent;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use ag_ui::axum::{AgUiInput, SseResponse};
use ag_ui::server::Runner;
use ag_ui::RunAgentInput;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::gauntlet::{self, corpus, RunOptions, MAX_PACE_MS};
use agent::GauntletAgent;

/// How the demo was started.
#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub corpus: PathBuf,
    pub ui_dir: PathBuf,
    /// The pace a run uses when the request does not ask for one.
    pub pace_ms: u64,
}

#[derive(Clone)]
struct AppState {
    corpus: PathBuf,
    ui_dir: PathBuf,
    pace_ms: u64,
}

/// A bound port with the demo behind it, not yet accepting.
pub struct Serving {
    listener: tokio::net::TcpListener,
    app: Router,
}

impl Serving {
    /// Accept connections until the process ends.
    pub async fn run(self) -> anyhow::Result<()> {
        axum::serve(self.listener, self.app).await?;
        Ok(())
    }
}

/// Bind the port and get everything ready.
///
/// Binding is separate from serving so that a port already in use is an error
/// the operator sees at startup, rather than a demo that quietly is not there
/// because a spawned task failed after `main` moved on.
pub async fn bind(cfg: Config) -> anyhow::Result<Serving> {
    place_holder_page(&cfg.ui_dir);
    let files = corpus::walk(&cfg.corpus).len();
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, cfg.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("could not bind {addr} for the gauntlet: {e}"))?;
    let port = listener.local_addr()?.port();

    gauntlet::set_launch(gauntlet::Launch {
        port,
        corpus: cfg.corpus.clone(),
        ui_dir: cfg.ui_dir.clone(),
        files,
    });
    tracing::info!(
        url = %format!("http://127.0.0.1:{port}/"),
        corpus = %cfg.corpus.display(),
        files,
        ui = %cfg.ui_dir.display(),
        "the Photoshop gauntlet is serving"
    );
    // One plain line as well, because the person running the demo is reading a
    // terminal, not a structured log.
    eprintln!("gauntlet ready: http://127.0.0.1:{port}/  ({files} corpus files)");

    let app = router(AppState {
        corpus: cfg.corpus,
        ui_dir: cfg.ui_dir,
        pace_ms: cfg.pace_ms,
    });
    Ok(Serving { listener, app })
}

/// Every path that starts a run over GET. `/agui` is the real one; the rest
/// are there because `apps/mcp/ui/gauntlet/index.html` — which belongs to
/// another workstream — probes a list of likely routes until one answers, and
/// a demo that needs the operator to type the right path into a query string
/// is a demo that fails in front of an audience.
pub const STREAM_ALIASES: &[&str] = &[
    "/gauntlet/stream",
    "/gauntlet/events",
    "/agui/gauntlet",
    "/agui/stream",
    "/events",
];

fn router(state: AppState) -> Router {
    let mut router = Router::new().route("/agui", get(agui_get).post(agui_post));
    for alias in STREAM_ALIASES {
        router = router.route(alias, get(agui_get).post(agui_post));
    }
    router
        .route("/health", get(health))
        .route("/", get(index))
        .route("/{*path}", get(static_file))
        .with_state(state)
}

// --- the run endpoint ----------------------------------------------------

/// `?pace=` and `?filter=`, on either method.
#[derive(Debug, Default, Deserialize)]
struct RunQuery {
    /// Milliseconds to wait after each file. 0 is full speed.
    pace: Option<u64>,
    /// Keep only files whose path contains this.
    filter: Option<String>,
}

impl RunQuery {
    /// Fill the gaps from `forwardedProps`, then from the server's default.
    fn into_options(self, state: &AppState, forwarded: Option<&serde_json::Value>) -> RunOptions {
        let from_props = |key: &str| forwarded.and_then(|v| v.get(key));
        let pace = self
            .pace
            .or_else(|| from_props("pace").and_then(serde_json::Value::as_u64))
            .unwrap_or(state.pace_ms)
            .min(MAX_PACE_MS);
        let filter = self
            .filter
            .or_else(|| {
                from_props("filter")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|f| !f.trim().is_empty());
        RunOptions {
            corpus: state.corpus.clone(),
            filter,
            pace_ms: pace,
        }
    }
}

/// The AG-UI HTTP binding: a `RunAgentInput` body, an SSE stream back.
async fn agui_post(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    headers: HeaderMap,
    input: Result<AgUiInput, ag_ui::axum::Error>,
) -> Response {
    let input = match input {
        Ok(AgUiInput(input)) => input,
        Err(e) => return e.into_response(),
    };
    let opts = query.into_options(&state, Some(&input.forwarded_props));
    stream_run(&headers, input, opts)
}

/// The same run for a client that can only issue a GET — `EventSource`, or a
/// person with `curl -N`.
async fn agui_get(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    headers: HeaderMap,
) -> Response {
    let opts = query.into_options(&state, None);
    let input = RunAgentInput::new(new_id("thread"), new_id("run"));
    stream_run(&headers, input, opts)
}

fn stream_run(headers: &HeaderMap, input: RunAgentInput, opts: RunOptions) -> Response {
    let accept = headers
        .get(header::ACCEPT)
        .map(|v| String::from_utf8_lossy(v.as_bytes()));
    let response = match SseResponse::negotiate(accept.as_deref()) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    let runner = Runner::new(GauntletAgent::new(opts));
    // Taken before `run` consumes the runner: this is what a closed browser
    // tab trips.
    let response = response
        .cancellation(runner.cancellation_token())
        .keep_alive(std::time::Duration::from_secs(15));
    no_auto_reconnect(response.stream(runner.run(input)))
}

/// Prepend an SSE `retry:` directive that effectively switches off
/// `EventSource`'s automatic reconnect.
///
/// A finished run closes the stream, and a browser's default reaction to a
/// closed `EventSource` is to reopen it a few seconds later. Here that would
/// silently start a second full corpus run, then a third: the demo would loop
/// on its own for as long as the tab is open. `retry:` is the mechanism SSE
/// provides for saying "do not", and an explicit reconnect — a reload, or the
/// page's own Reconnect button — makes a fresh `EventSource` and ignores it.
fn no_auto_reconnect(mut response: Response) -> Response {
    use futures_util::StreamExt as _;
    const A_DAY_IN_MS: &[u8] = b"retry: 86400000\n\n";
    let body = std::mem::replace(response.body_mut(), axum::body::Body::empty());
    let prefix = futures_util::stream::once(async {
        Ok::<_, axum::Error>(axum::body::Bytes::from_static(A_DAY_IN_MS))
    });
    *response.body_mut() = axum::body::Body::from_stream(prefix.chain(body.into_data_stream()));
    response
}

/// Ids that are unique within a process without pulling in a UUID crate: a
/// monotonic counter, plus the time so two runs started after a restart do not
/// collide in a browser's log.
fn new_id(kind: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{kind}-{t}-{n}")
}

// --- health --------------------------------------------------------------

async fn health(State(state): State<AppState>) -> Response {
    let body = serde_json::json!({
        "ok": true,
        "activeRuns": gauntlet::active_runs(),
        "corpus": state.corpus.display().to_string(),
        "files": corpus::walk(&state.corpus).len(),
        "defaultPaceMs": state.pace_ms,
        "endpoint": "/agui",
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

// --- the page ------------------------------------------------------------

/// The front end belongs to another workstream. If it has not landed yet, put
/// one line there so the demo has something to open — and never touch a file
/// that already exists.
fn place_holder_page(ui_dir: &Path) {
    let index = ui_dir.join("index.html");
    if index.exists() {
        return;
    }
    if std::fs::create_dir_all(ui_dir).is_err() {
        return;
    }
    let _ = std::fs::write(
        &index,
        "<!doctype html><meta charset=utf-8><title>Photoshop gauntlet</title>\n\
         <p>The front end has not landed yet. The run is at \
         <code>POST /agui</code> (or <code>GET /agui?pace=0</code>).\n",
    );
    tracing::info!(path = %index.display(), "wrote a placeholder page; the UI workstream owns this file");
}

async fn index(State(state): State<AppState>) -> Response {
    serve_file(&state.ui_dir, "index.html")
}

async fn static_file(State(state): State<AppState>, axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    serve_file(&state.ui_dir, &path)
}

fn serve_file(dir: &Path, rel: &str) -> Response {
    let Some(path) = resolve(dir, rel) else {
        return (StatusCode::NOT_FOUND, "no such file").into_response();
    };
    match std::fs::read(&path) {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, content_type(&path)), (header::CACHE_CONTROL, "no-cache")],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "no such file").into_response(),
    }
}

/// Resolve a request path inside the UI directory, or refuse it.
///
/// A static file server is the classic way to hand out `/etc/passwd`, so this
/// one rejects absolute paths, parent components and anything whose
/// canonicalised form leaves the directory — the same rule, and the same
/// reason, as [`crate::sandbox`].
fn resolve(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return resolve(dir, "index.html");
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute() || candidate.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let joined = dir.join(candidate);
    let real = joined.canonicalize().ok()?;
    let root = dir.canonicalize().ok()?;
    real.starts_with(&root).then_some(real)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gauntlet::DEFAULT_PACE_MS;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gauntlet-ui-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_placeholder_appears_only_when_there_is_no_real_page() {
        let dir = tmp("placeholder");
        place_holder_page(&dir);
        let written = std::fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(written.contains("has not landed yet"));

        std::fs::write(dir.join("index.html"), "<h1>the real thing</h1>").unwrap();
        place_holder_page(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("index.html")).unwrap(),
            "<h1>the real thing</h1>",
            "a real page must never be overwritten"
        );
    }

    #[test]
    fn a_path_cannot_climb_out_of_the_ui_directory() {
        let dir = tmp("traversal");
        std::fs::write(dir.join("index.html"), "ok").unwrap();
        std::fs::write(dir.parent().unwrap().join("gauntlet-secret.txt"), "no").unwrap();
        assert!(resolve(&dir, "index.html").is_some());
        assert!(resolve(&dir, "../gauntlet-secret.txt").is_none());
        assert!(resolve(&dir, "/etc/passwd").is_none());
        assert!(resolve(&dir, "does-not-exist.js").is_none());
        assert!(resolve(&dir, "").is_some(), "the root is the index");
    }

    #[test]
    fn the_query_beats_the_forwarded_props_which_beat_the_default() {
        let state = AppState {
            corpus: PathBuf::from("/tmp/psd"),
            ui_dir: PathBuf::from("/tmp/ui"),
            pace_ms: DEFAULT_PACE_MS,
        };
        let props = serde_json::json!({ "pace": 7, "filter": "from-props" });

        let q = RunQuery { pace: Some(3), filter: Some("from-query".into()) };
        let o = q.into_options(&state, Some(&props));
        assert_eq!(o.pace_ms, 3);
        assert_eq!(o.filter.as_deref(), Some("from-query"));

        let o = RunQuery::default().into_options(&state, Some(&props));
        assert_eq!(o.pace_ms, 7);
        assert_eq!(o.filter.as_deref(), Some("from-props"));

        let o = RunQuery::default().into_options(&state, None);
        assert_eq!(o.pace_ms, DEFAULT_PACE_MS);
        assert_eq!(o.filter, None);
    }

    #[test]
    fn full_speed_is_expressible_and_a_silly_pace_is_capped() {
        let state = AppState {
            corpus: PathBuf::from("/tmp/psd"),
            ui_dir: PathBuf::from("/tmp/ui"),
            pace_ms: DEFAULT_PACE_MS,
        };
        assert_eq!(RunQuery { pace: Some(0), ..Default::default() }.into_options(&state, None).pace_ms, 0);
        assert_eq!(
            RunQuery { pace: Some(u64::MAX), ..Default::default() }.into_options(&state, None).pace_ms,
            MAX_PACE_MS
        );
        // An empty filter is no filter, not a filter that matches nothing.
        assert_eq!(RunQuery { filter: Some("  ".into()), ..Default::default() }.into_options(&state, None).filter, None);
    }

    #[test]
    fn content_types_are_the_ones_a_browser_needs() {
        assert_eq!(content_type(Path::new("a/index.html")), "text/html; charset=utf-8");
        assert_eq!(content_type(Path::new("a/app.js")), "text/javascript; charset=utf-8");
        assert_eq!(content_type(Path::new("a/x.CSS")), "text/css; charset=utf-8");
        assert_eq!(content_type(Path::new("a/blob")), "application/octet-stream");
    }
}
