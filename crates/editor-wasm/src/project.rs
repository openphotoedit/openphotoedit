//! `project` entry points on `Engine`. See docs/architecture.md for who owns this file.

use wasm_bindgen::prelude::*;

use crate::Engine;

#[wasm_bindgen]
impl Engine {
    /// The current document as project bytes (`.opproj`): every layer and
    /// its pixels, losslessly. History is not included.
    pub fn save_project(&self) -> Result<Vec<u8>, JsError> {
        editor_project::save(&self.ed.doc).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Open project bytes as the current document. Returns the summary JSON
    /// with any warnings in `data.warnings`.
    pub fn load_project(&mut self, bytes: &[u8]) -> Result<String, JsError> {
        let loaded = editor_project::load(bytes, &editor_project::LoadOptions::default()).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(crate::psd::replace_document(self, loaded.doc, loaded.warnings))
    }
}
