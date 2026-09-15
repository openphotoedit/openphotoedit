//! The one resampler every transform uses.
//!
//! A [`Source`] is a raster's pixels copied into a flat buffer. It is sampled
//! at a document point together with the Jacobian of the destination→source
//! map at that point, which says how many source pixels one destination pixel
//! covers. Near 1:1 the chosen kernel (bicubic by default) is evaluated
//! directly; when the destination pixel covers more than ~1.25 source pixels
//! along an axis the pixel is supersampled with bilinear taps, and beyond 2×
//! the taps are read from a premultiplied 2×2 box pyramid, so strong
//! downscales stay alias-free at a bounded cost. Colour is always filtered
//! premultiplied by alpha, so transparent edges never pick up a dark fringe.

use std::cell::OnceCell;

use editor_core::geom::{Affine, Point, Rect};
use editor_core::layer::Raster;
use editor_core::plane::Plane;
use editor_core::transform::Resample;

/// A destination→source map with its Jacobian
/// `[dsx/dx, dsx/dy, dsy/dx, dsy/dy]`.
pub trait InvMap {
    fn map(&self, x: f64, y: f64) -> Option<([f64; 2], [f64; 4])>;
}

impl InvMap for Affine {
    #[inline]
    fn map(&self, x: f64, y: f64) -> Option<([f64; 2], [f64; 4])> {
        Some(([self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f], [self.a, self.c, self.b, self.d]))
    }
}

/// A projective transform, row-major 3×3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    /// Maps the rectangle `(x, y, w, h)` onto `quad` (corners TL, TR, BR, BL).
    pub fn rect_to_quad(x: f64, y: f64, w: f64, h: f64, quad: &[Point; 4]) -> Option<Homography> {
        if w.abs() < 1e-9 || h.abs() < 1e-9 {
            return None;
        }
        let [p0, p1, p2, p3] = *quad;
        let (dx1, dx2, dx3) = (p1.x - p2.x, p3.x - p2.x, p0.x - p1.x + p2.x - p3.x);
        let (dy1, dy2, dy3) = (p1.y - p2.y, p3.y - p2.y, p0.y - p1.y + p2.y - p3.y);
        // Unit square to quad (Heckbert).
        let sq = if dx3.abs() < 1e-12 && dy3.abs() < 1e-12 {
            [p1.x - p0.x, p2.x - p1.x, p0.x, p1.y - p0.y, p2.y - p1.y, p0.y, 0.0, 0.0, 1.0]
        } else {
            let det = dx1 * dy2 - dx2 * dy1;
            if det.abs() < 1e-12 {
                return None;
            }
            let g = (dx3 * dy2 - dx2 * dy3) / det;
            let hh = (dx1 * dy3 - dx3 * dy1) / det;
            [p1.x - p0.x + g * p1.x, p3.x - p0.x + hh * p3.x, p0.x, p1.y - p0.y + g * p1.y, p3.y - p0.y + hh * p3.y, p0.y, g, hh, 1.0]
        };
        let norm = [1.0 / w, 0.0, -x / w, 0.0, 1.0 / h, -y / h, 0.0, 0.0, 1.0];
        let hm = Homography(mul3(&sq, &norm));
        hm.invert()?;
        Some(hm)
    }

    pub fn invert(&self) -> Option<Homography> {
        let m = &self.0;
        let c00 = m[4] * m[8] - m[5] * m[7];
        let c01 = m[5] * m[6] - m[3] * m[8];
        let c02 = m[3] * m[7] - m[4] * m[6];
        let det = m[0] * c00 + m[1] * c01 + m[2] * c02;
        if det.abs() < 1e-18 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Homography([
            c00 * inv,
            (m[2] * m[7] - m[1] * m[8]) * inv,
            (m[1] * m[5] - m[2] * m[4]) * inv,
            c01 * inv,
            (m[0] * m[8] - m[2] * m[6]) * inv,
            (m[2] * m[3] - m[0] * m[5]) * inv,
            c02 * inv,
            (m[1] * m[6] - m[0] * m[7]) * inv,
            (m[0] * m[4] - m[1] * m[3]) * inv,
        ]))
    }

    /// Scale so the homogeneous weight is positive at `p`, which makes "weight
    /// ≤ 0" mean "behind the horizon" everywhere.
    pub fn oriented_at(mut self, p: Point) -> Homography {
        let m = &self.0;
        if m[6] * p.x + m[7] * p.y + m[8] < 0.0 {
            self.0.iter_mut().for_each(|v| *v = -*v);
        }
        self
    }

    #[inline]
    pub fn apply(&self, p: Point) -> Option<Point> {
        let m = &self.0;
        let w = m[6] * p.x + m[7] * p.y + m[8];
        if w <= 1e-12 {
            return None;
        }
        Some(Point::new((m[0] * p.x + m[1] * p.y + m[2]) / w, (m[3] * p.x + m[4] * p.y + m[5]) / w))
    }
}

