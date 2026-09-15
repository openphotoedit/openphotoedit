//! Command-level tests: every op through `Editor::exec`, as the UI calls it.

use editor_core::editor::Editor;
use editor_core::geom::Rect;
use editor_core::layer::LayerId;
use editor_core::ops::EditorError;
use editor_core::pixels::read_layer;
use editor_core::render::flatten;
use serde_json::{json, Value};

mod photo;

pub fn editor(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Editor {
    let mut ed = Editor::new(1, 1);
    crate::register(&mut ed);
    let px: Vec<u8> = (0..w * h).flat_map(|i| f(i % w, i / w)).collect();
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), &px).unwrap();
    ed
}

pub fn run(ed: &mut Editor, cmd: Value) -> Value {
    ed.exec(cmd.clone(), &[]).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

pub fn pixels(ed: &Editor) -> Vec<u8> {
    let id = ed.doc.active.unwrap();
    read_layer(&ed.doc, id, Rect::new(0, 0, ed.doc.width as i32, ed.doc.height as i32)).unwrap()
}

fn flat(v: [u8; 4]) -> impl Fn(usize, usize) -> [u8; 4] {
    move |_, _| v
}

/// Deterministic pseudo-random texture.
fn noise_img(x: usize, y: usize) -> [u8; 4] {
    let h = crate::util::hash3(x as i64, y as i64, 7);
    [(h & 255) as u8, ((h >> 8) & 255) as u8, ((h >> 16) & 255) as u8, 255]
}

fn mean(px: &[u8], c: usize) -> f64 {
    px.chunks_exact(4).map(|p| p[c] as f64).sum::<f64>() / (px.len() / 4) as f64
}

fn std(px: &[u8], c: usize) -> f64 {
    let m = mean(px, c);
    (px.chunks_exact(4).map(|p| (p[c] as f64 - m).powi(2)).sum::<f64>() / (px.len() / 4) as f64).sqrt()
}

fn select_rect(ed: &mut Editor, x: i32, y: i32, w: i32, h: i32) {
    run(ed, json!({"op": "select.rect", "x": x, "y": y, "width": w, "height": h}));
}

fn px_at(px: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * w + x) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

// ---------------------------------------------------------------------------
// Plumbing

#[test]
fn unknown_ops_fall_through() {
    let mut doc = editor_core::document::Document::new(4, 4);
    let err = crate::apply(json!({"op": "filter.no-such-thing"}), &mut doc, &[]).unwrap_err();
    assert!(matches!(err, EditorError::UnknownOp(_)), "{err:?}");
    let err = crate::analyze::apply(json!({"op": "analyze.nope"}), &mut doc, &[]).unwrap_err();
    assert!(matches!(err, EditorError::UnknownOp(_)));
    // A known op with bad params is a JSON error, not UnknownOp.
    let err = crate::apply(json!({"op": "filter.gaussian-blur"}), &mut doc, &[]).unwrap_err();
    assert!(matches!(err, EditorError::Json(_)), "{err:?}");
}

#[test]
fn locked_layer_is_refused() {
    let mut ed = editor(8, 8, flat([10, 20, 30, 255]));
    let id = ed.doc.active.unwrap();
    run(&mut ed, json!({"op": "layer.props", "id": id, "locks": {"pixels": true}}));
    for cmd in [json!({"op": "filter.gaussian-blur", "radius": 2}), json!({"op": "filter.invert"}), json!({"op": "filter.spot-heal", "x": 4, "y": 4, "radius": 2})] {
        let err = ed.exec(cmd, &[]).unwrap_err();
        assert!(matches!(err, EditorError::Locked(_)), "{err:?}");
    }
}

#[test]
fn empty_selection_is_an_error() {
    let mut ed = editor(8, 8, flat([10, 20, 30, 255]));
    select_rect(&mut ed, 2, 2, 2, 2);
    run(&mut ed, json!({"op": "select.rect", "x": 2, "y": 2, "width": 2, "height": 2, "mode": "subtract"}));
    let err = ed.exec(json!({"op": "filter.gaussian-blur", "radius": 2}), &[]).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
}

#[test]
fn filters_are_undo_steps_with_photoshop_labels() {
    let mut ed = editor(16, 16, noise_img);
    let before = pixels(&ed);
    let r = run(&mut ed, json!({"op": "filter.gaussian-blur", "radius": 2}));
    assert_eq!(r["label"], "Gaussian Blur");
    assert_eq!(r["changed"], true);
    assert_ne!(pixels(&ed), before);
    run(&mut ed, json!({"op": "edit.undo"}));
    assert_eq!(pixels(&ed), before);
}

#[test]
fn smart_filter_scratch_document_with_explicit_id() {
    // Smart filters run handlers on a tiny scratch document with an `id`.
    let mut ed = editor(3, 2, noise_img);
    let id = ed.doc.active.unwrap();
    for cmd in [
        json!({"op": "filter.gaussian-blur", "radius": 40, "id": id}),
        json!({"op": "filter.median", "radius": 9, "id": id}),
        json!({"op": "filter.reduce-noise", "id": id}),
        json!({"op": "filter.lens-blur", "radius": 12, "id": id}),
        json!({"op": "filter.motion-blur", "angle": 30, "distance": 50, "id": id}),
        json!({"op": "filter.surface-blur", "radius": 5, "threshold": 20, "id": id}),
        json!({"op": "filter.smart-sharpen", "amount": 100, "radius": 2, "id": id}),
        json!({"op": "filter.tilt-shift", "center_y": 1, "band": 0, "feather": 1, "radius": 5, "id": id}),
        json!({"op": "filter.twirl", "angle": 90, "id": id}),
        json!({"op": "filter.pixelate", "cell": 8, "id": id}),
        json!({"op": "filter.radial-blur", "amount": 50, "id": id}),
        json!({"op": "filter.emboss", "id": id}),
    ] {
        run(&mut ed, cmd);
    }
    let mut one = editor(1, 1, flat([200, 100, 50, 255]));
    let id = one.doc.active.unwrap();
    run(&mut one, json!({"op": "filter.gaussian-blur", "radius": 10, "id": id}));
    assert_eq!(pixels(&one), vec![200, 100, 50, 255]);
}

// ---------------------------------------------------------------------------
// Blurs

#[test]
fn gaussian_keeps_flat_colour_and_mean() {
    for radius in [0.0, 0.5, 3.0, 30.0] {
        let mut ed = editor(40, 30, flat([37, 150, 222, 255]));
        run(&mut ed, json!({"op": "filter.gaussian-blur", "radius": radius}));
        assert!(pixels(&ed).chunks_exact(4).all(|p| p == [37, 150, 222, 255]), "radius {radius}");
    }
    let mut ed = editor(64, 64, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.gaussian-blur", "radius": 4}));
    let after = pixels(&ed);
    for c in 0..3 {
        assert!((mean(&before, c) - mean(&after, c)).abs() < 1.5, "mean kept");
        assert!(std(&after, c) < std(&before, c) * 0.25, "detail removed");
    }
}

#[test]
fn blur_does_not_darken_transparent_edges() {
    let mut ed = editor(40, 40, |x, y| if (10..30).contains(&x) && (10..30).contains(&y) { [255, 40, 0, 255] } else { [0, 0, 0, 0] });
    run(&mut ed, json!({"op": "filter.gaussian-blur", "radius": 4}));
    let px = pixels(&ed);
    let edge = px_at(&px, 40, 8, 20);
    assert!(edge[3] > 10 && edge[3] < 200, "soft alpha {edge:?}");
    assert!(edge[0] >= 250 && (edge[1] as i32 - 40).abs() <= 2, "colour stays, not black: {edge:?}");
}

#[test]
fn blur_is_seamless_at_selection_edges() {
    let mut whole = editor(60, 40, noise_img);
    run(&mut whole, json!({"op": "filter.gaussian-blur", "radius": 3}));
    let mut part = editor(60, 40, noise_img);
    select_rect(&mut part, 20, 10, 25, 20);
    run(&mut part, json!({"op": "filter.gaussian-blur", "radius": 3}));
    let (a, b) = (pixels(&whole), pixels(&part));
    let orig = pixels(&editor(60, 40, noise_img));
    for y in 0..40 {
        for x in 0..60 {
            let (pa, pb) = (px_at(&a, 60, x, y), px_at(&b, 60, x, y));
            if (20..45).contains(&x) && (10..30).contains(&y) {
                for c in 0..3 {
                    assert!((pa[c] as i32 - pb[c] as i32).abs() <= 1, "({x},{y}) inside matches the whole-image blur");
                }
            } else {
                assert_eq!(pb, px_at(&orig, 60, x, y), "outside untouched");
            }
        }
    }
}

#[test]
fn box_and_motion_blur_keep_flat_and_mean() {
    for cmd in [json!({"op": "filter.box-blur", "radius": 3.5}), json!({"op": "filter.motion-blur", "angle": 27, "distance": 15}), json!({"op": "filter.motion-blur", "angle": 80, "distance": 9})] {
        let mut ed = editor(30, 30, flat([90, 91, 92, 255]));
        run(&mut ed, cmd.clone());
        assert!(pixels(&ed).chunks_exact(4).all(|p| p == [90, 91, 92, 255]), "{cmd}");
        let mut ed = editor(64, 64, noise_img);
        let before = pixels(&ed);
        run(&mut ed, cmd.clone());
        let after = pixels(&ed);
        assert!((mean(&before, 0) - mean(&after, 0)).abs() < 2.0, "{cmd}");
        assert!(std(&after, 0) < std(&before, 0) * 0.5, "{cmd}");
    }
}

#[test]
fn motion_blur_streaks_along_its_angle() {
    let mut ed = editor(41, 41, |x, y| if x == 20 && y == 20 { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
    run(&mut ed, json!({"op": "filter.motion-blur", "angle": 0, "distance": 11}));
    let px = pixels(&ed);
    assert!(px_at(&px, 41, 25, 20)[0] > 15);
    assert_eq!(px_at(&px, 41, 20, 24)[0], 0);
}

#[test]
fn radial_blur_spins_about_the_centre() {
    let mut ed = editor(64, 64, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.radial-blur", "amount": 30, "mode": "spin"}));
    let after = pixels(&ed);
    assert_eq!(px_at(&after, 64, 32, 32), px_at(&before, 64, 32, 32), "centre stays sharp");
    assert!(std(&after, 1) < std(&before, 1) * 0.8);
    let mut ed = editor(32, 32, flat([5, 100, 200, 255]));
    run(&mut ed, json!({"op": "filter.radial-blur", "amount": 60, "mode": "zoom", "cx": 3, "cy": 3}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [5, 100, 200, 255]));
}

#[test]
fn surface_blur_smooths_but_keeps_edges() {
    let img = |x: usize, y: usize| {
        let n = (crate::util::hash3(x as i64, y as i64, 3) % 9) as i32 - 4;
        let base = if x < 20 { 60 } else { 190 };
        let v = (base + n) as u8;
        [v, v, v, 255]
    };
    let mut ed = editor(40, 20, img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.surface-blur", "radius": 4, "threshold": 15}));
    let px = pixels(&ed);
    assert!((px_at(&px, 40, 19, 10)[0] as i32 - 60).abs() <= 4, "edge left side stays dark");
    assert!((px_at(&px, 40, 20, 10)[0] as i32 - 190).abs() <= 4, "edge right side stays light");
    let left_std = |p: &[u8]| {
        let v: Vec<u8> = (0..20).flat_map(|y| (2..16).map(move |x| (x, y))).flat_map(|(x, y)| px_at(p, 40, x, y)).collect();
        std(&v, 0)
    };
    assert!(left_std(&px) < left_std(&before) * 0.5);
}

#[test]
fn lens_blur_uniform_and_depth() {
    let mut ed = editor(30, 30, flat([120, 60, 30, 255]));
    run(&mut ed, json!({"op": "filter.lens-blur", "radius": 6}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p.iter().zip([120, 60, 30, 255]).all(|(a, b)| (*a as i32 - b).abs() <= 1)));
    // Depth 0 everywhere with focal 0: nothing blurs.
    let mut ed = editor(30, 30, noise_img);
    let before = pixels(&ed);
    ed.exec(json!({"op": "filter.lens-blur", "radius": 6, "depth": true}), &vec![0u8; 900]).unwrap();
    assert_eq!(pixels(&ed), before);
    // Right half far away: only it blurs.
    let depth: Vec<u8> = (0..900).map(|i| if i % 30 >= 15 { 255 } else { 0 }).collect();
    ed.exec(json!({"op": "filter.lens-blur", "radius": 6, "depth": true}), &depth).unwrap();
    let after = pixels(&ed);
    assert_eq!(px_at(&after, 30, 2, 10), px_at(&before, 30, 2, 10));
    assert_ne!(px_at(&after, 30, 25, 10), px_at(&before, 30, 25, 10));
    assert!(ed.exec(json!({"op": "filter.lens-blur", "radius": 6}), &[1, 2, 3]).is_err(), "wrong depth size");
}

#[test]
fn tilt_shift_keeps_band_sharp() {
    let mut ed = editor(40, 100, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.tilt-shift", "center_y": 50, "band": 10, "feather": 20, "radius": 4}));
    let after = pixels(&ed);
    let row = |p: &[u8], y: usize| -> Vec<u8> { p[y * 40 * 4..(y + 1) * 40 * 4].to_vec() };
    assert_eq!(row(&after, 50), row(&before, 50), "band untouched");
    assert!(std(&row(&after, 5), 0) < std(&row(&before, 5), 0) * 0.5, "far rows blurred");
}

// ---------------------------------------------------------------------------
// Sharpen

#[test]
fn unsharp_mask_amount_and_threshold() {
    let img = |x: usize, _| if x < 10 { [100, 100, 100, 255] } else { [103, 103, 103, 255] };
    let mut ed = editor(20, 5, img);
    run(&mut ed, json!({"op": "filter.unsharp-mask", "amount": 200, "radius": 1, "threshold": 10}));
    assert_eq!(pixels(&ed), pixels(&editor(20, 5, img)), "differences below threshold untouched");
    run(&mut ed, json!({"op": "filter.unsharp-mask", "amount": 200, "radius": 1, "threshold": 0}));
    let px = pixels(&ed);
    assert!(px_at(&px, 20, 9, 2)[0] < 100 && px_at(&px, 20, 10, 2)[0] > 103, "edge overshoots both ways");
    let mut ed = editor(20, 20, flat([80, 80, 80, 255]));
    run(&mut ed, json!({"op": "filter.unsharp-mask", "amount": 500, "radius": 3, "threshold": 0}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [80, 80, 80, 255]));
}

#[test]
fn smart_sharpen_adds_edge_contrast_with_limited_halo() {
    let img = |x: usize, _| if x < 20 { [60, 60, 60, 255] } else { [180, 180, 180, 255] };
    let mut ed = editor(40, 10, img);
    run(&mut ed, json!({"op": "filter.smart-sharpen", "amount": 150, "radius": 2, "reduce_noise": 10}));
    let px = pixels(&ed);
    let (l, r) = (px_at(&px, 40, 19, 5)[0], px_at(&px, 40, 20, 5)[0]);
    assert!(l < 60 && r > 180, "sharper: {l} {r}");
    assert!(l > 30 && r < 215, "halo limited: {l} {r}");
    assert_eq!(px_at(&px, 40, 2, 5)[0], 60, "flat stays");
}

#[test]
fn high_pass_is_grey_where_flat() {
    let mut ed = editor(20, 20, flat([10, 200, 90, 255]));
    run(&mut ed, json!({"op": "filter.high-pass", "radius": 3}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [128, 128, 128, 255]));
}

// ---------------------------------------------------------------------------
// Noise

#[test]
fn add_noise_is_zero_mean_and_mono_when_asked() {
    let mut ed = editor(100, 100, flat([128, 128, 128, 255]));
    run(&mut ed, json!({"op": "filter.add-noise", "amount": 20, "gaussian": true, "monochromatic": true}));
    let px = pixels(&ed);
    assert!((mean(&px, 0) - 128.0).abs() < 1.5);
    assert!(std(&px, 0) > 8.0);
    assert!(px.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2]));
    let mut ed2 = editor(100, 100, flat([128, 128, 128, 255]));
    run(&mut ed2, json!({"op": "filter.add-noise", "amount": 20, "gaussian": true, "monochromatic": true}));
    assert_eq!(pixels(&ed2), px, "deterministic");
    let mut ed3 = editor(100, 100, flat([128, 128, 128, 255]));
    run(&mut ed3, json!({"op": "filter.add-noise", "amount": 20}));
    let px3 = pixels(&ed3);
    assert!(px3.chunks_exact(4).any(|p| p[0] != p[1]));
    assert!(px3.chunks_exact(4).all(|p| (p[0] as i32 - 128).abs() <= 26), "uniform noise is bounded");
}

#[test]
fn reduce_noise_flattens_noise_and_keeps_flat() {
    let mut ed = editor(128, 128, |x, y| {
        let h = crate::util::hash3(x as i64, y as i64, 11);
        let n = |s: u64| ((h >> s) & 63) as i32 - 32;
        [(120 + n(0) / 2) as u8, (120 + n(8) / 2) as u8, (120 + n(16) / 2) as u8, 255]
    });
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.reduce-noise", "strength": 8, "preserve_details": 20, "reduce_color_noise": 60}));
    let after = pixels(&ed);
    for c in 0..3 {
        assert!(std(&after, c) < std(&before, c) * 0.35, "c{c}: {} -> {}", std(&before, c), std(&after, c));
    }
    let mut ed = editor(40, 40, flat([33, 66, 99, 255]));
    run(&mut ed, json!({"op": "filter.reduce-noise"}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p.iter().zip([33, 66, 99, 255]).all(|(a, b)| (*a as i32 - b).abs() <= 1)));
}

fn brute(src: &[u8], w: usize, h: usize, r: isize, f: impl Fn(&mut Vec<u8>) -> u8) -> Vec<u8> {
    let mut out = src.to_vec();
    for y in 0..h as isize {
        for x in 0..w as isize {
            for c in 0..4 {
                let mut v = Vec::new();
                for dy in -r..=r {
                    for dx in -r..=r {
                        let xx = (x + dx).clamp(0, w as isize - 1) as usize;
                        let yy = (y + dy).clamp(0, h as isize - 1) as usize;
                        v.push(src[(yy * w + xx) * 4 + c]);
                    }
                }
                out[(y as usize * w + x as usize) * 4 + c] = f(&mut v);
            }
        }
    }
    out
}

#[test]
fn median_minimum_maximum_match_brute_force() {
    let (w, h) = (37, 23);
    let src = pixels(&editor(w, h, noise_img));
    for r in [1isize, 2, 5] {
        let mut ed = editor(w, h, noise_img);
        run(&mut ed, json!({"op": "filter.median", "radius": r}));
        let want = brute(&src, w, h, r, |v| {
            v.sort();
            v[v.len() / 2]
        });
        assert_eq!(pixels(&ed), want, "median r{r}");
        let mut ed = editor(w, h, noise_img);
        run(&mut ed, json!({"op": "filter.maximum", "radius": r}));
        assert_eq!(pixels(&ed), brute(&src, w, h, r, |v| *v.iter().max().unwrap()), "maximum r{r}");
        let mut ed = editor(w, h, noise_img);
        run(&mut ed, json!({"op": "filter.minimum", "radius": r}));
        assert_eq!(pixels(&ed), brute(&src, w, h, r, |v| *v.iter().min().unwrap()), "minimum r{r}");
    }
}

#[test]
fn dust_and_scratches_only_replaces_outliers() {
    let img = |x: usize, y: usize| {
        if x == 10 && y == 10 {
            [255, 255, 255, 255]
        } else {
            let v = 100 + ((x + y) % 3) as u8;
            [v, v, v, 255]
        }
    };
    let mut ed = editor(20, 20, img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.dust-and-scratches", "radius": 2, "threshold": 20}));
    let after = pixels(&ed);
    assert!(px_at(&after, 20, 10, 10)[0] <= 102, "speck removed");
    assert_eq!(px_at(&after, 20, 3, 4), px_at(&before, 20, 3, 4), "texture below threshold kept");
}

// ---------------------------------------------------------------------------
// Stylize and friends

#[test]
fn pixelate_cells_are_uniform_block_means() {
    let (w, h) = (30, 20);
    let mut ed = editor(w, h, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.pixelate", "cell": 8}));
    let after = pixels(&ed);
    for by in (0..h).step_by(8) {
        for bx in (0..w).step_by(8) {
            let first = px_at(&after, w, bx, by);
            let mut sum = 0u32;
            let mut n = 0;
            for y in by..(by + 8).min(h) {
                for x in bx..(bx + 8).min(w) {
                    assert_eq!(px_at(&after, w, x, y), first);
                    sum += px_at(&before, w, x, y)[0] as u32;
                    n += 1;
                }
            }
            assert!((first[0] as i32 - (sum as f32 / n as f32).round() as i32).abs() <= 1);
        }
    }
}

#[test]
fn point_filters() {
    let mut ed = editor(4, 1, |x, _| [[0, 100, 200, 255], [255, 128, 127, 255], [10, 20, 30, 128], [1, 2, 3, 0]][x]);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.invert"}));
    assert_eq!(&pixels(&ed)[..4], &[255, 155, 55, 255]);
    run(&mut ed, json!({"op": "filter.invert"}));
    assert_eq!(pixels(&ed), before);
    run(&mut ed, json!({"op": "filter.solarize"}));
    assert_eq!(&pixels(&ed)[4..8], &[0, 127, 127, 255]);
    let mut ed = editor(1, 1, flat([200, 50, 100, 255]));
    run(&mut ed, json!({"op": "filter.desaturate"}));
    assert_eq!(pixels(&ed), vec![125, 125, 125, 255]);
}

#[test]
fn emboss_and_find_edges_on_flat() {
    let mut ed = editor(16, 16, flat([30, 60, 90, 255]));
    run(&mut ed, json!({"op": "filter.emboss", "angle": 135, "height": 3, "amount": 100}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [128, 128, 128, 255]));
    let mut ed = editor(16, 16, flat([30, 60, 90, 255]));
    run(&mut ed, json!({"op": "filter.find-edges"}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [255, 255, 255, 255]));
    let mut ed = editor(16, 16, |x, _| if x < 8 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
    run(&mut ed, json!({"op": "filter.find-edges"}));
    let px = pixels(&ed);
    assert!(px_at(&px, 16, 8, 8)[0] < 100 && px_at(&px, 16, 2, 8)[0] == 255);
}

#[test]
fn distortions_identity_at_zero_and_flat_stays_flat() {
    for (op, key) in [("filter.twirl", "angle"), ("filter.pinch", "amount"), ("filter.spherize", "amount"), ("filter.ripple", "amount")] {
        let mut ed = editor(32, 32, noise_img);
        let before = pixels(&ed);
        run(&mut ed, json!({"op": op, key: 0}));
        assert_eq!(pixels(&ed), before, "{op} at zero");
        for v in [-80, 80] {
            let mut ed = editor(32, 32, flat([7, 77, 177, 255]));
            run(&mut ed, json!({"op": op, key: v}));
            assert!(pixels(&ed).chunks_exact(4).all(|p| p == [7, 77, 177, 255]), "{op} {v}");
            let mut ed = editor(32, 32, noise_img);
            run(&mut ed, json!({"op": op, key: v}));
            assert_ne!(pixels(&ed), before, "{op} {v} changes the image");
        }
    }
    // Twirl leaves corners outside the inscribed circle alone.
    let mut ed = editor(32, 32, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.twirl", "angle": 300}));
    assert_eq!(px_at(&pixels(&ed), 32, 0, 0), px_at(&before, 32, 0, 0));
}

#[test]
fn polar_coordinates_maps_top_to_centre() {
    let img = |_: usize, y: usize| if y < 32 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
    let mut ed = editor(64, 64, img);
    run(&mut ed, json!({"op": "filter.polar-coordinates", "to_polar": true}));
    let px = pixels(&ed);
    assert_eq!(px_at(&px, 64, 32, 32)[0], 255, "top row lands at the centre");
    assert_eq!(px_at(&px, 64, 1, 1)[2], 255, "bottom rows at the corners");
    let mut ed = editor(64, 64, img);
    run(&mut ed, json!({"op": "filter.polar-coordinates", "to_polar": false}));
    let px = pixels(&ed);
    assert_eq!(px_at(&px, 64, 32, 2)[2], 255, "centre (bottom half… blue) spreads along the top row");
}

#[test]
fn offset_wraps_round_trip_and_clears_without_wrap() {
    let mut ed = editor(23, 17, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.offset", "dx": 7, "dy": -5, "wrap": true}));
    let moved = pixels(&ed);
    assert_eq!(px_at(&moved, 23, 7, 12), px_at(&before, 23, 0, 0));
    run(&mut ed, json!({"op": "filter.offset", "dx": -7, "dy": 5, "wrap": true}));
    assert_eq!(pixels(&ed), before);
    let mut ed = editor(23, 17, noise_img);
    run(&mut ed, json!({"op": "filter.offset", "dx": 3, "dy": 0, "wrap": false}));
    let px = pixels(&ed);
    assert_eq!(px_at(&px, 23, 1, 4)[3], 0);
    assert_eq!(px_at(&px, 23, 3, 4), px_at(&before, 23, 0, 4));
}

#[test]
fn clouds_between_colours_and_seeded() {
    let mut ed = editor(64, 48, flat([0, 0, 0, 0]));
    run(&mut ed, json!({"op": "filter.clouds", "seed": 4, "fg": {"r": 20, "g": 40, "b": 200}, "bg": {"r": 240, "g": 240, "b": 250}}));
    let px = pixels(&ed);
    assert!(px.chunks_exact(4).all(|p| p[3] == 255 && (20..=240).contains(&p[0]) && (200..=250).contains(&p[2])));
    assert!(std(&px, 0) > 5.0, "not uniform");
    let mut ed2 = editor(64, 48, flat([0, 0, 0, 0]));
    run(&mut ed2, json!({"op": "filter.clouds", "seed": 4, "fg": {"r": 20, "g": 40, "b": 200}, "bg": {"r": 240, "g": 240, "b": 250}}));
    assert_eq!(pixels(&ed2), px);
}

#[test]
fn custom_kernel() {
    let mut ed = editor(20, 20, noise_img);
    let before = pixels(&ed);
    run(&mut ed, json!({"op": "filter.custom", "kernel": [0, 0, 0, 0, 1, 0, 0, 0, 0], "scale": 1, "offset": 0}));
    assert_eq!(pixels(&ed), before);
    let mut ed = editor(20, 20, flat([50, 60, 70, 255]));
    run(&mut ed, json!({"op": "filter.custom", "kernel": vec![1; 25], "scale": 25, "offset": 10}));
    assert!(pixels(&ed).chunks_exact(4).all(|p| p == [60, 70, 80, 255]));
    assert!(ed.exec(json!({"op": "filter.custom", "kernel": [1, 2, 3, 4]}), &[]).is_err());
}

#[test]
fn apply_adjustment_matches_invert() {
    let mut ed = editor(10, 10, noise_img);
    let before = pixels(&ed);
    let r = run(&mut ed, json!({"op": "filter.apply-adjustment", "adjustment": {"kind": "invert"}}));
    assert_eq!(r["label"], "Invert");
    let after = pixels(&ed);
    for (a, b) in before.chunks_exact(4).zip(after.chunks_exact(4)) {
        assert_eq!([255 - a[0], 255 - a[1], 255 - a[2], a[3]], [b[0], b[1], b[2], b[3]]);
    }
}

#[test]
fn vignette_darkens_corners_not_centre() {
    let mut ed = editor(60, 40, flat([180, 180, 180, 255]));
    run(&mut ed, json!({"op": "filter.vignette", "amount": -80, "midpoint": 40, "feather": 50}));
    let px = pixels(&ed);
    assert_eq!(px_at(&px, 60, 30, 20)[0], 180);
    assert!(px_at(&px, 60, 0, 0)[0] < 110);
    let mut ed = editor(60, 40, flat([80, 80, 80, 255]));
    run(&mut ed, json!({"op": "filter.vignette", "amount": 60}));
    assert!(px_at(&pixels(&ed), 60, 59, 39)[0] > 120);
}

// ---------------------------------------------------------------------------
// Retouch

#[test]
fn content_aware_fill_continues_a_pattern() {
    // Diagonal stripes: a fill that copies texture reproduces them.
    let img = |x: usize, y: usize| if (x + y) % 12 < 6 { [200, 60, 40, 255] } else { [30, 90, 160, 255] };
    let (w, h) = (120, 100);
    let mut ed = editor(w, h, img);
    let orig = pixels(&ed);
    // Paint a grey blob over the hole so its content must not survive.
    select_rect(&mut ed, 45, 35, 30, 30);
    run(&mut ed, json!({"op": "layer.fill-selection", "color": {"r": 128, "g": 128, "b": 128}}));
    let r = run(&mut ed, json!({"op": "filter.content-aware-fill"}));
    assert_eq!(r["label"], "Content-Aware Fill");
    let px = pixels(&ed);
    let mut err = 0f64;
    for y in 35..65 {
        for x in 45..75 {
            let (a, b) = (px_at(&px, w, x, y), px_at(&orig, w, x, y));
            err += (0..3).map(|c| (a[c] as f64 - b[c] as f64).abs()).sum::<f64>() / 3.0;
        }
    }
    let mae = err / 900.0;
    assert!(mae < 12.0, "stripes reproduced, mean abs error {mae}");
    let mut ed = editor(20, 20, img);
    assert!(ed.exec(json!({"op": "filter.content-aware-fill"}), &[]).is_err(), "needs a selection");
}

#[test]
fn spot_heal_removes_a_blemish_and_matches_lighting() {
    let (w, h) = (96, 96);
    let base = |x: usize, y: usize| {
        let n = (crate::util::hash3(x as i64, y as i64, 5) % 11) as i32 - 5;
        let v = (80 + x as i32 + n).clamp(0, 255) as u8;
        [v, (v as i32 * 3 / 4) as u8, (v / 2), 255]
    };
    let spot = move |x: usize, y: usize| {
        let d = ((x as f32 - 48.0).powi(2) + (y as f32 - 48.0).powi(2)).sqrt();
        if d < 6.0 {
            [20, 10, 10, 255]
        } else {
            base(x, y)
        }
    };
    let mut ed = editor(w, h, spot);
    run(&mut ed, json!({"op": "filter.spot-heal", "x": 48, "y": 48, "radius": 8}));
    let px = pixels(&ed);
    for (x, y) in [(48, 48), (45, 50), (51, 46)] {
        let (a, b) = (px_at(&px, w, x, y), base(x, y));
        assert!((a[0] as i32 - b[0] as i32).abs() < 16, "({x},{y}) healed to the local gradient: {a:?} vs {b:?}");
    }
    assert_eq!(px_at(&px, w, 10, 10), base(10, 10), "outside untouched");
}

#[test]
fn red_eye_darkens_red_pupils_only() {
    let img = |x: usize, y: usize| {
        let d = ((x as f32 - 20.0).powi(2) + (y as f32 - 20.0).powi(2)).sqrt();
        if d < 6.0 {
            [220, 40, 40, 255]
        } else {
            [230, 190, 160, 255]
        }
    };
    let mut ed = editor(40, 40, img);
    run(&mut ed, json!({"op": "filter.red-eye", "x": 10, "y": 10, "width": 20, "height": 20}));
    let px = pixels(&ed);
    let c = px_at(&px, 40, 20, 20);
    assert!(c[0] < 60 && (c[0] as i32 - c[1] as i32).abs() < 12, "pupil dark and neutral: {c:?}");
    assert_eq!(px_at(&px, 40, 12, 20), [230, 190, 160, 255], "skin untouched");
    assert_eq!(px_at(&px, 40, 2, 2), [230, 190, 160, 255]);
}

#[test]
fn frequency_separation_reconstructs_exactly() {
    let mut ed = editor(50, 40, noise_img);
    let base = ed.doc.active.unwrap();
    let before = flatten(&ed.doc);
    let r = run(&mut ed, json!({"op": "filter.frequency-separation", "radius": 4}));
    let ids: Vec<LayerId> = r["data"]["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as LayerId).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ed.doc.layers.len(), 3);
    assert_eq!(ed.doc.layers[0].id, base);
    assert_eq!(ed.doc.layers[1].name, "Low frequency");
    assert_eq!(ed.doc.layers[2].name, "High frequency");
    assert_eq!(ed.doc.layers[2].blend, editor_core::blend::BlendMode::LinearLight);
    assert_eq!(flatten(&ed.doc), before, "low + high in Linear Light is the original");
    let high = read_layer(&ed.doc, ids[1], Rect::new(0, 0, 50, 40)).unwrap();
    assert!((mean(&high, 0) - 127.5).abs() < 3.0, "high frequency sits on mid grey");
}

// ---------------------------------------------------------------------------
// Analysis

#[test]
fn analysis_is_not_an_undo_step() {
    let mut ed = editor(10, 10, noise_img);
    let r = run(&mut ed, json!({"op": "analyze.histogram"}));
    assert_eq!(r["changed"], false);
}

#[test]
fn histogram_sums_to_pixel_count() {
    let mut ed = editor(37, 21, noise_img);
    let r = run(&mut ed, json!({"op": "analyze.histogram"}));
    for k in ["r", "g", "b", "l"] {
        let s: u64 = r["data"][k].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).sum();
        assert_eq!(s, 37 * 21, "{k}");
    }
    let mut ed = editor(4, 4, flat([10, 20, 30, 255]));
    let id = ed.doc.active.unwrap();
    let r = run(&mut ed, json!({"op": "analyze.histogram", "merged": false, "id": id, "rect": {"x": 0, "y": 0, "w": 2, "h": 2}}));
    assert_eq!(r["data"]["r"][10], 4);
    assert_eq!(r["data"]["l"][18], 4, "0.3·10 + 0.59·20 + 0.11·30 = 18.1");
    select_rect(&mut ed, 0, 0, 3, 1);
    let r = run(&mut ed, json!({"op": "analyze.histogram"}));
    assert_eq!(r["data"]["g"][20], 3, "selection only");
}

#[test]
fn auto_levels_modes() {
    let img = |x: usize, y: usize| {
        let v = 50 + ((x * 7 + y * 13) % 151) as u8;
        [v, v / 2 + 40, v, 255]
    };
    let mut ed = editor(100, 100, img);
    let r = run(&mut ed, json!({"op": "analyze.auto-levels", "mode": "tone"}));
    let red = &r["data"]["levels"]["red"];
    assert!((red["in_black"].as_f64().unwrap() - 50.0).abs() <= 1.0 && (red["in_white"].as_f64().unwrap() - 200.0).abs() <= 1.0, "{red}");
    let green = &r["data"]["levels"]["green"];
    assert!((green["in_black"].as_f64().unwrap() - 65.0).abs() <= 1.0, "{green}");
    let r = run(&mut ed, json!({"op": "analyze.auto-levels", "mode": "contrast"}));
    let m = &r["data"]["levels"]["master"];
    assert!((m["in_black"].as_f64().unwrap() - 50.0).abs() <= 1.0 && (m["in_white"].as_f64().unwrap() - 200.0).abs() <= 1.0, "{m}");
    let r = run(&mut ed, json!({"op": "analyze.auto-levels", "mode": "color"}));
    assert!(r["data"]["levels"]["blue"]["in_white"].as_f64().unwrap() > 150.0);
}

#[test]
fn auto_brightens_dark_images_and_bw_style() {
    let mut ed = editor(80, 60, |x, y| {
        let v = (10 + (x + y) / 3) as u8;
        [v, v, v, 255]
    });
    let r = run(&mut ed, json!({"op": "analyze.auto", "style": "auto"}));
    let d = &r["data"]["develop"];
    assert!(d["exposure"].as_f64().unwrap() > 0.4, "{d}");
    assert!(d["temperature"].as_f64().unwrap().abs() < 3.0, "grey stays neutral: {d}");
    let r = run(&mut ed, json!({"op": "analyze.auto", "style": "bw"}));
    assert_eq!(r["data"]["develop"]["black_white"], 100.0);
    // A blue cast on neutral content is corrected by warming.
    let mut ed = editor(80, 60, |x, y| {
        let v = (60 + (x + y)) as u8;
        [(v as f32 * 0.8) as u8, v, (v as f32 * 1.15).min(255.0) as u8, 255]
    });
    let r = run(&mut ed, json!({"op": "analyze.auto"}));
    assert!(r["data"]["develop"]["temperature"].as_f64().unwrap() > 10.0, "{}", r["data"]);
}

#[test]
fn facts_measure_noise_clipping_and_tilt() {
    // Gaussian-ish noise of known sigma on mid grey.
    let sigma = 8.0f32;
    let mut ed = editor(300, 300, move |x, y| {
        let u1 = crate::util::unit(crate::util::hash3(x as i64, y as i64, 1)).max(1e-6);
        let u2 = crate::util::unit(crate::util::hash3(x as i64, y as i64, 2));
        let n = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * sigma;
        let v = (128.0 + n).round().clamp(0.0, 255.0) as u8;
        [v, v, v, 255]
    });
    let f = run(&mut ed, json!({"op": "analyze.facts"}))["data"].clone();
    let measured = f["noise_sigma"].as_f64().unwrap();
    assert!((measured - 8.0).abs() < 1.5, "noise sigma {measured}");
    assert_eq!(f["clipping_high"], 0.0);
    assert!(f["sharpness"].as_f64().unwrap() > 100.0);

    // Horizontal bands tilted 3° clockwise on screen: leveling turns -3°.
    let t = 3f32.to_radians().tan();
    let mut ed = editor(400, 300, move |x, y| {
        let yy = y as f32 - x as f32 * t;
        if (yy.rem_euclid(60.0)) < 30.0 {
            [230, 230, 230, 255]
        } else {
            [30, 30, 30, 255]
        }
    });
    let f = run(&mut ed, json!({"op": "analyze.facts"}))["data"].clone();
    let tilt = f["tilt_degrees"].as_f64().unwrap();
    assert!((tilt + 3.0).abs() < 0.4, "tilt {tilt}");
    // Level image: no tilt.
    let mut ed = editor(400, 300, |_, y| if y % 60 < 30 { [230, 230, 230, 255] } else { [0, 0, 0, 255] });
    let f = run(&mut ed, json!({"op": "analyze.facts"}))["data"].clone();
    assert_eq!(f["tilt_degrees"], 0.0);
    assert!(f["clipping_low"].as_f64().unwrap() > 0.4);
}

#[test]
fn pick_averages_1_3_5() {
    let mut ed = editor(10, 10, |x, y| [(x * 10) as u8, (y * 10) as u8, 7, 255]);
    let p = |ed: &mut Editor, size: u32| run(ed, json!({"op": "analyze.pick", "x": 5.4, "y": 3.9, "size": size}))["data"]["color"].clone();
    assert_eq!(p(&mut ed, 1), json!({"r": 50, "g": 30, "b": 7, "a": 255}));
    assert_eq!(p(&mut ed, 3), json!({"r": 50, "g": 30, "b": 7, "a": 255}));
    let mut ed2 = editor(10, 10, |x, _| if x == 5 { [255, 0, 0, 255] } else { [0, 0, 0, 255] });
    let c = p(&mut ed2, 5);
    assert_eq!(c["r"], 51, "one column of five");
    let id = ed2.doc.active.unwrap();
    let c = run(&mut ed2, json!({"op": "analyze.pick", "x": 5, "y": 5, "merged": false, "id": id}))["data"]["color"].clone();
    assert_eq!(c["r"], 255);
}
