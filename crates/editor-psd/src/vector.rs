//! Vector masks (`vmsk` / `vsms`): Bézier path records rasterised into an
//! anti-aliased single-channel mask, with Photoshop's shape operations.

use editor_core::geom::Rect;
use editor_core::layer::Raster;
use editor_core::plane::Plane;

use crate::io::Reader;

#[derive(Clone, Debug)]
struct Subpath {
    closed: bool,
    op: i16,
    /// (preceding, anchor, leaving) as (x, y) in document pixels.
    knots: Vec<[(f64, f64); 3]>,
}

pub struct VectorMask {
    pub invert: bool,
    pub disabled: bool,
    initial_fill: bool,
    paths: Vec<Subpath>,
}

pub fn parse(data: &[u8], doc_w: u32, doc_h: u32) -> Option<VectorMask> {
    let mut r = Reader::new(data);
    let _version = r.u32().ok()?;
    let flags = r.u32().ok()?;
    let mut vm = VectorMask { invert: flags & 1 != 0, disabled: flags & 4 != 0, initial_fill: false, paths: Vec::new() };
    let fix = |v: i32| v as f64 / 16_777_216.0;
    while r.remaining() >= 26 {
        let sel = r.u16().ok()?;
        let rec = r.bytes(24, "path record").ok()?;
        let mut q = Reader::new(rec);
        match sel {
            0 | 3 => {
                let _len = q.u16().ok()?;
                let op = q.i16().ok()?;
                vm.paths.push(Subpath { closed: sel == 0, op, knots: Vec::new() });
            }
            1 | 2 | 4 | 5 => {
                let mut pts = [(0.0, 0.0); 3];
                for p in pts.iter_mut() {
                    let y = fix(q.i32().ok()?);
                    let x = fix(q.i32().ok()?);
                    *p = (x * doc_w as f64, y * doc_h as f64);
                }
                if let Some(sp) = vm.paths.last_mut() {
                    sp.knots.push(pts);
                }
            }
            8 => vm.initial_fill = q.u16().ok()? != 0,
            _ => {}
        }
    }
    Some(vm)
}

fn flatten(sp: &Subpath) -> Vec<(f64, f64)> {
    let n = sp.knots.len();
    let mut pts = Vec::new();
    if n == 0 {
        return pts;
    }
    pts.push(sp.knots[0][1]);
    let segs = if sp.closed { n } else { n - 1 };
    for i in 0..segs {
        let a = sp.knots[i];
        let b = sp.knots[(i + 1) % n];
        let (p0, p1, p2, p3) = (a[1], a[2], b[0], b[1]);
        let len = ((p1.0 - p0.0).hypot(p1.1 - p0.1)) + ((p2.0 - p1.0).hypot(p2.1 - p1.1)) + ((p3.0 - p2.0).hypot(p3.1 - p2.1));
        let steps = ((len / 3.0).ceil() as usize).clamp(1, 512);
        let straight = (p1 == p0 || p1 == p3) && (p2 == p3 || p2 == p0);
        let steps = if straight { 1 } else { steps };
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            let u = 1.0 - t;
            let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
            let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
            pts.push((x, y));
        }
    }
    pts
}

/// Non-zero coverage of closed polygons over `w`×`h` (5 sub-scanlines per
/// row, exact horizontal coverage).
fn rasterize(polys: &[Vec<(f64, f64)>], w: usize, h: usize) -> Vec<f32> {
    const SUB: usize = 5;
    let mut out = vec![0f32; w * h];
    let mut edges: Vec<(f64, f64, f64, f64, i32)> = Vec::new();
    for poly in polys {
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if a.1 == b.1 {
                continue;
            }
            let dir = if b.1 > a.1 { 1 } else { -1 };
            let (top, bot) = if a.1 < b.1 { (a, b) } else { (b, a) };
            edges.push((top.0, top.1, bot.0, bot.1, dir));
        }
    }
    if edges.is_empty() {
        return out;
    }
    let ymin = edges.iter().map(|e| e.1).fold(f64::INFINITY, f64::min).max(0.0);
    let ymax = edges.iter().map(|e| e.3).fold(f64::NEG_INFINITY, f64::max).min(h as f64);
    let mut cross: Vec<(f64, i32)> = Vec::new();
    let mut row = vec![0f32; w + 1];
    let y0 = ymin.floor() as usize;
    let y1 = (ymax.ceil() as usize).min(h);
    for py in y0..y1 {
        row.iter_mut().for_each(|v| *v = 0.0);
        for s in 0..SUB {
            let sy = py as f64 + (s as f64 + 0.5) / SUB as f64;
            cross.clear();
            for e in &edges {
                if sy >= e.1 && sy < e.3 {
                    let x = e.0 + (sy - e.1) / (e.3 - e.1) * (e.2 - e.0);
                    cross.push((x, e.4));
                }
            }
            cross.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0;
            for i in 0..cross.len() {
                let before = wind;
                wind += cross[i].1;
                if before == 0 && wind != 0 {
                    // Span starts here; find where it ends.
                    let mut w2 = wind;
                    let mut j = i + 1;
                    while j < cross.len() {
                        w2 += cross[j].1;
                        if w2 == 0 {
                            break;
                        }
                        j += 1;
                    }
                    let xa = cross[i].0.clamp(0.0, w as f64);
                    let xb = cross.get(j).map_or(w as f64, |c| c.0).clamp(0.0, w as f64);
                    span(&mut row, xa, xb, 1.0 / SUB as f32);
                }
            }
        }
        for x in 0..w {
            out[py * w + x] = row[x].min(1.0);
        }
    }
    out
}

