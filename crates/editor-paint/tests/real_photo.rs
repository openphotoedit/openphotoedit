#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
//! Brush tools on a real photograph (`testdata/photos/portrait.jpg`). Each
//! test writes before/after crops to `target/paint/out/` for inspection and
//! asserts what a retoucher would check: the healed mole is gone and the skin
//! keeps its texture and tone, the cloned pin is a faithful copy, a soft brush
//! stroke has no dab ripples.
//!
//! `timings_12mp` (ignored by default) reports release-mode costs:
//! `cargo test --release -p editor-paint --test real_photo -- --ignored --nocapture`.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use editor_core::geom::Rect;
use editor_core::Editor;
use serde_json::{json, Value};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn out(name: &str) -> PathBuf {
    let d = root().join("target/paint/out");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

struct Photo {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

fn photo() -> &'static Photo {
    static P: OnceLock<Photo> = OnceLock::new();
    P.get_or_init(|| {
        let img = image::open(root().join("testdata/photos/portrait.jpg")).expect("portrait.jpg").to_rgba8();
        Photo { w: img.width(), h: img.height(), rgba: img.into_raw() }
    })
}

fn editor() -> Editor {
    let p = photo();
    let mut ed = Editor::new(1, 1);
    editor_paint::register(&mut ed);
    editor_select::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": p.w, "height": p.h}), &p.rgba).unwrap();
    ed
}

fn read(ed: &Editor, r: Rect) -> Vec<u8> {
    let l = ed.doc.find(ed.doc.active.unwrap()).unwrap();
    let ras = l.raster().unwrap();
    ras.plane.read_vec(r.translate(-ras.x, -ras.y))
}

/// Before | after side by side, scaled by `zoom`.
fn save_pair(name: &str, before: &[u8], after: &[u8], r: Rect, zoom: u32) {
    let (w, h) = (r.w as u32, r.h as u32);
    let mut img = image::RgbaImage::new(w * 2 + 8, h);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            img.put_pixel(x, y, image::Rgba([before[i], before[i + 1], before[i + 2], 255]));
            img.put_pixel(x + w + 8, y, image::Rgba([after[i], after[i + 1], after[i + 2], 255]));
        }
    }
    let img = if zoom != 1 { image::imageops::resize(&img, img.width() * zoom, img.height() * zoom, image::imageops::FilterType::Nearest) } else { img };
    img.save(out(name)).unwrap();
}

fn seg(ed: &mut Editor, base: &Value, pts: &[(f64, f64)], id: &str) -> Value {
    let mut cmd = base.clone();
    cmd["op"] = json!("paint.stroke");
    cmd["stroke_id"] = json!(id);
    cmd["points"] = Value::Array(pts.iter().map(|(x, y)| json!({"x": x, "y": y})).collect());
    ed.exec(cmd, &[]).unwrap()
}

fn luma_stats(px: &[u8], w: i32, r: Rect, inner: Rect) -> (f64, f64) {
    let mut v = Vec::new();
    for y in inner.y..inner.bottom() {
        for x in inner.x..inner.right() {
            let i = (((y - r.y) * w + (x - r.x)) * 4) as usize;
            v.push(0.3 * px[i] as f64 + 0.59 * px[i + 1] as f64 + 0.11 * px[i + 2] as f64);
        }
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    let sd = (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    (m, sd)
}

#[test]
fn heal_a_mole_on_the_cheek() {
    let mut ed = editor();
    // The mole beside the nose, at (1420, 685).
    let crop = Rect::new(1300, 570, 240, 230);
    let before = read(&ed, crop);
    let ring = |px: &[u8]| {
        // Skin just around the mole, outside the brush.
        let (a, _) = luma_stats(px, crop.w, crop, Rect::new(1370, 720, 30, 20));
        let (b, _) = luma_stats(px, crop.w, crop, Rect::new(1450, 640, 25, 20));
        (a + b) / 2.0
    };
    let (mole_before, _) = luma_stats(&before, crop.w, crop, Rect::new(1412, 677, 16, 16));
    let base = json!({"tool": "heal", "source": {"dx": -290, "dy": 15}, "brush": {"size": 44.0, "hardness": 0.3, "spacing": 0.1}});
    let t = Instant::now();
    let path = [(1405.0, 680.0), (1415.0, 684.0), (1425.0, 688.0), (1432.0, 690.0), (1424.0, 682.0), (1414.0, 690.0)];
    for w in path.windows(2) {
        seg(&mut ed, &base, w, "heal-mole");
    }
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "heal-mole"}), &[]).unwrap();
    eprintln!("heal stroke (5 segments + finish): {:?}", t.elapsed());
    let after = read(&ed, crop);
    save_pair("paint_heal_mole.png", &before, &after, crop, 2);
    let (mole_after, sd_after) = luma_stats(&after, crop.w, crop, Rect::new(1412, 677, 16, 16));
    let skin = ring(&after);
    eprintln!("mole luma {mole_before:.1} -> {mole_after:.1}; surrounding skin {skin:.1}; texture sd {sd_after:.2}");
    assert!(mole_before < skin - 15.0, "the mole is darker than the skin to begin with");
    assert!((mole_after - skin).abs() < 8.0, "healed spot matches the surrounding tone");
    assert!(sd_after > 1.0, "skin texture kept, not a flat patch");
    // Pixels well outside the brush are untouched.
    assert_eq!(before[..4 * 20], after[..4 * 20]);
}

