//! Resampling and whole-plane geometry: resize, rotate by quarter turns,
//! flip, and an affine warp for free transform and straightening.

use serde::{Deserialize, Serialize};

use crate::geom::{Affine, Point, Rect};
use crate::plane::Plane;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resample {
    Nearest,
    Bilinear,
    #[default]
    Bicubic,
    Lanczos,
}

fn kernel(method: Resample) -> (f64, fn(f64) -> f64) {
    match method {
        Resample::Nearest => (0.5, |x| if x.abs() <= 0.5 { 1.0 } else { 0.0 }),
        Resample::Bilinear => (1.0, |x| (1.0 - x.abs()).max(0.0)),
        Resample::Bicubic => (2.0, |x| {
            // Catmull-Rom (Keys, a = -0.5).
            let x = x.abs();
            if x < 1.0 {
                1.5 * x * x * x - 2.5 * x * x + 1.0
            } else if x < 2.0 {
                -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
            } else {
                0.0
            }
        }),
        Resample::Lanczos => (3.0, |x| {
            let x = x.abs();
            if x < 1e-8 {
                1.0
            } else if x < 3.0 {
                let px = std::f64::consts::PI * x;
                3.0 * px.sin() * (px / 3.0).sin() / (px * px)
            } else {
                0.0
            }
        }),
    }
}

/// Precomputed filter taps for one axis.
fn taps(src: usize, dst: usize, method: Resample) -> Vec<(usize, Vec<f32>)> {
    let (support, f) = kernel(method);
    let ratio = src as f64 / dst as f64;
    let scale = ratio.max(1.0);
    let support = support * scale;
    (0..dst)
        .map(|i| {
            let centre = (i as f64 + 0.5) * ratio;
            let start = ((centre - support).floor().max(0.0)) as usize;
            let end = ((centre + support).ceil() as usize).min(src);
            let mut w: Vec<f32> = (start..end).map(|j| f((j as f64 + 0.5 - centre) / scale) as f32).collect();
            let sum: f32 = w.iter().sum();
            if sum.abs() > 1e-8 {
                w.iter_mut().for_each(|v| *v /= sum);
            } else if !w.is_empty() {
                let n = w.len() as f32;
                w.iter_mut().for_each(|v| *v = 1.0 / n);
            }
            (start, w)
        })
        .collect()
}

/// Resize raw pixels (`channels` 1 or 4, straight alpha) with a separable
/// filter. RGBA is filtered premultiplied so transparent edges stay clean.
pub fn resize_raw(src: &[u8], sw: usize, sh: usize, channels: usize, dw: usize, dh: usize, method: Resample) -> Vec<u8> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return vec![0; dw * dh * channels];
    }
    let mut f: Vec<f32> = Vec::with_capacity(sw * sh * channels);
    if channels == 4 {
        for p in src.chunks_exact(4) {
            let a = p[3] as f32 / 255.0;
            f.extend_from_slice(&[p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, p[3] as f32]);
        }
    } else {
        f.extend(src.iter().map(|&v| v as f32));
    }
    let hx = taps(sw, dw, method);
    let mut mid = vec![0f32; dw * sh * channels];
    for y in 0..sh {
        for (x, (start, w)) in hx.iter().enumerate() {
            for c in 0..channels {
                let mut acc = 0f32;
                for (k, wk) in w.iter().enumerate() {
                    acc += f[(y * sw + start + k) * channels + c] * wk;
                }
                mid[(y * dw + x) * channels + c] = acc;
            }
        }
    }
    let hy = taps(sh, dh, method);
    let mut out = vec![0u8; dw * dh * channels];
    for (y, (start, w)) in hy.iter().enumerate() {
        for x in 0..dw {
            let mut acc = [0f32; 4];
            for (k, wk) in w.iter().enumerate() {
                let i = ((start + k) * dw + x) * channels;
                for c in 0..channels {
                    acc[c] += mid[i + c] * wk;
                }
            }
            let o = (y * dw + x) * channels;
            if channels == 4 {
                let a = acc[3].clamp(0.0, 255.0);
                if a > 0.0 {
                    for c in 0..3 {
                        out[o + c] = (acc[c] / (a / 255.0)).round().clamp(0.0, 255.0) as u8;
                    }
                }
                out[o + 3] = a.round() as u8;
            } else {
                out[o] = acc[0].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

pub fn resize_plane(p: &Plane, dw: u32, dh: u32, method: Resample) -> Plane {
    let raw = p.to_raw();
    let out = resize_raw(&raw, p.width() as usize, p.height() as usize, p.channels(), dw as usize, dh as usize, method);
    Plane::from_raw(dw, dh, p.channels(), &out, p.fill())
}

/// Rotate clockwise by `turns` quarter turns.
pub fn rotate_quarter(p: &Plane, turns: i32) -> Plane {
    let t = turns.rem_euclid(4);
    if t == 0 {
        return p.clone();
    }
    let (w, h, ch) = (p.width() as usize, p.height() as usize, p.channels());
    let raw = p.to_raw();
    let (nw, nh) = if t % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![0u8; nw * nh * ch];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match t {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let s = (y * w + x) * ch;
            let d = (ny * nw + nx) * ch;
            out[d..d + ch].copy_from_slice(&raw[s..s + ch]);
        }
    }
    Plane::from_raw(nw as u32, nh as u32, ch, &out, p.fill())
}

pub fn flip(p: &Plane, horizontal: bool) -> Plane {
    let (w, h, ch) = (p.width() as usize, p.height() as usize, p.channels());
    let raw = p.to_raw();
    let mut out = vec![0u8; raw.len()];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = if horizontal { (w - 1 - x, y) } else { (x, h - 1 - y) };
            let s = (sy * w + sx) * ch;
            let d = (y * w + x) * ch;
            out[d..d + ch].copy_from_slice(&raw[s..s + ch]);
        }
    }
    Plane::from_raw(w as u32, h as u32, ch, &out, p.fill())
}

