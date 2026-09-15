//! Colour-tolerance regions: the magic wand, the paint bucket, the magic
//! eraser and Select › Similar.
//!
//! Tolerance follows Photoshop: a pixel matches when every channel lies
//! within `tolerance` levels of the sampled colour (0 = exact, 255 = all).
//! Fully transparent pixels match each other whatever their RGB.

use editor_core::geom::Rect;
use editor_core::plane::Plane;

use crate::sample::Sampler;

/// A canvas-sized coverage buffer and the bounds of its non-zero pixels.
pub struct Region {
    pub mask: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub bounds: Rect,
}

impl Region {
    pub fn new(width: i32, height: i32) -> Region {
        Region { mask: vec![0; (width.max(0) as usize) * (height.max(0) as usize)], width, height, bounds: Rect::empty() }
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        self.mask[(y * self.width + x) as usize]
    }

    /// Coverage over `rect` (clipped to the canvas; outside reads 0).
    pub fn read(&self, rect: Rect) -> Vec<u8> {
        let mut out = vec![0u8; rect.area().max(0) as usize];
        let clip = rect.intersect(&Rect::new(0, 0, self.width, self.height));
        for y in clip.y..clip.bottom() {
            let src = (y * self.width) as usize;
            let dst = ((y - rect.y) * rect.w) as usize;
            for x in clip.x..clip.right() {
                out[dst + (x - rect.x) as usize] = self.mask[src + x as usize];
            }
        }
        out
    }

