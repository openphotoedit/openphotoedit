#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
//! Selection algorithms on a real photograph (`testdata/photos/portrait.jpg`,
//! 2687×3356). Each test writes what it produced to `target/paint/out/` so the
//! results can be looked at, and asserts the properties a person would check
//! by eye: the wand stays inside the flag's canton, quick selection covers the
//! suit without taking the shirt or the curtain, refine puts a rough outline
//! on the silhouette.
//!
//! `timings_12mp` (ignored by default) reports release-mode costs on a 12 MP
//! upscale: `cargo test --release -p editor-select --test real_photo -- --ignored --nocapture`.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use editor_core::geom::Rect;
use editor_core::plane::Plane;
use editor_core::Editor;
use serde_json::json;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn out_dir() -> PathBuf {
    let d = root().join("target/paint/out");
    std::fs::create_dir_all(&d).unwrap();
    d
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

/// Coordinates below were read off an 640×800 preview; scale them up.
fn s(v: f64) -> f64 {
    v * photo().w as f64 / 640.0
}

fn editor() -> Editor {
    let p = photo();
    let mut ed = Editor::new(1, 1);
    editor_select::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": p.w, "height": p.h}), &p.rgba).unwrap();
    ed
}

/// Save the selection as a preview: selected pixels at full brightness,
/// unselected ones dimmed and tinted, downscaled to `max_h` rows; plus the
/// raw mask and a full-resolution crop of `crop`.
fn save_selection(name: &str, sel: &Plane, max_h: u32, crop: Option<Rect>) {
    let p = photo();
    let mask = sel.to_raw();
    let mut over = image::RgbaImage::new(p.w, p.h);
    for (i, px) in over.pixels_mut().enumerate() {
        let m = mask[i] as f32 / 255.0;
        let src = &p.rgba[i * 4..i * 4 + 4];
        let dim = |c: u8, tint: f32| (c as f32 * 0.25 + tint) as u8;
        let r = src[0] as f32 * m + dim(src[0], 60.0) as f32 * (1.0 - m);
        let g = src[1] as f32 * m + dim(src[1], 0.0) as f32 * (1.0 - m);
        let b = src[2] as f32 * m + dim(src[2], 30.0) as f32 * (1.0 - m);
        *px = image::Rgba([r as u8, g as u8, b as u8, 255]);
    }
    let small_w = p.w * max_h / p.h;
    let small = image::imageops::resize(&over, small_w, max_h, image::imageops::FilterType::Triangle);
    small.save(out_dir().join(format!("{name}.png"))).unwrap();
    let m = image::GrayImage::from_raw(p.w, p.h, mask).unwrap();
    image::imageops::resize(&m, small_w, max_h, image::imageops::FilterType::Triangle).save(out_dir().join(format!("{name}_mask.png"))).unwrap();
    if let Some(c) = crop {
        let sub = image::imageops::crop_imm(&over, c.x as u32, c.y as u32, c.w as u32, c.h as u32).to_image();
        sub.save(out_dir().join(format!("{name}_crop.png"))).unwrap();
    }
}

fn frac_selected(sel: &Plane, r: Rect) -> f32 {
    let v = sel.read_vec(r);
    v.iter().filter(|&&c| c >= 128).count() as f32 / v.len() as f32
}

#[test]
fn magic_wand_on_the_flag_canton() {
    let mut ed = editor();
    // The blue canton of the flag on the left, between stars.
    ed.exec(json!({"op": "select.magic-wand", "x": s(45.0), "y": s(110.0), "tolerance": 48, "sample_size": 5, "contiguous": true, "anti_alias": true}), &[]).unwrap();
    let sel = ed.doc.selection.clone().unwrap();
    save_selection("select_wand_canton", &sel, 800, Some(Rect::new(0, 0, s(120.0) as i32, s(360.0) as i32)));
    let b = sel.content_bounds();
    // It must stay in the canton (left of the face, above the stripes'
    // lower half) and not leak into the red curtain or the suit.
    assert!(b.right() < s(110.0) as i32, "leaked right: {b:?}");
    assert!(b.bottom() < s(400.0) as i32, "leaked down: {b:?}");
    assert!(frac_selected(&sel, Rect::new(s(300.0) as i32, s(450.0) as i32, 200, 200)) == 0.0, "suit untouched");
    let area = sel.to_raw().iter().filter(|&&c| c >= 128).count();
    assert!(area > 5_000, "selected a real region: {area}");
}

#[test]
fn magic_wand_global_on_the_curtain() {
    let mut ed = editor();
    ed.exec(json!({"op": "select.magic-wand", "x": s(150.0), "y": s(150.0), "tolerance": 28, "contiguous": false}), &[]).unwrap();
    let sel = ed.doc.selection.clone().unwrap();
    save_selection("select_wand_curtain_global", &sel, 800, None);
    // Curtain on both sides of the head, not the face or suit.
    assert!(frac_selected(&sel, Rect::new(s(120.0) as i32, s(100.0) as i32, 120, 120)) > 0.3);
    assert!(frac_selected(&sel, Rect::new(s(300.0) as i32, s(150.0) as i32, 120, 120)) < 0.05, "face");
    assert!(frac_selected(&sel, Rect::new(s(330.0) as i32, s(560.0) as i32, 120, 120)) < 0.02, "suit");
}

fn suit_strokes(ed: &mut Editor) {
    // Three strokes on the jacket: the left lapel/body, the right body, the
    // folded arms. Sent as short segments, as a pointer drag would.
    let strokes: [&[(f64, f64)]; 3] = [
        &[(180.0, 420.0), (130.0, 500.0), (80.0, 580.0), (120.0, 690.0)],
        &[(440.0, 380.0), (500.0, 480.0), (505.0, 600.0)],
        &[(260.0, 640.0), (330.0, 650.0), (420.0, 620.0)],
    ];
    for (k, stroke) in strokes.iter().enumerate() {
        for (i, seg) in stroke.windows(2).enumerate() {
            let pts: Vec<_> = seg.iter().map(|(x, y)| json!({"x": s(*x), "y": s(*y)})).collect();
            let _ = i;
            let mode = if k == 0 { "replace" } else { "add" };
            ed.exec(json!({"op": "select.quick", "points": pts, "radius": s(14.0), "mode": mode, "stroke_id": format!("suit-{k}")}), &[]).unwrap();
        }
        ed.exec(json!({"op": "edit.seal"}), &[]).unwrap();
    }
}

#[test]
fn quick_selection_on_the_suit() {
    let mut ed = editor();
    let t = Instant::now();
    suit_strokes(&mut ed);
    eprintln!("quick select, 3 strokes (7 segments): {:?}", t.elapsed());
    let sel = ed.doc.selection.clone().unwrap();
    save_selection("select_quick_suit", &sel, 800, Some(Rect::new(s(100.0) as i32, s(250.0) as i32, s(200.0) as i32, s(250.0) as i32)));
    // Jacket in, shirt/tie, face and curtain out.
    assert!(frac_selected(&sel, Rect::new(s(160.0) as i32, s(520.0) as i32, 80, 80)) > 0.95, "jacket left");
    assert!(frac_selected(&sel, Rect::new(s(465.0) as i32, s(480.0) as i32, 80, 80)) > 0.95, "jacket right");
    assert!(frac_selected(&sel, Rect::new(s(300.0) as i32, s(150.0) as i32, 150, 150)) < 0.01, "face");
    assert!(frac_selected(&sel, Rect::new(s(120.0) as i32, s(150.0) as i32, 150, 150)) < 0.01, "curtain");
    assert!(frac_selected(&sel, Rect::new(s(268.0) as i32, s(380.0) as i32, 40, 60)) < 0.2, "tie");
    // Three strokes, three undo steps.
    let labels = ed.history.undo_labels();
    assert_eq!(labels.iter().filter(|l| **l == "Quick Selection").count(), 3, "{labels:?}");
}

fn rough_subject(ed: &mut Editor) {
    let poly = [
        (310.0, 8.0),
        (360.0, 18.0),
        (402.0, 60.0),
        (420.0, 125.0),
        (412.0, 175.0),
        (380.0, 250.0),
        (465.0, 292.0),
        (525.0, 315.0),
        (548.0, 360.0),
        (545.0, 500.0),
        (530.0, 640.0),
        (505.0, 720.0),
        (505.0, 800.0),
        (112.0, 800.0),
        (100.0, 712.0),
        (40.0, 690.0),
        (25.0, 600.0),
        (50.0, 470.0),
        (75.0, 390.0),
        (100.0, 338.0),
        (150.0, 312.0),
        (240.0, 275.0),
        (215.0, 180.0),
        (212.0, 110.0),
        (245.0, 40.0),
    ];
    let pts: Vec<_> = poly.iter().map(|(x, y)| json!({"x": s(*x), "y": s(*y)})).collect();
    ed.exec(json!({"op": "select.polygon", "points": pts}), &[]).unwrap();
}

#[test]
fn refine_a_rough_subject_outline() {
    let mut ed = editor();
    rough_subject(&mut ed);
    let rough = ed.doc.selection.clone().unwrap();
    save_selection("select_refine_rough", &rough, 800, None);
    let t = Instant::now();
    ed.exec(json!({"op": "select.refine", "radius": s(24.0), "smooth": 10, "feather": 0, "contrast": 20, "shift_edge": 0}), &[]).unwrap();
    eprintln!("refine radius {:.0}: {:?}", s(24.0), t.elapsed());
    let sel = ed.doc.selection.clone().unwrap();
    save_selection("select_refine_result", &sel, 800, Some(Rect::new(s(190.0) as i32, s(0.0) as i32, s(260.0) as i32, s(300.0) as i32)));
    // Curtain and window inside the rough outline beside the neck are
    // dropped; the face, collar and suit stay.
    let px = |x: f64, y: f64, w: f64, h: f64| Rect::new(s(x) as i32, s(y) as i32, s(w) as i32, s(h) as i32);
    for (x, y, what) in [(400.0, 90.0, "window right of the head"), (60.0, 440.0, "flag left of the arm"), (490.0, 720.0, "desk right of the jacket"), (240.0, 255.0, "curtain left of the neck")] {
        assert!(frac_selected(&rough, px(x, y, 10.0, 10.0)) > 0.99, "rough outline covers the {what}");
        assert!(frac_selected(&sel, px(x, y, 10.0, 10.0)) < 0.02, "{what} dropped");
    }
    assert!(frac_selected(&sel, px(280.0, 100.0, 60.0, 100.0)) > 0.99, "face");
    assert!(frac_selected(&sel, px(300.0, 500.0, 100.0, 100.0)) > 0.99, "suit");
}

#[test]
#[ignore]
fn timings_12mp() {
    let p = photo();
    // 3000×3747 ≈ 11.2 MP… use 3100×3872 ≈ 12 MP.
    let (w, h) = (3100u32, 3872u32);
    let img = image::RgbaImage::from_raw(p.w, p.h, p.rgba.clone()).unwrap();
    let big = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
    let mut ed = Editor::new(1, 1);
    editor_select::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), big.as_raw()).unwrap();
    let k = w as f64 / 640.0;
    let time = |ed: &mut Editor, label: &str, cmd: serde_json::Value| {
        let t = Instant::now();
        ed.exec(cmd, &[]).unwrap();
        eprintln!("{label}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    };
    time(&mut ed, "magic wand contiguous (canton)", json!({"op": "select.magic-wand", "x": 12.0 * k, "y": 106.0 * k, "tolerance": 40}));
    time(&mut ed, "magic wand contiguous (curtain, large)", json!({"op": "select.magic-wand", "x": 150.0 * k, "y": 150.0 * k, "tolerance": 40}));
    time(&mut ed, "magic wand global", json!({"op": "select.magic-wand", "x": 150.0 * k, "y": 150.0 * k, "tolerance": 28, "contiguous": false}));
    time(&mut ed, "quick select first segment", json!({"op": "select.quick", "points": [{"x": 150.0 * k, "y": 430.0 * k}, {"x": 170.0 * k, "y": 520.0 * k}], "radius": 14.0 * k, "mode": "replace", "stroke_id": "t"}));
    time(&mut ed, "quick select second segment", json!({"op": "select.quick", "points": [{"x": 170.0 * k, "y": 520.0 * k}, {"x": 200.0 * k, "y": 600.0 * k}], "radius": 14.0 * k, "mode": "replace", "stroke_id": "t"}));
    time(&mut ed, "grow 20", json!({"op": "select.grow", "by": 20}));
    time(&mut ed, "color range skin", json!({"op": "select.color-range", "preset": "skin", "fuzziness": 40}));
}
