//! Hue/Saturation range bands survive the project format, and projects
//! written before bands existed load with Photoshop's default bands.

use editor_core::adjust::{Adjustment, HueSaturation, HUE_BANDS_DEFAULT};
use editor_core::document::Document;
use editor_core::layer::{Layer, LayerKind};

fn hue_sat(doc: &Document) -> HueSaturation {
    let mut out = None;
    doc.walk(&mut |l| {
        if let LayerKind::Adjustment(Adjustment::HueSaturation(h)) = &l.kind {
            out = Some(h.clone());
        }
    });
    out.expect("a hue/saturation layer")
}

#[test]
fn custom_bands_round_trip() {
    let mut hs = HueSaturation::default();
    hs.bands[0] = [300.0, 330.0, 20.0, 50.0];
    hs.bands[4] = [199.0, 359.0, 29.0, 170.0];
    hs.ranges[0].saturation = -40.0;
    let mut doc = Document::new(16, 16);
    doc.layers.push(Layer::new(1, "Hue/Saturation 1", LayerKind::Adjustment(Adjustment::HueSaturation(hs.clone()))));
    let bytes = editor_project::save(&doc).unwrap();
    let back = editor_project::load(&bytes, &Default::default()).unwrap().doc;
    assert_eq!(hue_sat(&back), hs);
}

#[test]
fn adjustment_json_without_bands_gets_defaults() {
    // The shape `layer.set-adjustment` and older projects use.
    let old = serde_json::json!({
        "kind": "hue-saturation",
        "master": { "hue": 0, "saturation": 20, "lightness": 0 },
        "ranges": [{}, {}, {}, {}, {}, {}],
        "colorize": false
    });
    let Adjustment::HueSaturation(h) = serde_json::from_value(old).unwrap() else { panic!() };
    assert_eq!(h.bands, HUE_BANDS_DEFAULT);
    assert_eq!(h.master.saturation, 20.0);
}

fn save_with(adj: Adjustment) -> Vec<u8> {
    let mut doc = Document::new(16, 16);
    doc.layers.push(Layer::new(1, "Adj", LayerKind::Adjustment(adj)));
    editor_project::save(&doc).unwrap()
}

#[test]
fn grain_round_trips_with_a_large_seed() {
    let g = editor_core::adjust::Grain { amount: 40.0, size: 3.0, roughness: 20.0, seed: 4_000_000_000 };
    let back = editor_project::load(&save_with(Adjustment::Grain(g.clone())), &Default::default()).unwrap().doc;
    let mut found = None;
    back.walk(&mut |l| {
        if let LayerKind::Adjustment(Adjustment::Grain(x)) = &l.kind {
            found = Some(x.clone());
        }
    });
    assert_eq!(found, Some(g));
}

#[test]
fn out_of_range_bands_and_grain_are_refused() {
    let mut hs = HueSaturation::default();
    hs.bands[2][1] = 5000.0;
    assert!(editor_project::load(&save_with(Adjustment::HueSaturation(hs)), &Default::default()).is_err());
    let g = editor_core::adjust::Grain { amount: 500.0, ..Default::default() };
    assert!(editor_project::load(&save_with(Adjustment::Grain(g)), &Default::default()).is_err());
}
