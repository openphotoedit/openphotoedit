//! Forward geometry (source document → destination document) and how every
//! layer kind follows it: rasters are resampled, shapes have their points
//! moved and are re-rasterised, text keeps its parameters under similarity
//! transforms, gradients move their endpoints.

use editor_core::geom::{Affine, Point, Rect};
use editor_core::layer::{Fill, Layer, LayerKind, Raster, ShapeData, ShapeKind};
use editor_core::shape::rasterize_shape;
use editor_core::transform::Resample;

use crate::sample::{warp_to_raster, Homography, InvMap, Source};

#[derive(Clone, Copy, Debug)]
pub enum Geo {
    Affine(Affine),
    /// Forward homography plus its inverse.
    Proj(Homography, Homography),
}

pub enum Inv {
    Affine(Affine),
    Proj(Homography),
}

impl InvMap for Inv {
    #[inline]
    fn map(&self, x: f64, y: f64) -> Option<([f64; 2], [f64; 4])> {
        match self {
            Inv::Affine(a) => a.map(x, y),
            Inv::Proj(h) => h.map(x, y),
        }
    }
}

impl Geo {
    /// A perspective that sends `bounds`' corners to `quad` (TL, TR, BR, BL).
    pub fn from_quad(bounds: Rect, quad: &[Point; 4]) -> Option<Geo> {
        let fwd = Homography::rect_to_quad(bounds.x as f64, bounds.y as f64, bounds.w as f64, bounds.h as f64, quad)?;
        let cx = bounds.x as f64 + bounds.w as f64 / 2.0;
        let cy = bounds.y as f64 + bounds.h as f64 / 2.0;
        let fwd = fwd.oriented_at(Point::new(cx, cy));
        let centre = fwd.apply(Point::new(cx, cy))?;
        let inv = fwd.invert()?.oriented_at(centre);
        // An affine quad (parallelogram) is cheaper and exact as an affine.
        let m = &fwd.0;
        if m[6].abs() < 1e-12 && m[7].abs() < 1e-12 {
            let s = m[8];
            return Some(Geo::Affine(Affine { a: m[0] / s, b: m[3] / s, c: m[1] / s, d: m[4] / s, e: m[2] / s, f: m[5] / s }));
        }
        Some(Geo::Proj(fwd, inv))
    }

    pub fn fwd(&self, p: Point) -> Option<Point> {
        match self {
            Geo::Affine(a) => Some(a.apply(p)),
            Geo::Proj(h, _) => h.apply(p),
        }
    }

    pub fn inverse(&self) -> Option<Inv> {
        match self {
            Geo::Affine(a) => a.invert().map(Inv::Affine),
            Geo::Proj(_, inv) => Some(Inv::Proj(*inv)),
        }
    }

    pub fn is_identity(&self) -> bool {
        matches!(self, Geo::Affine(a) if *a == Affine::IDENTITY)
    }

    /// Whole-pixel translation, which moves rasters without resampling.
    pub fn integer_translation(&self) -> Option<(i32, i32)> {
        match self {
            Geo::Affine(a) if a.a == 1.0 && a.b == 0.0 && a.c == 0.0 && a.d == 1.0 && a.e.fract() == 0.0 && a.f.fract() == 0.0 => {
                Some((a.e as i32, a.f as i32))
            }
            _ => None,
        }
    }

    /// Uniform scale and rotation (degrees) when the map is a similarity.
    pub fn similarity(&self) -> Option<(f64, f64)> {
        match self {
            Geo::Affine(m) => {
                let s = m.a.hypot(m.b);
                if s < 1e-9 || (m.a - m.d).abs() > 1e-6 * s || (m.b + m.c).abs() > 1e-6 * s {
                    return None;
                }
                Some((s, m.b.atan2(m.a).to_degrees()))
            }
            Geo::Proj(..) => None,
        }
    }

    /// Axis-aligned (scale, flip, translate only).
    pub fn axis_aligned(&self) -> bool {
        matches!(self, Geo::Affine(m) if m.b.abs() < 1e-12 && m.c.abs() < 1e-12)
    }

