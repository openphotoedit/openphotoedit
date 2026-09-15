//! Real-photo checks. Each writes PNGs to `target/transform/out/` for a person
//! to look at, and asserts what can be asserted numerically.
//!
//! Release timings: `cargo test -p editor-transform --release --test photos -- --ignored --nocapture`

#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

use std::path::PathBuf;
use std::time::Instant;

use editor_core::editor::Editor;
use editor_core::geom::Rect;
use editor_core::render::flatten;
use editor_core::transform::{resize_raw, Resample};
use serde_json::{json, Value};

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/transform/out");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn portrait() -> (Vec<u8>, u32, u32) {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/photos/portrait.jpg");
    let img = image::open(p).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

fn scaled(px: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    resize_raw(px, w as usize, h as usize, 4, nw as usize, nh as usize, Resample::Bicubic)
}

fn editor_with(px: &[u8], w: u32, h: u32) -> (Editor, u32) {
    let mut ed = Editor::new(1, 1);
    editor_transform::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), px).unwrap();
    let id = ed.doc.active.unwrap();
    (ed, id)
}

fn exec(ed: &mut Editor, v: Value) -> Value {
    ed.exec(v, &[]).unwrap()
}

/// Composite over light grey (dark fringes show) and save a preview no
/// larger than `max` px plus the full image.
fn save(name: &str, rgba: &[u8], w: u32, h: u32, max: u32) {
    let mut flat = rgba.to_vec();
    for p in flat.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = ((*c as u32 * a + 220 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    let k = (max as f64 / w.max(h) as f64).min(1.0);
    let (pw, ph) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    let small = scaled(&flat, w, h, pw, ph);
    image::save_buffer(out_dir().join(format!("{name}.png")), &small, pw, ph, image::ExtendedColorType::Rgba8).unwrap();
}

fn save_crop(name: &str, rgba: &[u8], w: u32, r: Rect) {
    let mut out = Vec::new();
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let o = (y as usize * w as usize + x as usize) * 4;
            let a = rgba[o + 3] as u32;
            for c in 0..3 {
                out.push(((rgba[o + c] as u32 * a + 220 * (255 - a) + 127) / 255) as u8);
            }
            out.push(255);
        }
    }
    image::save_buffer(out_dir().join(format!("{name}.png")), &out, r.w as u32, r.h as u32, image::ExtendedColorType::Rgba8).unwrap();
}

fn rot_scale(w: f64, h: f64, deg: f64, s: f64) -> Value {
    let (sn, cs) = deg.to_radians().sin_cos();
    let (a, b, c, d) = (cs * s, sn * s, -sn * s, cs * s);
    let (cx, cy) = (w / 2.0, h / 2.0);
    json!({"a": a, "b": b, "c": c, "d": d, "e": cx - a * cx - c * cy, "f": cy - b * cx - d * cy})
}

#[test]
fn photo_free_transform_rotate_scale() {
    let (px, w, h) = portrait();
    let (mut ed, id) = editor_with(&px, w, h);
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": rot_scale(w as f64, h as f64, 15.0, 0.8)}));
    let out = flatten(&ed.doc);
    save("free-transform-15deg-0.8x", &out, w, h, 900);
    // 1:1 crop across the top-left rotated edge.
    let r = ed.doc.find(id).unwrap().content_bounds().unwrap();
    save_crop("free-transform-edge-1to1", &out, w, Rect::new(1000, 60, 300, 250));
    let _ = r;
    // No dark fringe: partially transparent edge pixels keep the colour of
    // the content next to them rather than going towards black.
    let raster = ed.doc.find(id).unwrap().raster().unwrap().clone();
    let raw = raster.plane.to_raw();
    let mut dark_edges = 0;
    let mut edges = 0;
    for p in raw.chunks_exact(4) {
        if p[3] > 0 && p[3] < 200 {
            edges += 1;
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) < 30 {
                dark_edges += 1;
            }
        }
    }
    assert!(edges > 1000);
    assert!(dark_edges * 20 < edges, "{dark_edges} of {edges} edge pixels are near black");
}

#[test]
fn photo_perspective_quad() {
    let (px, w, h) = portrait();
    let (mut ed, id) = editor_with(&px, w, h);
    let quad = json!([{"x": 400, "y": 250}, {"x": 2450, "y": 650}, {"x": 2350, "y": 2800}, {"x": 150, "y": 3250}]);
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "quad": quad}));
    let out = flatten(&ed.doc);
    save("perspective-quad", &out, w, h, 900);
    save_crop("perspective-quad-far-edge-1to1", &out, w, Rect::new(2150, 1000, 400, 400));
    let b = ed.doc.find(id).unwrap().content_bounds().unwrap();
    assert!((b.x - 150).abs() <= 2 && (b.y - 250).abs() <= 2 && (b.right() - 2450).abs() <= 2 && (b.bottom() - 3250).abs() <= 2, "{b:?}");
}

