//! Decodes every file in testdata/raw (run crates/editor-raw/scripts/fetch-testdata.sh
//! first) and writes, per file, into target/raw/out/:
//!   <name>.jpg          full develop, downscaled to 1600 px for viewing
//!   <name>-vs-embedded.jpg  ours (left) beside the camera's embedded JPEG (right)
//!   <name>-crop.png     a 100% crop from the centre
//!   <name>-half.jpg     the preview-size develop
//! Timings print with --nocapture; run with --release for meaningful numbers.

use std::path::{Path, PathBuf};
use std::time::Instant;

use editor_raw::{preview, DevelopParams, Session};
use image::{imageops, RgbaImage};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn out_dir() -> PathBuf {
    let d = root().join("target/raw/out");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn corpus() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root().join("testdata/raw"))
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.retain(|p| p.extension().map(|e| editor_raw::EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false));
    v.sort();
    v
}

fn save_jpeg(img: &RgbaImage, path: &Path) {
    image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(path).unwrap();
}

fn fit(img: &RgbaImage, long: u32) -> RgbaImage {
    let s = long as f32 / img.width().max(img.height()) as f32;
    if s >= 1.0 {
        return img.clone();
    }
    imageops::resize(img, (img.width() as f32 * s).round() as u32, (img.height() as f32 * s).round() as u32, imageops::FilterType::Triangle)
}

#[test]
fn develop_every_corpus_file() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("testdata/raw is empty; run crates/editor-raw/scripts/fetch-testdata.sh");
        return;
    }
    let out = out_dir();
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();

        let t = Instant::now();
        let info = editor_raw::probe(&bytes).unwrap_or_else(|e| panic!("{name}: probe failed: {e}"));
        let t_probe = t.elapsed();

        let t = Instant::now();
        let mut s = Session::open(&bytes).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
        let t_decode = t.elapsed();

        let p = DevelopParams::default();
        let t = Instant::now();
        let half = s.develop(&p, true);
        let t_half = t.elapsed();
        let t = Instant::now();
        let half2 = s.develop(&DevelopParams { exposure: 0.5, ..p.clone() }, true);
        let t_half2 = t.elapsed();
        assert_eq!(half.rgba.len(), half2.rgba.len());

        let t = Instant::now();
        let full = s.develop(&p, false);
        let t_full = t.elapsed();

        assert_eq!((full.width as u32, full.height as u32), (info.width, info.height), "{name}: probe size matches develop");
        let img = RgbaImage::from_raw(full.width as u32, full.height as u32, full.rgba).unwrap();
        let himg = RgbaImage::from_raw(half.width as u32, half.height as u32, half.rgba).unwrap();

        save_jpeg(&fit(&img, 1600), &out.join(format!("{name}.jpg")));
        save_jpeg(&fit(&himg, 1200), &out.join(format!("{name}-half.jpg")));
        let (cw, ch) = (800.min(img.width()), 600.min(img.height()));
        let crop = imageops::crop_imm(&img, (img.width() - cw) / 2, (img.height() - ch) / 2, cw, ch).to_image();
        crop.save(out.join(format!("{name}-crop.png"))).unwrap();

        let t = Instant::now();
        let jpeg = preview::best_jpeg(&bytes, 1000);
        let t_jpeg = t.elapsed();
        if let Some(j) = jpeg {
            let emb = image::load_from_memory(&bytes[j.offset..j.offset + j.len]).unwrap().to_rgba8();
            // Embedded previews are stored unrotated; apply the RAW's orientation.
            let (px, pw, ph) = editor_raw::orient::apply(emb.as_raw(), emb.width() as usize, emb.height() as usize, 4, info.orientation);
            let emb = RgbaImage::from_raw(pw as u32, ph as u32, px).unwrap();
            let a = fit(&img, 900);
            let b = imageops::resize(&emb, a.width(), a.height(), imageops::FilterType::Triangle);
            let mut side = RgbaImage::from_pixel(a.width() * 2 + 8, a.height(), image::Rgba([0, 0, 0, 255]));
            imageops::overlay(&mut side, &a, 0, 0);
            imageops::overlay(&mut side, &b, a.width() as i64 + 8, 0);
            save_jpeg(&side, &out.join(format!("{name}-vs-embedded.jpg")));
        }

        println!(
            "{name}: {} {} {}x{} {} iso {:?} wb {:.0}K {:+.0} orient {} | probe {:?} decode {:?} half {:?} (again {:?}) full {:?} | jpeg {:?} {:?}",
            info.make,
            info.model,
            info.width,
            info.height,
            info.sensor,
            info.iso,
            info.as_shot_wb.temperature,
            info.as_shot_wb.tint,
            info.orientation,
            t_probe,
            t_decode,
            t_half,
            t_half2,
            t_full,
            jpeg.map(|j| (j.width, j.height)),
            t_jpeg,
        );
    }
}
