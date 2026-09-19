//! Helpers every destructive pixel operation shares: find the editable
//! layer, read a region with an apron, let the operation change it, and write
//! it back through the selection and the transparency lock.
//!
//! Filters, brushes, AI results and healing all go through [`edit_layer`], so
//! "respects the selection" and "respects locks" are implemented once.

use crate::document::Document;
use crate::geom::Rect;
use crate::layer::{LayerId, LayerKind};
use crate::ops::{EditorError, Result};
use crate::render::{render_view, to_u8, View};
use crate::selection;

/// What an edit may touch.
#[derive(Clone, Copy, Debug)]
pub struct EditScope {
    /// Document rectangle whose pixels the operation writes.
    pub rect: Rect,
    /// Extra pixels read around `rect` (blur radius etc.).
    pub apron: i32,
}

/// The rectangle a whole-layer operation should cover: the selection's
/// bounds, or the layer's pixels, clipped to the canvas.
pub fn default_rect(doc: &Document, id: LayerId) -> Result<Rect> {
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let layer = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    let base = if doc.selection.is_some() {
        selection::bounds(doc)
    } else {
        match layer.raster() {
            Some(r) => r.plane.content_bounds().translate(r.x, r.y),
            None => canvas,
        }
    };
    Ok(base.intersect(&canvas))
}

fn check_editable(doc: &Document, id: LayerId) -> Result<()> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    if l.locks.all || l.locks.pixels {
        return Err(EditorError::Locked(l.name.clone()));
    }
    match l.kind {
        LayerKind::Pixel(_) => Ok(()),
        LayerKind::Text { .. } | LayerKind::Shape { .. } => Err(EditorError::Invalid(format!(
            "`{}` is a {} layer; rasterize it first",
            l.name,
            l.kind.name()
        ))),
        _ => Err(EditorError::Invalid(format!("`{}` has no pixels to edit", l.name))),
    }
}

/// Read a layer's own RGBA over a document rectangle (transparent outside
/// its pixels).
pub fn read_layer(doc: &Document, id: LayerId, rect: Rect) -> Result<Vec<u8>> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    let r = l.raster().ok_or_else(|| EditorError::Invalid(format!("`{}` has no pixels", l.name)))?;
    Ok(r.plane.read_vec(rect.translate(-r.x, -r.y)))
}

/// Read the visible composite over a document rectangle at 1:1.
pub fn read_merged(doc: &Document, rect: Rect) -> Vec<u8> {
    let view = View { x: rect.x as f64, y: rect.y as f64, scale: 1.0, width: rect.w.max(0) as usize, height: rect.h.max(0) as usize };
    let f = render_view(doc, view);
    let mut out = vec![0u8; f.len()];
    to_u8(&f, &mut out);
    out
}

/// Selection coverage over a document rectangle (255 everywhere when nothing
/// is selected).
pub fn read_selection(doc: &Document, rect: Rect) -> Vec<u8> {
    match &doc.selection {
        None => vec![255; rect.area().max(0) as usize],
        Some(s) => s.read_vec(rect),
    }
}