    /// Recompute `bounds` from the buffer.
    pub fn update_bounds(&mut self) {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for y in 0..self.height {
            let row = &self.mask[(y * self.width) as usize..((y + 1) * self.width) as usize];
            if let Some(first) = row.iter().position(|&v| v != 0) {
                let last = row.iter().rposition(|&v| v != 0).unwrap_or(first);
                x0 = x0.min(first as i32);
                x1 = x1.max(last as i32);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        self.bounds = if x1 >= x0 { Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1) } else { Rect::empty() };
    }

    /// A document-sized selection plane.
    pub fn to_plane(&self) -> Plane {
        let mut p = Plane::mask(self.width.max(0) as u32, self.height.max(0) as u32, 0);
        if !self.bounds.is_empty() {
            p.write(self.bounds, &self.read(self.bounds));
            p.compact();
        }
        p
    }
}

/// Photoshop's tolerance distance: the largest per-channel difference.
#[inline]
pub fn diff(a: [u8; 4], b: [u8; 4]) -> u8 {
    if a[3] == 0 && b[3] == 0 {
        return 0;
    }
    let mut m = 0u8;
    for c in 0..4 {
        m = m.max(a[c].abs_diff(b[c]));
    }
    m
}

/// The colour a wand click samples: a single pixel or the average of an
/// odd `size`×`size` square.
pub fn seed_color(s: &mut Sampler, x: i32, y: i32, size: u32) -> [u8; 4] {
    let r = (size.max(1) as i32 - 1) / 2;
    let mut sum = [0u32; 4];
    let mut n = 0u32;
    for yy in (y - r).max(0)..=(y + r).min(s.height - 1) {
        for xx in (x - r).max(0)..=(x + r).min(s.width - 1) {
            let p = s.get(xx, yy);
            for c in 0..4 {
                sum[c] += p[c] as u32;
            }
            n += 1;
        }
    }
    if n == 0 {
        return [0; 4];
    }
    sum.map(|v| ((v + n / 2) / n) as u8)
}

/// Flood fill from `(x, y)`: every pixel reachable through matching pixels.
/// `eight` selects 8-connectivity (diagonal steps) instead of 4.
pub fn contiguous(s: &mut Sampler, x: i32, y: i32, seed: [u8; 4], tolerance: u8, eight: bool) -> Region {
    let (w, h) = (s.width, s.height);
    let mut reg = Region::new(w, h);
    if x < 0 || y < 0 || x >= w || y >= h {
        return reg;
    }
    // 0 = untested, 1 = tested and outside, 255 = inside.
    const OUT: u8 = 1;
    const IN: u8 = 255;
    let mask = &mut reg.mask;
    let test = |s: &mut Sampler, mask: &mut [u8], x: i32, y: i32| -> bool {
        let i = (y * w + x) as usize;
        match mask[i] {
            0 => {
                if diff(s.get(x, y), seed) <= tolerance {
                    true
                } else {
                    mask[i] = OUT;
                    false
                }
            }
            _ => false,
        }
    };
    let (mut bx0, mut by0, mut bx1, mut by1) = (x, y, x, y);
    let mut stack = vec![(x, y)];
    while let Some((px, py)) = stack.pop() {
        if !test(s, mask, px, py) {
            continue;
        }
        let row = (py * w) as usize;
        let mut l = px;
        while l > 0 && test(s, mask, l - 1, py) {
            l -= 1;
        }
        let mut r = px;
        while r + 1 < w && test(s, mask, r + 1, py) {
            r += 1;
        }
        mask[row + l as usize..=row + r as usize].fill(IN);
        bx0 = bx0.min(l);
        bx1 = bx1.max(r);
        by0 = by0.min(py);
        by1 = by1.max(py);
        let (sx0, sx1) = if eight { ((l - 1).max(0), (r + 1).min(w - 1)) } else { (l, r) };
        for ny in [py - 1, py + 1] {
            if ny < 0 || ny >= h {
                continue;
            }
            let mut in_run = false;
            for nx in sx0..=sx1 {
                if test(s, mask, nx, ny) {
                    if !in_run {
                        stack.push((nx, ny));
                        in_run = true;
                    }
                } else {
                    in_run = false;
                }
            }
        }
    }
    for v in mask.iter_mut() {
        if *v == OUT {
            *v = 0;
        }
    }
    reg.bounds = Rect::new(bx0, by0, bx1 - bx0 + 1, by1 - by0 + 1);
    reg
}

/// Every matching pixel on the canvas.
pub fn global(s: &Sampler, seed: [u8; 4], tolerance: u8) -> Region {
    let (w, h) = (s.width, s.height);
    let mut reg = Region::new(w, h);
    let strip = 256;
    let mut y = 0;
    while y < h {
        let rows = strip.min(h - y);
        let px = s.read(Rect::new(0, y, w, rows));
        let dst = &mut reg.mask[(y * w) as usize..((y + rows) * w) as usize];
        for (d, p) in dst.iter_mut().zip(px.chunks_exact(4)) {
            if diff([p[0], p[1], p[2], p[3]], seed) <= tolerance {
                *d = 255;
            }
        }
        y += rows;
    }
    reg.update_bounds();
    reg
}

/// Soften a binary region's edge by one pixel, the way an anti-aliased wand
/// selection looks: only pixels on the boundary (inside or out) change, and
/// they take a 3×3 tent average of the hard mask.
pub fn antialias(reg: &mut Region) {
    if reg.bounds.is_empty() {
        return;
    }
    let (w, h) = (reg.width, reg.height);
    let area = reg.bounds.inflate(1).intersect(&Rect::new(0, 0, w, h));
    let src = reg.read(area.inflate(1));
    let sw = area.w + 2;
    let at = |x: i32, y: i32| -> u32 {
        // Replicate the canvas edge: outside the canvas reads as the
        // nearest canvas pixel.
        let cx = x.clamp(0, w - 1);
        let cy = y.clamp(0, h - 1);
        src[((cy - area.y + 1) * sw + (cx - area.x + 1)) as usize] as u32
    };
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let c = at(x, y);
            if at(x - 1, y) == c && at(x + 1, y) == c && at(x, y - 1) == c && at(x, y + 1) == c {
                continue;
            }
            let sum = 4 * c
                + 2 * (at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1))
                + at(x - 1, y - 1)
                + at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + at(x + 1, y + 1);
            reg.mask[(y * w + x) as usize] = ((sum + 8) / 16) as u8;
        }
    }
    reg.bounds = area;
}

