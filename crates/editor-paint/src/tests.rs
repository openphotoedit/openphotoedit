//! Command-level tests: every paint op through `Editor::exec`, with history.

use editor_core::geom::{Point, Rect};
use editor_core::layer::{Layer, LayerKind, Raster, SmartSource};
use editor_core::plane::Plane;
use editor_core::Editor;
use serde_json::{json, Value};

fn editor_with(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Editor {
    let mut ed = Editor::new(1, 1);
    crate::register(&mut ed);
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&f(x, y));
        }
    }
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), &px).unwrap();
    ed
}

fn grey(w: u32, h: u32, v: u8) -> Editor {
    editor_with(w, h, |_, _| [v, v, v, 255])
}

fn px(ed: &Editor, x: i32, y: i32) -> [u8; 4] {
    let id = ed.doc.active.unwrap();
    let l = ed.doc.find(id).unwrap();
    let r = l.raster().unwrap();
    r.plane.get(x - r.x, y - r.y)
}

fn mask_px(ed: &Editor, x: i32, y: i32) -> u8 {
    let id = ed.doc.active.unwrap();
    let m = ed.doc.find(id).unwrap().mask.as_ref().unwrap();
    m.raster.plane.get(x - m.raster.x, y - m.raster.y)[0]
}

fn layer_pixels(ed: &Editor) -> Vec<u8> {
    let id = ed.doc.active.unwrap();
    let r = ed.doc.find(id).unwrap().raster().unwrap().clone();
    r.plane.read_vec(Rect::new(0, 0, ed.doc.width as i32, ed.doc.height as i32).translate(-r.x, -r.y))
}

fn stroke(ed: &mut Editor, extra: Value, points: &[(f64, f64)], id: &str) -> Value {
    let pts: Vec<Value> = points.iter().map(|(x, y)| json!({"x": x, "y": y})).collect();
    let mut cmd = json!({"op": "paint.stroke", "points": pts, "stroke_id": id});
    for (k, v) in extra.as_object().unwrap() {
        cmd[k] = v.clone();
    }
    ed.exec(cmd, &[]).unwrap()
}

fn red_brush(size: f32) -> Value {
    json!({"size": size, "hardness": 1.0, "opacity": 1.0, "flow": 1.0, "color": {"r": 255, "g": 0, "b": 0}})
}

#[test]
fn unknown_op_falls_through() {
    let mut doc = editor_core::document::Document::new(2, 2);
    assert!(matches!(crate::apply(json!({"op": "paint.nope"}), &mut doc, &[]), Err(editor_core::ops::EditorError::UnknownOp(_))));
}

#[test]
fn multi_segment_stroke_is_one_undo_step_and_undo_restores_pixels() {
    let mut ed = editor_with(80, 40, |x, y| [(x * 3) as u8, (y * 5) as u8, 90, 255]);
    let before = layer_pixels(&ed);
    let n0 = ed.history.undo_labels().len();
    let brush = json!({"tool": "brush", "brush": red_brush(8.0)});
    let r = stroke(&mut ed, brush.clone(), &[(10.0, 20.0), (25.0, 20.0)], "s-undo");
    assert!(r["dirty"].is_object());
    stroke(&mut ed, brush.clone(), &[(25.0, 20.0), (45.0, 22.0)], "s-undo");
    stroke(&mut ed, brush, &[(45.0, 22.0), (70.0, 20.0)], "s-undo");
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "s-undo"}), &[]).unwrap();
    assert_eq!(ed.history.undo_labels().len(), n0 + 1);
    assert_eq!(px(&ed, 30, 20), [255, 0, 0, 255]);
    assert_eq!(px(&ed, 68, 20), [255, 0, 0, 255]);
    ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
    assert_eq!(layer_pixels(&ed), before, "undo restores the pre-stroke pixels exactly");
    ed.exec(json!({"op": "edit.redo"}), &[]).unwrap();
    assert_eq!(px(&ed, 30, 20), [255, 0, 0, 255]);
}

