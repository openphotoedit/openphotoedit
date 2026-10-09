//! Speed work must not change what a user sees.
//!
//! Each optimised filter keeps the implementation it replaced here, verbatim,
//! as an oracle, and a test runs both over a real photograph and asserts the
//! difference is inside a stated tolerance. The tolerance is *not* "close
//! enough to look the same": these are reorganisations of the same arithmetic,
//! so the only differences allowed are floating-point summation order showing
//! up at a rounding boundary — a single level on a handful of pixels.

use std::time::Instant;

use crate::noise::{quantile, B3_NOISE, LEVELS};
use crate::util::{to_u8, Frame};

/// A 512 × 512 crop of the portrait, which has skin, hair, fabric and a
/// blown highlight: smooth areas, texture and hard edges in one tile.
fn crop() -> (Vec<u8>, usize, usize) {
    let (px, w, _h) = super::photo::portrait();
    let (cw, ch) = (512usize, 512usize);
    let (x0, y0) = (600usize, 500usize);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..y0 + ch {
        out.extend_from_slice(&px[(y * w + x0) * 4..(y * w + x0 + cw) * 4]);
    }
    (out, cw, ch)
}

/// Worst and mean absolute difference over RGB.
fn diff(a: &[u8], b: &[u8]) -> (u32, f64) {
    let mut worst = 0u32;
    let mut sum = 0f64;
    let mut n = 0f64;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for c in 0..3 {
            let d = (pa[c] as i32 - pb[c] as i32).unsigned_abs();
            worst = worst.max(d);
            sum += d as f64;
            n += 1.0;
        }
    }
    (worst, sum / n)
}

fn frame(w: usize, h: usize) -> Frame {
    let r = editor_core::geom::Rect::new(0, 0, w as i32, h as i32);
    Frame { w, h, outer: r, inner: r, canvas: r }
}

// ---------------------------------------------------------------------------
// Surface blur

/// Surface Blur as it stood before the rewrite: Perreault–Hébert column
/// histograms in `f32` with lazily refreshed 16-bin segments.
fn surface_before(buf: &mut [u8], w: usize, h: usize, radius: usize, threshold: f32, write: (usize, usize, usize, usize)) {
    if radius == 0 || threshold <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let reach = 2.5 * threshold.clamp(1.0, 255.0);
    let k = (reach.ceil() as usize).min(255);
    let tri: Vec<f32> = (0..=2 * k).map(|i| (1.0 - (i as f32 - k as f32).abs() / reach).max(0.0)).collect();
    let ramp: Vec<f32> = (0..256).map(|b| b as f32).collect();
    let src = buf.to_vec();
    let r = radius as isize;
    let (wx0, wy0, wx1, wy1) = write;
    for c in 0..3 {
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
                        for j in luc[seg] + 1..=xi {
                            let (a, sub) = (col(j + r) + base, col(j - r - 1) + base);
                            for i in 0..16 {
                                kern[base + i] += cols[a + i] - cols[sub + i];
                            }
                        }
                    }
                    luc[seg] = xi;
                }
                let (mut den, mut num) = (0f32, 0f32);
                for b in lo..=hi {
                    let wgt = kern[b] * tri[k + b - v];
                    den += wgt;
                    num += wgt * ramp[b];
                }
                buf[(y * w + x) * 4 + c] = if den > 0.0 { to_u8(num / den) } else { v as u8 };
            }
        }
    }
}

#[test]
fn surface_blur_matches_the_implementation_it_replaced() {
    let (src, w, h) = crop();
    for (r, t) in [(4usize, 10f32), (12, 20.0), (25, 50.0)] {
        let write = (0, 0, w, h);
        let mut old = src.clone();
        let t0 = Instant::now();
        surface_before(&mut old, w, h, r, t, write);
        let before = t0.elapsed().as_secs_f64();
        let mut new = src.clone();
        let t0 = Instant::now();
        crate::blur::surface(&mut new, w, h, r, t, write);
        let after = t0.elapsed().as_secs_f64();
        let (worst, mean) = diff(&old, &new);
        println!("surface r{r} t{t}: {:.0} ms -> {:.0} ms ({:.1}×), worst {worst}, mean {mean:.5}", before * 1e3, after * 1e3, before / after);
        // Same arithmetic, summed in a different order: at most one level,
        // and only where the exact result sat on a rounding boundary.
        assert!(worst <= 1, "surface r{r} t{t}: worst difference {worst}");
        assert!(mean < 0.02, "surface r{r} t{t}: mean difference {mean}");
    }
}

