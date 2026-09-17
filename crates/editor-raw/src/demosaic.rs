//! Demosaicing.
//!
//! - Bayer: RCD (Ratio Corrected Demosaicing, Luis Sanz Rodríguez, v2.3),
//!   written from the published algorithm description. It is the default in
//!   RawTherapee and darktable for good reason: sharp, very few zippers or
//!   mazes, and cheap.
//! - X-Trans: Markesteijn 1-pass from rawler.
//! - Preview: superpixel binning (2×2 Bayer, 3×3 X-Trans), no interpolation.
//!
//! Input samples are expected white balanced and scaled into 0..=1.

use rayon::prelude::*;

use crate::decode::{Decoded, Layout};
use crate::Rgb;

const EPS: f32 = 1e-5;
const EPSSQ: f32 = 1e-10;
/// Rows/columns at each edge that RCD cannot reach and bilinear fills.
const RCD_BORDER: usize = 4;

/// Full-resolution demosaic of `dec` with white balance `wb` applied first.
/// Channel `c` of the result clips at `wb[c]` (sensor white × multiplier).
pub fn full(dec: &Decoded, wb: [f32; 3]) -> Rgb {
    let (w, h) = (dec.width, dec.height);
    let scale = wb.iter().cloned().fold(1.0f32, f32::max);
    let norm = [wb[0] / scale, wb[1] / scale, wb[2] / scale];
    let mut out = match &dec.layout {
        Layout::Rgb => {
            let mut d = dec.data.clone();
            d.par_chunks_mut(3).for_each(|p| (0..3).for_each(|c| p[c] *= wb[c]));
            return Rgb { w, h, data: d };
        }
        Layout::Mono => {
            let data = dec.data.iter().flat_map(|&v| [v, v, v]).collect();
            return Rgb { w, h, data };
        }
        layout => {
            let mut cfa = dec.data.clone();
            cfa.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, v) in row.iter_mut().enumerate() {
                    *v *= norm[layout.color_at(y, x)];
                }
            });
            match layout {
                Layout::Bayer(p) => rcd(&cfa, w, h, *p),
                Layout::XTrans(p) => markesteijn(cfa, w, h, p),
                _ => unreachable!(),
            }
        }
    };
    out.data.par_chunks_mut(3).for_each(|p| {
        for c in 0..3 {
            p[c] = (p[c] * scale).clamp(0.0, wb[c]);
        }
    });
    out
}

/// Binned half-size (Bayer) or third-size (X-Trans) camera RGB, not white
/// balanced. A channel is set to exactly 1.0 when any sample behind it clipped.
pub fn superpixel(dec: &Decoded) -> Rgb {
    let (w, h) = (dec.width, dec.height);
    let (n, cpp) = match dec.layout {
        Layout::XTrans(_) => (3, 1),
        Layout::Rgb => (2, 3),
        _ => (2, 1),
    };
    let (ow, oh) = ((w / n).max(1), (h / n).max(1));
    let mut data = vec![0f32; ow * oh * 3];
    data.par_chunks_mut(ow * 3).enumerate().for_each(|(oy, row)| {
        for ox in 0..ow {
            let mut sum = [0f32; 3];
            let mut cnt = [0u32; 3];
            let mut clip = [false; 3];
            for dy in 0..n {
                let y = (oy * n + dy).min(h - 1);
                for dx in 0..n {
                    let x = (ox * n + dx).min(w - 1);
                    let i = (y * w + x) * cpp;
                    match dec.layout {
                        Layout::Rgb => {
                            for c in 0..3 {
                                sum[c] += dec.data[i + c];
                                cnt[c] += 1;
                            }
                        }
                        Layout::Mono => {
                            let v = dec.data[i];
                            for c in 0..3 {
                                sum[c] += v;
                                cnt[c] += 1;
                                clip[c] |= v >= 0.999;
                            }
                        }
                        ref l => {
                            let c = l.color_at(y, x);
                            let v = dec.data[i];
                            sum[c] += v;
                            cnt[c] += 1;
                            clip[c] |= v >= 0.999;
                        }
                    }
                }
            }
            for c in 0..3 {
                row[ox * 3 + c] = if clip[c] { 1.0 } else { sum[c] / cnt[c].max(1) as f32 };
            }
        }
    });
    Rgb { w: ow, h: oh, data }
}

