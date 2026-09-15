//! `transform.*` commands. Registered with the editor by `register`.
//!
//! - `transform.layer` — free transform / perspective of layers
//! - `transform.selection-pixels` — lift, transform and drop selected pixels
//! - `transform.warp` — 4×4 Bézier patch warp
//! - `transform.liquify` (+ `transform.liquify-end`) — displacement brushes
//! - `transform.perspective-crop`, `transform.lens-correct`,
//!   `transform.content-aware-scale` — whole-document geometry

// `as_chunks` postdates the workspace's minimum Rust (1.85).
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod carve;
pub mod geo;
pub mod lens;
pub mod liquify;
pub mod mesh;
pub mod sample;

use editor_core::document::Document;
use editor_core::editor::Editor;
use editor_core::geom::{Affine, Point, Rect};
use editor_core::layer::{Fill, Layer, LayerId, LayerKind, Raster};
use editor_core::ops::{parse, target_id, Applied, EditorError, Result};
use editor_core::plane::Plane;
use editor_core::transform::Resample;
use serde::Deserialize;
use serde_json::Value;

use geo::{transform_layer, warp_raster, Geo};
use sample::{warp_to_raster, Homography, Source};

pub fn register(ed: &mut Editor) {
    ed.register_domain("transform", apply);
}

