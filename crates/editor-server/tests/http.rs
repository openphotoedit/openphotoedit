//! The server over real sockets: headers, fallback, tokens, ranges, and the
//! paths that must never reach the disk outside the served folders.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use editor_server::models::{self, Fetched, Manifest};
use editor_server::server::{router, AppState};
use editor_server::web::WebSource;
use reqwest::header;
use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    _root: tempfile::TempDir,
    root: PathBuf,
    addr: SocketAddr,
    state: Arc<AppState>,
    client: reqwest::Client,
    model: Vec<u8>,
}

impl Fixture {
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

const INDEX: &str = "<!doctype html><title>OpenPhotoshop test shell</title>";
const SECRET: &str = "TOP-SECRET-CONTENTS";

async fn start() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let web = root.join("web");
    let models = root.join("models");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::create_dir_all(web.join("ort")).unwrap();
    std::fs::create_dir_all(web.join("models")).unwrap();
    std::fs::create_dir_all(models.join("sub")).unwrap();
    std::fs::write(web.join("index.html"), INDEX).unwrap();
    std::fs::write(web.join("favicon.svg"), "<svg/>").unwrap();
    std::fs::write(web.join("assets/index-abc123.js"), "console.log(1)").unwrap();
    std::fs::write(web.join("assets/editor_wasm_bg-abc.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(web.join("ort/ort-wasm-simd-threaded.jsep.mjs"), "export {}").unwrap();
    std::fs::write(web.join("models/fallback.onnx"), b"from-dist").unwrap();
    std::fs::write(root.join("secret.txt"), SECRET).unwrap();
    let model: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(models.join("sub/big.onnx"), &model).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("secret.txt"), models.join("link.onnx")).unwrap();

    let state = Arc::new(AppState::new(Some(WebSource::Dir(web)), models));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Fixture { _root: tmp, root, addr, state, client: reqwest::Client::new(), model }
}

/// Send `target` byte-for-byte. reqwest (via `url`) would normalise `..`
/// away before it left the client, which would test nothing.
async fn raw(addr: SocketAddr, target: &str, host: &str) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, text)
}

fn h<'a>(r: &'a reqwest::Response, name: &str) -> &'a str {
    r.headers().get(name).map(|v| v.to_str().unwrap()).unwrap_or("")
}

