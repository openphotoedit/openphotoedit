//! Undo and redo as whole-document snapshots.
//!
//! A snapshot is a `Document` clone, and a clone shares every tile with the
//! live document, so a step costs only the tiles its action rewrote.

use std::collections::HashSet;

use crate::document::Document;

pub struct Snapshot {
    pub doc: Document,
    pub label: String,
    merge_key: Option<String>,
}

pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Unique tile bytes history may hold beyond the live document.
    pub budget_bytes: usize,
    pub max_steps: usize,
    /// The merge key of the most recent record, reset by anything else.
    open_merge: Option<String>,
    /// An open transaction: every command until the matching `end` becomes
    /// one undo step.
    txn: Option<Txn>,
}

/// State of an open `edit.begin` … `edit.end` transaction.
struct Txn {
    /// The document as it was at the outermost `begin`.
    before: Document,
    /// The label the step gets; the first command's label when `begin` gave
    /// none.
    label: Option<String>,
    /// Nested `begin`s; the step is recorded when this returns to zero.
    depth: u32,
    /// Whether any recorded (document-changing) command ran inside.
    changed: bool,
}

impl Default for History {
    fn default() -> Self {
        History { undo: Vec::new(), redo: Vec::new(), budget_bytes: 1 << 30, max_steps: 200, open_merge: None, txn: None }
    }
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    /// Record `before` as the state an action is about to leave. Consecutive
    /// records with the same merge key (a slider drag) collapse into one step.
    pub fn record(&mut self, before: &Document, label: &str, merge_key: Option<&str>) {
        if let Some(t) = self.txn.as_mut() {
            // Inside a transaction the step is recorded at `end`.
            t.changed = true;
            t.label.get_or_insert_with(|| label.to_string());
            return;
        }
        if let (Some(k), Some(open)) = (merge_key, &self.open_merge) {
            if k == open && !self.undo.is_empty() {
                self.redo.clear();
                return;
            }
        }
        self.undo.push(Snapshot { doc: before.clone(), label: label.to_string(), merge_key: merge_key.map(String::from) });
        self.open_merge = merge_key.map(String::from);
        self.redo.clear();
        self.trim(before);
    }

    /// Open a transaction: commands until the matching [`History::end`]
    /// become a single undo step labelled `label` (or the first command's
    /// label). Transactions nest; only the outermost one records.
    pub fn begin(&mut self, current: &Document, label: Option<&str>) {
        match self.txn.as_mut() {
            Some(t) => t.depth += 1,
            None => {
                self.open_merge = None;
                self.txn = Some(Txn { before: current.clone(), label: label.map(String::from), depth: 1, changed: false });
            }
        }
    }

    /// Close one level of transaction. At the outermost level, record one
    /// step if anything changed; returns that step's label. A stray `end`
    /// with no open transaction does nothing, so callers can end in a
    /// `finally` without tracking state.
    pub fn end(&mut self) -> Option<String> {
        let t = self.txn.as_mut()?;
        t.depth -= 1;
        if t.depth > 0 {
            return None;
        }
        let t = self.txn.take()?;
        if !t.changed {
            return None;
        }
        let label = t.label.unwrap_or_else(|| "Edit".to_string());
        self.record(&t.before, &label, None);
        self.open_merge = None;
        Some(label)
    }

