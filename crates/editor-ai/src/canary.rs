//! Pixel-parity canary signatures.
//!
//! Two silent WebGPU failures have already shipped in sibling apps: a halved
//! GFPGAN that returned the same wash for any face, and a MODNet matte with
//! holes through the hair. Every call succeeded; only the pixels were wrong.
//! So on the first session per (model, backend) the worker runs a bundled
//! golden input and reduces the first output to a coarse signature — the
//! mean of each channel over a `GRID × GRID` grid — and compares it with the
//! signature recorded from the CPU backend. A mismatch, or an output that is
//! non-finite or flat, marks that backend bad for that model.

pub const GRID: usize = 4;

/// `c × h × w` tensor → `c × GRID × GRID` cell means (at most 3 channels).
pub fn signature(chw: &[f32], c: usize, h: usize, w: usize) -> Vec<f32> {
    let c = c.min(3);
    let mut out = vec![0.0f32; c * GRID * GRID];
    let plane = h * w;
    if plane == 0 || chw.len() < c * plane {
        return out;
    }
    for ch in 0..c {
        for gy in 0..GRID {
            let (y0, y1) = (gy * h / GRID, ((gy + 1) * h / GRID).max(gy * h / GRID + 1).min(h));
            for gx in 0..GRID {
                let (x0, x1) = (gx * w / GRID, ((gx + 1) * w / GRID).max(gx * w / GRID + 1).min(w));
                let mut acc = 0.0f64;
                for y in y0..y1 {
                    for x in x0..x1 {
                        acc += chw[ch * plane + y * w + x] as f64;
                    }
                }
                out[ch * GRID * GRID + gy * GRID + gx] = (acc / ((y1 - y0) * (x1 - x0)) as f64) as f32;
            }
        }
    }
    out
}

/// Largest cell difference relative to the reference's spread (0 = equal).
pub fn deviation(sig: &[f32], reference: &[f32]) -> f32 {
    if sig.len() != reference.len() || sig.is_empty() {
        return f32::INFINITY;
    }
    let (lo, hi) = reference.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
    let spread = (hi - lo).abs().max(1e-3);
    sig.iter().zip(reference).map(|(a, b)| if a.is_finite() { (a - b).abs() / spread } else { f32::INFINITY }).fold(0.0, f32::max)
}

/// An output that cannot be a model's answer: non-finite values, or flat.
pub fn degenerate(values: &[f32]) -> bool {
    if values.is_empty() {
        return true;
    }
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &v in values {
        if !v.is_finite() {
            return true;
        }
        lo = lo.min(v);
        hi = hi.max(v);
    }
    hi - lo < 1e-4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_of_a_gradient() {
        let (h, w) = (8, 8);
        let chw: Vec<f32> = (0..h * w).map(|i| (i % w) as f32).collect();
        let s = signature(&chw, 1, h, w);
        assert_eq!(s.len(), 16);
        assert_eq!(&s[..4], &[0.5, 2.5, 4.5, 6.5]);
        assert_eq!(deviation(&s, &s), 0.0);
        let mut t = s.clone();
        t[3] += 3.0;
        assert!(deviation(&t, &s) > 0.4);
    }

    #[test]
    fn flat_and_nan_are_degenerate() {
        assert!(degenerate(&[0.3; 10]));
        assert!(degenerate(&[0.0, f32::NAN]));
        assert!(!degenerate(&[0.0, 1.0]));
    }
}
