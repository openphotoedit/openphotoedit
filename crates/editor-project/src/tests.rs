use super::*;
use editor_core::adjust::{Curves, CurvePoints};
use editor_core::color::Rgba8;
use editor_core::effects::Shadow;
use editor_core::geom::Rect;
use editor_core::layer::{GradientKind, ShapeKind};

fn pixels(w: u32, h: u32, seed: u8) -> Plane {
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            raw.extend_from_slice(&[(x as u8).wrapping_mul(seed), (y as u8).wrapping_add(seed), seed, if (x + y) % 5 == 0 { 0 } else { 255 }]);
        }
    }
    Plane::from_raw(w, h, 4, &raw, [0; 4])
}

fn sample() -> Document {
    let mut doc = Document::new(600, 400);
    doc.resolution = 240.0;
    doc.guides.push(Guide { vertical: false, position: 12.25 });
    doc.meta.source_name = Some("sample.psd".into());
    doc.meta.actions.push(("created".into(), "test".into()));
    doc.meta.icc = Some(vec![1, 2, 3, 4]);
    doc.meta.extra.insert("psd".into(), Sidecar(Arc::new(vec![9; 100])));
    let mut sel = Plane::mask(600, 400, 0);
    sel.write(Rect::new(10, 10, 300, 20), &vec![200; 6000]);
    doc.selection = Some(sel);

    let mut bg = Layer::new(1, "Background", LayerKind::Pixel(Raster::new(pixels(600, 400, 3), 0, 0)));
    bg.locks.pixels = true;
    bg.color_label = 4;
    doc.layers.push(bg);
    let mut px = Layer::new(2, "Offset", LayerKind::Pixel(Raster::new(pixels(300, 300, 7), -40, 520)));
    let mut mask = LayerMask::hide_all(600, 400);
    mask.raster.plane.write(Rect::new(0, 0, 100, 100), &vec![128; 10_000]);
    mask.density = 0.5;
    mask.enabled = false;
    px.mask = Some(mask);
    px.effects = Some(LayerEffects { drop_shadow: Some(Shadow::default()), ..Default::default() });
    px.provenance.push("ai:test".into());
    px.blend = BlendMode::Screen;
    px.opacity = 0.4;
    px.fill_opacity = 0.8;
    px.clip = true;
    let adj = Layer::new(3, "Curves", LayerKind::Adjustment(Adjustment::Curves(Curves { red: CurvePoints(vec![[0.0, 10.0], [255.0, 240.0]]), ..Default::default() })));
    let fill = Layer::new(4, "Grad", LayerKind::Fill(Fill::Gradient { stops: vec![], gradient: GradientKind::Diamond, from: Point::new(0.1 + 0.2, 1.0 / 3.0), to: Point::new(9.0e-7, 8.123456789012345), reverse: true }));
    doc.layers.push(Layer::new(5, "Group", LayerKind::Group { children: vec![px, adj, fill], pass_through: false, expanded: false }));
    let text = TextData { text: "Hi ✓".into(), box_width: Some(120.0), ..Default::default() };
    doc.layers.push(Layer::new(6, "Text", LayerKind::Text { data: text, raster: Raster::new(pixels(40, 20, 11), 5, 5) }));
    let shape = ShapeData { kind: ShapeKind::Arrow, points: vec![Point::new(0.0, 0.0), Point::new(50.0, 50.0)], dash: Some([3.0, 2.0]), ..Default::default() };
    doc.layers.push(Layer::new(7, "Arrow", LayerKind::Shape { data: shape, raster: Raster::new(pixels(51, 51, 13), 0, 0) }));
    let src = pixels(64, 64, 17);
    doc.layers.push(Layer::new(
        8,
        "Smart",
        LayerKind::Smart {
            source: SmartSource::Pixels(src.clone()),
            quad: editor_core::smart::rect_quad(Rect::new(1, 2, 64, 64)),
            filters: vec![SmartFilter { filter: serde_json::json!({"op": "filter.gaussian-blur", "radius": 2}), enabled: true, opacity: 0.5, blend: BlendMode::Multiply }],
            raster: Raster::new(src, 1, 2),
            stale: true,
        },
    ));
    let mut inner = sample_inner();
    inner.meta.extra.insert("psd".into(), Sidecar(Arc::new(vec![7; 3])));
    doc.layers.push(Layer::new(
        9,
        "Smart doc",
        LayerKind::Smart { source: SmartSource::Document(Box::new(inner)), quad: [Point::new(0.0, 0.0); 4], filters: vec![], raster: Raster::new(pixels(8, 8, 19), 3, 3), stale: false },
    ));
    doc.reserve_ids();
    doc.active = Some(2);
    doc
}

