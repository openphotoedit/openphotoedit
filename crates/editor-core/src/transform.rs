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


/// A 3×3 projective transform, row-major, mapping `(x, y, 1)`.
#[derive(Clone, Copy, Debug)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    /// The transform taking the rectangle `(0,0)-(w,h)` to `quad`
    /// (top-left, top-right, bottom-right, bottom-left).
    pub fn rect_to_quad(w: f64, h: f64, quad: &[Point; 4]) -> Option<Homography> {
        let src = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        // Solve the 8×8 system for h0..h7 (h8 = 1).
        let mut a = [[0f64; 9]; 8];
        for i in 0..4 {
            let (x, y) = src[i];
            let (u, v) = (quad[i].x, quad[i].y);
            a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        for col in 0..8 {
            let pivot = (col..8).max_by(|&r1, &r2| a[r1][col].abs().total_cmp(&a[r2][col].abs()))?;
            if a[pivot][col].abs() < 1e-12 {
                return None;
            }
            a.swap(col, pivot);
            for r in 0..8 {
                if r != col {
                    let f = a[r][col] / a[col][col];
                    for c in col..9 {
                        a[r][c] -= f * a[col][c];
                    }
                }
            }
        }
        let hvals: Vec<f64> = (0..8).map(|i| a[i][8] / a[i][i]).collect();
        Some(Homography([hvals[0], hvals[1], hvals[2], hvals[3], hvals[4], hvals[5], hvals[6], hvals[7], 1.0]))
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.0;
        let wz = m[6] * x + m[7] * y + m[8];
        ((m[0] * x + m[1] * y + m[2]) / wz, (m[3] * x + m[4] * y + m[5]) / wz)
    }

    pub fn invert(&self) -> Option<Homography> {
        let m = &self.0;
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6]) + m[2] * (m[3] * m[7] - m[4] * m[6]);
        if det.abs() < 1e-15 {
            return None;
        }
        let inv = [
            (m[4] * m[8] - m[5] * m[7]) / det,
            (m[2] * m[7] - m[1] * m[8]) / det,
            (m[1] * m[5] - m[2] * m[4]) / det,
            (m[5] * m[6] - m[3] * m[8]) / det,
            (m[0] * m[8] - m[2] * m[6]) / det,
            (m[2] * m[3] - m[0] * m[5]) / det,
            (m[3] * m[7] - m[4] * m[6]) / det,
            (m[1] * m[6] - m[0] * m[7]) / det,
            (m[0] * m[4] - m[1] * m[3]) / det,
        ];
        Some(Homography(inv))
    }
}

/// Warp a plane so its rectangle lands on `quad` (document coordinates).
/// Strong minification first downsamples the source, so the result does
/// not alias. Returns the plane and its document origin.
pub fn warp_to_quad(src: &Plane, quad: &[Point; 4]) -> (Plane, i32, i32) {
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in quad {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let dst = Rect::cover(x0, y0, x1 - x0, y1 - y0);
    if dst.is_empty() || sw < 1.0 || sh < 1.0 {
        return (Plane::new(1, 1, src.channels(), src.fill()), 0, 0);
    }
    // Pre-shrink when the quad is much smaller than the source.
    let edge = |a: Point, b: Point| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    let out_w = edge(quad[0], quad[1]).max(edge(quad[3], quad[2]));
    let out_h = edge(quad[0], quad[3]).max(edge(quad[1], quad[2]));
    let (work, kx, ky);
    if out_w < sw * 0.5 || out_h < sh * 0.5 {
        let nw = (out_w.ceil() as u32).clamp(1, src.width());
        let nh = (out_h.ceil() as u32).clamp(1, src.height());
        work = resize_plane(src, nw, nh, Resample::Bicubic);
        kx = sw / nw as f64;
        ky = sh / nh as f64;
    } else {
        work = src.clone();
        kx = 1.0;
        ky = 1.0;
    }
    let Some(h) = Homography::rect_to_quad(sw, sh, quad).and_then(|h| h.invert()) else {
        return (Plane::new(1, 1, src.channels(), src.fill()), 0, 0);
    };
    let ch = src.channels();
    let mut out = Plane::new(dst.w as u32, dst.h as u32, ch, src.fill());
    let band_rows = 256;
    let mut yb = 0;
    while yb < dst.h {
        let bh = band_rows.min(dst.h - yb);
        let mut data = vec![0u8; dst.w as usize * bh as usize * ch];
        for j in 0..bh {
            for i in 0..dst.w {
                let (u, v) = h.apply(dst.x as f64 + i as f64 + 0.5, dst.y as f64 + (yb + j) as f64 + 0.5);
                if u < -1.0 || v < -1.0 || u > sw + 1.0 || v > sh + 1.0 {
                    continue;
                }
                let px = bilinear(&work, u / kx - 0.5, v / ky - 0.5);
                let o = (j as usize * dst.w as usize + i as usize) * ch;
                data[o..o + ch].copy_from_slice(&px[..ch]);
            }
        }
        out.write(Rect::new(0, yb, dst.w, bh), &data);
        yb += bh;
    }
    out.compact();
    (out, dst.x, dst.y)
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
    fn homography_maps_corners_and_quad_warp_scales() {
        let q = [Point::new(10.0, 20.0), Point::new(110.0, 25.0), Point::new(100.0, 90.0), Point::new(5.0, 80.0)];
        let h = Homography::rect_to_quad(50.0, 40.0, &q).unwrap();
        for (i, (x, y)) in [(0.0, 0.0), (50.0, 0.0), (50.0, 40.0), (0.0, 40.0)].iter().enumerate() {
            let (u, v) = h.apply(*x, *y);
            assert!((u - q[i].x).abs() < 1e-6 && (v - q[i].y).abs() < 1e-6);
        }
        let raw: Vec<u8> = (0..20 * 10).flat_map(|_| [0u8, 200, 0, 255]).collect();
        let p = Plane::from_raw(20, 10, 4, &raw, [0; 4]);
        let rect = [Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 20.0), Point::new(0.0, 20.0)];
        let (out, x, y) = warp_to_quad(&p, &rect);
        assert_eq!((x, y, out.width(), out.height()), (0, 0, 40, 20));
        assert_eq!(out.get(20, 10), [0, 200, 0, 255]);
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