fn span(row: &mut [f32], xa: f64, xb: f64, weight: f32) {
    if xb <= xa {
        return;
    }
    let ia = xa.floor() as usize;
    let ib = xb.floor() as usize;
    if ia == ib {
        if ia < row.len() {
            row[ia] += (xb - xa) as f32 * weight;
        }
        return;
    }
    row[ia] += (1.0 - (xa - ia as f64)) as f32 * weight;
    for v in row.iter_mut().take(ib).skip(ia + 1) {
        *v += weight;
    }
    if ib < row.len() {
        row[ib] += (xb - ib as f64) as f32 * weight;
    }
}

impl VectorMask {
    /// The mask over the whole document.
    pub fn rasterize(&self, doc_w: u32, doc_h: u32) -> Raster {
        let (w, h) = (doc_w as usize, doc_h as usize);
        let mut mask = vec![if self.initial_fill && self.paths.is_empty() { 1f32 } else { 0.0 }; w * h];
        let mut groups: Vec<Vec<&Subpath>> = Vec::new();
        for sp in &self.paths {
            if sp.op == -1 && !groups.is_empty() {
                groups.last_mut().unwrap().push(sp);
            } else {
                groups.push(vec![sp]);
            }
        }
        let mut first = true;
        for g in groups {
            let polys: Vec<Vec<(f64, f64)>> = g.iter().filter(|s| s.knots.len() > 1).map(|s| flatten(s)).collect();
            let plane = rasterize(&polys, w, h);
            let op = g[0].op;
            match op {
                0 => mask.iter_mut().zip(&plane).for_each(|(m, p)| *m = *m + p - 2.0 * *m * p),
                2 => {
                    if first {
                        mask.iter_mut().for_each(|m| *m = 1.0 - *m);
                    }
                    mask.iter_mut().zip(&plane).for_each(|(m, p)| *m = (*m - p).max(0.0));
                }
                3 => {
                    if first {
                        mask.iter_mut().for_each(|m| *m = 1.0 - *m);
                    }
                    mask.iter_mut().zip(&plane).for_each(|(m, p)| *m *= p);
                }
                _ => mask.iter_mut().zip(&plane).for_each(|(m, p)| *m = *m + p - *m * p),
            }
            first = false;
        }
        let bytes: Vec<u8> = mask.iter().map(|&m| {
            let v = if self.invert { 1.0 - m } else { m };
            (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        }).collect();
        let mut plane = Plane::mask(doc_w, doc_h, 0);
        plane.write(Rect::new(0, 0, doc_w as i32, doc_h as i32), &bytes);
        plane.compact();
        Raster::new(plane, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Writer;

    #[test]
    fn rectangle_path() {
        let mut w = Writer::new();
        w.u32(3);
        w.u32(0);
        let fix = |v: f64| (v * 16_777_216.0) as i32;
        w.u16(0);
        w.u16(4);
        w.i16(1);
        w.zeros(20);
        for (x, y) in [(0.25, 0.25), (0.75, 0.25), (0.75, 0.75), (0.25, 0.75)] {
            w.u16(1);
            for _ in 0..3 {
                w.i32(fix(y));
                w.i32(fix(x));
            }
        }
        let vm = parse(&w.buf, 20, 20).unwrap();
        let r = vm.rasterize(20, 20);
        assert_eq!(r.plane.get(10, 10)[0], 255);
        assert_eq!(r.plane.get(2, 2)[0], 0);
        assert_eq!(r.plane.get(5, 10)[0], 255);
        assert_eq!(r.plane.get(4, 10)[0], 0);
    }
}
