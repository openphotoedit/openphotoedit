//! Rasterising vector shapes (annotations, live shapes, redaction boxes) and
//! polygon selections, with tiny-skia.

use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

use crate::color::Rgba8;
use crate::geom::{Point, Rect};
use crate::layer::{Raster, ShapeData, ShapeKind};
use crate::plane::Plane;

fn paint(c: Rgba8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, c.a);
    p.anti_alias = true;
    p
}

fn rect_from(a: Point, b: Point) -> (f32, f32, f32, f32) {
    let x0 = a.x.min(b.x) as f32;
    let y0 = a.y.min(b.y) as f32;
    let x1 = a.x.max(b.x) as f32;
    let y1 = a.y.max(b.y) as f32;
    (x0, y0, (x1 - x0).max(0.5), (y1 - y0).max(0.5))
}

fn rounded_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.0 {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h).unwrap());
        return;
    }
    let k = 0.552_284_8 * r;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
}

/// Document-space bounds a shape will draw into.
pub fn shape_bounds(s: &ShapeData) -> Rect {
    if s.points.is_empty() {
        return Rect::empty();
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &s.points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let arrow = if matches!(s.kind, ShapeKind::Arrow) { s.stroke_width as f64 * 4.0 + 8.0 } else { 0.0 };
    let pad = s.stroke_width as f64 + arrow + 2.0;
    Rect::cover(x0 - pad, y0 - pad, x1 - x0 + 2.0 * pad, y1 - y0 + 2.0 * pad)
}

/// Rasterise a shape into a raster positioned at its bounds.
pub fn rasterize_shape(s: &ShapeData) -> Raster {
    let b = shape_bounds(s);
    if b.is_empty() {
        return Raster::empty(1, 1);
    }
    let Some(mut pm) = Pixmap::new(b.w as u32, b.h as u32) else { return Raster::empty(1, 1) };
    let tf = Transform::from_translate(-b.x as f32, -b.y as f32);
    let pts = &s.points;
    let mut stroke = Stroke { width: s.stroke_width.max(0.5), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    if let Some([on, off]) = s.dash {
        stroke.dash = StrokeDash::new(vec![on.max(0.5), off.max(0.5)], 0.0);
    }
    match s.kind {
        ShapeKind::Rect | ShapeKind::Redact | ShapeKind::Ellipse if pts.len() >= 2 => {
            let (x, y, w, h) = rect_from(pts[0], pts[1]);
            let mut pb = PathBuilder::new();
            if matches!(s.kind, ShapeKind::Ellipse) {
                pb.push_oval(tiny_skia::Rect::from_xywh(x, y, w, h).unwrap());
            } else {
                rounded_rect(&mut pb, x, y, w, h, s.corner_radius);
            }
            if let Some(path) = pb.finish() {
                let fill = if matches!(s.kind, ShapeKind::Redact) { Some(s.fill.unwrap_or(Rgba8::BLACK)) } else { s.fill };
                if let Some(f) = fill {
                    let mut p = paint(f);
                    // Redaction must cover completely: no anti-aliased
                    // half-transparent edge that leaks the pixel beneath.
                    if matches!(s.kind, ShapeKind::Redact) {
                        p.anti_alias = false;
                    }
                    pm.fill_path(&path, &p, FillRule::Winding, tf, None);
                }
                if let (Some(c), false) = (s.stroke, matches!(s.kind, ShapeKind::Redact)) {
                    pm.stroke_path(&path, &paint(c), &stroke, tf, None);
                }
            }
        }
        ShapeKind::Line | ShapeKind::Arrow if pts.len() >= 2 => {
            let (a, e) = (pts[0], pts[pts.len() - 1]);
            let color = s.stroke.unwrap_or(Rgba8::BLACK);
            let head = (s.stroke_width * 3.2 + 6.0) as f64;
            let len = ((e.x - a.x).powi(2) + (e.y - a.y).powi(2)).sqrt().max(1e-6);
            let (ux, uy) = ((e.x - a.x) / len, (e.y - a.y) / len);
            let arrow = matches!(s.kind, ShapeKind::Arrow);
            // Shorten the shaft so its round cap does not poke through the head.
            let trim_end = if arrow && s.arrow_end { head * 0.7 } else { 0.0 };
            let trim_start = if arrow && s.arrow_start { head * 0.7 } else { 0.0 };
            let mut pb = PathBuilder::new();
            pb.move_to((a.x + ux * trim_start) as f32, (a.y + uy * trim_start) as f32);
            pb.line_to((e.x - ux * trim_end) as f32, (e.y - uy * trim_end) as f32);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &paint(color), &stroke, tf, None);
            }
            let mut draw_head = |tip: Point, dx: f64, dy: f64| {
                let (bx, by) = (tip.x - dx * head, tip.y - dy * head);
                let (nx, ny) = (-dy * head * 0.55, dx * head * 0.55);
                let mut pb = PathBuilder::new();
                pb.move_to(tip.x as f32, tip.y as f32);
                pb.line_to((bx + nx) as f32, (by + ny) as f32);
                pb.line_to((bx - nx) as f32, (by - ny) as f32);
                pb.close();
                if let Some(path) = pb.finish() {
                    pm.fill_path(&path, &paint(color), FillRule::Winding, tf, None);
                }
            };
            if arrow && s.arrow_end {
                draw_head(e, ux, uy);
            }
            if arrow && s.arrow_start {
                draw_head(a, -ux, -uy);
            }
        }
        ShapeKind::Polygon | ShapeKind::Polyline if pts.len() >= 2 => {
            let mut pb = PathBuilder::new();
            pb.move_to(pts[0].x as f32, pts[0].y as f32);
            for p in &pts[1..] {
                pb.line_to(p.x as f32, p.y as f32);
            }
            if matches!(s.kind, ShapeKind::Polygon) {
                pb.close();
            }
            if let Some(path) = pb.finish() {
                if let (Some(f), true) = (s.fill, matches!(s.kind, ShapeKind::Polygon)) {
                    pm.fill_path(&path, &paint(f), FillRule::Winding, tf, None);
                }
                if let Some(c) = s.stroke {
                    pm.stroke_path(&path, &paint(c), &stroke, tf, None);
                }
            }
        }
        _ => {}
    }
    Raster::new(pixmap_to_plane(&pm), b.x, b.y)
}

