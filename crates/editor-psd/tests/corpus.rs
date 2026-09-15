//! The real-file corpus under `testdata/psd`: import, compare with the
//! psd-tools oracle, compare our composite with the file's own merged image,
//! export, re-import with ours and (when the venv exists) with psd-tools.
//!
//! `PSD_FIDELITY_REPORT=1 cargo test -p editor-psd --test corpus -- --nocapture`
//! also rewrites `testdata/psd/FIDELITY.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use editor_core::blend::BlendMode;
use editor_core::document::Document;
use editor_core::layer::{Layer, LayerKind};
use editor_core::render::flatten;
use editor_psd::{export, import, merged_rgba, ExportOptions, ImportOptions};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psd").canonicalize().unwrap()
}

fn corpus() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "oracle") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd") || e.eq_ignore_ascii_case("psb")) {
                out.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(&root(), &mut v);
    v
}

/// A comparable line per layer: depth, name, class, blend, opacity, visible, clipping.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    depth: usize,
    name: String,
    class: String,
    blend: String,
    opacity: i64,
    visible: bool,
    clip: bool,
}

fn our_rows(layers: &[Layer], depth: usize, out: &mut Vec<Row>) {
    for l in layers {
        let class = match &l.kind {
            LayerKind::Group { .. } => "group",
            LayerKind::Text { .. } => "text",
            LayerKind::Fill(_) => "fill",
            LayerKind::Adjustment(_) => "adjustment",
            _ if l.provenance.iter().any(|p| p == "psd:unsupported-adjustment") => "adjustment",
            _ if l.provenance.iter().any(|p| p == "psd:text") => "text",
            _ if l.provenance.iter().any(|p| p == "psd:gradient-fill") => "fill",
            _ => "pixel",
        };
        let blend = match &l.kind {
            LayerKind::Group { pass_through: true, .. } => "pass".to_string(),
            _ => String::from_utf8_lossy(&l.blend.psd_key()).into_owned(),
        };
        out.push(Row {
            depth,
            name: l.name.clone(),
            class: class.into(),
            blend,
            opacity: (l.opacity * 255.0).round() as i64,
            visible: l.visible,
            clip: l.clip,
        });
        if let Some(c) = l.children() {
            our_rows(c, depth + 1, out);
        }
    }
}

fn oracle_rows(layers: &[Value], depth: usize, out: &mut Vec<Row>) {
    for l in layers {
        let kind = l["kind"].as_str().unwrap_or("");
        let mut class = l["class"].as_str().unwrap_or("").to_string();
        if kind == "shape" {
            class = "fill|pixel".into();
        }
        out.push(Row {
            depth,
            name: l["name"].as_str().unwrap_or("").trim_end_matches('\0').into(),
            class,
            blend: l["blend"].as_str().unwrap_or("").into(),
            opacity: l["opacity"].as_i64().unwrap_or(-1),
            visible: l["visible"].as_bool().unwrap_or(true),
            clip: l["clipping"].as_bool().unwrap_or(false),
        });
        if let Some(c) = l["children"].as_array() {
            oracle_rows(c, depth + 1, out);
        }
    }
}

fn rows_match(ours: &[Row], theirs: &[Row]) -> (bool, String) {
    if ours.len() != theirs.len() {
        return (false, format!("{} layers vs {}", ours.len(), theirs.len()));
    }
    for (a, b) in ours.iter().zip(theirs) {
        // psd-tools does not read legacy `brit` brightness/contrast layers.
        let class_ok = b.class.split('|').any(|c| c == a.class) || (a.class == "adjustment" && a.name.starts_with("Brightness/Contrast") && b.class == "pixel");
        // psd-tools always reports groups as unclipped.
        let clip_ok = a.clip == b.clip || (a.class == "group" && !b.clip);
        // psd-tools reports pass-through for any group whose record says so.
        let blend_ok = a.blend == b.blend || (a.class == "group" && (b.blend == "pass" || a.blend == "pass") && (a.blend == "norm" || b.blend == "norm"));
        if a.depth != b.depth || a.name != b.name || !class_ok || !blend_ok || a.opacity != b.opacity || a.visible != b.visible || !clip_ok {
            return (false, format!("{a:?} vs {b:?}"));
        }
    }
    (true, String::new())
}

