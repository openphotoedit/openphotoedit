//! Per-pixel tool effects: dodge, burn, sponge, and the membrane solver the
//! healing brush uses to match the cloned texture to its surroundings.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Range {
    Shadows,
    #[default]
    Midtones,
    Highlights,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Sponge {
    Saturate,
    #[default]
    Desaturate,
}

/// Full-strength dodge (`burn = false`) or burn of one channel value 0..1,
/// weighted to the tonal range the way Photoshop's tools are: midtones move
/// by a gamma that leaves black and white alone, shadows and highlights
/// move most at their own end of the scale.
#[inline]
pub fn dodge_burn(v: f32, range: Range, burn: bool) -> f32 {
    let v = v.clamp(0.0, 1.0);
    let out = match (range, burn) {
        (Range::Midtones, false) => v.powf(1.0 / 1.8),
        (Range::Midtones, true) => v.powf(1.8),
        (Range::Highlights, false) => v + 0.6 * v * v * (1.0 - v) * 2.0,
        (Range::Highlights, true) => v - 0.6 * v * v,
        (Range::Shadows, false) => v + 0.6 * (1.0 - v) * (1.0 - v),
        (Range::Shadows, true) => v - 0.6 * v * (1.0 - v) * (1.0 - v) * 2.0,
    };
    out.clamp(0.0, 1.0)
}

/// Full-strength sponge on an RGB triple 0..1: saturation doubled, or
/// removed entirely, keeping luminance.
#[inline]
pub fn sponge(c: [f32; 3], mode: Sponge) -> [f32; 3] {
    let l = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let k = match mode {
        Sponge::Saturate => 2.0,
        Sponge::Desaturate => 0.0,
    };
    let mut out = c.map(|v| l + (v - l) * k);
    // Pull back toward grey rather than clip, so hue survives.
    let (lo, hi) = (out[0].min(out[1]).min(out[2]), out[0].max(out[1]).max(out[2]));
    let mut s = 1.0f32;
    if hi > 1.0 {
        s = s.min((1.0 - l) / (hi - l));
    }
    if lo < 0.0 {
        s = s.min(l / (l - lo));
    }
    if s < 1.0 {
        out = out.map(|v| l + (v - l) * s);
    }
    out
}

/// Solve Laplace's equation for a 3-channel field `h` over the pixels where
/// `unknown` is set, holding every other pixel fixed. Coarse-to-fine: each
/// level starts from the solution one level down, then relaxes with SOR.
pub fn membrane(h: &mut [f32], unknown: &[bool], w: usize, ht: usize) {
    let count = unknown.iter().filter(|&&u| u).count();
    if count == 0 {
        return;
    }
    if w >= 8 && ht >= 8 && count > 64 {
        let (cw, ch) = (w.div_ceil(2), ht.div_ceil(2));
        let mut ch_vals = vec![0f32; cw * ch * 3];
        let mut ch_unknown = vec![true; cw * ch];
        let mut n = vec![0u32; cw * ch];
        for y in 0..ht {
            for x in 0..w {
                let i = y * w + x;
                if unknown[i] {
                    continue;
                }
                let c = (y / 2) * cw + x / 2;
                for k in 0..3 {
                    ch_vals[c * 3 + k] += h[i * 3 + k];
                }
                n[c] += 1;
            }
        }
        for c in 0..cw * ch {
            if n[c] > 0 {
                ch_unknown[c] = false;
                for k in 0..3 {
                    ch_vals[c * 3 + k] /= n[c] as f32;
                }
            }
        }
        membrane(&mut ch_vals, &ch_unknown, cw, ch);
        // Bilinear initial guess for the unknown fine pixels.
        for y in 0..ht {
            let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
            let y0 = fy.floor() as usize;
            let y1 = (y0 + 1).min(ch - 1);
            let ty = fy - y0 as f32;
            for x in 0..w {
                let i = y * w + x;
                if !unknown[i] {
                    continue;
                }
                let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
                let x0 = fx.floor() as usize;
                let x1 = (x0 + 1).min(cw - 1);
                let tx = fx - x0 as f32;
                for k in 0..3 {
                    let a = ch_vals[(y0 * cw + x0) * 3 + k] * (1.0 - tx) + ch_vals[(y0 * cw + x1) * 3 + k] * tx;
                    let b = ch_vals[(y1 * cw + x0) * 3 + k] * (1.0 - tx) + ch_vals[(y1 * cw + x1) * 3 + k] * tx;
                    h[i * 3 + k] = a * (1.0 - ty) + b * ty;
                }
            }
        }
    }
    let idx: Vec<usize> = (0..w * ht).filter(|&i| unknown[i]).collect();
    const OMEGA: f32 = 1.8;
    for _ in 0..40 {
        for &i in &idx {
            let (x, y) = (i % w, i / w);
            let mut sum = [0f32; 3];
            let mut n = 0f32;
            let mut add = |j: usize| {
                for k in 0..3 {
                    sum[k] += h[j * 3 + k];
                }
                n += 1.0;
            };
            if x > 0 {
                add(i - 1);
            }
            if x + 1 < w {
                add(i + 1);
            }
            if y > 0 {
                add(i - w);
            }
            if y + 1 < ht {
                add(i + w);
            }
            if n > 0.0 {
                for k in 0..3 {
                    let v = h[i * 3 + k];
                    h[i * 3 + k] = v + OMEGA * (sum[k] / n - v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dodge_and_burn_respect_range_and_endpoints() {
        for r in [Range::Shadows, Range::Midtones, Range::Highlights] {
            for v in [0.1f32, 0.5, 0.9] {
                assert!(dodge_burn(v, r, false) >= v && dodge_burn(v, r, true) <= v);
            }
        }
        assert_eq!(dodge_burn(0.0, Range::Midtones, false), 0.0);
        assert_eq!(dodge_burn(1.0, Range::Midtones, true), 1.0);
        let lift = |r, v: f32| dodge_burn(v, r, false) - v;
        assert!(lift(Range::Shadows, 0.2) > lift(Range::Highlights, 0.2));
        assert!(lift(Range::Highlights, 0.8) > lift(Range::Shadows, 0.8));
        assert!(lift(Range::Midtones, 0.5) > lift(Range::Midtones, 0.05));
    }

    #[test]
    fn sponge_keeps_luminance() {
        let c = [0.8, 0.4, 0.2];
        let l = |c: [f32; 3]| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
        let d = sponge(c, Sponge::Desaturate);
        assert!((d[0] - d[1]).abs() < 1e-5 && (l(d) - l(c)).abs() < 1e-5);
        let s = sponge(c, Sponge::Saturate);
        assert!(s[0] - s[2] > c[0] - c[2] && (l(s) - l(c)).abs() < 1e-4);
        assert!(s.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn membrane_interpolates_linear_boundary_exactly() {
        // A linear ramp is harmonic: the solver must reproduce it inside.
        let (w, h) = (40usize, 30usize);
        let mut field = vec![0f32; w * h * 3];
        let mut unknown = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let v = 3.0 * x as f32 - 2.0 * y as f32;
                let inside = x > 3 && x < 36 && y > 3 && y < 26;
                unknown[i] = inside;
                for k in 0..3 {
                    field[i * 3 + k] = if inside { 0.0 } else { v + k as f32 };
                }
            }
        }
        membrane(&mut field, &unknown, w, h);
        let i = 15 * w + 20;
        assert!((field[i * 3] - (60.0 - 30.0)).abs() < 0.5, "{}", field[i * 3]);
    }
}
