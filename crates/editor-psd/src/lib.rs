//! editor-psd: Photoshop PSD and PSB, read and written.
//!
//! Import turns a file into an engine `Document` (layers, groups, masks,
//! adjustment and fill layers, type layers, smart objects, layer styles) and
//! keeps everything the engine does not model in a sidecar so the next
//! export can write it back. Export writes every layer kind plus a merged
//! composite rendered by `editor_core::render::flatten`, so readers that only
//! look at the composite show the right picture.

pub mod color;
pub mod compress;
pub mod descriptor;
pub mod effects;
pub mod engine_data;
pub mod error;
pub mod export;
pub mod import;
pub mod io;
pub mod kinds;
pub mod linked;
pub mod sidecar;
pub mod structure;
pub mod text;
pub mod vector;

pub use error::{PsdError, Result};
pub use export::{export, ExportOptions, Exported};
pub use import::{import, merged_rgba, ImportOptions, Imported};
