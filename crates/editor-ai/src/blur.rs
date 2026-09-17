//! Background blur that does not smear the subject.
//!
//! A plain Gaussian of the whole photo, masked to the background, leaves a
//! glowing halo around the subject: the pixels just outside the edge are
//! averages that include the subject. So this is a *normalised* convolution
//! weighted by the background matte — `G*(I·w) / G*(w)` — which only ever
//! mixes background into background. Where the weights vanish (deep inside
//! the subject, hidden by the layer mask anyway) a small unweighted share
//! keeps the division stable.
//!
//! A large sigma is computed at reduced resolution: the result has no detail
//! at that scale to lose, and a 60 px Gaussian on 12 MP would otherwise cost
//! seconds.

use crate::img::{resize, resize_plane, resize_u8};
use crate::Rgb;
use editor_core::transform::Resample;

/// Three box passes approximate a Gaussian of `sigma` (Wells 1986).
fn box_radius(sigma: f32) -> usize {
    let w = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    (((w - 1.0) / 2.0).round() as usize).max(1)
}

fn box_pass(src: &[f32], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    let n = (2 * r + 1) as f32;
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    for o in 0..outer {
        let idx = |i: isize| -> usize {
            let i = i.clamp(0, inner as isize - 1) as usize;
            if horizontal { o * w + i } else { i * w + o }
        };
        let mut acc: f32 = (-(r as isize)..=r as isize).map(|i| src[idx(i)]).sum();
        for i in 0..inner {
            out[idx(i as isize)] = acc / n;
            acc += src[idx(i as isize + r as isize + 1)] - src[idx(i as isize - r as isize)];
        }
    }
    out
}

pub fn gaussian_plane(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    if sigma < 0.3 {
        return src.to_vec();
    }
    let r = box_radius(sigma);
    let mut p = src.to_vec();
    for _ in 0..3 {
        p = box_pass(&p, w, h, r, true);
        p = box_pass(&p, w, h, r, false);
    }
    p
}

/// Blur `img` by `sigma` document pixels, drawing only on pixels where
/// `background` (8-bit, 255 = background) is set. Returns opaque RGB.
pub fn background_blur(img: &Rgb, background: &[u8], sigma: f32) -> Rgb {
    let (w, h) = (img.width, img.height);
    let factor = (sigma / 6.0).floor().max(1.0) as u32;
    let (sw, sh) = ((w / factor).max(1), (h / factor).max(1));
    let small = resize(img, sw, sh, Resample::Bilinear);
    let wsmall = resize_u8(background, w, h, sw, sh, Resample::Bilinear);
    let s = sigma / factor as f32;
    let (swu, shu) = (sw as usize, sh as usize);
    let n = swu * shu;
    let weight: Vec<f32> = wsmall.iter().map(|v| *v as f32 / 255.0).collect();
    let wb = gaussian_plane(&weight, swu, shu, s);
    const EPS: f32 = 1e-3;
    let mut out_planes: Vec<Vec<f32>> = Vec::with_capacity(3);
    for c in 0..3 {
        let chan: Vec<f32> = (0..n).map(|i| small.data[i * 3 + c] as f32).collect();
        let weighted: Vec<f32> = chan.iter().zip(&weight).map(|(v, w)| v * w).collect();
        let num = gaussian_plane(&weighted, swu, shu, s);
        let plain = gaussian_plane(&chan, swu, shu, s);
        let res: Vec<f32> = (0..n).map(|i| (num[i] + EPS * plain[i]) / (wb[i] + EPS)).collect();
        out_planes.push(resize_plane(&res, sw, sh, w, h));
    }
    let np = (w * h) as usize;
    let mut data = vec![0u8; np * 3];
    for i in 0..np {
        for c in 0..3 {
            data[i * 3 + c] = crate::q8(out_planes[c][i]);
        }
    }
    Rgb { width: w, height: h, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_preserves_flat_and_spreads_an_impulse() {
        let flat = vec![7.0f32; 50 * 40];
        assert!(gaussian_plane(&flat, 50, 40, 4.0).iter().all(|v| (v - 7.0).abs() < 1e-3));
        let mut imp = vec![0.0f32; 51 * 51];
        imp[25 * 51 + 25] = 1000.0;
        let g = gaussian_plane(&imp, 51, 51, 4.0);
        let sum: f32 = g.iter().sum();
        assert!((sum - 1000.0).abs() < 1.0);
        assert!(g[25 * 51 + 25] < 100.0 && g[25 * 51 + 29] > 1.0);
    }

    /// A red subject on a green background: the blurred background next to
    /// the subject must stay green, not pick up a red halo.
    #[test]
    fn no_subject_halo() {
        let (w, h) = (120u32, 80u32);
        let mut img = Rgb::new(w, h);
        let mut bg = vec![255u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let subject = (40..80).contains(&x);
                img.data[i * 3..i * 3 + 3].copy_from_slice(if subject { &[220, 20, 20] } else { &[20, 200, 20] });
                if subject {
                    bg[i] = 0;
                }
            }
        }
        let out = background_blur(&img, &bg, 10.0);
        let p = out.px(37, 40);
        assert!(p[0] < 40 && p[1] > 170, "halo at the edge: {p:?}");
    }
}
