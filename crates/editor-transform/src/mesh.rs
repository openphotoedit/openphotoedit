//! Bézier-patch warp: a 4×4 control grid over a rectangle defines a bicubic
//! surface. It is subdivided into a fine triangle mesh whose destination
//! triangles are rasterised with an exact fixed-point fill rule (every pixel
//! belongs to one triangle), each pixel inverse-mapped through its triangle's
//! affine map and resampled. The mesh extends a few source pixels past the
//! rectangle, so edges are antialiased by the resampler the same way an
//! affine transform's are.

use editor_core::geom::{Affine, Point, Rect};
use editor_core::layer::Raster;
use editor_core::plane::Plane;
use editor_core::transform::Resample;

use crate::sample::{crop_to_content, finish, Source};

pub struct Mesh {
    n: usize,
    /// (n+1)² vertices: source and destination document positions.
    src: Vec<Point>,
    dst: Vec<Point>,
}

fn bernstein(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t]
}

/// The identity grid over `b`: control points at thirds.
pub fn identity_grid(b: Rect) -> Vec<Point> {
    let mut g = Vec::with_capacity(16);
    for j in 0..4 {
        for i in 0..4 {
            g.push(Point::new(b.x as f64 + b.w as f64 * i as f64 / 3.0, b.y as f64 + b.h as f64 * j as f64 / 3.0));
        }
    }
    g
}

pub fn eval_patch(grid: &[Point], u: f64, v: f64) -> Point {
    let bu = bernstein(u);
    let bv = bernstein(v);
    let (mut x, mut y) = (0.0, 0.0);
    for j in 0..4 {
        for i in 0..4 {
            let w = bu[i] * bv[j];
            x += w * grid[j * 4 + i].x;
            y += w * grid[j * 4 + i].y;
        }
    }
    Point::new(x, y)
}