fn fifty() -> f64 {
    50.0
}
fn hundred() -> f64 {
    100.0
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Cmd {
    #[serde(rename = "transform.layer")]
    Layer {
        ids: Vec<LayerId>,
        #[serde(default)]
        matrix: Option<Affine>,
        #[serde(default)]
        quad: Option<[Point; 4]>,
        #[serde(default)]
        resample: Resample,
    },
    #[serde(rename = "transform.selection-pixels")]
    SelectionPixels {
        #[serde(default)]
        id: Option<LayerId>,
        #[serde(default)]
        matrix: Option<Affine>,
        #[serde(default)]
        quad: Option<[Point; 4]>,
        #[serde(default)]
        resample: Resample,
    },
    #[serde(rename = "transform.warp")]
    Warp {
        #[serde(default)]
        id: Option<LayerId>,
        grid: Vec<Point>,
        #[serde(default)]
        resample: Resample,
    },
    #[serde(rename = "transform.liquify")]
    Liquify {
        #[serde(default)]
        id: Option<LayerId>,
        tool: liquify::Tool,
        size: f64,
        #[serde(default = "hundred")]
        pressure: f64,
        #[serde(default = "fifty")]
        density: f64,
        points: Vec<liquify::StrokePoint>,
        #[serde(default)]
        stroke_id: Value,
    },
    /// Not recorded: drops cached liquify state for the layer.
    #[serde(rename = "transform.liquify-end")]
    LiquifyEnd {
        #[serde(default)]
        id: Option<LayerId>,
    },
    #[serde(rename = "transform.perspective-crop")]
    PerspectiveCrop {
        quad: [Point; 4],
        width: u32,
        height: u32,
        #[serde(default)]
        resample: Resample,
    },
    #[serde(rename = "transform.lens-correct")]
    LensCorrect {
        #[serde(default)]
        id: Option<LayerId>,
        #[serde(default)]
        distortion: f64,
        #[serde(default)]
        chromatic_rc: f64,
        #[serde(default)]
        chromatic_by: f64,
        #[serde(default)]
        vignette: f64,
        #[serde(default = "fifty")]
        vignette_midpoint: f64,
        #[serde(default = "hundred")]
        scale: f64,
        #[serde(default)]
        auto_scale: bool,
    },
    #[serde(rename = "transform.content-aware-scale")]
    ContentAwareScale {
        width: u32,
        height: u32,
        #[serde(default)]
        protect_skin: bool,
    },
}

const OPS: &[&str] = &[
    "transform.layer",
    "transform.selection-pixels",
    "transform.warp",
    "transform.liquify",
    "transform.liquify-end",
    "transform.perspective-crop",
    "transform.lens-correct",
    "transform.content-aware-scale",
];

pub fn apply(v: Value, doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
    if !OPS.contains(&op.as_str()) {
        return Err(EditorError::UnknownOp(op));
    }
    match parse::<Cmd>(v)? {
        Cmd::Layer { ids, matrix, quad, resample } => {
            if ids.is_empty() {
                return Err(EditorError::Invalid("no layers to transform".into()));
            }
            let geo = geometry(doc, &ids, matrix, quad)?;
            if geo.is_identity() {
                return Ok(Applied::quiet());
            }
            transform_layers(doc, &ids, &geo, resample)?;
            Ok(Applied::step("Free Transform"))
        }
        Cmd::SelectionPixels { id, matrix, quad, resample } => {
            let id = target_id(doc, id)?;
            if doc.selection.is_none() {
                let geo = geometry(doc, &[id], matrix, quad)?;
                if geo.is_identity() {
                    return Ok(Applied::quiet());
                }
                transform_layers(doc, &[id], &geo, resample)?;
                return Ok(Applied::step("Free Transform"));
            }
            transform_selection_pixels(doc, id, matrix, quad, resample)?;
            Ok(Applied::step("Free Transform"))
        }
        Cmd::Warp { id, grid, resample } => {
            let id = target_id(doc, id)?;
            warp(doc, id, &grid, resample)?;
            Ok(Applied::step("Warp"))
        }
        Cmd::Liquify { id, tool, size, pressure, density, points, stroke_id } => {
            let id = target_id(doc, id)?;
            let stroke_id = match stroke_id {
                Value::String(s) => s,
                Value::Null => String::new(),
                other => other.to_string(),
            };
            let key = format!("liquify:{stroke_id}");
            let dirty = liquify::apply(doc, id, liquify::Params { tool, size, pressure, density, points, stroke_id })?;
            Ok(Applied::step("Liquify").merge(key).dirty(dirty))
        }
        Cmd::LiquifyEnd { id } => {
            liquify::release(id);
            Ok(Applied::quiet())
        }
        Cmd::PerspectiveCrop { quad, width, height, resample } => {
            if width == 0 || height == 0 || width > 100_000 || height > 100_000 {
                return Err(EditorError::Invalid("crop size must be 1..100000".into()));
            }
            perspective_crop(doc, &quad, width, height, resample)?;
            Ok(Applied::step("Perspective Crop"))
        }
        Cmd::LensCorrect { id, distortion, chromatic_rc, chromatic_by, vignette, vignette_midpoint, scale, auto_scale } => {
            let id = target_id(doc, id)?;
            let p = lens::LensParams { distortion, chromatic_rc, chromatic_by, vignette, vignette_midpoint, scale, auto_scale };
            let dirty = lens::apply(doc, id, p)?;
            if dirty.is_empty() {
                return Ok(Applied::quiet());
            }
            Ok(Applied::step("Lens Correction").dirty(dirty))
        }
        Cmd::ContentAwareScale { width, height, protect_skin } => {
            if width == 0 || height == 0 {
                return Err(EditorError::Invalid("size must be at least 1×1".into()));
            }
            if width > doc.width.saturating_mul(4) || height > doc.height.saturating_mul(4) {
                return Err(EditorError::Invalid("content-aware scale can grow an image at most 4×".into()));
            }
            if width == doc.width && height == doc.height {
                return Ok(Applied::quiet());
            }
            content_aware_scale(doc, width, height, protect_skin);
            Ok(Applied::step("Content-Aware Scale"))
        }
    }
}

fn clip_rect(doc: &Document) -> Rect {
    let m = (doc.width.max(doc.height) as i32).saturating_mul(2);
    Rect::new(0, 0, doc.width as i32, doc.height as i32).inflate(m)
}

/// The union of the layers' content bounds (masks included for layers
/// without pixels).
fn union_bounds(doc: &Document, ids: &[LayerId]) -> Result<Rect> {
    let mut b = Rect::empty();
    for &id in ids {
        let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
        let mut lb = l.content_bounds().unwrap_or_else(Rect::empty);
        if lb.is_empty() {
            if let Some(m) = &l.mask {
                lb = m.raster.plane.content_bounds().translate(m.raster.x, m.raster.y);
            }
        }
        b = b.union(&lb);
    }
    Ok(b)
}

fn geometry(doc: &Document, ids: &[LayerId], matrix: Option<Affine>, quad: Option<[Point; 4]>) -> Result<Geo> {
    match (quad, matrix) {
        (Some(q), _) => {
            let b = union_bounds(doc, ids)?;
            if b.is_empty() {
                return Err(EditorError::Invalid("the layer is empty".into()));
            }
            Geo::from_quad(b, &q).ok_or_else(|| EditorError::Invalid("the corners do not form a usable quadrilateral".into()))
        }
        (None, Some(m)) => {
            let det = m.a * m.d - m.b * m.c;
            if det.abs() < 1e-9 || ![m.a, m.b, m.c, m.d, m.e, m.f].iter().all(|v| v.is_finite()) {
                return Err(EditorError::Invalid("the transform collapses the layer".into()));
            }
            Ok(Geo::Affine(m))
        }
        (None, None) => Err(EditorError::Invalid("give a matrix or a quad".into())),
    }
}

fn check_movable(l: &Layer) -> Result<()> {
    if l.locks.all || l.locks.position || l.locks.pixels {
        return Err(EditorError::Locked(l.name.clone()));
    }
    if let Some(children) = l.children() {
        for c in children {
            if c.locks.all || c.locks.position {
                return Err(EditorError::Locked(c.name.clone()));
            }
        }
    }
    Ok(())
}

fn transform_layers(doc: &mut Document, ids: &[LayerId], geo: &Geo, method: Resample) -> Result<()> {
    for &id in ids {
        check_movable(doc.find(id).ok_or(EditorError::NoLayer(id))?)?;
    }
    // A group and one of its own children would move twice.
    let ids: Vec<LayerId> = ids.iter().copied().filter(|&id| !ids.iter().any(|&other| other != id && doc.is_ancestor(other, id))).collect();
    let clip = clip_rect(doc);
    for id in ids {
        let l = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
        transform_layer(l, geo, method, clip);
    }
    Ok(())
}

fn transform_selection_pixels(doc: &mut Document, id: LayerId, matrix: Option<Affine>, quad: Option<[Point; 4]>, method: Resample) -> Result<()> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    check_movable(l)?;
    let raster = match &l.kind {
        LayerKind::Pixel(r) => r.clone(),
        LayerKind::Text { .. } | LayerKind::Shape { .. } | LayerKind::Smart { .. } => {
            return Err(EditorError::Invalid(format!("`{}` is a {} layer; rasterize it first", l.name, l.kind.name())))
        }
        _ => return Err(EditorError::Invalid(format!("`{}` has no pixels to transform", l.name))),
    };
    let sel = doc.selection.clone().expect("checked by caller");
    let sel_bounds = editor_core::selection::bounds(doc);
    if sel_bounds.is_empty() {
        return Err(EditorError::Invalid("the selection is empty".into()));
    }
    let geo = match (quad, matrix) {
        (Some(q), _) => Geo::from_quad(sel_bounds, &q).ok_or_else(|| EditorError::Invalid("the corners do not form a usable quadrilateral".into()))?,
        (None, Some(m)) => geometry(doc, &[id], Some(m), None)?,
        (None, None) => return Err(EditorError::Invalid("give a matrix or a quad".into())),
    };
    if geo.is_identity() {
        return Ok(());
    }
    // Lift: floating pixels carry the selection's coverage in their alpha.
    let area = sel_bounds.intersect(&raster.doc_rect());
    let clip = clip_rect(doc);
    let (canvas_w, canvas_h) = (doc.width, doc.height);
    let mut floating = Vec::new();
    let mut remaining = Vec::new();
    if !area.is_empty() {
        let px = raster.plane.read_vec(area.translate(-raster.x, -raster.y));
        let cov = sel.read_vec(area);
        floating = px.clone();
        remaining = px;
        for (k, c) in cov.iter().enumerate() {
            let a = floating[k * 4 + 3] as u32;
            floating[k * 4 + 3] = ((a * *c as u32 + 127) / 255) as u8;
            remaining[k * 4 + 3] = ((a * (255 - *c as u32) + 127) / 255) as u8;
        }
    }
    let moved = if area.is_empty() {
        None
    } else {
        let src = Source::from_raw(floating, area.w, area.h, 4, 0, area.x as f64, area.y as f64);
        let inv = geo.inverse().ok_or_else(|| EditorError::Invalid("the transform collapses the selection".into()))?;
        Some(warp_to_raster(&src, &inv, geo.dst_bounds(area, clip), method))
    };
    // The selection follows.
    let sel_src = Source::from_plane_rect(&sel, sel_bounds, 0, 0);
    let new_sel = match geo.inverse() {
        Some(inv) => {
            let r = warp_to_raster(&sel_src, &inv, geo.dst_bounds(sel_bounds, Rect::new(0, 0, canvas_w as i32, canvas_h as i32)), method);
            let mut p = Plane::mask(canvas_w, canvas_h, 0);
            p.paste(&r.plane, r.x, r.y);
            p.compact();
            p
        }
        None => sel.clone(),
    };

    let l = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
    let LayerKind::Pixel(r) = &mut l.kind else { unreachable!() };
    if !area.is_empty() {
        r.plane.write(area.translate(-r.x, -r.y), &remaining);
    }
    if let Some(m) = moved {
        let b = m.plane.content_bounds().translate(m.x, m.y);
        if !b.is_empty() {
            r.ensure_covers(b);
            let top = m.plane.read_vec(b.translate(-m.x, -m.y));
            let local = b.translate(-r.x, -r.y);
            let mut base = r.plane.read_vec(local);
            for (d, s) in base.chunks_exact_mut(4).zip(top.chunks_exact(4)) {
                if s[3] == 0 {
                    continue;
                }
                let o = mesh::over([s[0], s[1], s[2], s[3]], [d[0], d[1], d[2], d[3]]);
                d.copy_from_slice(&o);
            }
            r.plane.write(local, &base);
        }
    }
    r.plane.compact();
    doc.selection = Some(new_sel);
    Ok(())
}