    /// Mean linear scale near `p`, for stroke widths.
    pub fn linear_scale_at(&self, p: Point) -> f64 {
        match self {
            Geo::Affine(m) => (m.a * m.d - m.b * m.c).abs().sqrt(),
            Geo::Proj(h, _) => {
                let (Some(a), Some(b), Some(c)) = (h.apply(p), h.apply(Point::new(p.x + 1.0, p.y)), h.apply(Point::new(p.x, p.y + 1.0))) else {
                    return 1.0;
                };
                (((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs()).sqrt()
            }
        }
    }

    /// Destination bounds of a source rectangle, clipped to `clip`.
    pub fn dst_bounds(&self, r: Rect, clip: Rect) -> Rect {
        let corners = [(r.x, r.y), (r.right(), r.y), (r.right(), r.bottom()), (r.x, r.bottom())];
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in corners {
            let Some(p) = self.fwd(Point::new(x as f64, y as f64)) else { return clip };
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return clip;
        }
        let x0 = x0.max(clip.x as f64 - 4.0);
        let y0 = y0.max(clip.y as f64 - 4.0);
        let x1 = x1.min(clip.right() as f64 + 4.0);
        let y1 = y1.min(clip.bottom() as f64 + 4.0);
        if x1 <= x0 || y1 <= y0 {
            return Rect::empty();
        }
        Rect::cover(x0, y0, x1 - x0, y1 - y0).inflate(2).intersect(&clip)
    }
}

/// Resample a raster through `geo`. Empty rasters and masks that hold only
/// their fill are returned unchanged.
pub fn warp_raster(r: &Raster, geo: &Geo, method: Resample, clip: Rect) -> Raster {
    if let Some((dx, dy)) = geo.integer_translation() {
        return Raster::new(r.plane.clone(), r.x + dx, r.y + dy);
    }
    let src = Source::from_raster(r);
    if src.is_empty() {
        return r.clone();
    }
    let Some(inv) = geo.inverse() else { return r.clone() };
    let dst = geo.dst_bounds(src.doc_rect(), clip);
    warp_to_raster(&src, &inv, dst, method)
}

/// Points around a rectangle with rounded corners, clockwise from top-left.
fn rect_outline(a: Point, b: Point, radius: f64) -> Vec<Point> {
    let (x0, y0, x1, y1) = (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
    let r = radius.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    if r <= 0.0 {
        return vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)];
    }
    let mut pts = Vec::new();
    let centres = [(x1 - r, y0 + r, -90.0), (x1 - r, y1 - r, 0.0), (x0 + r, y1 - r, 90.0), (x0 + r, y0 + r, 180.0)];
    for (cx, cy, start) in centres {
        for k in 0..=8 {
            let t = (start + 90.0 * k as f64 / 8.0_f64).to_radians();
            pts.push(Point::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    pts
}

fn ellipse_outline(a: Point, b: Point) -> Vec<Point> {
    let (cx, cy) = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let (rx, ry) = ((a.x - b.x).abs() / 2.0, (a.y - b.y).abs() / 2.0);
    let n = ((rx.max(ry) * 0.5) as usize).clamp(32, 256);
    (0..n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            Point::new(cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// Move a shape's geometry. Returns false when the shape cannot stay a
/// vector (a redaction box that would no longer be axis-aligned).
pub fn transform_shape(s: &mut ShapeData, geo: &Geo) -> bool {
    if s.points.is_empty() {
        return true;
    }
    let n = s.points.len() as f64;
    let centroid = Point::new(s.points.iter().map(|p| p.x).sum::<f64>() / n, s.points.iter().map(|p| p.y).sum::<f64>() / n);
    let k = geo.linear_scale_at(centroid);
    let boxy = matches!(s.kind, ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::Redact) && s.points.len() >= 2;
    if boxy && !geo.axis_aligned() {
        match s.kind {
            ShapeKind::Redact => return false,
            ShapeKind::Rect => s.points = rect_outline(s.points[0], s.points[1], s.corner_radius as f64),
            _ => s.points = ellipse_outline(s.points[0], s.points[1]),
        }
        s.kind = ShapeKind::Polygon;
        s.corner_radius = 0.0;
    }
    let mut moved = Vec::with_capacity(s.points.len());
    for p in &s.points {
        match geo.fwd(*p) {
            Some(q) => moved.push(q),
            None => return false,
        }
    }
    s.points = moved;
    s.stroke_width *= k as f32;
    s.corner_radius *= k as f32;
    if let Some(d) = &mut s.dash {
        d[0] *= k as f32;
        d[1] *= k as f32;
    }
    true
}

/// Apply `geo` to one layer and, for groups, everything inside it.
pub fn transform_layer(l: &mut Layer, geo: &Geo, method: Resample, clip: Rect) {
    let mask_follows = l.mask.as_ref().is_none_or(|m| m.linked);
    let kind = std::mem::replace(&mut l.kind, LayerKind::Pixel(Raster::empty(1, 1)));
    l.kind = match kind {
        LayerKind::Pixel(r) => LayerKind::Pixel(warp_raster(&r, geo, method, clip)),
        LayerKind::Text { mut data, raster } => match geo.similarity() {
            Some((s, deg)) => {
                if let Some(p) = geo.fwd(Point::new(data.x as f64, data.y as f64)) {
                    data.x = p.x as f32;
                    data.y = p.y as f32;
                }
                let s = s as f32;
                data.font_size *= s;
                data.letter_spacing *= s;
                data.padding *= s;
                data.stroke_width *= s;
                if let Some(bw) = &mut data.box_width {
                    *bw *= s;
                }
                data.rotation = ((data.rotation as f64 + deg + 180.0).rem_euclid(360.0) - 180.0) as f32;
                LayerKind::Text { data, raster: warp_raster(&raster, geo, method, clip) }
            }
            // Skews and perspective cannot be expressed as text parameters.
            None => LayerKind::Pixel(warp_raster(&raster, geo, method, clip)),
        },
        LayerKind::Shape { mut data, raster } => {
            let keep = data.clone();
            if transform_shape(&mut data, geo) {
                let raster = rasterize_shape(&data);
                LayerKind::Shape { data, raster }
            } else {
                let mut r = warp_raster(&raster, geo, method, clip);
                if keep.kind == ShapeKind::Redact {
                    harden_alpha(&mut r);
                }
                LayerKind::Pixel(r)
            }
        }
        LayerKind::Fill(Fill::Gradient { stops, gradient, from, to, reverse }) => {
            let from = geo.fwd(from).unwrap_or(from);
            let to = geo.fwd(to).unwrap_or(to);
            LayerKind::Fill(Fill::Gradient { stops, gradient, from, to, reverse })
        }
        LayerKind::Group { mut children, pass_through, expanded } => {
            for c in &mut children {
                transform_layer(c, geo, method, clip);
            }
            LayerKind::Group { children, pass_through, expanded }
        }
        // Smart objects stay non-destructive: moving the quad's corners is
        // exact for affine and perspective maps alike, and the editor
        // rebuilds the pixels from the untouched source.
        LayerKind::Smart { source, quad, filters, raster, stale } => match map_quad(&quad, geo) {
            Some(q) => LayerKind::Smart { source, quad: q, filters, raster, stale: true },
            None => LayerKind::Smart { source, quad, filters, raster, stale },
        },
        other => other,
    };
    if mask_follows {
        if let Some(m) = &mut l.mask {
            m.raster = warp_raster(&m.raster, geo, method, clip);
        }
    }
    l.touch();
}

/// A smart object's corners through `geo`; `None` if one falls behind the
/// horizon.
pub fn map_quad(quad: &[Point; 4], geo: &Geo) -> Option<[Point; 4]> {
    Some([geo.fwd(quad[0])?, geo.fwd(quad[1])?, geo.fwd(quad[2])?, geo.fwd(quad[3])?])
}

/// Redaction must stay opaque: no half-covered pixel may show what is under.
fn harden_alpha(r: &mut Raster) {
    let rects = r.plane.present_rects();
    for rect in rects {
        let mut d = r.plane.read_vec(rect);
        for px in d.chunks_exact_mut(4) {
            px[3] = if px[3] >= 128 { 255 } else { 0 };
        }
        r.plane.write(rect, &d);
    }
    r.plane.compact();
}
