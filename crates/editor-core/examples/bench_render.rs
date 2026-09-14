//! Rough compositor timings: `cargo run --release -p editor-core --example bench_render`.
use std::time::Instant;

use editor_core::render::{flatten, Renderer, View};
use editor_core::Editor;

fn main() {
    let (w, h) = (6000u32, 4000u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            px[i] = (x % 256) as u8;
            px[i + 1] = (y % 256) as u8;
            px[i + 2] = ((x ^ y) % 256) as u8;
            px[i + 3] = 255;
        }
    }
    let mut ed = Editor::new(1, 1);
    let t = Instant::now();
    ed.exec_json(&format!(r#"{{"op":"doc.open-pixels","width":{w},"height":{h}}}"#), &px).unwrap();
    println!("open 24MP: {:?}", t.elapsed());
    ed.exec_json(r#"{"op":"layer.add-adjustment","adjustment":{"kind":"curves","master":[[0,0],[128,160],[255,255]]}}"#, &[]).unwrap();
    ed.exec_json(r#"{"op":"layer.add-adjustment","adjustment":{"kind":"hue-saturation","master":{"hue":10,"saturation":20,"lightness":0}}}"#, &[]).unwrap();
    let dev = ed.exec_json(r#"{"op":"layer.add-adjustment","adjustment":{"kind":"develop","exposure":0.3,"contrast":20,"clarity":30}}"#, &[]).unwrap();
    let dev_id = dev["data"]["id"].as_u64().unwrap();

    let mut r = Renderer::new();
    let (vw, vh) = (2400usize, 1600usize);
    for (label, scale, x, y) in [("fit 0.4", 0.4, 0.0, 0.0), ("1:1", 1.0, 1000.0, 1000.0), ("2x", 2.0, 2000.0, 2000.0)] {
        let view = View { x, y, scale, width: vw, height: vh };
        let mut out = vec![0u8; vw * vh * 4];
        let t = Instant::now();
        r.render(&ed.doc, view, &mut out);
        println!("render {label} {vw}x{vh} cold: {:?}", t.elapsed());
        // Drag the top develop layer: should reuse the checkpoint below it.
        for k in 0..3 {
            ed.exec_json(&format!(r#"{{"op":"layer.set-adjustment","id":{dev_id},"adjustment":{{"kind":"develop","exposure":{}}}}}"#, 0.1 * k as f32), &[]).unwrap();
            let t = Instant::now();
            r.render(&ed.doc, view, &mut out);
            println!("  slider drag {k}: {:?}", t.elapsed());
        }
    }
    let t = Instant::now();
    let flat = flatten(&ed.doc);
    println!("flatten 24MP: {:?} ({} bytes)", t.elapsed(), flat.len());
    let t = Instant::now();
    let _ = ed.exec_json(r#"{"op":"edit.undo"}"#, &[]).unwrap();
    println!("undo: {:?}", t.elapsed());
}
