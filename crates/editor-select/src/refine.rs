//! Select and Mask style refinement of the current selection against the
//! visible composite.
//!
//! In order, like Photoshop's panel:
//! - `radius` (px): a band of this half-width around the selection edge is
//!   re-estimated with a colour-guided filter, so a rough edge lands on the
//!   object boundary (and soft detail such as hair picks up partial coverage);
//! - `smooth` (0..100): rounds off jagged outlines (a blur of up to 20 px
//!   followed by a re-sharpening that keeps the edge width);
//! - `feather` (px): Gaussian softening;
//! - `contrast` (0..100 %): hardens soft transitions;
//! - `shift_edge` (-100..100 %): moves soft edges in or out.
//!
//! Colour decontamination rewrites pixels, not the selection, and is not done
//! here; a request for it is reported back in the result data.

use editor_core::adjust::box_blur_1ch;
use editor_core::document::Document;
use editor_core::plane::Plane;
use serde::Deserialize;

use editor_core::geom::Rect;
use editor_core::render::{render_view, View};

use crate::morph::{edt_sq, selection_area};
use crate::quick::{upsample_and_snap, watershed, Hist};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Params {
    pub radius: f32,
    pub smooth: f32,
    pub feather: f32,
    pub contrast: f32,
    pub shift_edge: f32,
}

pub fn refine(doc: &Document, sel: &Plane, p: &Params) -> Plane {
    let canvas = sel.bounds();
    let content = selection_area(sel);
    if content.is_empty() {
        return sel.clone();
    }
    let radius = p.radius.clamp(0.0, 250.0);
    let smooth_r = (p.smooth.clamp(0.0, 100.0) / 5.0).round() as usize;
    let feather = p.feather.clamp(0.0, 250.0);
    let pad = radius.ceil() as i32 + (feather * 3.0).ceil() as i32 + smooth_r as i32 * 3 + 4;
    let area = content.inflate(pad).intersect(&canvas);
    let (w, h) = (area.w as usize, area.h as usize);
    let mut m: Vec<f32> = sel.read_vec(area).iter().map(|&v| v as f32 / 255.0).collect();

    if radius >= 1.0 {
        snap_edge(doc, sel, area, &mut m, radius);
    }

    if smooth_r > 0 {
        box_blur_1ch(&mut m, w, h, smooth_r, 3);
        let k = 1.0 + smooth_r as f32 * 0.6;
        for v in m.iter_mut() {
            *v = ((*v - 0.5) * k + 0.5).clamp(0.0, 1.0);
        }
    }
    if feather > 0.0 {
        let pass_r = ((feather / 1.7).round() as usize).max(1);
        box_blur_1ch(&mut m, w, h, pass_r, 3);
    }
    let contrast = p.contrast.clamp(0.0, 100.0);
    if contrast > 0.0 {
        let k = 1.0 / (1.0 - 0.99 * contrast / 100.0);
        for v in m.iter_mut() {
            *v = ((*v - 0.5) * k + 0.5).clamp(0.0, 1.0);
        }
    }
    let shift = p.shift_edge.clamp(-100.0, 100.0);
    if shift != 0.0 {
        let gamma = 2f32.powf(-shift / 50.0);
        for v in m.iter_mut() {
            *v = v.clamp(0.0, 1.0).powf(gamma);
        }
    }

    let out: Vec<u8> = m.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    let mut plane = sel.clone();
    plane.write(area, &out);
    plane.compact();
    plane
}

