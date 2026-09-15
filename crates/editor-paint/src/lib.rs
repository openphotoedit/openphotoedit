//! `paint.*` commands: the brush engine (brush, pencil, eraser, dodge, burn,
//! sponge, blur, sharpen, smudge, clone stamp, healing brush), the paint
//! bucket, the magic eraser and the gradient tool. Registered with the editor
//! by `register`.
//!
//! See [`stroke`] for how multi-segment strokes keep Photoshop's flow and
//! opacity semantics and undo as one step.

// `as_chunks` postdates the workspace's rust-version (1.85).
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod brush;
pub mod fill;
pub mod stroke;
pub mod surface;
pub mod tone;

use editor_core::document::Document;
use editor_core::editor::Editor;
use editor_core::ops::{parse, Applied, EditorError, Result};
use serde::Deserialize;
use serde_json::Value;

pub fn register(ed: &mut Editor) {
    ed.register_domain("paint", apply);
}

#[derive(Deserialize)]
struct EndCmd {
    #[serde(default)]
    stroke_id: Option<Value>,
}

pub fn apply(v: Value, doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
    match op.as_str() {
        "paint.stroke" => stroke::stroke(doc, parse(v)?),
        "paint.stroke-end" => {
            let c: EndCmd = parse(v)?;
            stroke::stroke_end(doc, &c.stroke_id)
        }
        "paint.fill" => fill::fill(doc, parse(v)?, false),
        "paint.magic-erase" => fill::fill(doc, parse(v)?, true),
        "paint.gradient" => fill::gradient(doc, parse(v)?),
        _ => Err(EditorError::UnknownOp(op)),
    }
}

#[cfg(test)]
mod tests;
