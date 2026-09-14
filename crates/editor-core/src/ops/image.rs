//! `image.*` commands: whole-document geometry.

use serde::Deserialize;

use super::{parse, Applied, EditorError, Result};
use crate::document::Document;
use crate::geom::{Affine, Point, Rect};
use crate::layer::{Fill, Layer, LayerKind, Raster};
use crate::plane::Plane;
use crate::render::flatten;
use crate::shape::rasterize_shape;
use crate::transform::{flip, resize_plane, rotate_quarter, warp_affine, Resample};

#[derive(Debug, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Cmd {
    #[serde(rename = "image.crop")]
    Crop { x: i32, y: i32, width: u32, height: u32, #[serde(default = "yes")] delete_cropped: bool },
    #[serde(rename = "image.resize")]
    Resize { width: u32, height: u32, #[serde(default)] resample: Resample },
    #[serde(rename = "image.canvas-size")]
    CanvasSize { width: u32, height: u32, #[serde(default)] anchor: Anchor },
    /// Clockwise quarter turns.
    #[serde(rename = "image.rotate")]
    Rotate { turns: i32 },
    #[serde(rename = "image.flip")]
    Flip { horizontal: bool },
    /// Rotate by any angle about the centre. With `expand`, the canvas grows
    /// to hold the result; otherwise it keeps its size (straighten).
    #[serde(rename = "image.rotate-arbitrary")]
    RotateArbitrary { degrees: f64, #[serde(default)] expand: bool },
    /// Crop away transparent borders.
    #[serde(rename = "image.trim")]
    Trim,
    /// Grow the canvas to show every pixel of every layer.
    #[serde(rename = "image.reveal-all")]
    RevealAll,
}

fn yes() -> bool {
    true
}

pub fn apply(v: serde_json::Value, doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let cmd: Cmd = parse(v)?;
    match cmd {
        Cmd::Crop { x, y, width, height, delete_cropped } => {
            if width == 0 || height == 0 {
                return Err(EditorError::Invalid("crop must be at least 1×1".into()));
            }
            crop(doc, Rect::new(x, y, width as i32, height as i32), delete_cropped);
            Ok(Applied::step("Crop"))
        }
        Cmd::Resize { width, height, resample } => {
            if width == 0 || height == 0 || width > 100_000 || height > 100_000 {
                return Err(EditorError::Invalid("size must be 1..100000".into()));
            }
            resize(doc, width, height, resample);
            Ok(Applied::step("Image Size"))
        }
        Cmd::CanvasSize { width, height, anchor } => {
            if width == 0 || height == 0 {
                return Err(EditorError::Invalid("canvas must be at least 1×1".into()));
            }
            let (dw, dh) = (width as i32 - doc.width as i32, height as i32 - doc.height as i32);
            let (fx, fy) = match anchor {
                Anchor::TopLeft => (0, 0),
                Anchor::Top => (1, 0),
                Anchor::TopRight => (2, 0),
                Anchor::Left => (0, 1),
                Anchor::Center => (1, 1),
                Anchor::Right => (2, 1),
                Anchor::BottomLeft => (0, 2),
                Anchor::Bottom => (1, 2),
                Anchor::BottomRight => (2, 2),
            };
            let (ox, oy) = (dw * fx / 2, dh * fy / 2);
            shift_all(doc, ox, oy);
            doc.width = width;
            doc.height = height;
            Ok(Applied::step("Canvas Size"))
        }
        Cmd::Rotate { turns } => {
            let t = turns.rem_euclid(4);
            if t == 0 {
                return Ok(Applied::quiet());
            }
            rotate(doc, t);
            Ok(Applied::step(match t {
                1 => "Rotate 90° Clockwise",
                2 => "Rotate 180°",
                _ => "Rotate 90° Counter Clockwise",
            }))
        }
        Cmd::Flip { horizontal } => {
            flip_doc(doc, horizontal);
            Ok(Applied::step(if horizontal { "Flip Canvas Horizontal" } else { "Flip Canvas Vertical" }))
        }
        Cmd::RotateArbitrary { degrees, expand } => {
            if degrees.abs() < 1e-6 {
                return Ok(Applied::quiet());
            }
            rotate_arbitrary(doc, degrees, expand);
            Ok(Applied::step(if expand { "Rotate Canvas" } else { "Straighten" }))
        }
        Cmd::Trim => {
            let out = flatten(doc);
            let (w, h) = (doc.width as i32, doc.height as i32);
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1, -1);
            for y in 0..h {
                for x in 0..w {
                    if out[((y * w + x) * 4 + 3) as usize] != 0 {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            if x1 < 0 {
                return Err(EditorError::Invalid("the image is fully transparent".into()));
            }
            crop(doc, Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1), true);
            Ok(Applied::step("Trim"))
        }
        Cmd::RevealAll => {
            let mut b = Rect::new(0, 0, doc.width as i32, doc.height as i32);
            doc.walk(&mut |l| {
                if let Some(r) = l.content_bounds() {
                    b = b.union(&r);
                }
            });
            shift_all(doc, -b.x, -b.y);
            doc.width = b.w as u32;
            doc.height = b.h as u32;
            Ok(Applied::step("Reveal All"))
        }
    }
}

fn for_each_layer(layers: &mut [Layer], f: &mut impl FnMut(&mut Layer)) {
    for l in layers {
        f(l);
        l.touch();
        if let LayerKind::Group { children, .. } = &mut l.kind {
            for_each_layer(children, f);
        }
    }
}

fn shift_all(doc: &mut Document, dx: i32, dy: i32) {
    let (w, h) = (doc.width, doc.height);
    for_each_layer(&mut doc.layers, &mut |l| {
        if !l.is_group() {
            super::layer::offset_layer(l, dx, dy);
        }
        // Group masks move too (group children moved individually above).
        if l.is_group() {
            if let Some(m) = &mut l.mask {
                m.raster.x += dx;
                m.raster.y += dy;
            }
        }
    });
    if let Some(sel) = doc.selection.take() {
        let mut ns = Plane::mask(w.max(1), h.max(1), sel.fill()[0]);
        ns.paste(&sel, dx, dy);
        doc.selection = Some(ns);
    }
}

pub fn crop(doc: &mut Document, rect: Rect, delete: bool) {
    shift_all(doc, -rect.x, -rect.y);
    doc.width = rect.w as u32;
    doc.height = rect.h as u32;
    let canvas = Rect::new(0, 0, rect.w, rect.h);
    if delete {
        for_each_layer(&mut doc.layers, &mut |l| {
            if let Some(r) = l.raster_mut() {
                trim_raster(r, canvas);
            }
            if let Some(m) = &mut l.mask {
                trim_raster(&mut m.raster, canvas);
            }
        });
    }
    // The selection plane must match the new canvas size.
    if let Some(sel) = doc.selection.take() {
        let mut ns = Plane::mask(rect.w as u32, rect.h as u32, sel.fill()[0]);
        ns.paste(&sel, 0, 0);
        doc.selection = Some(ns);
    }
}

fn trim_raster(r: &mut Raster, canvas: Rect) {
    let keep = r.doc_rect().intersect(&canvas);
    if keep.is_empty() {
        r.plane = Plane::new(1, 1, r.plane.channels(), r.plane.fill());
        r.x = 0;
        r.y = 0;
        return;
    }
    if keep == r.doc_rect() {
        return;
    }
    r.plane = r.plane.extract(keep.translate(-r.x, -r.y));
    r.x = keep.x;
    r.y = keep.y;
}

fn resize(doc: &mut Document, width: u32, height: u32, method: Resample) {
    let sx = width as f64 / doc.width as f64;
    let sy = height as f64 / doc.height as f64;
    let scale_raster = |r: &mut Raster| {
        let nw = ((r.plane.width() as f64 * sx).round() as u32).max(1);
        let nh = ((r.plane.height() as f64 * sy).round() as u32).max(1);
        r.plane = resize_plane(&r.plane, nw, nh, method);
        r.x = (r.x as f64 * sx).round() as i32;
        r.y = (r.y as f64 * sy).round() as i32;
    };
    for_each_layer(&mut doc.layers, &mut |l| {
        match &mut l.kind {
            LayerKind::Pixel(r) => scale_raster(r),
            LayerKind::Text { data, raster } => {
                data.x *= sx as f32;
                data.y *= sy as f32;
                data.font_size *= sy as f32;
                if let Some(bw) = &mut data.box_width {
                    *bw *= sx as f32;
                }
                scale_raster(raster);
            }
            LayerKind::Shape { data, raster } => {
                for p in &mut data.points {
                    p.x *= sx;
                    p.y *= sy;
                }
                data.stroke_width *= ((sx + sy) / 2.0) as f32;
                *raster = rasterize_shape(data);
            }
            LayerKind::Fill(Fill::Gradient { from, to, .. }) => {
                from.x *= sx;
                from.y *= sy;
                to.x *= sx;
                to.y *= sy;
            }
            _ => {}
        }
        if let Some(m) = &mut l.mask {
            scale_raster(&mut m.raster);
        }
    });
    if let Some(sel) = doc.selection.take() {
        doc.selection = Some(resize_plane(&sel, width, height, Resample::Bilinear));
    }
    doc.width = width;
    doc.height = height;
}

fn rotate(doc: &mut Document, t: i32) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    let map_rect = |x: i32, y: i32, rw: i32, rh: i32| -> (i32, i32) {
        match t {
            1 => (h - (y + rh), x),
            2 => (w - (x + rw), h - (y + rh)),
            _ => (y, w - (x + rw)),
        }
    };
    let map_pt = |p: &mut Point| {
        let (x, y) = (p.x, p.y);
        let (nx, ny) = match t {
            1 => (h as f64 - y, x),
            2 => (w as f64 - x, h as f64 - y),
            _ => (y, w as f64 - x),
        };
        p.x = nx;
        p.y = ny;
    };
    let rot = |r: &mut Raster| {
        let (nx, ny) = map_rect(r.x, r.y, r.plane.width() as i32, r.plane.height() as i32);
        r.plane = rotate_quarter(&r.plane, t);
        r.x = nx;
        r.y = ny;
    };
    for_each_layer(&mut doc.layers, &mut |l| {
        match &mut l.kind {
            LayerKind::Pixel(r) => rot(r),
            LayerKind::Text { raster, data } => {
                rot(raster);
                data.x = raster.x as f32;
                data.y = raster.y as f32;
                data.rotation += 90.0 * t as f32;
            }
            LayerKind::Shape { data, raster } => {
                data.points.iter_mut().for_each(map_pt);
                *raster = rasterize_shape(data);
            }
            LayerKind::Fill(Fill::Gradient { from, to, .. }) => {
                map_pt(from);
                map_pt(to);
            }
            _ => {}
        }
        if let Some(m) = &mut l.mask {
            rot(&mut m.raster);
        }
    });
    if let Some(sel) = doc.selection.take() {
        doc.selection = Some(rotate_quarter(&sel, t));
    }
    if t % 2 == 1 {
        std::mem::swap(&mut doc.width, &mut doc.height);
    }
}

fn flip_doc(doc: &mut Document, horizontal: bool) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    let fl = |r: &mut Raster| {
        if horizontal {
            r.x = w - (r.x + r.plane.width() as i32);
        } else {
            r.y = h - (r.y + r.plane.height() as i32);
        }
        r.plane = flip(&r.plane, horizontal);
    };
    let fp = |p: &mut Point| {
        if horizontal {
            p.x = w as f64 - p.x;
        } else {
            p.y = h as f64 - p.y;
        }
    };
    for_each_layer(&mut doc.layers, &mut |l| {
        match &mut l.kind {
            LayerKind::Pixel(r) => fl(r),
            LayerKind::Text { raster, data } => {
                fl(raster);
                data.x = raster.x as f32;
                data.y = raster.y as f32;
            }
            LayerKind::Shape { data, raster } => {
                data.points.iter_mut().for_each(fp);
                *raster = rasterize_shape(data);
            }
            LayerKind::Fill(Fill::Gradient { from, to, .. }) => {
                fp(from);
                fp(to);
            }
            _ => {}
        }
        if let Some(m) = &mut l.mask {
            fl(&mut m.raster);
        }
    });
    if let Some(sel) = doc.selection.take() {
        doc.selection = Some(flip(&sel, horizontal));
    }
}

fn rotate_arbitrary(doc: &mut Document, degrees: f64, expand: bool) {
    let (w, h) = (doc.width as f64, doc.height as f64);
    let m = Affine::translate(-w / 2.0, -h / 2.0)
        .then(&Affine::rotate(degrees.to_radians()))
        .then(&Affine::translate(w / 2.0, h / 2.0));
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let new_bounds = m.bounds(&canvas);
    let warp = |r: &mut Raster| {
        let (p, x, y) = warp_affine(&r.plane, (r.x, r.y), &m);
        r.plane = p;
        r.x = x;
        r.y = y;
    };
    for_each_layer(&mut doc.layers, &mut |l| {
        match &mut l.kind {
            LayerKind::Pixel(r) => warp(r),
            LayerKind::Text { raster, data } => {
                warp(raster);
                data.rotation += degrees as f32;
            }
            LayerKind::Shape { data, raster } => {
                for p in &mut data.points {
                    *p = m.apply(*p);
                }
                *raster = rasterize_shape(data);
            }
            LayerKind::Fill(Fill::Gradient { from, to, .. }) => {
                *from = m.apply(*from);
                *to = m.apply(*to);
            }
            _ => {}
        }
        if let Some(mask) = &mut l.mask {
            // Masks whose fill is "reveal" keep revealing outside their plane.
            warp(&mut mask.raster);
        }
    });
    if let Some(sel) = doc.selection.take() {
        let (p, x, y) = warp_affine(&sel, (0, 0), &m);
        let mut ns = Plane::mask(doc.width, doc.height, 0);
        ns.paste(&p, x, y);
        doc.selection = Some(ns);
    }
    if expand {
        shift_all(doc, -new_bounds.x, -new_bounds.y);
        doc.width = new_bounds.w as u32;
        doc.height = new_bounds.h as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::flatten;
    use serde_json::json;

    fn doc_with(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Document {
        let mut doc = Document::new(1, 1);
        let px: Vec<u8> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| f(x, y)).collect();
        super::super::layer::apply(json!({"op": "doc.open-pixels", "width": w, "height": h}), &mut doc, &px).unwrap();
        doc
    }

    #[test]
    fn crop_then_flatten() {
        let mut doc = doc_with(10, 10, |x, y| [x as u8, y as u8, 0, 255]);
        apply(json!({"op": "image.crop", "x": 2, "y": 3, "width": 4, "height": 5}), &mut doc, &[]).unwrap();
        assert_eq!((doc.width, doc.height), (4, 5));
        let out = flatten(&doc);
        assert_eq!(&out[0..2], &[2, 3]);
    }

    #[test]
    fn rotate_matches_pixel_positions() {
        let mut doc = doc_with(3, 2, |x, y| [(x + 10 * y) as u8, 0, 0, 255]);
        apply(json!({"op": "image.rotate", "turns": 1}), &mut doc, &[]).unwrap();
        assert_eq!((doc.width, doc.height), (2, 3));
        let out = flatten(&doc);
        // Clockwise: the original bottom-left (0,1)=10 lands top-left.
        assert_eq!(out[0], 10);
        assert_eq!(out[4], 0);
    }

    #[test]
    fn resize_halves() {
        let mut doc = doc_with(8, 8, |_, _| [50, 60, 70, 255]);
        apply(json!({"op": "image.resize", "width": 4, "height": 4}), &mut doc, &[]).unwrap();
        let out = flatten(&doc);
        assert_eq!(out.len(), 4 * 4 * 4);
        assert!((out[0] as i32 - 50).abs() <= 1);
    }

    #[test]
    fn canvas_size_centres() {
        let mut doc = doc_with(2, 2, |_, _| [255, 0, 0, 255]);
        apply(json!({"op": "image.canvas-size", "width": 4, "height": 4}), &mut doc, &[]).unwrap();
        let out = flatten(&doc);
        assert_eq!(out[3], 0);
        assert_eq!(out[(1 * 4 + 1) * 4 + 3], 255);
    }

    #[test]
    fn flip_horizontal() {
        let mut doc = doc_with(3, 1, |x, _| [x as u8, 0, 0, 255]);
        apply(json!({"op": "image.flip", "horizontal": true}), &mut doc, &[]).unwrap();
        assert_eq!(flatten(&doc)[0], 2);
    }

    #[test]
    fn straighten_keeps_canvas() {
        let mut doc = doc_with(40, 30, |_, _| [9, 9, 9, 255]);
        apply(json!({"op": "image.rotate-arbitrary", "degrees": 5.0}), &mut doc, &[]).unwrap();
        assert_eq!((doc.width, doc.height), (40, 30));
        let out = flatten(&doc);
        assert_eq!(out[(15 * 40 + 20) * 4], 9);
    }
}
