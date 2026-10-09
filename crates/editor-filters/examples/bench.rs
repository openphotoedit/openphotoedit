//! Times every `filter.*` and `analyze.*` op on a real 24 MP photo.
//!
//! `testdata/photos/landscape.jpg` is 2400 × 1800; it is bilinearly upscaled
//! to 6000 × 4000 (24 MP) so the numbers are the ones a user of a modern
//! camera actually waits for. Each op runs on a fresh document (so no
//! previous filter's output is the next one's input) and is reported as
//! total milliseconds and milliseconds per megapixel.
//!
//! ```sh
//! CARGO_TARGET_DIR=target/h8 cargo run -p editor-filters --release --example bench
//! CARGO_TARGET_DIR=target/h8 cargo run -p editor-filters --release --example bench -- surface
//! ```
//! A bare word argument filters the op list by substring. `--mp N` changes
//! the megapixels, `--reps N` the repeat count (best of N is reported).

use std::time::Instant;

use editor_core::editor::Editor;
use serde_json::{json, Value};

fn upscaled(mp: f64) -> (Vec<u8>, usize, usize) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/photos/landscape.jpg");
    let img = image::open(path).expect("decode landscape").to_rgba8();
    let (sw, sh) = (img.width() as usize, img.height() as usize);
    let src = img.into_raw();
    // 3:2 output at the requested megapixels, rounded to even.
    let h = ((mp * 1e6 / 1.5).sqrt() / 2.0).round() as usize * 2;
    let w = (h * 3 / 2 / 2) * 2;
    if (w, h) == (sw, sh) {
        return (src, w, h);
    }
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy as usize, fy - fy.floor());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx as usize, fx - fx.floor());
            let x1 = (x0 + 1).min(sw - 1);
            let i = (y * w + x) * 4;
            for c in 0..4 {
                let a = src[(y0 * sw + x0) * 4 + c] as f32;
                let b = src[(y0 * sw + x1) * 4 + c] as f32;
                let d = src[(y1 * sw + x0) * 4 + c] as f32;
                let e = src[(y1 * sw + x1) * 4 + c] as f32;
                let top = a + (b - a) * tx;
                let bot = d + (e - d) * tx;
                out[i + c] = (top + (bot - top) * ty + 0.5) as u8;
            }
        }
    }
    (out, w, h)
}