#[test]
fn opacity_caps_a_stroke_and_flow_builds_toward_it() {
    let mut ed = grey(60, 30, 255);
    let b = json!({"brush": {"size": 10.0, "hardness": 1.0, "opacity": 0.5, "flow": 1.0, "spacing": 0.05, "color": {"r": 0, "g": 0, "b": 0}}});
    // Scrub back and forth over the same spot in several segments.
    for i in 0..6 {
        let pts = if i % 2 == 0 { [(10.0, 15.0), (50.0, 15.0)] } else { [(50.0, 15.0), (10.0, 15.0)] };
        stroke(&mut ed, b.clone(), &pts, "s-cap");
    }
    let v = px(&ed, 30, 15)[0];
    assert!((v as i32 - 128).abs() <= 1, "one stroke never exceeds its opacity: {v}");
    ed.exec(json!({"op": "edit.seal"}), &[]).unwrap();
    // A second stroke composites over the first.
    stroke(&mut ed, b, &[(10.0, 15.0), (50.0, 15.0)], "s-cap-2");
    let v2 = px(&ed, 30, 15)[0];
    assert!((v2 as i32 - 64).abs() <= 2, "{v2}");

    // Low flow: one dab adds a little, many overlapping dabs add more, and
    // the stroke still stops at its opacity.
    let mut ed = grey(60, 30, 255);
    let low = json!({"brush": {"size": 10.0, "opacity": 0.8, "flow": 0.1, "spacing": 0.02, "color": {"r": 0, "g": 0, "b": 0}}});
    stroke(&mut ed, low.clone(), &[(30.0, 15.0)], "s-flow");
    let one = 255 - px(&ed, 30, 15)[0] as i32;
    stroke(&mut ed, low.clone(), &[(30.0, 15.0), (31.0, 15.0), (30.0, 15.0), (31.0, 15.0)], "s-flow");
    let many = 255 - px(&ed, 30, 15)[0] as i32;
    assert!(one > 10 && one < 40, "{one}");
    assert!(many > one, "{many} > {one}");
    for _ in 0..30 {
        stroke(&mut ed, low.clone(), &[(30.0, 15.0), (31.0, 15.0), (30.0, 15.0)], "s-flow");
    }
    let most = 255 - px(&ed, 30, 15)[0] as i32;
    assert!((most - 204).abs() <= 2, "converges to the opacity: {most}");
}

#[test]
fn selection_clips_painting() {
    let mut ed = grey(40, 20, 200);
    ed.exec(json!({"op": "select.rect", "x": 0, "y": 0, "width": 20, "height": 20}), &[]).unwrap();
    stroke(&mut ed, json!({"brush": red_brush(6.0)}), &[(5.0, 10.0), (35.0, 10.0)], "s-sel");
    assert_eq!(px(&ed, 10, 10), [255, 0, 0, 255]);
    assert_eq!(px(&ed, 30, 10), [200, 200, 200, 255]);
}

#[test]
fn locks_and_layer_kinds() {
    let mut ed = grey(30, 20, 100);
    let id = ed.doc.active.unwrap();
    ed.exec(json!({"op": "layer.props", "id": id, "locks": {"pixels": true}}), &[]).unwrap();
    let cmd = json!({"op": "paint.stroke", "brush": red_brush(4.0), "points": [{"x": 5, "y": 5}], "stroke_id": "s-lock"});
    assert!(ed.exec(cmd.clone(), &[]).is_err());
    ed.exec(json!({"op": "layer.props", "id": id, "locks": {"pixels": false, "transparency": true}}), &[]).unwrap();
    // Half-transparent pixels keep their alpha; the colour changes.
    ed.doc.find_mut(id).unwrap().raster_mut().unwrap().plane.write(Rect::new(0, 0, 30, 20), &[100u8, 100, 100, 128].repeat(600));
    ed.exec(cmd, &[]).unwrap();
    let p = px(&ed, 5, 5);
    assert_eq!(p[3], 128);
    assert!(p[0] > 200 && p[1] < 50, "{p:?}");
    // The eraser cannot remove pixels there either.
    stroke(&mut ed, json!({"tool": "eraser", "brush": {"size": 4.0}}), &[(20.0, 10.0)], "s-lock-e");
    assert_eq!(px(&ed, 20, 10)[3], 128);

    // Shape and smart object layers must be rasterized first; adjustment
    // layers have nothing to paint.
    ed.exec(json!({"op": "layer.add-shape", "data": {"kind": "rect", "points": [{"x": 2, "y": 2}, {"x": 10, "y": 10}]}}), &[]).unwrap();
    let e = ed.exec(json!({"op": "paint.stroke", "points": [{"x": 5, "y": 5}]}), &[]).unwrap_err();
    assert!(e.to_string().contains("rasterize"), "{e}");
    let sid = ed.doc.alloc_id();
    let smart = LayerKind::Smart {
        source: SmartSource::Pixels(Plane::transparent(4, 4)),
        quad: [Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 4.0), Point::new(0.0, 4.0)],
        filters: vec![],
        raster: Raster::empty(4, 4),
        stale: false,
    };
    ed.doc.layers.push(Layer::new(sid, "smart", smart));
    ed.doc.active = Some(sid);
    let e = ed.exec(json!({"op": "paint.stroke", "points": [{"x": 1, "y": 1}]}), &[]).unwrap_err();
    assert!(e.to_string().contains("rasterize"), "{e}");
    let e = ed.exec(json!({"op": "paint.fill", "x": 1, "y": 1, "color": {"r": 1, "g": 2, "b": 3}}), &[]).unwrap_err();
    assert!(e.to_string().contains("rasterize"), "{e}");
    ed.exec(json!({"op": "layer.add-adjustment", "adjustment": {"kind": "invert"}}), &[]).unwrap();
    assert!(ed.exec(json!({"op": "paint.stroke", "points": [{"x": 1, "y": 1}]}), &[]).is_err());
}

