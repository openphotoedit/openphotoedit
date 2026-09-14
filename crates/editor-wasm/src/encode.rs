//! PNG encoding. JPEG and WebP go through the browser's own encoders
//! (`OffscreenCanvas.convertToBlob`), which are fast and good.

pub fn png(rgba: &[u8], width: u32, height: u32, ppi: f32) -> Result<Vec<u8>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("expected {} bytes, got {}", width as usize * height as usize * 4, rgba.len()));
    }
    let mut out = Vec::with_capacity(rgba.len() / 2);
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        // Pixels per metre, so print size survives the round trip.
        let ppm = (ppi / 0.0254).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
