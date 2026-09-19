use editor_core::editor::Editor;
use editor_core::geom::Rect;
use editor_core::layer::{LayerId, LayerKind, ShapeKind};
use editor_core::render::flatten;
use serde_json::{json, Value};

use crate::mesh::identity_grid;

fn editor() -> Editor {
    let mut ed = Editor::new(1, 1);
    crate::register(&mut ed);
    ed
}

fn exec(ed: &mut Editor, v: Value) -> Value {
    ed.exec(v, &[]).unwrap()
}

/// Deterministic noise-free but detailed test pattern.
fn pattern(w: u32, h: u32, alpha: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    let mut s: u32 = 12345;
    for y in 0..h {
        for x in 0..w {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let n = (s >> 16) as u8;
            let a = if alpha { 128u8.wrapping_add(n / 2) } else { 255 };
            out.extend_from_slice(&[(x * 7 + y * 3) as u8, n, (x * y) as u8, a]);
        }
    }
    out
}

fn open(ed: &mut Editor, w: u32, h: u32, px: &[u8]) -> LayerId {
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), px).unwrap();
    ed.doc.active.unwrap()
}

fn layer_px(ed: &Editor, id: LayerId, rect: Rect) -> Vec<u8> {
    editor_core::pixels::read_layer(&ed.doc, id, rect).unwrap()
}

#[test]
fn identity_matrix_is_a_no_op() {
    let mut ed = editor();
    let px = pattern(20, 10, true);
    let id = open(&mut ed, 20, 10, &px);
    let r = exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":1,"b":0,"c":0,"d":1,"e":0,"f":0}}));
    assert_eq!(r["changed"], false);
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, 20, 10)), px);
}

#[test]
fn identity_through_the_resampler_is_lossless() {
    // A quad equal to the bounds goes through the full sampling path.
    let mut ed = editor();
    let px = pattern(40, 30, true);
    let id = open(&mut ed, 40, 30, &px);
    let quad = json!([{"x":0,"y":0},{"x":40,"y":0},{"x":40,"y":30},{"x":0,"y":30}]);
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "quad": quad}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, 40, 30)), px);
    // And the same with a matrix that is identity up to rounding noise.
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":1.0000000001,"b":0,"c":0,"d":1,"e":0,"f":0}}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, 40, 30)), px);
}

#[test]
fn integer_translate_is_exact() {
    let mut ed = editor();
    let px = pattern(16, 12, true);
    let id = open(&mut ed, 16, 12, &px);
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":1,"b":0,"c":0,"d":1,"e":5,"f":-3}}));
    let got = layer_px(&ed, id, Rect::new(5, -3, 16, 12));
    assert_eq!(got, px);
    assert_eq!(ed.summary()["history"]["undo"].as_array().unwrap().last().unwrap(), "Free Transform");
}