fn warp(doc: &mut Document, id: LayerId, grid: &[Point], method: Resample) -> Result<()> {
    if grid.len() != 16 {
        return Err(EditorError::Invalid(format!("a warp grid has 16 points, got {}", grid.len())));
    }
    if grid.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(EditorError::Invalid("the warp grid has an invalid point".into()));
    }
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    check_movable(l)?;
    let bounds = union_bounds(doc, &[id])?;
    if bounds.is_empty() {
        return Err(EditorError::Invalid("the layer is empty".into()));
    }
    let mesh = mesh::Mesh::new(bounds, grid);
    let clip = clip_rect(doc);
    let l = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
    warp_layer(l, &mesh, grid, bounds, method, clip);
    Ok(())
}

fn warp_layer(l: &mut Layer, mesh: &mesh::Mesh, grid: &[Point], bounds: Rect, method: Resample, clip: Rect) {
    let mask_follows = l.mask.as_ref().is_none_or(|m| m.linked);
    let kind = std::mem::replace(&mut l.kind, LayerKind::Pixel(Raster::empty(1, 1)));
    l.kind = match kind {
        // A mesh warp has no vector or smart-object form: the result is pixels.
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } | LayerKind::Smart { raster: r, .. } => LayerKind::Pixel(mesh.warp(&r, method, clip)),
        LayerKind::Group { mut children, pass_through, expanded } => {
            for c in &mut children {
                warp_layer(c, mesh, grid, bounds, method, clip);
            }
            LayerKind::Group { children, pass_through, expanded }
        }
        LayerKind::Fill(Fill::Gradient { stops, gradient, from, to, reverse }) => {
            let map = |p: Point| {
                let (u, v) = ((p.x - bounds.x as f64) / bounds.w as f64, (p.y - bounds.y as f64) / bounds.h as f64);
                mesh::eval_patch(grid, u, v)
            };
            LayerKind::Fill(Fill::Gradient { stops, gradient, from: map(from), to: map(to), reverse })
        }
        other => other,
    };
    if mask_follows {
        if let Some(m) = &mut l.mask {
            m.raster = mesh.warp(&m.raster, method, clip);
        }
    }
    l.touch();
}