    /// Abandon an open transaction (all levels) and return the document to
    /// how it was at `begin`. Nothing is recorded.
    pub fn cancel(&mut self, current: &mut Document) -> bool {
        match self.txn.take() {
            Some(t) => {
                let changed = t.changed;
                *current = t.before;
                changed
            }
            None => false,
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.txn.is_some()
    }

    /// Close every level of an open transaction, recording its step. Undo,
    /// redo and history jumps call this first so they never act on half a
    /// gesture.
    pub fn commit_open(&mut self) {
        if let Some(t) = self.txn.as_mut() {
            t.depth = 1;
            self.end();
        }
    }

    /// Close any open merge so the next record starts a new step.
    pub fn seal(&mut self) {
        self.open_merge = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, current: &mut Document) -> Option<String> {
        self.commit_open();
        let s = self.undo.pop()?;
        self.open_merge = None;
        let label = s.label.clone();
        let now = std::mem::replace(current, s.doc);
        self.redo.push(Snapshot { doc: now, label: s.label, merge_key: s.merge_key });
        Some(label)
    }

    pub fn redo(&mut self, current: &mut Document) -> Option<String> {
        self.commit_open();
        let s = self.redo.pop()?;
        self.open_merge = None;
        let label = s.label.clone();
        let now = std::mem::replace(current, s.doc);
        self.undo.push(Snapshot { doc: now, label: s.label, merge_key: s.merge_key });
        Some(label)
    }

    /// Jump to a point: `undo_len` entries remaining on the undo stack.
    pub fn go_to(&mut self, undo_len: usize, current: &mut Document) {
        self.commit_open();
        while self.undo.len() > undo_len {
            if self.undo(current).is_none() {
                break;
            }
        }
        while self.undo.len() < undo_len {
            if self.redo(current).is_none() {
                break;
            }
        }
    }

    pub fn undo_labels(&self) -> Vec<&str> {
        self.undo.iter().map(|s| s.label.as_str()).collect()
    }
    pub fn redo_labels(&self) -> Vec<&str> {
        self.redo.iter().rev().map(|s| s.label.as_str()).collect()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open_merge = None;
        self.txn = None;
    }

    fn trim(&mut self, live: &Document) {
        while self.undo.len() > self.max_steps {
            self.undo.remove(0);
        }
        if self.undo.len() < 8 {
            return;
        }
        let mut seen = HashSet::new();
        live.unique_tile_bytes(&mut seen);
        let mut bytes = 0;
        // Newest first, so the oldest steps are the ones that go.
        let mut keep = self.undo.len();
        for (i, s) in self.undo.iter().enumerate().rev() {
            bytes += s.doc.unique_tile_bytes(&mut seen);
            if bytes > self.budget_bytes {
                keep = self.undo.len() - i - 1;
                break;
            }
        }
        let drop = self.undo.len() - keep.max(4);
        if drop > 0 {
            self.undo.drain(..drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_and_merge() {
        let mut doc = Document::new(10, 10);
        let mut h = History::new();
        h.record(&doc, "a", None);
        doc.resolution = 100.0;
        h.record(&doc, "drag", Some("slider"));
        doc.resolution = 110.0;
        h.record(&doc, "drag", Some("slider"));
        doc.resolution = 120.0;
        assert_eq!(h.undo_labels(), vec!["a", "drag"]);
        h.undo(&mut doc);
        assert_eq!(doc.resolution, 100.0);
        h.redo(&mut doc);
        assert_eq!(doc.resolution, 120.0);
        h.go_to(0, &mut doc);
        assert_eq!(doc.resolution, 72.0);
        assert_eq!(h.redo_labels(), vec!["a", "drag"]);
    }

    #[test]
    fn transaction_is_one_step() {
        let mut doc = Document::new(10, 10);
        let mut h = History::new();
        h.record(&doc, "first", None);
        doc.resolution = 90.0;
        h.begin(&doc, Some("Duplicate and move"));
        h.record(&doc, "Duplicate", None);
        doc.resolution = 100.0;
        h.begin(&doc, None); // nested: no extra step
        h.record(&doc, "Move", Some("offset"));
        doc.resolution = 110.0;
        assert_eq!(h.end(), None, "inner end records nothing");
        h.record(&doc, "Move", Some("offset"));
        doc.resolution = 120.0;
        assert_eq!(h.end().as_deref(), Some("Duplicate and move"));
        assert_eq!(h.undo_labels(), vec!["first", "Duplicate and move"]);
        h.undo(&mut doc);
        assert_eq!(doc.resolution, 90.0, "one undo reverts the whole gesture");
        h.redo(&mut doc);
        assert_eq!(doc.resolution, 120.0);
    }

    #[test]
    fn empty_stray_and_cancelled_transactions_record_nothing() {
        let mut doc = Document::new(10, 10);
        let mut h = History::new();
        h.begin(&doc, Some("nothing"));
        assert_eq!(h.end(), None);
        assert_eq!(h.end(), None, "stray end is harmless");
        assert!(!h.can_undo());
        h.begin(&doc, None);
        h.record(&doc, "Move", None);
        doc.resolution = 300.0;
        assert!(h.cancel(&mut doc));
        assert_eq!(doc.resolution, 72.0);
        assert!(!h.can_undo() && !h.in_transaction());
        // Undo inside an open transaction commits it first.
        h.begin(&doc, None);
        h.record(&doc, "Move", None);
        doc.resolution = 150.0;
        h.undo(&mut doc);
        assert_eq!(doc.resolution, 72.0);
        assert!(!h.in_transaction());
        assert_eq!(h.redo_labels(), vec!["Move"]);
    }
}
