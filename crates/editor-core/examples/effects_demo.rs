//! Renders layer styles over a real photo for eyeballing:
//! `cargo run --release -p editor-core --example effects_demo -- out.png`.
use editor_core::render::flatten;
use editor_core::Editor;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "effects.png".into());
    let img = image::open("testdata/photos/portrait.jpg").unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    let mut ed = Editor::new(1, 1);
    ed.exec_json(&format!(r#"{{"op":"doc.open-pixels","width":{w},"height":{h}}}"#), img.as_raw()).unwrap();
    let styles = [
        (r#"{"drop_shadow":{"distance":14,"size":18,"opacity":0.6},"stroke":{"size":6,"color":{"r":255,"g":255,"b":255}}}"#, 0.06),
        (r#"{"bevel":{"size":22,"depth":180},"color_overlay":{"color":{"r":30,"g":120,"b":220}}}"#, 0.38),
        (r#"{"outer_glow":{"size":30,"spread":10,"opacity":0.9,"color":{"r":255,"g":220,"b":80}},"inner_shadow":{"distance":8,"size":12,"opacity":0.7}}"#, 0.70),
    ];
    for (fx, fy) in styles {
        let x0 = (w as f64 * 0.15) as i32;
        let y0 = (h as f64 * fy) as i32;
        let cmd = format!(r#"{{"op":"layer.add-shape","data":{{"kind":"rect","points":[{{"x":{x0},"y":{y0}}},{{"x":{},"y":{}}}],"fill":{{"r":240,"g":80,"b":60}},"stroke":null,"corner_radius":40}}}}"#, x0 + (w as f64 * 0.7) as i32, y0 + (h as f64 * 0.2) as i32);
        let r = ed.exec_json(&cmd, &[]).unwrap();
        let id = r["data"]["id"].as_u64().unwrap();
        ed.exec_json(&format!(r#"{{"op":"layer.set-effects","id":{id},"effects":{fx}}}"#), &[]).unwrap();
    }
    let t = std::time::Instant::now();
    let px = flatten(&ed.doc);
    eprintln!("flatten with effects: {:?}", t.elapsed());
    image::save_buffer(&out, &px, w, h, image::ColorType::Rgba8).unwrap();
}
