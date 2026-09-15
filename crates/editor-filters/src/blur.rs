//! Blurs: Gaussian, box, motion, radial, surface, lens and tilt-shift.
//!
//! Plane functions work on one `f32` channel (0..255) and clamp at the buffer
//! edge; the command layer pads the buffer beyond the canvas and feeds colour
//! premultiplied (see `util::linear_filter`).

use crate::util::{is_opaque, to_u8, Frame};

// ---------------------------------------------------------------------------
// Gaussian

/// Pixels a Gaussian of `sigma` reaches, for aprons.
pub fn gaussian_reach(sigma: f32) -> i32 {
    if sigma <= 0.0 {
        0
    } else {
        (3.5 * sigma).ceil() as i32 + 3
    }
}

/// Gaussian blur of one plane. Photoshop's Gaussian Blur "radius" behaves as
/// the standard deviation, so `sigma` is that radius.
///
/// Small sigmas use an exact sampled kernel. Larger ones use four passes of
/// the extended box filter (Gwosdek et al. 2011), whose variance matches the
/// Gaussian exactly with fractional radii and costs O(n) whatever the radius.
pub fn gaussian(p: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma < 0.05 || w == 0 || h == 0 {
        return;
    }
    if sigma < 2.5 {
        let r = (3.0 * sigma).ceil() as usize;
        let mut k: Vec<f32> = (0..=2 * r).map(|i| {
            let d = i as f32 - r as f32;
            (-d * d / (2.0 * sigma * sigma)).exp()
        }).collect();
        let s: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= s);
        let mut tmp = vec![0f32; p.len()];
        convolve_h(p, &mut tmp, w, h, &k);
        convolve_v(&tmp, p, w, h, &k);
        return;
    }
    const PASSES: usize = 4;
    let s2 = sigma * sigma / PASSES as f32;
    let (r, alpha) = ebox_params(s2);
    let mut tmp = vec![0f32; p.len()];
    for _ in 0..PASSES {
        ebox_h(p, &mut tmp, w, h, r, alpha);
        ebox_v(&tmp, p, w, h, r, alpha);
    }
}

/// Integer radius and end-tap weight of an extended box with variance `s2`.
fn ebox_params(s2: f32) -> (usize, f32) {
    let r = (((1.0 + 12.0 * s2).sqrt() - 1.0) / 2.0).floor().max(0.0);
    let rf = r;
    let alpha = (2.0 * rf + 1.0) * (rf * (rf + 1.0) - 3.0 * s2) / (6.0 * (s2 - (rf + 1.0) * (rf + 1.0)));
    (r as usize, alpha.clamp(0.0, 1.0))
}

/// Symmetric kernel along rows, edges clamped.
pub fn convolve_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, k: &[f32]) {
    let r = k.len() / 2;
    let mut line = vec![0f32; w + 2 * r];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        line[..r].fill(row[0]);
        line[r..r + w].copy_from_slice(row);
        line[r + w..].fill(row[w - 1]);
        let out = &mut dst[y * w..(y + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            let win = &line[x..x + k.len()];
            *o = win.iter().zip(k).map(|(a, b)| a * b).sum();
        }
    }
}

/// Symmetric kernel along columns, processed row by row so the inner loop
/// vectorises.
pub fn convolve_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, k: &[f32]) {
    let r = k.len() as isize / 2;
    for y in 0..h {
        let out = &mut dst[y * w..(y + 1) * w];
        out.fill(0.0);
        for (i, &kw) in k.iter().enumerate() {
            let yy = (y as isize + i as isize - r).clamp(0, h as isize - 1) as usize;
            let row = &src[yy * w..(yy + 1) * w];
            for (o, v) in out.iter_mut().zip(row) {
                *o += kw * v;
            }
        }
    }
}

fn ebox_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, a: f32) {
    let norm = 1.0 / ((2 * r + 1) as f32 + 2.0 * a);
    let pad = r + 1;
    let mut line = vec![0f32; w + 2 * pad];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        line[..pad].fill(row[0]);
        line[pad..pad + w].copy_from_slice(row);
        line[pad + w..].fill(row[w - 1]);
        let mut sum: f32 = line[pad - r..=pad + r].iter().sum();
        let out = &mut dst[y * w..(y + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            let c = pad + x;
            *o = (sum + a * (line[c - r - 1] + line[c + r + 1])) * norm;
            sum += line[c + r + 1] - line[c - r];
        }
    }
}

