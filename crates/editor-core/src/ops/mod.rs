//! Commands. Every edit the UI, a script or the AI planner makes is one of
//! these, sent as JSON with an `op` field namespaced by domain:
//! `layer.*`, `image.*`, `select.*`, `filter.*`, `paint.*`, `ai.*`.
//!
//! The core domains (`doc`, `layer`, `image`, `select`) live here. Other
//! crates (filters, paint, selection algorithms, transforms) register a
//! [`DomainHandler`] with the editor for their prefix; `Editor::exec` routes
//! by the prefix and falls back to registered handlers when a core domain
//! does not recognise an operation.

pub mod image;
pub mod layer;
pub mod select;

/// A command handler for one domain prefix. Returns
/// `Err(EditorError::UnknownOp)` for operations it does not handle.
pub type DomainHandler = fn(serde_json::Value, &mut Document, &[u8]) -> Result<Applied>;

use crate::document::Document;
use crate::geom::Rect;
use crate::layer::{Layer, LayerId};

#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("invalid command: {0}")]
    Json(String),
    #[error("unknown operation `{0}`")]
    UnknownOp(String),
    #[error("no layer with id {0}")]
    NoLayer(LayerId),
    #[error("{0}")]
    Invalid(String),
    #[error("layer `{0}` is locked")]
    Locked(String),
}

pub type Result<T> = std::result::Result<T, EditorError>;

/// What a command did, for history and for the UI.
#[derive(Debug, Default)]
pub struct Applied {
    /// Shown in the History panel.
    pub label: String,
    /// Consecutive commands sharing a key collapse into one undo step.
    pub merge_key: Option<String>,
    /// False for commands that change no document state worth undoing
    /// (setting the active layer).
    pub undoable: bool,
    /// Document area that changed; `None` means "assume everything".
    pub dirty: Option<Rect>,
    /// Command-specific result data returned to the caller.
    pub data: Option<serde_json::Value>,
}

impl Applied {
    pub fn step(label: impl Into<String>) -> Applied {
        Applied { label: label.into(), undoable: true, ..Default::default() }
    }
    pub fn quiet() -> Applied {
        Applied { undoable: false, ..Default::default() }
    }
    pub fn merge(mut self, key: impl Into<String>) -> Applied {
        self.merge_key = Some(key.into());
        self
    }
    pub fn with_data(mut self, data: serde_json::Value) -> Applied {
        self.data = Some(data);
        self
    }
    pub fn dirty(mut self, r: Rect) -> Applied {
        self.dirty = Some(r);
        self
    }
}

pub fn parse<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> Result<T> {
    serde_json::from_value(v).map_err(|e| EditorError::Json(e.to_string()))
}

pub fn layer<'a>(doc: &'a Document, id: LayerId) -> Result<&'a Layer> {
    doc.find(id).ok_or(EditorError::NoLayer(id))
}

pub fn layer_mut<'a>(doc: &'a mut Document, id: LayerId) -> Result<&'a mut Layer> {
    doc.find_mut(id).ok_or(EditorError::NoLayer(id))
}

/// The layer a command without an explicit id acts on.
pub fn target_id(doc: &Document, id: Option<LayerId>) -> Result<LayerId> {
    id.or(doc.active).ok_or_else(|| EditorError::Invalid("no layer selected".into()))
}
