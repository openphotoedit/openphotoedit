//! Image buffers and resampling.

use editor_core::transform::{resize_raw, Resample};

/// An 8-bit interleaved RGB image.
#[derive(Clone, Debug, PartialEq)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgb {
    pub fn new(width: u32, height: u32) -> Self {
        Rgb { width, height, data: vec![0; (width * height * 3) as usize] }
    }

    pub fn from_vec(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(data.len(), (width * height * 3) as usize, "RGB buffer size");
        Rgb { width, height, data }
    }

    /// Drop the alpha channel of straight RGBA.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let n = (width * height) as usize;
        assert!(rgba.len() >= n * 4, "RGBA buffer size");
        let mut data = Vec::with_capacity(n * 3);
        for px in rgba[..n * 4].chunks_exact(4) {
            data.extend_from_slice(&px[..3]);
        }
        Rgb { width, height, data }
    }

    /// Straight RGBA with the given alpha plane (or opaque).
    pub fn to_rgba(&self, alpha: Option<&[u8]>) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() / 3 * 4);
        for (i, px) in self.data.chunks_exact(3).enumerate() {
            out.extend_from_slice(px);
            out.push(alpha.map_or(255, |a| a[i]));
        }
        out
    }

    #[inline]
    pub fn px(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Planar `f32` NCHW in `[0, 1]`.
    pub fn to_chw01(&self) -> Vec<f32> {
        let plane = (self.width * self.height) as usize;
        let mut out = vec![0.0; plane * 3];
        for (i, px) in self.data.chunks_exact(3).enumerate() {
            out[i] = px[0] as f32 / 255.0;
            out[plane + i] = px[1] as f32 / 255.0;
            out[2 * plane + i] = px[2] as f32 / 255.0;
        }
        out
    }

    /// Planar NCHW with per-channel `(v/255 - mean) / std`.
    pub fn to_chw_norm(&self, mean: [f32; 3], std: [f32; 3]) -> Vec<f32> {
        let plane = (self.width * self.height) as usize;
        let mut out = vec![0.0; plane * 3];
        for (i, px) in self.data.chunks_exact(3).enumerate() {
            for c in 0..3 {
                out[c * plane + i] = (px[c] as f32 / 255.0 - mean[c]) / std[c];
            }
        }
        out
    }

    /// The inverse of [`Rgb::to_chw01`], clamping.
    pub fn from_chw01(width: u32, height: u32, chw: &[f32]) -> Self {
        let plane = (width * height) as usize;
        assert!(chw.len() >= plane * 3, "CHW buffer too small");
        let mut data = vec![0u8; plane * 3];
        for i in 0..plane {
            data[i * 3] = crate::q8(chw[i] * 255.0);
            data[i * 3 + 1] = crate::q8(chw[plane + i] * 255.0);
            data[i * 3 + 2] = crate::q8(chw[2 * plane + i] * 255.0);
        }
        Rgb { width, height, data }
    }

    /// Luma (Rec. 601), `0..=255`.
    pub fn luma(&self) -> Vec<f32> {
        self.data.chunks_exact(3).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect()
    }

    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Rgb {
        assert!(x + w <= self.width && y + h <= self.height, "crop outside image");
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for row in y..y + h {
            let start = ((row * self.width + x) * 3) as usize;
            data.extend_from_slice(&self.data[start..start + (w * 3) as usize]);
        }
        Rgb { width: w, height: h, data }
    }
}

/// Resize RGB with the engine's separable filters.
pub fn resize(img: &Rgb, width: u32, height: u32, method: Resample) -> Rgb {
    let (width, height) = (width.max(1), height.max(1));
    if width == img.width && height == img.height {
        return img.clone();
    }
    let rgba = img.to_rgba(None);
    let out = resize_raw(&rgba, img.width as usize, img.height as usize, 4, width as usize, height as usize, method);
    Rgb::from_rgba(width, height, &out)
}