#[test]
fn rotate_90_matrix_matches_image_rotate() {
    let (w, h) = (9u32, 6u32);
    let px = pattern(w, h, true);
    let mut a = editor();
    let id = open(&mut a, w, h, &px);
    // Clockwise quarter turn inside a canvas of the rotated size: x' = h - y, y' = x.
    exec(&mut a, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":0,"b":1,"c":-1,"d":0,"e":h,"f":0}}));
    let got = layer_px(&a, id, Rect::new(0, 0, h as i32, w as i32));
    let mut b = editor();
    let idb = open(&mut b, w, h, &px);
    exec(&mut b, json!({"op": "image.rotate", "turns": 1}));
    let want = layer_px(&b, idb, Rect::new(0, 0, h as i32, w as i32));
    assert_eq!(got, want);
}

#[test]
fn perspective_quad_maps_corners() {
    let (w, h) = (100u32, 80u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let colours = [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
    for y in 0..h {
        for x in 0..w {
            let c = match (x < w / 2, y < h / 2) {
                (true, true) => colours[0],
                (false, true) => colours[1],
                (false, false) => colours[2],
                (true, false) => colours[3],
            };
            px[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize].copy_from_slice(&c);
        }
    }
    let mut ed = editor();
    ed.exec(json!({"op": "doc.new", "width": 300, "height": 300}), &[]).unwrap();
    let id = ed.exec(json!({"op": "layer.import", "width": w, "height": h, "x": 20, "y": 30}), &px).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    let quad = [(40.0, 50.0), (250.0, 20.0), (200.0, 260.0), (60.0, 180.0)];
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "quad": quad.iter().map(|(x, y)| json!({"x": x, "y": y})).collect::<Vec<_>>()}));
    let l = ed.doc.find(id).unwrap();
    let r = l.raster().unwrap();
    let (cx, cy) = (quad.iter().map(|p| p.0).sum::<f64>() / 4.0, quad.iter().map(|p| p.1).sum::<f64>() / 4.0);
    for (k, &(qx, qy)) in quad.iter().enumerate() {
        // A little inside the corner, toward the centre.
        let (sx, sy) = (qx + (cx - qx) * 0.08, qy + (cy - qy) * 0.08);
        let p = r.plane.get(sx as i32 - r.x, sy as i32 - r.y);
        assert_eq!(p, colours[k], "corner {k} at {sx},{sy}");
        // Just outside the corner is empty.
        let (ox, oy) = (qx - (cx - qx) * 0.05, qy - (cy - qy) * 0.05);
        assert_eq!(r.plane.get(ox as i32 - r.x, oy as i32 - r.y)[3], 0, "outside corner {k}");
    }
    // The bounds hug the quad.
    let b = l.content_bounds().unwrap();
    assert!((b.x - 40).abs() <= 2 && (b.y - 20).abs() <= 2 && (b.right() - 250).abs() <= 2 && (b.bottom() - 260).abs() <= 2, "{b:?}");
}

#[test]
fn rotated_edges_have_no_dark_fringe() {
    let mut ed = editor();
    ed.exec(json!({"op": "doc.new", "width": 200, "height": 200}), &[]).unwrap();
    let px: Vec<u8> = (0..80 * 80).flat_map(|_| [250u8, 250, 250, 255]).collect();
    let id = ed.exec(json!({"op": "layer.import", "width": 80, "height": 80, "x": 60, "y": 60}), &px).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    let (s, c) = (0.3f64.sin(), 0.3f64.cos());
    let m = json!({"a": c * 0.7, "b": s * 0.7, "c": -s * 0.7, "d": c * 0.7, "e": 30.0, "f": 20.0});
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": m}));
    let r = ed.doc.find(id).unwrap().raster().unwrap().clone();
    let raw = r.plane.to_raw();
    let mut partial = 0;
    for p in raw.chunks_exact(4) {
        if p[3] > 0 {
            assert!(p[0] >= 245, "fringe colour {p:?}");
            if p[3] < 255 {
                partial += 1;
            }
        }
    }
    assert!(partial > 20, "edges are antialiased");
}

#[test]
fn warp_identity_grid_is_near_lossless() {
    let (w, h) = (64u32, 48u32);
    let px = pattern(w, h, false);
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    let grid: Vec<Value> = identity_grid(Rect::new(0, 0, w as i32, h as i32)).iter().map(|p| json!({"x": p.x, "y": p.y})).collect();
    exec(&mut ed, json!({"op": "transform.warp", "id": id, "grid": grid}));
    let got = layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32));
    let mut max = 0;
    for (a, b) in got.iter().zip(px.iter()) {
        max = max.max((*a as i32 - *b as i32).abs());
    }
    assert!(max <= 2, "max diff {max}");
    // Nothing leaks outside.
    let b = ed.doc.find(id).unwrap().content_bounds().unwrap();
    assert_eq!(b, Rect::new(0, 0, w as i32, h as i32));
}