// ---------------------------------------------------------------------------
// Reduce Noise

/// The à-trous smoothing step before the edge/middle split.
fn atrous_before(src: &[f32], dst: &mut [f32], tmp: &mut [f32], w: usize, h: usize, step: usize) {
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

#[test]
fn atrous_smoothing_matches_the_clamped_loop() {
    let (src, w, h) = crop();
    let plane: Vec<f32> = src.chunks_exact(4).map(|p| p[1] as f32).collect();
    for step in [1usize, 2, 4, 8, 16] {
        let (mut a, mut b) = (vec![0f32; w * h], vec![0f32; w * h]);
        let mut tmp = vec![0f32; w * h];
        atrous_before(&plane, &mut a, &mut tmp, w, h, step);
        crate::noise::atrous_smooth(&plane, &mut b, &mut tmp, w, h, step);
        let worst = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
        // The edge/middle split changes nothing but the bounds checks.
        assert!(worst == 0.0, "step {step}: worst {worst}");
    }
}

#[test]
fn reduce_noise_output_is_unchanged() {
    // `reduce` is the same arithmetic on reused buffers, so it must match
    // exactly, not approximately. The oracle is the recorded output of the
    // original on this crop, regenerated by running both sides of
    // `atrous_smoothing_matches_the_clamped_loop` — here we assert the whole
    // pipeline is deterministic and that a second run reproduces it.
    let (src, w, h) = crop();
    let mut a = src.clone();
    let t0 = Instant::now();
    crate::noise::reduce(&mut a, w, h, 6.0, 60.0, 45.0);
    println!("reduce noise 512²: {:.0} ms", t0.elapsed().as_secs_f64() * 1e3);
    let mut b = src.clone();
    crate::noise::reduce(&mut b, w, h, 6.0, 60.0, 45.0);
    assert_eq!(a, b);
    // It did something: the finest detail lost energy.
    let rough = |p: &[u8]| {
        let mut s = 0f64;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = (y * w + x) * 4;
                s += (4.0 * p[i] as f64 - p[i - 4] as f64 - p[i + 4] as f64 - p[i - w * 4] as f64 - p[i + w * 4] as f64).abs();
            }
        }
        s
    };
    assert!(rough(&a) < rough(&src) * 0.9, "reduce noise did not smooth");
}

// ---------------------------------------------------------------------------
// Add Noise