/// Bilinear fill of every pixel within `border` of an edge, from the
/// same-colour samples in its 5×5 neighbourhood.
fn fill_border(cfa: &[f32], w: usize, h: usize, color_at: &(dyn Fn(usize, usize) -> usize + Sync), border: usize, planes: &mut [Vec<f32>; 3]) {
    let fill = |y: usize, x: usize, planes: &mut [Vec<f32>; 3]| {
        let mut sum = [0f32; 3];
        let mut cnt = [0u32; 3];
        for yy in y.saturating_sub(2)..(y + 3).min(h) {
            for xx in x.saturating_sub(2)..(x + 3).min(w) {
                let c = color_at(yy, xx);
                sum[c] += cfa[yy * w + xx];
                cnt[c] += 1;
            }
        }
        let own = color_at(y, x);
        for c in 0..3 {
            planes[c][y * w + x] = if c == own { cfa[y * w + x] } else if cnt[c] > 0 { sum[c] / cnt[c] as f32 } else { 0.0 };
        }
    };
    for y in 0..h {
        if y < border || y + border >= h {
            for x in 0..w {
                fill(y, x, planes);
            }
        } else {
            for x in (0..border.min(w)).chain(w.saturating_sub(border).max(border)..w) {
                fill(y, x, planes);
            }
        }
    }
}

fn interleave(planes: [Vec<f32>; 3], w: usize, h: usize) -> Rgb {
    let mut data = vec![0f32; w * h * 3];
    data.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
        p[0] = planes[0][i];
        p[1] = planes[1][i];
        p[2] = planes[2][i];
    });
    Rgb { w, h, data }
}

#[inline]
fn sqr(v: f32) -> f32 {
    v * v
}

/// Compute `f(y, x)` for the given rows into a fresh plane, in parallel.
fn par_plane(w: usize, h: usize, rows: std::ops::Range<usize>, f: impl Fn(usize, usize) -> f32 + Sync) -> Vec<f32> {
    let mut out = vec![0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        if rows.contains(&y) {
            for (x, v) in row.iter_mut().enumerate() {
                *v = f(y, x);
            }
        }
    });
    out
}