fn mul3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut o = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            o[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    o
}

impl InvMap for Homography {
    #[inline]
    fn map(&self, x: f64, y: f64) -> Option<([f64; 2], [f64; 4])> {
        let m = &self.0;
        let w = m[6] * x + m[7] * y + m[8];
        if w <= 1e-12 {
            return None;
        }
        let xs = m[0] * x + m[1] * y + m[2];
        let ys = m[3] * x + m[4] * y + m[5];
        let iw = 1.0 / w;
        let (sx, sy) = (xs * iw, ys * iw);
        let j = [(m[0] - sx * m[6]) * iw, (m[1] - sx * m[7]) * iw, (m[3] - sy * m[6]) * iw, (m[4] - sy * m[7]) * iw];
        Some(([sx, sy], j))
    }
}

struct Level {
    data: Vec<u8>,
    w: i32,
    h: i32,
}

/// Pixels to sample: straight RGBA (fill transparent) or one channel with a
/// fill value, positioned in the document.
pub struct Source {
    data: Vec<u8>,
    pub w: i32,
    pub h: i32,
    pub ch: usize,
    pub fill: u8,
    /// Document coordinate of the buffer's top-left corner.
    pub ox: f64,
    pub oy: f64,
    levels: OnceCell<Vec<Level>>,
}

const MAX_SS: usize = 12;

impl Source {
    pub fn from_raw(data: Vec<u8>, w: i32, h: i32, ch: usize, fill: u8, ox: f64, oy: f64) -> Source {
        assert_eq!(data.len(), w.max(0) as usize * h.max(0) as usize * ch);
        Source { data, w, h, ch, fill, ox, oy, levels: OnceCell::new() }
    }

    /// The content of a raster (pixels with alpha, or mask values that differ
    /// from the fill). Everything else samples as transparent / the fill.
    pub fn from_raster(r: &Raster) -> Source {
        let b = r.plane.content_bounds();
        Source::from_plane_rect(&r.plane, b, r.x, r.y)
    }