#[test]
fn eraser_brush_and_gradient_on_a_mask() {
    let mut ed = grey(40, 20, 100);
    stroke(&mut ed, json!({"tool": "eraser", "brush": {"size": 6.0}}), &[(10.0, 10.0)], "s-erase");
    assert_eq!(px(&ed, 10, 10)[3], 0);
    assert_eq!(px(&ed, 30, 10)[3], 255);

    let id = ed.doc.active.unwrap();
    ed.exec(json!({"op": "layer.add-mask", "id": id, "from": "reveal-all"}), &[]).unwrap();
    assert!(ed.exec(json!({"op": "paint.stroke", "target": "mask", "tool": "eraser", "brush": {"size": 6.0}, "points": [{"x": 30, "y": 10}], "stroke_id": "s-mask"}), &[]).is_ok());
    assert_eq!(mask_px(&ed, 30, 10), 0, "erasing a mask hides");
    assert_eq!(px(&ed, 30, 10)[3], 255, "pixels untouched");
    stroke(&mut ed, json!({"target": "mask", "brush": {"size": 6.0, "color": {"r": 255, "g": 255, "b": 255}}}), &[(30.0, 10.0)], "s-mask-2");
    assert_eq!(mask_px(&ed, 30, 10), 255, "white reveals");
    stroke(&mut ed, json!({"target": "mask", "brush": {"size": 6.0, "color": {"r": 0, "g": 0, "b": 0}, "opacity": 0.5}}), &[(5.0, 3.0)], "s-mask-3");
    assert!((mask_px(&ed, 5, 3) as i32 - 128).abs() <= 1);

    ed.exec(json!({"op": "paint.gradient", "target": "mask", "from": {"x": 0, "y": 0}, "to": {"x": 40, "y": 0}, "gradient": "linear", "dither": false,
        "stops": [{"pos": 0.0, "color": {"r": 0, "g": 0, "b": 0}}, {"pos": 1.0, "color": {"r": 255, "g": 255, "b": 255}}]}), &[]).unwrap();
    assert!(mask_px(&ed, 1, 10) < 12);
    assert!(mask_px(&ed, 38, 10) > 243);
    assert!((mask_px(&ed, 20, 10) as i32 - 130).abs() < 6);
}

#[test]
fn pencil_is_aliased_brush_is_not() {
    let mut ed = grey(40, 20, 255);
    stroke(&mut ed, json!({"tool": "pencil", "brush": {"size": 3.3, "color": {"r": 0, "g": 0, "b": 0}}}), &[(5.2, 5.7), (30.6, 12.1)], "s-pencil");
    for y in 0..20 {
        for x in 0..40 {
            let v = px(&ed, x, y)[0];
            assert!(v == 0 || v == 255, "({x},{y}) {v}");
        }
    }
    let mut ed = grey(40, 20, 255);
    stroke(&mut ed, json!({"brush": {"size": 3.3, "color": {"r": 0, "g": 0, "b": 0}}}), &[(5.2, 5.7), (30.6, 12.1)], "s-brush-aa");
    let partial = (0..20).flat_map(|y| (0..40).map(move |x| (x, y))).filter(|&(x, y)| {
        let v = px(&ed, x, y)[0];
        v > 0 && v < 255
    });
    assert!(partial.count() > 10);
}

