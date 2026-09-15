//! Real-photo checks on testdata/photos/portrait.jpg. Each writes before and
//! after PNGs to target/filters/out/ for a person to look at.

use std::time::Instant;

use editor_core::editor::Editor;
use serde_json::json;

use super::{editor, pixels, run};

fn out_dir() -> std::path::PathBuf {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/filters/out");
    std::fs::create_dir_all(&p).unwrap();
    p
}

pub fn portrait() -> (Vec<u8>, usize, usize) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/photos/portrait.jpg");
    let img = image::open(path).expect("decode portrait").to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    (img.into_raw(), w, h)
}

fn crop(px: &[u8], w: usize, x0: usize, y0: usize, cw: usize, ch: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..y0 + ch {
        out.extend_from_slice(&px[(y * w + x0) * 4..(y * w + x0 + cw) * 4]);
    }
    out
}

fn save(name: &str, px: &[u8], w: usize, h: usize) {
    image::save_buffer(out_dir().join(name), px, w as u32, h as u32, image::ColorType::Rgba8).unwrap();
}

fn open(px: &[u8], w: usize, h: usize) -> Editor {
    let mut ed = Editor::new(1, 1);
    crate::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), px).unwrap();
    ed
}

/// Mean absolute error over a rectangle, RGB.
fn mae(a: &[u8], b: &[u8], w: usize, r: (usize, usize, usize, usize), mask: impl Fn(usize, usize) -> bool) -> f64 {
    let (x0, y0, rw, rh) = r;
    let (mut s, mut n) = (0f64, 0f64);
    for y in y0..y0 + rh {
        for x in x0..x0 + rw {
            if !mask(x, y) {
                continue;
            }
            let i = (y * w + x) * 4;
            s += (0..3).map(|c| (a[i + c] as f64 - b[i + c] as f64).abs()).sum::<f64>() / 3.0;
            n += 1.0;
        }
    }
    s / n
}

/// Error of filling the region with the mean colour of a ring around it.
fn mean_fill_mae(orig: &[u8], w: usize, r: (usize, usize, usize, usize), inside: impl Fn(usize, usize) -> bool) -> f64 {
    let (x0, y0, rw, rh) = r;
    let mut acc = [0f64; 3];
    let mut n = 0f64;
    for y in y0.saturating_sub(10)..y0 + rh + 10 {
        for x in x0.saturating_sub(10)..x0 + rw + 10 {
            if inside(x, y) {
                continue;
            }
            for c in 0..3 {
                acc[c] += orig[(y * w + x) * 4 + c] as f64;
            }
            n += 1.0;
        }
    }
    let m = acc.map(|v| v / n);
    let (mut s, mut k) = (0f64, 0f64);
    for y in y0..y0 + rh {
        for x in x0..x0 + rw {
            if !inside(x, y) {
                continue;
            }
            s += (0..3).map(|c| (orig[(y * w + x) * 4 + c] as f64 - m[c]).abs()).sum::<f64>() / 3.0;
            k += 1.0;
        }
    }
    s / k
}

#[test]
fn content_aware_fill_on_real_photo() {
    let (px, w, _) = portrait();
    // Curtain beside the head, and the dark suit: 200×200 holes each.
    for (name, cx, cy) in [("curtain", 1700usize, 150usize), ("suit", 380, 1900)] {
        let (cw, ch) = (800, 800);
        let src = crop(&px, w, cx, cy, cw, ch);
        let mut ed = open(&src, cw, ch);
        let hole = (300, 300, 200, 200);
        // Blank the hole first so nothing of the original can leak through.
        run(&mut ed, json!({"op": "select.rect", "x": hole.0, "y": hole.1, "width": hole.2, "height": hole.3}));
        run(&mut ed, json!({"op": "layer.fill-selection", "color": {"r": 0, "g": 255, "b": 0}}));
        save(&format!("caf_{name}_before.png"), &pixels(&ed), cw, ch);
        let t = Instant::now();
        run(&mut ed, json!({"op": "filter.content-aware-fill"}));
        let secs = t.elapsed().as_secs_f64();
        let out = pixels(&ed);
        save(&format!("caf_{name}_after.png"), &out, cw, ch);
        let inside = |x: usize, y: usize| x >= hole.0 && x < hole.0 + hole.2 && y >= hole.1 && y < hole.1 + hole.3;
        let fill_err = mae(&out, &src, cw, hole, inside);
        let naive = mean_fill_mae(&src, cw, hole, inside);
        println!("content-aware fill {name}: mae {fill_err:.2} vs mean fill {naive:.2} in {secs:.2}s");
        assert!(fill_err < naive * 0.8, "{name}: fill {fill_err:.2} should beat mean fill {naive:.2}");
        assert!(!out.chunks_exact(4).any(|p| p[0] < 10 && p[1] == 255 && p[2] < 10), "green blank fully replaced");
    }
    // Object removal: the flag pin on the lapel.
    let (cw, ch) = (500, 400);
    let src = crop(&px, w, 1280, 1350, cw, ch);
    let mut ed = open(&src, cw, ch);
    run(&mut ed, json!({"op": "select.ellipse", "x": 200, "y": 160, "width": 110, "height": 80, "feather": 2}));
    run(&mut ed, json!({"op": "filter.content-aware-fill"}));
    save("caf_pin_before.png", &src, cw, ch);
    save("caf_pin_after.png", &pixels(&ed), cw, ch);
}