fn ebox_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, a: f32) {
    let norm = 1.0 / ((2 * r + 1) as f32 + 2.0 * a);
    let hi = h as isize - 1;
    let row = |i: isize| {
        let y = i.clamp(0, hi) as usize;
        &src[y * w..(y + 1) * w]
    };
    let mut sum = vec![0f32; w];
    for i in -(r as isize)..=(r as isize) {
        for (s, v) in sum.iter_mut().zip(row(i)) {
            *s += v;
        }
    }
    for y in 0..h as isize {
        let (up, dn) = (row(y - r as isize - 1), row(y + r as isize + 1));
        let out = &mut dst[y as usize * w..(y as usize + 1) * w];
        for x in 0..w {
            out[x] = (sum[x] + a * (up[x] + dn[x])) * norm;
        }
        let sub = row(y - r as isize);
        for x in 0..w {
            sum[x] += dn[x] - sub[x];
        }
    }
}

// ---------------------------------------------------------------------------
// Box

/// Photoshop's Box Blur: the mean over a (2r+1)² square; fractional radii
/// weight the outer ring.
pub fn box_blur(p: &mut [f32], w: usize, h: usize, radius: f32) {
    if radius <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let r = radius.floor() as usize;
    let a = radius - r as f32;
    let mut tmp = vec![0f32; p.len()];
    ebox_h(p, &mut tmp, w, h, r, a);
    ebox_v(&tmp, p, w, h, r, a);
}

// ---------------------------------------------------------------------------
// Motion

/// A uniform streak `distance` pixels long at `angle` degrees
/// (counter-clockwise from the x axis, as Photoshop's dial).
///
/// Each line of the streak direction is resampled into a row, box-filtered
/// in O(n) with a running sum, and splatted back with linear weights; the
/// two interpolations add well under a pixel of cross-blur.
pub fn motion(p: &mut [f32], w: usize, h: usize, angle: f32, distance: f32) {
    if distance <= 1.0 || w == 0 || h == 0 {
        return;
    }
    let th = angle.to_radians();
    let (dx, dy) = (th.cos(), -th.sin());
    if dx.abs() >= dy.abs() {
        motion_x_major(p, w, h, dy / dx, distance);
    } else {
        let mut t = transpose(p, w, h);
        motion_x_major(&mut t, h, w, dx / dy, distance);
        let back = transpose(&t, h, w);
        p.copy_from_slice(&back);
    }
}

fn transpose(p: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; p.len()];
    for y in 0..h {
        for x in 0..w {
            out[x * h + y] = p[y * w + x];
        }
    }
    out
}

