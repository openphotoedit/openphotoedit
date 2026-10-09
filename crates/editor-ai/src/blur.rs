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
    background_blur_alpha(img, None, background, sigma).0
}

/// [`background_blur`] for an image with transparency. Colour is weighted by
/// `alpha` as well as by the background, so a transparent pixel (whose colour
/// is meaningless, and black as the engine reads it) contributes nothing, and
/// the result carries an alpha of its own: how much of the surrounding
/// background was actually there. A background with nothing in it stays
/// transparent instead of coming out black. With `alpha` `None` the image is
/// opaque and the returned alpha is all 255.
pub fn background_blur_alpha(img: &Rgb, alpha: Option<&[u8]>, background: &[u8], sigma: f32) -> (Rgb, Vec<u8>) {
    let (w, h) = (img.width, img.height);
    let np = (w * h) as usize;
    let alpha = alpha.filter(|a| a.iter().any(|v| *v < 255));
    let factor = (sigma / 6.0).floor().max(1.0) as u32;
    let (sw, sh) = ((w / factor).max(1), (h / factor).max(1));
    // Premultiply before resampling, so transparent pixels do not darken
    // their visible neighbours on the way down.
    let small = match alpha {
        None => resize(img, sw, sh, Resample::Bilinear),
        Some(a) => {
            let mut pre = img.clone();
            for (i, px) in pre.data.chunks_exact_mut(3).enumerate() {
                for v in px {
                    *v = ((*v as u32 * a[i] as u32 + 127) / 255) as u8;
                }
            }
            resize(&pre, sw, sh, Resample::Bilinear)
        }
    };
    let wsmall = resize_u8(background, w, h, sw, sh, Resample::Bilinear);
    let asmall: Option<Vec<f32>> = alpha.map(|a| resize_u8(a, w, h, sw, sh, Resample::Bilinear).iter().map(|v| *v as f32 / 255.0).collect());
    let s = sigma / factor as f32;
    let (swu, shu) = (sw as usize, sh as usize);
    let n = swu * shu;
    let bgw: Vec<f32> = wsmall.iter().map(|v| *v as f32 / 255.0).collect();
    // Weight = background × alpha; `cover` is the same without the background.
    let weight: Vec<f32> = match &asmall {
        None => bgw.clone(),
        Some(a) => bgw.iter().zip(a).map(|(b, a)| b * a).collect(),
    };
    let wb = gaussian_plane(&weight, swu, shu, s);
    let cover = asmall.as_ref().map(|a| gaussian_plane(a, swu, shu, s));
    const EPS: f32 = 1e-3;
    let den: Vec<f32> = (0..n).map(|i| wb[i] + EPS * cover.as_ref().map_or(1.0, |c| c[i])).collect();
    let mut out_planes: Vec<Vec<f32>> = Vec::with_capacity(3);
    for c in 0..3 {
        // Premultiplied when there is alpha, so `chan` is already colour × alpha.
        let chan: Vec<f32> = (0..n).map(|i| small.data[i * 3 + c] as f32).collect();
        let weighted: Vec<f32> = chan.iter().zip(&bgw).map(|(v, w)| v * w).collect();
        let num = gaussian_plane(&weighted, swu, shu, s);
        let plain = gaussian_plane(&chan, swu, shu, s);
        let res: Vec<f32> = (0..n).map(|i| if den[i] > 1e-6 { (num[i] + EPS * plain[i]) / den[i] } else { 0.0 }).collect();
        out_planes.push(resize_plane(&res, sw, sh, w, h));
    }
    let mut data = vec![0u8; np * 3];
    for i in 0..np {
        for c in 0..3 {
            data[i * 3 + c] = crate::q8(out_planes[c][i]);
        }
    }
    let a_out = match alpha {
        None => vec![255u8; np],
        Some(_) => {
            // The share of the background around each pixel that was visible.
            let bb = gaussian_plane(&bgw, swu, shu, s);
            let cov: Vec<f32> = (0..n).map(|i| (den[i] / (bb[i] + EPS)).clamp(0.0, 1.0)).collect();
            resize_plane(&cov, sw, sh, w, h).iter().map(|v| crate::q8(v * 255.0)).collect()
        }
    };
    (Rgb { width: w, height: h, data }, a_out)
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

    /// A cut-out: the background is transparent (black colour, alpha 0), as
    /// a layer reads through a mask that hides it. Blurring it must not
    /// paint the background black; there is nothing there, so it stays
    /// transparent.
    #[test]
    fn transparent_background_stays_transparent_not_black() {
        let (w, h) = (120u32, 80u32);
        let mut img = Rgb::new(w, h);
        let mut alpha = vec![0u8; (w * h) as usize];
        let mut bg = vec![255u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if (40..80).contains(&x) {
                    img.data[i * 3..i * 3 + 3].copy_from_slice(&[220, 20, 20]);
                    alpha[i] = 255;
                    bg[i] = 0;
                }
            }
        }
        let (out, a) = background_blur_alpha(&img, Some(&alpha), &bg, 10.0);
        for (x, y) in [(2u32, 2u32), (10, 40), (115, 70)] {
            let i = (y * w + x) as usize;
            assert!(a[i] < 8, "background at ({x},{y}) should stay transparent, alpha {}", a[i]);
        }
        // Nowhere in the background is opaque black.
        for i in 0..(w * h) as usize {
            let p = &out.data[i * 3..i * 3 + 3];
            assert!(!(bg[i] == 255 && a[i] > 128 && p.iter().map(|v| *v as u32).sum::<u32>() < 30), "opaque black at {i}");
        }
    }

    /// Half the background visible, half transparent: next to the hole the
    /// blur keeps the visible colour instead of fading toward black.
    #[test]
    fn transparency_does_not_darken_visible_background() {
        let (w, h) = (120u32, 80u32);
        let mut img = Rgb::new(w, h);
        let mut alpha = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..60 {
                let i = (y * w + x) as usize;
                img.data[i * 3..i * 3 + 3].copy_from_slice(&[20, 200, 20]);
                alpha[i] = 255;
            }
        }
        let bg = vec![255u8; (w * h) as usize];
        let (out, a) = background_blur_alpha(&img, Some(&alpha), &bg, 10.0);
        let i = (40 * w + 56) as usize;
        let p = &out.data[i * 3..i * 3 + 3];
        assert!(p[1] > 185 && p[0] < 35, "colour beside the hole darkened: {p:?}");
        assert!(a[i] > 128 && a[(40 * w + 5) as usize] > 250 && a[(40 * w + 115) as usize] < 8, "alpha follows coverage");
    }

    /// An opaque image gives exactly what the opaque path gives.
    #[test]
    fn opaque_alpha_matches_the_opaque_path() {
        let (w, h) = (64u32, 48u32);
        let mut img = Rgb::new(w, h);
        for (i, v) in img.data.iter_mut().enumerate() {
            *v = (i * 37 % 251) as u8;
        }
        let bg: Vec<u8> = (0..(w * h) as usize).map(|i| if i % 64 < 30 { 255 } else { 0 }).collect();
        let plain = background_blur(&img, &bg, 7.0);
        let (with, a) = background_blur_alpha(&img, Some(&vec![255u8; (w * h) as usize]), &bg, 7.0);
        assert_eq!(plain.data, with.data);
        assert!(a.iter().all(|v| *v == 255));
    }
}