/// Edit a pixel layer in place.
///
/// `f` receives RGBA for `scope.rect` inflated by `scope.apron` (row-major,
/// straight alpha) plus its width and height, and modifies it. Only the
/// inner `scope.rect` is written back, mixed with the original by selection
/// coverage; a transparency lock keeps the original alpha. The layer grows if
/// the rectangle extends past its current pixels.
pub fn edit_layer(doc: &mut Document, id: LayerId, scope: EditScope, f: impl FnOnce(&mut [u8], usize, usize)) -> Result<Rect> {
    check_editable(doc, id)?;
    let rect = scope.rect;
    if rect.is_empty() {
        return Ok(rect);
    }
    let outer = rect.inflate(scope.apron.max(0));
    let original = read_layer(doc, id, outer)?;
    let mut work = original.clone();
    f(&mut work, outer.w as usize, outer.h as usize);

    let sel = read_selection(doc, rect);
    let has_sel = doc.selection.is_some();
    let layer = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
    let lock_alpha = layer.locks.transparency;
    let LayerKind::Pixel(raster) = &mut layer.kind else { unreachable!() };
    raster.ensure_covers(rect);

    let (ow, ax) = (outer.w as usize, scope.apron.max(0) as usize);
    let mut out = vec![0u8; rect.area() as usize * 4];
    for y in 0..rect.h as usize {
        for x in 0..rect.w as usize {
            let src = ((y + ax) * ow + x + ax) * 4;
            let dst = (y * rect.w as usize + x) * 4;
            let cov = if has_sel { sel[y * rect.w as usize + x] as u32 } else { 255 };
            let (o, n) = (&original[src..src + 4], &work[src..src + 4]);
            let px = &mut out[dst..dst + 4];
            if cov == 255 {
                px.copy_from_slice(n);
            } else if cov == 0 {
                px.copy_from_slice(o);
            } else {
                // Mix premultiplied: a straight mix would pull the colour
                // that a transparent side stores (usually black) into the
                // result, greying soft edges of blurred layers.
                let wo = o[3] as u32 * (255 - cov);
                let wn = n[3] as u32 * cov;
                let wsum = wo + wn;
                for c in 0..3 {
                    px[c] = if wsum > 0 {
                        ((o[c] as u32 * wo + n[c] as u32 * wn + wsum / 2) / wsum) as u8
                    } else {
                        ((o[c] as u32 * (255 - cov) + n[c] as u32 * cov + 127) / 255) as u8
                    };
                }
                px[3] = ((wsum + 127) / 255) as u8;
            }
            if lock_alpha {
                px[3] = o[3];
            }
        }
    }
    raster.plane.write(rect.translate(-raster.x, -raster.y), &out);
    raster.plane.compact();
    Ok(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::plane::Plane;

    #[test]
    fn edit_respects_selection_and_apron() {
        let mut ed = Editor::new(1, 1);
        let px: Vec<u8> = (0..8 * 8).flat_map(|_| [10u8, 10, 10, 255]).collect();
        ed.exec_json(r#"{"op":"doc.open-pixels","width":8,"height":8}"#, &px).unwrap();
        let id = ed.doc.active.unwrap();
        let mut sel = Plane::mask(8, 8, 0);
        sel.write(Rect::new(0, 0, 4, 8), &[255; 32]);
        ed.doc.selection = Some(sel);
        let rect = default_rect(&ed.doc, id).unwrap();
        assert_eq!(rect, Rect::new(0, 0, 4, 8));
        edit_layer(&mut ed.doc, id, EditScope { rect: Rect::new(0, 0, 8, 8), apron: 2 }, |buf, w, h| {
            assert_eq!((w, h), (12, 12));
            for p in buf.chunks_exact_mut(4) {
                p[0] = 200;
            }
        })
        .unwrap();
        let l = read_layer(&ed.doc, id, Rect::new(0, 0, 8, 1)).unwrap();
        assert_eq!(l[0], 200);
        assert_eq!(l[7 * 4], 10, "unselected pixels untouched");
    }

    /// 40×9 layer, opaque white for x < 20 and transparent beyond, with a
    /// soft selection edge over x 16..28. Returns the editor and layer id.
    fn soft_edge_setup(lock_alpha: bool) -> (Editor, LayerId) {
        let (w, h) = (40usize, 9usize);
        let px: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 20 { [255u8; 4] } else { [0u8; 4] }).collect();
        let mut ed = Editor::new(1, 1);
        ed.exec_json(r#"{"op":"doc.open-pixels","width":40,"height":9}"#, &px).unwrap();
        let id = ed.doc.active.unwrap();
        ed.doc.find_mut(id).unwrap().locks.transparency = lock_alpha;
        let row: Vec<u8> = (0..w).map(|x| if x < 16 { 255 } else if x >= 28 { 0 } else { (255 - (x - 16) * 255 / 12) as u8 }).collect();
        let sel: Vec<u8> = (0..h).flat_map(|_| row.clone()).collect();
        let mut plane = Plane::mask(w as u32, h as u32, 0);
        plane.write(Rect::new(0, 0, w as i32, h as i32), &sel);
        ed.doc.selection = Some(plane);
        (ed, id)
    }

    /// A premultiplied horizontal box blur, the way filters blur alpha.
    fn blur_row(buf: &mut [u8], w: usize, h: usize, r: usize) {
        let src = buf.to_vec();
        for y in 0..h {
            for x in 0..w {
                let (mut c, mut a, mut n) = ([0f32; 3], 0f32, 0f32);
                for xx in x.saturating_sub(r)..(x + r + 1).min(w) {
                    let p = &src[(y * w + xx) * 4..(y * w + xx) * 4 + 4];
                    let pa = p[3] as f32;
                    for k in 0..3 {
                        c[k] += p[k] as f32 * pa;
                    }
                    a += pa;
                    n += 1.0;
                }
                let d = &mut buf[(y * w + x) * 4..(y * w + x) * 4 + 4];
                for k in 0..3 {
                    d[k] = if a > 0.0 { (c[k] / a).round() as u8 } else { 0 };
                }
                d[3] = (a / n).round() as u8;
            }
        }
    }

    /// Gap B §3.7: blurring a white edge under a feathered selection turned
    /// it grey (214, 147, 71 at x = 20, 22, 24) because original and result
    /// were mixed as straight RGBA, pulling in the transparent pixels'
    /// stored black. Mixed premultiplied, white stays white.
    #[test]
    fn soft_selection_does_not_grey_transparent_edges() {
        let (mut ed, id) = soft_edge_setup(false);
        let rect = Rect::new(0, 0, 40, 9);
        edit_layer(&mut ed.doc, id, EditScope { rect, apron: 3 }, |b, w, h| blur_row(b, w, h, 3)).unwrap();
        let px = read_layer(&ed.doc, id, Rect::new(0, 4, 40, 1)).unwrap();
        for x in 0..40 {
            let p = &px[x * 4..x * 4 + 4];
            if p[3] > 0 {
                assert!(p[0] >= 254 && p[1] >= 254 && p[2] >= 254, "x = {x}: grey fringe {p:?}");
            }
        }
        assert!(px[20 * 4 + 3] > 0 && px[20 * 4 + 3] < 255, "the edge was actually blurred");
    }

    /// Fully selected pixels take the edit exactly, unselected pixels keep
    /// the original exactly, with or without a transparency lock: the same
    /// bytes the straight mix produced.
    #[test]
    fn full_and_zero_coverage_are_exact() {
        for lock in [false, true] {
            let (w, h) = (16usize, 4usize);
            let px: Vec<u8> = (0..w * h * 4).map(|i| (i * 37 % 251) as u8).collect();
            let mut ed = Editor::new(1, 1);
            ed.exec_json(r#"{"op":"doc.open-pixels","width":16,"height":4}"#, &px).unwrap();
            let id = ed.doc.active.unwrap();
            ed.doc.find_mut(id).unwrap().locks.transparency = lock;
            let mut sel = Plane::mask(16, 4, 0);
            sel.write(Rect::new(0, 0, 8, 4), &[255; 32]);
            ed.doc.selection = Some(sel);
            let edited: Vec<u8> = (0..w * h * 4).map(|i| (i * 91 % 241) as u8).collect();
            let e2 = edited.clone();
            edit_layer(&mut ed.doc, id, EditScope { rect: Rect::new(0, 0, 16, 4), apron: 0 }, move |b, _, _| b.copy_from_slice(&e2)).unwrap();
            let got = read_layer(&ed.doc, id, Rect::new(0, 0, 16, 4)).unwrap();
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    let mut want: [u8; 4] = if x < 8 { edited[i..i + 4].try_into().unwrap() } else { px[i..i + 4].try_into().unwrap() };
                    if lock {
                        want[3] = px[i + 3];
                    }
                    // A pixel that ends fully transparent has no colour.
                    if want[3] != 0 || got[i + 3] != 0 {
                        assert_eq!(&got[i..i + 4], &want, "lock {lock}, ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn soft_selection_with_locked_alpha_keeps_alpha_and_colour() {
        let (mut ed, id) = soft_edge_setup(true);
        let rect = Rect::new(0, 0, 40, 9);
        edit_layer(&mut ed.doc, id, EditScope { rect, apron: 3 }, |b, w, h| blur_row(b, w, h, 3)).unwrap();
        let px = read_layer(&ed.doc, id, Rect::new(0, 4, 40, 1)).unwrap();
        for x in 0..40 {
            let p = &px[x * 4..x * 4 + 4];
            assert_eq!(p[3], if x < 20 { 255 } else { 0 }, "alpha locked at {x}");
            if p[3] > 0 {
                assert_eq!(&p[..3], &[255, 255, 255], "x = {x}");
            }
        }
    }
}