fn motion_x_major(p: &mut [f32], w: usize, h: usize, t: f32, distance: f32) {
    let steps = distance / (1.0 + t * t).sqrt();
    if steps <= 1.0 {
        return;
    }
    let half = (steps - 1.0) / 2.0;
    let r = half.floor() as usize;
    let a = half - r as f32;
    let norm = 1.0 / ((2 * r + 1) as f32 + 2.0 * a);
    let src = p.to_vec();
    p.fill(0.0);
    let span = t * (w - 1) as f32;
    let jmin = (-span).min(0.0).floor() as i64 - 1;
    let jmax = (h as f32 - 1.0 + (-span).max(0.0)).ceil() as i64 + 1;
    let pad = r + 1;
    let mut line = vec![0f32; w + 2 * pad];
    let mut ys = vec![0f32; w];
    let hmax = (h - 1) as f32;
    for j in jmin..=jmax {
        // Skip lines that never touch the image.
        let y_first = j as f32;
        let y_last = j as f32 + span;
        if y_first.max(y_last) < -1.0 || y_first.min(y_last) > h as f32 {
            continue;
        }
        for x in 0..w {
            let yy = j as f32 + t * x as f32;
            ys[x] = yy;
            let fy = yy.clamp(0.0, hmax);
            let y0 = fy as usize;
            let y1 = (y0 + 1).min(h - 1);
            let ty = fy - y0 as f32;
            line[pad + x] = src[y0 * w + x] * (1.0 - ty) + src[y1 * w + x] * ty;
        }
        let (first, last) = (line[pad], line[pad + w - 1]);
        line[..pad].fill(first);
        line[pad + w..].fill(last);
        let mut sum: f32 = line[pad - r..=pad + r].iter().sum();
        for (x, &yy) in ys.iter().enumerate() {
            let c = pad + x;
            let v = (sum + a * (line[c - r - 1] + line[c + r + 1])) * norm;
            sum += line[c + r + 1] - line[c - r];
            let y0 = yy.floor();
            let fy = yy - y0;
            let y0 = y0 as i64;
            if y0 >= 0 && (y0 as usize) < h {
                p[y0 as usize * w + x] += v * (1.0 - fy);
            }
            if y0 + 1 >= 0 && ((y0 + 1) as usize) < h {
                p[(y0 + 1) as usize * w + x] += v * fy;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Radial

/// Spin (arcs about the centre) or zoom (streaks through it). `cx, cy` are
/// buffer coordinates.
///
/// Rotations about one centre commute, and so do scalings, so a uniform
/// streak of angle `A` is the composition of two-tap averages at ±A/4,
/// ±A/8, …: `k` passes give `2^k` evenly spaced samples for `2k` bilinear
/// taps per pixel. `k` is chosen so samples are at most a pixel apart at the
/// far corner, which makes the result smooth rather than grainy and the cost
/// logarithmic in the streak length. Work is premultiplied in 16 bits.
pub fn radial(buf: &mut [u8], frame: &Frame, amount: f32, zoom: bool, cx: f32, cy: f32) {
    let (w, h) = (frame.w, frame.h);
    let amount = amount.clamp(0.0, 100.0);
    // Total spread: an angle for spin, a log-scale range for zoom.
    let total = if zoom {
        let z = amount / 100.0 * 0.6;
        ((1.0 + z / 2.0) / (1.0 - z / 2.0)).ln()
    } else {
        amount.to_radians()
    };
    let inner = frame.inner;
    let far = [(inner.x, inner.y), (inner.right(), inner.y), (inner.x, inner.bottom()), (inner.right(), inner.bottom())]
        .iter()
        .map(|&(x, y)| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt())
        .fold(0f32, f32::max);
    let max_len = far * total;
    if max_len < 0.75 {
        return;
    }
    let passes = (max_len.log2().ceil() as i32).clamp(1, 12) as usize;
    let mut a: Vec<u16> = Vec::with_capacity(w * h * 4);
    for p in buf.chunks_exact(4) {
        let al = p[3] as u32;
        for c in 0..3 {
            a.push(((p[c] as u32 * al * 257 + 127) / 255) as u16);
        }
        a.push((al * 257) as u16);
    }
    let mut b = vec![0u16; a.len()];
    let (wm, hm) = ((w - 1) as f32, (h - 1) as f32);
    let sample = |src: &[u16], x: f32, y: f32| -> [f32; 4] {
        let fx = (x - 0.5).clamp(0.0, wm);
        let fy = (y - 0.5).clamp(0.0, hm);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let (i00, i10, i01, i11) = ((y0 * w + x0) * 4, (y0 * w + x1) * 4, (y1 * w + x0) * 4, (y1 * w + x1) * 4);
        std::array::from_fn(|c| {
            let top = src[i00 + c] as f32 + (src[i10 + c] as f32 - src[i00 + c] as f32) * tx;
            let bot = src[i01 + c] as f32 + (src[i11 + c] as f32 - src[i01 + c] as f32) * tx;
            top + (bot - top) * ty
        })
    };
    for j in 1..=passes {
        let t = total / (1u32 << (j + 1)) as f32;
        let (sn, cs) = t.sin_cos();
        let (grow, shrink) = (t.exp(), (-t).exp());
        // Pixels closer to the centre than this move under 0.3 px in this
        // pass; averaging two taps that close is just the pixel.
        let still = 0.3 / t;
        for y in 0..h {
            let py = y as f32 + 0.5 - cy;
            for x in 0..w {
                let px = x as f32 + 0.5 - cx;
                let o = (y * w + x) * 4;
                if px * px + py * py < still * still {
                    b[o..o + 4].copy_from_slice(&a[o..o + 4]);
                    continue;
                }
                let (p1, p2) = if zoom {
                    ((cx + px * grow, cy + py * grow), (cx + px * shrink, cy + py * shrink))
                } else {
                    ((cx + px * cs - py * sn, cy + px * sn + py * cs), (cx + px * cs + py * sn, cy - px * sn + py * cs))
                };
                let s1 = sample(&a, p1.0, p1.1);
                let s2 = sample(&a, p2.0, p2.1);
                for c in 0..4 {
                    b[o + c] = ((s1[c] + s2[c]) * 0.5 + 0.5) as u16;
                }
            }
        }
        std::mem::swap(&mut a, &mut b);
    }
    for y in inner.y as usize..inner.bottom() as usize {
        for x in inner.x as usize..inner.right() as usize {
            let o = (y * w + x) * 4;
            // Near the centre the streak is under a couple of pixels; keep
            // the original there instead of the passes' interpolation.
            let r = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let k = ((r * total - 0.5) / 1.5).clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            let px = &mut buf[o..o + 4];
            let orig_a = px[3] as f32 * 257.0;
            let mut p: [f32; 4] = std::array::from_fn(|c| if c == 3 { orig_a } else { px[c] as f32 * orig_a / 255.0 });
            for c in 0..4 {
                p[c] += (a[o + c] as f32 - p[c]) * k;
            }
            if p[3] < 128.0 {
                px.fill(0);
                continue;
            }
            for c in 0..3 {
                px[c] = to_u8(p[c] * 255.0 / p[3]);
            }
            px[3] = to_u8(p[3] / 257.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Surface

/// Photoshop's Surface Blur: over a square window, each neighbour is
/// weighted by `max(0, 1 − |v − centre| / (2.5 · threshold))`, per channel.
///
/// A sliding 256-bin histogram (Perreault–Hébert column histograms) makes
/// the window cost independent of the radius; the weighted sum only visits
/// bins within the threshold band.
pub fn surface(buf: &mut [u8], w: usize, h: usize, radius: usize, threshold: f32, write: (usize, usize, usize, usize)) {
    if radius == 0 || threshold <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let reach = 2.5 * threshold.clamp(1.0, 255.0);
    let k = (reach.ceil() as usize).min(255);
    // Weights indexed by `bin − centre + k`, and bin values, as flat slices
    // so the weighted sums vectorise.
    let tri: Vec<f32> = (0..=2 * k).map(|i| (1.0 - (i as f32 - k as f32).abs() / reach).max(0.0)).collect();
    let ramp: Vec<f32> = (0..256).map(|b| b as f32).collect();
    let src = buf.to_vec();
    let r = radius as isize;
    let (wx0, wy0, wx1, wy1) = write;
    for c in 0..3 {
        // Column histograms over rows [y - r, y + r], clamped.
        let mut cols = vec![0f32; w * 256];
        let at = |y: isize, x: usize| src[(y.clamp(0, h as isize - 1) as usize * w + x) * 4 + c] as usize;
        for x in 0..w {
            for dy in -r..=r {
                cols[x * 256 + at(wy0 as isize + dy, x)] += 1.0;
            }
        }
        let mut kern = vec![0f32; 256];
        for y in wy0..wy1 {
            if y > wy0 {
                for x in 0..w {
                    cols[x * 256 + at(y as isize - r - 1, x)] -= 1.0;
                    cols[x * 256 + at(y as isize + r, x)] += 1.0;
                }
            }
            let col = |x: isize| x.clamp(0, w as isize - 1) as usize * 256;
            // The window histogram is kept in 16-bin segments, each brought
            // up to date only when the threshold band needs it.
            let mut luc = [isize::MIN; 16];
            for x in wx0..wx1 {
                let xi = x as isize;
                let v = src[(y * w + x) * 4 + c] as usize;
                let lo = v.saturating_sub(k);
                let hi = (v + k).min(255);
                for seg in lo >> 4..=hi >> 4 {
                    let base = seg * 16;
                    if luc[seg] == isize::MIN || xi - luc[seg] > 2 * r + 1 {
                        kern[base..base + 16].fill(0.0);
                        for dx in -r..=r {
                            let o = col(xi + dx) + base;
                            for (kv, cv) in kern[base..base + 16].iter_mut().zip(&cols[o..o + 16]) {
                                *kv += cv;
                            }
                        }
                    } else {
                        let seg_k: &mut [f32; 16] = (&mut kern[base..base + 16]).try_into().unwrap();
                        for j in luc[seg] + 1..=xi {
                            let (a, sub) = (col(j + r) + base, col(j - r - 1) + base);
                            let add: &[f32; 16] = cols[a..a + 16].try_into().unwrap();
                            let rem: &[f32; 16] = cols[sub..sub + 16].try_into().unwrap();
                            for i in 0..16 {
                                seg_k[i] += add[i] - rem[i];
                            }
                        }
                    }
                    luc[seg] = xi;
                }
                let (den, num) = weighted_sums(&kern[lo..=hi], &tri[k + lo - v..=k + hi - v], &ramp[lo..=hi]);
                buf[(y * w + x) * 4 + c] = if den > 0.0 { to_u8(num / den) } else { v as u8 };
            }
        }
    }
}

/// `(Σ a·b, Σ a·b·c)` accumulated in eight lanes so the compiler can use
/// SIMD without reordering a single floating-point sum.
#[inline]
fn weighted_sums(a: &[f32], b: &[f32], c: &[f32]) -> (f32, f32) {
    let mut s0 = [0f32; 8];
    let mut s1 = [0f32; 8];
    let (ac, bc, cc) = (a.chunks_exact(8), b.chunks_exact(8), c.chunks_exact(8));
    let (ar, br, cr) = (ac.remainder(), bc.remainder(), cc.remainder());
    for ((a8, b8), c8) in ac.zip(bc).zip(cc) {
        let a8: &[f32; 8] = a8.try_into().unwrap();
        let b8: &[f32; 8] = b8.try_into().unwrap();
        let c8: &[f32; 8] = c8.try_into().unwrap();
        for l in 0..8 {
            let wgt = a8[l] * b8[l];
            s0[l] += wgt;
            s1[l] += wgt * c8[l];
        }
    }
    let (mut t0, mut t1) = (s0.iter().sum::<f32>(), s1.iter().sum::<f32>());
    for ((x, y), z) in ar.iter().zip(br).zip(cr) {
        let wgt = x * y;
        t0 += wgt;
        t1 += wgt * z;
    }
    (t0, t1)
}

// ---------------------------------------------------------------------------
// Lens

const LUT_BITS: usize = 12;

fn srgb_to_linear_table() -> Vec<f32> {
    (0..256).map(|i| editor_core::color::srgb_to_linear(i as f32 / 255.0)).collect()
}

fn linear_to_srgb_table() -> Vec<f32> {
    let n = 1 << LUT_BITS;
    (0..=n).map(|i| editor_core::color::linear_to_srgb(i as f32 / n as f32) * 255.0).collect()
}

#[inline]
fn lin_to_srgb(table: &[f32], v: f32) -> f32 {
    let n = (table.len() - 1) as f32;
    let x = v.clamp(0.0, 1.0) * n;
    let i = x as usize;
    if i + 1 >= table.len() {
        return table[table.len() - 1];
    }
    table[i] + (table[i + 1] - table[i]) * (x - i as f32)
}

/// Lens Blur: a circular aperture averaged in linear light, so bright points
/// open into discs the way an out-of-focus lens renders them.
///
/// `radius_at(x, y)` gives the aperture radius per buffer pixel (a depth map
/// or a constant). Every disc is summed as horizontal spans from per-row
/// prefix sums, with anti-aliased span ends.
pub fn lens(buf: &mut [u8], w: usize, h: usize, max_radius: f32, write: (usize, usize, usize, usize), uniform: bool, radius_at: impl Fn(usize, usize) -> f32) {
    if max_radius < 0.5 || w == 0 || h == 0 {
        return;
    }
    let to_lin = srgb_to_linear_table();
    let to_srgb = linear_to_srgb_table();
    let opaque = is_opaque(buf);
    let nch = if opaque { 3 } else { 4 };
    let pr = max_radius.ceil() as usize + 1;
    let (pw, ph) = (w + 2 * pr, h + 2 * pr);
    // Prefix sums per padded row: pref[c][y][x] = sum of the first x values.
    let mut pref = vec![0f32; nch * ph * (pw + 1)];
    let stride = pw + 1;
    for yy in 0..ph {
        let sy = (yy as isize - pr as isize).clamp(0, h as isize - 1) as usize;
        let mut acc = [0f32; 4];
        for xx in 0..pw {
            let sx = (xx as isize - pr as isize).clamp(0, w as isize - 1) as usize;
            let p = &buf[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4];
            let a = p[3] as f32 / 255.0;
            for c in 0..3 {
                let v = to_lin[p[c] as usize];
                acc[c] += if opaque { v } else { v * a };
            }
            if !opaque {
                acc[3] += a;
            }
            for c in 0..nch {
                pref[(c * ph + yy) * stride + xx + 1] = acc[c];
            }
        }
    }
    // Prefix sum at a continuous position: linear inside a pixel.
    let at = |c: usize, yy: usize, x: f32| -> f32 {
        let base = (c * ph + yy) * stride;
        let xi = x.floor();
        let t = x - xi;
        let xi = (xi as isize).clamp(0, pw as isize - 1) as usize;
        pref[base + xi] + (pref[base + xi + 1] - pref[base + xi]) * t
    };
    let (wx0, wy0, wx1, wy1) = write;
    if uniform {
        // Every pixel has the same disc, so each span is a shifted row
        // difference: whole rows at a time, which vectorises.
        let rad = max_radius;
        let ri = rad.floor() as isize;
        // Per row of the disc: the span's start and end in padded row
        // coordinates relative to x (integer part and fraction), and its length.
        let spans: Vec<(isize, usize, f32, usize, f32, f32)> = (-ri..=ri)
            .map(|dy| {
                let hw = (rad * rad - (dy * dy) as f32).max(0.0).sqrt();
                let (e0, e1) = (pr as f32 - hw, pr as f32 + hw + 1.0);
                (dy, e0.floor() as usize, e0 - e0.floor(), e1.floor() as usize, e1 - e1.floor(), e1 - e0)
            })
            .collect();
        let area: f32 = spans.iter().map(|s| s.5).sum();
        let inv = 1.0 / area;
        let n = wx1 - wx0;
        let mut acc = vec![0f32; n * nch];
        for y in wy0..wy1 {
            acc.fill(0.0);
            for &(dy, i0, t0, i1, t1, _) in &spans {
                let yy = (y as isize + pr as isize + dy) as usize;
                for c in 0..nch {
                    let base = (c * ph + yy) * stride;
                    let row = &pref[base..base + stride];
                    let r1 = &row[wx0 + i1..wx0 + i1 + n + 1];
                    let r0 = &row[wx0 + i0..wx0 + i0 + n + 1];
                    let out = &mut acc[c * n..(c + 1) * n];
                    for (((o, (a1, b1)), a0), b0) in out.iter_mut().zip(r1.iter().zip(&r1[1..])).zip(r0.iter()).zip(&r0[1..]) {
                        *o += (a1 + (b1 - a1) * t1) - (a0 + (b0 - a0) * t0);
                    }
                }
            }
            for k in 0..n {
                let i = (y * w + wx0 + k) * 4;
                if opaque {
                    for c in 0..3 {
                        buf[i + c] = to_u8(lin_to_srgb(&to_srgb, acc[c * n + k] * inv));
                    }
                } else {
                    let a = acc[3 * n + k] * inv;
                    if a <= 1e-4 {
                        buf[i..i + 4].fill(0);
                    } else {
                        for c in 0..3 {
                            buf[i + c] = to_u8(lin_to_srgb(&to_srgb, acc[c * n + k] * inv / a));
                        }
                        buf[i + 3] = to_u8(a * 255.0);
                    }
                }
            }
        }
        return;
    }
    for y in wy0..wy1 {
        for x in wx0..wx1 {
            let rad = radius_at(x, y).clamp(0.0, max_radius);
            if rad < 0.5 {
                continue;
            }
            let ri = rad.floor() as isize;
            let (cx, cy) = (x as f32 + pr as f32 + 0.5, y + pr);
            let mut acc = [0f32; 4];
            let mut area = 0f32;
            for dy in -ri..=ri {
                let hw = (rad * rad - (dy * dy) as f32).max(0.0).sqrt();
                let yy = (cy as isize + dy) as usize;
                let (x0, x1) = (cx - hw - 0.5, cx + hw + 0.5);
                for c in 0..nch {
                    acc[c] += at(c, yy, x1) - at(c, yy, x0);
                }
                area += x1 - x0;
            }
            let i = (y * w + x) * 4;
            if opaque {
                for c in 0..3 {
                    buf[i + c] = to_u8(lin_to_srgb(&to_srgb, acc[c] / area));
                }
            } else {
                let a = acc[3] / area;
                if a <= 1e-4 {
                    buf[i..i + 4].fill(0);
                } else {
                    for c in 0..3 {
                        buf[i + c] = to_u8(lin_to_srgb(&to_srgb, acc[c] / area / a));
                    }
                    buf[i + 3] = to_u8(a * 255.0);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tilt-shift

/// Sharp in a horizontal band, blurring to `radius` (Gaussian sigma) over
/// `feather` pixels above and below. The blur depends only on the row, so a
/// short stack of blur levels is computed over the rows each level serves
/// and neighbouring levels are mixed.
pub fn tilt_shift(buf: &mut [u8], w: usize, h: usize, sigma_of_row: impl Fn(usize) -> f32, max_sigma: f32) {
    if max_sigma < 0.05 || w == 0 || h == 0 {
        return;
    }
    const LEVELS: usize = 6;
    let level_sigma = |k: usize| max_sigma * k as f32 / LEVELS as f32;
    let opaque = is_opaque(buf);
    let nch = if opaque { 3 } else { 4 };
    let src = buf.to_vec();
    // Premultiplied source planes.
    let planes: Vec<Vec<f32>> = (0..nch)
        .map(|c| {
            src.chunks_exact(4)
                .map(|p| if c == 3 { p[3] as f32 } else if opaque { p[c] as f32 } else { p[c] as f32 * p[3] as f32 / 255.0 })
                .collect()
        })
        .collect();
    let mut out: Vec<Vec<f32>> = vec![vec![0f32; w * h]; nch];
    let pos: Vec<f32> = (0..h).map(|y| (sigma_of_row(y) / max_sigma).clamp(0.0, 1.0) * LEVELS as f32).collect();
    for k in 0..=LEVELS {
        // Rows where level k has weight.
        let rows: Vec<usize> = (0..h).filter(|&y| (pos[y] - k as f32).abs() < 1.0).collect();
        if rows.is_empty() {
            continue;
        }
        let (r0, r1) = (rows[0], rows[rows.len() - 1] + 1);
        let s = level_sigma(k);
        let apron = gaussian_reach(s) as usize;
        let (c0, c1) = (r0.saturating_sub(apron), (r1 + apron).min(h));
        let ch = c1 - c0;
        for c in 0..nch {
            let mut crop = planes[c][c0 * w..c1 * w].to_vec();
            if k > 0 {
                gaussian(&mut crop, w, ch, s);
            }
            for &y in &rows {
                let wgt = 1.0 - (pos[y] - k as f32).abs();
                let src_row = &crop[(y - c0) * w..(y - c0 + 1) * w];
                for (o, v) in out[c][y * w..(y + 1) * w].iter_mut().zip(src_row) {
                    *o += v * wgt;
                }
            }
        }
    }
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        if opaque {
            for c in 0..3 {
                px[c] = to_u8(out[c][i]);
            }
        } else {
            let a = out[3][i];
            if a <= 0.5 {
                px.fill(0);
            } else {
                for c in 0..3 {
                    px[c] = to_u8(out[c][i] * 255.0 / a);
                }
                px[3] = to_u8(a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(w: usize, h: usize) -> Vec<f32> {
        let mut p = vec![0f32; w * h];
        p[(h / 2) * w + w / 2] = 1000.0;
        p
    }

    fn variance_x(p: &[f32], w: usize, h: usize) -> f32 {
        let (mut s, mut sx2) = (0f32, 0f32);
        let cx = (w / 2) as f32;
        for y in 0..h {
            for x in 0..w {
                let v = p[y * w + x];
                s += v;
                sx2 += v * (x as f32 - cx).powi(2);
            }
        }
        sx2 / s
    }

    #[test]
    fn gaussian_variance_matches_sigma() {
        for sigma in [0.8f32, 2.0, 3.0, 7.5, 20.0] {
            let n = (sigma * 12.0) as usize + 11;
            let mut p = impulse(n, n);
            gaussian(&mut p, n, n, sigma);
            let var = variance_x(&p, n, n);
            assert!((var.sqrt() - sigma).abs() / sigma < 0.04, "sigma {sigma}: got {}", var.sqrt());
            let sum: f32 = p.iter().sum();
            assert!((sum - 1000.0).abs() < 1.0, "mass kept: {sum}");
        }
    }

    #[test]
    fn surface_blur_matches_direct_formula() {
        let (w, h) = (41, 29);
        let img: Vec<u8> = (0..w * h).flat_map(|i| {
            let v = crate::util::hash3((i % w) as i64, (i / w) as i64, 4);
            let base = if i % w < 20 { 70 } else { 170 };
            [(base + (v & 63) as i32 - 31) as u8, (v >> 8) as u8, (v >> 16 & 127) as u8, 255]
        }).collect();
        for (r, t) in [(1usize, 5.0f32), (3, 12.0), (6, 40.0)] {
            let mut got = img.clone();
            surface(&mut got, w, h, r, t, (0, 0, w, h));
            let reach = 2.5 * t;
            for y in 0..h {
                for x in 0..w {
                    for c in 0..3 {
                        let v = img[(y * w + x) * 4 + c] as f32;
                        let (mut num, mut den) = (0f64, 0f64);
                        for dy in -(r as isize)..=r as isize {
                            for dx in -(r as isize)..=r as isize {
                                let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                                let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                                let q = img[(yy * w + xx) * 4 + c] as f32;
                                let wgt = (1.0 - (q - v).abs() / reach).max(0.0) as f64;
                                num += wgt * q as f64;
                                den += wgt;
                            }
                        }
                        let want = (num / den).round() as i32;
                        let have = got[(y * w + x) * 4 + c] as i32;
                        assert!((want - have).abs() <= 1, "r{r} t{t} ({x},{y},{c}): {have} vs {want}");
                    }
                }
            }
        }
    }

    #[test]
    fn lens_uniform_fast_path_matches_per_pixel_discs() {
        let (w, h) = (23, 17);
        let img: Vec<u8> = (0..w * h).flat_map(|i| {
            let v = crate::util::hash3(i as i64, 3, 9);
            [(v & 255) as u8, (v >> 8 & 255) as u8, (v >> 16 & 255) as u8, if i % 7 == 0 { 90 } else { 255 }]
        }).collect();
        for r in [1.5f32, 4.0, 6.3] {
            let (mut a, mut b) = (img.clone(), img.clone());
            lens(&mut a, w, h, r, (0, 0, w, h), true, |_, _| r);
            lens(&mut b, w, h, r, (0, 0, w, h), false, |_, _| r);
            let worst = a.iter().zip(&b).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap();
            assert!(worst <= 1, "radius {r}: differs by {worst}");
        }
    }

    #[test]
    fn motion_blur_spreads_along_angle() {
        let n = 61;
        let mut p = impulse(n, n);
        motion(&mut p, n, n, 0.0, 21.0);
        let c = n / 2;
        assert!(p[c * n + c + 9] > 30.0, "horizontal streak");
        assert!(p[(c + 5) * n + c] < 1.0, "no vertical spread");
        let mut q = impulse(n, n);
        motion(&mut q, n, n, 90.0, 21.0);
        assert!(q[(c + 9) * n + c] > 30.0, "vertical streak");
        let mut d = impulse(n, n);
        motion(&mut d, n, n, 45.0, 21.0);
        // 45° counter-clockwise: up-right in image coordinates.
        assert!(d[(c - 5) * n + c + 5] > 10.0 && d[(c + 5) * n + c + 5] < 1.0);
        let total: f32 = d.iter().sum();
        assert!((total - 1000.0).abs() < 1.0);
    }
}
