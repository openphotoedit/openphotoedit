//! The WebAssembly surface. One `Engine` lives in a dedicated worker; the UI
//! talks to it through `packages/engine`.
//!
//! Commands go in as JSON plus an optional byte payload. Pixel reads come
//! back as `Vec<u8>` (a fresh `Uint8Array` on the JS side). Domain-specific
//! entry points (PSD, AI helpers) live in their own modules as further
//! `impl Engine` blocks.

use editor_core::geom::Rect;
use editor_core::layer::LayerId;
use editor_core::render::{render_layer_alone, render_view, to_u8, View};
use editor_core::Editor;
use wasm_bindgen::prelude::*;

mod ai;
mod encode;
mod project;
mod psd;

/// Every domain crate, registered once per engine.
fn register_domains(ed: &mut Editor) {
    editor_filters::register(ed);
    editor_paint::register(ed);
    editor_select::register(ed);
    editor_transform::register(ed);
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct Engine {
    /// The document every call acts on. Other open documents wait in
    /// `parked` and are swapped in by `switch_document`.
    pub(crate) ed: Editor,
    pub(crate) doc_id: u32,
    parked: Vec<(u32, Editor)>,
    next_doc: u32,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Engine {
        let mut ed = Editor::new(1, 1);
        register_domains(&mut ed);
        Engine { ed, doc_id: 1, parked: Vec::new(), next_doc: 2 }
    }

    /// Open a fresh, empty document alongside the others and make it
    /// current. Returns its id; the caller fills it with `doc.new` or
    /// `doc.open-pixels`.
    pub fn new_document(&mut self) -> u32 {
        let mut ed = Editor::new(1, 1);
        register_domains(&mut ed);
        let id = self.next_doc;
        self.next_doc += 1;
        let old = std::mem::replace(&mut self.ed, ed);
        self.parked.push((self.doc_id, old));
        self.doc_id = id;
        id
    }

    /// Make another open document current.
    pub fn switch_document(&mut self, id: u32) -> Result<(), JsError> {
        if id == self.doc_id {
            return Ok(());
        }
        let pos = self.parked.iter().position(|(d, _)| *d == id).ok_or_else(|| err(format!("no open document {id}")))?;
        let (_, ed) = self.parked.remove(pos);
        let old = std::mem::replace(&mut self.ed, ed);
        self.parked.push((self.doc_id, old));
        self.doc_id = id;
        Ok(())
    }

    /// Close a document. Closing the current one switches to the most
    /// recently used other document (or a blank one). Returns the new
    /// current id.
    pub fn close_document(&mut self, id: u32) -> Result<u32, JsError> {
        if id != self.doc_id {
            let pos = self.parked.iter().position(|(d, _)| *d == id).ok_or_else(|| err(format!("no open document {id}")))?;
            self.parked.remove(pos);
            return Ok(self.doc_id);
        }
        match self.parked.pop() {
            Some((next, ed)) => {
                self.ed = ed;
                self.doc_id = next;
            }
            None => {
                let mut ed = Editor::new(1, 1);
                register_domains(&mut ed);
                self.ed = ed;
            }
        }
        Ok(self.doc_id)
    }

    pub fn current_document(&self) -> u32 {
        self.doc_id
    }

    /// `[{id, width, height, name, current}]` for every open document.
    pub fn documents(&self) -> String {
        let row = |id: u32, ed: &Editor, current: bool| {
            serde_json::json!({ "id": id, "width": ed.doc.width, "height": ed.doc.height, "name": ed.doc.meta.source_name, "current": current })
        };
        let mut rows = vec![row(self.doc_id, &self.ed, true)];
        rows.extend(self.parked.iter().map(|(id, ed)| row(*id, ed, false)));
        serde_json::Value::Array(rows).to_string()
    }

    /// Run a command. Returns the result JSON.
    pub fn exec(&mut self, cmd: &str, bytes: &[u8]) -> Result<String, JsError> {
        let v = self.ed.exec_json(cmd, bytes).map_err(err)?;
        Ok(v.to_string())
    }

    /// Leave these layers out of viewport renders (tool previews). Pass an
    /// empty array to clear. Exports and document state are unaffected.
    pub fn set_preview_hidden(&mut self, ids: Vec<u32>) {
        self.ed.preview_hidden = ids;
    }

    /// The layer tree, selection bounds and history as JSON.
    pub fn summary(&self) -> String {
        self.ed.summary().to_string()
    }

    pub fn width(&self) -> u32 {
        self.ed.doc.width
    }

    pub fn height(&self) -> u32 {
        self.ed.doc.height
    }

    /// Straight RGBA8 of the document region starting at `(x, y)` in
    /// document pixels, at `scale` output pixels per document pixel.
    pub fn render(&mut self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Vec<u8> {
        let mut out = vec![0u8; width as usize * height as usize * 4];
        if width == 0 || height == 0 || scale <= 0.0 {
            return out;
        }
        let view = View { x, y, scale, width: width as usize, height: height as usize };
        self.ed.render(view, &mut out);
        out
    }

    /// The whole document composited at full resolution.
    pub fn flatten(&self) -> Vec<u8> {
        editor_core::render::flatten(&self.ed.doc)
    }

    /// Composite of a document rectangle at 1:1 (merged), for sampling.
    pub fn region(&self, x: i32, y: i32, width: u32, height: u32) -> Vec<u8> {
        let view = View { x: x as f64, y: y as f64, scale: 1.0, width: width as usize, height: height as usize };
        let f = render_view(&self.ed.doc, view);
        let mut out = vec![0u8; f.len()];
        to_u8(&f, &mut out);
        out
    }

    /// One layer's own pixels over a document rectangle at 1:1, ignoring its
    /// opacity and blend mode.
    pub fn layer_region(&self, id: LayerId, x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
        let layer = self.ed.doc.find(id).ok_or_else(|| err(format!("no layer {id}")))?;
        let view = View { x: x as f64, y: y as f64, scale: 1.0, width: width as usize, height: height as usize };
        let f = render_layer_alone(layer, &self.ed.doc, view);
        let mut out = vec![0u8; f.len()];
        to_u8(&f, &mut out);
        Ok(out)
    }

    /// A layer thumbnail fitted into `size`×`size`, letterboxed by the
    /// document's aspect ratio. Returns `[w_lo, w_hi, h_lo, h_hi, ...rgba]`.
    pub fn thumbnail(&self, id: Option<LayerId>, size: u32) -> Vec<u8> {
        let doc = &self.ed.doc;
        let s = (size as f64 / doc.width.max(doc.height) as f64).min(1.0);
        let w = ((doc.width as f64 * s).round() as usize).max(1);
        let h = ((doc.height as f64 * s).round() as usize).max(1);
        let view = View { x: 0.0, y: 0.0, scale: s, width: w, height: h };
        let f = match id.and_then(|i| doc.find(i)) {
            Some(l) => render_layer_alone(l, doc, view),
            None => render_view(doc, view),
        };
        let mut out = vec![0u8; 4 + f.len()];
        out[0..2].copy_from_slice(&(w as u16).to_le_bytes());
        out[2..4].copy_from_slice(&(h as u16).to_le_bytes());
        to_u8(&f, &mut out[4..]);
        out
    }

    /// Layer mask pixels (single channel) over a document rectangle.
    pub fn mask_region(&self, id: LayerId, x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>, JsError> {
        let layer = self.ed.doc.find(id).ok_or_else(|| err(format!("no layer {id}")))?;
        let m = layer.mask.as_ref().ok_or_else(|| err("layer has no mask"))?;
        let r = Rect::new(x - m.raster.x, y - m.raster.y, width as i32, height as i32);
        Ok(m.raster.plane.read_vec(r))
    }

    /// A layer mask fitted into `size`×`size` over the document's aspect
    /// ratio, single channel, prefixed like `thumbnail` with
    /// `[w_lo, w_hi, h_lo, h_hi]`.
    pub fn mask_thumbnail(&self, id: LayerId, size: u32) -> Result<Vec<u8>, JsError> {
        let doc = &self.ed.doc;
        let layer = doc.find(id).ok_or_else(|| err(format!("no layer {id}")))?;
        let m = layer.mask.as_ref().ok_or_else(|| err("layer has no mask"))?;
        let s = (size as f64 / doc.width.max(doc.height) as f64).min(1.0);
        let w = ((doc.width as f64 * s).round() as usize).max(1);
        let h = ((doc.height as f64 * s).round() as usize).max(1);
        let mut f = vec![0f32; w * h];
        m.raster.plane.resample(-m.raster.x as f64, -m.raster.y as f64, 1.0 / s, w, h, &mut f);
        let mut out = vec![0u8; 4 + w * h];
        out[0..2].copy_from_slice(&(w as u16).to_le_bytes());
        out[2..4].copy_from_slice(&(h as u16).to_le_bytes());
        for (o, v) in out[4..].iter_mut().zip(f.iter()) {
            *o = (v * 255.0 + 0.5) as u8;
        }
        Ok(out)
    }

    /// Selection coverage (single channel) resampled like `render`, for the
    /// marching-ants overlay. Empty when nothing is selected.
    pub fn selection_view(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Vec<u8> {
        let Some(sel) = &self.ed.doc.selection else { return Vec::new() };
        let (w, h) = (width as usize, height as usize);
        let mut f = vec![0f32; w * h];
        sel.resample(x, y, 1.0 / scale, w, h, &mut f);
        f.iter().map(|v| (v * 255.0 + 0.5) as u8).collect()
    }

    /// Selection coverage at 1:1 over a document rectangle.
    pub fn selection_region(&self, x: i32, y: i32, width: u32, height: u32) -> Vec<u8> {
        match &self.ed.doc.selection {
            None => vec![255; width as usize * height as usize],
            Some(sel) => sel.read_vec(Rect::new(x, y, width as i32, height as i32)),
        }
    }

    /// Encode straight RGBA8 as PNG with the document's resolution.
    pub fn encode_png(&self, rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsError> {
        encode::png(rgba, width, height, self.ed.doc.resolution).map_err(err)
    }
}
