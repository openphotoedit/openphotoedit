//! Print decode diagnostics for RAW files: levels, white balance, matrix,
//! embedded JPEGs. `cargo run -p editor-raw --example rawinfo -- file...`
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let src = RawSource::new_from_slice(&bytes);
        let dec = rawler::get_decoder(&src).unwrap();
        let raw = dec.raw_image(&src, &RawDecodeParams::default(), false).unwrap();
        let max = match &raw.data {
            rawler::RawImageData::Integer(d) => d.iter().copied().max().unwrap_or(0) as f32,
            rawler::RawImageData::Float(d) => d.iter().copied().fold(0.0, f32::max),
        };
        println!("{path}\n  {}x{} cpp {} bps {} black {:?} white {:?} max {max}", raw.width, raw.height, raw.cpp, raw.bps, raw.blacklevel, raw.whitelevel);
        println!("  wb {:?} crop {:?} active {:?}", raw.wb_coeffs, raw.crop_area, raw.active_area);
        println!("  matrices {:?}", raw.color_matrix);
        println!("  photometric {:?}", match &raw.photometric { rawler::rawimage::RawPhotometricInterpretation::Cfa(c) => c.cfa.to_string(), o => format!("{o:?}") });
        let info = editor_raw::probe(&bytes).unwrap();
        println!("  info {}", serde_json::to_string(&info).unwrap());
        let mut n = 0;
        for i in 0..bytes.len().saturating_sub(3) {
            if bytes[i] == 0xFF && bytes[i + 1] == 0xD8 && bytes[i + 2] == 0xFF {
                n += 1;
                if n < 8 {
                    println!("  SOI at {i}: next {:02x?}", &bytes[i + 2..i + 24]);
                }
            }
        }
        println!("  jpegs {:?}", editor_raw::preview::find_jpegs(&bytes));
    }
}
