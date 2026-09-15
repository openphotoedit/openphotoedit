//! Grow, shrink, border and smooth, built on an exact Euclidean distance
//! transform (Felzenszwalb & Huttenlocher) of the half-selected pixels.
//!
//! Grow by `n` fully selects every pixel whose centre lies within `n` of a
//! selected pixel's centre; shrink by `n` keeps only pixels farther than `n`
//! from an unselected one. The next pixel out gets partial coverage by how
//! far past `n` it lies, so diagonals stay smooth. Shrink treats the canvas
//! edge as selected (Photoshop's default).

use editor_core::adjust::box_blur_1ch;
use editor_core::geom::Rect;
use editor_core::plane::Plane;

const BIG: f64 = 1e20;

/// The area a selection's pixels occupy: its content bounds, or the whole
/// canvas when the plane's fill is selected.
pub fn selection_area(sel: &Plane) -> Rect {
    if sel.fill()[0] > 0 {
        sel.bounds()
    } else {
        sel.content_bounds()
    }
}

fn dt1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f64;
        let mut s = (fq - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * (q as f64 - v[k] as f64));
        while s <= z[k] {
            k -= 1;
            s = (fq - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * (q as f64 - v[k] as f64));
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let dx = q as f64 - v[k] as f64;
        *dq = dx * dx + f[v[k]];
    }
}

/// Squared distance from each pixel to the nearest feature pixel. Values at
/// or above `cap` are reported as `cap` (they are exact below it).
pub fn edt_sq(feature: &[bool], w: usize, h: usize, cap: f32) -> Vec<f32> {
    let mut grid: Vec<f32> = feature.iter().map(|&f| if f { 0.0 } else { cap }).collect();
    let n = w.max(h);
    let mut f = vec![0f64; n];
    let mut d = vec![0f64; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    // Columns.
    for x in 0..w {
        for y in 0..h {
            let g = grid[y * w + x];
            f[y] = if g >= cap { BIG } else { g as f64 };
        }
        dt1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y].min(cap as f64) as f32;
        }
    }
    // Rows.
    for y in 0..h {
        for x in 0..w {
            let g = grid[y * w + x];
            f[x] = if g >= cap { BIG } else { g as f64 };
        }
        dt1d(&f[..w], &mut d[..w], &mut v, &mut z);
        for x in 0..w {
            grid[y * w + x] = d[x].min(cap as f64) as f32;
        }
    }
    grid
}

/// Expand the selection by `by` pixels.
pub fn grow(sel: &Plane, by: f32) -> Plane {
    let canvas = sel.bounds();
    let area = selection_area(sel);
    if by <= 0.0 || area.is_empty() {
        return sel.clone();
    }
    let reach = by.ceil() as i32 + 2;
    let a = area.inflate(reach).intersect(&canvas);
    let cov = sel.read_vec(a);
    let feature: Vec<bool> = cov.iter().map(|&c| c >= 128).collect();
    if !feature.iter().any(|&f| f) {
        return sel.clone();
    }
    let cap = (by + 2.0) * (by + 2.0);
    let d2 = edt_sq(&feature, a.w as usize, a.h as usize, cap);
    let out: Vec<u8> = cov
        .iter()
        .zip(d2.iter())
        .map(|(&c, &d)| {
            let g = (by + 1.0 - d.sqrt()).clamp(0.0, 1.0);
            c.max((g * 255.0).round() as u8)
        })
        .collect();
    let mut p = sel.clone();
    p.write(a, &out);
    p.compact();
    p
}

/// Contract the selection by `by` pixels.
pub fn shrink(sel: &Plane, by: f32) -> Plane {
    let canvas = sel.bounds();
    let area = selection_area(sel);
    if by <= 0.0 || area.is_empty() {
        return sel.clone();
    }
    let a = area.inflate(1).intersect(&canvas);
    let cov = sel.read_vec(a);
    let feature: Vec<bool> = cov.iter().map(|&c| c < 128).collect();
    let cap = (by + 2.0) * (by + 2.0);
    let d2 = edt_sq(&feature, a.w as usize, a.h as usize, cap);
    let out: Vec<u8> = cov
        .iter()
        .zip(d2.iter())
        .map(|(&c, &d)| {
            let k = (d.sqrt() - by).clamp(0.0, 1.0);
            c.min((k * 255.0).round() as u8)
        })
        .collect();
    let mut p = sel.clone();
    p.write(a, &out);
    p.compact();
    p
}

