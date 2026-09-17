//! AI model files: where they live, what should be there, and fetching them.
//!
//! # The manifest (`models.json`)
//!
//! ```json
//! {
//!   "version": 1,
//!   "models": [
//!     {
//!       "id": "birefnet-lite",
//!       "file": "birefnet/birefnet_lite_fp32.onnx",
//!       "url": "https://huggingface.co/<org>/<repo>/resolve/<commit>/model.onnx",
//!       "sha256": "<64 hex chars>",
//!       "size": 176000000,
//!       "license": "MIT",
//!       "attribution": "BiRefNet, Zheng et al. 2024",
//!       "features": ["select-subject", "remove-background"],
//!       "tier": 1
//!     }
//!   ]
//! }
//! ```
//!
//! - `id`, `file`, `url`, `sha256` are required. `file` is a relative path
//!   under the models folder and is served at `/models/<file>`.
//! - `size`, `license`, `attribution`, `features`, `tier` are optional here;
//!   any other fields are kept and passed through `/api/models` untouched, so
//!   the UI can carry its own metadata without a server change.
//! - `url` must be `https`, except loopback `http` (tests, local mirrors).
//!
//! The manifest is looked for, first match wins: `--manifest`, then
//! `<models folder>/models.json`, then the web build's `models/models.json`,
//! then its `models.json`, then the empty manifest compiled into this crate.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::web::WebSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

const BUILTIN: &str = include_str!("../models.json");

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest> {
        let m: Manifest = serde_json::from_str(text).context("models.json is not valid")?;
        if m.version != 1 {
            bail!("models.json has version {}, this build understands version 1", m.version);
        }
        for e in &m.models {
            e.validate()?;
        }
        Ok(m)
    }

    pub fn builtin() -> Manifest {
        Manifest::parse(BUILTIN).expect("built-in models.json")
    }

    pub fn find(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }
}

impl ModelEntry {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            bail!("a model in models.json has an empty id");
        }
        if crate::paths::clean(&self.file).map(|s| s.is_empty()).unwrap_or(true) {
            bail!("model {}: `file` must be a plain relative path, got {:?}", self.id, self.file);
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("model {}: `sha256` must be 64 hex characters", self.id);
        }
        let loopback_http = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
            .iter()
            .any(|p| self.url.strip_prefix(p).is_some_and(|rest| rest.starts_with([':', '/'])));
        if !self.url.starts_with("https://") && !loopback_http {
            bail!("model {}: `url` must be https", self.id);
        }
        Ok(())
    }

    pub fn segments(&self) -> Vec<String> {
        crate::paths::clean(&self.file).expect("validated")
    }

    /// Where this model lives in `dir`.
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        let mut p = dir.to_path_buf();
        for s in self.segments() {
            p.push(s);
        }
        p
    }
}

/// The per-user models folder.
///
/// macOS `~/Library/Application Support/OpenPhotoEdit/models`, Linux
/// `$XDG_DATA_HOME/openphotoedit/models` (default `~/.local/share/…`), Windows
/// `%APPDATA%\OpenPhotoEdit\models`. `OPENPHOTOEDIT_MODELS_DIR` overrides it.
pub fn default_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("OPENPHOTOEDIT_MODELS_DIR").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let name = if cfg!(all(unix, not(target_os = "macos"))) { "openphotoedit" } else { "OpenPhotoEdit" };
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(name).join("models")
}

/// Find the manifest (see the module docs for the order) and say where it
/// came from.
pub async fn load_manifest(explicit: Option<&Path>, dir: &Path, web: Option<&WebSource>) -> Result<(Manifest, String)> {
    if let Some(p) = explicit {
        let text = std::fs::read_to_string(p).with_context(|| format!("could not read {}", p.display()))?;
        return Ok((Manifest::parse(&text)?, p.display().to_string()));
    }
    let local = dir.join("models.json");
    if local.is_file() {
        let text = std::fs::read_to_string(&local).with_context(|| format!("could not read {}", local.display()))?;
        return Ok((Manifest::parse(&text)?, local.display().to_string()));
    }
    if let Some(web) = web {
        for key in [&["models", "models.json"][..], &["models.json"][..]] {
            let segs: Vec<String> = key.iter().map(|s| s.to_string()).collect();
            if let Some(found) = web.get(&segs).await {
                let bytes = read_all(found.content).await?;
                let text = String::from_utf8(bytes).context("models.json is not UTF-8")?;
                return Ok((Manifest::parse(&text)?, format!("web:{}", segs.join("/"))));
            }
        }
    }
    Ok((Manifest::builtin(), "builtin".into()))
}

async fn read_all(content: crate::http::Content) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    Ok(match content {
        crate::http::Content::Bytes(b) => b.to_vec(),
        crate::http::Content::File { mut file, .. } => {
            let mut v = Vec::new();
            file.read_to_end(&mut v).await?;
            v
        }
    })
}