#[test]
fn photo_warp_bulge() {
    let (px, w, h) = portrait();
    let (sw, sh) = (w / 2, h / 2);
    let small = scaled(&px, w, h, sw, sh);
    let (mut ed, id) = editor_with(&small, sw, sh);
    let mut grid = Vec::new();
    for j in 0..4 {
        for i in 0..4 {
            let (mut x, mut y) = (sw as f64 * i as f64 / 3.0, sh as f64 * j as f64 / 3.0);
            if (1..=2).contains(&i) && (1..=2).contains(&j) {
                // Inner points pushed outward: a bulge.
                x += (x - sw as f64 / 2.0) * 0.9;
                y += (y - sh as f64 / 2.0) * 0.9;
            }
            grid.push(json!({"x": x, "y": y}));
        }
    }
    let t = Instant::now();
    exec(&mut ed, json!({"op": "transform.warp", "id": id, "grid": grid}));
    eprintln!("warp {}x{}: {:?}", sw, sh, t.elapsed());
    let out = flatten(&ed.doc);
    save("warp-bulge", &out, sw, sh, 900);
    save_crop("warp-bulge-centre-1to1", &out, sw, Rect::new(sw as i32 / 2 - 200, sh as i32 / 2 - 200, 400, 400));
    // The corners do not move.
    assert!((out[3] as i32 - 255).abs() <= 1);
}

#[test]
fn photo_liquify_bloat_face() {
    let (px, w, h) = portrait();
    let (mut ed, id) = editor_with(&px, w, h);
    let before = editor_core::pixels::read_layer(&ed.doc, id, Rect::new(0, 0, w as i32, h as i32)).unwrap();
    // Viewer's-left eye, then the mouth, as two strokes of several segments.
    for seg in 0..14 {
        exec(&mut ed, json!({"op": "transform.liquify", "id": id, "tool": "bloat", "size": 380, "pressure": 80, "density": 50, "points": [{"x": 1175 + seg, "y": 585}], "stroke_id": "eye"}));
    }
    for seg in 0..6 {
        let x = 1250.0 + seg as f64 * 25.0;
        exec(&mut ed, json!({"op": "transform.liquify", "id": id, "tool": "forward", "size": 300, "pressure": 50, "density": 50, "points": [{"x": x, "y": 880}, {"x": x + 25.0, "y": 860}], "stroke_id": "smile"}));
    }
    let out = flatten(&ed.doc);
    save("liquify-bloat-face", &out, w, h, 900);
    save_crop("liquify-bloat-face-1to1", &out, w, Rect::new(900, 350, 900, 700));
    save_crop("liquify-before-1to1", &before, w, Rect::new(900, 350, 900, 700));
    // Far from the brushes nothing changed at all.
    let far = Rect::new(0, 2000, w as i32, 200);
    let a = editor_core::pixels::read_layer(&ed.doc, id, far).unwrap();
    let b: Vec<u8> = (far.y..far.bottom()).flat_map(|y| before[(y as usize * w as usize) * 4..((y as usize + 1) * w as usize) * 4].to_vec()).collect();
    assert_eq!(a, b);
    // Two strokes = two undo steps; undoing both restores the photo.
    exec(&mut ed, json!({"op": "edit.undo"}));
    exec(&mut ed, json!({"op": "edit.undo"}));
    assert_eq!(editor_core::pixels::read_layer(&ed.doc, id, Rect::new(0, 0, w as i32, h as i32)).unwrap(), before);
}

#[test]
fn photo_content_aware_scale() {
    let (px, w, h) = portrait();
    // About 3 MP.
    let (sw, sh) = (1550u32, 1936u32);
    let small = scaled(&px, w, h, sw, sh);
    for protect in [true, false] {
        let (mut ed, _) = editor_with(&small, sw, sh);
        let tw = (sw as f64 * 0.8).round() as u32;
        let t = Instant::now();
        exec(&mut ed, json!({"op": "transform.content-aware-scale", "width": tw, "height": sh, "protect_skin": protect}));
        eprintln!("content-aware scale {sw}x{sh} -> {tw} (skin {protect}): {:?}", t.elapsed());
        assert_eq!((ed.doc.width, ed.doc.height), (tw, sh));
        let out = flatten(&ed.doc);
        save(&format!("content-aware-80pct{}", if protect { "-skin" } else { "" }), &out, tw, sh, 900);
    }
    // For comparison: a plain resize.
    let plain = scaled(&small, sw, sh, (sw as f64 * 0.8).round() as u32, sh);
    save("plain-resize-80pct", &plain, (sw as f64 * 0.8).round() as u32, sh, 900);
}

