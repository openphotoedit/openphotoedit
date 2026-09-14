//! The editor session: a document, its history and a render cache, driven by
//! JSON commands.

use serde_json::{json, Value};

use crate::document::Document;
use crate::ops::{self, Applied, EditorError, Result};
use crate::render::{Renderer, View};
use crate::summary;

pub struct Editor {
    pub doc: Document,
    pub history: crate::history::History,
    renderer: Renderer,
    /// Increments on every change, so the UI can tell whether its cached
    /// summary is current.
    pub revision: u64,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new(1, 1)
    }
}

impl Editor {
    pub fn new(width: u32, height: u32) -> Editor {
        Editor { doc: Document::new(width, height), history: Default::default(), renderer: Renderer::new(), revision: 0 }
    }

    /// Run a command given as JSON text. `bytes` carries pixel payloads.
    pub fn exec_json(&mut self, text: &str, bytes: &[u8]) -> Result<Value> {
        let v: Value = serde_json::from_str(text).map_err(|e| EditorError::Json(e.to_string()))?;
        self.exec(v, bytes)
    }

    pub fn exec(&mut self, v: Value, bytes: &[u8]) -> Result<Value> {
        let op = v.get("op").and_then(Value::as_str).ok_or_else(|| EditorError::Json("missing `op`".into()))?.to_string();

        // History navigation is not itself recorded.
        match op.as_str() {
            "edit.undo" => {
                let label = self.history.undo(&mut self.doc);
                return Ok(self.finish_nav(label));
            }
            "edit.redo" => {
                let label = self.history.redo(&mut self.doc);
                return Ok(self.finish_nav(label));
            }
            "edit.history-go" => {
                let n = v.get("index").and_then(Value::as_u64).ok_or_else(|| EditorError::Json("missing `index`".into()))?;
                self.history.go_to(n as usize, &mut self.doc);
                return Ok(self.finish_nav(Some("history".into())));
            }
            "edit.seal" => {
                self.history.seal();
                return Ok(json!({ "changed": false }));
            }
            "edit.clear-history" => {
                self.history.clear();
                return Ok(json!({ "changed": false }));
            }
            _ => {}
        }

        let before = self.doc.clone();
        let domain = op.split('.').next().unwrap_or("");
        let applied: Result<Applied> = match domain {
            "doc" | "layer" => ops::layer::apply(v, &mut self.doc, bytes),
            "image" => ops::image::apply(v, &mut self.doc, bytes),
            "select" => ops::select::apply(v, &mut self.doc, bytes),
            "filter" => ops::parse::<ops::filter::Cmd>(v).and_then(|c| ops::filter::apply(c, &mut self.doc, bytes)),
            "paint" => ops::parse::<ops::paint::Cmd>(v).and_then(|c| ops::paint::apply(c, &mut self.doc, bytes)),
            "ai" => ops::parse::<ops::ai::Cmd>(v).and_then(|c| ops::ai::apply(c, &mut self.doc, bytes)),
            _ => Err(EditorError::UnknownOp(op.clone())),
        };
        let applied = match applied {
            Ok(a) => a,
            Err(e) => {
                // Commands validate before mutating where they can; restore
                // anyway so a half-applied failure never leaks.
                self.doc = before;
                return Err(e);
            }
        };
        if applied.undoable {
            self.history.record(&before, &applied.label, applied.merge_key.as_deref());
            if op.starts_with("doc.") {
                // A new document starts a fresh history.
                self.history.clear();
                self.renderer.invalidate();
            }
            self.revision += 1;
        } else {
            self.revision += 1;
        }
        Ok(json!({
            "changed": applied.undoable,
            "label": applied.label,
            "dirty": applied.dirty,
            "data": applied.data,
            "revision": self.revision,
        }))
    }

    fn finish_nav(&mut self, label: Option<String>) -> Value {
        let changed = label.is_some();
        if changed {
            self.revision += 1;
        }
        json!({ "changed": changed, "label": label, "revision": self.revision })
    }

    /// Render a view into straight RGBA8.
    pub fn render(&mut self, view: View, out: &mut [u8]) {
        self.renderer.render(&self.doc, view, out);
    }

    pub fn summary(&self) -> Value {
        summary::document(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_undo_redo_round_trip() {
        let mut ed = Editor::new(1, 1);
        ed.exec_json(r#"{"op":"doc.new","width":20,"height":10,"background":{"r":255,"g":255,"b":255}}"#, &[]).unwrap();
        ed.exec_json(r#"{"op":"layer.add-adjustment","adjustment":{"kind":"invert"}}"#, &[]).unwrap();
        let mut out = vec![0u8; 20 * 10 * 4];
        let view = View { x: 0.0, y: 0.0, scale: 1.0, width: 20, height: 10 };
        ed.render(view, &mut out);
        assert_eq!(out[0], 0);
        ed.exec_json(r#"{"op":"edit.undo"}"#, &[]).unwrap();
        ed.render(view, &mut out);
        assert_eq!(out[0], 255);
        ed.exec_json(r#"{"op":"edit.redo"}"#, &[]).unwrap();
        ed.render(view, &mut out);
        assert_eq!(out[0], 0);
        let s = ed.summary();
        assert_eq!(s["layers"].as_array().unwrap().len(), 2);
        assert_eq!(s["history"]["undo"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn failed_command_leaves_document_untouched() {
        let mut ed = Editor::new(1, 1);
        ed.exec_json(r#"{"op":"doc.new","width":4,"height":4}"#, &[]).unwrap();
        let err = ed.exec_json(r#"{"op":"layer.merge-down","id":999}"#, &[]);
        assert!(err.is_err());
        assert_eq!(ed.doc.layer_count(), 1);
        assert!(ed.exec_json(r#"{"op":"nope.nothing"}"#, &[]).is_err());
    }
}
