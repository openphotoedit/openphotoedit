//! Inspect a PSD: records, blocks, the imported tree, and PNGs of our render
//! and the file's merged image.
//! `cargo run -p editor-psd --example psd_debug -- FILE [OUTDIR]`

use editor_core::layer::{Layer, LayerKind};
use editor_psd::structure;

fn save_png(path: &str, rgba: &[u8], w: u32, h: u32) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn tree(layers: &[Layer], depth: usize) {
    for l in layers.iter().rev() {
        let extra = match &l.kind {
            LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Smart { raster: r, .. } => format!("{:?}", r.doc_rect()),
            LayerKind::Adjustment(a) => serde_json::to_string(a).unwrap_or_default().chars().take(160).collect(),
            LayerKind::Fill(f) => serde_json::to_string(f).unwrap_or_default().chars().take(160).collect(),
            _ => String::new(),
        };
        println!(
            "{}{} [{}] blend={:?} op={:.2} fill={:.2} vis={} clip={} mask={} fx={} {}",
            "  ".repeat(depth),
            l.name,
            l.kind.name(),
            l.blend,
            l.opacity,
            l.fill_opacity,
            l.visible,
            l.clip,
            l.mask.as_ref().map(|m| format!("{:?} fill={} en={}", m.raster.doc_rect(), m.raster.plane.fill()[0], m.enabled)).unwrap_or("-".into()),
            l.effects.is_some(),
            extra
        );
        if let Some(c) = l.children() {
            tree(c, depth + 1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let f = structure::parse(&bytes).unwrap();
    println!("{:?} merged_alpha={} resources={:?}", f.header, f.merged_alpha, f.resources.iter().map(|r| r.id).collect::<Vec<_>>());
    println!("global blocks: {:?}", f.global_blocks.iter().map(|b| (String::from_utf8_lossy(&b.key).into_owned(), b.data.len())).collect::<Vec<_>>());
    for (i, r) in f.layers.iter().enumerate() {
        println!(
            "#{i} {:?} {:?} blend={} op={} clip={} flags={:#x} ch={:?} mask={:?} blocks={:?}",
            r.name,
            r.rect,
            String::from_utf8_lossy(&r.blend),
            r.opacity,
            r.clipping,
            r.flags,
            r.channels.iter().map(|c| (c.id, c.raw.len())).collect::<Vec<_>>(),
            r.mask,
            r.blocks.iter().map(|b| format!("{}:{}", String::from_utf8_lossy(&b.key), b.data.len())).collect::<Vec<_>>()
        );
    }
    let imp = editor_psd::import(&bytes, &Default::default()).unwrap();
    println!("warnings: {:#?}", imp.warnings);
    tree(&imp.doc.layers, 0);
    if let Some(out) = args.get(2) {
        std::fs::create_dir_all(out).unwrap();
        let d = &imp.doc;
        save_png(&format!("{out}/ours.png"), &editor_core::render::flatten(d), d.width, d.height);
        if let Ok((w, h, m)) = editor_psd::merged_rgba(&bytes) {
            save_png(&format!("{out}/merged.png"), &m, w, h);
        }
    }
}
