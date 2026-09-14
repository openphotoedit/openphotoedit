//! Smart objects: containers whose pixels are rebuilt from a source, a stack
//! of live filters and a four-corner transform. Nothing about the source is
//! lost by scaling, rotating or filtering the object.
//!
//! Filters are ordinary engine commands (`filter.*`). Rebuilding runs them on
//! a scratch document through the same domain handlers the editor uses, so a
//! smart filter and a destructive filter are the same code.

use crate::blend::{composite_px, BlendMode};
use crate::document::Document;
use crate::geom::{Point, Rect};
use crate::layer::{Layer, LayerKind, Raster, SmartFilter, SmartSource};
use crate::ops::{DomainHandler, EditorError};
use crate::plane::Plane;
use crate::render::flatten;
use crate::transform::warp_to_quad;

/// The quad of an axis-aligned rectangle.
pub fn rect_quad(r: Rect) -> [Point; 4] {
    let (x0, y0, x1, y1) = (r.x as f64, r.y as f64, r.right() as f64, r.bottom() as f64);
    [Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)]
}

/// Flatten a source to pixels at its own resolution.
pub fn source_pixels(source: &SmartSource) -> Plane {
    match source {
        SmartSource::Pixels(p) => p.clone(),
        SmartSource::Document(d) => Plane::from_raw(d.width, d.height, 4, &flatten(d), [0; 4]),
    }
}

