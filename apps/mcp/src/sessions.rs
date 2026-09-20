//! Open documents that live between tool calls.
//!
//! A 24 megapixel JPEG costs the better part of a second to decode and 96 MB
//! to hold. Doing that again for every slider an agent nudges is wasteful and
//! it also throws away the thing that makes this engine worth driving: the
//! history. `open_document` once, then `apply_operations`, `undo`,
//! `apply_operations` again — that is the shape of real editing, and it is
//! the shape the engine was built for.
//!
//! The one-shot tools do not need any of this. They open, edit, write and
//! forget, and they keep working when no session has ever been opened.
//!
//! **The caps.** At most [`MAX_SESSIONS`] documents open at once and at most
//! [`MAX_TOTAL_PIXELS`] pixels across all of them, counted as
//! width × height (history snapshots share tiles, so the document itself is
//! the honest unit). Past either, `open_document` refuses and says which cap
//! was hit and what to close. An agent that forgets to close things should
//! get an error, not a machine that swaps itself to death.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use editor_core::Editor;
use serde_json::{json, Value};

use crate::engine::{self, Family};

/// How many documents may be open at once.
pub const MAX_SESSIONS: usize = 8;
/// How many pixels, in total, may be open at once. 160 megapixels is roughly
/// 640 MB of straight RGBA before history and tiles.
pub const MAX_TOTAL_PIXELS: u64 = 160_000_000;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no open document with id `{0}`. Call open_document first, or document_state to list what is open.")]
    NoSuchSession(String),
    #[error(
        "{MAX_SESSIONS} documents are already open, which is the cap. \
         Close one with close_document. Open now: {0}"
    )]
    TooMany(String),
    #[error(
        "opening this would put {wanted} megapixels open at once, over the {cap} megapixel cap. \
         Close a document with close_document, or use the one-shot tools, which hold nothing."
    )]
    TooManyPixels { wanted: u64, cap: u64 },
}

pub struct Open {
    pub ed: Editor,
    pub path: PathBuf,
    pub family: Family,
    pub format: String,
    /// How the source stored colour, before the engine's 8-bit straight RGBA.
    pub color_mode: String,
    /// The size of the file on disk when it was opened.
    pub file_bytes: u64,
    pub warnings: Vec<String>,
    /// False once an operation has been applied and not yet saved.
    pub saved: bool,
}

impl Open {
    pub fn pixels(&self) -> u64 {
        self.ed.doc.width as u64 * self.ed.doc.height as u64
    }

    /// Everything a caller (or a viewer) needs to know about this document.
    pub fn state(&self, id: &str) -> Value {
        let summary = self.ed.summary();
        json!({
            "session_id": id,
            "path": self.path.display().to_string(),
            "format": self.format,
            "family": self.family,
            "color_mode": self.color_mode,
            "file_bytes": self.file_bytes,
            "width": self.ed.doc.width,
            "height": self.ed.doc.height,
            "resolution": self.ed.doc.resolution,
            "saved": self.saved,
            "revision": self.ed.revision,
            "layers": summary["layers"],
            "layer_lines": engine::layer_lines(&summary),
            "selection": summary["selection"],
            "history": summary["history"],
            "warnings": self.warnings,
        })
    }
}

#[derive(Default)]
struct Inner {
    docs: BTreeMap<String, Open>,
    next: u64,
}

/// The open documents. Cloned into every tool call; the state is shared.
#[derive(Default)]
pub struct Sessions {
    inner: Mutex<Inner>,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a file as a session and return its id and state.
    pub fn open(&self, path: &Path) -> Result<(String, Value), Box<dyn std::error::Error + Send + Sync>> {
        // Decoding happens outside the lock: it is the slow part, and
        // holding the lock through it would serialise every other tool call
        // behind a RAW develop.
        let opened = engine::open(path)?;
        let doc = Open {
            path: path.to_path_buf(),
            family: opened.family,
            format: opened.format,
            color_mode: opened.color_mode,
            file_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            warnings: opened.warnings,
            saved: true,
            ed: opened.ed,
        };

        let mut inner = self.inner.lock().expect("sessions lock");
        if inner.docs.len() >= MAX_SESSIONS {
            let open: Vec<&str> = inner.docs.keys().map(String::as_str).collect();
            return Err(Box::new(SessionError::TooMany(open.join(", "))));
        }
        let already: u64 = inner.docs.values().map(Open::pixels).sum();
        let wanted = already + doc.pixels();
        if wanted > MAX_TOTAL_PIXELS {
            return Err(Box::new(SessionError::TooManyPixels {
                wanted: wanted / 1_000_000,
                cap: MAX_TOTAL_PIXELS / 1_000_000,
            }));
        }
        inner.next += 1;
        let id = format!("doc-{}", inner.next);
        let state = doc.state(&id);
        inner.docs.insert(id.clone(), doc);
        Ok((id, state))
    }