fn sample_inner() -> Document {
    let mut d = Document::new(8, 8);
    d.layers.push(Layer::new(1, "inner", LayerKind::Fill(Fill::Solid { color: Rgba8::rgb(1, 2, 3) })));
    d.reserve_ids();
    d
}

fn same_plane(a: &Plane, b: &Plane) {
    assert_eq!((a.width(), a.height(), a.channels(), a.fill()), (b.width(), b.height(), b.channels(), b.fill()));
    let cols = a.width().div_ceil(TILE).max(1);
    let rows = a.height().div_ceil(TILE).max(1);
    for ty in 0..rows {
        for tx in 0..cols {
            match (a.tile(tx, ty), b.tile(tx, ty)) {
                (None, None) => {}
                (Some(x), Some(y)) => assert_eq!(x.data(), y.data(), "tile {tx},{ty}"),
                _ => panic!("tile presence differs at {tx},{ty}"),
            }
        }
    }
}

fn same_raster(a: &Raster, b: &Raster) {
    assert_eq!((a.x, a.y), (b.x, b.y));
    same_plane(&a.plane, &b.plane);
}

fn same_layer(a: &Layer, b: &Layer) {
    assert_eq!(a.id, b.id);
    assert_eq!(a.name, b.name);
    assert_eq!((a.visible, a.opacity, a.fill_opacity, a.blend, a.clip, a.locks, a.color_label), (b.visible, b.opacity, b.fill_opacity, b.blend, b.clip, b.locks, b.color_label));
    assert_eq!(a.provenance, b.provenance);
    assert_eq!(a.effects, b.effects);
    match (&a.mask, &b.mask) {
        (None, None) => {}
        (Some(x), Some(y)) => {
            same_raster(&x.raster, &y.raster);
            assert_eq!((x.enabled, x.linked, x.density), (y.enabled, y.linked, y.density));
        }
        _ => panic!("mask presence differs"),
    }
    match (&a.kind, &b.kind) {
        (LayerKind::Pixel(x), LayerKind::Pixel(y)) => same_raster(x, y),
        (LayerKind::Adjustment(x), LayerKind::Adjustment(y)) => assert_eq!(x, y),
        (LayerKind::Fill(x), LayerKind::Fill(y)) => assert_eq!(x, y),
        (LayerKind::Group { children: x, pass_through: p, expanded: e }, LayerKind::Group { children: y, pass_through: q, expanded: f }) => {
            assert_eq!((p, e), (q, f));
            assert_eq!(x.len(), y.len());
            x.iter().zip(y).for_each(|(i, j)| same_layer(i, j));
        }
        (LayerKind::Text { data: x, raster: r }, LayerKind::Text { data: y, raster: s }) => {
            assert_eq!(x, y);
            same_raster(r, s);
        }
        (LayerKind::Shape { data: x, raster: r }, LayerKind::Shape { data: y, raster: s }) => {
            assert_eq!(x, y);
            same_raster(r, s);
        }
        (LayerKind::Smart { source: s1, quad: q1, filters: f1, raster: r1, stale: t1 }, LayerKind::Smart { source: s2, quad: q2, filters: f2, raster: r2, stale: t2 }) => {
            assert_eq!((q1, f1, t1), (q2, f2, t2));
            same_raster(r1, r2);
            match (s1, s2) {
                (SmartSource::Pixels(x), SmartSource::Pixels(y)) => same_plane(x, y),
                (SmartSource::Document(x), SmartSource::Document(y)) => same_doc(x, y),
                _ => panic!("smart source differs"),
            }
        }
        _ => panic!("kind differs: {} vs {}", a.kind.name(), b.kind.name()),
    }
}