#[test]
fn pressure_changes_width() {
    let mut ed = grey(80, 40, 255);
    let b = json!({"brush": {"size": 20.0, "pressure_size": true, "color": {"r": 0, "g": 0, "b": 0}}});
    let pts: Vec<Value> = vec![json!({"x": 10, "y": 20, "p": 0.2}), json!({"x": 70, "y": 20, "p": 1.0})];
    ed.exec(json!({"op": "paint.stroke", "points": pts, "stroke_id": "s-p", "brush": b["brush"]}), &[]).unwrap();
    let width_at = |ed: &Editor, x: i32| (0..40).filter(|&y| px(ed, x, y)[0] < 128).count();
    assert!(width_at(&ed, 15) < width_at(&ed, 65), "{} {}", width_at(&ed, 15), width_at(&ed, 65));
}

#[test]
fn dodge_burn_sponge() {
    let mut ed = editor_with(60, 20, |_, _| [150, 110, 90, 255]);
    stroke(&mut ed, json!({"tool": "dodge", "range": "midtones", "exposure": 1.0, "brush": {"size": 8.0}}), &[(10.0, 10.0)], "s-dodge");
    assert!(px(&ed, 10, 10)[0] > 170);
    stroke(&mut ed, json!({"tool": "burn", "range": "midtones", "exposure": 0.5, "brush": {"size": 8.0}}), &[(30.0, 10.0)], "s-burn");
    let b = px(&ed, 30, 10);
    assert!(b[0] < 140 && b[0] > 90, "{b:?}");
    stroke(&mut ed, json!({"tool": "sponge", "sponge": "desaturate", "brush": {"size": 8.0}}), &[(50.0, 10.0)], "s-sponge");
    let s = px(&ed, 50, 10);
    assert!(s[0] as i32 - s[2] as i32 == 0, "{s:?}");
}

#[test]
fn blur_softens_and_sharpen_adds_contrast() {
    let checker = |x: u32, y: u32| if (x + y).is_multiple_of(2) { [220, 220, 220, 255] } else { [40, 40, 40, 255] };
    let mut ed = editor_with(40, 40, checker);
    let var = |ed: &Editor| {
        let mut s = 0.0;
        for y in 15..25 {
            for x in 15..25 {
                s += (px(ed, x, y)[0] as f64 - 130.0).powi(2);
            }
        }
        s
    };
    let v0 = var(&ed);
    stroke(&mut ed, json!({"tool": "blur", "strength": 1.0, "brush": {"size": 20.0}}), &[(10.0, 20.0), (30.0, 20.0)], "s-blur");
    assert!(var(&ed) < v0 * 0.3, "{} vs {v0}", var(&ed));

    let mut ed = editor_with(40, 20, |x, _| if x < 20 { [100, 100, 100, 255] } else { [150, 150, 150, 255] });
    stroke(&mut ed, json!({"tool": "sharpen", "strength": 1.0, "brush": {"size": 12.0}}), &[(20.0, 4.0), (20.0, 16.0)], "s-sharp");
    assert!(px(&ed, 19, 10)[0] < 100 && px(&ed, 20, 10)[0] > 150, "{:?} {:?}", px(&ed, 19, 10), px(&ed, 20, 10));
}

#[test]
fn smudge_drags_colour_along_the_stroke() {
    let mut ed = editor_with(60, 20, |x, _| if x < 20 { [220, 20, 20, 255] } else { [20, 20, 220, 255] });
    let pts: Vec<(f64, f64)> = (0..=20).map(|i| (12.0 + i as f64 * 1.5, 10.0)).collect();
    stroke(&mut ed, json!({"tool": "smudge", "strength": 0.8, "brush": {"size": 8.0, "spacing": 0.1}}), &pts[..10], "s-smudge");
    stroke(&mut ed, json!({"tool": "smudge", "strength": 0.8, "brush": {"size": 8.0, "spacing": 0.1}}), &pts[9..], "s-smudge");
    let p = px(&ed, 28, 10);
    assert!(p[0] > 90, "red carried into blue: {p:?}");
    assert_eq!(px(&ed, 50, 2), [20, 20, 220, 255], "untouched away from the stroke");
}

