//! One file through the gauntlet, and the running tally over the corpus.
//!
//! This module holds no protocol and no HTTP. It imports a Photoshop file,
//! flattens it with our own compositor, reads the composite Photoshop itself
//! wrote into the same file, lines the two up, measures the disagreement and
//! makes three small pictures of it. [`crate::agui`] streams what comes out.
//!
//! **It recomputes, every time.** There is no recorded table anywhere in this
//! code path: every number in the stream came from `editor_core::render` a few
//! milliseconds earlier, on this machine.

use std::time::Instant;

use serde_json::{json, Value};

use editor_core::render::flatten;
use editor_psd::{import, merged_rgba, ImportOptions};

use super::compare::{self, Frame, Metrics, Verdict};
use super::corpus;

/// Where the runner has got to with the file it is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Import,
    Flatten,
    Compare,
    Done,
}

impl Phase {
    pub const fn key(self) -> &'static str {
        match self {
            Phase::Import => "import",
            Phase::Flatten => "flatten",
            Phase::Compare => "compare",
            Phase::Done => "done",
        }
    }
}

/// Everything one file produced.
#[derive(Clone, Debug)]
pub struct FileOutcome {
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub layers: usize,
    pub metrics: Option<Metrics>,
    pub verdict: Verdict,
    pub ms: u64,
    pub ours: Option<String>,
    pub adobe: Option<String>,
    pub diff: Option<String>,
    /// Why this file is not a straight comparison, when it is not.
    pub note: Option<String>,
}

impl FileOutcome {
    /// A file that produced nothing measurable. Public so the streaming layer
    /// can report a worker that panicked without inventing a passing number.
    pub fn blank(file: &str, verdict: Verdict, note: impl Into<String>, ms: u64) -> FileOutcome {
        FileOutcome {
            file: file.to_string(),
            width: 0,
            height: 0,
            layers: 0,
            metrics: None,
            verdict,
            ms,
            ours: None,
            adobe: None,
            diff: None,
            note: Some(note.into()),
        }
    }

    /// Megapixels of the document, for the throughput figure.
    pub fn megapixels(&self) -> f64 {
        self.width as f64 * self.height as f64 / 1_000_000.0
    }