fn same_doc(a: &Document, b: &Document) {
    assert_eq!((a.width, a.height, a.resolution, a.active), (b.width, b.height, b.resolution, b.active));
    assert_eq!(a.guides.len(), b.guides.len());
    for (g, h) in a.guides.iter().zip(&b.guides) {
        assert_eq!((g.vertical, g.position), (h.vertical, h.position));
    }
    assert_eq!(a.meta.source_name, b.meta.source_name);
    assert_eq!(a.meta.actions, b.meta.actions);
    assert_eq!(a.meta.icc, b.meta.icc);
    assert_eq!(a.meta.extra, b.meta.extra);
    match (&a.selection, &b.selection) {
        (None, None) => {}
        (Some(x), Some(y)) => same_plane(x, y),
        _ => panic!("selection presence differs"),
    }
    assert_eq!(a.layers.len(), b.layers.len());
    a.layers.iter().zip(&b.layers).for_each(|(x, y)| same_layer(x, y));
}

#[test]
fn round_trip_is_exact() {
    let doc = sample();
    let bytes = save(&doc).unwrap();
    let back = load(&bytes, &LoadOptions::default()).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    same_doc(&doc, &back.doc);
    // New layers do not reuse ids.
    let mut d = back.doc;
    assert!(d.alloc_id() > 9);
    // Saving the reloaded document loses nothing either.
    let again = load(&save(&d).unwrap(), &LoadOptions::default()).unwrap();
    same_doc(&doc, &again.doc);
}

#[test]
fn corrupt_input_errors_without_panicking() {
    let bytes = save(&sample()).unwrap();
    assert_eq!(load(b"", &LoadOptions::default()).unwrap_err(), Error::NotProject);
    assert!(load(b"PK\x03\x04 not really", &LoadOptions::default()).is_err());
    for cut in (0..bytes.len()).step_by(bytes.len() / 97 + 1) {
        assert!(load(&bytes[..cut], &LoadOptions::default()).is_err(), "cut {cut}");
    }
    let mut state = 12345u64;
    for _ in 0..300 {
        let mut b = bytes.clone();
        for _ in 0..4 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let at = (state >> 33) as usize % b.len();
            b[at] ^= 1 << ((state >> 20) % 8);
        }
        let _ = load(&b, &LoadOptions { max_bytes: 200_000_000 });
    }
}

#[test]
fn budget_is_enforced() {
    let bytes = save(&sample()).unwrap();
    assert_eq!(load(&bytes, &LoadOptions { max_bytes: 1000 }).err(), Some(Error::TooLarge));
}

fn rewrite_manifest(bytes: &[u8], f: impl Fn(&mut Value)) -> Vec<u8> {
    let z = ZipReader::open(bytes).unwrap();
    let mut out = ZipWriter::new();
    let names: Vec<String> = z.names().map(String::from).collect();
    for n in names {
        let mut data = z.read(&n, u64::MAX).unwrap().unwrap();
        if n == "manifest.json" {
            let mut v: Value = serde_json::from_slice(&data).unwrap();
            f(&mut v);
            data = serde_json::to_vec(&v).unwrap();
        }
        out.add(&n, &data, n.ends_with(".json")).unwrap();
    }
    out.finish().unwrap()
}