#[test]
fn spot_heal_on_real_photo() {
    let (px, w, _) = portrait();
    let (cw, ch) = (400, 300);
    let src = crop(&px, w, 1100, 380, cw, ch);
    save("heal_orig.png", &src, cw, ch);
    // A synthetic blemish on the forehead.
    let (sx, sy, r) = (200.0f32, 150.0f32, 18.0f32);
    let mut blemished = src.clone();
    for y in 0..ch {
        for x in 0..cw {
            let d = ((x as f32 + 0.5 - sx).powi(2) + (y as f32 + 0.5 - sy).powi(2)).sqrt();
            if d < 12.0 {
                let i = (y * cw + x) * 4;
                blemished[i] = (blemished[i] as f32 * 0.55) as u8;
                blemished[i + 1] = (blemished[i + 1] as f32 * 0.4) as u8;
                blemished[i + 2] = (blemished[i + 2] as f32 * 0.4) as u8;
            }
        }
    }
    let mut ed = open(&blemished, cw, ch);
    save("heal_before.png", &blemished, cw, ch);
    let t = Instant::now();
    run(&mut ed, json!({"op": "filter.spot-heal", "x": sx, "y": sy, "radius": r}));
    let secs = t.elapsed().as_secs_f64();
    let out = pixels(&ed);
    save("heal_after.png", &out, cw, ch);
    let inside = |x: usize, y: usize| ((x as f32 + 0.5 - sx).powi(2) + (y as f32 + 0.5 - sy).powi(2)).sqrt() < r;
    let rect = ((sx - r) as usize, (sy - r) as usize, (2.0 * r) as usize + 1, (2.0 * r) as usize + 1);
    let err = mae(&out, &src, cw, rect, inside);
    let blem = mae(&blemished, &src, cw, rect, inside);
    let naive = mean_fill_mae(&src, cw, rect, inside);
    println!("spot heal: mae {err:.2} (blemished {blem:.2}, mean fill {naive:.2}) in {secs:.3}s");
    assert!(err < naive * 0.9 && err < blem * 0.3, "heal {err:.2} vs mean fill {naive:.2}");
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a.chunks_exact(4).zip(b.chunks_exact(4)).map(|(p, q)| (0..3).map(|c| (p[c] as f64 - q[c] as f64).powi(2)).sum::<f64>() / 3.0).sum::<f64>() / (a.len() / 4) as f64;
    10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
}

