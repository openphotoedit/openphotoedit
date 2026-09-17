//! Click-to-select with EdgeTAM (a SAM 2 family model, Apache-2.0).
//!
//! The encoder takes the image squashed to 1024² (the SAM 2 processor
//! resizes to a square rather than letterboxing), ImageNet-normalised. The
//! decoder takes points in that 1024 frame and returns three candidate
//! masks as 256² logits plus a predicted IoU for each.
//!
//! Post-processing upsamples the *logits* bilinearly and only then applies a
//! sigmoid: thresholding at 256² first and upsampling the binary mask gives a
//! staircase edge twelve document pixels tall on a 3 MP photo.

use crate::img::{fit_within, resize, resize_plane};
use crate::Rgb;
use editor_core::transform::Resample;

pub const ENCODER_SIZE: u32 = 1024;
pub const MASK_SIZE: u32 = 256;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

pub fn encoder_input(img: &Rgb) -> Vec<f32> {
    resize(img, ENCODER_SIZE, ENCODER_SIZE, Resample::Bilinear).to_chw_norm(MEAN, STD)
}

/// Document-space points `[x0, y0, x1, y1, …]` → the encoder's 1024 frame.
pub fn point_coords(points: &[f32], width: u32, height: u32) -> Vec<f32> {
    let sx = ENCODER_SIZE as f32 / width.max(1) as f32;
    let sy = ENCODER_SIZE as f32 / height.max(1) as f32;
    points.chunks_exact(2).flat_map(|p| [p[0] * sx, p[1] * sy]).collect()
}

/// Which of the three candidates to use.
///
/// SAM's usual rule — the highest predicted IoU — picks a *part* far too
/// often for a photo editor: on the test street scene one click on a walker
/// chose her coat, and on a museum product shot the IoU head rated the whole
/// shoe 0.01 and a speck 0.13. Candidate 0 is the whole-object mask in the
/// EdgeTAM export, and "select the object I clicked" means the whole object,
/// so it is taken unless it is degenerate (a speck, or nearly the entire
/// frame, which is the background), in which case the best-rated sane
/// candidate is used.
pub fn choose(pred: &[f32], iou: &[f32], prefer_whole: bool) -> usize {
    let plane = (MASK_SIZE * MASK_SIZE) as usize;
    let n = (pred.len() / plane).min(iou.len()).min(3);
    let area = |k: usize| pred[k * plane..(k + 1) * plane].iter().filter(|v| **v > 0.0).count() as f32 / plane as f32;
    let sane = |k: usize| {
        let a = area(k);
        (0.0005..0.85).contains(&a)
    };
    if n == 0 {
        return 0;
    }
    // A box prompt already says how big the object is, and there the IoU
    // head is reliable (measured: the whole walker was candidate 1 at 0.95).
    if prefer_whole && sane(0) {
        return 0;
    }
    (0..n).filter(|&k| sane(k)).max_by(|&a, &b| iou[a].total_cmp(&iou[b])).unwrap_or(0)
}

/// Candidate `index` of `pred` (`n × 256 × 256` logits) → an 8-bit document
/// mask, refined against `img` at working size when given.
pub fn to_mask(pred: &[f32], index: usize, width: u32, height: u32, img: Option<&Rgb>) -> Vec<u8> {
    let plane = (MASK_SIZE * MASK_SIZE) as usize;
    let mut cleaned = pred[index * plane..(index + 1) * plane].to_vec();
    clean_regions(&mut cleaned, MASK_SIZE as usize);
    let logits = &cleaned[..];
    let (ww, wh) = fit_within(width, height, crate::matte::REFINE_MAX);
    let up = resize_plane(logits, MASK_SIZE, MASK_SIZE, ww, wh);
    // Edge softness from the logits' own confidence. A confident mask has
    // logits of ±10 or more and a gentle slope already gives a crisp edge;
    // a low-contrast object (a cream shoe on a white plinth) can sit at +1
    // throughout, where a fixed slope leaves the whole object half selected.
    // So scale by the typical magnitude inside the mask.
    let mut inside: Vec<f32> = logits.iter().filter(|l| **l > 0.0).copied().collect();
    let typical = if inside.is_empty() {
        1.0
    } else {
        let mid = inside.len() / 2;
        *inside.select_nth_unstable_by(mid, |a, b| a.total_cmp(b)).1
    };
    let k = (6.0 / typical.max(0.25)).clamp(0.5, 24.0);
    let mut alpha: Vec<f32> = up.iter().map(|l| 1.0 / (1.0 + (-k * l).exp())).collect();
    if let Some(img) = img {
        let guide = if img.width == ww && img.height == wh { img.clone() } else { resize(img, ww, wh, Resample::Bilinear) };
        crate::matte::guided_filter(&guide, &mut alpha, 2, 1e-4);
    }
    let full = resize_plane(&alpha, ww, wh, width, height);
    full.iter().map(|v| crate::q8(v * 255.0)).collect()
}

