//! Scene-referred tone mapping, inspired by darktable's sigmoid module.
//!
//! A generalised log-logistic curve `y = W·xᵍ / (xᵍ + Kᵍ)` maps scene-linear
//! light to display-linear. `K` and `W` are solved so that middle grey stays
//! at 0.18 and a chosen number of stops above grey reaches display white, so
//! the contrast slider only changes the slope through grey. Channels are
//! mapped independently (which desaturates highlights the way film does) and
//! the original hue is then restored.

use crate::color::REC2020_LUMA;
use crate::filters;

pub const GREY: f32 = 0.18;

#[derive(Debug, Clone, Copy)]
pub struct Sigmoid {
    g: f32,
    k: f32,
    w: f32,
}

impl Sigmoid {
    /// `contrast` is the slope exponent (≈1.6 is a pleasing default);
    /// `white_ev` is how many stops above grey reach display white.
    pub fn new(contrast: f32, white_ev: f32) -> Self {
        let g = contrast.clamp(0.6, 4.0) as f64;
        let grey = GREY as f64;
        let xw = grey * 2f64.powf(white_ev.clamp(1.0, 12.0) as f64);
        let a = grey.powf(g);
        let b = xw.powf(g);
        let den = grey * b - a;
        // K^g and W from the two constraints (see module docs).
        let kg = if den > 0.0 { (1.0 - grey) * a * b / den } else { a };
        let w = grey * (a + kg) / a;
        Sigmoid { g: g as f32, k: kg.powf(1.0 / g) as f32, w: w as f32 }
    }

    #[inline]
    pub fn map(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        let xg = x.powf(self.g);
        let kg = self.k.powf(self.g);
        (self.w * xg / (xg + kg)).min(1.0)
    }
}

/// A lookup over log2 scene exposure for speed; exact within 0.1%.
pub struct SigmoidLut {
    lo: f32,
    step: f32,
    table: Vec<f32>,
    curve: Sigmoid,
}

impl SigmoidLut {
    const LO_EV: f32 = -16.0;
    const HI_EV: f32 = 8.0;
    const N: usize = 4096;

    pub fn new(curve: Sigmoid) -> Self {
        let lo = Self::LO_EV;
        let step = (Self::HI_EV - Self::LO_EV) / (Self::N - 1) as f32;
        let table = (0..Self::N).map(|i| curve.map(GREY * 2f32.powf(lo + step * i as f32))).collect();
        SigmoidLut { lo, step, table, curve }
    }

    #[inline]
    pub fn map(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        let ev = (x / GREY).log2();
        let f = (ev - self.lo) / self.step;
        if f <= 0.0 {
            return self.curve.map(x);
        }
        let i = f as usize;
        if i >= Self::N - 1 {
            return self.table[Self::N - 1];
        }
        let t = f - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * t
    }
}

/// Pull a pixel with negative components back to the gamut boundary by
/// desaturating towards its luminance.
#[inline]
pub fn clip_negative(p: [f32; 3], luma: [f32; 3]) -> [f32; 3] {
    let mn = p[0].min(p[1]).min(p[2]);
    if mn >= 0.0 {
        return p;
    }
    let y = p[0] * luma[0] + p[1] * luma[1] + p[2] * luma[2];
    if y <= 0.0 {
        return [0.0; 3];
    }
    let t = y / (y - mn);
    [y + (p[0] - y) * t, y + (p[1] - y) * t, y + (p[2] - y) * t]
}

/// Per-channel map followed by restoring the input's hue: the middle channel
/// keeps its relative position between the smallest and largest.
#[inline]
pub fn map_preserving_hue(p: [f32; 3], f: impl Fn(f32) -> f32) -> [f32; 3] {
    let p = clip_negative(p, REC2020_LUMA);
    let mut o = [f(p[0]), f(p[1]), f(p[2])];
    let (mut imax, mut imin) = (0, 0);
    for c in 1..3 {
        if p[c] > p[imax] {
            imax = c;
        }
        if p[c] < p[imin] {
            imin = c;
        }
    }
    if imax != imin {
        let imid = 3 - imax - imin;
        let span = p[imax] - p[imin];
        if span > 1e-9 {
            let ratio = (p[imid] - p[imin]) / span;
            o[imid] = o[imin] + (o[imax] - o[imin]) * ratio;
        }
    }
    o
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Local exposure changes from an edge-preserving base layer of log
/// luminance: compress bright regions (`highlights` < 0), lift dark regions
/// (`shadows` > 0). `default_compression` is always applied, gently, so the
/// default rendering keeps skies and highlights.
pub fn local_tone(data: &mut [f32], w: usize, h: usize, highlights: f32, shadows: f32, default_compression: f32, scale: f32) {
    use rayon::prelude::*;
    let hl = highlights.clamp(-1.0, 1.0) * 0.45 - default_compression;
    let sh = shadows.clamp(-1.0, 1.0) * 0.45;
    if hl.abs() < 1e-4 && sh.abs() < 1e-4 {
        return;
    }
    let lg: Vec<f32> = data
        .par_chunks(3)
        .map(|p| (p[0] * REC2020_LUMA[0] + p[1] * REC2020_LUMA[1] + p[2] * REC2020_LUMA[2]).max(1e-6).log2())
        .collect();
    // Radius tied to the full-resolution image so previews match the result.
    let r = ((w.max(h) as f32 / scale) * 0.03 * scale).max(4.0) as usize;
    let sub = if w.max(h) > 2000 { 4 } else { 2 };
    let base = filters::guided(&lg, &lg, w, h, r, 0.35, sub);
    let grey = GREY.log2();
    data.par_chunks_mut(3).zip(base.par_iter()).for_each(|(p, &b)| {
        let d = b - grey;
        let up = smoothstep(0.5, 3.5, d) * d.max(0.0);
        let down = smoothstep(0.5, 4.0, -d) * (-d).max(0.0);
        let ev = hl * up + sh * down;
        if ev != 0.0 {
            let g = 2f32.powf(ev);
            p[0] *= g;
            p[1] *= g;
            p[2] *= g;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_is_anchored_and_white_reached() {
        for &c in &[1.0, 1.6, 2.4] {
            let s = Sigmoid::new(c, 6.0);
            assert!((s.map(GREY) - GREY).abs() < 1e-3, "contrast {c}");
            assert!((s.map(GREY * 64.0) - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn curve_is_monotonic() {
        let s = SigmoidLut::new(Sigmoid::new(1.6, 6.0));
        let mut prev = -1.0;
        for i in 0..20_000 {
            let x = 1e-6 * 1.001f32.powi(i);
            let y = s.map(x);
            assert!(y >= prev, "not monotonic at {x}: {y} < {prev}");
            assert!((0.0..=1.0).contains(&y));
            prev = y;
        }
    }

    #[test]
    fn lut_matches_exact() {
        let c = Sigmoid::new(1.7, 5.5);
        let l = SigmoidLut::new(c);
        for i in 0..200 {
            let x = GREY * 2f32.powf(-10.0 + i as f32 * 0.08);
            assert!((c.map(x) - l.map(x)).abs() < 2e-3);
        }
    }

    #[test]
    fn hue_preserved() {
        let s = Sigmoid::new(1.6, 6.0);
        let p = [0.9, 0.5, 0.1];
        let o = map_preserving_hue(p, |x| s.map(x));
        let ratio_in = (p[1] - p[2]) / (p[0] - p[2]);
        let ratio_out = (o[1] - o[2]) / (o[0] - o[2]);
        assert!((ratio_in - ratio_out).abs() < 1e-4);
        assert!(o[0] > o[1] && o[1] > o[2]);
    }
}
