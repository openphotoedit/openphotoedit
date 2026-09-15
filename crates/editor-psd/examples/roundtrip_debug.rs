//! Import, export, re-import; print per-layer differences.
use editor_core::layer::Layer;
use editor_core::render::{render_layer_alone, View};

fn walk<'a>(l: &'a [Layer], out: &mut Vec<&'a Layer>) {
    for x in l {
        out.push(x);
        if let Some(c) = x.children() {
            walk(c, out);
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let d1 = editor_psd::import(&bytes, &Default::default()).unwrap().doc;
    let e = editor_psd::export(&d1, &Default::default()).unwrap();
    let d2 = editor_psd::import(&e.bytes, &Default::default()).unwrap().doc;
    let (mut l1, mut l2) = (vec![], vec![]);
    walk(&d1.layers, &mut l1);
    walk(&d2.layers, &mut l2);
    let view = View::full(&d1);
    for (x, y) in l1.iter().zip(l2.iter()) {
        let fa = render_layer_alone(x, &d1, view);
        let fb = render_layer_alone(y, &d2, view);
        let diff: f32 = fa.iter().zip(fb.iter()).map(|(p, q)| (p - q).abs()).sum::<f32>() / fa.len().max(1) as f32 * 255.0;
        let same_mask = match (&x.mask, &y.mask) {
            (Some(m), Some(n)) => format!("{:?}/{} vs {:?}/{} dens {} {}", m.raster.doc_rect(), m.raster.plane.fill()[0], n.raster.doc_rect(), n.raster.plane.fill()[0], m.density, n.density),
            (None, None) => "-".into(),
            _ => "MISMATCH".into(),
        };
        println!("{:30} {} {} diff={diff:.3} fx={}/{} fill={}/{} mask {}", x.name, x.kind.name(), y.kind.name(), x.effects.is_some(), y.effects.is_some(), x.fill_opacity, y.fill_opacity, same_mask);
    }
    for k in 1..=d1.layers.len().min(d2.layers.len()) {
        let mut a = d1.clone();
        a.layers.truncate(k);
        let mut b = d2.clone();
        b.layers.truncate(k);
        let fa = editor_core::render::flatten(&a);
        let fb = editor_core::render::flatten(&b);
        let diff: f64 = fa.iter().zip(fb.iter()).map(|(p, q)| (*p as f64 - *q as f64).abs()).sum::<f64>() / fa.len() as f64;
        println!("prefix {k} ({}): {diff:.3}", d1.layers[k - 1].name);
    }
}
