//! Where the web app's files come from: inside the binary, or a folder.

use std::path::PathBuf;

use crate::http::Found;
#[cfg(feature = "embed")]
use crate::http::Content;

/// The built web app, baked in at compile time. `build.rs` points this at
/// `apps/web/dist`, or at a placeholder page when that build is missing.
///
/// Model weights are left out: they are hundreds of megabytes, they are
/// fetched into the per-user models folder instead, and a binary that grows
/// by whatever happened to be in `public/models` is a silent trap. Source
/// maps are left out for size.
#[cfg(feature = "embed")]
#[derive(rust_embed::RustEmbed)]
#[folder = "$OPENPHOTOEDIT_EMBED_DIR"]
#[exclude = "*.map"]
#[exclude = "*.onnx"]
#[exclude = "*.ort"]
#[exclude = "*.onnx_data"]
#[exclude = "*.safetensors"]
#[exclude = "*.gguf"]
#[exclude = "*.bin"]
struct Embedded;

#[derive(Debug, Clone)]
pub enum WebSource {
    /// Files compiled into the binary.
    #[cfg(feature = "embed")]
    Embedded,
    /// A build folder on disk, for development (`--dir`).
    Dir(PathBuf),
}

impl WebSource {
    /// The default for this build: embedded when compiled in.
    pub fn default_for_build() -> Option<WebSource> {
        #[cfg(feature = "embed")]
        {
            Some(WebSource::Embedded)
        }
        #[cfg(not(feature = "embed"))]
        {
            None
        }
    }

    /// How `/api/health` describes this source.
    pub fn describe(&self) -> &'static str {
        match self {
            #[cfg(feature = "embed")]
            WebSource::Embedded => {
                if env!("OPENPHOTOEDIT_EMBED_PLACEHOLDER") == "1" {
                    "placeholder"
                } else {
                    "embedded"
                }
            }
            WebSource::Dir(_) => "dir",
        }
    }

    /// Look up a file by its cleaned segments.
    pub async fn get(&self, segments: &[String]) -> Option<Found> {
        match self {
            #[cfg(feature = "embed")]
            WebSource::Embedded => {
                let file = Embedded::get(&crate::paths::join_key(segments))?;
                let etag = format!("\"{}\"", hex(&file.metadata.sha256_hash()[..16]));
                let modified = file.metadata.last_modified().map(|s| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s));
                let bytes = match file.data {
                    std::borrow::Cow::Borrowed(b) => bytes::Bytes::from_static(b),
                    std::borrow::Cow::Owned(v) => bytes::Bytes::from(v),
                };
                Some(Found { content: Content::Bytes(bytes), etag, modified })
            }
            WebSource::Dir(dir) => {
                let path = crate::paths::resolve_under(dir, segments)?;
                Found::open(&path).await.ok()
            }
        }
    }
}

#[cfg(feature = "embed")]
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
