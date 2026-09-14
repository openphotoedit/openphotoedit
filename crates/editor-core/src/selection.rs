//! Selections: a document-sized single-channel plane where 255 is fully
//! selected. `Document::selection == None` means nothing is selected, which
//! tools treat as "everything".

use serde::{Deserialize, Serialize};

use crate::adjust::box_blur_1ch;
use crate::document::Document;
use crate::geom::Rect;
use crate::plane::{Plane, TILE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectMode {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

fn tile_rects(w: u32, h: u32) -> impl Iterator<Item = Rect> {
    let cols = w.div_ceil(TILE);
    let rows = h.div_ceil(TILE);
    let full = Rect::new(0, 0, w as i32, h as i32);
    (0..rows).flat_map(move |ty| {
        (0..cols).map(move |tx| Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32).intersect(&full))
    })
}

/// Combine a new coverage plane into the document selection.
pub fn combine(doc: &mut Document, new: Plane, mode: SelectMode) {
    let (w, h) = (doc.width, doc.height);
    let current = doc.selection.take();
    let result = match (mode, current) {
        (SelectMode::Replace, _) | (SelectMode::Add, None) => new,
        (SelectMode::Subtract, None) => {
            // Subtracting from "everything".
            let mut all = Plane::mask(w, h, 255);
            merge(&mut all, &new, |a, b| (a as u32 * (255 - b as u32) / 255) as u8);
            all
        }
        (SelectMode::Intersect, None) => new,
        (SelectMode::Add, Some(mut cur)) => {
            merge(&mut cur, &new, |a, b| a.max(b));
            cur
        }
        (SelectMode::Subtract, Some(mut cur)) => {
            merge(&mut cur, &new, |a, b| (a as u32 * (255 - b as u32) / 255) as u8);
            cur
        }
        (SelectMode::Intersect, Some(mut cur)) => {
            merge(&mut cur, &new, |a, b| a.min(b));
            cur
        }
    };
    doc.selection = normalise(result);
}

/// A selection with nothing in it is stored as `None`… but `None` means
/// "everything" to tools. An empty selection is therefore kept as an empty
/// plane, and only a deliberate "deselect" clears it.
fn normalise(mut p: Plane) -> Option<Plane> {
    p.compact();
    Some(p)
}

fn merge(dst: &mut Plane, src: &Plane, f: impl Fn(u8, u8) -> u8) {
    let (w, h) = (dst.width(), dst.height());
    for r in tile_rects(w, h) {
        let a_present = dst.tile(r.x as u32 / TILE, r.y as u32 / TILE).is_some();
        let b_present = src.tile(r.x as u32 / TILE, r.y as u32 / TILE).is_some();
        if !a_present && !b_present {
            let v = f(dst.fill()[0], src.fill()[0]);
            if v == dst.fill()[0] {
                continue;
            }
        }
        let mut a = dst.read_vec(r);
        let b = src.read_vec(r);
        for (x, y) in a.iter_mut().zip(b.iter()) {
            *x = f(*x, *y);
        }
        dst.write(r, &a);
    }
    dst.compact();
}

pub fn invert(doc: &mut Document) {
    let (w, h) = (doc.width, doc.height);
    let sel = match doc.selection.take() {
        None => Plane::mask(w, h, 0),
        Some(cur) => {
            let mut out = Plane::mask(w, h, 255);
            merge(&mut out, &cur, |_, b| 255 - b);
            out
        }
    };
    doc.selection = Some(sel);
}

/// Gaussian-like feather (three box passes) over the selection's content.
pub fn feather(plane: &mut Plane, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    let r = radius.ceil() as i32;
    let bounds = plane.content_bounds();
    // Also blur the edge where "fill" meets content.
    let area = if bounds.is_empty() { return } else { bounds.inflate(r * 3).intersect(&plane.bounds()) };
    let data = plane.read_vec(area);
    let mut f: Vec<f32> = data.iter().map(|&v| v as f32).collect();
    let pass_r = ((radius / 1.7).round() as usize).max(1);
    box_blur_1ch(&mut f, area.w as usize, area.h as usize, pass_r, 3);
    let out: Vec<u8> = f.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    plane.write(area, &out);
    plane.compact();
}

/// Bounds of selected pixels, or the canvas when nothing is selected.
pub fn bounds(doc: &Document) -> Rect {
    match &doc.selection {
        None => Rect::new(0, 0, doc.width as i32, doc.height as i32),
        Some(s) => {
            if s.fill()[0] > 0 {
                Rect::new(0, 0, doc.width as i32, doc.height as i32)
            } else {
                s.content_bounds()
            }
        }
    }
}

/// Coverage 0..1 at a document pixel.
pub fn coverage(doc: &Document, x: i32, y: i32) -> f32 {
    match &doc.selection {
        None => 1.0,
        Some(s) => s.get(x, y)[0] as f32 / 255.0,
    }
}

/// Selection from a layer's transparency.
pub fn from_alpha(doc: &Document, raster: &crate::layer::Raster) -> Plane {
    let mut out = Plane::mask(doc.width, doc.height, 0);
    for r in raster.plane.present_rects() {
        let data = raster.plane.read_vec(r);
        let alpha: Vec<u8> = data.chunks_exact(4).map(|p| p[3]).collect();
        out.write(r.translate(raster.x, raster.y), &alpha);
    }
    out.compact();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ellipse_mask;

    fn rect_mask(w: u32, h: u32, r: Rect) -> Plane {
        let mut p = Plane::mask(w, h, 0);
        p.write(r, &vec![255; r.area() as usize]);
        p
    }

    #[test]
    fn add_subtract_intersect() {
        let mut doc = Document::new(600, 300);
        combine(&mut doc, rect_mask(600, 300, Rect::new(0, 0, 100, 100)), SelectMode::Replace);
        combine(&mut doc, rect_mask(600, 300, Rect::new(300, 0, 100, 100)), SelectMode::Add);
        assert_eq!(bounds(&doc), Rect::new(0, 0, 400, 100));
        combine(&mut doc, rect_mask(600, 300, Rect::new(0, 0, 50, 300)), SelectMode::Subtract);
        assert_eq!(bounds(&doc), Rect::new(50, 0, 350, 100));
        combine(&mut doc, rect_mask(600, 300, Rect::new(320, 0, 10, 10)), SelectMode::Intersect);
        assert_eq!(bounds(&doc), Rect::new(320, 0, 10, 10));
    }

    #[test]
    fn invert_twice_restores() {
        let mut doc = Document::new(64, 64);
        combine(&mut doc, ellipse_mask(64, 64, (8.0, 8.0, 40.0, 40.0), true), SelectMode::Replace);
        let before = doc.selection.as_ref().unwrap().to_raw();
        invert(&mut doc);
        assert_eq!(coverage(&doc, 0, 0), 1.0);
        invert(&mut doc);
        assert_eq!(doc.selection.as_ref().unwrap().to_raw(), before);
    }

    #[test]
    fn feather_softens_edge() {
        let mut p = rect_mask(100, 100, Rect::new(30, 30, 40, 40));
        feather(&mut p, 6.0);
        let edge = p.get(30, 50)[0];
        assert!(edge > 40 && edge < 220, "edge {edge}");
        assert_eq!(p.get(50, 50)[0], 255);
    }
}
