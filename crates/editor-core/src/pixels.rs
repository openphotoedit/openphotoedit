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
            for c in 0..4 {
                let o = original[src + c] as u32;
                let n = work[src + c] as u32;
                out[dst + c] = ((o * (255 - cov) + n * cov + 127) / 255) as u8;
            }
            if lock_alpha {
                out[dst + 3] = original[src + 3];
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
}
