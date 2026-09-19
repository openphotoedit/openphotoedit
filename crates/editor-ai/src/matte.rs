//! Subject mattes: MODNet for people, U²-Net-p for everything else, and the
//! guided-filter refinement that snaps a model-resolution matte to the real
//! edges of the photo (from OpenPhotoId `frame-matting`).
//!
//! Both models run at a fixed square (MODNet 512, U²-Net-p 320) so a WebGPU
//! session never sees a second shape. The photo is squashed into the square
//! rather than letterboxed: OpenPhotoId validated MODNet that way, and a
//! squash spends every input pixel on the picture.
//!
//! Refinement runs at a *working* size (long side ≤ [`REFINE_MAX`]) and the
//! result is bilinearly upsampled: a guided filter at 12 MP needs a dozen
//! full-size float planes, several hundred MB of wasm memory, for an edge
//! improvement that does not survive the viewer's own downsampling.

use crate::img::{fit_within, resize, resize_plane};
use crate::Rgb;
use editor_core::transform::Resample;

pub const MODNET_SIZE: u32 = 512;
pub const U2NET_SIZE: u32 = 320;
pub const REFINE_MAX: u32 = 1600;
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// MODNet input: 512² NCHW RGB in `[-1, 1]`.
pub fn modnet_input(img: &Rgb) -> Vec<f32> {
    let small = resize(img, MODNET_SIZE, MODNET_SIZE, Resample::Bilinear);
    small.to_chw_norm([0.5; 3], [0.5; 3])
}

/// U²-Net-p input: 320² NCHW, ImageNet-normalised.
pub fn u2net_input(img: &Rgb) -> Vec<f32> {
    let small = resize(img, U2NET_SIZE, U2NET_SIZE, Resample::Bilinear);
    small.to_chw_norm(IMAGENET_MEAN, IMAGENET_STD)
}

/// U²-Net's `d0` is not calibrated: stretch to its own min/max (as rembg
/// does). MODNet's output is already an alpha in `[0, 1]`.
pub fn normalise_u2net(d0: &[f32]) -> Vec<f32> {
    let n = (U2NET_SIZE * U2NET_SIZE) as usize;
    let d0 = &d0[..n];
    let (lo, hi) = d0.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let range = (hi - lo).max(1e-6);
    d0.iter().map(|v| (v - lo) / range).collect()
}

/// A model-resolution matte (`size²`, `[0, 1]`) → an 8-bit matte at
/// `width × height`, refined against `img` at a working size.
///
/// `img` may already be at working size; it is resized here if not.
pub fn finish_matte(small: &[f32], size: u32, img: &Rgb, width: u32, height: u32, refine: bool) -> Vec<u8> {
    let (ww, wh) = fit_within(width, height, REFINE_MAX);
    let mut alpha = resize_plane(small, size, size, ww, wh);
    if refine {
        let guide = if img.width == ww && img.height == wh { img.clone() } else { resize(img, ww, wh, Resample::Bilinear) };
        guided_filter(&guide, &mut alpha, 2, 1e-4);
    }
    let full = resize_plane(&alpha, ww, wh, width, height);
    full.iter().map(|v| crate::q8(v.clamp(0.0, 1.0) * 255.0)).collect()
}

/// Box mean with radius `r`, clamped borders, O(N).
fn box_filter(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0f32; w * h];
    let n = (2 * r + 1) as f32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let at = |i: isize| row[i.clamp(0, w as isize - 1) as usize];
        let mut acc: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for x in 0..w {
            tmp[y * w + x] = acc / n;
            acc += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
        }
    }
    let mut out = vec![0.0f32; w * h];
    for x in 0..w {
        let at = |i: isize| tmp[(i.clamp(0, h as isize - 1) as usize) * w + x];
        let mut acc: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for y in 0..h {
            out[y * w + x] = acc / n;
            acc += at(y as isize + r as isize + 1) - at(y as isize - r as isize);
        }
    }
    out
}

/// Guided filter (He et al.) with a grey guide. The radius stays small
/// regardless of image size: OpenPhotoId measured a halo around hair at any
/// radius above 2.
pub fn guided_filter(img: &Rgb, alpha: &mut [f32], radius: usize, eps: f32) {
    let (w, h) = (img.width as usize, img.height as usize);
    assert_eq!(alpha.len(), w * h);
    let guide: Vec<f32> = img.luma().iter().map(|v| v / 255.0).collect();
    let mean_i = box_filter(&guide, w, h, radius);
    let mean_p = box_filter(alpha, w, h, radius);
    let ii: Vec<f32> = guide.iter().map(|v| v * v).collect();
    let ip: Vec<f32> = guide.iter().zip(alpha.iter()).map(|(g, p)| g * p).collect();
    let mean_ii = box_filter(&ii, w, h, radius);
    drop(ii);
    let mean_ip = box_filter(&ip, w, h, radius);
    drop(ip);
    let mut a = vec![0.0f32; w * h];
    let mut b = vec![0.0f32; w * h];
    for i in 0..w * h {
        let var = mean_ii[i] - mean_i[i] * mean_i[i];
        let cov = mean_ip[i] - mean_i[i] * mean_p[i];
        a[i] = cov / (var + eps);
        b[i] = mean_p[i] - a[i] * mean_i[i];
    }
    let mean_a = box_filter(&a, w, h, radius);
    let mean_b = box_filter(&b, w, h, radius);
    for i in 0..w * h {
        alpha[i] = (mean_a[i] * guide[i] + mean_b[i]).clamp(0.0, 1.0);
    }
}

