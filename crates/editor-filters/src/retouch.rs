//! Retouching: Content-Aware Fill, Spot Healing, Red Eye and Frequency
//! Separation.

use editor_core::blend::BlendMode;
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{Layer, LayerId, LayerKind, Raster};
use editor_core::ops::{target_id, Applied, EditorError, Result};
use editor_core::pixels::read_layer;
use editor_core::plane::Plane;

use crate::blur::{box_blur, gaussian_reach};
use crate::inpaint::{harmonic_fill, inpaint};
use crate::util::{area_rect, canvas, linear_filter, pad_clamp, run_rect, to_u8, Area};

/// Content-Aware Fill: replace the selection with texture synthesised from
/// the area around it.
pub fn content_aware_fill(doc: &mut Document, id: Option<LayerId>, seed: u64) -> Result<Applied> {
    let id = target_id(doc, id)?;
    let Some(sel) = doc.selection.clone() else {
        return Err(EditorError::Invalid("select the area to fill first".into()));
    };
    let rect = area_rect(doc, id, Area::Content(0))?;
    let margin = (rect.w.max(rect.h)).clamp(40, 400);
    let outer = rect.inflate(margin);
    let cov = sel.read_vec(outer);
    let inside_rect = canvas(doc).intersect(&outer).translate(-outer.x, -outer.y);
    let (ow, oh) = (outer.w.max(0) as usize, outer.h.max(0) as usize);
    let inside: Vec<bool> = (0..ow * oh).map(|i| inside_rect.contains((i % ow) as i32, (i / ow) as i32)).collect();
    let hole: Vec<bool> = (0..ow * oh).map(|i| cov[i] > 0 && inside[i]).collect();
    let usable = (0..ow * oh).filter(|&i| inside[i] && !hole[i]).count();
    if !rect.is_empty() && usable < 64 {
        return Err(EditorError::Invalid("there is not enough image around the selection to fill from".into()));
    }
    run_rect(doc, id, rect, margin, false, "Content-Aware Fill", |buf, f| {
        debug_assert_eq!(f.w * f.h, hole.len());
        inpaint(buf, f.w, f.h, &hole, &inside, seed);
    })
}

/// Spot Healing Brush: synthesise the spot and a ring around it from nearby
/// texture, then correct the synthesis in the gradient domain — a harmonic
/// membrane matching it to the original ring — so its lighting and colour
/// agree with the surroundings.
pub fn spot_heal(doc: &mut Document, id: Option<LayerId>, x: f32, y: f32, radius: f32) -> Result<Applied> {
    let id = target_id(doc, id)?;
    let radius = radius.clamp(1.0, 500.0);
    let ring = (radius * 0.35).max(3.0);
    let outer_r = radius + ring;
    let rect = Rect::cover((x - outer_r) as f64, (y - outer_r) as f64, (2.0 * outer_r) as f64, (2.0 * outer_r) as f64).intersect(&canvas(doc));
    let margin = ((3.0 * radius) as i32).clamp(16, 300);
    let outer = rect.inflate(margin);
    let inside_rect = canvas(doc).intersect(&outer).translate(-outer.x, -outer.y);
    run_rect(doc, id, rect, margin, false, "Spot Healing Brush", |buf, f| {
        let (w, h) = (f.w, f.h);
        let (cx, cy) = (x - f.outer.x as f32, y - f.outer.y as f32);
        let dist = |i: usize| {
            let (px, py) = ((i % w) as f32 + 0.5 - cx, (i / w) as f32 + 0.5 - cy);
            (px * px + py * py).sqrt()
        };
        let inside: Vec<bool> = (0..w * h).map(|i| inside_rect.contains((i % w) as i32, (i / w) as i32)).collect();
        let hole: Vec<bool> = (0..w * h).map(|i| inside[i] && dist(i) <= outer_r).collect();
        let disc: Vec<bool> = (0..w * h).map(|i| inside[i] && dist(i) <= radius).collect();
        let orig = buf.to_vec();
        inpaint(buf, w, h, &hole, &inside, 0x5B07);
        let ring_mask: Vec<f32> = (0..w * h).map(|i| if hole[i] && !disc[i] { 1.0 } else { 0.0 }).collect();
        let mut ring_norm = ring_mask.clone();
        box_blur(&mut ring_norm, w, h, 2.0);
        for c in 0..4 {
            // Difference between the original and the synthesis on the ring,
            // smoothed within the ring so single noisy pixels do not print.
            let mut d: Vec<f32> = (0..w * h).map(|i| (orig[i * 4 + c] as f32 - buf[i * 4 + c] as f32) * ring_mask[i]).collect();
            box_blur(&mut d, w, h, 2.0);
            for i in 0..w * h {
                d[i] = if ring_mask[i] > 0.0 && ring_norm[i] > 1e-3 { d[i] / ring_norm[i] } else { 0.0 };
            }
            harmonic_fill(&mut d, w, h, &disc);
            for i in 0..w * h {
                if !hole[i] {
                    continue;
                }
                let healed = buf[i * 4 + c] as f32 + d[i];
                let v = if disc[i] {
                    healed
                } else {
                    // Across the ring, hand over from the corrected synthesis
                    // to the untouched original.
                    let t = ((dist(i) - radius) / ring).clamp(0.0, 1.0);
                    let t = t * t * (3.0 - 2.0 * t);
                    healed + (orig[i * 4 + c] as f32 - healed) * t
                };
                buf[i * 4 + c] = to_u8(v);
            }
        }
    })
}

