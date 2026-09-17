//! Camera RAW entry points on `Engine`. See docs/architecture.md for who owns
//! this file; the maths lives in `crates/editor-raw`.
//!
//! One decoded file is cached between calls (keyed by a fingerprint of its
//! bytes), so slider changes only re-run the develop stage. Every method
//! that takes `bytes` accepts an empty array to mean "the cached file".
//!
//! Byte layouts returned to JS (all integers little-endian):
//! - `raw_preview`: `[kind u8 (1 = JPEG, 2 = RGBA), orientation u8, 0, 0,
//!   width u32, height u32, payload…]`. A JPEG payload is the camera's
//!   embedded preview, stored unrotated: apply `orientation` (EXIF 1..8)
//!   when drawing it. RGBA payloads are already oriented (orientation 1).
//! - `raw_develop`: `[width u32, height u32, rgba…]`.

use std::cell::RefCell;

use editor_raw::{preview, DevelopParams, Session, Size};
use wasm_bindgen::prelude::*;

use crate::{err, Engine};

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Run `f` on the session for `bytes`, decoding (and replacing the cache) if
/// the bytes are a different file.
fn with_session<T>(bytes: &[u8], f: impl FnOnce(&mut Session) -> T) -> Result<T, JsError> {
    SESSION.with(|cell| {
        let mut slot = cell.borrow_mut();
        let key = (!bytes.is_empty()).then(|| editor_raw::fingerprint(bytes));
        let stale = match (&*slot, key) {
            (None, _) => true,
            (Some(s), Some(k)) => s.key != k,
            (Some(_), None) => false,
        };
        if stale {
            if bytes.is_empty() {
                return Err(err("No RAW file is loaded. Open the file again."));
            }
            // Free the old file before decoding the new one.
            *slot = None;
            *slot = Some(Session::open(bytes).map_err(err)?);
        }
        Ok(f(slot.as_mut().expect("session present")))
    })
}

fn parse_params(json: &str) -> Result<(DevelopParams, Option<usize>), JsError> {
    if json.trim().is_empty() {
        return Ok((DevelopParams::default(), None));
    }
    let v: serde_json::Value = serde_json::from_str(json).map_err(err)?;
    let max = v.get("max_size").and_then(|m| m.as_u64()).map(|m| m as usize);
    let p: DevelopParams = serde_json::from_value(v).map_err(err)?;
    Ok((p, max))
}

fn with_header(width: usize, height: usize, rgba: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + rgba.len());
    out.extend_from_slice(&(width as u32).to_le_bytes());
    out.extend_from_slice(&(height as u32).to_le_bytes());
    out.extend_from_slice(&rgba);
    out
}

#[wasm_bindgen]
impl Engine {
    /// Camera and exposure metadata without decoding pixels:
    /// `{make, model, width, height, iso, exposure, aperture, focal, lens,
    /// as_shot_wb: {temperature, tint, multipliers}, orientation, sensor}`.
    pub fn raw_probe(&self, bytes: &[u8]) -> Result<String, JsError> {
        let info = editor_raw::probe(bytes).map_err(err)?;
        serde_json::to_string(&info).map_err(err)
    }

    /// A fast first image: the embedded JPEG when there is one at least
    /// `min(max_size, 1024)` on its long side, else a default develop fitted
    /// into `max_size` (this decodes and caches the file).
    pub fn raw_preview(&self, bytes: &[u8], max_size: u32) -> Result<Vec<u8>, JsError> {
        let want = max_size.clamp(64, 1024);
        if let Some(j) = preview::best_jpeg(bytes, want).filter(|j| j.width.max(j.height) >= want) {
            let orientation = editor_raw::probe(bytes).map(|i| i.orientation).unwrap_or(1);
            let mut out = Vec::with_capacity(12 + j.len);
            out.extend_from_slice(&[1, orientation, 0, 0]);
            out.extend_from_slice(&j.width.to_le_bytes());
            out.extend_from_slice(&j.height.to_le_bytes());
            out.extend_from_slice(&bytes[j.offset..j.offset + j.len]);
            return Ok(out);
        }
        let o = with_session(bytes, |s| s.develop_at(&DevelopParams::default(), Size::Fit(max_size.max(64) as usize)))?;
        let mut out = Vec::with_capacity(12 + o.rgba.len());
        out.extend_from_slice(&[2, 1, 0, 0]);
        out.extend_from_slice(&(o.width as u32).to_le_bytes());
        out.extend_from_slice(&(o.height as u32).to_le_bytes());
        out.extend_from_slice(&o.rgba);
        Ok(out)
    }

    /// Develop with `params_json` (see `editor_raw::DevelopParams`; an extra
    /// `max_size` caps the long side of a `half` develop). `half` uses 2×2
    /// (Bayer) or 3×3 (X-Trans) superpixels; otherwise full resolution.
    pub fn raw_develop(&self, bytes: &[u8], params_json: &str, half: bool) -> Result<Vec<u8>, JsError> {
        let (p, max) = parse_params(params_json)?;
        let size = match (half, max) {
            (false, _) => Size::Full,
            (true, Some(m)) => Size::Fit(m),
            (true, None) => Size::Half,
        };
        let o = with_session(bytes, |s| s.develop_at(&p, size))?;
        Ok(with_header(o.width, o.height, o.rgba))
    }

    /// The white balance `params_json` resolves to (for "as-shot", "auto" and
    /// presets): `{temperature, tint, multipliers}`.
    pub fn raw_white_balance(&self, bytes: &[u8], params_json: &str) -> Result<String, JsError> {
        let (p, _) = parse_params(params_json)?;
        let wb = with_session(bytes, |s| s.white_balance(&p))?;
        serde_json::to_string(&wb).map_err(err)
    }

    /// Develop at full resolution and open the result as a new document via
    /// `doc.open-pixels`. Frees the RAW cache. Returns the summary JSON.
    pub fn raw_open(&mut self, bytes: &[u8], params_json: &str, name: &str) -> Result<String, JsError> {
        let (p, _) = parse_params(params_json)?;
        let o = with_session(bytes, |s| s.develop_at(&p, Size::Full))?;
        SESSION.with(|c| *c.borrow_mut() = None);
        let cmd = serde_json::json!({ "op": "doc.open-pixels", "width": o.width, "height": o.height, "name": name });
        self.ed.exec_json(&cmd.to_string(), &o.rgba).map_err(err)?;
        Ok(self.ed.summary().to_string())
    }

    /// Drop the cached RAW file (the dialog was cancelled).
    pub fn raw_close(&self) {
        SESSION.with(|c| *c.borrow_mut() = None);
    }
}