/// Select › Similar: every pixel whose colour lies within `tolerance` of
/// some colour already (at least half) selected, plus the selection itself.
///
/// Selected colours are recorded in a 64³ occupancy grid (4 levels per
/// cell) dilated by the tolerance, so the match is exact to within 3 levels.
pub fn similar(s: &Sampler, sel: &Plane, tolerance: u8, anti_alias: bool) -> Region {
    let (w, h) = (s.width, s.height);
    const N: usize = 64;
    let mut occ = vec![0u8; N * N * N];
    let mut transparent = false;
    let area = if sel.fill()[0] > 0 { Rect::new(0, 0, w, h) } else { sel.content_bounds() };
    let strip = 256;
    let mut y = area.y;
    while y < area.bottom() {
        let rows = strip.min(area.bottom() - y);
        let r = Rect::new(area.x, y, area.w, rows);
        let cov = sel.read_vec(r);
        let px = s.read(r);
        for (c, p) in cov.iter().zip(px.chunks_exact(4)) {
            if *c >= 128 {
                if p[3] == 0 {
                    transparent = true;
                } else {
                    occ[((p[0] as usize >> 2) * N + (p[1] as usize >> 2)) * N + (p[2] as usize >> 2)] = 1;
                }
            }
        }
        y += rows;
    }
    let rb = (tolerance as usize).div_ceil(4);
    dilate3(&mut occ, N, rb);

    let mut reg = Region::new(w, h);
    let mut y = 0;
    while y < h {
        let rows = strip.min(h - y);
        let r = Rect::new(0, y, w, rows);
        let px = s.read(r);
        let dst = &mut reg.mask[(y * w) as usize..((y + rows) * w) as usize];
        for (d, p) in dst.iter_mut().zip(px.chunks_exact(4)) {
            let hit = if p[3] == 0 { transparent } else { occ[((p[0] as usize >> 2) * N + (p[1] as usize >> 2)) * N + (p[2] as usize >> 2)] != 0 };
            if hit {
                *d = 255;
            }
        }
        y += rows;
    }
    reg.update_bounds();
    if anti_alias {
        antialias(&mut reg);
    }
    // Keep everything that was selected, including soft edges.
    let orig_area = area.intersect(&reg.canvas());
    if !orig_area.is_empty() {
        let cov = sel.read_vec(orig_area);
        for yy in 0..orig_area.h {
            for xx in 0..orig_area.w {
                let i = ((orig_area.y + yy) * w + orig_area.x + xx) as usize;
                reg.mask[i] = reg.mask[i].max(cov[(yy * orig_area.w + xx) as usize]);
            }
        }
        reg.bounds = reg.bounds.union(&orig_area);
    }
    reg
}

impl Region {
    fn canvas(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }
}

