//! `select.*` commands.

use serde::Deserialize;

use super::{layer, parse, Applied, EditorError, Result};
use crate::document::Document;
use crate::geom::{Point, Rect};
use crate::layer::LayerId;
use crate::plane::Plane;
use crate::selection::{self, SelectMode};
use crate::shape::{ellipse_mask, polygon_mask};

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Cmd {
    #[serde(rename = "select.all")]
    All,
    #[serde(rename = "select.none")]
    None,
    #[serde(rename = "select.invert")]
    Invert,
    #[serde(rename = "select.rect")]
    Rect { x: f64, y: f64, width: f64, height: f64, #[serde(default)] mode: SelectMode, #[serde(default)] feather: f32 },
    #[serde(rename = "select.ellipse")]
    Ellipse { x: f64, y: f64, width: f64, height: f64, #[serde(default)] mode: SelectMode, #[serde(default)] feather: f32, #[serde(default = "yes")] anti_alias: bool },
    #[serde(rename = "select.polygon")]
    Polygon { points: Vec<Point>, #[serde(default)] mode: SelectMode, #[serde(default)] feather: f32, #[serde(default = "yes")] anti_alias: bool },
    /// Coverage from single-channel `bytes` (`width`×`height` at `(x, y)`),
    /// as a segmentation model produces it.
    #[serde(rename = "select.mask")]
    Mask { x: i32, y: i32, width: u32, height: u32, #[serde(default)] mode: SelectMode, #[serde(default)] feather: f32, #[serde(default)] label: Option<String> },
    #[serde(rename = "select.layer-alpha")]
    LayerAlpha { id: LayerId, #[serde(default)] mode: SelectMode },
    #[serde(rename = "select.feather")]
    Feather { radius: f32 },
}

fn yes() -> bool {
    true
}

pub fn apply(v: serde_json::Value, doc: &mut Document, bytes: &[u8]) -> Result<Applied> {
    let cmd: Cmd = parse(v)?;
    let (w, h) = (doc.width, doc.height);
    let finish = |doc: &mut Document, mut plane: Plane, mode: SelectMode, feather: f32, label: &str| {
        selection::feather(&mut plane, feather);
        selection::combine(doc, plane, mode);
        Applied::step(label)
    };
    match cmd {
        Cmd::All => {
            doc.selection = Some(Plane::mask(w, h, 255));
            Ok(Applied::step("Select All"))
        }
        Cmd::None => {
            if doc.selection.is_none() {
                return Ok(Applied::quiet());
            }
            doc.selection = None;
            Ok(Applied::step("Deselect"))
        }
        Cmd::Invert => {
            selection::invert(doc);
            Ok(Applied::step("Inverse"))
        }
        Cmd::Rect { x, y, width, height, mode, feather } => {
            let pts = [Point::new(x, y), Point::new(x + width, y), Point::new(x + width, y + height), Point::new(x, y + height)];
            // Rectangular marquees snap to whole pixels, like Photoshop's.
            let r = Rect::cover(x.round(), y.round(), width.round(), height.round()).intersect(&Rect::new(0, 0, w as i32, h as i32));
            let plane = if r.is_empty() {
                polygon_mask(w, h, &pts, false)
            } else {
                let mut p = Plane::mask(w, h, 0);
                p.write(r, &vec![255; r.area() as usize]);
                p
            };
            Ok(finish(doc, plane, mode, feather, "Rectangular Marquee"))
        }
        Cmd::Ellipse { x, y, width, height, mode, feather, anti_alias } => {
            let plane = ellipse_mask(w, h, (x, y, width, height), anti_alias);
            Ok(finish(doc, plane, mode, feather, "Elliptical Marquee"))
        }
        Cmd::Polygon { points, mode, feather, anti_alias } => {
            if points.len() < 3 {
                return Err(EditorError::Invalid("a lasso needs at least three points".into()));
            }
            let plane = polygon_mask(w, h, &points, anti_alias);
            Ok(finish(doc, plane, mode, feather, "Lasso"))
        }
        Cmd::Mask { x, y, width, height, mode, feather, label } => {
            if bytes.len() != width as usize * height as usize {
                return Err(EditorError::Invalid(format!("expected {} mask bytes, got {}", width * height, bytes.len())));
            }
            let mut plane = Plane::mask(w, h, 0);
            plane.write(Rect::new(x, y, width as i32, height as i32), bytes);
            plane.compact();
            let label = label.unwrap_or_else(|| "Select".into());
            Ok(finish(doc, plane, mode, feather, &label))
        }
        Cmd::LayerAlpha { id, mode } => {
            let l = layer(doc, id)?;
            let Some(r) = l.raster() else {
                return Err(EditorError::Invalid(format!("`{}` has no pixels", l.name)));
            };
            let plane = selection::from_alpha(doc, r);
            Ok(finish(doc, plane, mode, 0.0, "Load Selection"))
        }
        Cmd::Feather { radius } => {
            let Some(mut sel) = doc.selection.take() else {
                return Err(EditorError::Invalid("there is no selection".into()));
            };
            selection::feather(&mut sel, radius);
            doc.selection = Some(sel);
            Ok(Applied::step("Feather"))
        }
    }
}