#[tokio::test]
async fn health_identifies_the_native_backend() {
    let f = start().await;
    let r = f.client.get(f.url("/api/health")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(h(&r, "content-type").starts_with("application/json"));
    let v: serde_json::Value = r.json().await.unwrap();
    assert_eq!(v["app"], "openphotoshop");
    assert_eq!(v["native"], true);
    assert_eq!(v["api"], 1);
    assert_eq!(v["web"], "dir");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert!(v["features"].as_array().unwrap().iter().any(|x| x == "open-file"));
}

#[tokio::test]
async fn isolation_headers_on_every_response() {
    let f = start().await;
    for path in ["/", "/api/health", "/assets/index-abc123.js", "/assets/nope.js", "/models/nope.onnx", "/api/nope"] {
        let r = f.client.get(f.url(path)).send().await.unwrap();
        assert_eq!(h(&r, "cross-origin-opener-policy"), "same-origin", "{path}");
        assert_eq!(h(&r, "cross-origin-embedder-policy"), "require-corp", "{path}");
        assert_eq!(h(&r, "x-content-type-options"), "nosniff", "{path}");
    }
}

#[tokio::test]
async fn mime_types_and_cache_policy() {
    let f = start().await;
    let get = |p: &str| f.client.get(f.url(p)).send();

    let r = get("/assets/editor_wasm_bg-abc.wasm").await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(h(&r, "content-type"), "application/wasm");
    assert_eq!(h(&r, "cache-control"), "public, max-age=31536000, immutable");

    let r = get("/assets/index-abc123.js").await.unwrap();
    assert_eq!(h(&r, "content-type"), "text/javascript; charset=utf-8");
    assert_eq!(h(&r, "cache-control"), "public, max-age=31536000, immutable");

    let r = get("/ort/ort-wasm-simd-threaded.jsep.mjs").await.unwrap();
    assert_eq!(h(&r, "content-type"), "text/javascript; charset=utf-8");
    assert_eq!(h(&r, "cache-control"), "no-cache");

    for p in ["/", "/index.html"] {
        let r = get(p).await.unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(h(&r, "content-type"), "text/html; charset=utf-8");
        assert_eq!(h(&r, "cache-control"), "no-cache");
        assert_eq!(r.text().await.unwrap(), INDEX);
    }

    let r = get("/models/sub/big.onnx").await.unwrap();
    assert_eq!(h(&r, "content-type"), "application/octet-stream");
    assert_eq!(h(&r, "accept-ranges"), "bytes");
}

#[tokio::test]
async fn etag_revalidation() {
    let f = start().await;
    let r = f.client.get(f.url("/favicon.svg")).send().await.unwrap();
    let etag = h(&r, "etag").to_string();
    assert!(etag.starts_with('"'));
    let r = f.client.get(f.url("/favicon.svg")).header(header::IF_NONE_MATCH, &etag).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(h(&r, "cross-origin-embedder-policy"), "require-corp");
}

#[tokio::test]
async fn spa_fallback_only_for_app_routes() {
    let f = start().await;
    for p in ["/pro", "/lite/editor", "/some/deep/route"] {
        let r = f.client.get(f.url(p)).send().await.unwrap();
        assert_eq!(r.status(), 200, "{p}");
        assert_eq!(h(&r, "cache-control"), "no-cache");
        assert_eq!(r.text().await.unwrap(), INDEX, "{p}");
    }
    for p in ["/assets/missing-xyz.js", "/assets/nofile", "/missing.css", "/ort/gone", "/models/missing.onnx", "/api/nope", "/api"] {
        let r = f.client.get(f.url(p)).send().await.unwrap();
        assert_eq!(r.status(), 404, "{p}");
        assert_ne!(r.text().await.unwrap(), INDEX, "{p}");
    }
    let r = f.client.post(f.url("/")).send().await.unwrap();
    assert_eq!(r.status(), 405);
}

#[tokio::test]
async fn token_hands_a_file_over_once() {
    let f = start().await;
    let photo = f.root.join("Holiday photo é.jpg");
    std::fs::write(&photo, b"jpeg-bytes").unwrap();
    let token = f.state.tokens.issue(photo.clone());

    // A cross-site request is refused and does not spend the token.
    let r = f.client.get(f.url(&format!("/api/file/{token}"))).header("sec-fetch-site", "cross-site").send().await.unwrap();
    assert_eq!(r.status(), 403);

    let r = f.client.get(f.url(&format!("/api/file/{token}"))).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(h(&r, "content-type"), "image/jpeg");
    assert_eq!(h(&r, "cache-control"), "no-store");
    assert_eq!(h(&r, "x-file-name"), "Holiday%20photo%20%C3%A9%2Ejpg");
    assert!(h(&r, "content-disposition").contains("filename*=UTF-8''Holiday%20photo%20%C3%A9%2Ejpg"));
    assert_eq!(h(&r, "cross-origin-embedder-policy"), "require-corp");
    assert_eq!(r.bytes().await.unwrap().as_ref(), b"jpeg-bytes");

    let r = f.client.get(f.url(&format!("/api/file/{token}"))).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::GONE);

    let r = f.client.get(f.url(&format!("/api/file/{}", "ab".repeat(32)))).send().await.unwrap();
    assert_eq!(r.status(), 404);
    let (status, body) = raw(f.addr, "/api/file/..%2f..%2fsecret.txt", "127.0.0.1").await;
    assert_eq!(status, 404);
    assert!(!body.contains(SECRET));
}