#[test]
fn clone_copies_from_the_offset_layer_or_merged() {
    let mut ed = editor_with(60, 30, |x, y| if x < 20 { [((x * 13) % 255) as u8, ((y * 17) % 255) as u8, 7, 255] } else { [128, 128, 128, 255] });
    let src = json!({"dx": -30, "dy": 0});
    stroke(&mut ed, json!({"tool": "clone", "source": src, "brush": {"size": 10.0, "hardness": 1.0}}), &[(40.0, 15.0), (40.0, 16.0)], "s-clone");
    assert_eq!(px(&ed, 40, 15), px(&ed, 10, 15));
    assert_eq!(px(&ed, 38, 14), px(&ed, 8, 14));
    assert!(ed.exec(json!({"op": "paint.stroke", "tool": "clone", "points": [{"x": 1, "y": 1}]}), &[]).is_err(), "needs a source");

    // Sample all layers: clone onto an empty layer above.
    ed.exec(json!({"op": "layer.add-pixel"}), &[]).unwrap();
    stroke(&mut ed, json!({"tool": "clone", "source": {"dx": -30, "dy": 0, "sample_all": true}, "brush": {"size": 6.0}}), &[(45.0, 10.0)], "s-clone-all");
    assert_eq!(px(&ed, 45, 10), {
        let b = ed.doc.layers[0].raster().unwrap();
        b.plane.get(15, 10)
    });
    // Sampling only the (empty) current layer clones nothing.
    stroke(&mut ed, json!({"tool": "clone", "source": {"dx": -30, "dy": 0}, "brush": {"size": 6.0}}), &[(45.0, 25.0)], "s-clone-cur");
    assert_eq!(px(&ed, 45, 25)[3], 0);
}

#[test]
fn heal_keeps_source_texture_and_takes_destination_tone() {
    // Left: textured dark grey. Right: smooth light grey with a darker blemish.
    let noise = |x: u32, y: u32| ((x * 7919 + y * 104729) % 23) as i32 - 11;
    let mut ed = editor_with(120, 60, |x, y| {
        if x < 50 {
            let v = (90 + noise(x, y)) as u8;
            [v, v, v, 255]
        } else {
            let d = ((x as f32 - 90.0).powi(2) + (y as f32 - 30.0).powi(2)).sqrt();
            let v = if d < 5.0 { 120 } else { 200 };
            [v, v, v, 255]
        }
    });
    let b = json!({"tool": "heal", "source": {"dx": -65, "dy": 0}, "brush": {"size": 24.0, "hardness": 0.5}});
    stroke(&mut ed, b.clone(), &[(84.0, 30.0), (90.0, 30.0)], "s-heal");
    stroke(&mut ed, b, &[(90.0, 30.0), (96.0, 30.0)], "s-heal");
    let n0 = ed.history.undo_labels().len();
    let r = ed.exec(json!({"op": "paint.stroke-end", "stroke_id": "s-heal"}), &[]).unwrap();
    assert_eq!(ed.history.undo_labels().len(), n0, "finishing merges into the stroke's step");
    assert!(r["dirty"].is_object());
    let mut sum = 0.0;
    let mut sq = 0.0;
    let mut n = 0.0;
    for y in 27..34 {
        for x in 87..94 {
            let v = px(&ed, x, y)[0] as f64;
            sum += v;
            sq += v * v;
            n += 1.0;
        }
    }
    let mean = sum / n;
    let sd = (sq / n - mean * mean).sqrt();
    assert!((mean - 200.0).abs() < 8.0, "takes the surrounding tone: {mean}");
    assert!(sd > 3.0, "keeps the source texture: {sd}");
    // Far from the stroke nothing changed.
    assert_eq!(px(&ed, 118, 2)[0], 200);
}

#[test]
fn undo_mid_stroke_starts_a_fresh_stroke_state() {
    let mut ed = grey(40, 20, 255);
    let b = json!({"brush": {"size": 6.0, "opacity": 0.5, "color": {"r": 0, "g": 0, "b": 0}}});
    stroke(&mut ed, b.clone(), &[(10.0, 10.0)], "s-reuse");
    ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
    assert_eq!(px(&ed, 10, 10)[0], 255);
    stroke(&mut ed, b, &[(30.0, 10.0)], "s-reuse");
    assert_eq!(px(&ed, 10, 10)[0], 255, "stale stroke state not replayed");
    assert!((px(&ed, 30, 10)[0] as i32 - 128).abs() <= 1);
}