/// Every op in the domain, with the parameters a user most plausibly picks.
fn cases() -> Vec<(&'static str, Value)> {
    vec![
        ("filter.gaussian-blur r20", json!({"op": "filter.gaussian-blur", "radius": 20.0})),
        ("filter.box-blur r20", json!({"op": "filter.box-blur", "radius": 20.0})),
        ("filter.motion-blur d40", json!({"op": "filter.motion-blur", "angle": 30.0, "distance": 40.0})),
        ("filter.radial-blur a20", json!({"op": "filter.radial-blur", "amount": 20.0})),
        ("filter.surface-blur r12 t20", json!({"op": "filter.surface-blur", "radius": 12.0, "threshold": 20.0})),
        ("filter.surface-blur r40 t50", json!({"op": "filter.surface-blur", "radius": 40.0, "threshold": 50.0})),
        ("filter.lens-blur r20", json!({"op": "filter.lens-blur", "radius": 20.0})),
        ("filter.tilt-shift r20", json!({"op": "filter.tilt-shift", "center_y": 1000.0, "band": 600.0, "feather": 400.0, "radius": 20.0})),
        ("filter.unsharp-mask r3", json!({"op": "filter.unsharp-mask", "amount": 100.0, "radius": 3.0, "threshold": 2.0})),
        ("filter.smart-sharpen r3", json!({"op": "filter.smart-sharpen", "amount": 100.0, "radius": 3.0, "reduce_noise": 20.0})),
        ("filter.high-pass r10", json!({"op": "filter.high-pass", "radius": 10.0})),
        ("filter.add-noise a10", json!({"op": "filter.add-noise", "amount": 10.0, "gaussian": true})),
        ("filter.reduce-noise", json!({"op": "filter.reduce-noise"})),
        ("filter.median r5", json!({"op": "filter.median", "radius": 5.0})),
        ("filter.dust-and-scratches r3", json!({"op": "filter.dust-and-scratches", "radius": 3.0, "threshold": 20.0})),
        ("filter.minimum r5", json!({"op": "filter.minimum", "radius": 5.0})),
        ("filter.maximum r5", json!({"op": "filter.maximum", "radius": 5.0})),
        ("filter.pixelate c16", json!({"op": "filter.pixelate", "cell": 16})),
        ("filter.emboss", json!({"op": "filter.emboss"})),
        ("filter.find-edges", json!({"op": "filter.find-edges"})),
        ("filter.solarize", json!({"op": "filter.solarize"})),
        ("filter.invert", json!({"op": "filter.invert"})),
        ("filter.desaturate", json!({"op": "filter.desaturate"})),
        ("filter.twirl", json!({"op": "filter.twirl", "angle": 60.0})),
        ("filter.pinch", json!({"op": "filter.pinch", "amount": 50.0})),
        ("filter.spherize", json!({"op": "filter.spherize", "amount": 50.0})),
        ("filter.ripple", json!({"op": "filter.ripple", "amount": 100.0})),
        ("filter.polar-coordinates", json!({"op": "filter.polar-coordinates", "to_polar": true})),
        ("filter.offset", json!({"op": "filter.offset", "dx": 100, "dy": 100, "wrap": true})),
        ("filter.clouds", json!({"op": "filter.clouds"})),
        ("filter.custom 5x5", json!({"op": "filter.custom", "kernel": vec![1.0f32; 25], "scale": 25.0})),
        ("filter.vignette", json!({"op": "filter.vignette", "amount": -50.0})),
        ("filter.frequency-separation r8", json!({"op": "filter.frequency-separation", "radius": 8.0})),
        ("filter.apply-adjustment curves", json!({"op": "filter.apply-adjustment", "adjustment": {"kind": "brightness-contrast", "brightness": 10.0, "contrast": 20.0}})),
        ("filter.red-eye 400²", json!({"op": "filter.red-eye", "x": 1000, "y": 1000, "width": 400, "height": 400})),
        ("filter.spot-heal r40", json!({"op": "filter.spot-heal", "x": 1000.0, "y": 1000.0, "radius": 40.0})),
        ("analyze.histogram", json!({"op": "analyze.histogram"})),
        ("analyze.auto", json!({"op": "analyze.auto"})),
        ("analyze.auto-levels", json!({"op": "analyze.auto-levels"})),
        ("analyze.facts", json!({"op": "analyze.facts"})),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut filter = String::new();
    let mut mp = 24.0f64;
    let mut reps = 1usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mp" => {
                mp = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--reps" => {
                reps = args[i + 1].parse().unwrap();
                i += 2;
            }
            other => {
                filter = other.to_string();
                i += 1;
            }
        }
    }

    let t = Instant::now();
    let (px, w, h) = upscaled(mp);
    let mpix = (w * h) as f64 / 1e6;
    eprintln!("{w} × {h} ({mpix:.1} MP), decoded and scaled in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);

    let mut rows: Vec<(String, f64)> = Vec::new();
    for (name, cmd) in cases() {
        if !filter.is_empty() && !name.contains(&filter) {
            continue;
        }
        let mut best = f64::INFINITY;
        for _ in 0..reps {
            let mut ed = Editor::new(1, 1);
            editor_filters::register(&mut ed);
            ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), &px).unwrap();
            let t = Instant::now();
            let r = ed.exec(cmd.clone(), &[]);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            if let Err(e) = r {
                eprintln!("{name}: {e}");
                best = f64::NAN;
                break;
            }
            best = best.min(ms);
        }
        println!("{:<34} {:>9.1} ms {:>9.1} ms/MP", name, best, best / mpix);
        rows.push((name.to_string(), best));
    }

    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    println!("\nTop ten by time ({mpix:.1} MP):");
    for (n, ms) in rows.iter().take(10) {
        println!("  {:<34} {:>9.1} ms {:>9.1} ms/MP", n, ms, ms / mpix);
    }
}