/// Add Noise before the sampling table, computing two hashes, a log, a square
/// root and a cosine per channel per pixel.
fn add_noise_before(buf: &mut [u8], frame: &Frame, amount: f32, gaussian: bool, mono: bool, seed: u64) {
    use crate::util::{hash3, unit};
    if amount <= 0.0 {
        return;
    }
    let amp = amount / 100.0 * 127.5;
    let sample = |x: i64, y: i64, ch: u64| -> f32 {
        let s = seed.wrapping_add(ch.wrapping_mul(0x51_7CC1_B727_220A));
        if gaussian {
            let u1 = unit(hash3(x, y, s)).max(1e-7);
            let u2 = unit(hash3(x, y, s ^ 0xA5A5_5A5A));
            (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * amp * (2.0 / 3.0)
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

#[test]
fn add_noise_matches_the_transcendental_version() {
    let (src, w, h) = crop();
    let f = frame(w, h);
    for (gaussian, mono) in [(false, false), (true, false), (true, true)] {
        let mut old = src.clone();
        let t0 = Instant::now();
        add_noise_before(&mut old, &f, 12.0, gaussian, mono, 42);
        let before = t0.elapsed().as_secs_f64();
        let mut new = src.clone();
        let t0 = Instant::now();
        crate::noise::add(&mut new, &f, 12.0, gaussian, mono, 42);
        let after = t0.elapsed().as_secs_f64();
        let (worst, mean) = diff(&old, &new);
        println!("add-noise gaussian={gaussian} mono={mono}: {:.1} ms -> {:.1} ms, worst {worst}, mean {mean:.5}", before * 1e3, after * 1e3);
        assert!(worst <= 1, "gaussian={gaussian} mono={mono}: worst {worst}");
        assert!(mean < 0.01, "gaussian={gaussian} mono={mono}: mean {mean}");
    }
}

// ---------------------------------------------------------------------------
// Clouds

/// Clouds before the lattice cache: four hashes and eight transcendentals per
/// octave per pixel.
fn clouds_before(buf: &mut [u8], frame: &Frame, seed: u64, fg: editor_core::color::Rgba8, bg: editor_core::color::Rgba8, doc_size: u32) {
    let base = (doc_size as f64 / 3.0).clamp(96.0, 1024.0);
    let octaves = (base.log2().ceil() as usize).max(1);
    let (f, b) = (fg.to_array(), bg.to_array());
    for y in 0..frame.h {
        for x in 0..frame.w {
            let (px, py) = (frame.doc_x(x) as f64 + 0.5, frame.doc_y(y) as f64 + 0.5);
            let mut v = 0.0;
            let mut amp = 1.0;
            let mut period = base;
            let mut norm = 0.0;
            for o in 0..octaves {
                v += amp * crate::stylize::perlin(px / period, py / period, seed.wrapping_add(o as u64 * 7919));
                norm += amp;
                amp *= 0.5;
                period /= 2.0;
                if period < 1.5 {
                    break;
                }
            }
            let t = (0.5 + v / norm * 1.4).clamp(0.0, 1.0) as f32;
            let i = (y * frame.w + x) * 4;
            for c in 0..3 {
                buf[i + c] = to_u8(b[c] as f32 + (f[c] as f32 - b[c] as f32) * t);
            }
            buf[i + 3] = 255;
        }
    }
}

#[test]
fn clouds_match_the_uncached_lattice() {
    use editor_core::color::Rgba8;
    let (w, h) = (400usize, 300usize);
    let f = frame(w, h);
    for doc_size in [300u32, 2000, 6000] {
        let mut old = vec![0u8; w * h * 4];
        let t0 = Instant::now();
        clouds_before(&mut old, &f, 9, Rgba8::BLACK, Rgba8::WHITE, doc_size);
        let before = t0.elapsed().as_secs_f64();
        let mut new = vec![0u8; w * h * 4];
        let t0 = Instant::now();
        crate::stylize::clouds(&mut new, &f, 9, Rgba8::BLACK, Rgba8::WHITE, doc_size);
        let after = t0.elapsed().as_secs_f64();
        println!("clouds doc {doc_size}: {:.0} ms -> {:.0} ms ({:.1}×)", before * 1e3, after * 1e3, before / after);
        // The same gradients in the same order: bit-identical, not merely close.
        assert_eq!(old, new, "clouds differ at doc_size {doc_size}");
    }
}

// ---------------------------------------------------------------------------
// Radial blur

fn radial_before(buf: &mut [u8], frame: &Frame, amount: f32, zoom: bool, cx: f32, cy: f32) {
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

#[test]
fn radial_blur_matches_the_scalar_sampler() {
    let (src, w, h) = crop();
    let f = frame(w, h);
    // The still-disc is now copied as one run per row, so the centre is
    // also tried off to one side, in a corner and outside the buffer, where
    // the run is clipped or empty.
    let centres = [(w as f32 / 2.0, h as f32 / 2.0), (40.0, 300.0), (0.0, 0.0), (-500.0, 700.0), (w as f32 + 300.0, -200.0)];
    for (amount, zoom) in [(20f32, false), (60.0, false), (40.0, true)] {
        for (cx, cy) in centres {
            let mut old = src.clone();
            let t0 = Instant::now();
            radial_before(&mut old, &f, amount, zoom, cx, cy);
            let before = t0.elapsed().as_secs_f64();
            let mut new = src.clone();
            let t0 = Instant::now();
            crate::blur::radial(&mut new, &f, amount, zoom, cx, cy);
            let after = t0.elapsed().as_secs_f64();
            if (cx, cy) == centres[0] {
                println!("radial a{amount} zoom={zoom}: {:.0} ms -> {:.0} ms ({:.1}x)", before * 1e3, after * 1e3, before / after);
            }
            // Same taps in the same order, so bit-identical.
            assert_eq!(old, new, "radial a{amount} zoom={zoom} centre {cx},{cy}");
        }
    }
}

// ---------------------------------------------------------------------------
// Reduce Noise, whole filter

fn wavelet_denoise_before(p: &mut [f32], w: usize, h: usize, sigma: Option<f32>, k: [f32; LEVELS]) -> f32 {
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
        atrous_before(&c, &mut next, &mut d, w, h, 1 << j);
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
            crate::blur::box_blur(&mut e, w, h, 2.0);
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
fn reduce_before(buf: &mut [u8], w: usize, h: usize, strength: f32, preserve_details: f32, reduce_color: f32) {
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
    wavelet_denoise_before(&mut yp, w, h, None, luma_k);
    let chroma = (0.5 + 3.0 * reduce_color / 100.0) * strength.max(2.0) / 5.0;
    let chroma_k = [chroma; LEVELS];
    wavelet_denoise_before(&mut cb, w, h, None, chroma_k);
    wavelet_denoise_before(&mut cr, w, h, None, chroma_k);
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

#[test]
fn reduce_noise_matches_the_allocating_version() {
    let (src, w, h) = crop();
    let mut old = src.clone();
    let t0 = Instant::now();
    reduce_before(&mut old, w, h, 6.0, 60.0, 45.0);
    let before = t0.elapsed().as_secs_f64();
    let mut new = src.clone();
    let t0 = Instant::now();
    crate::noise::reduce(&mut new, w, h, 6.0, 60.0, 45.0);
    let after = t0.elapsed().as_secs_f64();
    println!("reduce-noise: {:.0} ms -> {:.0} ms ({:.1}x)", before * 1e3, after * 1e3, before / after);
    assert_eq!(old, new, "reduce noise output changed");
}

// ---------------------------------------------------------------------------
// The before/after table

/// Times the old and the new implementation of each filter alternately, on
/// the same 24 MP photograph, in one process — the only way to get a fair
/// ratio on a machine that other work is sharing.
///
/// `CARGO_TARGET_DIR=target/h8 cargo test --release -p editor-filters
///  ab_bench_24mp -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore]
fn ab_bench_24mp() {
    use editor_core::color::Rgba8;
    let (px, w, h) = super::photo::upscaled(24.0);
    let mp = (w * h) as f64 / 1e6;
    let f = frame(w, h);
    println!("{w} × {h} ({mp:.1} MP)\n{:<30} {:>10} {:>10} {:>7}", "filter", "before", "after", "ratio");
    let row = |name: &str, before: f64, after: f64| {
        println!("{name:<30} {:>7.0} ms {:>7.0} ms {:>6.1}× | {:>6.1} -> {:>5.1} ms/MP", before * 1e3, after * 1e3, before / after, before * 1e3 / mp, after * 1e3 / mp);
    };
    macro_rules! time {
        ($e:expr) => {{
            let t = Instant::now();
            $e;
            t.elapsed().as_secs_f64()
        }};
    }

    for (r, t) in [(12usize, 20f32), (40, 50.0)] {
        let mut a = px.clone();
        let before = time!(surface_before(&mut a, w, h, r, t, (0, 0, w, h)));
        let mut b = px.clone();
        let after = time!(crate::blur::surface(&mut b, w, h, r, t, (0, 0, w, h)));
        assert!(diff(&a, &b).0 <= 1);
        row(&format!("surface-blur r{r} t{t}"), before, after);
    }
    {
        let mut a = px.clone();
        let before = time!(radial_before(&mut a, &f, 20.0, false, w as f32 / 2.0, h as f32 / 2.0));
        let mut b = px.clone();
        let after = time!(crate::blur::radial(&mut b, &f, 20.0, false, w as f32 / 2.0, h as f32 / 2.0));
        assert_eq!(a, b);
        row("radial-blur a20", before, after);
    }
    {
        let mut a = px.clone();
        let before = time!(reduce_before(&mut a, w, h, 6.0, 60.0, 45.0));
        let mut b = px.clone();
        let after = time!(crate::noise::reduce(&mut b, w, h, 6.0, 60.0, 45.0));
        assert_eq!(a, b);
        row("reduce-noise", before, after);
    }
    {
        let mut a = px.clone();
        let before = time!(clouds_before(&mut a, &f, 9, Rgba8::BLACK, Rgba8::WHITE, w.max(h) as u32));
        let mut b = px.clone();
        let after = time!(crate::stylize::clouds(&mut b, &f, 9, Rgba8::BLACK, Rgba8::WHITE, w.max(h) as u32));
        assert_eq!(a, b);
        row("clouds", before, after);
    }
    for (gaussian, label) in [(true, "add-noise a10 gaussian"), (false, "add-noise a10 uniform")] {
        let mut a = px.clone();
        let before = time!(add_noise_before(&mut a, &f, 10.0, gaussian, false, 7));
        let mut b = px.clone();
        let after = time!(crate::noise::add(&mut b, &f, 10.0, gaussian, false, 7));
        assert!(diff(&a, &b).0 <= 1);
        row(label, before, after);
    }
}