#[tokio::test]
async fn model_range_requests() {
    let f = start().await;
    let url = f.url("/models/sub/big.onnx");
    let len = f.model.len();

    let r = f.client.get(&url).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(h(&r, "content-length"), len.to_string());
    let etag = h(&r, "etag").to_string();
    assert_eq!(r.bytes().await.unwrap().as_ref(), &f.model[..]);

    let r = f.client.get(&url).header(header::RANGE, "bytes=1000-1999").send().await.unwrap();
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(h(&r, "content-range"), format!("bytes 1000-1999/{len}"));
    assert_eq!(h(&r, "content-length"), "1000");
    assert_eq!(r.bytes().await.unwrap().as_ref(), &f.model[1000..2000]);

    let r = f.client.get(&url).header(header::RANGE, "bytes=999000-").send().await.unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.bytes().await.unwrap().as_ref(), &f.model[999_000..]);

    let r = f.client.get(&url).header(header::RANGE, "bytes=-10").send().await.unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(h(&r, "content-range"), format!("bytes {}-{}/{len}", len - 10, len - 1));
    assert_eq!(r.bytes().await.unwrap().as_ref(), &f.model[len - 10..]);

    let r = f.client.get(&url).header(header::RANGE, format!("bytes={len}-")).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(h(&r, "content-range"), format!("bytes */{len}"));

    // A stale If-Range gets the whole, current file.
    let r = f.client.get(&url).header(header::RANGE, "bytes=0-9").header(header::IF_RANGE, "\"stale\"").send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = f.client.get(&url).header(header::RANGE, "bytes=0-9").header(header::IF_RANGE, &etag).send().await.unwrap();
    assert_eq!(r.status(), 206);

    let r = f.client.head(&url).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(h(&r, "content-length"), len.to_string());

    // Not in the models folder: falls back to the web build's models/.
    let r = f.client.get(f.url("/models/fallback.onnx")).header(header::RANGE, "bytes=5-").send().await.unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.text().await.unwrap(), "dist");
}

#[tokio::test]
async fn traversal_never_reaches_the_disk() {
    let f = start().await;
    let targets = [
        "/models/../secret.txt",
        "/models/../../etc/passwd",
        "/models/../../../../../../etc/passwd",
        "/models/%2e%2e/secret.txt",
        "/models/%2E%2E/%2E%2E/etc/passwd",
        "/models/..%2fsecret.txt",
        "/models/%2e%2e%2fsecret.txt",
        "/models/..%5csecret.txt",
        "/models/%252e%252e/secret.txt",
        "/models/sub/../../secret.txt",
        "/models//etc/passwd",
        "/models/%2fetc%2fpasswd",
        "/models/link.onnx",
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/assets/../../secret.txt",
        "/assets/%2e%2e%2f%2e%2e%2fsecret.txt",
        "/..%2f..%2fetc%2fpasswd",
        "/models%2f..%2f..%2fsecret.txt",
        "/.git/config",
        "/%c0%ae%c0%ae/secret.txt",
    ];
    for t in targets {
        let (status, body) = raw(f.addr, t, &f.addr.to_string()).await;
        assert!(matches!(status, 400 | 404), "{t} answered {status}");
        assert!(!body.contains(SECRET), "{t} leaked the file");
        assert!(!body.contains("root:"), "{t} leaked /etc/passwd");
        assert!(!body.contains(INDEX), "{t} fell back to the app shell");
    }
}

#[tokio::test]
async fn non_loopback_host_is_refused() {
    let f = start().await;
    let (status, _) = raw(f.addr, "/api/health", "evil.example.com").await;
    assert_eq!(status, 421);
    let (status, _) = raw(f.addr, "/api/health", "127.0.0.1.evil.example.com:80").await;
    assert_eq!(status, 421);
    for host in ["localhost:1234", "127.0.0.1", "[::1]:99", "LOCALHOST"] {
        let (status, _) = raw(f.addr, "/api/health", host).await;
        assert_eq!(status, 200, "{host}");
    }
}