    /// The `current` object in the streamed state.
    pub fn to_json(&self) -> Value {
        json!({
            "file": self.file,
            "width": self.width,
            "height": self.height,
            "layers": self.layers,
            "mae": self.metrics.map(|m| round4(m.mae)),
            "p95": self.metrics.map(|m| round4(m.p95)),
            "verdict": self.verdict.key(),
            "ms": self.ms,
            "ours": self.ours,
            "adobe": self.adobe,
            "diff": self.diff,
            "note": self.note,
        })
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Run one file: import, flatten, compare.
///
/// Never panics on a bad file if it can help it, and never reports a file it
/// could not measure as a pass — an import failure is [`Verdict::Failed`] and
/// a missing reference is [`Verdict::StructureOnly`], both of which the tally
/// keeps separate from the four passing bands.
pub fn measure<F: FnMut(Phase)>(entry: &corpus::Entry, mut on_phase: F) -> FileOutcome {
    let started = Instant::now();
    let rel = entry.rel.as_str();
    let ms = |t: &Instant| t.elapsed().as_millis() as u64;

    on_phase(Phase::Import);
    let bytes = match std::fs::read(&entry.path) {
        Ok(b) => b,
        Err(e) => return FileOutcome::blank(rel, Verdict::Failed, format!("could not read: {e}"), ms(&started)),
    };
    let imported = match import(&bytes, &ImportOptions::default()) {
        Ok(i) => i,
        Err(e) => return FileOutcome::blank(rel, Verdict::Failed, format!("import failed: {e}"), ms(&started)),
    };
    let doc = imported.doc;
    let (w, h) = (doc.width, doc.height);
    let layers = doc.layer_count();

    on_phase(Phase::Flatten);
    let ours = flatten(&doc);
    drop(doc);

    on_phase(Phase::Compare);
    let reference = reference_composite(&bytes, w, h, &ours);
    let mut outcome = FileOutcome {
        file: rel.to_string(),
        width: w,
        height: h,
        layers,
        metrics: None,
        verdict: Verdict::StructureOnly,
        ms: 0,
        ours: None,
        adobe: None,
        diff: None,
        note: None,
    };

    match reference {
        Reference::Present(rw, rh, merged) => {
            match compare::align(Frame::new(w, h, &ours), Some(Frame::new(rw, rh, &merged))) {
                Ok(aligned) => {
                    let m = compare::metrics(&aligned.ours, &aligned.theirs);
                    let (a, b, d) = compare::thumbnails(&aligned);
                    outcome.metrics = Some(m);
                    outcome.verdict = compare::classify(m.mae);
                    outcome.ours = a;
                    outcome.adobe = b;
                    outcome.diff = d;
                    if aligned.cropped {
                        outcome.note = Some(format!(
                            "sizes differ — ours {w}×{h}, Photoshop's {rw}×{rh}; \
                             measured over the {}×{} overlap",
                            aligned.width, aligned.height
                        ));
                    }
                }
                Err(e) => {
                    outcome.verdict = Verdict::StructureOnly;
                    outcome.note = Some(e.to_string());
                    outcome.ours = our_thumbnail(w, h, &ours);
                }
            }
        }
        Reference::Absent(why) => {
            outcome.verdict = Verdict::StructureOnly;
            outcome.note = Some(why);
            outcome.ours = our_thumbnail(w, h, &ours);
        }
    }
    outcome.ms = ms(&started);
    outcome
}

fn our_thumbnail(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    compare::jpeg_data_uri(
        w,
        h,
        &compare::rgb_over_white(rgba),
        compare::THUMB_MAX_SIDE,
        compare::THUMB_QUALITY,
    )
}

enum Reference {
    Present(u32, u32, Vec<u8>),
    Absent(String),
}

/// Decide whether the file carries a composite worth comparing against.
///
/// The three refusals are exactly the ones
/// `crates/editor-psd/tests/composite_diff.rs` makes, and for the same
/// reasons. Getting this wrong in either direction is how a fidelity number
/// becomes a lie: counting a placeholder as an exact match inflates the pass
/// rate, and dropping a real composite hides a failure.
fn reference_composite(bytes: &[u8], w: u32, h: u32, ours: &[u8]) -> Reference {
    let Ok((rw, rh, merged)) = merged_rgba(bytes) else {
        return Reference::Absent("this file carries no merged image".into());
    };
    // Image resource 1057 is "version info"; byte 4 is `hasRealMergedData`.
    // Zero means the writer saved without Maximize Compatibility, so the
    // merged image in the file is not a composite of the layers.
    let no_real = editor_psd::structure::parse(bytes)
        .ok()
        .and_then(|f| f.resource(1057).map(|d| d.get(4) == Some(&0)))
        .unwrap_or(false);
    if no_real {
        return Reference::Absent(
            "saved without Maximize Compatibility, so the merged image is a placeholder".into(),
        );
    }
    // Some writers leave a blank white merged image behind instead. If theirs
    // is uniformly white and ours is not, theirs is not a render of this file.
    let theirs_blank = merged.chunks_exact(4).all(|p| p == [255, 255, 255, 255]);
    let ours_blank = ours.chunks_exact(4).all(|p| p == [255, 255, 255, 255]);
    if theirs_blank && !ours_blank {
        return Reference::Absent(
            "the merged image in this file is blank white, so it is a placeholder".into(),
        );
    }
    let _ = (w, h);
    Reference::Present(rw, rh, merged)
}

// --- the running tally ---------------------------------------------------

/// Counts per verdict. Disjoint bands: they sum to the files processed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub exact: u32,
    pub within1: u32,
    pub within3: u32,
    pub within10: u32,
    pub over: u32,
    pub structure_only: u32,
    pub failed: u32,
}

impl Tally {
    pub fn add(&mut self, v: Verdict) {
        match v {
            Verdict::Exact => self.exact += 1,
            Verdict::Within1 => self.within1 += 1,
            Verdict::Within3 => self.within3 += 1,
            Verdict::Within10 => self.within10 += 1,
            Verdict::Over => self.over += 1,
            Verdict::StructureOnly => self.structure_only += 1,
            Verdict::Failed => self.failed += 1,
        }
    }