fn perspective_crop(doc: &mut Document, quad: &[Point; 4], width: u32, height: u32, method: Resample) -> Result<()> {
    // Output rectangle → quad in the current document: exactly the map
    // each output pixel pulls through.
    let out_rect = Rect::new(0, 0, width as i32, height as i32);
    let inv = Homography::rect_to_quad(0.0, 0.0, width as f64, height as f64, quad)
        .ok_or_else(|| EditorError::Invalid("the corners do not form a usable quadrilateral".into()))?
        .oriented_at(Point::new(width as f64 / 2.0, height as f64 / 2.0));
    let centre = inv.apply(Point::new(width as f64 / 2.0, height as f64 / 2.0)).ok_or_else(|| EditorError::Invalid("bad quad".into()))?;
    let fwd = inv.invert().ok_or_else(|| EditorError::Invalid("bad quad".into()))?.oriented_at(centre);
    let geo = Geo::Proj(fwd, inv);
    fn go(layers: &mut [Layer], geo: &Geo, out: Rect, method: Resample) {
        for l in layers {
            let kind = std::mem::replace(&mut l.kind, LayerKind::Pixel(Raster::empty(1, 1)));
            l.kind = match kind {
                LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } => LayerKind::Pixel(warp_raster(&r, geo, method, out)),
                // Corners map exactly; the editor rebuilds from the source.
                LayerKind::Smart { source, quad, filters, raster, .. } => match geo::map_quad(&quad, geo) {
                    Some(q) => LayerKind::Smart { source, quad: q, filters, raster, stale: true },
                    None => LayerKind::Pixel(warp_raster(&raster, geo, method, out)),
                },
                LayerKind::Group { mut children, pass_through, expanded } => {
                    go(&mut children, geo, out, method);
                    LayerKind::Group { children, pass_through, expanded }
                }
                LayerKind::Fill(Fill::Gradient { stops, gradient, from, to, reverse }) => {
                    LayerKind::Fill(Fill::Gradient { stops, gradient, from: geo.fwd(from).unwrap_or(from), to: geo.fwd(to).unwrap_or(to), reverse })
                }
                other => other,
            };
            if let Some(m) = &mut l.mask {
                m.raster = warp_raster(&m.raster, geo, method, out);
            }
            l.touch();
        }
    }
    go(&mut doc.layers, &geo, out_rect, method);
    doc.width = width;
    doc.height = height;
    doc.selection = None;
    Ok(())
}