/// Fraction of pixels above half coverage.
pub fn coverage(matte: &[u8]) -> f32 {
    if matte.is_empty() {
        return 0.0;
    }
    matte.iter().filter(|&&v| v >= 128).count() as f32 / matte.len() as f32
}

/// Whether a matte looks like a real subject rather than noise: it covers a
/// plausible share of the frame and is mostly decided (few pixels in the
/// uncertain middle band).
pub fn looks_like_subject(matte: &[u8]) -> bool {
    let c = coverage(matte);
    let uncertain = matte.iter().filter(|&&v| (40..216).contains(&v)).count() as f32 / matte.len().max(1) as f32;
    (0.03..0.92).contains(&c) && uncertain < 0.25
}


/// Boxes around the main blobs of a matte, `[x0, y0, x1, y1]` in document
/// pixels, largest first — prompts for EdgeTAM, which turns a rough saliency
/// or portrait matte into whole, crisp objects. Found on a grid with a long
/// side of 256; blobs whose padded boxes overlap merge, so a matte with holes
/// through one person still yields one box for that person.
pub fn subject_boxes(matte: &[u8], width: u32, height: u32, max_boxes: usize) -> Vec<[f32; 4]> {
    let (gw, gh) = fit_within(width, height, 256);
    let small = crate::img::resize_u8(matte, width, height, gw, gh, Resample::Bilinear);
    let (gw, gh) = (gw as usize, gh as usize);
    let mut label = vec![0u32; gw * gh];
    let mut comps: Vec<(usize, [usize; 4])> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..gw * gh {
        if small[start] < 128 || label[start] != 0 {
            continue;
        }
        let id = comps.len() as u32 + 1;
        let (mut area, mut bb) = (0usize, [usize::MAX, usize::MAX, 0, 0]);
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % gw, i / gw);
            area += 1;
            bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
            let mut visit = |j: usize| {
                if small[j] >= 128 && label[j] == 0 {
                    label[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 { visit(i - 1); }
            if x + 1 < gw { visit(i + 1); }
            if y > 0 { visit(i - gw); }
            if y + 1 < gh { visit(i + gw); }
        }
        comps.push((area, bb));
    }
    let total: usize = comps.iter().map(|c| c.0).sum();
    let min_area = ((total as f32 * 0.015) as usize).max((gw * gh) / 1000).max(4);
    let mut boxes: Vec<(usize, [f32; 4])> = comps
        .into_iter()
        .filter(|c| c.0 >= min_area)
        .map(|(a, b)| {
            let pad = ((b[2] - b[0]).max(b[3] - b[1]) as f32 * 0.04).max(1.0);
            (a, [b[0] as f32 - pad, b[1] as f32 - pad, b[2] as f32 + 1.0 + pad, b[3] as f32 + 1.0 + pad])
        })
        .collect();
    // Merge overlapping boxes until none overlap.
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (boxes[i].1, boxes[j].1);
                // Allow a vertical gap of a tenth of the taller box: a matte
                // hole through a waist separates the halves by that much.
                let gap = (a[3] - a[1]).max(b[3] - b[1]) * 0.1;
                if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] + gap && b[1] < a[3] + gap {
                    let inter = (a[2].min(b[2]) - a[0].max(b[0])) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
                    let smaller = ((a[2] - a[0]) * (a[3] - a[1])).min((b[2] - b[0]) * (b[3] - b[1]));
                    let xo = a[2].min(b[2]) - a[0].max(b[0]);
                    let narrower = (a[2] - a[0]).min(b[2] - b[0]);
                    // Heavy overlap, or one blob stacked on another (a person
                    // cut in two by a hole in the matte).
                    if inter > smaller * 0.3 || xo > narrower * 0.6 {
                        boxes[i] = (boxes[i].0 + boxes[j].0, [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
                        boxes.remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
        }
        if !merged {
            break;
        }
    }
    boxes.sort_by(|a, b| b.0.cmp(&a.0));
    let sx = width as f32 / gw as f32;
    let sy = height as f32 / gh as f32;
    boxes
        .into_iter()
        .take(max_boxes)
        .map(|(_, b)| [(b[0] * sx).max(0.0), (b[1] * sy).max(0.0), (b[2] * sx).min(width as f32), (b[3] * sy).min(height as f32)])
        .collect()
}

/// Remove Background's final mask, combining the model's matte with what
/// the layer already has instead of refusing (Compositor multiplies into an
/// existing mask and limits the change to the selection: Filters.swift,
/// SubjectRemoval.swift, MIT):
///
/// - `existing`: the layer's current mask over the same pixels (None = the
///   layer has no mask, i.e. all 255). The matte multiplies into it, so
///   anything already hidden stays hidden.
/// - `selection`: selection coverage (None = no selection, i.e. all 255).
///   Outside the selection the existing mask is kept exactly; across a soft
///   edge the two blend: `sel·(existing·matte) + (1−sel)·existing`.
///
/// All three are single-channel planes of the same length.
pub fn combine_with_existing(matte: &[u8], existing: Option<&[u8]>, selection: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(matte.len());
    for (i, &m) in matte.iter().enumerate() {
        let e = existing.map_or(255.0, |e| e[i] as f32);
        let s = selection.map_or(1.0, |s| s[i] as f32 / 255.0);
        let removed = e * m as f32 / 255.0;
        out.push((s * removed + (1.0 - s) * e).round().clamp(0.0, 255.0) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_have_model_shapes_and_ranges() {
        let img = crate::testutil::synthetic(100, 60);
        let m = modnet_input(&img);
        assert_eq!(m.len(), 3 * 512 * 512);
        assert!(m.iter().all(|v| (-1.0..=1.0).contains(v)));
        let u = u2net_input(&img);
        assert_eq!(u.len(), 3 * 320 * 320);
    }

    #[test]
    fn guided_filter_moves_a_misaligned_edge_toward_the_image_edge() {
        let (w, h) = (64u32, 32u32);
        let mut img = Rgb::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                img.data[i..i + 3].copy_from_slice(if x < 32 { &[40, 10, 10] } else { &[90, 230, 90] });
            }
        }
        let mut alpha = vec![0.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                alpha[(y * w + x) as usize] = ((42.0 - x as f32) / 12.0).clamp(0.0, 1.0);
            }
        }
        let cross = |a: &[f32]| a[16 * 64..17 * 64].iter().position(|v| *v < 0.5).unwrap();
        let before = cross(&alpha);
        guided_filter(&img, &mut alpha, 6, 1e-6);
        guided_filter(&img, &mut alpha, 4, 1e-6);
        guided_filter(&img, &mut alpha, 2, 1e-6);
        let after = cross(&alpha);
        assert!(after < before && after.abs_diff(32) <= 2, "{before} -> {after}");
    }

    #[test]
    fn finish_matte_upsamples_to_the_document() {
        let small: Vec<f32> = (0..320 * 320).map(|i| if (i % 320) < 160 { 1.0 } else { 0.0 }).collect();
        let img = crate::testutil::synthetic(80, 40);
        let m = finish_matte(&small, 320, &img, 80, 40, false);
        assert_eq!(m.len(), 80 * 40);
        assert_eq!(m[0], 255);
        assert_eq!(m[79], 0);
        assert!(looks_like_subject(&m));
    }

    #[test]
    fn subject_boxes_find_blobs_merge_overlaps_and_drop_specks() {
        let (w, h) = (400u32, 300u32);
        let mut m = vec![0u8; (w * h) as usize];
        let mut rect = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for y in y0..y1 {
                for x in x0..x1 {
                    m[(y * w + x) as usize] = 255;
                }
            }
        };
        rect(20, 20, 120, 280); // person A, split by a hole below
        rect(20, 150, 120, 160);
        rect(250, 50, 380, 250); // person B
        rect(200, 10, 202, 12); // speck
        // Punch a horizontal hole through A.
        for y in 140..150 {
            for x in 20..120 {
                m[(y * w + x) as usize] = 0;
            }
        }
        let b = subject_boxes(&m, w, h, 6);
        assert_eq!(b.len(), 2, "{b:?}");
        assert!(b[0][0] >= 240.0 && b[0][2] <= 400.0, "largest first: {b:?}");
        assert!(b[1][1] <= 20.0 && b[1][3] >= 278.0, "the split person is one box: {b:?}");
    }

    #[test]
    fn u2net_output_is_stretched() {
        let mut d0 = vec![0.2f32; 320 * 320];
        d0[320 * 160..].iter_mut().for_each(|v| *v = 0.6);
        let n = normalise_u2net(&d0);
        assert_eq!(n[0], 0.0);
        assert_eq!(*n.last().unwrap(), 1.0);
    }

    #[test]
    fn remove_background_multiplies_into_an_existing_mask() {
        assert_eq!(combine_with_existing(&[255, 0, 128], Some(&[128, 128, 255]), None), vec![128, 0, 128]);
        assert_eq!(combine_with_existing(&[255, 0], None, None), vec![255, 0]);
    }

    #[test]
    fn remove_background_respects_the_selection() {
        // Left half selected: changed there; right half keeps the existing mask exactly.
        let matte = [0u8, 0, 0, 0];
        let existing = [200u8, 200, 90, 90];
        let sel = [255u8, 255, 0, 0];
        assert_eq!(combine_with_existing(&matte, Some(&existing), Some(&sel)), vec![0, 0, 90, 90]);
        // Without a mask the unselected part stays fully shown; a half-selected pixel blends.
        assert_eq!(combine_with_existing(&[0, 0, 0], None, Some(&[255, 128, 0])), vec![0, 127, 255]);
    }
}