/// Straight RGBA over white.
fn over_white(p: &[u8]) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    [0, 1, 2].map(|c| p[c] as f32 * a + 255.0 * (1.0 - a))
}

fn lab(c: [f32; 3]) -> [f32; 3] {
    let lin = |v: f32| editor_core::color::srgb_to_linear(v / 255.0);
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
}

/// (mean absolute error over RGB-on-white, mean ΔE76)
fn compare(a: &[u8], b: &[u8]) -> (f64, f64) {
    let n = (a.len() / 4).max(1);
    let (mut mae, mut de) = (0f64, 0f64);
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        let (ca, cb) = (over_white(pa), over_white(pb));
        mae += ca.iter().zip(cb.iter()).map(|(x, y)| (x - y).abs() as f64).sum::<f64>() / 3.0;
        let (la, lb) = (lab(ca), lab(cb));
        de += (la.iter().zip(lb.iter()).map(|(x, y)| ((x - y) as f64).powi(2)).sum::<f64>()).sqrt();
    }
    (mae / n as f64, de / n as f64)
}

fn grid(rgba: &[u8], w: usize, h: usize) -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    for gy in 0..16 {
        let y0 = gy * h / 16;
        let y1 = ((gy + 1) * h / 16).max(y0 + 1);
        for gx in 0..16 {
            let x0 = gx * w / 16;
            let x1 = ((gx + 1) * w / 16).max(x0 + 1);
            let mut s = [0f64; 4];
            let mut n = 0.0;
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    let i = (y * w + x) * 4;
                    let a = rgba[i + 3] as f64 / 255.0;
                    for c in 0..3 {
                        s[c] += rgba[i + c] as f64 * a + 255.0 * (1.0 - a);
                    }
                    s[3] += rgba[i + 3] as f64;
                    n += 1.0;
                }
            }
            out.push(s.map(|v| if n > 0.0 { v / n } else { 0.0 }));
        }
    }
    out
}

fn grid_diff(ours: &[[f64; 4]], oracle: &Value) -> Option<f64> {
    let g = oracle.as_array()?;
    let mut worst = 0f64;
    for (gy, row) in g.iter().enumerate() {
        for (gx, cell) in row.as_array()?.iter().enumerate() {
            let c = cell.as_array()?;
            let o = ours.get(gy * 16 + gx)?;
            // Compare colour only where there is some coverage.
            let a = c[3].as_f64()?;
            worst = worst.max((o[3] - a).abs());
            for k in 0..3 {
                worst = worst.max((o[k] - c[k].as_f64()?).abs());
            }
        }
    }
    Some(worst)
}

struct Outcome {
    rel: String,
    import_ok: Result<(), String>,
    layers: usize,
    structure: Option<(bool, String)>,
    merged_decode: Option<f64>,
    composite: Option<(f64, f64)>,
    roundtrip_ours: Option<(bool, String, f64)>,
    roundtrip_psd_tools: Option<(bool, String)>,
    export_path: Option<PathBuf>,
    warnings: Vec<String>,
    blank_merged: bool,
    project_ok: bool,
}

fn doc_rows(doc: &Document) -> Vec<Row> {
    let mut r = Vec::new();
    our_rows(&doc.layers, 0, &mut r);
    r
}