#[test]
fn reduce_noise_improves_psnr_on_real_photo() {
    let (px, w, _) = portrait();
    let (cw, ch) = (700, 700);
    let clean = crop(&px, w, 1000, 300, cw, ch);
    let sigma = 12.0f32;
    let noisy: Vec<u8> = clean
        .chunks_exact(4)
        .enumerate()
        .flat_map(|(i, p)| {
            let (x, y) = ((i % cw) as i64, (i / cw) as i64);
            let n = |c: u64| {
                let u1 = crate::util::unit(crate::util::hash3(x, y, 100 + c)).max(1e-7);
                let u2 = crate::util::unit(crate::util::hash3(x, y, 200 + c));
                (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * sigma
            };
            [crate::util::to_u8(p[0] as f32 + n(0)), crate::util::to_u8(p[1] as f32 + n(1)), crate::util::to_u8(p[2] as f32 + n(2)), 255]
        })
        .collect();
    save("denoise_clean.png", &clean, cw, ch);
    save("denoise_noisy.png", &noisy, cw, ch);
    let before = psnr(&noisy, &clean);
    for (name, s, pd, rc) in [("default", 6.0, 60.0, 45.0), ("strong", 10.0, 20.0, 80.0)] {
        let mut ed = open(&noisy, cw, ch);
        let t = Instant::now();
        run(&mut ed, json!({"op": "filter.reduce-noise", "strength": s, "preserve_details": pd, "reduce_color_noise": rc}));
        let secs = t.elapsed().as_secs_f64();
        let out = pixels(&ed);
        save(&format!("denoise_{name}.png"), &out, cw, ch);
        let after = psnr(&out, &clean);
        println!("reduce noise {name}: PSNR {before:.2} dB -> {after:.2} dB in {secs:.2}s");
        assert!(after > before + 3.0, "{name}: {before:.2} -> {after:.2}");
    }
    // A clean photo is barely touched at default settings.
    let mut ed = open(&clean, cw, ch);
    run(&mut ed, json!({"op": "filter.reduce-noise"}));
    let p = psnr(&pixels(&ed), &clean);
    println!("reduce noise on the clean photo: PSNR vs original {p:.2} dB");
    assert!(p > 36.0);
}

fn develop_render(src: &[u8], w: usize, h: usize, develop: &serde_json::Value) -> Vec<u8> {
    let mut adj = develop.as_object().unwrap().clone();
    adj.insert("kind".into(), json!("develop"));
    let mut ed = open(src, w, h);
    run(&mut ed, json!({"op": "filter.apply-adjustment", "adjustment": adj}));
    pixels(&ed)
}

#[test]
fn auto_develop_on_real_photo() {
    let (px, w, h) = portrait();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, px).unwrap();
    let small = image::imageops::resize(&img, 540, 675, image::imageops::FilterType::Triangle);
    let (sw, sh) = (540, 675);
    let base = small.into_raw();
    // The photo as shot, plus a botched copy: a stop under with a cool cast.
    let botched: Vec<u8> = base
        .chunks_exact(4)
        .flat_map(|p| {
            let f = |v: u8, k: f32| {
                let l = editor_core::color::srgb_to_linear(v as f32 / 255.0) * 0.45 * k;
                crate::util::to_u8(editor_core::color::linear_to_srgb(l) * 255.0)
            };
            [f(p[0], 0.8), f(p[1], 1.0), f(p[2], 1.25), 255]
        })
        .collect();
    for (label, src) in [("asshot", &base), ("botched", &botched)] {
        save(&format!("auto_{label}_input.png"), src, sw, sh);
        let mut ed = open(src, sw, sh);
        for style in ["auto", "vivid", "natural", "bw"] {
            let r = run(&mut ed, json!({"op": "analyze.auto", "style": style}));
            let d = r["data"]["develop"].clone();
            println!("auto {label} {style}: {d}");
            let out = develop_render(src, sw, sh, &d);
            save(&format!("auto_{label}_{style}.png"), &out, sw, sh);
        }
        let facts = run(&mut ed, json!({"op": "analyze.facts"}));
        println!("facts {label}: {}", facts["data"]);
    }
    let mut ed = open(&botched, sw, sh);
    let d = run(&mut ed, json!({"op": "analyze.auto"}))["data"]["develop"].clone();
    assert!(d["exposure"].as_f64().unwrap() > 0.5, "underexposure corrected: {d}");
    assert!(d["temperature"].as_f64().unwrap() > 8.0, "cool cast warmed: {d}");
    let mut ed = open(&base, sw, sh);
    let d = run(&mut ed, json!({"op": "analyze.auto"}))["data"]["develop"].clone();
    assert!(d["exposure"].as_f64().unwrap().abs() < 0.6, "a well exposed photo stays near: {d}");
    assert!(d["temperature"].as_f64().unwrap().abs() < 25.0, "the red curtain does not drag white balance: {d}");
}

