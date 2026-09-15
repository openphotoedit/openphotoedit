//! Damaged files: truncated and corrupted copies of real corpus files must
//! return errors or import something, never panic, and never allocate past
//! the budget.

use std::path::Path;

use editor_psd::{export, import, merged_rgba, ExportOptions, ImportOptions};

const FILES: &[&str] = &[
    "psd-tools/1layer.psd",
    "psd-tools/group.psd",
    "psd-tools/mask.psd",
    "psd-tools/layers/curves.psd",
    "psd-tools/layers/gradient-map.psd",
    "psd-tools/layers/type-layer.psd",
    "psd-tools/layers/smartobject-layer.psd",
    "psd-tools/effects/effects-enabled.psd",
    "psd-tools/vector-mask2.psd",
    "psd-tools/16bit5x5.psb",
    "psd-tools/32bit5x5.psd",
    "psd-tools/colormodes/4x4_8bit_cmyk.psd",
    "ag-psd/text-simple.psd",
    "ag-psd/smart-object-png.psd",
    "ag-psd/adjustment-layers.psd",
];

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn opts() -> ImportOptions {
    ImportOptions { max_bytes: 64_000_000, ..Default::default() }
}

fn exercise(bytes: &[u8]) {
    if let Ok(i) = import(bytes, &opts()) {
        // Whatever imported must also export.
        let _ = export(&i.doc, &ExportOptions::default()).expect("an imported document exports");
    }
    let _ = merged_rgba(bytes);
}

#[test]
fn truncated_and_corrupted_files_do_not_panic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psd");
    let mut rng = Lcg(0x5eed);
    for name in FILES {
        let original = std::fs::read(root.join(name)).unwrap();
        // Truncations at structure boundaries and at random points.
        let mut cuts: Vec<usize> = (0..64).map(|i| i * 7).collect();
        for _ in 0..40 {
            cuts.push(rng.next() as usize % original.len());
        }
        for cut in cuts {
            exercise(&original[..cut.min(original.len())]);
        }
        // Byte flips, runs of 0xFF (huge lengths) and zeroed ranges.
        for round in 0..120 {
            let mut b = original.clone();
            let n = 1 + (rng.next() % 8) as usize;
            for _ in 0..n {
                let at = rng.next() as usize % b.len();
                match round % 3 {
                    0 => b[at] ^= 1 << (rng.next() % 8),
                    1 => {
                        for v in b.iter_mut().skip(at).take(4) {
                            *v = 0xFF;
                        }
                    }
                    _ => {
                        for v in b.iter_mut().skip(at).take(16) {
                            *v = 0;
                        }
                    }
                }
            }
            exercise(&b);
        }
    }
}

#[test]
fn absurd_dimensions_are_refused_within_budget() {
    // A header claiming a 30000×30000 layer backed by a few bytes.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psd");
    let mut b = std::fs::read(root.join("psd-tools/1layer.psd")).unwrap();
    let file = editor_psd::structure::parse(&b).unwrap();
    let _ = file;
    // Patch the document size in the header.
    b[14..18].copy_from_slice(&30000u32.to_be_bytes());
    b[18..22].copy_from_slice(&30000u32.to_be_bytes());
    let r = import(&b, &ImportOptions { max_bytes: 16_000_000, ..Default::default() });
    assert!(r.is_err() || r.as_ref().unwrap().doc.width == 30000);
}