#[test]
fn photo_lens_correct_barrel() {
    let (px, w, h) = portrait();
    let (sw, sh) = (w / 2, h / 2);
    let small = scaled(&px, w, h, sw, sh);
    let (mut ed, id) = editor_with(&small, sw, sh);
    exec(&mut ed, json!({"op": "transform.lens-correct", "id": id, "distortion": -80, "chromatic_rc": 60, "vignette": -60, "vignette_midpoint": 40, "auto_scale": true}));
    let out = flatten(&ed.doc);
    save("lens-add-barrel-ca-vignette", &out, sw, sh, 900);
    let (mut ed2, id2) = editor_with(&small, sw, sh);
    exec(&mut ed2, json!({"op": "transform.lens-correct", "id": id2, "distortion": 80}));
    let out2 = flatten(&ed2.doc);
    save("lens-remove-barrel", &out2, sw, sh, 900);
    // Auto scale fills the frame: no transparent corners.
    assert_eq!(out[3], 255);
    assert_eq!(out[out.len() - 1], 255);
}

#[test]
#[ignore]
fn release_timings() {
    let (px, w, h) = portrait();
    let (bw, bh) = (4000u32, 3000u32);
    // A 12 MP layer: the portrait stretched (content does not matter here).
    let big = scaled(&px, w, h, bw, bh);
    let (mut ed, id) = editor_with(&big, bw, bh);
    let t = Instant::now();
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": rot_scale(bw as f64, bh as f64, 15.0, 0.9)}));
    eprintln!("free transform 12MP rotate 15° scale 0.9: {:?}", t.elapsed());
    exec(&mut ed, json!({"op": "edit.undo"}));
    let t = Instant::now();
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": rot_scale(bw as f64, bh as f64, 15.0, 0.25)}));
    eprintln!("free transform 12MP rotate 15° scale 0.25 (supersampled): {:?}", t.elapsed());
    exec(&mut ed, json!({"op": "edit.undo"}));
    let quad = json!([{"x": 300, "y": 200}, {"x": 3800, "y": 500}, {"x": 3600, "y": 2900}, {"x": 100, "y": 2800}]);
    let t = Instant::now();
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "quad": quad}));
    eprintln!("perspective 12MP: {:?}", t.elapsed());
    exec(&mut ed, json!({"op": "edit.undo"}));

    let mut grid = Vec::new();
    for j in 0..4 {
        for i in 0..4 {
            let (mut x, mut y) = (bw as f64 * i as f64 / 3.0, bh as f64 * j as f64 / 3.0);
            if (1..=2).contains(&i) && (1..=2).contains(&j) {
                x += (x - bw as f64 / 2.0) * 0.5;
                y += (y - bh as f64 / 2.0) * 0.5;
            }
            grid.push(json!({"x": x, "y": y}));
        }
    }
    let t = Instant::now();
    exec(&mut ed, json!({"op": "transform.warp", "id": id, "grid": grid}));
    eprintln!("warp 12MP: {:?}", t.elapsed());
    exec(&mut ed, json!({"op": "edit.undo"}));

    for (k, size) in [(0, 150.0), (1, 400.0)] {
        let mut times = Vec::new();
        for seg in 0..10 {
            let x = 1500.0 + seg as f64 * 30.0;
            let t = Instant::now();
            exec(&mut ed, json!({"op": "transform.liquify", "id": id, "tool": "forward", "size": size, "pressure": 60, "points": [{"x": x, "y": 1500}, {"x": x + 30.0, "y": 1510}], "stroke_id": format!("t{k}")}));
            times.push(t.elapsed());
        }
        let first = times[0];
        times.sort();
        eprintln!("liquify segment (size {size}, 30 px move) on 12MP: first {first:?}, median {:?}", times[5]);
    }

    let (sw, sh) = (2000u32, 1500u32);
    let small = scaled(&px, w, h, sw, sh);
    let (mut ed3, _) = editor_with(&small, sw, sh);
    let t = Instant::now();
    exec(&mut ed3, json!({"op": "transform.content-aware-scale", "width": 1600, "height": sh}));
    eprintln!("content-aware scale 3MP (2000x1500) -> 80% width: {:?}", t.elapsed());
}