    /// `rect` (plane coordinates) of a plane positioned at `(px, py)`.
    pub fn from_plane_rect(p: &Plane, rect: Rect, px: i32, py: i32) -> Source {
        let data = if rect.is_empty() { Vec::new() } else { p.read_vec(rect) };
        let (w, h) = if rect.is_empty() { (0, 0) } else { (rect.w, rect.h) };
        Source::from_raw(data, w, h, p.channels(), p.fill()[0], (px + rect.x) as f64, (py + rect.y) as f64)
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Document rectangle covered by the source buffer.
    pub fn doc_rect(&self) -> Rect {
        Rect::new(self.ox as i32, self.oy as i32, self.w.max(0), self.h.max(0))
    }

    fn fill_acc(&self) -> [f32; 4] {
        if self.ch == 4 {
            [0.0; 4]
        } else {
            [self.fill as f32, 0.0, 0.0, 0.0]
        }
    }

    fn levels(&self) -> &Vec<Level> {
        self.levels.get_or_init(|| {
            let mut out: Vec<Level> = Vec::new();
            let (mut w, mut h) = (self.w, self.h);
            while w > 1 || h > 1 {
                let prev: &[u8] = out.last().map_or(&self.data, |l| &l.data);
                let (nw, nh) = ((w + 1) / 2, (h + 1) / 2);
                let ch = self.ch;
                let mut next = vec![0u8; nw as usize * nh as usize * ch];
                for y in 0..nh {
                    for x in 0..nw {
                        let mut acc = [0u32; 4];
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (sx, sy) = (2 * x + dx, 2 * y + dy);
                            if sx >= w || sy >= h {
                                if ch == 1 {
                                    acc[0] += self.fill as u32;
                                }
                                continue;
                            }
                            let i = (sy * w + sx) as usize * ch;
                            if ch == 4 {
                                let a = prev[i + 3] as u32;
                                acc[0] += prev[i] as u32 * a;
                                acc[1] += prev[i + 1] as u32 * a;
                                acc[2] += prev[i + 2] as u32 * a;
                                acc[3] += a;
                            } else {
                                acc[0] += prev[i] as u32;
                            }
                        }
                        let o = (y * nw + x) as usize * ch;
                        if ch == 4 {
                            for c in 0..3 {
                                next[o + c] = (acc[c] + acc[3] / 2).checked_div(acc[3]).unwrap_or(0).min(255) as u8;
                            }
                            next[o + 3] = ((acc[3] + 2) / 4) as u8;
                        } else {
                            next[o] = ((acc[0] + 2) / 4) as u8;
                        }
                    }
                }
                out.push(Level { data: next, w: nw, h: nh });
                w = nw;
                h = nh;
            }
            out
        })
    }

    #[inline(always)]
    fn level_data(&self, level: usize) -> (&[u8], i32, i32) {
        if level == 0 {
            (&self.data, self.w, self.h)
        } else {
            let l = &self.levels()[level - 1];
            (&l.data, l.w, l.h)
        }
    }

    /// Weighted taps starting at `(ix0, iy0)`; accumulates premultiplied.
    #[inline(always)]
    fn taps<const N: usize>(&self, level: usize, ix0: i32, iy0: i32, wx: &[f32; N], wy: &[f32; N], acc: &mut [f32; 4]) {
        let (d, w, h) = self.level_data(level);
        let ch = self.ch;
        let inside = ix0 >= 0 && iy0 >= 0 && ix0 + N as i32 <= w && iy0 + N as i32 <= h;
        for (j, &wyj) in wy.iter().enumerate() {
            if wyj == 0.0 {
                continue;
            }
            let yy = iy0 + j as i32;
            for (i, &wxi) in wx.iter().enumerate() {
                let wgt = wxi * wyj;
                if wgt == 0.0 {
                    continue;
                }
                let xx = ix0 + i as i32;
                if !inside && (xx < 0 || yy < 0 || xx >= w || yy >= h) {
                    if ch == 1 {
                        acc[0] += wgt * self.fill as f32;
                    }
                    continue;
                }
                let o = (yy as usize * w as usize + xx as usize) * ch;
                if ch == 4 {
                    let a = d[o + 3] as f32;
                    if a == 0.0 {
                        continue;
                    }
                    let wa = wgt * a;
                    acc[0] += wa * d[o] as f32;
                    acc[1] += wa * d[o + 1] as f32;
                    acc[2] += wa * d[o + 2] as f32;
                    acc[3] += wa;
                } else {
                    acc[0] += wgt * d[o] as f32;
                }
            }
        }
    }

    #[inline(always)]
    fn bilinear_acc(&self, level: usize, u: f64, v: f64, acc: &mut [f32; 4]) {
        let (x, y) = (u - 0.5, v - 0.5);
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = ((x - fx) as f32, (y - fy) as f32);
        self.taps(level, fx as i32, fy as i32, &[1.0 - tx, tx], &[1.0 - ty, ty], acc);
    }