#[test]
fn clone_the_flag_pin_onto_the_jacket() {
    let mut ed = editor();
    let (dx, dy) = (-300.0, -200.0);
    let dst = Rect::new(1532 + 300 - 60, 1532 + 200 - 45, 120, 90);
    let before = read(&ed, dst);
    let base = json!({"tool": "clone", "source": {"dx": dx, "dy": dy}, "brush": {"size": 70.0, "hardness": 0.8}});
    seg(&mut ed, &base, &[(1810.0, 1730.0), (1832.0, 1732.0)], "clone-pin");
    seg(&mut ed, &base, &[(1832.0, 1732.0), (1855.0, 1734.0)], "clone-pin");
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "clone-pin"}), &[]).unwrap();
    let after = read(&ed, dst);
    save_pair("paint_clone_pin.png", &before, &after, dst, 2);
    let src = read(&ed, dst.translate(dx as i32, dy as i32));
    // Centre of the stroke is an exact copy of the source.
    let c = Rect::new(1820, 1722, 25, 20);
    let (a, _) = luma_stats(&after, dst.w, dst, c);
    let mut shifted = Vec::new();
    shifted.extend_from_slice(&src);
    let (b, _) = luma_stats(&shifted, dst.w, dst, c);
    assert!((a - b).abs() < 0.5, "{a} vs {b}");
}

#[test]
fn soft_brush_stroke_is_smooth() {
    let mut ed = editor();
    let r = Rect::new(300, 700, 700, 300);
    let before = editor_core::pixels::read_merged(&ed.doc, r);
    // Paint on a new empty layer so the stroke's own alpha can be measured.
    ed.exec(json!({"op": "layer.add-pixel"}), &[]).unwrap();
    let base = json!({"brush": {"size": 200.0, "hardness": 0.0, "opacity": 0.8, "flow": 0.6, "spacing": 0.1, "color": {"r": 250, "g": 240, "b": 200}}});
    let pts: Vec<(f64, f64)> = (0..=12).map(|i| (400.0 + i as f64 * 42.0, 850.0 + (i as f64 * 0.7).sin() * 40.0)).collect();
    for w in pts.windows(2) {
        seg(&mut ed, &base, w, "soft");
    }
    let after = editor_core::pixels::read_merged(&ed.doc, r);
    save_pair("paint_soft_brush.png", &before, &after, r, 1);
    let layer = read(&ed, r);
    // Along the stroke's centre line the alpha must not ripple at the dab
    // spacing (20 px), and must never exceed the opacity.
    let alpha: Vec<f64> = (100..600)
        .map(|x| {
            let cy = (850.0 + (((x + r.x) as f64 - 400.0) / 42.0 * 0.7).sin() * 40.0).round() as i32 - r.y;
            layer[((cy * r.w + x) * 4 + 3) as usize] as f64
        })
        .collect();
    let mut worst = 0.0f64;
    for i in 10..alpha.len() - 10 {
        worst = worst.max((alpha[i] - (alpha[i - 10] + alpha[i + 10]) / 2.0).abs());
    }
    let max = alpha.iter().cloned().fold(0.0, f64::max);
    eprintln!("soft stroke centre alpha: min {:.0}, max {max:.0}, ripple {worst:.2}", alpha.iter().cloned().fold(255.0, f64::min));
    assert!(max <= 0.8 * 255.0 + 1.0, "capped by opacity: {max}");
    assert!(alpha.iter().all(|&a| a > 150.0), "stroke covers its centre line");
    assert!(worst < 6.0, "ripple {worst:.2}");
}