#[test]
fn filter_gallery_on_real_photo() {
    let (px, w, _) = portrait();
    let (cw, ch) = (600, 450);
    let src = crop(&px, w, 950, 250, cw, ch);
    save("gallery_source.png", &src, cw, ch);
    for (name, cmd) in [
        ("gaussian", json!({"op": "filter.gaussian-blur", "radius": 6})),
        ("motion", json!({"op": "filter.motion-blur", "angle": 30, "distance": 40})),
        ("radial_spin", json!({"op": "filter.radial-blur", "amount": 10, "mode": "spin"})),
        ("radial_zoom", json!({"op": "filter.radial-blur", "amount": 40, "mode": "zoom"})),
        ("surface", json!({"op": "filter.surface-blur", "radius": 8, "threshold": 18})),
        ("lens", json!({"op": "filter.lens-blur", "radius": 12})),
        ("tiltshift", json!({"op": "filter.tilt-shift", "center_y": 225, "band": 60, "feather": 120, "radius": 8})),
        ("unsharp", json!({"op": "filter.unsharp-mask", "amount": 150, "radius": 2, "threshold": 2})),
        ("smartsharpen", json!({"op": "filter.smart-sharpen", "amount": 200, "radius": 1.5, "reduce_noise": 20})),
        ("highpass", json!({"op": "filter.high-pass", "radius": 4})),
        ("median", json!({"op": "filter.median", "radius": 6})),
        ("maximum", json!({"op": "filter.maximum", "radius": 3})),
        ("mosaic", json!({"op": "filter.pixelate", "cell": 24})),
        ("emboss", json!({"op": "filter.emboss", "angle": 135, "height": 3, "amount": 150})),
        ("findedges", json!({"op": "filter.find-edges"})),
        ("twirl", json!({"op": "filter.twirl", "angle": 180})),
        ("pinch", json!({"op": "filter.pinch", "amount": 80})),
        ("spherize", json!({"op": "filter.spherize", "amount": 90})),
        ("ripple", json!({"op": "filter.ripple", "amount": 300, "size": "medium"})),
        ("polar", json!({"op": "filter.polar-coordinates", "to_polar": true})),
        ("clouds", json!({"op": "filter.clouds", "seed": 1})),
        ("vignette", json!({"op": "filter.vignette", "amount": -70})),
        ("addnoise", json!({"op": "filter.add-noise", "amount": 12, "gaussian": true, "monochromatic": true})),
    ] {
        let mut ed = editor(1, 1, |_, _| [0, 0, 0, 255]);
        ed.exec(json!({"op": "doc.open-pixels", "width": cw, "height": ch}), &src).unwrap();
        run(&mut ed, cmd);
        save(&format!("gallery_{name}.png"), &pixels(&ed), cw, ch);
    }
}

/// Timings on a 12 MP image. Run in release:
/// `CARGO_TARGET_DIR=target/filters cargo test --release -p editor-filters bench_12mp -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_12mp() {
    let (px, w, h) = portrait();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, px).unwrap();
    let (bw, bh) = (3000usize, 4000usize);
    let big = image::imageops::resize(&img, bw as u32, bh as u32, image::imageops::FilterType::Triangle).into_raw();
    if let Ok(path) = std::env::var("BENCH_RAW") {
        // The same pixels for the wasm benchmark.
        std::fs::write(path, &big).unwrap();
    }
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("gaussian r2", json!({"op": "filter.gaussian-blur", "radius": 2})),
        ("gaussian r50", json!({"op": "filter.gaussian-blur", "radius": 50})),
        ("box r20", json!({"op": "filter.box-blur", "radius": 20})),
        ("motion 30deg d60", json!({"op": "filter.motion-blur", "angle": 30, "distance": 60})),
        ("radial spin 10", json!({"op": "filter.radial-blur", "amount": 10})),
        ("surface r10 t15", json!({"op": "filter.surface-blur", "radius": 10, "threshold": 15})),
        ("lens r20", json!({"op": "filter.lens-blur", "radius": 20})),
        ("tilt-shift r15", json!({"op": "filter.tilt-shift", "center_y": 2000, "band": 400, "feather": 800, "radius": 15})),
        ("unsharp r2", json!({"op": "filter.unsharp-mask", "amount": 100, "radius": 2, "threshold": 0})),
        ("smart sharpen r2", json!({"op": "filter.smart-sharpen", "amount": 100, "radius": 2, "reduce_noise": 10})),
        ("reduce noise", json!({"op": "filter.reduce-noise"})),
        ("median r3", json!({"op": "filter.median", "radius": 3})),
        ("median r25", json!({"op": "filter.median", "radius": 25})),
        ("maximum r5", json!({"op": "filter.maximum", "radius": 5})),
        ("custom 5x5", json!({"op": "filter.custom", "kernel": vec![1; 25], "scale": 25})),
        ("twirl", json!({"op": "filter.twirl", "angle": 120})),
        ("add noise", json!({"op": "filter.add-noise", "amount": 10})),
        ("frequency separation r8", json!({"op": "filter.frequency-separation", "radius": 8})),
        ("spot heal r30", json!({"op": "filter.spot-heal", "x": 1500, "y": 900, "radius": 30})),
        ("analyze.histogram", json!({"op": "analyze.histogram"})),
        ("analyze.auto", json!({"op": "analyze.auto"})),
        ("analyze.facts", json!({"op": "analyze.facts"})),
    ];
    for (name, cmd) in cases {
        let mut ed = open(&big, bw, bh);
        let t = Instant::now();
        run(&mut ed, cmd);
        println!("{name:28} {:7.3} s", t.elapsed().as_secs_f64());
    }
    let mut ed = open(&big, bw, bh);
    run(&mut ed, json!({"op": "select.rect", "x": 2100, "y": 600, "width": 200, "height": 200}));
    let t = Instant::now();
    run(&mut ed, json!({"op": "filter.content-aware-fill"}));
    println!("{:28} {:7.3} s", "content-aware fill 200x200", t.elapsed().as_secs_f64());
}
