//! `psd` entry points on `Engine`. See docs/architecture.md for who owns this file.

use editor_core::document::Document;
use wasm_bindgen::prelude::*;

use crate::Engine;

/// Replace the current document with `doc` and return the summary JSON with
/// the warnings in `data.warnings`.
pub(crate) fn replace_document(engine: &mut Engine, doc: Document, warnings: Vec<String>) -> String {
    engine.ed.doc = doc;
    engine.ed.history.clear();
    engine.ed.revision += 1;
    let mut summary = engine.ed.summary();
    if let Some(o) = summary.as_object_mut() {
        o.insert("data".into(), serde_json::json!({ "warnings": warnings }));
    }
    summary.to_string()
}

#[wasm_bindgen]
impl Engine {
    /// Open a PSD or PSB file as the current document. Returns the summary
    /// JSON; anything that could not be shown faithfully is listed in
    /// `data.warnings`.
    pub fn import_psd(&mut self, bytes: &[u8]) -> Result<String, JsError> {
        let imported = editor_psd::import(bytes, &editor_psd::ImportOptions::default()).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(replace_document(self, imported.doc, imported.warnings))
    }

    /// Save the current document as PSD (or PSB). `options` is JSON:
    /// `{"psb": false, "max_compat": true}`; both are optional.
    pub fn export_psd(&self, options: &str) -> Result<Vec<u8>, JsError> {
        let opts: editor_psd::ExportOptions = if options.trim().is_empty() { Default::default() } else { serde_json::from_str(options).map_err(|e| JsError::new(&format!("export options: {e}")))? };
        let exported = editor_psd::export(&self.ed.doc, &opts).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(exported.bytes)
    }
}