/// Whether a model's file is present in `dir`. Size is checked when the
/// manifest gives one; the hash is checked at fetch time, not on every list.
pub fn installed(entry: &ModelEntry, dir: &Path) -> bool {
    match std::fs::metadata(entry.path_in(dir)) {
        Ok(m) if m.is_file() => entry.size.is_none_or(|s| s == m.len()),
        _ => false,
    }
}

/// The manifest as `/api/models` returns it: every entry plus `installed`.
pub fn listing(manifest: &Manifest, dir: &Path) -> Value {
    let models: Vec<Value> = manifest
        .models
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).expect("serialisable");
            v["installed"] = Value::Bool(installed(e, dir));
            v["url_path"] = Value::String(format!("/models/{}", e.segments().join("/")));
            v
        })
        .collect();
    serde_json::json!({ "version": manifest.version, "models": models })
}

async fn sha256_file(path: &Path) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Fetched {
    Downloaded,
    AlreadyPresent,
}

/// Download one model into `dir`: stream to `<file>.part` while hashing,
/// check size and sha256, then rename into place. A mismatch deletes the
/// partial file and leaves any existing good copy alone.
pub async fn fetch(client: &reqwest::Client, entry: &ModelEntry, dir: &Path, progress: bool) -> Result<Fetched> {
    let dest = entry.path_in(dir);
    if dest.is_file() && sha256_file(&dest).await?.eq_ignore_ascii_case(&entry.sha256) {
        return Ok(Fetched::AlreadyPresent);
    }
    let parent = dest.parent().ok_or_else(|| anyhow!("model path has no parent"))?;
    tokio::fs::create_dir_all(parent).await.with_context(|| format!("could not create {}", parent.display()))?;
    let part = PathBuf::from(format!("{}.part", dest.display()));

    let result = download(client, entry, &part, progress).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    tokio::fs::rename(&part, &dest).await.with_context(|| format!("could not move the download into {}", dest.display()))?;
    Ok(Fetched::Downloaded)
}

async fn download(client: &reqwest::Client, entry: &ModelEntry, part: &Path, progress: bool) -> Result<()> {
    let resp = client.get(&entry.url).send().await.with_context(|| format!("could not reach {}", entry.url))?;
    if !resp.status().is_success() {
        bail!("{} answered {}", entry.url, resp.status());
    }
    let total = entry.size.or(resp.content_length());
    let mut file = tokio::fs::File::create(part).await.with_context(|| format!("could not create {}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut last_pct = u64::MAX;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("the download was interrupted")?;
        if let Some(limit) = entry.size {
            if got + chunk.len() as u64 > limit {
                bail!("{} is larger than the {} bytes models.json expects", entry.id, limit);
            }
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        got += chunk.len() as u64;
        if progress {
            if let Some(t) = total.filter(|t| *t > 0) {
                let pct = got * 100 / t;
                if pct / 5 != last_pct / 5 {
                    last_pct = pct;
                    eprint!("\r  {}: {pct:>3}%", entry.id);
                }
            }
        }
    }
    if progress && total.is_some() {
        eprintln!();
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    if let Some(s) = entry.size {
        if got != s {
            bail!("{}: downloaded {got} bytes, models.json expects {s}", entry.id);
        }
    }
    let actual = hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(&entry.sha256) {
        bail!("{}: sha256 is {actual}, models.json expects {}. The file was discarded.", entry.id, entry.sha256);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file: &str, url: &str) -> String {
        format!(r#"{{"version":1,"models":[{{"id":"m","file":"{file}","url":"{url}","sha256":"{}","note":"kept"}}]}}"#, "a".repeat(64))
    }

    #[test]
    fn validates_entries() {
        assert!(Manifest::parse(&entry("m/x.onnx", "https://example.com/x")).is_ok());
        assert!(Manifest::parse(&entry("m/x.onnx", "http://127.0.0.1:9/x")).is_ok());
        assert!(Manifest::parse(&entry("../x.onnx", "https://example.com/x")).is_err());
        assert!(Manifest::parse(&entry("x.onnx", "http://example.com/x")).is_err());
        assert!(Manifest::parse(&entry("x.onnx", "http://localhost.evil.com/x")).is_err());
        assert!(Manifest::parse(r#"{"version":2,"models":[]}"#).is_err());
        assert!(Manifest::builtin().models.is_empty());
    }

    #[test]
    fn passes_unknown_fields_through() {
        let m = Manifest::parse(&entry("x.onnx", "https://example.com/x")).unwrap();
        let v = listing(&m, Path::new("/nonexistent"));
        assert_eq!(v["models"][0]["note"], "kept");
        assert_eq!(v["models"][0]["installed"], false);
        assert_eq!(v["models"][0]["url_path"], "/models/x.onnx");
    }
}