#[test]
fn warp_bulge_moves_centre_content() {
    let (w, h) = (60u32, 60u32);
    let mut ed = editor();
    let px: Vec<u8> = (0..w * h).flat_map(|i| if (i % w) < 30 { [255u8, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
    let id = open(&mut ed, w, h, &px);
    let mut grid = identity_grid(Rect::new(0, 0, 60, 60));
    for k in [5, 9] {
        grid[k].x += 12.0; // push the inner-left column right
    }
    let grid: Vec<Value> = grid.iter().map(|p| json!({"x": p.x, "y": p.y})).collect();
    exec(&mut ed, json!({"op": "transform.warp", "id": id, "grid": grid}));
    let r = ed.doc.find(id).unwrap().raster().unwrap().clone();
    // Red spreads right across the middle row.
    assert_eq!(r.plane.get(32 - r.x, 30 - r.y)[0], 255);
    assert_eq!(r.plane.get(32 - r.x, 1 - r.y)[0], 0, "top edge row barely moves");
    // The corners stay put.
    assert_eq!(r.plane.get(1 - r.x, 1 - r.y), [255, 0, 0, 255]);
}

fn liquify(ed: &mut Editor, id: LayerId, tool: &str, pts: &[(f64, f64)], stroke: &str, pressure: f64) {
    let points: Vec<Value> = pts.iter().map(|(x, y)| json!({"x": x, "y": y})).collect();
    exec(ed, json!({"op": "transform.liquify", "id": id, "tool": tool, "size": 40, "pressure": pressure, "density": 50, "points": points, "stroke_id": stroke}));
}

#[test]
fn liquify_reconstruct_returns_to_original() {
    let (w, h) = (120u32, 90u32);
    let px = pattern(w, h, false);
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    let all = Rect::new(0, 0, w as i32, h as i32);
    liquify(&mut ed, id, "forward", &[(40.0, 45.0), (55.0, 45.0), (70.0, 48.0)], "a", 100.0);
    liquify(&mut ed, id, "bloat", &[(60.0, 40.0)], "b", 100.0);
    assert_ne!(layer_px(&ed, id, all), px, "strokes change pixels");
    for k in 0..12 {
        let pts: Vec<(f64, f64)> = (0..14).map(|i| (10.0 + i as f64 * 8.0, 20.0 + (i % 2) as f64 * 50.0 * 0.0 + (k % 3) as f64 * 25.0)).collect();
        liquify(&mut ed, id, "reconstruct", &pts, &format!("r{k}"), 100.0);
    }
    assert_eq!(layer_px(&ed, id, all), px);
}

#[test]
fn liquify_segments_merge_and_undo_restores() {
    let (w, h) = (100u32, 80u32);
    let px = pattern(w, h, false);
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    let steps = ed.summary()["history"]["undo"].as_array().unwrap().len();
    liquify(&mut ed, id, "twirl-cw", &[(50.0, 40.0)], "s1", 80.0);
    liquify(&mut ed, id, "twirl-cw", &[(52.0, 41.0), (55.0, 43.0)], "s1", 80.0);
    liquify(&mut ed, id, "twirl-cw", &[(58.0, 45.0), (60.0, 45.0)], "s1", 80.0);
    let after = layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32));
    assert_ne!(after, px);
    assert_eq!(ed.summary()["history"]["undo"].as_array().unwrap().len(), steps + 1, "one undo step");
    exec(&mut ed, json!({"op": "edit.undo"}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32)), px);
    // Redo, then a second stroke continues from the same field.
    exec(&mut ed, json!({"op": "edit.redo"}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32)), after);
    liquify(&mut ed, id, "pucker", &[(30.0, 30.0)], "s2", 80.0);
    exec(&mut ed, json!({"op": "edit.undo"}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32)), after);
}