/// Resize a single 8-bit plane.
pub fn resize_u8(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32, method: Resample) -> Vec<u8> {
    resize_raw(src, sw as usize, sh as usize, 1, dw as usize, dh as usize, method)
}

/// Bilinear resize of an `f32` plane (mattes, logits, chroma).
pub fn resize_plane(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    assert!(src.len() >= (sw * sh) as usize);
    if sw == dw && sh == dh {
        return src[..(sw * sh) as usize].to_vec();
    }
    let mut out = vec![0.0; (dw * dh) as usize];
    let sx = sw as f32 / dw as f32;
    let sy = sh as f32 / dh as f32;
    let xs: Vec<(usize, usize, f32)> = (0..dw)
        .map(|x| {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (sw - 1) as f32);
            let x0 = fx.floor() as u32;
            (x0 as usize, (x0 + 1).min(sw - 1) as usize, fx - x0 as f32)
        })
        .collect();
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (sh - 1) as f32);
        let y0 = fy.floor() as u32;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        let r0 = (y0 * sw) as usize;
        let r1 = (y1 * sw) as usize;
        let row = (y * dw) as usize;
        for (x, &(x0, x1, tx)) in xs.iter().enumerate() {
            let top = src[r0 + x0] * (1.0 - tx) + src[r0 + x1] * tx;
            let bot = src[r1 + x0] * (1.0 - tx) + src[r1 + x1] * tx;
            out[row + x] = top * (1.0 - ty) + bot * ty;
        }
    }
    out
}

/// Area-average an NCHW `f32` tensor of `c` channels by an integer factor.
pub fn box_down_chw(chw: &[f32], c: usize, w: u32, h: u32, factor: u32) -> Vec<f32> {
    let (ow, oh) = (w / factor, h / factor);
    let plane = (w * h) as usize;
    let oplane = (ow * oh) as usize;
    let mut out = vec![0.0; oplane * c];
    let norm = 1.0 / (factor * factor) as f32;
    for ch in 0..c {
        let src = &chw[ch * plane..(ch + 1) * plane];
        let dst = &mut out[ch * oplane..(ch + 1) * oplane];
        for oy in 0..oh {
            for ox in 0..ow {
                let mut acc = 0.0;
                for dy in 0..factor {
                    let row = ((oy * factor + dy) * w) as usize;
                    for dx in 0..factor {
                        acc += src[row + (ox * factor + dx) as usize];
                    }
                }
                dst[(oy * ow + ox) as usize] = acc * norm;
            }
        }
    }
    out
}

/// Longest side capped at `max`, keeping aspect.
pub fn fit_within(w: u32, h: u32, max: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max {
        return (w, h);
    }
    let s = max as f64 / long as f64;
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::synthetic;

    #[test]
    fn chw_round_trip() {
        let img = synthetic(13, 7);
        assert_eq!(Rgb::from_chw01(13, 7, &img.to_chw01()), img);
    }

    #[test]
    fn resize_keeps_flat_colour_and_changes_size() {
        let img = Rgb::from_vec(10, 10, vec![200; 300]);
        let up = resize(&img, 40, 30, Resample::Lanczos);
        assert_eq!((up.width, up.height), (40, 30));
        assert!(up.data.iter().all(|&v| v == 200));
    }

    #[test]
    fn box_down_averages() {
        let chw: Vec<f32> = vec![0.0, 1.0, 1.0, 0.0];
        assert_eq!(box_down_chw(&chw, 1, 2, 2, 2), vec![0.5]);
    }

    #[test]
    fn plane_resize_identity_and_interpolates() {
        let p = vec![0.0, 1.0, 0.0, 1.0];
        assert_eq!(resize_plane(&p, 2, 2, 2, 2), p);
        let big = resize_plane(&p, 2, 2, 4, 2);
        assert!(big[0] <= big[1] && big[1] <= big[2]);
    }
}
