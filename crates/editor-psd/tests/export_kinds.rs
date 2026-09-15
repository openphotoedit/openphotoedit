//! Documents made in the editor (not imported) export every layer kind and
//! come back with the same structure and pixels; PSB is written when asked.

use editor_core::adjust::{Adjustment, Curves, CurvePoints, Develop, GradientMap};
use editor_core::blend::BlendMode;
use editor_core::color::Rgba8;
use editor_core::document::{Document, Guide};
use editor_core::effects::{LayerEffects, Shadow, StrokeEffect};
use editor_core::geom::{Point, Rect};
use editor_core::layer::{Fill, GradientKind, Layer, LayerKind, LayerMask, Raster, ShapeData, ShapeKind, SmartSource, TextData};
use editor_core::plane::Plane;
use editor_core::render::flatten;
use editor_psd::{export, import, merged_rgba, ExportOptions, ImportOptions};

fn pixels(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Plane {
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            raw.extend_from_slice(&f(x, y));
        }
    }
    Plane::from_raw(w, h, 4, &raw, [0; 4])
}

fn build() -> Document {
    let mut doc = Document::new(120, 80);
    doc.resolution = 300.0;
    doc.guides.push(Guide { vertical: true, position: 40.5 });
    let mut id = 0;
    let mut next = || {
        id += 1;
        id
    };
    doc.layers.push(Layer::new(next(), "Background", LayerKind::Pixel(Raster::new(pixels(120, 80, |x, y| [x as u8 * 2, y as u8 * 3, 90, 255]), 0, 0))));

    let mut masked = Layer::new(next(), "Masked", LayerKind::Pixel(Raster::new(pixels(40, 30, |_, _| [250, 20, 20, 255]), 10, 10)));
    let mut m = LayerMask::hide_all(120, 80);
    m.raster.plane.write(Rect::new(0, 0, 30, 80), &vec![255; 30 * 80]);
    masked.mask = Some(m);
    masked.blend = BlendMode::Multiply;
    masked.opacity = 0.75;
    masked.effects = Some(LayerEffects { drop_shadow: Some(Shadow::default()), stroke: Some(StrokeEffect::default()), ..Default::default() });
    doc.layers.push(masked);

    let child = Layer::new(next(), "Child", LayerKind::Pixel(Raster::new(pixels(20, 20, |_, _| [0, 200, 0, 200]), 70, 40)));
    let gid = next();
    let mut clipped = Layer::new(next(), "Clipped", LayerKind::Pixel(Raster::new(pixels(40, 40, |_, _| [0, 0, 255, 255]), 60, 30)));
    clipped.clip = true;
    doc.layers.push(Layer::new(gid, "Group", LayerKind::Group { children: vec![child, clipped], pass_through: true, expanded: false }));

    doc.layers.push(Layer::new(
        next(),
        "Curves",
        LayerKind::Adjustment(Adjustment::Curves(Curves { master: CurvePoints(vec![[0.0, 0.0], [128.0, 160.0], [255.0, 255.0]]), ..Default::default() })),
    ));
    doc.layers.push(Layer::new(next(), "Develop", LayerKind::Adjustment(Adjustment::Develop(Develop { exposure: 0.3, saturation: 20.0, ..Default::default() }))));
    doc.layers.push(Layer::new(next(), "Gradient map", LayerKind::Adjustment(Adjustment::GradientMap(GradientMap::default()))));
    let mut grad = Layer::new(
        next(),
        "Gradient",
        LayerKind::Fill(Fill::Gradient { stops: GradientMap::default().stops, gradient: GradientKind::Radial, from: Point::new(60.0, 40.0), to: Point::new(100.0, 40.0), reverse: false }),
    );
    grad.opacity = 0.3;
    grad.fill_opacity = 0.5;
    doc.layers.push(grad);
    let mut solid = Layer::new(next(), "Solid", LayerKind::Fill(Fill::Solid { color: Rgba8::rgb(255, 200, 0) }));
    solid.blend = BlendMode::Overlay;
    solid.visible = false;
    doc.layers.push(solid);

    let text = TextData { text: "Hello".into(), font_size: 20.0, x: 5.0, y: 50.0, ..Default::default() };
    doc.layers.push(Layer::new(next(), "Hello", LayerKind::Text { data: text, raster: Raster::new(pixels(50, 22, |x, _| [0, 0, 0, if x % 3 == 0 { 255 } else { 0 }]), 5, 50) }));
    let shape = ShapeData { kind: ShapeKind::Ellipse, points: vec![Point::new(80.0, 5.0), Point::new(110.0, 30.0)], fill: Some(Rgba8::rgb(10, 10, 10)), ..Default::default() };
    let raster = editor_core::shape::rasterize_shape(&shape);
    doc.layers.push(Layer::new(next(), "Ellipse", LayerKind::Shape { data: shape, raster }));

    let src = pixels(16, 16, |x, y| [x as u8 * 16, y as u8 * 16, 0, 255]);
    let quad = editor_core::smart::rect_quad(Rect::new(2, 60, 16, 16));
    let raster = Raster::new(src.clone(), 2, 60);
    doc.layers.push(Layer::new(next(), "Smart pixels", LayerKind::Smart { source: SmartSource::Pixels(src), quad, filters: vec![], raster, stale: false }));
    let mut inner = Document::new(10, 10);
    let iid = inner.alloc_id();
    inner.layers.push(Layer::new(iid, "Inner", LayerKind::Pixel(Raster::new(pixels(10, 10, |_, _| [200, 0, 200, 255]), 0, 0))));
    let quad = editor_core::smart::rect_quad(Rect::new(30, 60, 10, 10));
    let raster = Raster::new(pixels(10, 10, |_, _| [200, 0, 200, 255]), 30, 60);
    doc.layers.push(Layer::new(next(), "Smart document", LayerKind::Smart { source: SmartSource::Document(Box::new(inner)), quad, filters: vec![], raster, stale: false }));
    doc.reserve_ids();
    doc
}