#[test]
fn paint_bucket_and_magic_eraser() {
    // Two white regions split by a black line, with a near-white pixel.
    let mut ed = editor_with(30, 10, |x, y| {
        if x == 15 {
            [0, 0, 0, 255]
        } else if (x, y) == (3, 3) {
            [245, 245, 245, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    ed.exec(json!({"op": "paint.fill", "x": 2, "y": 2, "tolerance": 5, "anti_alias": false, "color": {"r": 0, "g": 200, "b": 0}}), &[]).unwrap();
    assert_eq!(px(&ed, 5, 5), [0, 200, 0, 255]);
    assert_eq!(px(&ed, 3, 3), [245, 245, 245, 255], "outside tolerance");
    assert_eq!(px(&ed, 20, 5), [255, 255, 255, 255], "not contiguous");
    ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
    ed.exec(json!({"op": "paint.fill", "x": 2, "y": 2, "tolerance": 10, "contiguous": false, "anti_alias": false, "opacity": 0.5, "color": {"r": 0, "g": 0, "b": 0}}), &[]).unwrap();
    assert!((px(&ed, 20, 5)[0] as i32 - 128).abs() <= 1);
    assert!((px(&ed, 3, 3)[0] as i32 - 123).abs() <= 1);
    ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
    ed.exec(json!({"op": "select.rect", "x": 0, "y": 0, "width": 10, "height": 10}), &[]).unwrap();
    ed.exec(json!({"op": "paint.magic-erase", "x": 20, "y": 5, "tolerance": 10, "contiguous": false, "anti_alias": false}), &[]).unwrap();
    assert_eq!(px(&ed, 5, 5)[3], 0, "erased inside the selection");
    assert_eq!(px(&ed, 20, 5)[3], 255, "outside the selection kept");
    assert_eq!(px(&ed, 15, 5)[3], 255);
}

#[test]
fn gradients_linear_radial_and_reverse() {
    let stops = json!([{"pos": 0.0, "color": {"r": 0, "g": 0, "b": 0}}, {"pos": 1.0, "color": {"r": 255, "g": 255, "b": 255}}]);
    let mut ed = grey(100, 10, 50);
    ed.exec(json!({"op": "paint.gradient", "from": {"x": 0, "y": 5}, "to": {"x": 100, "y": 5}, "stops": stops, "dither": false}), &[]).unwrap();
    assert!(px(&ed, 0, 5)[0] <= 2 && px(&ed, 99, 5)[0] >= 253);
    assert!((px(&ed, 50, 5)[0] as i32 - 129).abs() <= 2);
    let mut ed = grey(100, 100, 50);
    ed.exec(json!({"op": "paint.gradient", "from": {"x": 50, "y": 50}, "to": {"x": 90, "y": 50}, "gradient": "radial", "reverse": true, "stops": stops, "opacity": 1.0}), &[]).unwrap();
    assert!(px(&ed, 50, 50)[0] > 245);
    assert!(px(&ed, 0, 0)[0] < 3);
    assert!((px(&ed, 70, 50)[0] as i32 - px(&ed, 50, 70)[0] as i32).abs() <= 2, "radially symmetric");
    for kind in ["angle", "reflected", "diamond"] {
        ed.exec(json!({"op": "paint.gradient", "from": {"x": 50, "y": 50}, "to": {"x": 90, "y": 50}, "gradient": kind, "stops": stops}), &[]).unwrap();
    }
    assert!(ed.exec(json!({"op": "paint.gradient", "from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 0}, "stops": []}), &[]).is_err());
}

// ---- Smoothing, blur brush and hard edge (workstream H3) ----

/// A stroke sent as `segments` calls sharing one id, each call starting at
/// the previous call's last point.
fn segmented(ed: &mut Editor, extra: Value, pts: &[(f64, f64)], segments: usize, id: &str) {
    let n = pts.len();
    let mut start = 0;
    for s in 0..segments {
        let end = ((s + 1) * (n - 1)) / segments;
        if end < start {
            continue;
        }
        let from = if s == 0 { 0 } else { start + 1 };
        if from > end {
            continue;
        }
        stroke(ed, extra.clone(), &pts[from..=end], id);
        start = end;
    }
    ed.exec(json!({"op": "paint.stroke-end", "stroke_id": id}), &[]).unwrap();
}

fn landscape(x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/photos/landscape.jpg");
    let img = image::open(path).expect("decode landscape").to_rgba8();
    image::imageops::crop_imm(&img, x0, y0, w, h).to_image().into_raw()
}

fn editor_from(px: &[u8], w: u32, h: u32) -> Editor {
    let mut ed = Editor::new(1, 1);
    crate::register(&mut ed);
    ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), px).unwrap();
    ed
}

/// Gradient energy (sum of |horizontal differences|, RGB) over a rectangle.
fn texture(px: &[u8], w: u32, r: (u32, u32, u32, u32)) -> f64 {
    let mut s = 0.0;
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            let i = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                s += (px[i + c] as f64 - px[i + 4 + c] as f64).abs();
            }
        }
    }
    s
}

#[test]
fn blur_brush_ignores_segmentation_and_scales_with_size() {
    let (w, h) = (600u32, 200u32);
    let truth = landscape(700, 1400, w, h);
    let path: Vec<(f64, f64)> = (0..=48).map(|i| (60.0 + 10.0 * i as f64, 100.0)).collect();
    let brush = json!({"tool": "blur", "strength": 1.0, "brush": {"size": 60.0, "hardness": 0.0, "opacity": 1.0, "flow": 1.0, "spacing": 0.1}});
    let run = |segments: usize| {
        let mut ed = editor_from(&truth, w, h);
        segmented(&mut ed, brush.clone(), &path, segments, "b");
        layer_pixels(&ed)
    };
    let one = run(1);
    let many = run(48);
    let diff = one.iter().zip(&many).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / (w * h * 3) as f64;
    let core = (150, 85, 300, 30);
    let left = texture(&one, w, core) / texture(&truth, w, core);
    eprintln!("blur brush: 1 vs 48 segments mean |diff| {diff:.4}; texture left in the core {left:.3}");
    assert_eq!(one, many, "the same path in 1 or 48 segments paints the same pixels");
    assert!(left <= 0.25, "a 60-px blur at full strength leaves {left:.3} of the texture");
    // Beyond the brush (and the blur's reach) nothing changes.
    for y in [0u32, 20, 180, 199] {
        for x in (0..w).step_by(7) {
            let i = ((y * w + x) * 4) as usize;
            assert_eq!(&one[i..i + 4], &truth[i..i + 4], "({x},{y})");
        }
    }
    // A bigger brush blurs more: σ follows the size.
    let small = {
        let mut ed = editor_from(&truth, w, h);
        let b = json!({"tool": "blur", "strength": 1.0, "brush": {"size": 16.0, "hardness": 0.0, "spacing": 0.1}});
        segmented(&mut ed, b, &path, 1, "s");
        layer_pixels(&ed)
    };
    let core_small = (150, 97, 300, 6);
    let left_small = texture(&small, w, core_small) / texture(&truth, w, core_small);
    let left_big = texture(&one, w, core_small) / texture(&truth, w, core_small);
    eprintln!("texture left on the centre line: 16 px {left_small:.3}, 60 px {left_big:.3}");
    assert!(left_big < left_small, "{left_big} vs {left_small}");
    // Half strength sits between.
    let half = {
        let mut ed = editor_from(&truth, w, h);
        let b = json!({"tool": "blur", "strength": 0.5, "brush": {"size": 60.0, "hardness": 0.0, "spacing": 0.1}});
        segmented(&mut ed, b, &path, 7, "h");
        layer_pixels(&ed)
    };
    let left_half = texture(&half, w, core) / texture(&truth, w, core);
    assert!(left_half > left + 0.1 && left_half < 0.9, "{left_half}");
}

#[test]
fn coarse_samples_follow_a_smooth_curve() {
    use crate::brush::{Brush, PathState, StrokePoint};
    let (cx, cy, r) = (300.0f64, 300.0f64, 100.0f64);
    let on = |deg: f64| StrokePoint { x: cx + r * deg.to_radians().cos(), y: cy - r * deg.to_radians().sin(), p: 1.0 };
    let b = Brush { size: 4.0, spacing: 0.1, ..Default::default() };
    let dev = |dabs: &[crate::brush::Dab]| dabs.iter().map(|d| ((d.x - cx).hypot(d.y - cy) - r).abs()).fold(0.0, f64::max);
    // Four samples 60° apart: the chords sag 13.4 px inside the circle.
    let samples: Vec<StrokePoint> = [0.0, 60.0, 120.0, 180.0].iter().map(|&a| on(a)).collect();
    let mut st = PathState::default();
    let (mut dabs, tail) = b.walk_smooth(&mut st, &samples);
    dabs.extend(tail);
    let mut lin = PathState::default();
    let chords = b.walk(&mut lin, &samples);
    // The middle piece has real neighbours on both sides; the end pieces
    // have mirrored ones.
    let middle: Vec<_> = dabs.iter().copied().filter(|d| d.y < cy - r * 0.5f64.sqrt() * 1.0 && (d.x - cx).abs() <= r * 0.5 + 1e-9).collect();
    eprintln!("curve deviation: middle {:.2} px, whole {:.2} px; straight chords {:.2} px", dev(&middle), dev(&dabs), dev(&chords));
    // Catmull–Rom itself sags 2.57 px on a circle sampled every 60°.
    assert!(dev(&chords) > 13.0);
    assert!(dev(&middle) < 2.8, "middle piece deviates {:.2} px", dev(&middle));
    assert!(dev(&dabs) < 2.8, "whole curve deviates {:.2} px", dev(&dabs));
    // Every 30°: chords sag 3.4 px, the curve 0.17 px.
    let samples: Vec<StrokePoint> = (0..7).map(|i| on(i as f64 * 30.0)).collect();
    let mut st = PathState::default();
    let (mut dabs, tail) = b.walk_smooth(&mut st, &samples);
    dabs.extend(tail);
    eprintln!("curve deviation at 30° samples: {:.2} px; straight chords {:.2} px", dev(&dabs), dev(&b.walk(&mut PathState::default(), &samples)));
    assert!(dev(&dabs) < 0.4, "{:.2}", dev(&dabs));
    // Their test (BrushTests.swift): seven samples 30° apart through a 4-px
    // brush paint the arc between them.
    let mut ed = grey(600, 600, 255);
    let pts: Vec<(f64, f64)> = (0..7).map(|i| on(i as f64 * 30.0)).map(|p| (p.x, p.y)).collect();
    stroke(&mut ed, json!({"tool": "brush", "brush": red_brush(4.0)}), &pts, "arc");
    for deg in [45.0, 75.0, 105.0, 135.0] {
        let p = on(deg);
        assert_eq!(px(&ed, p.x as i32, p.y as i32)[1], 0, "arc at {deg}°");
    }
}

#[test]
fn smoothed_strokes_do_not_depend_on_segmentation() {
    // A wavy path sampled sparsely, sent whole, point by point, and in threes.
    let pts: Vec<(f64, f64)> = (0..16).map(|i| (20.0 + i as f64 * 22.0, 100.0 + 50.0 * (i as f64 * 0.9).sin())).collect();
    for brush in [red_brush(18.0), json!({"size": 30.0, "hardness": 0.2, "opacity": 0.7, "flow": 0.4, "color": {"r": 0, "g": 0, "b": 255}})] {
        let run = |segments: usize| {
            let mut ed = grey(400, 200, 255);
            segmented(&mut ed, json!({"tool": "brush", "brush": brush}), &pts, segments, "w");
            layer_pixels(&ed)
        };
        let whole = run(1);
        for segments in [5, 15] {
            let split = run(segments);
            let worst = whole.iter().zip(&split).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(worst <= 1, "{segments} segments differ by up to {worst}");
        }
    }
}

#[test]
fn hard_stroke_edge_has_no_scallop() {
    // A 120-px hard stroke: the outermost antialiased row is flat.
    let (w, h) = (1200u32, 300u32);
    let mut ed = editor_with(w, h, |_, _| [0, 0, 0, 0]);
    let b = json!({"size": 120.0, "hardness": 1.0, "opacity": 1.0, "flow": 1.0, "spacing": 0.1, "color": {"r": 0, "g": 0, "b": 0}});
    stroke(&mut ed, json!({"tool": "brush", "brush": b}), &[(120.0, 150.3), (1080.0, 150.3)], "hard");
    let px = layer_pixels(&ed);
    let mut worst = 0;
    for y in 0..h {
        let row: Vec<u8> = (360..840).map(|x| px[((y * w + x) * 4 + 3) as usize]).collect();
        let (mn, mx) = (*row.iter().min().unwrap(), *row.iter().max().unwrap());
        if mx > 0 && mn < 255 {
            eprintln!("row {y}: alpha {mn}..{mx}");
        }
        worst = worst.max(mx - mn);
    }
    assert!(worst <= 1, "edge ripple {worst}/255");
}
