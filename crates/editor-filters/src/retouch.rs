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
        // Across the ring, hand over from the corrected synthesis to the
        // untouched original.
        membrane_correct(buf, &orig, w, h, &hole, &disc, |i| ((dist(i) - radius) / ring).clamp(0.0, 1.0));
    })
}

/// Spot Healing Brush over a painted stroke: heal everything the stroke
/// covered in one pass. `rect` is where the single-channel `coverage` sits in
/// the document; every pixel it touches (antialiased rim included) is
/// resynthesised from the texture around the stroke.
///
/// Healing the whole footprint at once lets every part of a long scratch
/// see clean surroundings, where dab-by-dab healing used still-damaged
/// neighbours as its boundary (skin scratch RMSE 12.9 → 5.7). The idea of
/// healing the painted coverage on release comes from Compositor's spot
/// heal (HealPixels.c, MIT). The synthesis is ours: measured on thin
/// strokes, the disc path's harmonic membrane only added error (the hole
/// has to grow by a ring for it), so the stroke is inpainted directly. The
/// rim is replaced outright, avoiding their `coverage × opacity` dark rim.
pub fn spot_heal_mask(doc: &mut Document, id: Option<LayerId>, rect: Rect, coverage: &[u8]) -> Result<Applied> {
    let id = target_id(doc, id)?;
    if rect.w <= 0 || rect.h <= 0 || coverage.len() != (rect.w as usize) * (rect.h as usize) {
        return Err(EditorError::Invalid(format!(
            "stroke mask must be {}×{} single-channel bytes, got {}",
            rect.w.max(0),
            rect.h.max(0),
            coverage.len()
        )));
    }
    // Tight box of the painted pixels.
    let (mw, mh) = (rect.w as usize, rect.h as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for y in 0..mh {
        for x in 0..mw {
            if coverage[y * mw + x] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 == usize::MAX {
        return Err(EditorError::Invalid("paint over the area to heal first".into()));
    }
    let area = Rect::new(rect.x + x0 as i32, rect.y + y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32).intersect(&canvas(doc));
    // As much surrounding texture as Content-Aware Fill looks at.
    let margin = area.w.max(area.h).clamp(40, 400);
    let outer = area.inflate(margin);
    let inside_rect = canvas(doc).intersect(&outer).translate(-outer.x, -outer.y);
    run_rect(doc, id, area, margin, false, "Spot Healing Brush", |buf, f| {
        let (w, h) = (f.w, f.h);
        let inside: Vec<bool> = (0..w * h).map(|i| inside_rect.contains((i % w) as i32, (i / w) as i32)).collect();
        let (ox, oy) = (rect.x - f.outer.x, rect.y - f.outer.y);
        let mut hole = vec![false; w * h];
        for my in 0..mh {
            let fy = my as i32 + oy;
            if fy < 0 || fy >= h as i32 {
                continue;
            }
            for mx in 0..mw {
                let fx = mx as i32 + ox;
                if fx >= 0 && fx < w as i32 && coverage[my * mw + mx] > 0 {
                    let i = fy as usize * w + fx as usize;
                    hole[i] = inside[i];
                }
            }
        }
        inpaint(buf, w, h, &hole, &inside, 0x5B07);
    })
}

/// Correct a synthesis over `hole` in the gradient domain: the difference to
/// the original on the ring (`hole` minus `core`) is smoothed, spread over
/// the core as a harmonic membrane and added; across the ring the result
/// hands over to the original by `t(i)` (0 at the core, 1 at the outer edge).
fn membrane_correct(buf: &mut [u8], orig: &[u8], w: usize, h: usize, hole: &[bool], core: &[bool], t: impl Fn(usize) -> f32) {
    let ring_mask: Vec<f32> = (0..w * h).map(|i| if hole[i] && !core[i] { 1.0 } else { 0.0 }).collect();
    let mut ring_norm = ring_mask.clone();
    box_blur(&mut ring_norm, w, h, 2.0);
    for c in 0..4 {
        let mut d: Vec<f32> = (0..w * h).map(|i| (orig[i * 4 + c] as f32 - buf[i * 4 + c] as f32) * ring_mask[i]).collect();
        box_blur(&mut d, w, h, 2.0);
        for i in 0..w * h {
            d[i] = if ring_mask[i] > 0.0 && ring_norm[i] > 1e-3 { d[i] / ring_norm[i] } else { 0.0 };
        }
        harmonic_fill(&mut d, w, h, core);
        for i in 0..w * h {
            if !hole[i] {
                continue;
            }
            let healed = buf[i * 4 + c] as f32 + d[i];
            let v = if core[i] {
                healed
            } else {
                let t = t(i);
                let t = t * t * (3.0 - 2.0 * t);
                healed + (orig[i * 4 + c] as f32 - healed) * t
            };
            buf[i * 4 + c] = to_u8(v);
        }
    }
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

#[cfg(test)]
mod tests {
    use editor_core::editor::Editor;
    use editor_core::geom::Rect;
    use editor_core::pixels::read_layer;
    use serde_json::json;

    fn photo_crop(name: &str, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/photos").join(name);
        let img = image::open(path).expect("decode portrait").to_rgba8();
        image::imageops::crop_imm(&img, x0, y0, w, h).to_image().into_raw()
    }

    fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let v = (b.0 - a.0, b.1 - a.1);
        let l2 = v.0 * v.0 + v.1 * v.1;
        let t = if l2 > 0.0 { (((p.0 - a.0) * v.0 + (p.1 - a.1) * v.1) / l2).clamp(0.0, 1.0) } else { 0.0 };
        ((p.0 - a.0 - t * v.0).powi(2) + (p.1 - a.1 - t * v.1).powi(2)).sqrt()
    }

    /// Antialiased coverage of a round-capped stroke along a polyline.
    fn stroke_mask(w: u32, h: u32, pts: &[(f32, f32)], size: f32) -> Vec<u8> {
        let r = size / 2.0;
        (0..w * h)
            .map(|i| {
                let p = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                let d = pts.windows(2).map(|s| seg_dist(p, s[0], s[1])).fold(f32::MAX, f32::min);
                ((r - d + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8
            })
            .collect()
    }

    fn open(px: &[u8], w: u32, h: u32) -> Editor {
        let mut ed = Editor::new(1, 1);
        crate::register(&mut ed);
        ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), px).unwrap();
        ed
    }

    fn layer(ed: &Editor) -> Vec<u8> {
        read_layer(&ed.doc, ed.doc.active.unwrap(), Rect::new(0, 0, ed.doc.width as i32, ed.doc.height as i32)).unwrap()
    }

    fn rmse(a: &[u8], b: &[u8], keep: impl Fn(usize) -> bool) -> f64 {
        let (mut s, mut n) = (0f64, 0f64);
        for i in 0..a.len() / 4 {
            if keep(i) {
                for c in 0..3 {
                    let d = a[i * 4 + c] as f64 - b[i * 4 + c] as f64;
                    s += d * d;
                    n += 1.0;
                }
            }
        }
        (s / n).sqrt()
    }

    /// A 16-px pale scratch painted across skin heals in one command, one
    /// undo step, at least as well as Content-Aware Fill on the same mask,
    /// with no rim and nothing changed away from the stroke.
    #[test]
    fn spot_heal_mask_heals_a_painted_scratch_in_one_step() {
        heal_case("skin scratch", "portrait.jpg", (1150, 330, 360, 240), &[(60.0, 90.0), (300.0, 140.0)], 16.0, [200.0, 190.0, 180.0], 7.0);
    }

    /// The same on gravel, a busy texture, along a bent stroke.
    #[test]
    fn spot_heal_mask_heals_a_scratch_on_gravel() {
        heal_case("gravel scratch", "landscape.jpg", (700, 1350, 500, 320), &[(80.0, 150.0), (220.0, 120.0), (420.0, 190.0)], 22.0, [30.0, 25.0, 20.0], 30.0);
    }

    fn heal_case(name: &str, photo: &str, crop: (u32, u32, u32, u32), pts: &[(f32, f32)], size: f32, color: [f32; 3], limit: f64) {
        let (w, h) = (crop.2, crop.3);
        let truth = photo_crop(photo, crop.0, crop.1, w, h);
        let mask = stroke_mask(w, h, pts, size);
        let mut damaged = truth.clone();
        for i in 0..mask.len() {
            let a = mask[i] as f32 / 255.0;
            for (c, v) in color.into_iter().enumerate() {
                damaged[i * 4 + c] = (damaged[i * 4 + c] as f32 * (1.0 - a) + v * a).round() as u8;
            }
        }
        // The UI sends the mask over the stroke's padded bounding box.
        let pad = size / 2.0 + 2.0;
        let bx = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min) - pad;
        let by = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min) - pad;
        let bx1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max) + pad;
        let by1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max) + pad;
        let (bx, by) = (bx.floor() as u32, by.floor() as u32);
        let (bw, bh) = (bx1.ceil() as u32 - bx, by1.ceil() as u32 - by);
        let sub: Vec<u8> = (by..by + bh).flat_map(|y| (bx..bx + bw).map(move |x| (y, x))).map(|(y, x)| mask[(y * w + x) as usize]).collect();
        assert_eq!(sub.iter().map(|&v| v as u32).sum::<u32>(), mask.iter().map(|&v| v as u32).sum::<u32>());

        let mut ed = open(&damaged, w, h);
        let before = ed.history.undo_labels().len();
        let t = std::time::Instant::now();
        ed.exec(json!({"op": "filter.spot-heal", "x": bx, "y": by, "width": bw, "height": bh, "radius": size / 2.0}), &sub).unwrap();
        let ms = t.elapsed().as_millis();
        assert_eq!(ed.history.undo_labels().len(), before + 1, "one stroke is one undo step");
        let healed = layer(&ed);

        let mut caf = open(&damaged, w, h);
        caf.exec(json!({"op": "select.mask", "x": 0, "y": 0, "width": w, "height": h}), &mask).unwrap();
        caf.exec(json!({"op": "filter.content-aware-fill"}), &[]).unwrap();
        let filled = layer(&caf);

        let e_heal = rmse(&truth, &healed, |i| mask[i] >= 128);
        let e_caf = rmse(&truth, &filled, |i| mask[i] >= 128);
        let e_damaged = rmse(&truth, &damaged, |i| mask[i] >= 128);
        // The antialiased rim: partly painted pixels.
        let rim = |i: usize| mask[i] > 0 && mask[i] < 255;
        let rim_bias: f64 = {
            let (mut s, mut n) = (0f64, 0f64);
            for i in (0..mask.len()).filter(|&i| rim(i)) {
                s += (0..3).map(|c| healed[i * 4 + c] as f64 - truth[i * 4 + c] as f64).sum::<f64>() / 3.0;
                n += 1.0;
            }
            s / n
        };
        eprintln!("{name}: spot heal mask rmse {e_heal:.2} (content-aware fill {e_caf:.2}, damaged {e_damaged:.2}), rim bias {rim_bias:.2}, {ms} ms");
        assert!(e_heal <= limit, "heal rmse {e_heal:.2}");
        assert!(e_heal <= e_caf + 0.5, "heal {e_heal:.2} vs fill {e_caf:.2}");
        assert!(rim_bias.abs() <= 6.0, "rim bias {rim_bias:.2}");
        // Away from the stroke and its hand-over ring nothing moves.
        let far = stroke_mask(w, h, pts, size + 2.0 * 20.0);
        for i in 0..mask.len() {
            if far[i] == 0 {
                assert_eq!(&healed[i * 4..i * 4 + 4], &damaged[i * 4..i * 4 + 4], "pixel {i} changed");
            }
        }
    }

    #[test]
    fn spot_heal_mask_rejects_a_wrong_sized_mask() {
        let mut ed = open(&[128u8; 64 * 64 * 4], 64, 64);
        assert!(ed.exec(json!({"op": "filter.spot-heal", "x": 0, "y": 0, "width": 10, "height": 10}), &[255u8; 50]).is_err());
        assert!(ed.exec(json!({"op": "filter.spot-heal", "x": 0, "y": 0, "width": 10, "height": 10}), &[0u8; 100]).is_err());
        // The disc form still works.
        ed.exec(json!({"op": "filter.spot-heal", "x": 32, "y": 32, "radius": 6}), &[]).unwrap();
    }
}