#[test]
fn newer_files_open_when_they_allow_it() {
    let bytes = save(&sample()).unwrap();
    // A later version that added fields and a new layer kind, readable by v1.
    let newer = rewrite_manifest(&bytes, |v| {
        v["version"] = 3.into();
        v["min_reader"] = 1.into();
        v["document"]["future_field"] = serde_json::json!({"a": 1});
        let layers = v["document"]["layers"].as_array_mut().unwrap();
        layers.push(serde_json::json!({"id": 99, "name": "Hologram", "kind": {"type": "hologram", "depth": 3}, "sparkle": true}));
    });
    let loaded = load(&newer, &LoadOptions::default()).unwrap();
    assert_eq!(loaded.doc.layers.last().unwrap().name, "Hologram");
    assert_eq!(loaded.warnings.len(), 1);
    // One that says v1 cannot read it.
    let too_new = rewrite_manifest(&bytes, |v| {
        v["version"] = 2.into();
        v["min_reader"] = 2.into();
    });
    assert_eq!(load(&too_new, &LoadOptions::default()).err(), Some(Error::TooNew(2)));
}

// ---------------------------------------------------------------------------
// Value limits (validate.rs). Each case saves an ordinary project with the
// crate's own writer, tampers one value in the manifest, and must be refused
// with an error that names the problem.

fn refused(f: impl Fn(&mut Value)) -> String {
    let bytes = rewrite_manifest(&save(&sample()).unwrap(), f);
    match load(&bytes, &LoadOptions::default()) {
        Err(Error::Invalid(msg)) => msg,
        Err(e) => panic!("refused with the wrong error: {e:?}"),
        Ok(_) => panic!("accepted"),
    }
}

/// The Curves adjustment inside the sample's group.
fn adj(v: &mut Value) -> &mut Value {
    &mut layer_at(v, 1)["kind"]["children"][1]["kind"]["adjustment"]
}

/// Top-level layer `i` of the sample (0 bg, 1 group, 2 text, 3 shape, 4 smart, 5 smart doc).
fn layer_at(v: &mut Value, i: usize) -> &mut Value {
    &mut v["document"]["layers"][i]
}

#[test]
fn ordinary_and_legacy_projects_still_load() {
    let bytes = save(&sample()).unwrap();
    let legacy = rewrite_manifest(&bytes, |v| v["format"] = LEGACY_FORMAT.into());
    let back = load(&legacy, &LoadOptions::default()).unwrap();
    same_doc(&sample(), &back.doc);
}

#[test]
fn far_away_rasters_are_refused() {
    // The probe case: accepted before, the next edit looped to x = 2^31.
    let msg = refused(|v| layer_at(v, 0)["kind"]["raster"]["x"] = 2_147_483_646i64.into());
    assert!(msg.contains("placed at x = 2147483646"), "{msg}");
    let msg = refused(|v| layer_at(v, 1)["kind"]["children"][0]["mask"]["y"] = (-5_000_000).into());
    assert!(msg.contains("mask") && msg.contains("y = -5000000"), "{msg}");
    let msg = refused(|v| layer_at(v, 2)["kind"]["raster"]["y"] = 1.5e6.into());
    assert!(msg.contains("Text"), "{msg}");
}

#[test]
fn huge_smart_object_quads_are_refused() {
    // The probe case: corners at 1e7 implied a 10⁷ × 10⁷ warp on the next edit.
    let msg = refused(|v| layer_at(v, 4)["kind"]["quad"] = serde_json::json!([{"x": 0, "y": 0}, {"x": 1e7, "y": 0}, {"x": 1e7, "y": 1e7}, {"x": 0, "y": 1e7}]));
    assert!(msg.contains("corner"), "{msg}");
    // Within the offset limit but a 20,000² box: 400 MP.
    let msg = refused(|v| layer_at(v, 4)["kind"]["quad"] = serde_json::json!([{"x": 0, "y": 0}, {"x": 20000, "y": 0}, {"x": 20000, "y": 20000}, {"x": 0, "y": 20000}]));
    assert!(msg.contains("20000×20000"), "{msg}");
    // A number too large for f32 becomes infinity if it gets past the check.
    let msg = refused(|v| layer_at(v, 4)["kind"]["quad"][2]["x"] = 1e39.into());
    assert!(msg.contains("corner"), "{msg}");
}