#[test]
fn liquify_segments_do_not_accumulate_blur() {
    // Many tiny forward moves then returning must not soften detail the way
    // repeated resampling would: pixels far from the brush stay exact.
    let (w, h) = (100u32, 100u32);
    let px = pattern(w, h, false);
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    for i in 0..20 {
        liquify(&mut ed, id, "forward", &[(30.0 + i as f64, 50.0), (31.0 + i as f64, 50.0)], "long", 30.0);
    }
    let got = layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32));
    let far = Rect::new(0, 0, 100, 10);
    assert_eq!(&got[..(far.area() * 4) as usize], &px[..(far.area() * 4) as usize]);
}

#[test]
fn seam_carving_reduces_width_and_keeps_object() {
    let (w, h) = (120u32, 60u32);
    // Smooth gradient background with a high-energy checker object.
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let in_obj = (70..90).contains(&x) && (20..40).contains(&y);
            if in_obj {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                px.extend_from_slice(&[v, v, 0, 255]);
            } else {
                px.extend_from_slice(&[(x / 4) as u8 + 50, 100, 120, 255]);
            }
        }
    }
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    exec(&mut ed, json!({"op": "transform.content-aware-scale", "width": 84, "height": 60}));
    assert_eq!((ed.doc.width, ed.doc.height), (84, 60));
    let out = flatten(&ed.doc);
    assert_eq!(out.len(), 84 * 60 * 4);
    // The checker survives intact somewhere on rows 20..40.
    let row = |y: usize| &out[y * 84 * 4..(y + 1) * 84 * 4];
    let mut found = false;
    for x0 in 0..=(84 - 20) {
        let ok = (20..40).all(|y| {
            (0..20).all(|dx| {
                let p = &row(y)[(x0 + dx) * 4..(x0 + dx) * 4 + 4];
                let v = if (70 + dx + y) % 2 == 0 { 255 } else { 0 };
                p[0] == v && p[1] == v
            })
        });
        if ok {
            found = true;
            break;
        }
    }
    assert!(found, "object preserved");
    assert!(ed.doc.find(id).is_some());
}

#[test]
fn seam_insertion_expands_exactly() {
    let px = pattern(50, 30, false);
    let mut ed = editor();
    open(&mut ed, 50, 30, &px);
    exec(&mut ed, json!({"op": "transform.content-aware-scale", "width": 65, "height": 36, "protect_skin": true}));
    assert_eq!((ed.doc.width, ed.doc.height), (65, 36));
    let r = ed.doc.layers[0].raster().unwrap();
    assert_eq!(r.plane.content_bounds(), Rect::new(0, 0, 65, 36));
}

#[test]
fn lens_zero_is_identity() {
    let px = pattern(40, 30, false);
    let mut ed = editor();
    let id = open(&mut ed, 40, 30, &px);
    let r = exec(&mut ed, json!({"op": "transform.lens-correct", "id": id, "distortion": 0, "chromatic_rc": 0, "chromatic_by": 0, "vignette": 0, "scale": 100}));
    assert_eq!(r["changed"], false);
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, 40, 30)), px);
    // A vignette alone leaves geometry exactly in place at the centre.
    exec(&mut ed, json!({"op": "transform.lens-correct", "id": id, "vignette": -60}));
    let got = layer_px(&ed, id, Rect::new(0, 0, 40, 30));
    let c = (15 * 40 + 20) * 4;
    assert_eq!(&got[c..c + 4], &px[c..c + 4]);
    assert!(got[0] as i32 <= px[0] as i32, "corner darker");
}