fn names(layers: &[Layer], out: &mut Vec<String>) {
    for l in layers {
        out.push(format!("{}:{}:{:?}:{:.2}:{:.2}:{}:{}", l.name, l.kind.name(), l.blend, l.opacity, l.fill_opacity, l.visible, l.clip));
        if let Some(c) = l.children() {
            names(c, out);
        }
    }
}

fn mae(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

#[test]
fn every_kind_round_trips() {
    let doc = build();
    for psb in [false, true] {
        let e = export(&doc, &ExportOptions { psb: Some(psb), max_compat: true }).unwrap();
        assert!(e.warnings.iter().any(|w| w.contains("Develop")), "{:?}", e.warnings);
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/psd");
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(out.join(if psb { "export_kinds.psb" } else { "export_kinds.psd" }), &e.bytes);
        let f = editor_psd::structure::parse(&e.bytes).unwrap();
        assert_eq!(f.header.version, if psb { 2 } else { 1 });
        let back = import(&e.bytes, &ImportOptions::default()).unwrap();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        names(&doc.layers, &mut a);
        names(&back.doc.layers, &mut b);
        // Shapes come back as pixel layers and Develop as a colour lookup.
        let a: Vec<String> = a.into_iter().map(|s| s.replace(":shape:", ":pixel:")).collect();
        assert_eq!(a, b);
        assert_eq!(back.doc.resolution, 300.0);
        assert_eq!(back.doc.guides.len(), 1);
        let d = back.doc.layers.iter().find(|l| l.name == "Develop").unwrap();
        assert!(matches!(&d.kind, LayerKind::Adjustment(Adjustment::ColorLookup(_))), "{:?}", d.kind.name());
        let text = back.doc.layers.iter().find(|l| l.name == "Hello").unwrap();
        let LayerKind::Text { data, .. } = &text.kind else { panic!("text kind") };
        assert_eq!(data.text, "Hello");
        let smart = back.doc.layers.iter().find(|l| l.name == "Smart document").unwrap();
        assert!(matches!(&smart.kind, LayerKind::Smart { source: SmartSource::Document(_), .. }));
        let render = flatten(&doc);
        let (_, _, merged) = merged_rgba(&e.bytes).unwrap();
        assert!(mae(&render, &merged) < 1.0, "merged image is our render");
        // The Develop layer became a 33³ LUT: allow its sampling error.
        assert!(mae(&render, &flatten(&back.doc)) < 1.5, "{}", mae(&render, &flatten(&back.doc)));
    }
}

#[test]
fn large_documents_become_psb() {
    let mut doc = Document::new(30_001, 2);
    doc.layers.push(Layer::new(1, "wide", LayerKind::Pixel(Raster::new(pixels(3, 2, |_, _| [1, 2, 3, 255]), 29_990, 0))));
    let e = export(&doc, &ExportOptions::default()).unwrap();
    assert_eq!(editor_psd::structure::parse(&e.bytes).unwrap().header.version, 2);
    let back = import(&e.bytes, &ImportOptions::default()).unwrap();
    assert_eq!(back.doc.width, 30_001);
    assert_eq!(back.doc.layers[0].raster().unwrap().doc_rect(), Rect::new(29_990, 0, 3, 2));
}