#[test]
fn too_many_layers_are_refused() {
    let msg = refused(|v| {
        let layers = v["document"]["layers"].as_array_mut().unwrap();
        for i in 0..validate::MAX_LAYERS {
            layers.push(serde_json::json!({"id": 1000 + i, "name": "x", "kind": {"type": "adjustment", "adjustment": {"kind": "invert"}}}));
        }
    });
    assert!(msg.contains("more than 10000 layers"), "{msg}");
}

#[test]
fn deep_group_nesting_is_refused() {
    let nest = |depth: usize| {
        move |v: &mut Value| {
            let mut l = serde_json::json!({"id": 5000, "name": "leaf", "kind": {"type": "adjustment", "adjustment": {"kind": "invert"}}});
            for d in 0..depth - 1 {
                l = serde_json::json!({"id": 6000 + d, "name": "g", "kind": {"type": "group", "children": [l], "pass_through": true, "expanded": false}});
            }
            v["document"]["layers"].as_array_mut().unwrap().push(l);
        }
    };
    let ok = rewrite_manifest(&save(&sample()).unwrap(), nest(validate::MAX_GROUP_DEPTH));
    assert!(load(&ok, &LoadOptions::default()).is_ok(), "{} levels are allowed", validate::MAX_GROUP_DEPTH);
    let msg = refused(nest(validate::MAX_GROUP_DEPTH + 1));
    assert!(msg.contains("nested more than 32"), "{msg}");
}

#[test]
fn long_names_are_refused() {
    let msg = refused(|v| layer_at(v, 0)["name"] = "n".repeat(validate::MAX_NAME_BYTES + 1).into());
    assert!(msg.contains("name is 16385 bytes"), "{msg}");
}

#[test]
fn out_of_range_parameters_are_refused() {
    // Adjustments: an f32-overflowing value, a range the engine relies on,
    // too many curve points, a lookup table that does not match its size.
    let msg = refused(|v| adj(v)["red"][0][1] = 1e39.into());
    assert!(msg.contains("curves adjustment") && msg.contains("out-of-range"), "{msg}");
    let msg = refused(|v| adj(v)["red"] = serde_json::json!((0..300).map(|i| [i % 256, i % 256]).collect::<Vec<_>>()));
    assert!(msg.contains("300 entries"), "{msg}");
    let msg = refused(|v| *adj(v) = serde_json::json!({"kind": "levels", "master": {"in_black": 0, "in_white": 255, "gamma": 50, "out_black": 0, "out_white": 255}}));
    assert!(msg.contains("gamma"), "{msg}");
    let msg = refused(|v| *adj(v) = serde_json::json!({"kind": "posterize", "levels": 0}));
    assert!(msg.contains("levels"), "{msg}");
    let msg = refused(|v| *adj(v) = serde_json::json!({"kind": "color-lookup", "name": "x", "size": 3, "table": vec![0.0; 9]}));
    assert!(msg.contains("expected 81"), "{msg}");
    // Layer style, text and shape sizes.
    let msg = refused(|v| layer_at(v, 1)["kind"]["children"][0]["effects"]["drop_shadow"]["size"] = 1e6.into());
    assert!(msg.contains("layer style") && msg.contains("size"), "{msg}");
    let msg = refused(|v| layer_at(v, 2)["kind"]["data"]["font_size"] = 1e5.into());
    assert!(msg.contains("font_size"), "{msg}");
    let msg = refused(|v| layer_at(v, 3)["kind"]["data"]["points"] = serde_json::json!(vec![serde_json::json!({"x": 0, "y": 0}); 100_001]));
    assert!(msg.contains("100001 entries"), "{msg}");
    let msg = refused(|v| layer_at(v, 0)["opacity"] = 7.into());
    assert!(msg.contains("opacity"), "{msg}");
}
