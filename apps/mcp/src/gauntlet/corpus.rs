//! Finding the files. `testdata/psd` holds the psd-tools and ag-psd corpora
//! side by side, plus an `oracle/` directory of reference JSON that is not
//! itself a Photoshop file.
//!
//! The walk is deliberately the same one `crates/editor-psd/tests/composite_diff.rs`
//! does — sorted, recursive, `oracle` skipped, `.psd` and `.psb` only — so the
//! two see the same 269 files in the same order and their tallies can be
//! compared.

use std::path::{Path, PathBuf};

/// One corpus entry: where it is, and what to call it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    /// Path relative to the corpus root, with forward slashes —
    /// `psd-tools/layer_effects.psd`. This is what goes on the wire.
    pub rel: String,
}

/// Where the corpus lives when nobody says otherwise: `testdata/psd` in this
/// checkout. Compiled in rather than resolved from the working directory,
/// because the demo is started from wherever the operator happens to be.
pub fn default_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psd");
    here.canonicalize().unwrap_or(here)
}

/// Every `.psd` and `.psb` under `root`, sorted, skipping `oracle/`.
pub fn walk(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    walk_into(root, root, &mut out);
    out
}

fn walk_into(root: &Path, dir: &Path, out: &mut Vec<Entry>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            // `oracle/` is psd-tools' reference JSON, not Photoshop files.
            if p.file_name().is_some_and(|n| n != "oracle") {
                walk_into(root, &p, out);
            }
        } else if p
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("psd") || e.eq_ignore_ascii_case("psb"))
        {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(Entry { path: p, rel });
        }
    }
}

/// Keep the entries whose relative path contains `filter` — or any one of its
/// comma-separated alternatives, as `FIDELITY_FILTER` does. An empty or
/// missing filter keeps everything.
pub fn select(entries: Vec<Entry>, filter: Option<&str>) -> Vec<Entry> {
    let Some(filter) = filter.filter(|f| !f.trim().is_empty()) else {
        return entries;
    };
    let needles: Vec<&str> = filter
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    entries
        .into_iter()
        .filter(|e| needles.iter().any(|n| e.rel.contains(n)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(rels: &[&str]) -> Vec<Entry> {
        rels.iter()
            .map(|r| Entry {
                path: PathBuf::from(r),
                rel: (*r).to_string(),
            })
            .collect()
    }

    #[test]
    fn no_filter_keeps_everything() {
        let all = entries(&["a/x.psd", "b/y.psb"]);
        assert_eq!(select(all.clone(), None).len(), 2);
        assert_eq!(select(all.clone(), Some("")).len(), 2);
        assert_eq!(select(all, Some("   ")).len(), 2);
    }

    #[test]
    fn a_filter_is_a_substring_of_the_relative_path() {
        let all = entries(&["ag-psd/effects.psd", "psd-tools/group.psd"]);
        assert_eq!(select(all.clone(), Some("effects")).len(), 1);
        assert_eq!(select(all.clone(), Some("ag-psd/")).len(), 1);
        assert_eq!(select(all, Some("nothing-like-this")).len(), 0);
    }

    #[test]
    fn commas_are_alternatives() {
        let all = entries(&["a/one.psd", "b/two.psd", "c/three.psd"]);
        let kept = select(all, Some("one, three"));
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].rel, "a/one.psd");
        assert_eq!(kept[1].rel, "c/three.psd");
    }

    #[test]
    fn the_real_corpus_is_there_and_the_oracle_is_not_in_it() {
        let root = default_root();
        if !root.is_dir() {
            return; // A checkout without testdata; the other tests still run.
        }
        let all = walk(&root);
        assert!(all.len() > 200, "found only {} corpus files", all.len());
        assert!(
            all.iter().all(|e| !e.rel.starts_with("oracle/")),
            "the oracle directory is reference JSON, not Photoshop files"
        );
        assert!(all.iter().all(|e| e.rel.ends_with(".psd") || e.rel.ends_with(".psb")));
        // Sorted and deduplicated: step names on the wire have to be unique.
        let mut rels: Vec<&str> = all.iter().map(|e| e.rel.as_str()).collect();
        let before = rels.len();
        rels.dedup();
        assert_eq!(rels.len(), before, "two corpus entries share a name");
    }
}