/// Bicubic-free affine warp: maps `src` (positioned at `src_origin` in the
/// document) through `m` and returns the result with its document origin.
/// Bilinear sampling, premultiplied for RGBA.
pub fn warp_affine(src: &Plane, src_origin: (i32, i32), m: &Affine) -> (Plane, i32, i32) {
    let src_rect = Rect::new(src_origin.0, src_origin.1, src.width() as i32, src.height() as i32);
    let dst = m.bounds(&src_rect);
    let Some(inv) = m.invert() else { return (Plane::new(1, 1, src.channels(), src.fill()), 0, 0) };
    if dst.is_empty() {
        return (Plane::new(1, 1, src.channels(), src.fill()), 0, 0);
    }
    let ch = src.channels();
    let mut out = Plane::new(dst.w as u32, dst.h as u32, ch, src.fill());
    let rows_per_band = 256;
    let mut y0 = 0;
    while y0 < dst.h {
        let band_h = rows_per_band.min(dst.h - y0);
        let band = Rect::new(0, y0, dst.w, band_h);
        let mut data = vec![0u8; dst.w as usize * band_h as usize * ch];
        for j in 0..band_h {
            for i in 0..dst.w {
                let p = inv.apply(Point::new(dst.x as f64 + i as f64 + 0.5, dst.y as f64 + y0 as f64 + j as f64 + 0.5));
                let sx = p.x - src_origin.0 as f64 - 0.5;
                let sy = p.y - src_origin.1 as f64 - 0.5;
                let px = bilinear(src, sx, sy);
                let o = (j as usize * dst.w as usize + i as usize) * ch;
                data[o..o + ch].copy_from_slice(&px[..ch]);
            }
        }
        out.write(band, &data);
        y0 += band_h;
    }
    out.compact();
    (out, dst.x, dst.y)
}

pub fn bilinear(p: &Plane, x: f64, y: f64) -> [u8; 4] {
    let x0 = x.floor();
    let y0 = y.floor();
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let s = [p.get(ix, iy), p.get(ix + 1, iy), p.get(ix, iy + 1), p.get(ix + 1, iy + 1)];
    let w = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let mut out = [0u8; 4];
    if p.channels() == 4 {
        let a: f32 = (0..4).map(|k| s[k][3] as f32 * w[k]).sum();
        if a > 0.0 {
            for c in 0..3 {
                let v: f32 = (0..4).map(|k| s[k][c] as f32 * s[k][3] as f32 * w[k]).sum();
                out[c] = (v / a).round().clamp(0.0, 255.0) as u8;
            }
        }
        out[3] = a.round().clamp(0.0, 255.0) as u8;
    } else {
        let v: f32 = (0..4).map(|k| s[k][0] as f32 * w[k]).sum();
        out[0] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_solid_stays_solid() {
        let src: Vec<u8> = (0..16 * 16).flat_map(|_| [200u8, 100, 50, 255]).collect();
        for m in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic, Resample::Lanczos] {
            let out = resize_raw(&src, 16, 16, 4, 7, 23, m);
            assert!(out.chunks_exact(4).all(|p| (p[0] as i32 - 200).abs() <= 1 && p[3] == 255), "{m:?}");
        }
    }

    #[test]
    fn rotate_four_times_is_identity() {
        let raw: Vec<u8> = (0..5 * 3).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        let p = Plane::from_raw(5, 3, 4, &raw, [0; 4]);
        let r1 = rotate_quarter(&p, 1);
        assert_eq!((r1.width(), r1.height()), (3, 5));
        // top-left of the original ends at top-right after a clockwise turn
        assert_eq!(r1.get(2, 0)[0], 0);
        let back = rotate_quarter(&rotate_quarter(&r1, 1), 2);
        assert_eq!(back.to_raw(), raw);
    }

    #[test]
    fn flip_twice_is_identity() {
        let raw: Vec<u8> = (0..4 * 3).flat_map(|i| [i as u8, 1, 2, 255]).collect();
        let p = Plane::from_raw(4, 3, 4, &raw, [0; 4]);
        assert_eq!(flip(&flip(&p, true), true).to_raw(), raw);
        assert_eq!(flip(&p, false).get(0, 0)[0], 8);
    }

    #[test]
    fn translate_warp_moves_pixels() {
        let raw: Vec<u8> = (0..4 * 4).flat_map(|_| [255u8, 0, 0, 255]).collect();
        let p = Plane::from_raw(4, 4, 4, &raw, [0; 4]);
        let (out, x, y) = warp_affine(&p, (0, 0), &Affine::translate(10.0, 5.0));
        assert_eq!((x, y), (10, 5));
        assert_eq!(out.get(1, 1), [255, 0, 0, 255]);
    }
}