#[test]
fn lens_barrel_correction_pulls_edges_in() {
    // A vertical line near the left edge bows toward the centre in the
    // middle rows when barrel distortion is removed.
    let (w, h) = (200u32, 150u32);
    let mut px = vec![255u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 20..23 {
            let o = ((y * w + x) * 4) as usize;
            px[o..o + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
    let mut ed = editor();
    let id = open(&mut ed, w, h, &px);
    exec(&mut ed, json!({"op": "transform.lens-correct", "id": id, "distortion": 60, "auto_scale": true}));
    let got = layer_px(&ed, id, Rect::new(0, 0, w as i32, h as i32));
    let darkest = |y: u32| (0..w).min_by_key(|&x| got[((y * w + x) * 4) as usize]).unwrap();
    let mid = darkest(h / 2);
    let top = darkest(2);
    assert!(mid > top + 3, "the line's middle bows inward relative to its ends: mid {mid} top {top}");
}

#[test]
fn selection_pixels_move_and_selection_follows() {
    let mut ed = editor();
    let px: Vec<u8> = (0..40 * 40).flat_map(|_| [200u8, 10, 10, 255]).collect();
    let id = open(&mut ed, 40, 40, &px);
    exec(&mut ed, json!({"op": "select.rect", "x": 5, "y": 5, "width": 10, "height": 10}));
    exec(&mut ed, json!({"op": "transform.selection-pixels", "id": id, "matrix": {"a":1,"b":0,"c":0,"d":1,"e":20,"f":0}}));
    let got = layer_px(&ed, id, Rect::new(0, 0, 40, 40));
    let at = |x: usize, y: usize| &got[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
    assert_eq!(at(8, 8)[3], 0, "hole where pixels were lifted");
    assert_eq!(at(28, 8), &[200, 10, 10, 255]);
    let sel = ed.doc.selection.as_ref().unwrap();
    assert_eq!(sel.get(28, 8)[0], 255);
    assert_eq!(sel.get(8, 8)[0], 0);
    exec(&mut ed, json!({"op": "edit.undo"}));
    assert_eq!(layer_px(&ed, id, Rect::new(0, 0, 40, 40)), px);
}

#[test]
fn text_similarity_updates_parameters_and_skew_rasterizes() {
    let mut ed = editor();
    ed.exec(json!({"op": "doc.new", "width": 200, "height": 200}), &[]).unwrap();
    let px: Vec<u8> = (0..30 * 10).flat_map(|_| [0u8, 0, 0, 255]).collect();
    let data = json!({"text": "Hi", "font_size": 20.0, "x": 10.0, "y": 20.0});
    let id = ed.exec(json!({"op": "layer.add-text", "data": data, "width": 30, "height": 10, "x": 10, "y": 20}), &px).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":0,"b":2,"c":-2,"d":0,"e":100,"f":0}}));
    match &ed.doc.find(id).unwrap().kind {
        LayerKind::Text { data, raster } => {
            assert!((data.font_size - 40.0).abs() < 1e-3);
            assert!((data.rotation - 90.0).abs() < 1e-3);
            assert!((data.x - 60.0).abs() < 1e-3 && (data.y - 20.0).abs() < 1e-3, "{} {}", data.x, data.y);
            let w = raster.plane.content_bounds().w;
            assert!((20..=24).contains(&w), "2x the 10 px height plus antialiasing: {w}");
        }
        _ => panic!("still text"),
    }
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":1,"b":0,"c":0.5,"d":1,"e":0,"f":0}}));
    assert!(matches!(ed.doc.find(id).unwrap().kind, LayerKind::Pixel(_)));
}

#[test]
fn shapes_move_as_vectors_and_groups_move_children() {
    let mut ed = editor();
    ed.exec(json!({"op": "doc.new", "width": 200, "height": 200}), &[]).unwrap();
    let a = ed.exec(json!({"op": "layer.add-shape", "data": {"kind": "rect", "points": [{"x": 20, "y": 20}, {"x": 60, "y": 40}], "fill": {"r": 0, "g": 0, "b": 255}}}), &[]).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    let px: Vec<u8> = (0..10 * 10).flat_map(|_| [255u8, 0, 0, 255]).collect();
    let b = ed.exec(json!({"op": "layer.import", "width": 10, "height": 10, "x": 100, "y": 100}), &px).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    let g = exec(&mut ed, json!({"op": "layer.group", "ids": [a, b]}))["data"]["id"].as_u64().unwrap() as LayerId;
    let (s, c) = (0.5f64.sin(), 0.5f64.cos());
    exec(&mut ed, json!({"op": "transform.layer", "ids": [g], "matrix": {"a": c, "b": s, "c": -s, "d": c, "e": 50, "f": 0}}));
    match &ed.doc.find(a).unwrap().kind {
        LayerKind::Shape { data, .. } => {
            assert_eq!(data.kind, ShapeKind::Polygon);
            assert_eq!(data.points.len(), 4);
            let p = data.points[0];
            assert!((p.x - (c * 20.0 - s * 20.0 + 50.0)).abs() < 1e-9);
        }
        _ => panic!("shape kept"),
    }
    let rb = ed.doc.find(b).unwrap().content_bounds().unwrap();
    let cx = c * 105.0 - s * 105.0 + 50.0;
    assert!((rb.x as f64 + rb.w as f64 / 2.0 - cx).abs() < 2.0, "{rb:?} {cx}");
}

#[test]
fn perspective_crop_of_a_rectangle_is_a_crop() {
    let px = pattern(60, 40, false);
    let mut ed = editor();
    let id = open(&mut ed, 60, 40, &px);
    exec(&mut ed, json!({"op": "select.rect", "x": 1, "y": 1, "width": 5, "height": 5}));
    exec(&mut ed, json!({"op": "transform.perspective-crop", "quad": [{"x":10,"y":5},{"x":40,"y":5},{"x":40,"y":25},{"x":10,"y":25}], "width": 30, "height": 20}));
    assert_eq!((ed.doc.width, ed.doc.height), (30, 20));
    assert!(ed.doc.selection.is_none());
    let got = layer_px(&ed, id, Rect::new(0, 0, 30, 20));
    let mut want = Vec::new();
    for y in 5..25 {
        want.extend_from_slice(&px[(y * 60 + 10) * 4..(y * 60 + 40) * 4]);
    }
    assert_eq!(got, want);
}

#[test]
fn locked_layers_refuse() {
    let mut ed = editor();
    let px = pattern(10, 10, false);
    let id = open(&mut ed, 10, 10, &px);
    exec(&mut ed, json!({"op": "layer.props", "id": id, "locks": {"position": true}}));
    assert!(ed.exec(json!({"op": "transform.layer", "ids": [id], "matrix": {"a":2,"b":0,"c":0,"d":2,"e":0,"f":0}}), &[]).is_err());
    assert!(ed.exec(json!({"op": "transform.nothing"}), &[]).is_err());
}

#[test]
fn smart_objects_move_their_quad_and_stay_smart() {
    let mut ed = editor();
    ed.exec(json!({"op": "doc.new", "width": 200, "height": 200}), &[]).unwrap();
    let px = pattern(40, 20, false);
    let id = ed.exec(json!({"op": "layer.place-smart", "width": 40, "height": 20, "x": 10, "y": 10}), &px).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":2,"b":0,"c":0,"d":2,"e":5,"f":0}}));
    let LayerKind::Smart { quad, .. } = &ed.doc.find(id).unwrap().kind else { panic!("still smart") };
    assert_eq!((quad[0].x, quad[0].y, quad[2].x, quad[2].y), (25.0, 20.0, 105.0, 60.0));
    // The editor rebuilt the pixels at the new size from the source.
    let b = ed.doc.find(id).unwrap().content_bounds().unwrap();
    assert!((b.w - 80).abs() <= 2 && (b.h - 40).abs() <= 2, "{b:?}");
    // Scaling back down and up again is lossless: the source is untouched.
    exec(&mut ed, json!({"op": "transform.layer", "ids": [id], "matrix": {"a":0.5,"b":0,"c":0,"d":0.5,"e":-2.5,"f":0}}));
    let got = editor_core::pixels::read_layer(&ed.doc, id, Rect::new(10, 10, 40, 20)).unwrap();
    assert_eq!(got, px);
}

/// Two layers at different positions: `(x, w)` of each, and its pixels.
fn two_layers(ed: &mut Editor) -> (LayerId, LayerId, Vec<u8>, Vec<u8>) {
    ed.exec(json!({"op": "doc.new", "width": 100, "height": 40}), &[]).unwrap();
    let pa: Vec<u8> = (0..6 * 4).flat_map(|i| [i as u8 * 10, 0, 0, 255]).collect();
    let pb: Vec<u8> = (0..8 * 4).flat_map(|i| [0, 0, i as u8 * 7, 255]).collect();
    let a = ed.exec(json!({"op": "layer.import", "width": 6, "height": 4, "x": 10, "y": 5}), &pa).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    let b = ed.exec(json!({"op": "layer.import", "width": 8, "height": 4, "x": 60, "y": 20}), &pb).unwrap()["data"]["id"].as_u64().unwrap() as LayerId;
    (a, b, pa, pb)
}

fn mirror_rows(px: &[u8], w: usize) -> Vec<u8> {
    px.chunks_exact(w * 4).flat_map(|row| row.chunks_exact(4).rev().flatten().copied().collect::<Vec<u8>>()).collect()
}

/// Flip of a multi-selection mirrors the layers as one box about the centre
/// of their combined bounds, so they swap sides; the active layer's own
/// centre (what the menu used) would have left the other layer far away.
#[test]
fn flip_several_layers_about_their_combined_centre() {
    let mut ed = editor();
    let (a, b, pa, pb) = two_layers(&mut ed);
    // Union: x 10..68, y 5..24 → centre x 39.
    let r = exec(&mut ed, json!({"op": "transform.flip", "ids": [a, b], "horizontal": true}));
    assert_eq!(r["label"], "Flip Horizontal");
    let ba = ed.doc.find(a).unwrap().content_bounds().unwrap();
    let bb = ed.doc.find(b).unwrap().content_bounds().unwrap();
    assert_eq!(ba, Rect::new(62, 5, 6, 4), "a mirrored to the right edge of the union");
    assert_eq!(bb, Rect::new(10, 20, 8, 4), "b mirrored to the left edge of the union");
    assert_eq!(layer_px(&ed, a, ba), mirror_rows(&pa, 6), "pixels mirrored exactly");
    assert_eq!(layer_px(&ed, b, bb), mirror_rows(&pb, 8));
    assert_eq!(ed.history.undo_labels().last(), Some(&"Flip Horizontal"));

    // Vertical: union y 5..24 → a goes to the bottom, b to the top.
    exec(&mut ed, json!({"op": "transform.flip", "ids": [a, b], "horizontal": false}));
    assert_eq!(ed.doc.find(a).unwrap().content_bounds().unwrap().y, 20);
    assert_eq!(ed.doc.find(b).unwrap().content_bounds().unwrap().y, 5);

    // No ids: the active layer flips in place.
    exec(&mut ed, json!({"op": "layer.set-active", "id": a}));
    let before = ed.doc.find(a).unwrap().content_bounds().unwrap();
    exec(&mut ed, json!({"op": "transform.flip", "horizontal": true}));
    assert_eq!(ed.doc.find(a).unwrap().content_bounds().unwrap(), before);
}

/// The same box behaviour through `transform.layer` with a matrix about the
/// union centre (what the Free Transform box sends).
#[test]
fn transform_layer_with_union_centre_swaps_positions() {
    let mut ed = editor();
    let (a, b, _, _) = two_layers(&mut ed);
    exec(&mut ed, json!({"op": "transform.layer", "ids": [a, b], "matrix": {"a": -1, "b": 0, "c": 0, "d": 1, "e": 78, "f": 0}}));
    assert_eq!(ed.doc.find(a).unwrap().content_bounds().unwrap().x, 62);
    assert_eq!(ed.doc.find(b).unwrap().content_bounds().unwrap().x, 10);
}