/// Red Eye: find strongly red pixels inside the rectangle (weighted toward
/// an ellipse sized by `pupil_size`) and replace their red with a dark
/// neutral built from green and blue, which keeps the catchlight.
pub fn red_eye(doc: &mut Document, id: Option<LayerId>, rect: Rect, pupil_size: f32, darken: f32) -> Result<Applied> {
    let id = target_id(doc, id)?;
    if rect.w <= 0 || rect.h <= 0 {
        return Err(EditorError::Invalid("drag a box around the eye first".into()));
    }
    let area = rect.intersect(&canvas(doc));
    let (rx, ry) = (rect.x as f32 + rect.w as f32 / 2.0, rect.y as f32 + rect.h as f32 / 2.0);
    let scale = 0.5 + pupil_size / 100.0;
    let (ax, ay) = (rect.w as f32 / 2.0 * scale, rect.h as f32 / 2.0 * scale);
    run_rect(doc, id, area, 2, true, "Red Eye", |buf, f| {
        let (w, h) = (f.w, f.h);
        let mut mask = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let p = &buf[i * 4..i * 4 + 4];
                let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
                let redness = (r - g.max(b)) / (r + 1.0);
                // Skin is red-orange (redness ≈ 0.1–0.25); a flash-lit pupil is far redder.
                let strength = ((redness - 0.3) / 0.2).clamp(0.0, 1.0) * (r / 60.0).min(1.0);
                let (nx, ny) = ((f.doc_x(x) as f32 + 0.5 - rx) / ax.max(0.5), (f.doc_y(y) as f32 + 0.5 - ry) / ay.max(0.5));
                let e = (nx * nx + ny * ny).sqrt();
                let region = 1.0 - ((e - 0.75) / 0.25).clamp(0.0, 1.0);
                mask[i] = strength * region;
            }
        }
        box_blur(&mut mask, w, h, 1.0);
        let dark = 1.0 - 0.75 * darken / 100.0;
        for i in 0..w * h {
            let m = mask[i];
            if m <= 0.0 {
                continue;
            }
            let p = &mut buf[i * 4..i * 4 + 4];
            let target = (p[1] as f32 + p[2] as f32) / 2.0 * dark;
            for c in 0..3 {
                let v = p[c] as f32;
                p[c] = to_u8(v + (target - v) * m.min(1.0));
            }
        }
    })
}

/// Frequency Separation: a "Low frequency" layer (Gaussian blur) and a
/// "High frequency" layer set to Linear Light above the target. The pair
/// reproduces the original exactly: the low layer is nudged by at most one
/// level where needed so the 8-bit high layer never has to round.
pub fn frequency_separation(doc: &mut Document, id: Option<LayerId>, radius: f32) -> Result<Applied> {
    let id = target_id(doc, id)?;
    let layer = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    let raster = layer.raster().ok_or_else(|| EditorError::Invalid(format!("`{}` has no pixels to separate", layer.name)))?;
    let bounds = raster.doc_rect();
    if bounds.is_empty() {
        return Err(EditorError::Invalid("the layer has no pixels to separate".into()));
    }
    let apron = gaussian_reach(radius);
    let outer = bounds.inflate(apron);
    let mut buf = read_layer(doc, id, outer)?;
    let (ow, oh) = (outer.w as usize, outer.h as usize);
    // Beyond the canvas, repeat the canvas edge (as the blur filter does).
    let keep = canvas(doc).intersect(&outer).translate(-outer.x, -outer.y);
    pad_clamp(&mut buf, ow, oh, keep);
    let orig = buf.clone();
    linear_filter(&mut buf, ow, oh, |p, w, h| crate::blur::gaussian(p, w, h, radius));
    let (bw, bh) = (bounds.w as usize, bounds.h as usize);
    let mut low = vec![0u8; bw * bh * 4];
    let mut high = vec![0u8; bw * bh * 4];
    let a = apron as usize;
    for y in 0..bh {
        for x in 0..bw {
            let s = ((y + a) * ow + x + a) * 4;
            let d = (y * bw + x) * 4;
            for c in 0..3 {
                let o = orig[s + c] as i32;
                let mut l = buf[s + c] as i32;
                if (o - l) % 2 == 0 {
                    l = if l < 255 { l + 1 } else { l - 1 };
                }
                low[d + c] = l as u8;
                high[d + c] = ((o - l + 255) / 2) as u8;
            }
            low[d + 3] = orig[s + 3];
            high[d + 3] = orig[s + 3];
        }
    }
    let low_id = doc.alloc_id();
    let low_layer = Layer::new(low_id, "Low frequency", LayerKind::Pixel(Raster::new(Plane::from_raw(bw as u32, bh as u32, 4, &low, [0; 4]), bounds.x, bounds.y)));
    doc.insert_above(Some(id), low_layer);
    let high_id = doc.alloc_id();
    let mut high_layer = Layer::new(high_id, "High frequency", LayerKind::Pixel(Raster::new(Plane::from_raw(bw as u32, bh as u32, 4, &high, [0; 4]), bounds.x, bounds.y)));
    high_layer.blend = BlendMode::LinearLight;
    doc.insert_above(Some(low_id), high_layer);
    doc.active = Some(high_id);
    Ok(Applied::step("Frequency Separation").with_data(serde_json::json!({ "ids": [low_id, high_id] })).dirty(bounds))
}
