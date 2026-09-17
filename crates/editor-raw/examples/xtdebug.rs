use editor_raw::{DevelopParams, Session};
fn main() {
    let bytes = std::fs::read("testdata/raw/fujifilm-x-s10.raf").unwrap();
    let mut s = Session::open(&bytes).unwrap();
    for (name, p) in [
        ("default", DevelopParams::default()),
        ("nonr", DevelopParams { noise_chroma: 0.0, ..Default::default() }),
        ("nosharp", DevelopParams { sharpen: 0.0, ..Default::default() }),
    ] {
        let o = s.develop(&p, false);
        let (x0, y0) = (o.width * 55 / 100, o.height * 45 / 100);
        let mut m = [0f64; 3];
        for y in y0..y0 + 60 { for x in x0..x0 + 60 { for c in 0..3 { m[c] += o.rgba[(y * o.width + x) * 4 + c] as f64 / 3600.0; } } }
        let o2 = s.develop(&p, true);
        let (x1, y1) = (o2.width * 55 / 100, o2.height * 45 / 100);
        let mut m2 = [0f64; 3];
        for y in y1..y1 + 20 { for x in x1..x1 + 20 { for c in 0..3 { m2[c] += o2.rgba[(y * o2.width + x) * 4 + c] as f64 / 400.0; } } }
        println!("{name}: full {m:.1?} half {m2:.1?}");
    }
}
