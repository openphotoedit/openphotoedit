//! Hue/Saturation against Photoshop's own output.
//!
//! The psd-tools corpus has four files (`adjustments/huesaturation_*.psd`)
//! with sixteen Hue/Saturation layers, most with custom range bands, each
//! masked to a 50 px strip over the same photo. Photoshop's merged image is
//! the reference.
//!
//! Mean absolute error per file (0..255, RGB over white), before → after the
//! band work: rgb 25.25 → 1.25, saturation 17.95 → 0.40, lightness
//! 57.85 → 0.33, colorize 11.85 → 0.47.

use std::path::{Path, PathBuf};

use editor_core::adjust::{Adjustment, HUE_BANDS_DEFAULT};
use editor_core::layer::LayerKind;
use editor_core::render::flatten;
use editor_psd::{export, import, merged_rgba, structure, ExportOptions, ImportOptions};

fn file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psd/psd-tools/adjustments").join(format!("{name}.psd"))
}

fn over_white(p: &[u8]) -> [f64; 3] {
    let a = p[3] as f64 / 255.0;
    [0, 1, 2].map(|c| p[c] as f64 * a + 255.0 * (1.0 - a))
}

/// Mean error of each 50 px strip, above the colour swatches at the bottom.
fn strip_errors(ours: &[u8], theirs: &[u8], w: usize) -> [f64; 4] {
    let mut acc = [0f64; 4];
    for y in 0..170 {
        for x in 0..200.min(w) {
            let i = (y * w + x) * 4;
            let (a, b) = (over_white(&ours[i..i + 4]), over_white(&theirs[i..i + 4]));
            acc[x / 50] += (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f64>() / 3.0;
        }
    }
    acc.map(|v| v / (170.0 * 50.0))
}

#[test]
fn hue_saturation_matches_photoshop() {
    for name in ["huesaturation_rgb", "huesaturation_saturation_rgb", "huesaturation_lightness_rgb", "huesaturation_colorize_rgb"] {
        let path = file(name);
        if !path.exists() {
            eprintln!("skipping: {} is missing", path.display());
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let doc = import(&bytes, &ImportOptions::default()).unwrap().doc;
        let (w, _, merged) = merged_rgba(&bytes).unwrap();
        let strips = strip_errors(&flatten(&doc), &merged, w as usize);
        let mean = strips.iter().sum::<f64>() / 4.0;
        eprintln!("{name}: strips {strips:.2?} mean {mean:.2}");
        assert!(mean < 1.5, "{name}: mean error {mean:.2}");
        assert!(strips.iter().all(|&e| e < 2.5), "{name}: strip errors {strips:.2?}");
    }
}

#[test]
fn custom_bands_are_read() {
    let path = file("huesaturation_lightness_rgb");
    if !path.exists() {
        return;
    }
    let doc = import(&std::fs::read(&path).unwrap(), &ImportOptions::default()).unwrap().doc;
    let mut bands = Vec::new();
    doc.walk(&mut |l| {
        if let LayerKind::Adjustment(Adjustment::HueSaturation(h)) = &l.kind {
            bands.push((l.name.clone(), h.bands));
        }
    });
    let layer1 = bands.iter().find(|(n, _)| n == "Hue/Saturation 1").expect("layer");
    // psd-tools reads these for the same layer.
    assert_eq!(layer1.1[0], [303.0, 4.0, 82.0, 175.0]);
    assert_eq!(layer1.1[1], [12.0, 45.0, 75.0, 105.0]);
    assert_eq!(layer1.1[5], [243.0, 310.0, 39.0, 104.0]);
    assert!(bands.iter().any(|(_, b)| *b != HUE_BANDS_DEFAULT));
}

#[test]
fn hue_saturation_blocks_round_trip_byte_for_byte() {
    for name in ["huesaturation_rgb", "huesaturation_saturation_rgb", "huesaturation_lightness_rgb", "huesaturation_colorize_rgb"] {
        let path = file(name);
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let doc = import(&bytes, &ImportOptions::default()).unwrap().doc;
        let out = export(&doc, &ExportOptions::default()).unwrap().bytes;
        let blocks = |b: &[u8]| -> Vec<(String, Vec<u8>)> {
            let f = structure::parse(b).unwrap();
            f.layers.iter().filter_map(|l| l.block(b"hue2").map(|d| (l.name.clone(), d.to_vec()))).collect()
        };
        let (before, after) = (blocks(&bytes), blocks(&out));
        assert_eq!(before.len(), 4, "{name}");
        for (n, d) in &before {
            let back = after.iter().find(|(m, _)| m == n).unwrap_or_else(|| panic!("{name}: {n} lost"));
            assert_eq!(&back.1, d, "{name}: {n} hue2 bytes differ");
        }
    }
}

#[test]
fn grain_exports_as_noise_and_comes_back_as_grain() {
    use editor_core::adjust::Grain;
    use editor_core::blend::BlendMode;
    use editor_core::layer::{Layer, Raster};
    use editor_core::plane::Plane;
    let mut doc = editor_core::document::Document::new(64, 48);
    let grey = vec![128u8; 64 * 48 * 4].chunks(4).flat_map(|_| [128, 128, 128, 255]).collect::<Vec<u8>>();
    doc.layers.push(Layer::new(1, "Background", LayerKind::Pixel(Raster::new(Plane::from_raw(64, 48, 4, &grey, [0; 4]), 0, 0))));
    let g = Grain { amount: 50.0, size: 2.0, roughness: 30.0, seed: 3_000_000_000 };
    let mut layer = Layer::new(2, "Grain 1", LayerKind::Adjustment(Adjustment::Grain(g.clone())));
    layer.blend = BlendMode::Screen;
    doc.layers.push(layer);
    let e = export(&doc, &ExportOptions::default()).unwrap();
    assert!(e.warnings.iter().any(|w| w.contains("Grain")));
    // Photoshop sees a Linear Light pixel layer.
    let f = structure::parse(&e.bytes).unwrap();
    let rec = f.layers.iter().find(|l| l.name == "Grain 1").unwrap();
    assert_eq!(&rec.blend, b"lLit");
    assert_eq!(rec.rect.width(), 64);
    // We see the adjustment again, with its own blend mode.
    let back = import(&e.bytes, &ImportOptions::default()).unwrap().doc;
    let l = back.find(2).unwrap();
    assert_eq!(l.blend, BlendMode::Screen);
    assert!(matches!(&l.kind, LayerKind::Adjustment(Adjustment::Grain(x)) if *x == g));
}

#[test]
fn grain_noise_layer_reproduces_grain_on_mid_grey() {
    use editor_core::adjust::Grain;
    use editor_core::layer::{Layer, Raster};
    use editor_core::plane::Plane;
    // Over mid-grey, Photoshop's Linear Light of our noise layer equals the
    // Grain adjustment (its midtone weight is 1 there).
    let (w, h) = (48usize, 32usize);
    let grey: Vec<u8> = (0..w * h).flat_map(|_| [128u8, 128, 128, 255]).collect();
    let g = Grain { amount: 40.0, size: 1.5, roughness: 50.0, seed: 11 };
    let noise = editor_psd::kinds::grain_layer_pixels(&g, w, h);
    let mut doc = editor_core::document::Document::new(w as u32, h as u32);
    doc.layers.push(Layer::new(1, "bg", LayerKind::Pixel(Raster::new(Plane::from_raw(w as u32, h as u32, 4, &grey, [0; 4]), 0, 0))));
    doc.layers.push(Layer::new(2, "g", LayerKind::Adjustment(Adjustment::Grain(g))));
    let live = flatten(&doc);
    let mut max = 0i32;
    for i in 0..w * h {
        let ll = (128.0 + 2.0 * noise[i * 4] as f32 - 255.0).round().clamp(0.0, 255.0) as i32;
        max = max.max((ll - live[i * 4] as i32).abs());
    }
    assert!(max <= 2, "max difference {max}");
}