/// SAM's own post-processing (`remove_small_regions`): fill holes and drop
/// islands smaller than a share of the mask, on the 256² logit grid. A
/// low-contrast object comes back speckled with little negative islands;
/// a selection with holes through it is never what a click meant.
fn clean_regions(logits: &mut [f32], size: usize) {
    let fg: Vec<bool> = logits.iter().map(|l| *l > 0.0).collect();
    let area = fg.iter().filter(|v| **v).count();
    if area == 0 {
        return;
    }
    let limit = (area / 8).max(16);
    for want in [false, true] {
        // Components of pixels whose value is `want`; holes are background
        // components not touching the border, islands are small foreground ones.
        let mut seen = vec![false; size * size];
        let mut stack = Vec::new();
        let mut members = Vec::new();
        for start in 0..size * size {
            if seen[start] || fg[start] != want {
                continue;
            }
            members.clear();
            let mut border = false;
            seen[start] = true;
            stack.push(start);
            while let Some(i) = stack.pop() {
                members.push(i);
                let (x, y) = (i % size, i / size);
                if x == 0 || y == 0 || x + 1 == size || y + 1 == size {
                    border = true;
                }
                let mut visit = |j: usize| {
                    if !seen[j] && fg[j] == want {
                        seen[j] = true;
                        stack.push(j);
                    }
                };
                if x > 0 { visit(i - 1); }
                if x + 1 < size { visit(i + 1); }
                if y > 0 { visit(i - size); }
                if y + 1 < size { visit(i + size); }
            }
            let small = members.len() <= limit;
            let flip = if want { small && members.len() * 100 < area } else { small && !border };
            if flip {
                let v = if want { -1.0 } else { 1.0 };
                for &i in &members {
                    logits[i] = v * logits[i].abs().max(1.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_fill_and_specks_vanish() {
        let n = 256;
        let mut l = vec![-3.0f32; n * n];
        for y in 50..200 {
            for x in 50..200 {
                l[y * n + x] = 3.0;
            }
        }
        for y in 100..110 {
            for x in 100..110 {
                l[y * n + x] = -3.0; // hole
            }
        }
        l[10 * n + 240] = 3.0; // speck
        clean_regions(&mut l, n);
        assert!(l[105 * n + 105] > 0.0, "hole filled");
        assert!(l[10 * n + 240] < 0.0, "speck removed");
        assert!(l[5 * n + 5] < 0.0, "background untouched");
    }

    #[test]
    fn points_scale_into_the_square_frame() {
        let p = point_coords(&[100.0, 50.0, 2000.0, 1000.0], 2000, 1000);
        assert_eq!(p, vec![51.2, 51.2, 1024.0, 1024.0]);
    }

    #[test]
    fn whole_object_candidate_wins_unless_degenerate() {
        let plane = 256 * 256;
        let mut pred = vec![-1.0f32; plane * 3];
        // 0: a large object (25%); 1: a part (1%); 2: nearly everything.
        for i in 0..plane / 4 {
            pred[i] = 1.0;
        }
        for i in 0..plane / 100 {
            pred[plane + i] = 1.0;
        }
        for i in 0..plane * 95 / 100 {
            pred[2 * plane + i] = 1.0;
        }
        assert_eq!(choose(&pred, &[0.01, 0.9, 0.95], true), 0, "whole object despite a low IoU");
        // Candidate 0 a speck: best sane IoU among the rest (2 is the background).
        for v in pred[..plane].iter_mut() {
            *v = -1.0;
        }
        pred[5] = 1.0;
        assert_eq!(choose(&pred, &[0.99, 0.5, 0.95], true), 1);
        assert_eq!(choose(&pred, &[0.01, 0.9, 0.95], false), 1, "box prompts take the best sane IoU");
    }

    #[test]
    fn mask_follows_logit_sign_and_fills_the_document() {
        let plane = 256 * 256;
        let mut pred = vec![-2.0f32; plane * 3];
        // Candidate 1: left half positive.
        for y in 0..256 {
            for x in 0..128 {
                pred[plane + y * 256 + x] = 2.0;
            }
        }
        let m = to_mask(&pred, 1, 400, 300, None);
        assert_eq!(m.len(), 400 * 300);
        assert!(m[400 * 10 + 10] > 250, "a weak but consistent mask is fully selected");
        assert!(m[400 * 10 + 399] < 25);
        // With a logit ramp across the edge (as real masks have), the edge
        // is anti-aliased rather than a staircase.
        let ramp: Vec<f32> = (0..plane * 3).map(|i| ((128.0 - (i % 256) as f32) * 0.5).clamp(-8.0, 8.0)).collect();
        let r = to_mask(&ramp, 0, 400, 300, None);
        assert!(r.iter().any(|&v| v > 20 && v < 235));
    }
}