    /// Sample at document point `(sx, sy)`. `jac` is the destination→source
    /// Jacobian for antialiasing. Returns premultiplied accumulators; pass
    /// them (or an average of several) to [`Source::finish`].
    pub fn sample(&self, sx: f64, sy: f64, jac: &[f64; 4], method: Resample) -> [f32; 4] {
        let u = sx - self.ox;
        let v = sy - self.oy;
        let fx = jac[0].hypot(jac[2]);
        let fy = jac[1].hypot(jac[3]);
        let margin = 4.0 + fx.max(fy) * 0.5;
        if self.is_empty() || u < -margin || v < -margin || u > self.w as f64 + margin || v > self.h as f64 + margin {
            return self.fill_acc();
        }
        let mut acc = [0f32; 4];
        if method == Resample::Nearest {
            self.taps(0, u.floor() as i32, v.floor() as i32, &[1.0], &[1.0], &mut acc);
            return acc;
        }
        let fmin = fx.min(fy);
        let mut level = 0usize;
        if fmin > 2.0 {
            let max_level = self.levels().len();
            level = ((fmin / 2.0).log2().floor().max(0.0) as usize).min(max_level);
        }
        let scale = (1u32 << level) as f64;
        let (fxl, fyl) = (fx / scale, fy / scale);
        let nx = if fxl > 1.25 { (fxl.ceil() as usize).min(MAX_SS) } else { 1 };
        let ny = if fyl > 1.25 { (fyl.ceil() as usize).min(MAX_SS) } else { 1 };
        if level == 0 && nx == 1 && ny == 1 {
            let (x, y) = (u - 0.5, v - 0.5);
            let (ix, iy) = (x.floor(), y.floor());
            let (tx, ty) = ((x - ix) as f32, (y - iy) as f32);
            let (ix, iy) = (ix as i32, iy as i32);
            match method {
                Resample::Bilinear => self.taps(0, ix, iy, &[1.0 - tx, tx], &[1.0 - ty, ty], &mut acc),
                Resample::Lanczos => self.taps(0, ix - 2, iy - 2, &lanczos3(tx), &lanczos3(ty), &mut acc),
                _ => self.taps(0, ix - 1, iy - 1, &catmull_rom(tx), &catmull_rom(ty), &mut acc),
            }
            return acc;
        }
        for j in 0..ny {
            let oy = (j as f64 + 0.5) / ny as f64 - 0.5;
            for i in 0..nx {
                let ox = (i as f64 + 0.5) / nx as f64 - 0.5;
                let su = u + jac[0] * ox + jac[1] * oy;
                let sv = v + jac[2] * ox + jac[3] * oy;
                self.bilinear_acc(level, su / scale, sv / scale, &mut acc);
            }
        }
        let n = (nx * ny) as f32;
        acc.iter_mut().for_each(|a| *a /= n);
        acc
    }

    /// Accumulators to straight 8-bit.
    #[inline]
    pub fn finish(&self, acc: [f32; 4]) -> [u8; 4] {
        finish(acc, self.ch)
    }
}

#[inline]
pub fn finish(acc: [f32; 4], ch: usize) -> [u8; 4] {
    if ch == 1 {
        return [acc[0].round().clamp(0.0, 255.0) as u8, 0, 0, 0];
    }
    let a = acc[3];
    if a < 0.5 {
        return [0; 4];
    }
    let inv = 1.0 / a;
    [
        (acc[0] * inv).round().clamp(0.0, 255.0) as u8,
        (acc[1] * inv).round().clamp(0.0, 255.0) as u8,
        (acc[2] * inv).round().clamp(0.0, 255.0) as u8,
        a.round().min(255.0) as u8,
    ]
}

#[inline(always)]
fn catmull_rom(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2]
}

