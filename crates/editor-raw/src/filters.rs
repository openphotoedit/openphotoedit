//! Separable box and Gaussian blurs and the guided filter, on single-channel
//! f32 planes. O(n) in the radius.

use rayon::prelude::*;

/// Box blur of radius `r` (window 2r+1), edges clamped. Horizontal then vertical.
pub fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut tmp = vec![0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        box_line(&src[y * w..(y + 1) * w], out, r);
    });
    // Vertical pass as running column sums, one row at a time (vectorises).
    let norm = 1.0 / (2 * r + 1) as f32;
    let row = |y: isize| {
        let y = y.clamp(0, h as isize - 1) as usize;
        &tmp[y * w..(y + 1) * w]
    };
    let mut sum = vec![0f32; w];
    for dy in -(r as isize)..=(r as isize) {
        for (s, v) in sum.iter_mut().zip(row(dy)) {
            *s += v;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let o = &mut out[y * w..(y + 1) * w];
        for (ov, s) in o.iter_mut().zip(&sum) {
            *ov = s * norm;
        }
        let add = row(y as isize + r as isize + 1);
        let sub = row(y as isize - r as isize);
        for ((s, a), b) in sum.iter_mut().zip(add).zip(sub) {
            *s += a - b;
        }
        // Float drift is bounded: re-seed every 512 rows.
        if y % 512 == 511 && y + 1 < h {
            sum.iter_mut().for_each(|s| *s = 0.0);
            for dy in -(r as isize)..=(r as isize) {
                for (s, v) in sum.iter_mut().zip(row(y as isize + 1 + dy)) {
                    *s += v;
                }
            }
        }
    }
    out
}

fn box_line(src: &[f32], out: &mut [f32], r: usize) {
    let n = src.len();
    let at = |i: isize| src[i.clamp(0, n as isize - 1) as usize];
    let mut sum = 0f32;
    for i in -(r as isize)..=(r as isize) {
        sum += at(i);
    }
    let norm = 1.0 / (2 * r + 1) as f32;
    let ri = r as isize;
    for (i, o) in out.iter_mut().enumerate() {
        *o = sum * norm;
        let i = i as isize;
        let (a, b) = (i + ri + 1, i - ri);
        if b >= 0 && (a as usize) < n {
            sum += src[a as usize] - src[b as usize];
        } else {
            sum += at(a) - at(b);
        }
    }
}

/// Exact separable Gaussian with a kernel of radius ceil(3σ), for small σ.
pub fn small_gauss(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil().max(1.0) as isize;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|v| v / s).collect();
    let mut tmp = vec![0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let line = &src[y * w..(y + 1) * w];
        for (x, o) in row.iter_mut().enumerate() {
            let mut acc = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let xx = (x as isize + j as isize - r).clamp(0, w as isize - 1) as usize;
                acc += line[xx] * kv;
            }
            *o = acc;
        }
    });
    let mut out = vec![0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let mut acc = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let yy = (y as isize + j as isize - r).clamp(0, h as isize - 1) as usize;
                acc += tmp[yy * w + x] * kv;
            }
            *o = acc;
        }
    });
    out
}

/// Downsample by an integer factor (box average).
pub fn downsample(src: &[f32], w: usize, h: usize, f: usize) -> (Vec<f32>, usize, usize) {
    let (ow, oh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0f32; ow * oh];
    out.par_chunks_mut(ow).enumerate().for_each(|(oy, row)| {
        for (ox, o) in row.iter_mut().enumerate() {
            let mut s = 0.0;
            let mut n = 0;
            for y in oy * f..((oy + 1) * f).min(h) {
                for x in ox * f..((ox + 1) * f).min(w) {
                    s += src[y * w + x];
                    n += 1;
                }
            }
            *o = s / n.max(1) as f32;
        }
    });
    (out, ow, oh)
}

/// Bilinear sample of a low-resolution plane at full-resolution pixel (x, y).
#[inline]
pub fn sample_up(lo: &[f32], lw: usize, lh: usize, f: usize, x: usize, y: usize) -> f32 {
    let fx = ((x as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (lw - 1) as f32);
    let fy = ((y as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (lh - 1) as f32);
    let (x0, y0) = (fx as usize, fy as usize);
    let (x1, y1) = ((x0 + 1).min(lw - 1), (y0 + 1).min(lh - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let a = lo[y0 * lw + x0] + (lo[y0 * lw + x1] - lo[y0 * lw + x0]) * tx;
    let b = lo[y1 * lw + x0] + (lo[y1 * lw + x1] - lo[y1 * lw + x0]) * tx;
    a + (b - a) * ty
}

/// Fast guided filter (He & Sun 2015): filter `p` guided by `guide`, with
/// the linear coefficients computed at 1/`sub` resolution. Edge-preserving:
/// structures in `guide` with variance above `eps` survive.
pub fn guided(guide: &[f32], p: &[f32], w: usize, h: usize, r: usize, eps: f32, sub: usize) -> Vec<f32> {
    let sub = sub.max(1);
    let (gi, lw, lh) = downsample(guide, w, h, sub);
    let (pi, _, _) = downsample(p, w, h, sub);
    let r = (r / sub).max(1);
    let mean_i = box_blur(&gi, lw, lh, r);
    let mean_p = box_blur(&pi, lw, lh, r);
    let ii: Vec<f32> = gi.iter().map(|v| v * v).collect();
    let ip: Vec<f32> = gi.iter().zip(&pi).map(|(a, b)| a * b).collect();
    let corr_i = box_blur(&ii, lw, lh, r);
    let corr_ip = box_blur(&ip, lw, lh, r);
    let mut a = vec![0f32; lw * lh];
    let mut b = vec![0f32; lw * lh];
    for i in 0..lw * lh {
        let var = (corr_i[i] - mean_i[i] * mean_i[i]).max(0.0);
        let cov = corr_ip[i] - mean_i[i] * mean_p[i];
        a[i] = cov / (var + eps);
        b[i] = mean_p[i] - a[i] * mean_i[i];
    }
    let ma = box_blur(&a, lw, lh, r);
    let mb = box_blur(&b, lw, lh, r);
    let mut out = vec![0f32; w * h];
    if sub == 1 {
        out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let i = y * w + x;
                *o = ma[i] * guide[i] + mb[i];
            }
        });
        return out;
    }
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let i = y * w + x;
            *o = sample_up(&ma, lw, lh, sub, x, y) * guide[i] + sample_up(&mb, lw, lh, sub, x, y);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_preserves_flat_and_mean() {
        let src = vec![0.5f32; 30 * 20];
        let out = box_blur(&src, 30, 20, 3);
        assert!(out.iter().all(|v| (v - 0.5).abs() < 1e-5));
    }

    #[test]
    fn guided_filter_keeps_strong_edges() {
        let (w, h) = (64, 64);
        let src: Vec<f32> = (0..w * h).map(|i| if i % w < 32 { -3.0 } else { 1.0 }).collect();
        let out = guided(&src, &src, w, h, 8, 0.01, 2);
        assert!((out[20 * w + 10] + 3.0).abs() < 0.05);
        assert!((out[20 * w + 50] - 1.0).abs() < 0.05);
    }
}
