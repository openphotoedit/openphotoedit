//! `openphotoedit render`: the engine, natively, from the command line.
//!
//! Decode an image, open it as a document with `doc.open-pixels`, run JSON
//! commands exactly as the UI sends them to the wasm worker, flatten, write a
//! PNG. It proves the engine is not tied to WebAssembly, and it makes the
//! engine scriptable.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use editor_core::Editor;
use image::ImageDecoder;
use serde_json::Value;

pub struct Report {
    pub width: u32,
    pub height: u32,
    pub commands: usize,
    pub layers: usize,
    pub out: PathBuf,
}

/// Decode `input` into straight RGBA8, applying the EXIF orientation so the
/// document is upright, as the browser shows it.
pub fn decode(input: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let reader = image::ImageReader::open(input)
        .with_context(|| format!("could not open {}", input.display()))?
        .with_guessed_format()
        .with_context(|| format!("could not read {}", input.display()))?;
    let mut decoder = reader.into_decoder().with_context(|| format!("{} is not an image this build can read", input.display()))?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).with_context(|| format!("could not decode {}", input.display()))?;
    img.apply_orientation(orientation);
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw()))
}

/// Parse `--op` values. Each is one command object or an array of them.
pub fn parse_ops(ops: &[String]) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for (i, text) in ops.iter().enumerate() {
        let v: Value = serde_json::from_str(text).with_context(|| format!("--op #{} is not valid JSON", i + 1))?;
        match v {
            Value::Array(items) => out.extend(items),
            Value::Object(_) => out.push(v),
            _ => bail!("--op #{} must be a JSON object or an array of objects", i + 1),
        }
    }
    for (i, v) in out.iter().enumerate() {
        if v.get("op").and_then(Value::as_str).is_none() {
            bail!("command #{} has no \"op\"", i + 1);
        }
    }
    Ok(out)
}

pub fn render(input: &Path, ops: &[Value], out: &Path) -> Result<Report> {
    let (w, h, rgba) = decode(input)?;
    let mut ed = Editor::new(1, 1);
    let name = input.file_name().map(|n| n.to_string_lossy().into_owned());
    let open = serde_json::json!({ "op": "doc.open-pixels", "width": w, "height": h, "name": name });
    ed.exec(open, &rgba).context("the engine refused to open the image")?;
    drop(rgba);
    for (i, cmd) in ops.iter().enumerate() {
        let op = cmd["op"].as_str().unwrap_or("?").to_string();
        let res = ed.exec(cmd.clone(), &[]).map_err(|e| anyhow::anyhow!("command #{} ({op}) failed: {e}", i + 1))?;
        tracing::info!(op, result = %res, "command");
    }
    let (dw, dh) = (ed.doc.width, ed.doc.height);
    let flat = editor_core::render::flatten(&ed.doc);
    encode_png(out, &flat, dw, dh, ed.doc.resolution)?;
    let layers = ed.summary()["layers"].as_array().map(Vec::len).unwrap_or(0);
    Ok(Report { width: dw, height: dh, commands: ops.len(), layers, out: out.to_path_buf() })
}

fn encode_png(path: &Path, rgba: &[u8], w: u32, h: u32, ppi: f32) -> Result<()> {
    let file = std::fs::File::create(path).with_context(|| format!("could not create {}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    if ppi.is_finite() && ppi > 0.0 {
        let ppm = (ppi / 0.0254).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
    }
    let mut writer = enc.write_header().context("could not write the PNG header")?;
    writer.write_image_data(rgba).context("could not write the PNG")?;
    writer.finish().context("could not finish the PNG")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ops_accepts_objects_and_arrays() {
        let v = parse_ops(&[r#"{"op":"a"}"#.into(), r#"[{"op":"b"},{"op":"c"}]"#.into()]).unwrap();
        assert_eq!(v.len(), 3);
        assert!(parse_ops(&["{}".into()]).is_err());
        assert!(parse_ops(&["3".into()]).is_err());
    }

    #[test]
    fn exposure_brightens_natively() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("grey.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([100, 100, 100, 255])).save(&input).unwrap();
        let out = dir.path().join("out.png");
        let ops = parse_ops(&[r#"{"op":"layer.add-adjustment","adjustment":{"kind":"develop","exposure":1}}"#.into()]).unwrap();
        let r = render(&input, &ops, &out).unwrap();
        assert_eq!((r.width, r.height, r.layers), (8, 6, 2));
        let img = image::open(&out).unwrap().into_rgba8();
        assert!(img.get_pixel(3, 3)[0] > 120, "exposure +1 should brighten, got {:?}", img.get_pixel(3, 3));
    }
}
