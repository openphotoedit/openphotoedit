//! Stylize, Pixelate, Render and Other: Mosaic, Emboss, Find Edges,
//! Solarize, Invert, Desaturate, Custom, Offset, Clouds, Vignette, and
//! destructive adjustments.

use editor_core::adjust::{Adjustment, ApplyCtx};
use editor_core::color::{linear_to_srgb, srgb_to_linear, Rgba8};
use editor_core::geom::Rect;

use crate::util::{hash3, is_opaque, put_premultiplied, to_u8, Frame, Sampler};

/// A per-channel lookup on RGB; alpha untouched.
pub fn point(buf: &mut [u8], f: impl Fn(u8) -> u8) {
    let lut: Vec<u8> = (0..=255u8).map(&f).collect();
    for px in buf.chunks_exact_mut(4) {
        for c in 0..3 {
            px[c] = lut[px[c] as usize];
        }
    }
}

/// Photoshop's Desaturate: HSL lightness, `(max + min) / 2`.
pub fn desaturate(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let mx = px[0].max(px[1]).max(px[2]) as u16;
        let mn = px[0].min(px[1]).min(px[2]) as u16;
        let l = (mx + mn).div_ceil(2) as u8;
        px[..3].fill(l);
    }
}

/// Mosaic: every `cell`×`cell` block (aligned to the filtered area) becomes
/// its alpha-weighted mean. The block's pixels are overwritten, so nothing
/// of the original detail remains to be recovered.
pub fn pixelate(buf: &mut [u8], w: usize, h: usize, cell: usize) {
    if cell <= 1 {
        return;
    }
    for by in (0..h).step_by(cell) {
        for bx in (0..w).step_by(cell) {
            let (x1, y1) = ((bx + cell).min(w), (by + cell).min(h));
            let mut acc = [0f64; 4];
            let n = ((x1 - bx) * (y1 - by)) as f64;
            for y in by..y1 {
                for x in bx..x1 {
                    let p = &buf[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    let a = p[3] as f64;
                    acc[0] += p[0] as f64 * a;
                    acc[1] += p[1] as f64 * a;
                    acc[2] += p[2] as f64 * a;
                    acc[3] += a;
                }
            }
            let out = if acc[3] > 0.0 {
                [(acc[0] / acc[3]).round() as u8, (acc[1] / acc[3]).round() as u8, (acc[2] / acc[3]).round() as u8, (acc[3] / n).round() as u8]
            } else {
                [0, 0, 0, 0]
            };
            for y in by..y1 {
                for x in bx..x1 {
                    buf[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&out);
                }
            }
        }
    }
}

/// Emboss: grey where flat, relief where the image changes along `angle`.
/// Per channel, as Photoshop does, so strong colour edges keep a tint.
pub fn emboss(buf: &mut [u8], frame: &Frame, angle: f32, height: f32, amount: f32) {
    let (w, h) = (frame.w, frame.h);
    let src = buf.to_vec();
    let s = Sampler { src: &src, w, h };
    let th = angle.to_radians();
    // Light comes from `angle`: sample towards it and away from it.
    let (dx, dy) = (th.cos() * height / 2.0, -th.sin() * height / 2.0);
    let k = amount / 100.0;
    let inner = frame.inner;
    for y in inner.y as usize..inner.bottom() as usize {
        for x in inner.x as usize..inner.right() as usize {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let a = s.sample(cx + dx, cy + dy);
            let b = s.sample(cx - dx, cy - dy);
            let i = (y * w + x) * 4;
            let alpha = src[i + 3] as f32;
            let unpre = |p: [f32; 4], c: usize| if p[3] > 0.5 { p[c] * 255.0 / p[3] } else { 0.0 };
            for c in 0..3 {
                buf[i + c] = to_u8(128.0 + (unpre(a, c) - unpre(b, c)) * k);
            }
            buf[i + 3] = alpha as u8;
        }
    }
}

/// Find Edges: dark coloured lines on white, from per-channel Sobel
/// gradients.
pub fn find_edges(buf: &mut [u8], w: usize, h: usize) {
    let src = buf.to_vec();
    let at = |x: isize, y: isize, c: usize| -> f32 {
        let xx = x.clamp(0, w as isize - 1) as usize;
        let yy = y.clamp(0, h as isize - 1) as usize;
        src[(yy * w + xx) * 4 + c] as f32
    };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = (y as usize * w + x as usize) * 4;
            for c in 0..3 {
                let gx = at(x + 1, y - 1, c) + 2.0 * at(x + 1, y, c) + at(x + 1, y + 1, c) - at(x - 1, y - 1, c) - 2.0 * at(x - 1, y, c) - at(x - 1, y + 1, c);
                let gy = at(x - 1, y + 1, c) + 2.0 * at(x, y + 1, c) + at(x + 1, y + 1, c) - at(x - 1, y - 1, c) - 2.0 * at(x, y - 1, c) - at(x + 1, y - 1, c);
                let mag = (gx * gx + gy * gy).sqrt() / 2.0;
                buf[i + c] = to_u8(255.0 - mag);
            }
        }
    }
}

