//! Noise: Add Noise, Reduce Noise, Median, Dust & Scratches, Minimum and
//! Maximum, plus the noise estimators the analysis ops share.

use crate::blur::box_blur;
use crate::util::{hash3, to_u8, unit, Frame};

// ---------------------------------------------------------------------------
// Add Noise

/// Photoshop's Add Noise. Uniform noise spans ±`amount`% of half the range;
/// Gaussian noise has the same spread as a standard deviation of half that,
/// so its tails reach further, as in Photoshop. The noise is a function of
/// document position, so it is identical however the image is tiled.
pub fn add(buf: &mut [u8], frame: &Frame, amount: f32, gaussian: bool, mono: bool, seed: u64) {
    if amount <= 0.0 {
        return;
    }
    let amp = amount / 100.0 * 127.5;
    let sample = |x: i64, y: i64, ch: u64| -> f32 {
        let s = seed.wrapping_add(ch.wrapping_mul(0x51_7CC1_B727_220A));
        if gaussian {
            let u1 = unit(hash3(x, y, s)).max(1e-7);
            let u2 = unit(hash3(x, y, s ^ 0xA5A5_5A5A));
            (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * amp * 0.5
        } else {
            (unit(hash3(x, y, s)) * 2.0 - 1.0) * amp
        }
    };
    for y in 0..frame.h {
        for x in 0..frame.w {
            let (dx, dy) = (frame.doc_x(x), frame.doc_y(y));
            let i = (y * frame.w + x) * 4;
            if mono {
                let n = sample(dx, dy, 0);
                for c in 0..3 {
                    buf[i + c] = to_u8(buf[i + c] as f32 + n);
                }
            } else {
                for c in 0..3 {
                    buf[i + c] = to_u8(buf[i + c] as f32 + sample(dx, dy, c as u64 + 1));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Noise estimation

/// Immerkær's fast noise variance estimate (1996): the mean absolute
/// response to a Laplacian-difference mask that cancels smooth structure.
/// `exclude` drops that fraction of the strongest-gradient pixels first
/// (Tai & Yang's refinement), so edges and texture do not read as noise.
/// Returns sigma in the plane's units.
pub fn immerkaer_sigma(p: &[f32], w: usize, h: usize, exclude: Option<f32>) -> f32 {
    if w < 3 || h < 3 {
        return 0.0;
    }
    let at = |x: usize, y: usize| p[y * w + x];
    let n_inner = (w - 2) * (h - 2);
    let mut resp = Vec::with_capacity(n_inner);
    let mut grad = Vec::with_capacity(if exclude.is_some() { n_inner } else { 0 });
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let v = at(x - 1, y - 1) - 2.0 * at(x, y - 1) + at(x + 1, y - 1) - 2.0 * at(x - 1, y) + 4.0 * at(x, y) - 2.0 * at(x + 1, y) + at(x - 1, y + 1) - 2.0 * at(x, y + 1) + at(x + 1, y + 1);
            resp.push(v.abs());
            if exclude.is_some() {
                let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
                let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
                grad.push(gx.abs() + gy.abs());
            }
        }
    }
    let (sum, count) = match exclude {
        None => (resp.iter().map(|&v| v as f64).sum::<f64>(), resp.len()),
        Some(frac) => {
            let cut = quantile(&grad, 1.0 - frac.clamp(0.0, 0.95));
            let mut s = 0f64;
            let mut n = 0usize;
            for (r, g) in resp.iter().zip(&grad) {
                if *g <= cut {
                    s += *r as f64;
                    n += 1;
                }
            }
            (s, n)
        }
    };
    if count == 0 {
        return 0.0;
    }
    ((std::f64::consts::PI / 2.0).sqrt() * sum / (6.0 * count as f64)) as f32
}

/// Approximate quantile of non-negative values through a histogram.
pub fn quantile(v: &[f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let max = v.iter().cloned().fold(0f32, f32::max);
    if max <= 0.0 {
        return 0.0;
    }
    const BINS: usize = 4096;
    let mut hist = vec![0u32; BINS];
    let scale = (BINS - 1) as f32 / max;
    for &x in v {
        hist[((x.max(0.0) * scale) as usize).min(BINS - 1)] += 1;
    }
    let target = (q.clamp(0.0, 1.0) * v.len() as f32) as u64;
    let mut acc = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc > target {
            return (i as f32 + 0.5) / scale;
        }
    }
    max
}

// ---------------------------------------------------------------------------
// Reduce Noise

/// Levels of the à-trous decomposition, and the aprons that reach needs.
const LEVELS: usize = 5;
pub const DENOISE_REACH: i32 = 2 * ((1 << LEVELS) - 1) + 8;

/// Standard deviation of unit white noise in each B3-spline à-trous detail
/// layer (Starck & Murtagh).
const B3_NOISE: [f32; LEVELS] = [0.889, 0.200, 0.086, 0.041, 0.020];

/// One B3-spline smoothing step with holes of `step` pixels.
fn atrous_smooth(src: &[f32], dst: &mut [f32], tmp: &mut [f32], w: usize, h: usize, step: usize) {
    const K: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    let s = step as isize;
    let (wi, hi) = (w as isize - 1, h as isize - 1);
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut tmp[y * w..(y + 1) * w];
        for x in 0..w {
            let xi = x as isize;
            let mut acc = 0f32;
            for (k, kw) in K.iter().enumerate() {
                let xx = (xi + (k as isize - 2) * s).clamp(0, wi) as usize;
                acc += kw * row[xx];
            }
            out[x] = acc;
        }
    }
    for y in 0..h {
        let out = &mut dst[y * w..(y + 1) * w];
        out.fill(0.0);
        for (k, kw) in K.iter().enumerate() {
            let yy = (y as isize + (k as isize - 2) * s).clamp(0, hi) as usize;
            let row = &tmp[yy * w..(yy + 1) * w];
            for (o, v) in out.iter_mut().zip(row) {
                *o += kw * v;
            }
        }
    }
}

/// Wavelet denoising of one plane: an undecimated B3-spline à-trous
/// decomposition whose detail coefficients are shrunk by a local Wiener gain
/// `max(0, E[d²] − (k·σⱼ)²) / E[d²]`, with `E[d²]` averaged over a 5×5
/// window. Returns the noise sigma it estimated (from the median absolute
/// finest-level coefficient) when `sigma` is `None`.
pub fn wavelet_denoise(p: &mut [f32], w: usize, h: usize, sigma: Option<f32>, k: [f32; LEVELS]) -> f32 {
    let n = w * h;
    if w < 4 || h < 4 {
        return 0.0;
    }
    let mut c = p.to_vec();
    let mut next = vec![0f32; n];
    let mut d = vec![0f32; n];
    let mut e = vec![0f32; n];
    let mut out = vec![0f32; n];
    let mut sigma_used = sigma.unwrap_or(0.0);
    for (j, &kj) in k.iter().enumerate() {
        atrous_smooth(&c, &mut next, &mut d, w, h, 1 << j);
        for i in 0..n {
            d[i] = c[i] - next[i];
        }
        if j == 0 && sigma.is_none() {
            let abs: Vec<f32> = d.iter().map(|v| v.abs()).collect();
            sigma_used = quantile(&abs, 0.5) / 0.6745 / B3_NOISE[0];
        }
        let t = kj * sigma_used * B3_NOISE[j];
        let t2 = t * t;
        if t2 <= 0.0 {
            for i in 0..n {
                out[i] += d[i];
            }
        } else {
            for i in 0..n {
                e[i] = d[i] * d[i];
            }
            box_blur(&mut e, w, h, 2.0);
            for i in 0..n {
                let energy = e[i];
                let gain = if energy > t2 { (energy - t2) / energy } else { 0.0 };
                out[i] += d[i] * gain;
            }
        }
        std::mem::swap(&mut c, &mut next);
    }
    for i in 0..n {
        p[i] = out[i] + c[i];
    }
    sigma_used
}

/// Reduce Noise: luminance and colour noise handled separately in a
/// luma/colour-difference space. `strength` 0..10 scales the shrinkage
/// against the measured noise; `preserve_details` protects the two finest
/// luminance scales; `reduce_color_noise` pushes chroma much harder, where
/// detail loss is invisible.
pub fn reduce(buf: &mut [u8], w: usize, h: usize, strength: f32, preserve_details: f32, reduce_color: f32) {
    if strength <= 0.0 && reduce_color <= 0.0 {
        return;
    }
    let n = w * h;
    let mut yp = vec![0f32; n];
    let mut cb = vec![0f32; n];
    let mut cr = vec![0f32; n];
    for (i, px) in buf.chunks_exact(4).enumerate() {
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        yp[i] = y;
        cb[i] = b - y;
        cr[i] = r - y;
    }
    let base = strength / 5.0;
    let protect = 1.0 - 0.55 * preserve_details / 100.0;
    let luma_k = [base * protect, base * (0.5 + 0.5 * protect), base, base, base];
    wavelet_denoise(&mut yp, w, h, None, luma_k);
    let chroma = (0.5 + 3.0 * reduce_color / 100.0) * strength.max(2.0) / 5.0;
    let chroma_k = [chroma; LEVELS];
    wavelet_denoise(&mut cb, w, h, None, chroma_k);
    wavelet_denoise(&mut cr, w, h, None, chroma_k);
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        let y = yp[i];
        let r = y + cr[i];
        let b = y + cb[i];
        let g = (y - 0.299 * r - 0.114 * b) / 0.587;
        px[0] = to_u8(r);
        px[1] = to_u8(g);
        px[2] = to_u8(b);
    }
}

// ---------------------------------------------------------------------------
// Median

/// Median over a (2r+1)² square per channel (alpha included), in constant
/// time per pixel whatever the radius: Perreault & Hébert's column
/// histograms with a 16-bin coarse level and lazily updated fine bins.
/// With `threshold > 0` this is Dust & Scratches: a pixel only takes the
/// median when it differs from it by more than the threshold.
pub fn median(buf: &mut [u8], frame: &Frame, r: usize, threshold: f32) {
    if r == 0 {
        return;
    }
    let (w, h) = (frame.w, frame.h);
    let inner = frame.inner;
    let (wx0, wy0, wx1, wy1) = (inner.x as usize, inner.y as usize, inner.right() as usize, inner.bottom() as usize);
    let src = buf.to_vec();
    let n = 2 * r + 1;
    let half = (n * n / 2) as u32;
    let ri = r as isize;
    // An opaque layer's alpha median is 255 everywhere: skip it.
    let channels = if crate::util::is_opaque(&src) { 3 } else { 4 };
    for c in 0..channels {
        let at = |y: isize, x: usize| src[(y.clamp(0, h as isize - 1) as usize * w + x) * 4 + c] as usize;
        let mut coarse = vec![0u16; w * 16];
        let mut fine = vec![0u16; w * 256];
        for x in 0..w {
            for dy in -ri..=ri {
                let v = at(wy0 as isize + dy, x);
                coarse[x * 16 + (v >> 4)] += 1;
                fine[x * 256 + v] += 1;
            }
        }
        let colx = |x: isize| x.clamp(0, w as isize - 1) as usize;
        for y in wy0..wy1 {
            if y > wy0 {
                for x in 0..w {
                    let old = at(y as isize - ri - 1, x);
                    let new = at(y as isize + ri, x);
                    coarse[x * 16 + (old >> 4)] -= 1;
                    fine[x * 256 + old] -= 1;
                    coarse[x * 16 + (new >> 4)] += 1;
                    fine[x * 256 + new] += 1;
                }
            }
            let mut kc = [0u32; 16];
            let mut kf = [0u32; 256];
            // Column index at which each fine segment was last brought up to date.
            let mut luc = [isize::MIN; 16];
            for dx in -ri..=ri {
                let o = colx(wx0 as isize + dx) * 16;
                for (k, v) in kc.iter_mut().zip(&coarse[o..o + 16]) {
                    *k += *v as u32;
                }
            }
            for x in wx0..wx1 {
                let xi = x as isize;
                if x > wx0 {
                    let (a, s) = (colx(xi + ri) * 16, colx(xi - ri - 1) * 16);
                    for k in 0..16 {
                        kc[k] = kc[k] + coarse[a + k] as u32 - coarse[s + k] as u32;
                    }
                }
                let mut sum = 0u32;
                let mut seg = 0;
                while seg < 15 && sum + kc[seg] <= half {
                    sum += kc[seg];
                    seg += 1;
                }
                let base = seg * 16;
                if luc[seg] == isize::MIN || xi - luc[seg] > 2 * ri + 1 {
                    kf[base..base + 16].fill(0);
                    for dx in -ri..=ri {
                        let o = colx(xi + dx) * 256 + base;
                        for (k, v) in kf[base..base + 16].iter_mut().zip(&fine[o..o + 16]) {
                            *k += *v as u32;
                        }
                    }
                } else {
                    for j in luc[seg] + 1..=xi {
                        let (a, s) = (colx(j + ri) * 256 + base, colx(j - ri - 1) * 256 + base);
                        for k in 0..16 {
                            kf[base + k] = kf[base + k] + fine[a + k] as u32 - fine[s + k] as u32;
                        }
                    }
                }
                luc[seg] = xi;
                let mut b = 0;
                while b < 15 && sum + kf[base + b] <= half {
                    sum += kf[base + b];
                    b += 1;
                }
                let med = (base + b) as u8;
                let i = (y * w + x) * 4 + c;
                if threshold <= 0.0 || (src[i] as f32 - med as f32).abs() > threshold {
                    buf[i] = med;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Minimum / Maximum

/// van Herk / Gil–Werman running extremum over `2r+1` samples, O(n).
fn van_herk<T: Copy + PartialOrd>(line: &[T], out: &mut [T], r: usize, is_max: bool, g: &mut Vec<T>, hh: &mut Vec<T>) {
    let n = line.len();
    let k = 2 * r + 1;
    let pick = |a: T, b: T| if (a > b) == is_max { a } else { b };
    g.clear();
    hh.clear();
    // Pad with edge values.
    let at = |i: usize| line[i.saturating_sub(r).min(n - 1)];
    let len = n + 2 * r;
    for i in 0..len {
        let v = at(i);
        g.push(if i % k == 0 { v } else { pick(g[i - 1], v) });
    }
    hh.resize(len, line[0]);
    for i in (0..len).rev() {
        let v = at(i);
        hh[i] = if i % k == k - 1 || i == len - 1 { v } else { pick(hh[i + 1], v) };
    }
    for (x, o) in out.iter_mut().enumerate() {
        *o = pick(hh[x], g[x + k - 1]);
    }
}

/// Minimum/Maximum of one float plane over a square.
pub fn min_max_plane(p: &mut [f32], w: usize, h: usize, r: usize, is_max: bool) {
    if r == 0 {
        return;
    }
    let (mut g, mut hh) = (Vec::new(), Vec::new());
    let mut out = vec![0f32; w.max(h)];
    for y in 0..h {
        let row = &mut p[y * w..(y + 1) * w];
        van_herk(row, &mut out[..w], r, is_max, &mut g, &mut hh);
        row.copy_from_slice(&out[..w]);
    }
    let mut col = vec![0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = p[y * w + x];
        }
        van_herk(&col, &mut out[..h], r, is_max, &mut g, &mut hh);
        for y in 0..h {
            p[y * w + x] = out[y];
        }
    }
}

/// Photoshop's Minimum (spreads dark) and Maximum (spreads light) over a
/// square of radius `r`, every channel including alpha.
pub fn min_max(buf: &mut [u8], w: usize, h: usize, r: usize, is_max: bool) {
    if r == 0 {
        return;
    }
    let (mut g, mut hh) = (Vec::new(), Vec::new());
    let mut line = vec![0u8; w.max(h)];
    let mut out = vec![0u8; w.max(h)];
    let channels = if crate::util::is_opaque(buf) { 3 } else { 4 };
    for c in 0..channels {
        for y in 0..h {
            for x in 0..w {
                line[x] = buf[(y * w + x) * 4 + c];
            }
            van_herk(&line[..w], &mut out[..w], r, is_max, &mut g, &mut hh);
            for x in 0..w {
                buf[(y * w + x) * 4 + c] = out[x];
            }
        }
        for x in 0..w {
            for y in 0..h {
                line[y] = buf[(y * w + x) * 4 + c];
            }
            van_herk(&line[..h], &mut out[..h], r, is_max, &mut g, &mut hh);
            for y in 0..h {
                buf[(y * w + x) * 4 + c] = out[y];
            }
        }
    }
}
