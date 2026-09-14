//! `transform.*` commands. Registered with the editor by `register`.

use editor_core::document::Document;
use editor_core::editor::Editor;
use editor_core::ops::{Applied, EditorError, Result};
use serde_json::Value;

pub fn register(ed: &mut Editor) {
    ed.register_domain("transform", apply);
}

pub fn apply(v: Value, _doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
    Err(EditorError::UnknownOp(op))
}
