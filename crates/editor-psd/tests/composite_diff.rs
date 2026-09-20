//! A fast look at composite fidelity alone: import every corpus file, flatten
//! it with `editor_core::render` and compare with the file's own merged
//! image. Writes a per-file table and side-by-side PNGs (ours | merged |
//! |diff|×4) for the worst files or the ones named by `FIDELITY_FILTER`.
//!
//! ```sh
//! CARGO_TARGET_DIR=target/fidelity cargo test --release -p editor-psd --test composite_diff -- --ignored --nocapture
//! FIDELITY_FILTER=knockout FIDELITY_LABEL=after …
//! ```
//!
//! Output goes to `target/fidelity/` (`composite-<label>.tsv`, `out/*.png`).

use std::path::{Path, PathBuf};

use editor_core::render::flatten;
use editor_psd::{import, merged_rgba, ImportOptions};

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

fn over_white(p: &[u8]) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    [0, 1, 2].map(|c| p[c] as f32 * a + 255.0 * (1.0 - a))
}

fn mae(a: &[u8], b: &[u8]) -> f64 {
    let n = (a.len() / 4).max(1);
    let mut s = 0f64;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let (ca, cb) = (over_white(pa), over_white(pb));
        s += ca.iter().zip(cb.iter()).map(|(x, y)| (x - y).abs() as f64).sum::<f64>() / 3.0;
    }
    s / n as f64
}

fn side_by_side(path: &Path, w: usize, h: usize, ours: &[u8], theirs: &[u8]) {
    // Upscale tiny files so they can be looked at.
    let k = (256 / w.max(h).max(1)).clamp(1, 32);
    let (ow, oh) = (w * k, h * k);
    let gap = 4;
    let total_w = ow * 3 + gap * 2;
    let mut img = vec![128u8; total_w * oh * 3];
    for y in 0..oh {
        for x in 0..ow {
            let s = ((y / k) * w + x / k) * 4;
            let a = over_white(&ours[s..s + 4]);
            let b = over_white(&theirs[s..s + 4]);
            for (col, px) in [a, b, [0, 1, 2].map(|c| ((a[c] - b[c]).abs() * 4.0).min(255.0))].iter().enumerate() {
                let o = (y * total_w + col * (ow + gap) + x) * 3;
                img[o] = px[0] as u8;
                img[o + 1] = px[1] as u8;
                img[o + 2] = px[2] as u8;
            }
        }
    }
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), total_w as u32, oh as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img).unwrap();
}

#[test]
#[ignore]
fn composite_diff() {
    let filter = std::env::var("FIDELITY_FILTER").ok();
    let label = std::env::var("FIDELITY_LABEL").unwrap_or_else(|_| "current".into());
    let worst_n: usize = std::env::var("FIDELITY_WORST").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fidelity");
    let pngs = out.join("out");
    std::fs::create_dir_all(&pngs).unwrap();
    let mut rows: Vec<(String, f64, usize, usize, Vec<u8>, Vec<u8>)> = Vec::new();
    for path in corpus() {
        let rel = path.strip_prefix(root()).unwrap().to_string_lossy().into_owned();
        if filter.as_ref().is_some_and(|f| !f.split(',').any(|p| rel.contains(p))) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let Ok(imp) = import(&bytes, &ImportOptions::default()) else { continue };
        let Ok((w, h, merged)) = merged_rgba(&bytes) else { continue };
        let no_real = editor_psd::structure::parse(&bytes).ok().and_then(|f| f.resource(1057).map(|d| d.get(4) == Some(&0))).unwrap_or(false);
        let render = flatten(&imp.doc);
        if no_real || (merged.chunks_exact(4).all(|p| p == [255, 255, 255, 255]) && render.chunks_exact(4).any(|p| p != [255, 255, 255, 255])) {
            continue;
        }
        let m = mae(&render, &merged);
        rows.push((rel, m, w as usize, h as usize, render, merged));
    }
    let mut tsv = String::new();
    for r in &rows {
        tsv.push_str(&format!("{}\t{:.2}\n", r.0, r.1));
    }
    std::fs::write(out.join(format!("composite-{label}.tsv")), &tsv).unwrap();
    let mut sorted: Vec<&(String, f64, usize, usize, Vec<u8>, Vec<u8>)> = rows.iter().collect();
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
    let within = |t: f64| rows.iter().filter(|r| r.1 <= t).count();
    let mut v: Vec<f64> = rows.iter().map(|r| r.1).collect();
    v.sort_by(f64::total_cmp);
    println!(
        "files {}  <=1: {}  <=3: {}  <=10: {}  median {:.2}  mean {:.2}",
        rows.len(),
        within(1.0),
        within(3.0),
        within(10.0),
        v.get(v.len() / 2).copied().unwrap_or(0.0),
        v.iter().sum::<f64>() / v.len().max(1) as f64
    );
    for r in sorted.iter().take(if filter.is_some() { usize::MAX } else { 40 }) {
        println!("{:8.2}  {}", r.1, r.0);
    }
    let dump: Vec<_> = if filter.is_some() { sorted.clone() } else { sorted.iter().take(worst_n).copied().collect() };
    for r in dump {
        let name = r.0.replace(['/', '.'], "_");
        side_by_side(&pngs.join(format!("{name}.png")), r.2, r.3, &r.4, &r.5);
    }
}