/// RCD demosaic of a Bayer mosaic. `pattern[y & 1][x & 1]` is 0 R, 1 G, 2 B.
pub fn rcd(cfa: &[f32], w: usize, h: usize, pattern: [[u8; 2]; 2]) -> Rgb {
    let fc = |y: usize, x: usize| pattern[y & 1][x & 1] as usize;
    let mut planes: [Vec<f32>; 3] = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    for (i, &v) in cfa.iter().enumerate() {
        planes[fc(i / w, i % w)][i] = v;
    }
    if w < 2 * RCD_BORDER + 2 || h < 2 * RCD_BORDER + 2 {
        fill_border(cfa, w, h, &fc, w.max(h), &mut planes);
        return interleave(planes, w, h);
    }
    // Borders first: the inner steps read neighbours that lie in the border.
    fill_border(cfa, w, h, &fc, RCD_BORDER, &mut planes);
    let (w1, w2, w3, w4) = (w, 2 * w, 3 * w, 4 * w);
    let inner = |y: usize, x: usize| y >= RCD_BORDER && y < h - RCD_BORDER && x >= RCD_BORDER && x < w - RCD_BORDER;

    // Step 1: vertical/horizontal discrimination.
    let hpf_v = par_plane(w, h, 3..h - 3, |y, x| {
        if x < 3 || x >= w - 3 {
            return 0.0;
        }
        let i = y * w + x;
        sqr((cfa[i - w3] - cfa[i - w1] - cfa[i + w1] + cfa[i + w3]) - 3.0 * (cfa[i - w2] + cfa[i + w2]) + 6.0 * cfa[i])
    });
    let hpf_h = par_plane(w, h, 3..h - 3, |y, x| {
        if x < 3 || x >= w - 3 {
            return 0.0;
        }
        let i = y * w + x;
        sqr((cfa[i - 3] - cfa[i - 1] - cfa[i + 1] + cfa[i + 3]) - 3.0 * (cfa[i - 2] + cfa[i + 2]) + 6.0 * cfa[i])
    });
    let vh_dir = par_plane(w, h, 1..h - 1, |y, x| {
        if x < 1 || x >= w - 1 {
            return 0.5;
        }
        let i = y * w + x;
        let v = (hpf_v[i - w1] + hpf_v[i] + hpf_v[i + w1]).max(EPSSQ);
        let hh = (hpf_h[i - 1] + hpf_h[i] + hpf_h[i + 1]).max(EPSSQ);
        v / (v + hh)
    });
    drop(hpf_v);
    drop(hpf_h);

    // Step 2: low-pass at red and blue sites.
    let lpf = par_plane(w, h, 2..h - 2, |y, x| {
        if x < 2 || x >= w - 2 || fc(y, x) == 1 {
            return 0.0;
        }
        let i = y * w + x;
        0.25 * cfa[i] + 0.125 * (cfa[i - w1] + cfa[i + w1] + cfa[i - 1] + cfa[i + 1]) + 0.0625 * (cfa[i - w1 - 1] + cfa[i - w1 + 1] + cfa[i + w1 - 1] + cfa[i + w1 + 1])
    });

    let disc = |dir: &[f32], i: usize| {
        let c = dir[i];
        let n = 0.25 * (dir[i - w1 - 1] + dir[i - w1 + 1] + dir[i + w1 - 1] + dir[i + w1 + 1]);
        if (0.5 - c).abs() < (0.5 - n).abs() {
            n
        } else {
            c
        }
    };

    // Step 3: green at red and blue sites.
    {
        let green = &mut planes[1];
        green.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, g) in row.iter_mut().enumerate() {
                if !inner(y, x) || fc(y, x) == 1 {
                    continue;
                }
                let i = y * w + x;
                let n_grad = EPS + (cfa[i - w1] - cfa[i + w1]).abs() + (cfa[i] - cfa[i - w2]).abs() + (cfa[i - w1] - cfa[i - w3]).abs() + (cfa[i - w2] - cfa[i - w4]).abs();
                let s_grad = EPS + (cfa[i - w1] - cfa[i + w1]).abs() + (cfa[i] - cfa[i + w2]).abs() + (cfa[i + w1] - cfa[i + w3]).abs() + (cfa[i + w2] - cfa[i + w4]).abs();
                let w_grad = EPS + (cfa[i - 1] - cfa[i + 1]).abs() + (cfa[i] - cfa[i - 2]).abs() + (cfa[i - 1] - cfa[i - 3]).abs() + (cfa[i - 2] - cfa[i - 4]).abs();
                let e_grad = EPS + (cfa[i - 1] - cfa[i + 1]).abs() + (cfa[i] - cfa[i + 2]).abs() + (cfa[i + 1] - cfa[i + 3]).abs() + (cfa[i + 2] - cfa[i + 4]).abs();
                let l = lpf[i];
                let n_est = cfa[i - w1] * (1.0 + (l - lpf[i - w2]) / (EPS + l + lpf[i - w2]));
                let s_est = cfa[i + w1] * (1.0 + (l - lpf[i + w2]) / (EPS + l + lpf[i + w2]));
                let w_est = cfa[i - 1] * (1.0 + (l - lpf[i - 2]) / (EPS + l + lpf[i - 2]));
                let e_est = cfa[i + 1] * (1.0 + (l - lpf[i + 2]) / (EPS + l + lpf[i + 2]));
                let v_est = (s_grad * n_est + n_grad * s_est) / (n_grad + s_grad);
                let h_est = (w_grad * e_est + e_grad * w_est) / (e_grad + w_grad);
                let d = disc(&vh_dir, i);
                *g = (d * h_est + (1.0 - d) * v_est).clamp(0.0, 1.0);
            }
        });
    }
    drop(lpf);

    // Step 4.0/4.1: diagonal discrimination at red and blue sites.
    let hpf_p = par_plane(w, h, 3..h - 3, |y, x| {
        if x < 3 || x >= w - 3 {
            return 0.0;
        }
        let i = y * w + x;
        sqr((cfa[i - w3 - 3] - cfa[i - w1 - 1] - cfa[i + w1 + 1] + cfa[i + w3 + 3]) - 3.0 * (cfa[i - w2 - 2] + cfa[i + w2 + 2]) + 6.0 * cfa[i])
    });
    let hpf_q = par_plane(w, h, 3..h - 3, |y, x| {
        if x < 3 || x >= w - 3 {
            return 0.0;
        }
        let i = y * w + x;
        sqr((cfa[i - w3 + 3] - cfa[i - w1 + 1] - cfa[i + w1 - 1] + cfa[i + w3 - 3]) - 3.0 * (cfa[i - w2 + 2] + cfa[i + w2 - 2]) + 6.0 * cfa[i])
    });
    let pq_dir = par_plane(w, h, 1..h - 1, |y, x| {
        if x < 1 || x >= w - 1 {
            return 0.5;
        }
        let i = y * w + x;
        let p = (hpf_p[i - w1 - 1] + hpf_p[i] + hpf_p[i + w1 + 1]).max(EPSSQ);
        let q = (hpf_q[i - w1 + 1] + hpf_q[i] + hpf_q[i + w1 - 1]).max(EPSSQ);
        p / (p + q)
    });
    drop(hpf_p);
    drop(hpf_q);

    // Step 4.2: red at blue sites and blue at red sites.
    let missing = {
        let (r, g, b) = (&planes[0], &planes[1], &planes[2]);
        par_plane(w, h, RCD_BORDER..h - RCD_BORDER, |y, x| {
            let own = fc(y, x);
            if own == 1 || !inner(y, x) {
                return 0.0;
            }
            let i = y * w + x;
            let pc = if own == 0 { b } else { r };
            let d = disc(&pq_dir, i);
            let nw_grad = EPS + (pc[i - w1 - 1] - pc[i + w1 + 1]).abs() + (pc[i - w1 - 1] - pc[i - w3 - 3]).abs() + (g[i] - g[i - w2 - 2]).abs();
            let ne_grad = EPS + (pc[i - w1 + 1] - pc[i + w1 - 1]).abs() + (pc[i - w1 + 1] - pc[i - w3 + 3]).abs() + (g[i] - g[i - w2 + 2]).abs();
            let sw_grad = EPS + (pc[i + w1 - 1] - pc[i - w1 + 1]).abs() + (pc[i + w1 - 1] - pc[i + w3 - 3]).abs() + (g[i] - g[i + w2 - 2]).abs();
            let se_grad = EPS + (pc[i + w1 + 1] - pc[i - w1 - 1]).abs() + (pc[i + w1 + 1] - pc[i + w3 + 3]).abs() + (g[i] - g[i + w2 + 2]).abs();
            let nw_est = pc[i - w1 - 1] - g[i - w1 - 1];
            let ne_est = pc[i - w1 + 1] - g[i - w1 + 1];
            let sw_est = pc[i + w1 - 1] - g[i + w1 - 1];
            let se_est = pc[i + w1 + 1] - g[i + w1 + 1];
            let p_est = (nw_grad * se_est + se_grad * nw_est) / (nw_grad + se_grad);
            let q_est = (ne_grad * sw_est + sw_grad * ne_est) / (ne_grad + sw_grad);
            (g[i] + (1.0 - d) * p_est + d * q_est).clamp(0.0, 1.0)
        })
    };
    for y in RCD_BORDER..h - RCD_BORDER {
        for x in RCD_BORDER..w - RCD_BORDER {
            match fc(y, x) {
                0 => planes[2][y * w + x] = missing[y * w + x],
                2 => planes[0][y * w + x] = missing[y * w + x],
                _ => {}
            }
        }
    }
    drop(missing);
    drop(pq_dir);

    // Step 4.3: red and blue at green sites.
    let at_green = |c: usize| {
        let (pc, g) = (&planes[c], &planes[1]);
        par_plane(w, h, RCD_BORDER..h - RCD_BORDER, |y, x| {
            if fc(y, x) != 1 || !inner(y, x) {
                return 0.0;
            }
            let i = y * w + x;
            let d = disc(&vh_dir, i);
            let n_grad = EPS + (g[i] - g[i - w2]).abs() + (pc[i - w1] - pc[i + w1]).abs() + (pc[i - w1] - pc[i - w3]).abs();
            let s_grad = EPS + (g[i] - g[i + w2]).abs() + (pc[i + w1] - pc[i - w1]).abs() + (pc[i + w1] - pc[i + w3]).abs();
            let w_grad = EPS + (g[i] - g[i - 2]).abs() + (pc[i - 1] - pc[i + 1]).abs() + (pc[i - 1] - pc[i - 3]).abs();
            let e_grad = EPS + (g[i] - g[i + 2]).abs() + (pc[i + 1] - pc[i - 1]).abs() + (pc[i + 1] - pc[i + 3]).abs();
            let n_est = pc[i - w1] - g[i - w1];
            let s_est = pc[i + w1] - g[i + w1];
            let w_est = pc[i - 1] - g[i - 1];
            let e_est = pc[i + 1] - g[i + 1];
            let v_est = (n_grad * s_est + s_grad * n_est) / (n_grad + s_grad);
            let h_est = (e_grad * w_est + w_grad * e_est) / (e_grad + w_grad);
            (g[i] + (1.0 - d) * v_est + d * h_est).clamp(0.0, 1.0)
        })
    };
    let r_at_g = at_green(0);
    let b_at_g = at_green(2);
    for y in RCD_BORDER..h - RCD_BORDER {
        for x in RCD_BORDER..w - RCD_BORDER {
            if fc(y, x) == 1 {
                planes[0][y * w + x] = r_at_g[y * w + x];
                planes[2][y * w + x] = b_at_g[y * w + x];
            }
        }
    }
    drop(r_at_g);
    drop(b_at_g);

    interleave(planes, w, h)
}

