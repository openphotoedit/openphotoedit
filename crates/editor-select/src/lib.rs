//! `select.*` commands beyond the marquee and lasso shapes core handles:
//! magic wand, similar, color range, grow/shrink/border/smooth, transform,
//! quick selection and refine edge. Registered with the editor by
//! `register`; core falls through to [`apply`] for operations it does not
//! know.
//!
//! The building blocks (tolerance flood fill, distance transform, guided
//! filter) are public so the paint crate's bucket and magic eraser share
//! them.

// `as_chunks` postdates the workspace's rust-version (1.85).
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod guided;
pub mod morph;
pub mod quick;
pub mod range;
pub mod refine;
pub mod sample;
pub mod wand;
pub mod warp;

use editor_core::color::Rgba8;
use editor_core::document::Document;
use editor_core::editor::Editor;
use editor_core::geom::{Affine, Point, Rect};
use editor_core::layer::LayerId;
use editor_core::ops::{parse, Applied, EditorError, Result};
use editor_core::plane::Plane;
use editor_core::selection::{self, SelectMode};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn register(ed: &mut Editor) {
    ed.register_domain("select", apply);
}

fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}
fn four() -> u8 {
    4
}
fn default_tolerance() -> f32 {
    32.0
}
fn default_fuzziness() -> f32 {
    40.0
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Cmd {
    #[serde(rename = "select.magic-wand")]
    MagicWand {
        x: f64,
        y: f64,
        #[serde(default = "default_tolerance")]
        tolerance: f32,
        #[serde(default = "yes")]
        contiguous: bool,
        #[serde(default)]
        sample_all: bool,
        #[serde(default = "yes")]
        anti_alias: bool,
        #[serde(default)]
        mode: SelectMode,
        #[serde(default)]
        id: Option<LayerId>,
        /// 4 or 8.
        #[serde(default = "four")]
        connectivity: u8,
        /// Point sample (1) or the average of a 3×3 / 5×5 square.
        #[serde(default = "one")]
        sample_size: u32,
    },
    #[serde(rename = "select.similar")]
    Similar {
        #[serde(default = "default_tolerance")]
        tolerance: f32,
        #[serde(default)]
        sample_all: bool,
        #[serde(default = "yes")]
        anti_alias: bool,
        #[serde(default)]
        id: Option<LayerId>,
    },
    #[serde(rename = "select.color-range")]
    ColorRange {
        #[serde(default)]
        color: Option<Rgba8>,
        #[serde(default)]
        preset: Option<range::Preset>,
        #[serde(default = "default_fuzziness")]
        fuzziness: f32,
        #[serde(default)]
        mode: SelectMode,
    },
    #[serde(rename = "select.grow")]
    Grow { by: f32 },
    #[serde(rename = "select.shrink")]
    Shrink { by: f32 },
    #[serde(rename = "select.border")]
    Border { width: f32 },
    #[serde(rename = "select.smooth")]
    Smooth { radius: f32 },
    #[serde(rename = "select.transform")]
    Transform { matrix: Affine },
    #[serde(rename = "select.quick")]
    Quick {
        points: Vec<Point>,
        radius: f64,
        #[serde(default = "quick_default_mode")]
        mode: SelectMode,
        #[serde(default)]
        stroke_id: Option<Value>,
    },
    #[serde(rename = "select.refine")]
    Refine {
        #[serde(flatten)]
        params: refine::Params,
        #[serde(default)]
        decontaminate: bool,
    },
}

fn quick_default_mode() -> SelectMode {
    SelectMode::Add
}

const OPS: [&str; 10] = [
    "select.magic-wand",
    "select.similar",
    "select.color-range",
    "select.grow",
    "select.shrink",
    "select.border",
    "select.smooth",
    "select.transform",
    "select.quick",
    "select.refine",
];

fn current(doc: &Document) -> Result<&Plane> {
    doc.selection.as_ref().ok_or_else(|| EditorError::Invalid("there is no selection".into()))
}

fn replace_selection(doc: &mut Document, plane: Plane, label: &str) -> Applied {
    let before = doc.selection.as_ref().map(morph::selection_area).unwrap_or_default();
    let after = morph::selection_area(&plane);
    selection::combine(doc, plane, SelectMode::Replace);
    Applied::step(label).dirty(before.union(&after))
}

pub fn apply(v: Value, doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
    if !OPS.contains(&op.as_str()) {
        return Err(EditorError::UnknownOp(op));
    }
    let cmd: Cmd = parse(v)?;
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    match cmd {
        Cmd::MagicWand { x, y, tolerance, contiguous, sample_all, anti_alias, mode, id, connectivity, sample_size } => {
            let (px, py) = (x.floor() as i32, y.floor() as i32);
            if !canvas.contains(px, py) {
                return Err(EditorError::Invalid("click inside the image to use the magic wand".into()));
            }
            let tol = tolerance.round().clamp(0.0, 255.0) as u8;
            let layer = id.or(doc.active);
            let mut reg = {
                let mut s = sample::Sampler::new(doc, layer, sample_all);
                let seed = wand::seed_color(&mut s, px, py, sample_size);
                if contiguous {
                    wand::contiguous(&mut s, px, py, seed, tol, connectivity == 8)
                } else {
                    wand::global(&s, seed, tol)
                }
            };
            if anti_alias {
                wand::antialias(&mut reg);
            }
            let dirty = reg.bounds;
            selection::combine(doc, reg.to_plane(), mode);
            Ok(Applied::step("Magic Wand").dirty(dirty))
        }
        Cmd::Similar { tolerance, sample_all, anti_alias, id } => {
            let sel = current(doc)?.clone();
            let tol = tolerance.round().clamp(0.0, 255.0) as u8;
            let reg = {
                let s = sample::Sampler::new(doc, id.or(doc.active), sample_all);
                wand::similar(&s, &sel, tol, anti_alias)
            };
            Ok(replace_selection(doc, reg.to_plane(), "Similar"))
        }
        Cmd::ColorRange { color, preset, fuzziness, mode } => {
            if color.is_none() && preset.is_none() {
                return Err(EditorError::Invalid("color range needs a colour or a preset".into()));
            }
            let scorer = range::Scorer::new(color, preset, fuzziness);
            let plane = range::color_range(doc, &scorer);
            selection::combine(doc, plane, mode);
            Ok(Applied::step("Color Range").dirty(canvas))
        }
        Cmd::Grow { by } => {
            let p = morph::grow(current(doc)?, by);
            Ok(replace_selection(doc, p, "Expand"))
        }
        Cmd::Shrink { by } => {
            let p = morph::shrink(current(doc)?, by);
            Ok(replace_selection(doc, p, "Contract"))
        }
        Cmd::Border { width } => {
            let p = morph::border(current(doc)?, width);
            Ok(replace_selection(doc, p, "Border"))
        }
        Cmd::Smooth { radius } => {
            let p = morph::smooth(current(doc)?, radius.round().max(0.0) as u32);
            Ok(replace_selection(doc, p, "Smooth"))
        }
        Cmd::Transform { matrix } => {
            let p = warp::transform(current(doc)?, &matrix).ok_or_else(|| EditorError::Invalid("the transform squashes the selection flat".into()))?;
            Ok(replace_selection(doc, p, "Transform Selection"))
        }
        Cmd::Quick { points, radius, mode, stroke_id } => {
            let qm = match mode {
                SelectMode::Replace => quick::QuickMode::New,
                SelectMode::Add => quick::QuickMode::Add,
                SelectMode::Subtract => quick::QuickMode::Subtract,
                SelectMode::Intersect => return Err(EditorError::Invalid("quick selection adds or subtracts".into())),
            };
            if points.is_empty() {
                return Err(EditorError::Invalid("quick selection needs at least one point".into()));
            }
            let id = stroke_id.map(|v| match v {
                Value::String(s) => s,
                other => other.to_string(),
            });
            let dirty = quick::apply(doc, &points, radius, qm, id.as_deref());
            let mut applied = Applied::step("Quick Selection").dirty(dirty);
            if let Some(id) = id {
                applied = applied.merge(format!("quick:{id}"));
            }
            Ok(applied)
        }
        Cmd::Refine { params, decontaminate } => {
            let sel = current(doc)?.clone();
            let p = refine::refine(doc, &sel, &params);
            let mut applied = replace_selection(doc, p, "Refine Edge");
            if decontaminate {
                applied = applied.with_data(json!({ "decontaminate": "not supported: colour decontamination is not applied" }));
            }
            Ok(applied)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::Editor;

    fn editor(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Editor {
        let mut ed = Editor::new(1, 1);
        register(&mut ed);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        ed.exec(json!({"op": "doc.open-pixels", "width": w, "height": h}), &px).unwrap();
        ed
    }

    fn sel_at(ed: &Editor, x: i32, y: i32) -> u8 {
        ed.doc.selection.as_ref().map_or(255, |s| s.get(x, y)[0])
    }

    #[test]
    fn unknown_ops_fall_through() {
        let mut doc = Document::new(4, 4);
        assert!(matches!(apply(json!({"op": "select.bogus"}), &mut doc, &[]), Err(EditorError::UnknownOp(_))));
    }

    #[test]
    fn magic_wand_through_the_editor_with_modes_and_undo() {
        let mut ed = editor(40, 20, |x, _| if x < 20 { [250, 250, 250, 255] } else { [20, 20, 200, 255] });
        let r = ed.exec(json!({"op": "select.magic-wand", "x": 5, "y": 5, "tolerance": 10, "anti_alias": false}), &[]).unwrap();
        assert_eq!(r["changed"], true);
        assert_eq!(sel_at(&ed, 3, 3), 255);
        assert_eq!(sel_at(&ed, 30, 3), 0);
        ed.exec(json!({"op": "select.magic-wand", "x": 30, "y": 5, "tolerance": 10, "mode": "add", "anti_alias": false}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 30, 3), 255);
        ed.exec(json!({"op": "select.magic-wand", "x": 5, "y": 5, "tolerance": 10, "mode": "subtract"}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 3, 3), 0);
        ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 3, 3), 255);
        assert!(ed.exec(json!({"op": "select.magic-wand", "x": 500, "y": 5}), &[]).is_err());
    }

    #[test]
    fn similar_grow_shrink_border_smooth_transform_via_commands() {
        let mut ed = editor(60, 30, |x, _| if (10..20).contains(&x) || (40..50).contains(&x) { [200, 30, 30, 255] } else { [30, 30, 30, 255] });
        assert!(ed.exec(json!({"op": "select.grow", "by": 2}), &[]).is_err(), "no selection");
        ed.exec(json!({"op": "select.rect", "x": 12, "y": 5, "width": 4, "height": 4}), &[]).unwrap();
        ed.exec(json!({"op": "select.similar", "tolerance": 16, "anti_alias": false}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 45, 20), 255);
        assert_eq!(sel_at(&ed, 30, 20), 0);
        ed.exec(json!({"op": "select.grow", "by": 3}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 22, 20), 255);
        assert_eq!(sel_at(&ed, 24, 20), 0);
        ed.exec(json!({"op": "select.shrink", "by": 3}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 20, 20), 0);
        assert_eq!(sel_at(&ed, 19, 20), 255);
        ed.exec(json!({"op": "select.smooth", "radius": 1}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 15, 15), 255);
        ed.exec(json!({"op": "select.transform", "matrix": {"a": 1, "b": 0, "c": 0, "d": 1, "e": 5, "f": 0}}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 12, 15), 0);
        assert_eq!(sel_at(&ed, 22, 15), 255);
        ed.exec(json!({"op": "select.border", "width": 4}), &[]).unwrap();
        assert!(sel_at(&ed, 15, 15) > 128);
        assert_eq!(sel_at(&ed, 20, 15), 0);
    }

    #[test]
    fn color_range_and_refine_commands() {
        let mut ed = editor(40, 20, |x, _| if x < 20 { [220, 180, 150, 255] } else { [30, 90, 40, 255] });
        ed.exec(json!({"op": "select.color-range", "color": {"r": 220, "g": 180, "b": 150}, "fuzziness": 30}), &[]).unwrap();
        assert_eq!(sel_at(&ed, 5, 5), 255);
        assert_eq!(sel_at(&ed, 30, 5), 0);
        ed.exec(json!({"op": "select.color-range", "preset": "skin", "fuzziness": 0}), &[]).unwrap();
        assert!(sel_at(&ed, 5, 5) > 128);
        assert!(ed.exec(json!({"op": "select.color-range", "fuzziness": 10}), &[]).is_err());
        ed.exec(json!({"op": "select.rect", "x": 0, "y": 0, "width": 16, "height": 20}), &[]).unwrap();
        let r = ed.exec(json!({"op": "select.refine", "radius": 6, "smooth": 0, "feather": 0, "contrast": 0, "shift_edge": 0, "decontaminate": true}), &[]).unwrap();
        assert!(r["data"]["decontaminate"].is_string());
        assert!(sel_at(&ed, 18, 10) > 180, "{}", sel_at(&ed, 18, 10));
        assert!(sel_at(&ed, 22, 10) < 75, "{}", sel_at(&ed, 22, 10));
    }

    #[test]
    fn quick_selection_segments_merge_into_one_undo_step() {
        let mut ed = editor(120, 80, |x, y| {
            let d = ((x as f32 - 60.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt();
            if d < 25.0 {
                [40, 40, 120, 255]
            } else {
                [230, 220, 200, 255]
            }
        });
        let undo_len = |ed: &Editor| ed.history.undo_labels().len();
        let n0 = undo_len(&ed);
        for (i, x) in [52.0, 56.0, 60.0, 64.0].iter().enumerate() {
            ed.exec(json!({"op": "select.quick", "points": [{"x": x, "y": 40}], "radius": 4, "mode": "add", "stroke_id": 7, "i": i}), &[]).unwrap();
        }
        assert_eq!(undo_len(&ed), n0 + 1);
        assert_eq!(sel_at(&ed, 60, 40), 255);
        assert_eq!(sel_at(&ed, 5, 5), 0);
        ed.exec(json!({"op": "edit.undo"}), &[]).unwrap();
        assert!(ed.doc.selection.is_none());
    }
}