/// Re-decide the pixels within `radius` of the selection edge.
///
/// Pixels farther inside are certain foreground, farther outside certain
/// background. A seeded watershed over colour and colour-model edges (the
/// same solver as Quick Selection) assigns the uncertain band to whichever
/// side reaches it across weaker edges, at a resolution capped to a pixel
/// budget; the result's boundary is then snapped to the image at full
/// resolution with a guided filter.
fn snap_edge(doc: &Document, sel: &Plane, area: Rect, m: &mut [f32], radius: f32) {
    const BUDGET: f64 = 2_000_000.0;
    let s = (BUDGET / area.area() as f64).sqrt().min(1.0);
    let lw = ((area.w as f64 * s).ceil() as usize).max(1);
    let lh = ((area.h as f64 * s).ceil() as usize).max(1);
    let n = lw * lh;
    let rgba = render_view(doc, View { x: area.x as f64, y: area.y as f64, scale: s, width: lw, height: lh });
    let img: Vec<f32> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut low = vec![0f32; n];
    sel.resample(area.x as f64, area.y as f64, 1.0 / s, lw, lh, &mut low);
    let inside: Vec<bool> = low.iter().map(|&v| v >= 0.5).collect();
    let mut boundary = vec![false; n];
    for y in 0..lh {
        for x in 0..lw {
            let c = inside[y * lw + x];
            boundary[y * lw + x] = (x > 0 && inside[y * lw + x - 1] != c)
                || (x + 1 < lw && inside[y * lw + x + 1] != c)
                || (y > 0 && inside[(y - 1) * lw + x] != c)
                || (y + 1 < lh && inside[(y + 1) * lw + x] != c);
        }
    }
    let rl = (radius as f64 * s).max(1.0) as f32;
    let reach = rl * 2.5 + 2.0;
    let d: Vec<f32> = edt_sq(&boundary, lw, lh, reach * reach).iter().map(|v| v.sqrt()).collect();

    let mut fg = Hist::new();
    let mut bg = Hist::new();
    let mut seed = vec![0u8; n];
    for i in 0..n {
        if d[i] < rl {
            continue;
        }
        let c = [img[i * 3], img[i * 3 + 1], img[i * 3 + 2]];
        seed[i] = if inside[i] { 1 } else { 2 };
        if d[i] < reach - 2.0 {
            if inside[i] {
                fg.add(c);
            } else {
                bg.add(c);
            }
        }
    }
    fg.smooth();
    bg.smooth();
    let mut pf: Vec<f32> = (0..n)
        .map(|i| {
            let c = [img[i * 3], img[i * 3 + 1], img[i * 3 + 2]];
            let (f, b) = (fg.density(c) + 1e-5, bg.density(c) + 1e-5);
            f / (f + b)
        })
        .collect();
    box_blur_1ch(&mut pf, lw, lh, 1, 1);
    let label = watershed(&seed, &img, &pf, lw, lh);
    let decided: Vec<f32> = label.iter().map(|&l| (l == 1) as u8 as f32).collect();
    let snapped = upsample_and_snap(doc, area, &decided, s, lw, lh);

    // Mix into the full-resolution mask by a weight that fades out at the
    // band's outer limit.
    let ramp = (rl * 0.3).max(1.0);
    let wlow: Vec<f32> = d.iter().map(|&v| ((rl - v) / ramp).clamp(0.0, 1.0)).collect();
    let (w, h) = (area.w as usize, area.h as usize);
    for y in 0..h {
        let ly = ((y as f64 + 0.5) * s - 0.5).clamp(0.0, (lh - 1) as f64);
        let y0 = ly.floor() as usize;
        let y1 = (y0 + 1).min(lh - 1);
        let ty = (ly - y0 as f64) as f32;
        for x in 0..w {
            let lx = ((x as f64 + 0.5) * s - 0.5).clamp(0.0, (lw - 1) as f64);
            let x0 = lx.floor() as usize;
            let x1 = (x0 + 1).min(lw - 1);
            let tx = (lx - x0 as f64) as f32;
            let wt = (wlow[y0 * lw + x0] * (1.0 - tx) + wlow[y0 * lw + x1] * tx) * (1.0 - ty) + (wlow[y1 * lw + x0] * (1.0 - tx) + wlow[y1 * lw + x1] * tx) * ty;
            if wt > 0.0 {
                let i = y * w + x;
                m[i] += (snapped[i] - m[i]) * wt;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::layer::{Layer, LayerKind, Raster};

    fn two_tone(w: u32, h: u32, split: u32) -> Document {
        let mut data = Vec::new();
        for _y in 0..h {
            for x in 0..w {
                if x < split {
                    data.extend_from_slice(&[180, 40, 40, 255]);
                } else {
                    data.extend_from_slice(&[40, 60, 170, 255]);
                }
            }
        }
        let mut doc = Document::new(w, h);
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "px", LayerKind::Pixel(Raster::new(Plane::from_raw(w, h, 4, &data, [0; 4]), 0, 0))));
        doc
    }

    #[test]
    fn radius_snaps_a_rough_edge_to_the_colour_boundary() {
        let doc = two_tone(120, 60, 60);
        let mut sel = Plane::mask(120, 60, 0);
        // Rough: selects up to x = 52 on top rows and x = 68 on the bottom.
        for y in 0..60 {
            let edge = if y < 30 { 52 } else { 68 };
            sel.write(Rect::new(0, y, edge, 1), &vec![255; edge as usize]);
        }
        let out = refine(&doc, &sel, &Params { radius: 12.0, ..Default::default() });
        for y in [10, 50] {
            assert!(out.get(56, y)[0] > 180, "red joins at y={y}: {}", out.get(56, y)[0]);
            assert!(out.get(64, y)[0] < 75, "blue leaves at y={y}: {}", out.get(64, y)[0]);
        }
        assert_eq!(out.get(10, 30)[0], 255, "far inside untouched");
        assert_eq!(out.get(110, 30)[0], 0, "far outside untouched");
    }

    #[test]
    fn feather_contrast_and_shift() {
        let doc = two_tone(80, 40, 40);
        let mut sel = Plane::mask(80, 40, 0);
        sel.write(Rect::new(0, 0, 40, 40), &[255; 1600]);
        let soft = refine(&doc, &sel, &Params { feather: 6.0, ..Default::default() });
        let edge = soft.get(40, 20)[0];
        assert!(edge > 30 && edge < 200, "{edge}");
        let hard = refine(&doc, &sel, &Params { feather: 6.0, contrast: 90.0, ..Default::default() });
        assert!(hard.get(36, 20)[0] > soft.get(36, 20)[0]);
        let out = refine(&doc, &sel, &Params { feather: 6.0, shift_edge: 60.0, ..Default::default() });
        assert!(out.get(43, 20)[0] > soft.get(43, 20)[0]);
    }
}