fn lanczos3(t: f32) -> [f32; 6] {
    let mut w = [0f32; 6];
    let mut sum = 0.0;
    for (k, wk) in w.iter_mut().enumerate() {
        let x = (k as f32 - 2.0 - t).abs();
        *wk = if x < 1e-6 {
            1.0
        } else if x < 3.0 {
            let px = std::f32::consts::PI * x;
            3.0 * px.sin() * (px / 3.0).sin() / (px * px)
        } else {
            0.0
        };
        sum += *wk;
    }
    w.iter_mut().for_each(|v| *v /= sum);
    w
}

/// Render `dst` (document rectangle) by pulling every pixel centre through
/// `map`. Returns a positioned raster cropped to its content.
pub fn warp_to_raster(src: &Source, map: &impl InvMap, dst: Rect, method: Resample) -> Raster {
    let fill = if src.ch == 4 { [0; 4] } else { [src.fill, 0, 0, 0] };
    if dst.is_empty() {
        return Raster::new(Plane::new(1, 1, src.ch, fill), 0, 0);
    }
    let ch = src.ch;
    let mut plane = Plane::new(dst.w as u32, dst.h as u32, ch, fill);
    let band = 128;
    let mut buf = Vec::new();
    let mut y0 = 0;
    while y0 < dst.h {
        let bh = band.min(dst.h - y0);
        buf.clear();
        buf.resize(dst.w as usize * bh as usize * ch, 0);
        if ch == 1 && src.fill != 0 {
            buf.fill(src.fill);
        }
        let mut any = false;
        for j in 0..bh {
            let py = (dst.y + y0 + j) as f64 + 0.5;
            let row = j as usize * dst.w as usize;
            for i in 0..dst.w {
                let Some((s, jac)) = map.map((dst.x + i) as f64 + 0.5, py) else { continue };
                let px = src.finish(src.sample(s[0], s[1], &jac, method));
                let o = (row + i as usize) * ch;
                buf[o..o + ch].copy_from_slice(&px[..ch]);
                any = true;
            }
        }
        if any {
            plane.write(Rect::new(0, y0, dst.w, bh), &buf);
        }
        y0 += bh;
    }
    crop_to_content(Raster::new(plane, dst.x, dst.y))
}

/// Shrink a raster to the bounds of its content (alpha > 0, or mask values
/// that differ from the fill).
pub fn crop_to_content(mut r: Raster) -> Raster {
    r.plane.compact();
    let b = r.plane.content_bounds();
    if b.is_empty() {
        return Raster::new(Plane::new(1, 1, r.plane.channels(), r.plane.fill()), r.x, r.y);
    }
    if b == r.plane.bounds() {
        return r;
    }
    Raster::new(r.plane.extract(b), r.x + b.x, r.y + b.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_maps_corners() {
        let q = [Point::new(10.0, 5.0), Point::new(90.0, 20.0), Point::new(70.0, 80.0), Point::new(0.0, 60.0)];
        let h = Homography::rect_to_quad(0.0, 0.0, 100.0, 50.0, &q).unwrap();
        for (p, e) in [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)].iter().zip(q.iter()) {
            let r = h.apply(Point::new(p.0, p.1)).unwrap();
            assert!((r.x - e.x).abs() < 1e-9 && (r.y - e.y).abs() < 1e-9, "{r:?} vs {e:?}");
        }
        let inv = h.invert().unwrap();
        let back = inv.apply(Point::new(70.0, 80.0)).unwrap();
        assert!((back.x - 100.0).abs() < 1e-9 && (back.y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn strong_downscale_is_antialiased() {
        // 1-px checkerboard shrunk 8x must come out flat grey, not moire.
        let n = 256;
        let data: Vec<u8> = (0..n * n).flat_map(|i| if (i % n + i / n) % 2 == 0 { [255u8, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
        let src = Source::from_raw(data, n, n, 4, 0, 0.0, 0.0);
        let inv = Affine::scale(8.0, 8.0);
        let r = warp_to_raster(&src, &inv, Rect::new(0, 0, 32, 32), Resample::Bicubic);
        let raw = r.plane.to_raw();
        for px in raw.chunks_exact(4) {
            assert!((px[0] as i32 - 128).abs() <= 6, "got {}", px[0]);
        }
    }
}