fn content_aware_scale(doc: &mut Document, width: u32, height: u32, protect_skin: bool) {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let canvas = Rect::new(0, 0, w as i32, h as i32);
    let merged = editor_core::pixels::read_merged(doc, canvas);
    let stages = carve::plan(&merged, w, h, width as usize, height as usize, protect_skin);
    let run = |buf: Vec<u8>, ch: usize| -> (Vec<u8>, usize, usize) {
        let (mut b, mut cw, mut chh) = (buf, w, h);
        for s in &stages {
            (b, cw, chh) = s.apply(&b, cw, chh, ch);
        }
        (b, cw, chh)
    };
    let (sx, sy) = (width as f64 / w as f64, height as f64 / h as f64);
    fn go(layers: &mut [Layer], canvas: Rect, run: &dyn Fn(Vec<u8>, usize) -> (Vec<u8>, usize, usize), s: (f64, f64)) {
        for l in layers {
            let kind = std::mem::replace(&mut l.kind, LayerKind::Pixel(Raster::empty(1, 1)));
            let carve = |r: &Raster| -> Raster {
                let buf = r.plane.read_vec(canvas.translate(-r.x, -r.y));
                let (b, nw, nh) = run(buf, r.plane.channels());
                let mut p = Plane::from_raw(nw as u32, nh as u32, r.plane.channels(), &b, r.plane.fill());
                p.compact();
                Raster::new(p, 0, 0)
            };
            l.kind = match kind {
                LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } | LayerKind::Smart { raster: r, .. } => LayerKind::Pixel(carve(&r)),
                LayerKind::Group { mut children, pass_through, expanded } => {
                    go(&mut children, canvas, run, s);
                    LayerKind::Group { children, pass_through, expanded }
                }
                LayerKind::Fill(Fill::Gradient { stops, gradient, from, to, reverse }) => {
                    let sc = |p: Point| Point::new(p.x * s.0, p.y * s.1);
                    LayerKind::Fill(Fill::Gradient { stops, gradient, from: sc(from), to: sc(to), reverse })
                }
                other => other,
            };
            if let Some(m) = &mut l.mask {
                m.raster = carve(&m.raster);
            }
            l.touch();
        }
    }
    go(&mut doc.layers, canvas, &run, (sx, sy));
    doc.width = width;
    doc.height = height;
    doc.selection = None;
}

#[cfg(test)]
mod tests;