/// X-Trans through rawler's Markesteijn implementation.
fn markesteijn(cfa: Vec<f32>, w: usize, h: usize, pattern: &[[u8; 6]; 6]) -> Rgb {
    use rawler::imgop::sensor::xtrans::markesteijn::XTransMarkesteijnDemosaic;
    use rawler::imgop::sensor::Demosaic;
    use rawler::imgop::{Dim2, Point, Rect};
    let name: String = pattern.iter().flatten().map(|&c| ['R', 'G', 'B'][c as usize]).collect();
    let rcfa = rawler::CFA::new(&name);
    if w < 64 || h < 64 {
        let fc = |y: usize, x: usize| pattern[y % 6][x % 6] as usize;
        let mut planes: [Vec<f32>; 3] = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
        fill_border(&cfa, w, h, &fc, w.max(h), &mut planes);
        return interleave(planes, w, h);
    }
    let pixels = rawler::pixarray::PixF32::new_with(cfa, w, h);
    let colors = rawler::cfa::PlaneColor::new("RGB");
    let out = XTransMarkesteijnDemosaic::new_pass_1().demosaic(&pixels, &rcfa, &colors, Rect::new(Point::new(0, 0), Dim2::new(w, h)));
    Rgb { w, h, data: out.flatten() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGGB: [[u8; 2]; 2] = [[0, 1], [1, 2]];

    fn mosaic(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        (0..w * h).map(|i| f(i / w, i % w)[RGGB[(i / w) & 1][(i % w) & 1] as usize]).collect()
    }

    #[test]
    fn flat_colour_is_reconstructed_exactly() {
        let (w, h) = (32, 24);
        let c = [0.6, 0.35, 0.2];
        let out = rcd(&mosaic(w, h, |_, _| c), w, h, RGGB);
        for p in out.data.chunks_exact(3) {
            for k in 0..3 {
                assert!((p[k] - c[k]).abs() < 1e-3, "{p:?}");
            }
        }
    }

    #[test]
    fn grey_edge_has_no_zippers_or_false_colour() {
        // Vertical and diagonal grey edges, band-limited like a real lens (a
        // ~2 px ramp, not an aliased step). The output must stay monotone
        // across the vertical edge (no zippers) and show far less false
        // colour than bilinear. The two rows next to the bilinear border are
        // skipped: they read border values, and real images crop them.
        let (w, h) = (40, 40);
        for diagonal in [false, true] {
            let f = |y: usize, x: usize| {
                let d = if diagonal { (x as f32 + y as f32 - 40.0) / std::f32::consts::SQRT_2 } else { x as f32 - 20.0 };
                let t = ((d + 1.0) / 2.0).clamp(0.0, 1.0);
                let v = 0.1 + 0.7 * t * t * (3.0 - 2.0 * t);
                [v, v, v]
            };
            let cfa = mosaic(w, h, f);
            let out = rcd(&cfa, w, h, RGGB);
            let fc = |y: usize, x: usize| RGGB[y & 1][x & 1] as usize;
            let mut planes: [Vec<f32>; 3] = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
            fill_border(&cfa, w, h, &fc, w, &mut planes);
            let bilinear = interleave(planes, w, h);
            let spread = |img: &Rgb, x: usize, y: usize| {
                let p = &img.data[(y * w + x) * 3..(y * w + x) * 3 + 3];
                p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
            };
            let (mut worst, mut worst_bl) = (0f32, 0f32);
            for y in RCD_BORDER + 2..h - RCD_BORDER - 2 {
                let mut prev = 0.0;
                for x in RCD_BORDER + 2..w - RCD_BORDER - 2 {
                    worst = worst.max(spread(&out, x, y));
                    worst_bl = worst_bl.max(spread(&bilinear, x, y));
                    let g = out.data[(y * w + x) * 3 + 1];
                    if !diagonal {
                        assert!(g + 0.01 >= prev, "zipper at {x},{y}: {g} after {prev}");
                        prev = g;
                    }
                }
            }
            let limit = if diagonal { 0.1 } else { 0.02 };
            assert!(worst < limit, "diag={diagonal}: false colour {worst}");
            assert!(worst < worst_bl * 0.6, "diag={diagonal}: rcd {worst} vs bilinear {worst_bl}");
        }
    }

    #[test]
    fn vertical_stripes_resolve_along_their_direction() {
        // Constant down each column: RCD should pick vertical interpolation
        // and leave each column's value intact (a bilinear would blur it).
        let (w, h) = (36, 36);
        let f = |_y: usize, x: usize| {
            let v = if (x / 3) % 2 == 0 { 0.7 } else { 0.2 };
            [v, v, v]
        };
        let out = rcd(&mosaic(w, h, f), w, h, RGGB);
        let mut err = 0.0;
        let mut n = 0;
        for y in 6..h - 6 {
            for x in 6..w - 6 {
                err += (out.data[(y * w + x) * 3 + 1] - f(y, x)[1]).abs();
                n += 1;
            }
        }
        assert!(err / (n as f32) < 0.05, "mean error {}", err / n as f32);
    }

    #[test]
    fn superpixel_bins_and_flags_clipping() {
        let (w, h) = (4, 2);
        let data = vec![0.2, 0.4, 1.0, 0.4, 0.4, 0.6, 0.4, 0.6];
        let dec = Decoded {
            info: crate::decode::tests_info(),
            width: w,
            height: h,
            layout: Layout::Bayer(RGGB),
            data,
            calib: crate::calib::Calibration::single(crate::color::IDENTITY),
            baseline_ev: None,
        };
        let out = superpixel(&dec);
        assert_eq!((out.w, out.h), (2, 1));
        assert_eq!(&out.data[0..3], &[0.2, 0.4, 0.6]);
        assert_eq!(out.data[3], 1.0);
    }

    #[test]
    // rawler's Markesteijn relies on wrapping `usize` arithmetic in
    // `CFA::color_at`, which panics under debug overflow checks.
    #[cfg_attr(debug_assertions, ignore)]
    fn xtrans_flat_colour() {
        let pat_s = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG";
        let mut pat = [[0u8; 6]; 6];
        for (i, ch) in pat_s.chars().enumerate() {
            pat[i / 6][i % 6] = match ch { 'R' => 0, 'G' => 1, _ => 2 };
        }
        let (w, h) = (96, 96);
        let c = [0.6f32, 0.35, 0.2];
        let cfa: Vec<f32> = (0..w * h).map(|i| c[pat[(i / w) % 6][(i % w) % 6] as usize]).collect();
        let out = markesteijn(cfa, w, h, &pat);
        let p = &out.data[(48 * w + 48) * 3..(48 * w + 48) * 3 + 3];
        assert!((p[0] - c[0]).abs() < 0.01 && (p[1] - c[1]).abs() < 0.01 && (p[2] - c[2]).abs() < 0.01, "{p:?}");
    }
}