/// Chebyshev dilation of an `n`³ occupancy grid by `r` cells, one axis at a
/// time with running counts.
fn dilate3(grid: &mut [u8], n: usize, r: usize) {
    if r == 0 {
        return;
    }
    let mut line = vec![0u8; n];
    let mut out = vec![0u8; n];
    for axis in 0..3 {
        let stride = [n * n, n, 1][axis];
        for a in 0..n {
            for b in 0..n {
                let base = match axis {
                    0 => a * n + b,
                    1 => a * n * n + b,
                    _ => (a * n + b) * n,
                };
                for (i, v) in line.iter_mut().enumerate() {
                    *v = grid[base + i * stride];
                }
                let mut prefix = vec![0u32; n + 1];
                for i in 0..n {
                    prefix[i + 1] = prefix[i] + line[i] as u32;
                }
                for (i, o) in out.iter_mut().enumerate() {
                    let lo = i.saturating_sub(r);
                    let hi = (i + r + 1).min(n);
                    *o = (prefix[hi] > prefix[lo]) as u8;
                }
                for (i, v) in out.iter().enumerate() {
                    grid[base + i * stride] = *v;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::document::Document;
    use editor_core::layer::{Layer, LayerKind, Raster};

    fn doc_from(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Document {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&f(x, y));
            }
        }
        let mut doc = Document::new(w, h);
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "px", LayerKind::Pixel(Raster::new(Plane::from_raw(w, h, 4, &data, [0; 4]), 0, 0))));
        doc.active = Some(id);
        doc
    }

    #[test]
    fn tolerance_boundary_is_inclusive() {
        // Columns 0..10 hold grey 100 + x; seed at x = 0.
        let doc = doc_from(10, 4, |x, _| [100 + x as u8, 100, 100, 255]);
        let mut s = Sampler::new(&doc, doc.active, false);
        let seed = seed_color(&mut s, 0, 0, 1);
        let reg = contiguous(&mut s, 0, 0, seed, 5, false);
        for x in 0..10 {
            assert_eq!(reg.get(x, 2) == 255, x <= 5, "x={x}");
        }
        assert_eq!(reg.bounds, Rect::new(0, 0, 6, 4));
        let g = global(&s, seed, 5);
        assert_eq!(g.bounds, Rect::new(0, 0, 6, 4));
    }

    #[test]
    fn contiguous_stops_at_barrier_global_does_not() {
        // A vertical black line at x = 5 splits two white halves.
        let doc = doc_from(11, 6, |x, _| if x == 5 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
        let mut s = Sampler::new(&doc, doc.active, false);
        let reg = contiguous(&mut s, 1, 1, [255, 255, 255, 255], 10, true);
        assert_eq!(reg.get(8, 3), 0);
        assert_eq!(reg.get(4, 3), 255);
        let g = global(&s, [255, 255, 255, 255], 10);
        assert_eq!(g.get(8, 3), 255);
        assert_eq!(g.get(5, 3), 0);
    }

    #[test]
    fn diagonal_gap_depends_on_connectivity() {
        // Checkerboard: white cells touch only diagonally.
        let doc = doc_from(6, 6, |x, y| if (x + y) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
        let mut s = Sampler::new(&doc, doc.active, false);
        let four = contiguous(&mut s, 0, 0, [255; 4], 0, false);
        assert_eq!(four.mask.iter().filter(|&&v| v == 255).count(), 1);
        let eight = contiguous(&mut s, 0, 0, [255; 4], 0, true);
        assert_eq!(eight.mask.iter().filter(|&&v| v == 255).count(), 18);
    }

    #[test]
    fn transparent_pixels_match_regardless_of_rgb() {
        let doc = doc_from(4, 1, |x, _| [x as u8 * 60, 0, 0, 0]);
        let mut s = Sampler::new(&doc, doc.active, false);
        let reg = contiguous(&mut s, 0, 0, [0, 0, 0, 0], 0, false);
        assert_eq!(reg.bounds.w, 4);
    }

    #[test]
    fn antialias_touches_only_the_edge() {
        let mut reg = Region::new(20, 20);
        for y in 5..15 {
            for x in 5..15 {
                reg.mask[(y * 20 + x) as usize] = 255;
            }
        }
        reg.bounds = Rect::new(5, 5, 10, 10);
        antialias(&mut reg);
        assert_eq!(reg.get(10, 10), 255);
        assert_eq!(reg.get(0, 0), 0);
        let inner = reg.get(5, 10);
        let outer = reg.get(4, 10);
        assert!(inner > 128 && inner < 255, "{inner}");
        assert!(outer > 0 && outer < 128, "{outer}");
    }

    #[test]
    fn similar_adds_matching_colours_elsewhere() {
        // Red squares at both ends, blue between.
        let doc = doc_from(30, 5, |x, _| if !(10..20).contains(&x) { [200, 10, 10, 255] } else { [10, 10, 200, 255] });
        let mut sel = Plane::mask(30, 5, 0);
        sel.write(Rect::new(0, 0, 3, 5), &[255; 15]);
        let s = Sampler::new(&doc, doc.active, false);
        let reg = similar(&s, &sel, 8, false);
        assert_eq!(reg.get(25, 2), 255);
        assert_eq!(reg.get(15, 2), 0);
        assert_eq!(reg.get(1, 2), 255);
    }
}