    /// Do something with an open document.
    pub fn with<T>(&self, id: &str, f: impl FnOnce(&mut Open) -> T) -> Result<T, SessionError> {
        let mut inner = self.inner.lock().expect("sessions lock");
        let doc = inner
            .docs
            .get_mut(id)
            .ok_or_else(|| SessionError::NoSuchSession(id.to_string()))?;
        Ok(f(doc))
    }

    pub fn close(&self, id: &str) -> Result<(), SessionError> {
        let mut inner = self.inner.lock().expect("sessions lock");
        inner
            .docs
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| SessionError::NoSuchSession(id.to_string()))
    }

    /// One line per open document, for `document_state` with no id.
    pub fn list(&self) -> Vec<Value> {
        let inner = self.inner.lock().expect("sessions lock");
        inner
            .docs
            .iter()
            .map(|(id, d)| {
                json!({
                    "session_id": id,
                    "path": d.path.display().to_string(),
                    "width": d.ed.doc.width,
                    "height": d.ed.doc.height,
                    "saved": d.saved,
                    "layers": d.ed.doc.layer_count(),
                })
            })
            .collect()
    }

    pub fn count(&self) -> usize {
        self.inner.lock().expect("sessions lock").docs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        image::RgbaImage::from_pixel(w, h, image::Rgba([80, 90, 100, 255]))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn a_session_survives_between_calls_and_carries_its_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = png(dir.path(), "a.png", 40, 30);
        let s = Sessions::new();
        let (id, state) = s.open(&path).unwrap();
        assert_eq!(state["width"], 40);

        s.with(&id, |d| {
            engine::run(
                &mut d.ed,
                &[json!({"op": "image.crop", "x": 0, "y": 0, "width": 20, "height": 15})],
            )
            .unwrap();
            d.saved = false;
        })
        .unwrap();

        let after = s.with(&id, |d| d.state(&id)).unwrap();
        assert_eq!(after["width"], 20);
        assert_eq!(after["saved"], false);
        assert_eq!(
            after["history"]["undo"].as_array().unwrap().len(),
            1,
            "the crop should be one undo step"
        );

        s.with(&id, |d| d.ed.exec(json!({"op": "edit.undo"}), &[]).unwrap())
            .unwrap();
        let undone = s.with(&id, |d| d.state(&id)).unwrap();
        assert_eq!(undone["width"], 40, "undo should restore the canvas");
    }

    #[test]
    fn the_state_carries_what_the_viewer_puts_in_its_status_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = png(dir.path(), "a.png", 12, 9);
        let s = Sessions::new();
        let (_, state) = s.open(&path).unwrap();
        assert!(state["color_mode"].as_str().unwrap().contains("Rgba"));
        assert_eq!(
            state["file_bytes"].as_u64().unwrap(),
            std::fs::metadata(&path).unwrap().len()
        );
    }

    #[test]
    fn closing_frees_the_slot_and_closing_twice_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = png(dir.path(), "a.png", 8, 8);
        let s = Sessions::new();
        let (id, _) = s.open(&path).unwrap();
        assert_eq!(s.count(), 1);
        s.close(&id).unwrap();
        assert_eq!(s.count(), 0);
        assert!(matches!(
            s.close(&id),
            Err(SessionError::NoSuchSession(_))
        ));
    }

    #[test]
    fn an_unknown_session_id_says_what_to_do() {
        let s = Sessions::new();
        let err = s.with("doc-99", |_| ()).unwrap_err();
        assert!(err.to_string().contains("open_document"), "{err}");
    }

    #[test]
    fn the_session_cap_is_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let s = Sessions::new();
        let mut ids = Vec::new();
        for i in 0..MAX_SESSIONS {
            let p = png(dir.path(), &format!("{i}.png"), 8, 8);
            ids.push(s.open(&p).unwrap().0);
        }
        let one_more = png(dir.path(), "extra.png", 8, 8);
        let err = s.open(&one_more).unwrap_err().to_string();
        assert!(err.contains("close_document"), "{err}");
        s.close(&ids[0]).unwrap();
        assert!(s.open(&one_more).is_ok(), "a slot should have freed up");
    }

    #[test]
    fn ids_are_not_reused_after_a_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = png(dir.path(), "a.png", 8, 8);
        let s = Sessions::new();
        let (first, _) = s.open(&path).unwrap();
        s.close(&first).unwrap();
        let (second, _) = s.open(&path).unwrap();
        assert_ne!(first, second, "a stale id must not hit a new document");
    }

    #[test]
    fn a_file_that_is_not_an_image_fails_to_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.png");
        std::fs::write(&path, b"this is text, whatever the extension says").unwrap();
        let s = Sessions::new();
        assert!(s.open(&path).is_err());
        assert_eq!(s.count(), 0, "a failed open must not leave a session");
    }
}