fn run_file(path: &Path, out_dir: &Path) -> Outcome {
    let rel = path.strip_prefix(root()).unwrap().to_string_lossy().into_owned();
    let bytes = std::fs::read(path).unwrap();
    let oracle: Option<Value> = std::fs::read_to_string(root().join("oracle").join(format!("{rel}.json"))).ok().and_then(|s| serde_json::from_str(&s).ok());
    let mut o = Outcome {
        rel: rel.clone(),
        import_ok: Ok(()),
        layers: 0,
        structure: None,
        merged_decode: None,
        composite: None,
        roundtrip_ours: None,
        roundtrip_psd_tools: None,
        export_path: None,
        warnings: Vec::new(),
        blank_merged: false,
        project_ok: false,
    };
    let imported = match import(&bytes, &ImportOptions::default()) {
        Ok(i) => i,
        Err(e) => {
            o.import_ok = Err(e.to_string());
            return o;
        }
    };
    let doc = imported.doc;
    o.warnings = imported.warnings;
    o.layers = doc.layer_count();
    let rows = doc_rows(&doc);
    if let Some(or) = &oracle {
        if or.get("layers").is_some() {
            let mut theirs = Vec::new();
            oracle_rows(or["layers"].as_array().unwrap(), 0, &mut theirs);
            // A flat file becomes one "Background" layer here.
            let flat = theirs.is_empty() && rows.len() == 1 && rows[0].name == "Background";
            o.structure = Some(if flat { (true, String::new()) } else { rows_match(&rows, &theirs) });
        }
    }
    let render = flatten(&doc);
    if let Ok((w, h, merged)) = merged_rgba(&bytes) {
        // psd-tools is a reliable second decoder for 8- and 16-bit RGB only:
        // it shows 32-bit data without the sRGB curve and ignores grayscale
        // transparency.
        let hdr = editor_psd::structure::parse(&bytes).ok().map(|f| f.header);
        let comparable = hdr.is_some_and(|h| h.mode == 3 && h.depth <= 16);
        if let Some(g) = oracle.as_ref().and_then(|or| or.get("merged")).and_then(|m| m.get("grid")).filter(|_| comparable) {
            o.merged_decode = grid_diff(&grid(&merged, w as usize, h as usize), g);
        }
        // Files saved without "Maximize Compatibility" (and many written by
        // other tools) carry a blank white merged image: no reference.
        let no_real_merged = editor_psd::structure::parse(&bytes).ok().and_then(|f| f.resource(1057).map(|d| d.get(4) == Some(&0))).unwrap_or(false);
        let blank = no_real_merged || (merged.as_chunks::<4>().0.iter().all(|p| *p == [255, 255, 255, 255]) && render.as_chunks::<4>().0.iter().any(|p| *p != [255, 255, 255, 255]));
        if blank {
            o.blank_merged = true;
        } else {
            o.composite = Some(compare(&render, &merged));
        }
    }
    match export(&doc, &ExportOptions::default()) {
        Ok(e) => {
            // Through the native project format and back: the PSD written
            // afterwards must be byte-identical.
            let via_project = editor_project::save(&doc).and_then(|p| editor_project::load(&p, &Default::default()));
            o.project_ok = match via_project {
                Ok(l) => export(&l.doc, &ExportOptions::default()).map(|e2| e2.bytes == e.bytes).unwrap_or(false),
                Err(_) => false,
            };
            let dst = out_dir.join(&rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::write(&dst, &e.bytes).unwrap();
            o.export_path = Some(dst);
            match import(&e.bytes, &ImportOptions::default()) {
                Ok(back) => {
                    let back_rows = doc_rows(&back.doc);
                    let (same, why) = if back_rows == rows { (true, String::new()) } else { (false, rows_match(&back_rows, &rows).1) };
                    let (mae, _) = compare(&flatten(&back.doc), &render);
                    // The written merged image must be our render. Photoshop
                    // mattes it against white in 8 bits, so faint pixels
                    // lose a little colour precision; allow for that.
                    let merged_back = merged_rgba(&e.bytes).map(|m| compare(&m.2, &render).0).unwrap_or(255.0);
                    let why = if !same { why } else if mae >= 0.5 { format!("re-imported composite differs by {mae:.2}") } else if merged_back >= 1.0 { format!("written merged image differs by {merged_back:.2}") } else { why };
                    o.roundtrip_ours = Some((same && mae < 0.5 && merged_back < 1.0, why, mae.max(merged_back)));
                }
                Err(err) => o.roundtrip_ours = Some((false, err.to_string(), 255.0)),
            }
        }
        Err(err) => o.roundtrip_ours = Some((false, format!("export failed: {err}"), 255.0)),
    }
    let _ = BlendMode::Normal;
    o
}

#[test]
fn corpus_fidelity() {
    let files = corpus();
    assert!(files.len() > 50, "corpus missing: {}", files.len());
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/psd/roundtrip");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut outcomes: Vec<Outcome> = files.iter().map(|f| run_file(f, &out_dir)).collect();

    // Re-read every export with psd-tools, in one interpreter.
    let python = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/psd/venv/bin/python");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/oracle.py");
    let dumps = out_dir.join("oracle");
    let have_python = python.exists()
        && Command::new(&python).arg(&script).arg("--corpus").arg(&out_dir).arg(&dumps).arg("--force").status().map(|s| s.success()).unwrap_or(false);
    if have_python {
        for o in outcomes.iter_mut() {
            let Some(path) = &o.export_path else { continue };
            let rel = path.strip_prefix(&out_dir).unwrap().to_string_lossy().into_owned();
            let dump: Option<Value> = std::fs::read_to_string(dumps.join(format!("{rel}.json"))).ok().and_then(|s| serde_json::from_str(&s).ok());
            o.roundtrip_psd_tools = Some(match dump {
                Some(d) if d.get("error").is_none() => {
                    let bytes = std::fs::read(path).unwrap();
                    let doc = import(&bytes, &ImportOptions::default()).unwrap().doc;
                    let mut theirs = Vec::new();
                    oracle_rows(d["layers"].as_array().unwrap(), 0, &mut theirs);
                    let (ok, why) = rows_match(&doc_rows(&doc), &theirs);
                    let merged_ok = match d.get("merged").and_then(|m| m.get("grid")) {
                        Some(g) => {
                            let render = flatten(&doc);
                            grid_diff(&grid(&render, doc.width as usize, doc.height as usize), g).is_some_and(|x| x <= 2.0)
                        }
                        None => false,
                    };
                    (ok && merged_ok, if !merged_ok && ok { "merged image differs".into() } else { why })
                }
                Some(d) => (false, d["error"].as_str().unwrap_or("").to_string()),
                None => (false, "no dump".into()),
            });
        }
    }

    // Summary.
    let total = outcomes.len();
    let imported = outcomes.iter().filter(|o| o.import_ok.is_ok()).count();
    let with_oracle: Vec<&Outcome> = outcomes.iter().filter(|o| o.structure.is_some()).collect();
    let structure_ok = with_oracle.iter().filter(|o| o.structure.as_ref().unwrap().0).count();
    let decode_checked: Vec<f64> = outcomes.iter().filter_map(|o| o.merged_decode).collect();
    let decode_ok = decode_checked.iter().filter(|d| **d <= 2.0).count();
    let composites: Vec<(f64, f64)> = outcomes.iter().filter_map(|o| o.composite).collect();
    let within = |t: f64| composites.iter().filter(|c| c.0 <= t).count();
    let rt_ours = outcomes.iter().filter(|o| o.roundtrip_ours.as_ref().is_some_and(|r| r.0)).count();
    let rt_tools = outcomes.iter().filter(|o| o.roundtrip_psd_tools.as_ref().is_some_and(|r| r.0)).count();
    let blank = outcomes.iter().filter(|o| o.blank_merged).count();
    let project = outcomes.iter().filter(|o| o.project_ok).count();
    let median = {
        let mut v: Vec<f64> = composites.iter().map(|c| c.0).collect();
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };

    let mut report = String::new();
    report.push_str("# PSD fidelity report\n\n");
    report.push_str("Generated by `PSD_FIDELITY_REPORT=1 CARGO_TARGET_DIR=target/psd cargo test -p editor-psd --test corpus -- --nocapture`. Do not edit by hand.\n\n");
    report.push_str("## Summary\n\n");
    report.push_str(&format!("- Files: {total}; imported without error: {imported}.\n"));
    report.push_str(&format!("- Layer structure identical to psd-tools (names, kinds, blend, opacity, visibility, clipping, nesting): {structure_ok} of {}.\n", with_oracle.len()));
    report.push_str(&format!("- Our decode of the file's merged image agrees with psd-tools' (8/16-bit RGB files; 16×16 grid over white, ≤ 2/255): {decode_ok} of {}.\n", decode_checked.len()));
    report.push_str(&format!(
        "- Our composite vs the file's own merged image (mean absolute error over RGB on white, 0–255): ≤ 1: {}, ≤ 3: {}, ≤ 10: {}, of {}; median {:.2}.\n",
        within(1.0),
        within(3.0),
        within(10.0),
        composites.len(),
        median
    ));
    report.push_str(&format!("- Files whose merged image is a placeholder (saved without Maximize Compatibility, or by a tool that writes a blank one), so there is no reference: {blank}.\n"));
    report.push_str(&format!("- Export → re-import with ours (same layer tree, re-imported composite within 0.5, written merged image within 1.0): {rt_ours} of {total}.\n"));
    report.push_str(&format!("- Import → save as project → load → export gives the same PSD bytes as exporting directly: {project} of {total}.\n"));
    if have_python {
        report.push_str(&format!("- Export → re-read with psd-tools (layer tree matches ours, merged image grid within 2/255): {rt_tools} of {total}.\n"));
    } else {
        report.push_str("- psd-tools was not available, so exports were not re-read with it.\n");
    }
    report.push_str("\n## What the checks do and do not show\n\n");
    report.push_str("- The reference for the composite is the merged image Photoshop (or the writing tool) saved in the same file. Our composite is rendered by `editor_core::render`, so a large error is either a PSD feature the import does not carry into the document, or a difference in the engine's own drawing.\n");
    report.push_str("- psd-tools is a second parser, not a renderer we trust for pixels: it confirms the layer tree we read and that the files we write parse, but it cannot confirm that Photoshop reopens adjustment, fill, type and smart-object layers from our blocks. That has not been checked against Photoshop.\n");
    report.push_str("- Round trips compare our own import of our export. Untouched layers write back their original blocks byte for byte; edited or new layers are written from the engine's parameters.\n");
    report.push_str("\n### Causes of the largest composite errors (checked by eye on side-by-side renders)\n\n");
    report.push_str("| Cause | Where it lives | Files |\n|---|---|---|\n");
    report.push_str("| Knockout (`knko`) and fill opacity on groups are not drawn | core renderer | `transparency/knockout-*`, `passthrough_fill_adjustment` |\n");
    report.push_str("| A layer clipped to a pass-through group is composited without the group's backdrop | core renderer | `clipping-mask2`, `clipping-mask4` |\n");
    report.push_str("| Hue/Saturation per-range and lightness maths, Brightness/Contrast, Vivid Light, Hard Mix differ from Photoshop | core adjustments and blend modes | `adjustments/huesaturation_*`, `adjustments/brightnesscontrast_rgb`, `blend-modes/vivid-light`, `blend-modes/hard-mix`, `rgb-blend-modes` |\n");
    report.push_str("| Dissolve uses different noise | core (inherent) | `blend-modes/dissolve` |\n");
    report.push_str("| Layer style rendering (strokes on shapes, gradient/pattern strokes, bevels, multiple strokes) | core effects; pattern overlay and gradient strokes are not modelled | `effects/*`, `effect-stroke-gradient`, `layer_effects` |\n");
    report.push_str("| Artboard background colours and artboard clipping are not drawn | import (no model for artboards) | `artboard-bgcolor`, `ag-psd/artboards` |\n");
    report.push_str("| Colour lookup layers stored as ICC profiles, pattern fills and pattern overlays are not drawn | import (unsupported) | `fill_adjustments`, `patterns`, `ag-psd/pattern` |\n");
    report.push_str("| 4×4 gradient-fill test sheets: dithering and sub-pixel gradient geometry dominate at that size | import geometry / inherent | `colormodes/4x4_*`, `colorprofiles/north_america_newspaper` |\n");
    report.push_str("| 32-bit documents blend in linear light in Photoshop; here they are converted to 8-bit sRGB first | import conversion | `colormodes/4x4_32bit_*`, `vector-mask`, `300dpi`, `pen-text` |\n");
    report.push_str("| Mask feather is not applied | import (unsupported) | `layer_mask_data`, `mask_parameters` |\n");
    report.push_str("\nThe merged image is what Photoshop itself rendered and saved. Differences come from features this engine does not draw (or draws differently) — see the notes column — and from the adjustment and blend maths themselves. 16-bit, 32-bit, CMYK, Lab, indexed and grayscale files are converted to 8-bit RGB, so their errors include that conversion.\n\n");
    report.push_str("## Per file\n\n| File | Layers | Structure | Merged decode Δ | Composite MAE | ΔE76 | Round trip (ours) | Round trip (psd-tools) | Notes |\n|---|---:|---|---:|---:|---:|---|---|---|\n");
    for o in &outcomes {
        let structure = match (&o.import_ok, &o.structure) {
            (Err(e), _) => format!("import failed: {e}"),
            (_, Some((true, _))) => "ok".into(),
            (_, Some((false, why))) => format!("differs: {}", why.replace('|', "/").chars().take(120).collect::<String>()),
            (_, None) => "no oracle".into(),
        };
        let (mae, de) = o.composite.map(|c| (format!("{:.2}", c.0), format!("{:.2}", c.1))).unwrap_or(if o.blank_merged { ("no reference".into(), "–".into()) } else { ("–".into(), "–".into()) });
        let rt = match &o.roundtrip_ours {
            Some((true, _, _)) => "ok".to_string(),
            Some((false, why, mae)) => format!("differs ({:.2}) {}", mae, why.replace('|', "/").chars().take(80).collect::<String>()),
            None => "–".into(),
        };
        let rt2 = match &o.roundtrip_psd_tools {
            Some((true, _)) => "ok".to_string(),
            Some((false, why)) => format!("differs: {}", why.replace('|', "/").chars().take(80).collect::<String>()),
            None => "–".into(),
        };
        let notes = o.warnings.iter().map(|w| w.replace('|', "/")).collect::<Vec<_>>().join(" ");
        let notes: String = notes.chars().take(220).collect();
        let decode = o.merged_decode.map(|d| format!("{d:.1}")).unwrap_or("–".into());
        report.push_str(&format!("| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n", o.rel, o.layers, structure, decode, mae, de, rt, rt2, notes));
    }
    println!("{}", report.lines().take(14).collect::<Vec<_>>().join("\n"));
    if std::env::var("PSD_FIDELITY_REPORT").is_ok() {
        std::fs::write(root().join("FIDELITY.md"), &report).unwrap();
    }
    std::fs::write(out_dir.join("FIDELITY.md"), &report).unwrap();

    // Hard requirements: every file psd-tools opens must import; our own
    // round trip must be lossless for the structure.
    let failed: Vec<_> = outcomes.iter().filter(|o| o.import_ok.is_err()).map(|o| format!("{}: {:?}", o.rel, o.import_ok)).collect();
    assert!(failed.is_empty(), "imports failed: {failed:#?}");
    let rt_failed: Vec<_> = outcomes.iter().filter(|o| !o.roundtrip_ours.as_ref().is_some_and(|r| r.0)).map(|o| format!("{}: {:?}", o.rel, o.roundtrip_ours)).collect();
    assert!(rt_failed.is_empty(), "round trips failed: {rt_failed:#?}");
    let pj: Vec<_> = outcomes.iter().filter(|o| !o.project_ok).map(|o| o.rel.clone()).collect();
    assert!(pj.is_empty(), "project round trips changed the PSD: {pj:#?}");
}