/// tiny-skia pixmaps are premultiplied; planes are straight.
pub fn pixmap_to_plane(pm: &Pixmap) -> Plane {
    let mut raw = pm.data().to_vec();
    for px in raw.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Plane::from_raw(pm.width(), pm.height(), 4, &raw, [0; 4])
}

/// Coverage mask (single channel, document-sized) of a closed polygon or
/// ellipse, for selections.
pub fn polygon_mask(width: u32, height: u32, points: &[Point], anti_alias: bool) -> Plane {
    let mut plane = Plane::mask(width, height, 0);
    if points.len() < 3 {
        return plane;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x as f32, points[0].y as f32);
    for p in &points[1..] {
        pb.line_to(p.x as f32, p.y as f32);
    }
    pb.close();
    if let Some(path) = pb.finish() {
        fill_path_into_mask(&mut plane, &path, anti_alias);
    }
    plane
}

pub fn ellipse_mask(width: u32, height: u32, rect: (f64, f64, f64, f64), anti_alias: bool) -> Plane {
    let mut plane = Plane::mask(width, height, 0);
    let (x, y, w, h) = rect;
    if let Some(r) = tiny_skia::Rect::from_xywh(x as f32, y as f32, w.max(0.5) as f32, h.max(0.5) as f32) {
        let mut pb = PathBuilder::new();
        pb.push_oval(r);
        if let Some(path) = pb.finish() {
            fill_path_into_mask(&mut plane, &path, anti_alias);
        }
    }
    plane
}

fn fill_path_into_mask(plane: &mut Plane, path: &tiny_skia::Path, anti_alias: bool) {
    let b = path.bounds();
    let rect = Rect::cover(b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64)
        .inflate(1)
        .intersect(&plane.bounds());
    if rect.is_empty() {
        return;
    }
    let Some(mut mask) = tiny_skia::Mask::new(rect.w as u32, rect.h as u32) else { return };
    mask.fill_path(path, FillRule::Winding, anti_alias, Transform::from_translate(-rect.x as f32, -rect.y as f32));
    plane.write(rect, mask.data());
    plane.compact();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_is_fully_opaque() {
        let s = ShapeData { kind: ShapeKind::Redact, points: vec![Point::new(10.3, 10.7), Point::new(30.2, 20.9)], ..Default::default() };
        let r = rasterize_shape(&s);
        let mid = r.plane.get((20 - r.x) as i32, (15 - r.y) as i32);
        assert_eq!(mid, [0, 0, 0, 255]);
        for y in 0..r.plane.height() as i32 {
            for x in 0..r.plane.width() as i32 {
                let a = r.plane.get(x, y)[3];
                assert!(a == 0 || a == 255, "no partial alpha at the edge");
            }
        }
    }

    #[test]
    fn arrow_draws_something() {
        let s = ShapeData { kind: ShapeKind::Arrow, points: vec![Point::new(5.0, 5.0), Point::new(80.0, 40.0)], ..Default::default() };
        let r = rasterize_shape(&s);
        assert!(!r.plane.content_bounds().is_empty());
    }

    #[test]
    fn ellipse_mask_covers_centre_not_corner() {
        let m = ellipse_mask(40, 40, (0.0, 0.0, 40.0, 40.0), true);
        assert_eq!(m.get(20, 20)[0], 255);
        assert_eq!(m.get(1, 1)[0], 0);
    }
}
