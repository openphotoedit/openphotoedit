//! Cheap content heuristics for content-aware quick actions.
//!
//! There is no permissively licensed sky segmentation model with clean
//! training data (research 04, §3A #4), so "is there sky?" is answered from
//! colour and texture alone. It only decides whether to *offer* sky actions,
//! so a false negative costs a button, not a wrong edit.

use crate::img::{resize, fit_within};
use crate::Rgb;
use editor_core::transform::Resample;

/// Share of the top band that looks like sky, `0..1`.
pub fn sky_fraction(img: &Rgb) -> f32 {
    let (w, h) = fit_within(img.width, img.height, 192);
    let s = resize(img, w, h, Resample::Bilinear);
    let luma = s.luma();
    let band = (h as f32 * 0.4).ceil() as u32;
    let mut sky = 0u32;
    let mut total = 0u32;
    for y in 0..band {
        for x in 0..w {
            let [r, g, b] = s.px(x, y);
            let (r, g, b) = (r as f32, g as f32, b as f32);
            let l = luma[(y * w + x) as usize];
            // Smoothness: small luma difference to right and lower neighbours.
            let lr = luma[(y * w + (x + 1).min(w - 1)) as usize];
            let ld = luma[((y + 1).min(h - 1) * w + x) as usize];
            let smooth = (l - lr).abs() + (l - ld).abs() < 10.0;
            let blue = b > r + 8.0 && b >= g - 10.0 && l > 90.0;
            let overcast = l > 170.0 && (r - b).abs() < 25.0 && (g - b).abs() < 25.0;
            total += 1;
            if smooth && (blue || overcast) {
                sky += 1;
            }
        }
    }
    sky as f32 / total.max(1) as f32
}

pub fn has_sky(img: &Rgb) -> bool {
    sky_fraction(img) > 0.25
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_top_is_sky_texture_is_not() {
        let (w, h) = (200u32, 150u32);
        let mut img = Rgb::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                let px = if y < 70 { [110u8, 160, 230] } else { [60, 90, 40] };
                img.data[i..i + 3].copy_from_slice(&px);
            }
        }
        assert!(has_sky(&img));
        assert!(!has_sky(&crate::testutil::synthetic(w, h)));
    }
}
