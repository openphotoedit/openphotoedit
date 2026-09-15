//! Sharpening: Unsharp Mask, Smart Sharpen, High Pass.

use crate::blur::gaussian;
use crate::noise::{immerkaer_sigma, min_max_plane};
use crate::util::{is_opaque, to_u8};

/// Straight-colour Gaussian blur of RGB, alpha-weighted so transparent
/// neighbours do not pull colour toward black. Returns three planes 0..255.
pub fn blurred_rgb(buf: &[u8], w: usize, h: usize, sigma: f32) -> [Vec<f32>; 3] {
    if is_opaque(buf) {
        return std::array::from_fn(|c| {
            let mut p: Vec<f32> = buf.chunks_exact(4).map(|px| px[c] as f32).collect();
            gaussian(&mut p, w, h, sigma);
            p
        });
    }
    let mut a: Vec<f32> = buf.chunks_exact(4).map(|px| px[3] as f32 / 255.0).collect();
    let planes: [Vec<f32>; 3] = std::array::from_fn(|c| {
        let mut p: Vec<f32> = buf.chunks_exact(4).map(|px| px[c] as f32 * px[3] as f32 / 255.0).collect();
        gaussian(&mut p, w, h, sigma);
        p
    });
    gaussian(&mut a, w, h, sigma);
    let mut planes = planes;
    for (i, &av) in a.iter().enumerate() {
        for p in planes.iter_mut() {
            p[i] = if av > 1e-4 { p[i] / av } else { 0.0 };
        }
    }
    planes
}

/// Photoshop's Unsharp Mask: `original + amount · (original − blur)` per
/// channel, skipped where the difference is below `threshold` levels.
pub fn unsharp(buf: &mut [u8], w: usize, h: usize, amount: f32, sigma: f32, threshold: f32) {
    if amount <= 0.0 || sigma <= 0.0 {
        return;
    }
    let k = amount / 100.0;
    let blurred = blurred_rgb(buf, w, h, sigma);
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        for c in 0..3 {
            let v = px[c] as f32;
            let d = v - blurred[c][i];
            if threshold <= 0.0 || d.abs() >= threshold {
                px[c] = to_u8(v + k * d);
            }
        }
    }
}

/// High Pass: detail above `radius` on a 50 % grey base.
pub fn high_pass(buf: &mut [u8], w: usize, h: usize, sigma: f32) {
    if sigma <= 0.0 {
        // Photoshop's minimum radius still removes nearly everything.
        for px in buf.chunks_exact_mut(4) {
            px[..3].fill(128);
        }
        return;
    }
    let blurred = blurred_rgb(buf, w, h, sigma);
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        for c in 0..3 {
            px[c] = to_u8(px[c] as f32 - blurred[c][i] + 128.0);
        }
    }
}

/// Smart Sharpen ("Remove: Gaussian Blur").
///
/// Works on luminance so edges do not grow colour fringes. The detail band
/// `Y − G(Y)` is soft-thresholded against the image's own noise level
/// (scaled by `reduce_noise`), added back with `amount`, and the result is
/// held within a slightly widened local min/max so edges do not ring into
/// halos the way plain Unsharp Mask does.
pub fn smart(buf: &mut [u8], w: usize, h: usize, amount: f32, sigma: f32, reduce_noise: f32) {
    if amount <= 0.0 || sigma <= 0.0 || w < 3 || h < 3 {
        return;
    }
    let y: Vec<f32> = buf.chunks_exact(4).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect();
    let mut blurred = y.clone();
    gaussian(&mut blurred, w, h, sigma);
    let noise = immerkaer_sigma(&y, w, h, Some(0.1));
    let t = reduce_noise / 100.0 * 2.0 * noise;
    let k = amount / 100.0;
    // Overshoot limit: the local range around each pixel, widened by a
    // fraction of itself.
    let reach = (sigma.ceil() as usize).clamp(1, 8);
    let mut lo = y.clone();
    let mut hi = y.clone();
    min_max_plane(&mut lo, w, h, reach, false);
    min_max_plane(&mut hi, w, h, reach, true);
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        let d = y[i] - blurred[i];
        let d = if d > t {
            d - t
        } else if d < -t {
            d + t
        } else {
            0.0
        };
        if d == 0.0 {
            continue;
        }
        let range = hi[i] - lo[i];
        let target = (y[i] + k * d).clamp(lo[i] - 0.15 * range - 2.0, hi[i] + 0.15 * range + 2.0);
        let delta = target - y[i];
        for c in 0..3 {
            px[c] = to_u8(px[c] as f32 + delta);
        }
    }
}