/// A band `width` pixels wide centred on the selection edge.
pub fn border(sel: &Plane, width: f32) -> Plane {
    let half = (width / 2.0).max(0.5);
    let outer = grow(sel, half);
    let inner = shrink(sel, half);
    let area = selection_area(&outer);
    let mut p = Plane::mask(sel.width(), sel.height(), 0);
    if area.is_empty() {
        return p;
    }
    let o = outer.read_vec(area);
    let i = inner.read_vec(area);
    let mut band: Vec<f32> = o.iter().zip(i.iter()).map(|(&a, &b)| a as f32 * (255 - b) as f32 / 255.0).collect();
    // Photoshop's border is soft-edged; a one-pixel blur takes the stairs out.
    box_blur_1ch(&mut band, area.w as usize, area.h as usize, 1, 1);
    let out: Vec<u8> = band.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    p.write(area, &out);
    p.compact();
    p
}

/// Majority smoothing: a pixel is selected when more than half of the
/// `(2r+1)²` square around it is. The result keeps a one-pixel soft edge.
pub fn smooth(sel: &Plane, radius: u32) -> Plane {
    let canvas = sel.bounds();
    let area = selection_area(sel);
    if radius == 0 || area.is_empty() {
        return sel.clone();
    }
    let r = radius as i32;
    let a = area.inflate(2 * r + 2).intersect(&canvas);
    let (w, h) = (a.w as usize, a.h as usize);
    let cov = sel.read_vec(a);
    // Summed-area table of the binary mask.
    let mut sat = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut run = 0u32;
        for x in 0..w {
            run += (cov[y * w + x] >= 128) as u32;
            sat[(y + 1) * (w + 1) + x + 1] = sat[y * (w + 1) + x + 1] + run;
        }
    }
    let side = (2 * r + 1) as f32;
    let mut out = vec![0u8; w * h];
    for y in 0..h as i32 {
        let y0 = (y - r).max(0) as usize;
        let y1 = ((y + r + 1) as usize).min(h);
        // Rows beyond the canvas count as absent, rows beyond the area
        // (but inside the canvas) are unselected.
        let cy0 = (a.y + y - r).max(0);
        let cy1 = (a.y + y + r + 1).min(canvas.h);
        for x in 0..w as i32 {
            let x0 = (x - r).max(0) as usize;
            let x1 = ((x + r + 1) as usize).min(w);
            let s = sat[y1 * (w + 1) + x1] + sat[y0 * (w + 1) + x0] - sat[y0 * (w + 1) + x1] - sat[y1 * (w + 1) + x0];
            let cx0 = (a.x + x - r).max(0);
            let cx1 = (a.x + x + r + 1).min(canvas.w);
            let n = ((cx1 - cx0) * (cy1 - cy0)) as f32;
            let frac = s as f32 / n;
            let v = ((frac - 0.5) * side + 0.5).clamp(0.0, 1.0);
            out[y as usize * w + x as usize] = (v * 255.0).round() as u8;
        }
    }
    let mut p = sel.clone();
    p.write(a, &out);
    p.compact();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_sel(w: u32, h: u32, r: Rect) -> Plane {
        let mut p = Plane::mask(w, h, 0);
        p.write(r, &vec![255; r.area() as usize]);
        p
    }

    fn brute_d2(feature: &[bool], w: usize, h: usize, x: usize, y: usize) -> f32 {
        let mut best = f32::MAX;
        for yy in 0..h {
            for xx in 0..w {
                if feature[yy * w + xx] {
                    let dx = xx as f32 - x as f32;
                    let dy = yy as f32 - y as f32;
                    best = best.min(dx * dx + dy * dy);
                }
            }
        }
        best
    }

    #[test]
    fn edt_matches_brute_force() {
        let (w, h) = (37, 23);
        let feature: Vec<bool> = (0..w * h).map(|i| (i * 7919 + i / 3) % 53 == 0).collect();
        let d = edt_sq(&feature, w, h, 1e6);
        for y in 0..h {
            for x in 0..w {
                assert_eq!(d[y * w + x], brute_d2(&feature, w, h, x, y), "({x},{y})");
            }
        }
    }

    #[test]
    fn grow_is_exact() {
        let sel = rect_sel(60, 60, Rect::new(20, 25, 10, 6));
        let by = 5.0f32;
        let g = grow(&sel, by);
        let feature: Vec<bool> = sel.to_raw().iter().map(|&v| v >= 128).collect();
        for y in 0..60 {
            for x in 0..60 {
                let d = brute_d2(&feature, 60, 60, x, y).sqrt();
                let v = g.get(x as i32, y as i32)[0];
                if d <= by {
                    assert_eq!(v, 255, "({x},{y}) d={d}");
                } else if d >= by + 1.0 {
                    assert_eq!(v, 0, "({x},{y}) d={d}");
                } else {
                    assert!(v > 0 && v < 255, "({x},{y}) d={d} v={v}");
                }
            }
        }
        // Axis-aligned extent is exactly `by` further out on every side.
        assert_eq!(g.content_bounds(), Rect::new(15, 20, 20, 16));
    }

    #[test]
    fn shrink_is_exact_and_respects_canvas_edge() {
        let sel = rect_sel(40, 40, Rect::new(0, 10, 20, 20));
        let s = shrink(&sel, 3.0);
        // Left edge touches the canvas: no shrink there.
        assert_eq!(s.get(0, 20)[0], 255);
        // Right edge: pixels 17,18,19 go (distances 3,2,1 to x=20), 16 stays.
        assert_eq!(s.get(16, 20)[0], 255);
        assert_eq!(s.get(17, 20)[0], 0);
        assert_eq!(s.get(10, 13)[0], 255);
        assert_eq!(s.get(10, 12)[0], 0);
        assert_eq!(s.content_bounds(), Rect::new(0, 13, 17, 14));
        // Select all: nothing changes.
        let all = Plane::mask(40, 40, 255);
        assert_eq!(shrink(&all, 4.0).get(0, 0)[0], 255);
    }

    #[test]
    fn grow_then_shrink_restores_a_convex_rect() {
        let sel = rect_sel(80, 80, Rect::new(30, 30, 20, 20));
        let back = shrink(&grow(&sel, 4.0), 4.0);
        // Corners round off under a Euclidean grow; the edges come back exactly.
        assert_eq!(back.get(40, 30)[0], 255);
        assert_eq!(back.get(40, 29)[0], 0);
        assert_eq!(back.get(29, 40)[0], 0);
    }

    #[test]
    fn border_is_a_band_on_the_edge() {
        let sel = rect_sel(80, 80, Rect::new(20, 20, 40, 40));
        let b = border(&sel, 8.0);
        assert!(b.get(20, 40)[0] > 200, "on the edge");
        assert_eq!(b.get(40, 40)[0], 0, "centre");
        assert_eq!(b.get(5, 40)[0], 0, "far outside");
    }

    #[test]
    fn smooth_removes_specks_and_fills_pinholes() {
        let mut sel = rect_sel(50, 50, Rect::new(10, 10, 30, 30));
        sel.write(Rect::new(25, 25, 1, 1), &[0]);
        sel.write(Rect::new(3, 3, 1, 1), &[255]);
        let s = smooth(&sel, 2);
        assert_eq!(s.get(25, 25)[0], 255);
        assert_eq!(s.get(3, 3)[0], 0);
        assert_eq!(s.get(20, 20)[0], 255);
    }
}
