//! The WebAssembly surface. One `Engine` lives in a dedicated worker; the UI
//! talks to it through `packages/engine`.
//!
//! Commands go in as JSON plus an optional byte payload. Pixel reads come
//! back as `Vec<u8>` (a fresh `Uint8Array` on the JS side). Domain-specific
//! entry points (PSD, AI helpers) live in their own modules as further
//! `impl Engine` blocks.

use editor_core::geom::Rect;
use editor_core::layer::{Layer, LayerId};
use editor_core::Document;
use editor_core::render::{render_layer_alone, render_view, to_u8, View};
use editor_core::Editor;
use wasm_bindgen::prelude::*;

mod ai;
mod encode;
mod project;
mod psd;
mod raw;

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
            serde_json::json!({ "id": id, "width": ed.doc.width, "height": ed.doc.height, "name": ed.doc.meta.source_name, "current": current, "layers": ed.doc.layer_count() })
        };
        let mut rows = vec![row(self.doc_id, &self.ed, true)];
        rows.extend(self.parked.iter().map(|(id, ed)| row(*id, ed, false)));
        serde_json::Value::Array(rows).to_string()
    }

    /// Copy layers from open document `from` into open document `to`, above
    /// the target's active layer, as one undo step there ("Copy Layers").
    /// Pixels, masks, effects, adjustment and fill layers, smart objects and
    /// whole groups come along; the source is untouched. Returns
    /// `{ids, count}` with the new top-level ids, bottom to top.
    pub fn copy_layers(&mut self, from: u32, to: u32, ids: Vec<u32>) -> Result<String, JsError> {
        if from == to {
            return Err(err("drop the layers on another document's tab"));
        }
        // A document clone shares every tile, so this costs almost nothing.
        let src = self.editor_of(from)?.doc.clone();
        let dst = self.editor_of_mut(to)?;
        let new_ids = copy_layers_into(&src, dst, &ids).map_err(err)?;
        Ok(serde_json::json!({ "ids": new_ids, "count": new_ids.len() }).to_string())
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

    /// Render a thumbnail of the document as it would look after `cmds` (a
    /// JSON array of commands), without touching the document or history.
    /// The trial runs on a clone that shares every tile, so it is cheap.
    /// Returns the `thumbnail` format, or an error naming the failing command.
    pub fn preview_thumbnail(&self, cmds: &str, size: u32) -> Result<Vec<u8>, JsError> {
        let list: Vec<serde_json::Value> = serde_json::from_str(cmds).map_err(err)?;
        let mut trial = Editor::new(1, 1);
        register_domains(&mut trial);
        trial.doc = self.ed.doc.clone();
        trial.history.max_steps = 0;
        for c in list {
            trial.exec(c, &[]).map_err(err)?;
        }
        let doc = &trial.doc;
        let s = (size as f64 / doc.width.max(doc.height) as f64).min(1.0);
        let w = ((doc.width as f64 * s).round() as usize).max(1);
        let h = ((doc.height as f64 * s).round() as usize).max(1);
        let f = render_view(doc, View { x: 0.0, y: 0.0, scale: s, width: w, height: h });
        let mut out = vec![0u8; 4 + f.len()];
        out[0..2].copy_from_slice(&(w as u16).to_le_bytes());
        out[2..4].copy_from_slice(&(h as u16).to_le_bytes());
        to_u8(&f, &mut out[4..]);
        Ok(out)
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

impl Engine {
    fn editor_of(&self, id: u32) -> Result<&Editor, JsError> {
        if id == self.doc_id {
            return Ok(&self.ed);
        }
        self.parked.iter().find(|(d, _)| *d == id).map(|(_, e)| e).ok_or_else(|| err(format!("no open document {id}")))
    }

    fn editor_of_mut(&mut self, id: u32) -> Result<&mut Editor, JsError> {
        if id == self.doc_id {
            return Ok(&mut self.ed);
        }
        self.parked.iter_mut().find(|(d, _)| *d == id).map(|(_, e)| e).ok_or_else(|| err(format!("no open document {id}")))
    }
}

fn fresh_ids(doc: &mut Document, l: &mut Layer) {
    l.id = doc.alloc_id();
    l.touch();
    if let Some(children) = l.children_mut() {
        for c in children {
            fresh_ids(doc, c);
        }
    }
}

/// See `Engine::copy_layers`. Layers whose ancestor is also listed come
/// along inside it. The copies keep their bottom-to-top order and their
/// placement relative to each other; when the canvases differ in size the
/// set is re-centred so it lands where it sat on the source canvas. A
/// clipped layer whose base is not copied is released rather than clipped
/// to whatever lies below it in the target.
fn copy_layers_into(src: &Document, dst: &mut Editor, ids: &[LayerId]) -> Result<Vec<LayerId>, String> {
    if ids.is_empty() {
        return Err("no layers to copy".into());
    }
    for id in ids {
        if src.find(*id).is_none() {
            return Err(format!("no layer {id}"));
        }
    }
    let mut picked: Vec<&Layer> = Vec::new();
    src.walk(&mut |l| {
        if ids.contains(&l.id) && !ids.iter().any(|a| *a != l.id && src.is_ancestor(*a, l.id)) {
            picked.push(l);
        }
    });
    let dx = (dst.doc.width as i32 - src.width as i32) / 2;
    let dy = (dst.doc.height as i32 - src.height as i32) / 2;
    let before = dst.doc.clone();
    let mut prev: Option<(editor_core::document::Slot, bool)> = None;
    let mut anchor = dst.doc.active;
    let mut out = Vec::new();
    for l in picked {
        let slot = src.slot_of(l.id).expect("picked layers exist");
        let mut c = l.clone();
        // A clip survives only when its base (or the clipped layer just
        // below it, whose own base survived) is copied right beneath it.
        let base_ok = prev.as_ref().is_some_and(|(p, ok)| *ok && p.parent == slot.parent && p.index + 1 == slot.index);
        if c.clip && !base_ok {
            c.clip = false;
        }
        prev = Some((slot, !l.clip || c.clip));
        fresh_ids(&mut dst.doc, &mut c);
        editor_core::ops::layer::offset_layer(&mut c, dx, dy);
        let id = c.id;
        dst.doc.insert_above(anchor, c);
        anchor = Some(id);
        out.push(id);
    }
    dst.doc.active = anchor;
    dst.history.record(&before, "Copy Layers", None);
    dst.revision += 1;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::layer::LayerKind;
    use serde_json::json;

    fn editor(w: u32, h: u32) -> Editor {
        let mut ed = Editor::new(1, 1);
        register_domains(&mut ed);
        ed.exec(json!({ "op": "doc.new", "width": w, "height": h, "background": { "r": 255, "g": 255, "b": 255 } }), &[]).unwrap();
        ed
    }

    fn add_red(ed: &mut Editor, name: &str, x: i32, y: i32) -> LayerId {
        let px: Vec<u8> = [255u8, 0, 0, 255].repeat(4 * 4);
        let r = ed.exec(json!({ "op": "layer.import", "width": 4, "height": 4, "x": x, "y": y, "name": name }), &px).unwrap();
        r["data"]["id"].as_u64().unwrap() as LayerId
    }

    fn names(doc: &Document) -> Vec<String> {
        let mut v = Vec::new();
        doc.walk(&mut |l| v.push(l.name.clone()));
        v
    }

    #[test]
    fn copies_layers_with_mask_effects_group_and_adjustment_as_one_step() {
        let mut src = editor(40, 30);
        let a = add_red(&mut src, "A", 2, 3);
        src.exec(json!({ "op": "layer.add-mask", "id": a, "from": "hide-all" }), &[]).unwrap();
        src.exec(json!({ "op": "layer.set-effects", "id": a, "effects": { "stroke": { "enabled": true } } }), &[]).unwrap();
        let b = add_red(&mut src, "B", 10, 10);
        let g = src.exec(json!({ "op": "layer.group", "ids": [b], "name": "G" }), &[]).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
        let adj = src.exec(json!({ "op": "layer.add-adjustment", "adjustment": { "kind": "invert" }, "name": "Inv" }), &[]).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
        let src_before = names(&src.doc);
        let src_undo = src.history.undo_labels().len();

        // Same size target with one background layer.
        let mut dst = editor(40, 30);
        let dst_undo = dst.history.undo_labels().len();
        let n0 = dst.doc.layer_count();
        let ids = copy_layers_into(&src.doc, &mut dst, &[adj, a, g, b]).unwrap();
        assert_eq!(ids.len(), 3, "b rides along inside its group");
        // A, G(B), Inv plus B inside the group.
        assert_eq!(dst.doc.layer_count(), n0 + 4);
        assert_eq!(dst.history.undo_labels().len(), dst_undo + 1);
        assert_eq!(dst.history.undo_labels().last().copied(), Some("Copy Layers"));
        let ca = dst.doc.find(ids[0]).unwrap();
        assert_eq!(ca.name, "A");
        assert!(ca.mask.is_some() && ca.effects.is_some());
        let LayerKind::Pixel(r) = &ca.kind else { panic!("pixel") };
        assert_eq!((r.x, r.y), (2, 3));
        let cg = dst.doc.find(ids[1]).unwrap();
        assert_eq!(cg.children().unwrap()[0].name, "B");
        assert_ne!(cg.children().unwrap()[0].id, b);
        assert!(matches!(dst.doc.find(ids[2]).unwrap().kind, LayerKind::Adjustment(_)));
        assert_eq!(dst.doc.active, Some(ids[2]));
        // Every id in the target is unique.
        let mut seen = std::collections::HashSet::new();
        dst.doc.walk(&mut |l| assert!(seen.insert(l.id)));
        // The source is untouched.
        assert_eq!(names(&src.doc), src_before);
        assert_eq!(src.history.undo_labels().len(), src_undo);
        // One undo takes the whole copy back.
        dst.exec(json!({ "op": "edit.undo" }), &[]).unwrap();
        assert_eq!(dst.doc.layer_count(), n0);
    }

    #[test]
    fn recentres_on_a_different_canvas_and_releases_orphan_clips() {
        let mut src = editor(20, 20);
        let base = add_red(&mut src, "Base", 0, 0);
        let c1 = add_red(&mut src, "Clip1", 1, 1);
        let c2 = add_red(&mut src, "Clip2", 2, 2);
        src.exec(json!({ "op": "layer.props", "id": c1, "clip": true }), &[]).unwrap();
        src.exec(json!({ "op": "layer.props", "id": c2, "clip": true }), &[]).unwrap();
        let mut dst = editor(60, 40);
        // Base copied: the chain stays clipped.
        let ids = copy_layers_into(&src.doc, &mut dst, &[base, c1, c2]).unwrap();
        assert!(!dst.doc.find(ids[0]).unwrap().clip);
        assert!(dst.doc.find(ids[1]).unwrap().clip && dst.doc.find(ids[2]).unwrap().clip);
        let LayerKind::Pixel(r) = &dst.doc.find(ids[0]).unwrap().kind else { panic!() };
        assert_eq!((r.x, r.y), (20, 10));
        // Base left behind: both clipped layers are released.
        let ids = copy_layers_into(&src.doc, &mut dst, &[c1, c2]).unwrap();
        assert!(!dst.doc.find(ids[0]).unwrap().clip && !dst.doc.find(ids[1]).unwrap().clip);
    }
}