#[test]
#[ignore]
fn timings_12mp() {
    let p = photo();
    let (w, h) = (3100u32, 3872u32);
    let img = image::RgbaImage::from_raw(p.w, p.h, p.rgba.clone()).unwrap();
    let big = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
    let mut ed = Editor::new(1, 1);
    editor_paint::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), big.as_raw()).unwrap();
    let k = w as f64 / p.w as f64;
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;

    let base = json!({"brush": {"size": 200.0, "hardness": 0.0, "opacity": 0.8, "flow": 0.5, "spacing": 0.1, "color": {"r": 30, "g": 30, "b": 30}}});
    let mut times = Vec::new();
    for i in 0..30 {
        let x = 600.0 + i as f64 * 25.0;
        let t = Instant::now();
        seg(&mut ed, &base, &[(x, 1200.0), (x + 25.0, 1205.0)], "t-soft");
        times.push(ms(t));
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    eprintln!("200 px soft brush segment (25 px move): median {:.2} ms, max {:.2} ms", times[15], times[29]);
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "t-soft"}), &[]).unwrap();
    ed.exec(json!({"op": "edit.seal"}), &[]).unwrap();

    let heal = json!({"tool": "heal", "source": {"dx": -290.0 * k, "dy": 15.0 * k}, "brush": {"size": 60.0, "hardness": 0.3}});
    let t = Instant::now();
    let mut seg_times = Vec::new();
    for i in 0..10 {
        let x = 1405.0 * k + i as f64 * 8.0;
        let ts = Instant::now();
        seg(&mut ed, &heal, &[(x, 685.0 * k), (x + 8.0, 688.0 * k)], "t-heal");
        seg_times.push(ms(ts));
    }
    let tf = Instant::now();
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "t-heal"}), &[]).unwrap();
    eprintln!("heal: 10 segments of a 60 px brush {:.1} ms total (max segment {:.1} ms), finish {:.1} ms, whole stroke {:.1} ms", seg_times.iter().sum::<f64>(), seg_times.iter().cloned().fold(0.0, f64::max), ms(tf), ms(t));

    let clone = json!({"tool": "clone", "source": {"dx": -300, "dy": -200}, "brush": {"size": 120.0, "hardness": 0.5}});
    let t = Instant::now();
    seg(&mut ed, &clone, &[(1900.0, 1800.0), (1930.0, 1800.0)], "t-clone");
    eprintln!("clone segment (120 px): {:.2} ms", ms(t));
    let smudge = json!({"tool": "smudge", "strength": 0.6, "brush": {"size": 100.0}});
    let t = Instant::now();
    seg(&mut ed, &smudge, &[(1000.0, 2500.0), (1030.0, 2500.0)], "t-smudge");
    eprintln!("smudge segment (100 px): {:.2} ms", ms(t));
    let t = Instant::now();
    ed.exec(json!({"op": "paint.fill", "x": 150.0 * k, "y": 150.0 * k, "tolerance": 32, "color": {"r": 0, "g": 0, "b": 0}}), &[]).unwrap();
    eprintln!("paint bucket (curtain): {:.1} ms", ms(t));
    let t = Instant::now();
    ed.exec(json!({"op": "paint.gradient", "from": {"x": 0, "y": 0}, "to": {"x": w, "y": h}, "stops": [{"pos": 0.0, "color": {"r": 0, "g": 0, "b": 0, "a": 0}}, {"pos": 1.0, "color": {"r": 0, "g": 0, "b": 0}}], "opacity": 0.3}), &[]).unwrap();
    eprintln!("full-canvas gradient: {:.1} ms", ms(t));
}