#[tokio::test]
async fn models_listing_reports_installed() {
    let f = start().await;
    let manifest = format!(
        r#"{{"version":1,"models":[
            {{"id":"big","file":"sub/big.onnx","url":"https://example.com/big","sha256":"{}","size":1000000,"license":"MIT"}},
            {{"id":"absent","file":"absent.onnx","url":"https://example.com/a","sha256":"{}"}}]}}"#,
        "0".repeat(64),
        "1".repeat(64)
    );
    std::fs::write(f.state.models_dir.join("models.json"), manifest).unwrap();
    let v: serde_json::Value = f.client.get(f.url("/api/models")).send().await.unwrap().json().await.unwrap();
    assert_eq!(v["models"][0]["installed"], true);
    assert_eq!(v["models"][0]["url_path"], "/models/sub/big.onnx");
    assert_eq!(v["models"][1]["installed"], false);
    assert!(v["source"].as_str().unwrap().ends_with("models.json"));
}

async fn serve_bytes(bytes: &'static [u8]) -> SocketAddr {
    let app = axum::Router::new().route("/m.onnx", axum::routing::get(move || async move { bytes }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    addr
}

fn sha256_hex(b: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn no_part_files(dir: &Path) -> bool {
    walk(dir).iter().all(|p| p.extension().is_none_or(|e| e != "part"))
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[tokio::test]
async fn fetch_verifies_and_renames() {
    const BODY: &[u8] = b"pretend these are model weights";
    let addr = serve_bytes(BODY).await;
    let dir = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();

    let good = Manifest::parse(&format!(
        r#"{{"version":1,"models":[{{"id":"m","file":"nested/m.onnx","url":"http://{addr}/m.onnx","sha256":"{}","size":{}}}]}}"#,
        sha256_hex(BODY),
        BODY.len()
    ))
    .unwrap();
    let e = good.find("m").unwrap();
    assert_eq!(models::fetch(&client, e, dir.path(), false).await.unwrap(), Fetched::Downloaded);
    assert_eq!(std::fs::read(dir.path().join("nested/m.onnx")).unwrap(), BODY);
    assert!(no_part_files(dir.path()));
    assert!(models::installed(e, dir.path()));
    assert_eq!(models::fetch(&client, e, dir.path(), false).await.unwrap(), Fetched::AlreadyPresent);

    let bad = Manifest::parse(&format!(
        r#"{{"version":1,"models":[{{"id":"bad","file":"bad.onnx","url":"http://{addr}/m.onnx","sha256":"{}"}}]}}"#,
        "f".repeat(64)
    ))
    .unwrap();
    let err = models::fetch(&client, bad.find("bad").unwrap(), dir.path(), false).await.unwrap_err();
    assert!(format!("{err:#}").contains("sha256"), "{err:#}");
    assert!(!dir.path().join("bad.onnx").exists());
    assert!(no_part_files(dir.path()));

    let short = Manifest::parse(&format!(
        r#"{{"version":1,"models":[{{"id":"s","file":"s.onnx","url":"http://{addr}/m.onnx","sha256":"{}","size":5}}]}}"#,
        sha256_hex(BODY)
    ))
    .unwrap();
    assert!(models::fetch(&client, short.find("s").unwrap(), dir.path(), false).await.is_err());
    assert!(!dir.path().join("s.onnx").exists());
    assert!(no_part_files(dir.path()));
}

#[cfg(feature = "embed")]
#[tokio::test]
async fn embedded_build_serves_a_shell() {
    let state = Arc::new(AppState::new(WebSource::default_for_build(), tempfile::tempdir().unwrap().keep()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    let client = reqwest::Client::new();
    let r = client.get(format!("http://{addr}/")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.text().await.unwrap().to_lowercase().contains("<!doctype html>"));
    let v: serde_json::Value = client.get(format!("http://{addr}/api/health")).send().await.unwrap().json().await.unwrap();
    assert!(matches!(v["web"].as_str(), Some("embedded" | "placeholder")));
}