impl Mesh {
    /// `grid` is 16 points, row-major from the top-left, over `bounds`.
    pub fn new(bounds: Rect, grid: &[Point]) -> Mesh {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in grid {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        let extent = (x1 - x0).max(y1 - y0).max(bounds.w.max(bounds.h) as f64);
        let n = ((extent / 6.0).ceil() as usize).clamp(4, 256);
        let margin = 3.0;
        let (bw, bh) = (bounds.w.max(1) as f64, bounds.h.max(1) as f64);
        let (mu, mv) = (margin / bw, margin / bh);
        let mut src = Vec::with_capacity((n + 1) * (n + 1));
        let mut dst = Vec::with_capacity((n + 1) * (n + 1));
        for j in 0..=n {
            let v = -mv + (1.0 + 2.0 * mv) * j as f64 / n as f64;
            for i in 0..=n {
                let u = -mu + (1.0 + 2.0 * mu) * i as f64 / n as f64;
                src.push(Point::new(bounds.x as f64 + u * bw, bounds.y as f64 + v * bh));
                dst.push(eval_patch(grid, u, v));
            }
        }
        Mesh { n, src, dst }
    }

    pub fn dst_bounds(&self) -> Rect {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &self.dst {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Rect::cover(x0, y0, x1 - x0, y1 - y0).inflate(1)
    }

    /// Resample `r` through the mesh. RGBA folds composite "over" each
    /// other; masks keep the last value.
    pub fn warp(&self, r: &Raster, method: Resample, clip: Rect) -> Raster {
        let src = Source::from_raster(r);
        if src.is_empty() {
            return r.clone();
        }
        let ch = src.ch;
        let dst = self.dst_bounds().intersect(&clip);
        if dst.is_empty() {
            return Raster::new(Plane::new(1, 1, ch, r.plane.fill()), 0, 0);
        }
        let (dw, dh) = (dst.w as usize, dst.h as usize);
        let mut buf = vec![if ch == 1 { src.fill } else { 0 }; dw * dh * ch];
        let n = self.n;
        let srect = src.doc_rect().inflate(4);
        for j in 0..n {
            for i in 0..n {
                let k = [j * (n + 1) + i, j * (n + 1) + i + 1, (j + 1) * (n + 1) + i + 1, (j + 1) * (n + 1) + i];
                // Skip cells whose source lies entirely outside the pixels.
                let sx0 = self.src[k[0]].x.min(self.src[k[3]].x);
                let sx1 = self.src[k[1]].x.max(self.src[k[2]].x);
                let sy0 = self.src[k[0]].y.min(self.src[k[1]].y);
                let sy1 = self.src[k[2]].y.max(self.src[k[3]].y);
                if sx1 < srect.x as f64 || sy1 < srect.y as f64 || sx0 > srect.right() as f64 || sy0 > srect.bottom() as f64 {
                    continue;
                }
                for tri in [[k[0], k[1], k[2]], [k[0], k[2], k[3]]] {
                    self.raster_tri(&src, tri, dst, &mut buf, method);
                }
            }
        }
        let plane = Plane::from_raw(dst.w as u32, dst.h as u32, ch, &buf, r.plane.fill());
        crop_to_content(Raster::new(plane, dst.x, dst.y))
    }

    fn raster_tri(&self, src: &Source, tri: [usize; 3], dst: Rect, buf: &mut [u8], method: Resample) {
        const SUB: f64 = 256.0;
        let q = |p: Point| ((p.x * SUB).round() as i64, (p.y * SUB).round() as i64);
        let mut v = [q(self.dst[tri[0]]), q(self.dst[tri[1]]), q(self.dst[tri[2]])];
        let mut s = [self.src[tri[0]], self.src[tri[1]], self.src[tri[2]]];
        let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[1].1 - v[0].1) * (v[2].0 - v[0].0);
        if area == 0 {
            return;
        }
        if area < 0 {
            v.swap(1, 2);
            s.swap(1, 2);
        }
        // Affine from snapped destination to source.
        let d: Vec<Point> = v.iter().map(|p| Point::new(p.0 as f64 / SUB, p.1 as f64 / SUB)).collect();
        let Some(m) = tri_affine(&d, &s) else { return };
        let jac = [m.a, m.c, m.b, m.d];
        let minx = (v.iter().map(|p| p.0).min().unwrap() as f64 / SUB).floor() as i32;
        let maxx = (v.iter().map(|p| p.0).max().unwrap() as f64 / SUB).ceil() as i32;
        let miny = (v.iter().map(|p| p.1).min().unwrap() as f64 / SUB).floor() as i32;
        let maxy = (v.iter().map(|p| p.1).max().unwrap() as f64 / SUB).ceil() as i32;
        let area_rect = Rect::new(minx, miny, maxx - minx + 1, maxy - miny + 1).intersect(&dst);
        if area_rect.is_empty() {
            return;
        }
        // Edge i runs v[i] -> v[i+1]. Pixel centres strictly inside, or on a
        // top or left edge, belong to this triangle.
        let edges: Vec<(i64, i64, i64, i64, bool)> = (0..3)
            .map(|i| {
                let a = v[i];
                let b = v[(i + 1) % 3];
                let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                // Clockwise on screen (y down) with positive area: top edge is
                // horizontal going right, left edges go up.
                let top_left = (ey == 0 && ex > 0) || ey < 0;
                (a.0, a.1, ex, ey, top_left)
            })
            .collect();
        let ch = src.ch;
        for py in area_rect.y..area_rect.bottom() {
            let cy = py as i64 * SUB as i64 + SUB as i64 / 2;
            for px in area_rect.x..area_rect.right() {
                let cx = px as i64 * SUB as i64 + SUB as i64 / 2;
                let inside = edges.iter().all(|&(ax, ay, ex, ey, tl)| {
                    let e = ex * (cy - ay) - ey * (cx - ax);
                    e > 0 || (e == 0 && tl)
                });
                if !inside {
                    continue;
                }
                let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                let sx = m.a * x + m.c * y + m.e;
                let sy = m.b * x + m.d * y + m.f;
                let acc = src.sample(sx, sy, &jac, method);
                let o = ((py - dst.y) as usize * dst.w as usize + (px - dst.x) as usize) * ch;
                if ch == 1 {
                    buf[o] = finish(acc, 1)[0];
                    continue;
                }
                let new = finish(acc, 4);
                let ba = buf[o + 3] as u32;
                if ba == 0 || new[3] == 255 {
                    buf[o..o + 4].copy_from_slice(&new);
                } else if new[3] > 0 {
                    let below = [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]];
                    buf[o..o + 4].copy_from_slice(&over(new, below));
                }
            }
        }
    }
}

/// Straight-alpha source-over.
pub fn over(f: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    let fa = f[3] as f32 / 255.0;
    let ba = b[3] as f32 / 255.0;
    let oa = fa + ba * (1.0 - fa);
    if oa <= 0.0 {
        return [0; 4];
    }
    let mut o = [0u8; 4];
    for c in 0..3 {
        o[c] = ((f[c] as f32 * fa + b[c] as f32 * ba * (1.0 - fa)) / oa).round().clamp(0.0, 255.0) as u8;
    }
    o[3] = (oa * 255.0).round() as u8;
    o
}

/// The affine map taking triangle `d` to triangle `s`.
fn tri_affine(d: &[Point], s: &[Point; 3]) -> Option<Affine> {
    let (x1, y1) = (d[1].x - d[0].x, d[1].y - d[0].y);
    let (x2, y2) = (d[2].x - d[0].x, d[2].y - d[0].y);
    let det = x1 * y2 - x2 * y1;
    if det.abs() < 1e-12 {
        return None;
    }
    let (u1, v1) = (s[1].x - s[0].x, s[1].y - s[0].y);
    let (u2, v2) = (s[2].x - s[0].x, s[2].y - s[0].y);
    // [a c; b d] * [x1 x2; y1 y2] = [u1 u2; v1 v2]
    let a = (u1 * y2 - u2 * y1) / det;
    let c = (u2 * x1 - u1 * x2) / det;
    let b = (v1 * y2 - v2 * y1) / det;
    let dd = (v2 * x1 - v1 * x2) / det;
    let e = s[0].x - a * d[0].x - c * d[0].y;
    let f = s[0].y - b * d[0].x - dd * d[0].y;
    Some(Affine { a, b, c, d: dd, e, f })
}