/// Custom: an odd square kernel, `sum(k · p) / scale + offset`, straight
/// colour for opaque pixels and premultiplied where there is transparency.
pub fn custom(buf: &mut [u8], w: usize, h: usize, kernel: &[f32], n: usize, scale: f32, offset: f32) {
    let r = (n / 2) as isize;
    let src = buf.to_vec();
    let opaque = is_opaque(&src);
    let s = Sampler { src: &src, w, h };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let mut acc = [0f32; 4];
            for ky in 0..n as isize {
                let yy = (y + ky - r).clamp(0, h as isize - 1) as usize;
                for kx in 0..n as isize {
                    let kw = kernel[(ky as usize) * n + kx as usize];
                    if kw == 0.0 {
                        continue;
                    }
                    let xx = (x + kx - r).clamp(0, w as isize - 1) as usize;
                    let p = s.premul(xx, yy);
                    for c in 0..4 {
                        acc[c] += kw * p[c];
                    }
                }
            }
            let i = (y as usize * w + x as usize) * 4;
            if opaque {
                for c in 0..3 {
                    buf[i + c] = to_u8(acc[c] / scale + offset);
                }
            } else {
                let a = (acc[3] / scale).clamp(0.0, 255.0);
                let mut p = [0f32; 4];
                for c in 0..3 {
                    p[c] = (acc[c] / scale + offset * a / 255.0).clamp(0.0, a);
                }
                p[3] = a;
                put_premultiplied(&mut buf[i..i + 4], p);
            }
        }
    }
}

/// Offset: shift the area by `(dx, dy)`. With `wrap` the pixels pushed off
/// one edge come back on the other; otherwise the vacated area becomes
/// transparent.
pub fn offset(buf: &mut [u8], w: usize, h: usize, dx: i32, dy: i32, wrap: bool) {
    let src = buf.to_vec();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let (mut sx, mut sy) = (x - dx as i64, y - dy as i64);
            let i = (y as usize * w + x as usize) * 4;
            if wrap {
                sx = sx.rem_euclid(w as i64);
                sy = sy.rem_euclid(h as i64);
            } else if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                buf[i..i + 4].fill(0);
                continue;
            }
            let j = (sy as usize * w + sx as usize) * 4;
            buf[i..i + 4].copy_from_slice(&src[j..j + 4]);
        }
    }
}

/// Gradient noise at a lattice-scaled position.
fn perlin(x: f64, y: f64, seed: u64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (xi, yi) = (x0 as i64, y0 as i64);
    let grad = |ix: i64, iy: i64, dx: f64, dy: f64| {
        let a = (hash3(ix, iy, seed) >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
        a.cos() * dx + a.sin() * dy
    };
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v) = (fade(fx), fade(fy));
    let n00 = grad(xi, yi, fx, fy);
    let n10 = grad(xi + 1, yi, fx - 1.0, fy);
    let n01 = grad(xi, yi + 1, fx, fy - 1.0);
    let n11 = grad(xi + 1, yi + 1, fx - 1.0, fy - 1.0);
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    a + (b - a) * v
}

/// Clouds: fractal gradient noise between the foreground and background
/// colours, a function of document position (the same pattern however the
/// canvas is selected), fully opaque.
pub fn clouds(buf: &mut [u8], frame: &Frame, seed: u64, fg: Rgba8, bg: Rgba8, doc_size: u32) {
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
                v += amp * perlin(px / period, py / period, seed.wrapping_add(o as u64 * 7919));
                norm += amp;
                amp *= 0.5;
                period /= 2.0;
                if period < 1.5 {
                    break;
                }
            }
            // Perlin output sits mostly within ±0.5 of the normaliser.
            let t = (0.5 + v / norm * 1.4).clamp(0.0, 1.0) as f32;
            let i = (y * frame.w + x) * 4;
            for c in 0..3 {
                buf[i + c] = to_u8(b[c] as f32 + (f[c] as f32 - b[c] as f32) * t);
            }
            buf[i + 3] = 255;
        }
    }
}

/// Vignette: darken (negative) or lighten (positive) toward the corners of
/// `area`. `midpoint` sets where the falloff starts, `feather` how gradual it
/// is. Darkening happens in linear light, like a lens.
pub fn vignette(buf: &mut [u8], frame: &Frame, area: Rect, amount: f32, midpoint: f32, feather: f32) {
    if amount == 0.0 || area.is_empty() {
        return;
    }
    let (cx, cy) = (area.x as f32 + area.w as f32 / 2.0, area.y as f32 + area.h as f32 / 2.0);
    let (hw, hh) = (area.w as f32 / 2.0, area.h as f32 / 2.0);
    let start = 0.15 + 0.75 * midpoint / 100.0;
    let width = 0.05 + 0.9 * feather / 100.0;
    let a = amount / 100.0;
    let to_lin: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    for y in 0..frame.h {
        for x in 0..frame.w {
            let nx = (frame.doc_x(x) as f32 + 0.5 - cx) / hw;
            let ny = (frame.doc_y(y) as f32 + 0.5 - cy) / hh;
            let d = ((nx * nx + ny * ny) / 2.0).sqrt();
            let t = ((d - start) / width).clamp(0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            if s <= 0.0 {
                continue;
            }
            let i = (y * frame.w + x) * 4;
            for c in 0..3 {
                let v = buf[i + c];
                buf[i + c] = if a < 0.0 {
                    to_u8(linear_to_srgb(to_lin[v as usize] * (1.0 + a * s)) * 255.0)
                } else {
                    let f = v as f32;
                    to_u8(f + (255.0 - f) * a * s)
                };
            }
        }
    }
}

/// Image › Adjustments applied destructively, through the same maths as the
/// adjustment layer.
pub fn apply_adjustment(buf: &mut [u8], frame: &Frame, adj: &Adjustment, doc_w: u32, doc_h: u32) {
    let mut f: Vec<f32> = buf.iter().map(|&v| v as f32 / 255.0).collect();
    let ctx = ApplyCtx { x0: frame.outer.x as f64, y0: frame.outer.y as f64, step: 1.0, doc_w, doc_h, width: frame.w, height: frame.h };
    adj.apply(&mut f, &ctx);
    for (i, v) in f.iter().enumerate() {
        if i % 4 != 3 {
            buf[i] = to_u8(v * 255.0);
        }
    }
}