/// Rebuild a smart object's cached raster. Returns warnings for filters
/// that could not run.
pub fn rebuild(source: &SmartSource, quad: &[Point; 4], filters: &[SmartFilter], domains: &[(String, DomainHandler)]) -> (Raster, Vec<String>) {
    let mut warnings = Vec::new();
    let mut plane = source_pixels(source);
    let active: Vec<&SmartFilter> = filters.iter().filter(|f| f.enabled).collect();
    if !active.is_empty() {
        let (w, h) = (plane.width(), plane.height());
        let mut scratch = Document::new(w, h);
        let id = scratch.alloc_id();
        scratch.layers.push(Layer::new(id, "contents", LayerKind::Pixel(Raster::new(plane.clone(), 0, 0))));
        scratch.active = Some(id);
        for f in active {
            let before = match &scratch.layers[0].kind {
                LayerKind::Pixel(r) => r.plane.clone(),
                _ => unreachable!(),
            };
            let mut cmd = f.filter.clone();
            if let Some(obj) = cmd.as_object_mut() {
                obj.insert("id".into(), serde_json::json!(id));
            }
            let op = cmd.get("op").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let prefix = op.split('.').next().unwrap_or("").to_string();
            let mut ran = false;
            for (p, h) in domains.iter().filter(|(p, _)| *p == prefix) {
                let _ = p;
                match h(cmd.clone(), &mut scratch, &[]) {
                    Err(EditorError::UnknownOp(_)) => continue,
                    Err(e) => {
                        warnings.push(format!("{op}: {e}"));
                        ran = true;
                        break;
                    }
                    Ok(_) => {
                        ran = true;
                        break;
                    }
                }
            }
            if !ran {
                warnings.push(format!("{op} is not available"));
                continue;
            }
            // A filter that grew or moved the layer is kept within the object.
            if let LayerKind::Pixel(r) = &mut scratch.layers[0].kind {
                if r.x != 0 || r.y != 0 || r.plane.width() != w || r.plane.height() != h {
                    let mut fitted = Plane::transparent(w, h);
                    fitted.paste(&r.plane, r.x, r.y);
                    *r = Raster::new(fitted, 0, 0);
                }
                if f.opacity < 1.0 || f.blend != BlendMode::Normal {
                    blend_planes(&before, &mut r.plane, f.opacity.clamp(0.0, 1.0), f.blend);
                }
            }
        }
        if let LayerKind::Pixel(r) = &scratch.layers[0].kind {
            plane = r.plane.clone();
        }
    }
    let (w, h) = (plane.width() as i32, plane.height() as i32);
    let identity = rect_quad(Rect::new(quad[0].x.round() as i32, quad[0].y.round() as i32, w, h));
    let is_translation = quad.iter().zip(identity.iter()).all(|(a, b)| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
    let raster = if is_translation {
        Raster::new(plane, identity[0].x as i32, identity[0].y as i32)
    } else {
        let (p, x, y) = warp_to_quad(&plane, quad);
        Raster::new(p, x, y)
    };
    (raster, warnings)
}

/// "Fade" a filter result against what it replaced.
fn blend_planes(before: &Plane, after: &mut Plane, opacity: f32, mode: BlendMode) {
    let rects = {
        let mut r = before.present_rects();
        r.extend(after.present_rects());
        r.sort_by_key(|r| (r.y, r.x));
        r.dedup();
        r
    };
    for rect in rects {
        let b = before.read_vec(rect);
        let a = after.read_vec(rect);
        let mut out = vec![0u8; a.len()];
        for k in 0..(rect.area() as usize) {
            let o = k * 4;
            let mut d = [b[o] as f32 / 255.0, b[o + 1] as f32 / 255.0, b[o + 2] as f32 / 255.0, b[o + 3] as f32 / 255.0];
            let sa = a[o + 3] as f32 / 255.0 * opacity;
            composite_px(&mut d, [a[o] as f32 / 255.0, a[o + 1] as f32 / 255.0, a[o + 2] as f32 / 255.0], sa, mode);
            for c in 0..4 {
                out[o + c] = (d[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        after.write(rect, &out);
    }
    after.compact();
}

/// Rebuild every stale smart object in a layer list. Returns warnings.
pub fn refresh(layers: &mut [Layer], domains: &[(String, DomainHandler)]) -> Vec<String> {
    let mut warnings = Vec::new();
    for l in layers.iter_mut() {
        match &mut l.kind {
            LayerKind::Smart { source, quad, filters, raster, stale } if *stale => {
                let (r, w) = rebuild(source, quad, filters, domains);
                *raster = r;
                *stale = false;
                warnings.extend(w);
                l.touch();
            }
            LayerKind::Group { children, .. } => {
                let w = refresh(children, domains);
                if !w.is_empty() {
                    warnings.extend(w);
                }
            }
            _ => {}
        }
    }
    warnings
}

/// Whether any smart object in the tree needs rebuilding.
pub fn any_stale(layers: &[Layer]) -> bool {
    layers.iter().any(|l| match &l.kind {
        LayerKind::Smart { stale, .. } => *stale,
        LayerKind::Group { children, .. } => any_stale(children),
        _ => false,
    })
}

/// Map every quad corner through `f` and mark the object stale.
pub fn map_quad(kind: &mut LayerKind, f: impl Fn(Point) -> Point) {
    if let LayerKind::Smart { quad, stale, .. } = kind {
        for p in quad.iter_mut() {
            *p = f(*p);
        }
        *stale = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::render::flatten;

    fn blur_handler(v: serde_json::Value, doc: &mut crate::document::Document, _b: &[u8]) -> crate::ops::Result<crate::ops::Applied> {
        // A stand-in filter: invert, so the effect is easy to assert.
        if v["op"] != "filter.test-invert" {
            return Err(crate::ops::EditorError::UnknownOp(v["op"].to_string()));
        }
        let id = v["id"].as_u64().unwrap() as u32;
        let rect = crate::geom::Rect::new(0, 0, doc.width as i32, doc.height as i32);
        crate::pixels::edit_layer(doc, id, crate::pixels::EditScope { rect, apron: 0 }, |buf, _, _| {
            for p in buf.chunks_exact_mut(4) {
                for c in &mut p[..3] {
                    *c = 255 - *c;
                }
            }
        })?;
        Ok(crate::ops::Applied::step("Invert"))
    }

    fn editor_with_photo() -> Editor {
        let mut ed = Editor::new(1, 1);
        ed.register_domain("filter", blur_handler);
        let px: Vec<u8> = (0..40 * 20).flat_map(|i| [(i % 40) as u8 * 6, 50, 100, 255]).collect();
        ed.exec_json(r#"{"op":"doc.open-pixels","width":40,"height":20}"#, &px).unwrap();
        ed
    }

    #[test]
    fn convert_scale_and_filter_are_lossless() {
        let mut ed = editor_with_photo();
        let id = ed.doc.active.unwrap();
        let before = flatten(&ed.doc);
        let r = ed.exec_json(&format!(r#"{{"op":"layer.convert-to-smart","ids":[{id}]}}"#), &[]).unwrap();
        let sid = r["data"]["id"].as_u64().unwrap();
        assert_eq!(flatten(&ed.doc), before, "converting changes nothing visible");
        // Shrink to a quarter, then scale back up: the source is untouched.
        ed.exec_json(&format!(r#"{{"op":"layer.smart-transform","id":{sid},"quad":[{{"x":0,"y":0}},{{"x":10,"y":0}},{{"x":10,"y":5}},{{"x":0,"y":5}}]}}"#), &[]).unwrap();
        assert_eq!(flatten(&ed.doc)[(10 * 40 + 30) * 4 + 3], 0, "shrunk");
        ed.exec_json(&format!(r#"{{"op":"layer.smart-transform","id":{sid},"quad":[{{"x":0,"y":0}},{{"x":40,"y":0}},{{"x":40,"y":20}},{{"x":0,"y":20}}]}}"#), &[]).unwrap();
        assert_eq!(flatten(&ed.doc), before, "scaling back up restores the original exactly");
        let r = ed.exec_json(&format!(r#"{{"op":"layer.smart-filter-add","id":{sid},"filter":{{"op":"filter.test-invert"}}}}"#), &[]).unwrap();
        assert!(r["warnings"].as_array().unwrap().is_empty());
        assert_eq!(flatten(&ed.doc)[2], 255 - 100);
        ed.exec_json(&format!(r#"{{"op":"layer.smart-filter-set","id":{sid},"index":0,"enabled":false}}"#), &[]).unwrap();
        assert_eq!(flatten(&ed.doc), before);
        ed.exec_json(r#"{"op":"edit.undo"}"#, &[]).unwrap();
        assert_eq!(flatten(&ed.doc)[2], 255 - 100, "undo restores the enabled filter");
    }

    #[test]
    fn unknown_filter_warns_instead_of_failing() {
        let mut ed = editor_with_photo();
        let id = ed.doc.active.unwrap();
        let r = ed.exec_json(&format!(r#"{{"op":"layer.convert-to-smart","ids":[{id}]}}"#), &[]).unwrap();
        let sid = r["data"]["id"].as_u64().unwrap();
        let r = ed.exec_json(&format!(r#"{{"op":"layer.smart-filter-add","id":{sid},"filter":{{"op":"filter.nope"}}}}"#), &[]).unwrap();
        assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn group_to_smart_and_back() {
        let mut ed = editor_with_photo();
        let bottom = ed.doc.active.unwrap();
        let top = ed.exec_json(r#"{"op":"layer.add-adjustment","adjustment":{"kind":"invert"}}"#, &[]).unwrap()["data"]["id"].as_u64().unwrap();
        let before = flatten(&ed.doc);
        let sid = ed.exec_json(&format!(r#"{{"op":"layer.convert-to-smart","ids":[{bottom},{top}]}}"#), &[]).unwrap()["data"]["id"].as_u64().unwrap();
        assert_eq!(ed.doc.layers.len(), 1);
        assert_eq!(flatten(&ed.doc), before);
        ed.exec_json(&format!(r#"{{"op":"layer.smart-unpack","id":{sid}}}"#), &[]).unwrap();
        assert_eq!(ed.doc.layers.len(), 2);
        assert_eq!(flatten(&ed.doc), before);
    }

    #[test]
    fn image_resize_keeps_smart_source() {
        let mut ed = editor_with_photo();
        let id = ed.doc.active.unwrap();
        ed.exec_json(&format!(r#"{{"op":"layer.convert-to-smart","ids":[{id}]}}"#), &[]).unwrap();
        let before = flatten(&ed.doc);
        ed.exec_json(r#"{"op":"image.resize","width":10,"height":5}"#, &[]).unwrap();
        ed.exec_json(r#"{"op":"image.resize","width":40,"height":20}"#, &[]).unwrap();
        assert_eq!(flatten(&ed.doc), before, "down and back up is lossless for a smart object");
    }
}