    pub fn total(self) -> u32 {
        self.exact + self.within1 + self.within3 + self.within10 + self.over + self.structure_only + self.failed
    }

    // By value: `Tally` is seven counters and `Copy`.
    pub fn to_json(self) -> Value {
        json!({
            "exact": self.exact,
            "within1": self.within1,
            "within3": self.within3,
            "within10": self.within10,
            "over": self.over,
            "structureOnly": self.structure_only,
            "failed": self.failed,
        })
    }
}

/// One of the worst files so far, with its pictures.
#[derive(Clone, Debug)]
pub struct WorstEntry {
    pub file: String,
    pub mae: f64,
    pub ours: Option<String>,
    pub adobe: Option<String>,
    pub diff: Option<String>,
}

impl WorstEntry {
    fn to_json(&self) -> Value {
        json!({
            "file": self.file,
            "mae": round4(self.mae),
            "ours": self.ours,
            "adobe": self.adobe,
            "diff": self.diff,
        })
    }
}

/// How many worst files the stream carries.
pub const WORST_KEPT: usize = 5;

/// What changed when a file was recorded, so the delta can name only those
/// paths instead of resending the whole state.
#[derive(Clone, Debug)]
pub struct Changed {
    pub tally_key: &'static str,
    pub bucket: Option<usize>,
    pub comparable_changed: bool,
    pub worst_changed: bool,
}

/// The whole run so far.
pub struct Progress {
    pub total: usize,
    pub comparable: usize,
    pub index: usize,
    pub tally: Tally,
    pub histogram: Vec<u32>,
    pub worst: Vec<WorstEntry>,
    pub megapixels: f64,
    started: Instant,
}

impl Progress {
    pub fn new(total: usize) -> Progress {
        Progress {
            total,
            comparable: 0,
            index: 0,
            tally: Tally::default(),
            histogram: vec![0; compare::HISTOGRAM_BUCKETS],
            worst: Vec::new(),
            megapixels: 0.0,
            started: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Fold one file in, and say what moved.
    pub fn record(&mut self, o: &FileOutcome) -> Changed {
        self.index += 1;
        self.megapixels += o.megapixels();
        self.tally.add(o.verdict);
        let mut changed = Changed {
            tally_key: o.verdict.key(),
            bucket: None,
            comparable_changed: false,
            worst_changed: false,
        };
        if let Some(m) = o.metrics.filter(|_| o.verdict.is_comparison()) {
            self.comparable += 1;
            changed.comparable_changed = true;
            let b = compare::bucket_of(m.mae);
            self.histogram[b] += 1;
            changed.bucket = Some(b);
            changed.worst_changed = self.offer_worst(o, m.mae);
        }
        changed
    }

    fn offer_worst(&mut self, o: &FileOutcome, mae: f64) -> bool {
        if self.worst.len() == WORST_KEPT && self.worst.last().is_some_and(|w| w.mae >= mae) {
            return false;
        }
        self.worst.push(WorstEntry {
            file: o.file.clone(),
            mae,
            ours: o.ours.clone(),
            adobe: o.adobe.clone(),
            diff: o.diff.clone(),
        });
        self.worst.sort_by(|a, b| b.mae.total_cmp(&a.mae));
        self.worst.truncate(WORST_KEPT);
        true
    }

    pub fn throughput_json(&self) -> Value {
        let elapsed = self.elapsed_ms();
        let secs = (elapsed as f64 / 1000.0).max(1e-6);
        json!({
            "filesPerSec": round4(self.index as f64 / secs),
            "megapixelsPerSec": round4(self.megapixels / secs),
            "elapsedMs": elapsed,
        })
    }

    pub fn worst_json(&self) -> Value {
        Value::Array(self.worst.iter().map(WorstEntry::to_json).collect())
    }

    /// The whole state document, for `STATE_SNAPSHOT`.
    pub fn snapshot(&self, phase: Phase, current: Option<&FileOutcome>) -> Value {
        json!({
            "total": self.total,
            "comparable": self.comparable,
            "index": self.index,
            "phase": phase.key(),
            "current": current.map(FileOutcome::to_json).unwrap_or(Value::Null),
            "tally": self.tally.to_json(),
            "histogram": self.histogram,
            "histogramEdges": compare::bucket_edges(),
            "worst": self.worst_json(),
            "throughput": self.throughput_json(),
        })
    }

    /// A one-line summary for the terminal and for `RUN_FINISHED`.
    pub fn summary(&self) -> Value {
        let t = &self.tally;
        json!({
            "files": self.index,
            "tallied": self.tally.total(),
            "comparable": self.comparable,
            "tally": t.to_json(),
            "within3Cumulative": t.exact + t.within1 + t.within3,
            "within10Cumulative": t.exact + t.within1 + t.within3 + t.within10,
            "megapixels": round4(self.megapixels),
            "throughput": self.throughput_json(),
            "worst": Value::Array(
                self.worst
                    .iter()
                    .map(|w| json!({ "file": w.file, "mae": round4(w.mae) }))
                    .collect(),
            ),
        })
    }
}

/// One RFC 6902 `replace`: a JSON Pointer and the value that goes there.
/// [`crate::agui`] turns these into `PatchOperation`s; keeping the protocol
/// type out of this module is what lets the runner be tested without one.
pub type Replace = (String, Value);

/// Build the patch that takes the last published state to this one.
///
/// Hand-built rather than diffed: the runner knows exactly which eight paths
/// can move when a file lands, and naming them is both smaller on the wire and
/// easier to read in a browser's network tab than a generic diff of a document
/// that carries three base64 JPEGs. Every path exists in the opening snapshot,
/// which is what `replace` requires.
pub fn delta_ops(progress: &Progress, phase: Phase, current: &FileOutcome, changed: &Changed) -> Vec<Replace> {
    let mut ops = vec![
        ("/index".to_string(), json!(progress.index)),
        ("/phase".to_string(), json!(phase.key())),
        ("/current".to_string(), current.to_json()),
        (
            format!("/tally/{}", changed.tally_key),
            tally_field(&progress.tally, changed.tally_key),
        ),
        ("/throughput".to_string(), progress.throughput_json()),
    ];
    if changed.comparable_changed {
        ops.push(("/comparable".to_string(), json!(progress.comparable)));
    }
    if let Some(b) = changed.bucket {
        ops.push((format!("/histogram/{b}"), json!(progress.histogram[b])));
    }
    if changed.worst_changed {
        ops.push(("/worst".to_string(), progress.worst_json()));
    }
    ops
}

/// The two-op delta that says which file is starting and what is being done
/// to it.
pub fn phase_ops(index: usize, phase: Phase) -> Vec<Replace> {
    vec![
        ("/index".to_string(), json!(index)),
        ("/phase".to_string(), json!(phase.key())),
    ]
}

fn tally_field(t: &Tally, key: &str) -> Value {
    match key {
        "exact" => json!(t.exact),
        "within1" => json!(t.within1),
        "within3" => json!(t.within3),
        "within10" => json!(t.within10),
        "over" => json!(t.over),
        "structureOnly" => json!(t.structure_only),
        _ => json!(t.failed),
    }
}

/// Walk a corpus start to finish with no protocol in the way — the numbers
/// without a browser attached. Used by the whole-corpus measurement below.
#[cfg(test)]
pub fn run_to_completion(entries: &[corpus::Entry], mut each: impl FnMut(&FileOutcome, &Progress)) -> Progress {
    let mut progress = Progress::new(entries.len());
    for entry in entries {
        let outcome = measure(entry, |_| {});
        progress.record(&outcome);
        each(&outcome, &progress);
    }
    progress
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(file: &str, mae: Option<f64>) -> FileOutcome {
        let verdict = mae.map_or(Verdict::StructureOnly, compare::classify);
        FileOutcome {
            file: file.into(),
            width: 100,
            height: 100,
            layers: 1,
            metrics: mae.map(|mae| Metrics { mae, p95: mae }),
            verdict,
            ms: 1,
            ours: Some("data:image/jpeg;base64,AAAA".into()),
            adobe: mae.map(|_| "data:image/jpeg;base64,BBBB".into()),
            diff: mae.map(|_| "data:image/jpeg;base64,CCCC".into()),
            note: None,
        }
    }

    #[test]
    fn the_tally_adds_up_to_the_files_processed() {
        let mut p = Progress::new(6);
        for (f, m) in [
            ("a", Some(0.0)),
            ("b", Some(0.5)),
            ("c", Some(2.0)),
            ("d", Some(7.0)),
            ("e", Some(40.0)),
            ("f", None),
        ] {
            p.record(&outcome(f, m));
        }
        assert_eq!(p.tally.total(), 6);
        assert_eq!(p.index, 6);
        assert_eq!(p.comparable, 5, "the file with no composite is not comparable");
        assert_eq!(p.tally, Tally { exact: 1, within1: 1, within3: 1, within10: 1, over: 1, structure_only: 1, failed: 0 });
    }

    #[test]
    fn the_histogram_counts_exactly_the_comparable_files() {
        let mut p = Progress::new(4);
        for (f, m) in [("a", Some(0.0)), ("b", Some(0.5)), ("c", Some(40.0)), ("d", None)] {
            p.record(&outcome(f, m));
        }
        let counted: u32 = p.histogram.iter().sum();
        assert_eq!(counted as usize, p.comparable);
        assert_eq!(p.histogram.len(), compare::HISTOGRAM_BUCKETS);
        // An exact match and a 40-level miss cannot share a bucket.
        assert_ne!(compare::bucket_of(0.0), compare::bucket_of(40.0));
    }

    #[test]
    fn the_histogram_never_loses_a_file_however_bad_it_is() {
        let mut p = Progress::new(3);
        for (f, m) in [("a", Some(255.0)), ("b", Some(1e12)), ("c", Some(f64::NAN))] {
            p.record(&outcome(f, m));
        }
        assert_eq!(p.histogram.iter().sum::<u32>(), 3);
        assert_eq!(p.tally.over, 3, "a NaN is not a pass");
    }

    #[test]
    fn the_worst_five_are_the_worst_five_in_order() {
        let mut p = Progress::new(8);
        for (i, mae) in [3.0, 41.0, 0.1, 12.0, 80.0, 7.0, 39.0, 0.0].iter().enumerate() {
            p.record(&outcome(&format!("f{i}"), Some(*mae)));
        }
        let got: Vec<f64> = p.worst.iter().map(|w| w.mae).collect();
        assert_eq!(got, vec![80.0, 41.0, 39.0, 12.0, 7.0]);
        assert!(p.worst.iter().all(|w| w.diff.is_some()), "the worst carry their pictures");
    }

    #[test]
    fn a_file_that_cannot_displace_the_worst_five_does_not_redraw_them() {
        let mut p = Progress::new(6);
        for mae in [10.0, 20.0, 30.0, 40.0, 50.0] {
            p.record(&outcome("x", Some(mae)));
        }
        let changed = p.record(&outcome("tiny", Some(0.01)));
        assert!(!changed.worst_changed, "a better file must not resend the worst list");
        let changed = p.record(&outcome("awful", Some(99.0)));
        assert!(changed.worst_changed);
    }

    #[test]
    fn the_delta_names_only_what_moved() {
        let mut p = Progress::new(2);
        let o = outcome("a", Some(0.5));
        let changed = p.record(&o);
        let ops = delta_ops(&p, Phase::Compare, &o, &changed);
        let paths: Vec<&str> = ops.iter().map(|(path, _)| path.as_str()).collect();
        assert!(paths.contains(&"/index"));
        assert!(paths.contains(&"/current"));
        assert!(paths.contains(&"/tally/within1"));
        assert!(paths.contains(&"/throughput"));
        assert!(paths.contains(&"/comparable"));
        assert!(!paths.iter().any(|p| p.starts_with("/tally/over")), "an untouched band is not resent");
        // Every path the delta names has to exist in the opening snapshot, or
        // an RFC 6902 `replace` is inapplicable.
        let snapshot = p.snapshot(Phase::Import, None);
        for (path, _) in &ops {
            assert!(snapshot.pointer(path).is_some(), "{path} is not in the snapshot");
        }
    }

    #[test]
    fn the_snapshot_has_the_shape_the_front_end_reads() {
        let p = Progress::new(269);
        let s = p.snapshot(Phase::Import, None);
        for key in ["total", "comparable", "index", "phase", "current", "tally", "histogram", "worst", "throughput"] {
            assert!(s.get(key).is_some(), "the snapshot is missing {key}");
        }
        assert_eq!(s["total"], 269);
        assert_eq!(s["current"], Value::Null);
        assert_eq!(s["histogram"].as_array().unwrap().len(), compare::HISTOGRAM_BUCKETS);
        for key in ["exact", "within1", "within3", "within10", "over", "structureOnly", "failed"] {
            assert_eq!(s["tally"][key], 0, "tally.{key}");
        }
        for key in ["filesPerSec", "megapixelsPerSec", "elapsedMs"] {
            assert!(s["throughput"].get(key).is_some(), "throughput.{key}");
        }
    }

    #[test]
    fn a_real_file_goes_through_and_carries_its_pictures() {
        let root = corpus::default_root();
        let path = root.join("psd-tools/group.psd");
        if !path.is_file() {
            return;
        }
        let entry = corpus::Entry { path, rel: "psd-tools/group.psd".into() };
        let mut phases = Vec::new();
        let o = measure(&entry, |p| phases.push(p));
        assert_eq!(phases, vec![Phase::Import, Phase::Flatten, Phase::Compare]);
        assert!(o.width > 0 && o.height > 0);
        assert!(o.layers > 0);
        assert!(o.metrics.is_some(), "group.psd has a real composite");
        assert!(o.ours.as_deref().is_some_and(|s| s.starts_with("data:image/jpeg;base64,")));
        assert!(o.adobe.is_some() && o.diff.is_some());
    }

    #[test]
    fn a_file_that_is_not_a_psd_fails_rather_than_passing() {
        let dir = std::env::temp_dir().join("gauntlet-not-a-psd");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nope.psd");
        std::fs::write(&path, b"this is not a Photoshop file").unwrap();
        let o = measure(&corpus::Entry { path, rel: "nope.psd".into() }, |_| {});
        assert_eq!(o.verdict, Verdict::Failed);
        assert!(o.metrics.is_none());
        assert!(o.note.is_some());
    }

    /// The whole corpus, printed. Not part of the normal test run — it takes
    /// minutes and it is a measurement, not an assertion.
    ///
    /// ```sh
    /// CARGO_TARGET_DIR=../../target/mcp cargo test --release --bin openphotoedit-mcp \
    ///     gauntlet::run::tests::whole_corpus -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "measures the whole corpus; run it deliberately"]
    fn whole_corpus() {
        let root = std::env::var("GAUNTLET_CORPUS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| corpus::default_root());
        let entries = corpus::select(corpus::walk(&root), std::env::var("GAUNTLET_FILTER").ok().as_deref());
        eprintln!("{} files under {}", entries.len(), root.display());
        let mut rows: Vec<(String, f64, &'static str, u64)> = Vec::new();
        let progress = run_to_completion(&entries, |o, _| {
            rows.push((
                o.file.clone(),
                o.metrics.map(|m| m.mae).unwrap_or(f64::NAN),
                o.verdict.key(),
                o.ms,
            ));
        });
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/mcp/gauntlet-corpus.tsv");
        let mut tsv = String::from("file\tmae\tverdict\tms\n");
        for (f, mae, v, ms) in &rows {
            tsv.push_str(&format!("{f}\t{mae:.4}\t{v}\t{ms}\n"));
        }
        let _ = std::fs::create_dir_all(out.parent().unwrap());
        let _ = std::fs::write(&out, &tsv);
        println!("{}", serde_json::to_string_pretty(&progress.summary()).unwrap());
        println!("per-file table: {}", out.display());
        assert_eq!(progress.tally.total() as usize, entries.len());
    }
}
