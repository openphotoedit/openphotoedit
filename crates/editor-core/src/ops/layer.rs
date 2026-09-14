//! `layer.*` and `doc.*` commands: the layer tree, layer properties, masks,
//! merging, and pixel import.

use serde::Deserialize;

use super::{layer, layer_mut, parse, target_id, Applied, EditorError, Result};
use crate::adjust::Adjustment;
use crate::blend::BlendMode;
use crate::color::Rgba8;
use crate::document::Document;
use crate::geom::{Point, Rect};
use crate::layer::{Fill, Layer, LayerId, LayerKind, LayerMask, Locks, Raster, ShapeData, SmartFilter, SmartSource, TextData};
use crate::plane::Plane;
use crate::render::{render_layers, to_u8, View};
use crate::selection;
use crate::shape::rasterize_shape;

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum MaskFrom {
    RevealAll,
    HideAll,
    Selection,
    HideSelection,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Up,
    Down,
    Top,
    Bottom,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Cmd {
    #[serde(rename = "doc.new")]
    NewDocument { width: u32, height: u32, #[serde(default)] background: Option<Rgba8>, #[serde(default)] resolution: Option<f32> },
    /// Replace the document with one layer holding RGBA `bytes`.
    #[serde(rename = "doc.open-pixels")]
    OpenPixels { width: u32, height: u32, #[serde(default)] name: Option<String> },
    #[serde(rename = "doc.set-resolution")]
    SetResolution { resolution: f32 },

    #[serde(rename = "layer.add-pixel")]
    AddPixel { #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId> },
    /// A new layer from RGBA `bytes` of `width`×`height` placed at `(x, y)`.
    #[serde(rename = "layer.import")]
    Import { width: u32, height: u32, #[serde(default)] x: i32, #[serde(default)] y: i32, #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId>, #[serde(default)] provenance: Option<String> },
    /// Overwrite a region of a pixel layer with RGBA `bytes`. With
    /// `respect_selection`, the new pixels only land where selected.
    #[serde(rename = "layer.set-pixels")]
    SetPixels { #[serde(default)] id: Option<LayerId>, x: i32, y: i32, width: u32, height: u32, #[serde(default)] respect_selection: bool, #[serde(default)] label: Option<String>, #[serde(default)] provenance: Option<String> },
    #[serde(rename = "layer.add-adjustment")]
    AddAdjustment { adjustment: Adjustment, #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId> },
    #[serde(rename = "layer.set-adjustment")]
    SetAdjustment { id: LayerId, adjustment: Adjustment },
    #[serde(rename = "layer.add-fill")]
    AddFill { fill: Fill, #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId> },
    #[serde(rename = "layer.set-fill")]
    SetFill { id: LayerId, fill: Fill },
    /// A text layer; `bytes` is the browser's RGBA rendering of it.
    #[serde(rename = "layer.add-text")]
    AddText { data: TextData, width: u32, height: u32, x: i32, y: i32, #[serde(default)] above: Option<LayerId> },
    #[serde(rename = "layer.set-text")]
    SetText { id: LayerId, data: TextData, width: u32, height: u32, x: i32, y: i32 },
    #[serde(rename = "layer.add-shape")]
    AddShape { data: ShapeData, #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId> },
    #[serde(rename = "layer.set-shape")]
    SetShape { id: LayerId, data: ShapeData },

    #[serde(rename = "layer.group")]
    Group { ids: Vec<LayerId>, #[serde(default)] name: Option<String> },
    #[serde(rename = "layer.ungroup")]
    Ungroup { id: LayerId },
    #[serde(rename = "layer.delete")]
    Delete { ids: Vec<LayerId> },
    #[serde(rename = "layer.duplicate")]
    Duplicate { ids: Vec<LayerId> },
    #[serde(rename = "layer.move")]
    Move { id: LayerId, #[serde(default)] parent: Option<LayerId>, index: usize },
    #[serde(rename = "layer.reorder")]
    Reorder { id: LayerId, direction: Direction },
    #[serde(rename = "layer.props")]
    Props {
        id: LayerId,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        fill_opacity: Option<f32>,
        #[serde(default)]
        blend: Option<BlendMode>,
        #[serde(default)]
        clip: Option<bool>,
        #[serde(default)]
        locks: Option<Locks>,
        #[serde(default)]
        color_label: Option<u8>,
        #[serde(default)]
        pass_through: Option<bool>,
        #[serde(default)]
        expanded: Option<bool>,
    },
    /// Move layers by a document offset (the Move tool).
    #[serde(rename = "layer.offset")]
    Offset { ids: Vec<LayerId>, dx: i32, dy: i32 },
    #[serde(rename = "layer.set-active")]
    SetActive { id: Option<LayerId> },

    #[serde(rename = "layer.merge-down")]
    MergeDown { id: LayerId },
    #[serde(rename = "layer.merge-visible")]
    MergeVisible,
    #[serde(rename = "layer.flatten")]
    Flatten,
    /// Composite of everything visible as a new layer on top.
    #[serde(rename = "layer.stamp-visible")]
    StampVisible,
    #[serde(rename = "layer.rasterize")]
    Rasterize { id: LayerId },

    /// Replace the layer style; `null` clears it.
    #[serde(rename = "layer.set-effects")]
    SetEffects { id: LayerId, effects: Option<crate::effects::LayerEffects> },
    #[serde(rename = "layer.add-mask")]
    AddMask { #[serde(default)] id: Option<LayerId>, from: MaskFrom },
    #[serde(rename = "layer.delete-mask")]
    DeleteMask { id: LayerId, #[serde(default)] apply: bool },
    #[serde(rename = "layer.mask-props")]
    MaskProps { id: LayerId, #[serde(default)] enabled: Option<bool>, #[serde(default)] linked: Option<bool>, #[serde(default)] density: Option<f32> },
    /// Replace a mask region with single-channel `bytes`.
    #[serde(rename = "layer.set-mask-pixels")]
    SetMaskPixels { id: LayerId, x: i32, y: i32, width: u32, height: u32 },

    /// Place pixels as a smart object (scales without losing resolution).
    #[serde(rename = "layer.place-smart")]
    PlaceSmart { width: u32, height: u32, #[serde(default)] x: i32, #[serde(default)] y: i32, #[serde(default)] name: Option<String>, #[serde(default)] above: Option<LayerId> },
    #[serde(rename = "layer.convert-to-smart")]
    ConvertToSmart { ids: Vec<LayerId>, #[serde(default)] name: Option<String> },
    /// Where the object's corners land (TL, TR, BR, BL).
    #[serde(rename = "layer.smart-transform")]
    SmartTransform { id: LayerId, quad: [Point; 4] },
    #[serde(rename = "layer.smart-filter-add")]
    SmartFilterAdd { id: LayerId, filter: serde_json::Value, #[serde(default = "one")] opacity: f32, #[serde(default)] blend: BlendMode },
    #[serde(rename = "layer.smart-filter-set")]
    SmartFilterSet { id: LayerId, index: usize, #[serde(default)] filter: Option<serde_json::Value>, #[serde(default)] enabled: Option<bool>, #[serde(default)] opacity: Option<f32>, #[serde(default)] blend: Option<BlendMode> },
    #[serde(rename = "layer.smart-filter-remove")]
    SmartFilterRemove { id: LayerId, index: usize },
    #[serde(rename = "layer.smart-filter-move")]
    SmartFilterMove { id: LayerId, from: usize, to: usize },
    /// Replace the contents with new RGBA pixels, keeping the transform and filters.
    #[serde(rename = "layer.smart-replace")]
    SmartReplace { id: LayerId, width: u32, height: u32 },
    /// Turn a smart object back into its layers (or a plain pixel layer).
    #[serde(rename = "layer.smart-unpack")]
    SmartUnpack { id: LayerId },

    /// Layer via copy / via cut from the selection.
    #[serde(rename = "layer.from-selection")]
    FromSelection { #[serde(default)] id: Option<LayerId>, #[serde(default)] cut: bool },
    /// Delete selected pixels (to transparency).
    #[serde(rename = "layer.clear")]
    Clear { #[serde(default)] id: Option<LayerId> },
    /// Fill the selection (or the layer) with a colour.
    #[serde(rename = "layer.fill-selection")]
    FillSelection { #[serde(default)] id: Option<LayerId>, color: Rgba8, #[serde(default = "one")] opacity: f32 },
}

fn one() -> f32 {
    1.0
}

pub fn apply(v: serde_json::Value, doc: &mut Document, bytes: &[u8]) -> Result<Applied> {
    let cmd: Cmd = parse(v)?;
    run(cmd, doc, bytes)
}

fn check_rgba(bytes: &[u8], w: u32, h: u32) -> Result<()> {
    let want = w as usize * h as usize * 4;
    if bytes.len() != want {
        return Err(EditorError::Invalid(format!("expected {want} bytes of RGBA for {w}x{h}, got {}", bytes.len())));
    }
    Ok(())
}

fn editable_raster<'a>(doc: &'a mut Document, id: LayerId) -> Result<&'a mut Layer> {
    let l = layer_mut(doc, id)?;
    if l.locks.all || l.locks.pixels {
        return Err(EditorError::Locked(l.name.clone()));
    }
    match l.kind {
        LayerKind::Pixel(_) => Ok(l),
        LayerKind::Text { .. } | LayerKind::Shape { .. } => Err(EditorError::Invalid(format!(
            "`{}` is a {} layer; rasterize it before editing its pixels",
            l.name,
            l.kind.name()
        ))),
        _ => Err(EditorError::Invalid(format!("`{}` has no pixels to edit", l.name))),
    }
}

fn default_name(doc: &Document, base: &str) -> String {
    let mut n = 1;
    doc.walk(&mut |l| {
        if l.name.starts_with(base) {
            n += 1;
        }
    });
    format!("{base} {n}")
}

/// Mask built from the current selection, if there is one.
fn mask_from_selection(doc: &Document) -> Option<LayerMask> {
    doc.selection.as_ref().map(|s| LayerMask { raster: Raster::new(s.clone(), 0, 0), enabled: true, linked: true, density: 1.0 })
}

fn add_layer(doc: &mut Document, mut l: Layer, above: Option<LayerId>) -> LayerId {
    let id = l.id;
    let anchor = above.or(doc.active);
    // Adding above a group's header puts the layer inside nothing surprising:
    // it lands as a sibling above the anchor.
    l.touch();
    doc.insert_above(anchor, l);
    doc.active = Some(id);
    id
}

pub fn run(cmd: Cmd, doc: &mut Document, bytes: &[u8]) -> Result<Applied> {
    match cmd {
        Cmd::NewDocument { width, height, background, resolution } => {
            if width == 0 || height == 0 || width > 100_000 || height > 100_000 {
                return Err(EditorError::Invalid("canvas must be 1..100000 px on each side".into()));
            }
            *doc = Document::new(width, height);
            if let Some(r) = resolution {
                doc.resolution = r;
            }
            let id = doc.alloc_id();
            let mut raster = Raster::empty(width, height);
            if let Some(bg) = background.filter(|c| c.a > 0) {
                raster.plane = Plane::new(width, height, 4, bg.to_array());
            }
            let name = if background.is_some() { "Background" } else { "Layer 1" };
            doc.layers.push(Layer::new(id, name, LayerKind::Pixel(raster)));
            doc.active = Some(id);
            Ok(Applied::step("New Document"))
        }
        Cmd::OpenPixels { width, height, name } => {
            check_rgba(bytes, width, height)?;
            *doc = Document::new(width, height);
            let id = doc.alloc_id();
            let plane = Plane::from_raw(width, height, 4, bytes, [0; 4]);
            doc.layers.push(Layer::new(id, "Background", LayerKind::Pixel(Raster::new(plane, 0, 0))));
            doc.active = Some(id);
            doc.meta.source_name = name;
            Ok(Applied::step("Open"))
        }
        Cmd::SetResolution { resolution } => {
            doc.resolution = resolution.clamp(1.0, 10_000.0);
            Ok(Applied::step("Image Resolution"))
        }
        Cmd::AddPixel { name, above } => {
            let id = doc.alloc_id();
            let name = name.unwrap_or_else(|| default_name(doc, "Layer"));
            let l = Layer::new(id, name, LayerKind::Pixel(Raster::empty(doc.width, doc.height)));
            add_layer(doc, l, above);
            Ok(Applied::step("New Layer").with_data(serde_json::json!({ "id": id })))
        }
        Cmd::Import { width, height, x, y, name, above, provenance } => {
            check_rgba(bytes, width, height)?;
            let id = doc.alloc_id();
            let plane = Plane::from_raw(width, height, 4, bytes, [0; 4]);
            let mut l = Layer::new(id, name.unwrap_or_else(|| default_name(doc, "Layer")), LayerKind::Pixel(Raster::new(plane, x, y)));
            if let Some(p) = provenance {
                l.provenance.push(p);
            }
            add_layer(doc, l, above);
            Ok(Applied::step("Place").with_data(serde_json::json!({ "id": id })).dirty(Rect::new(x, y, width as i32, height as i32)))
        }
        Cmd::SetPixels { id, x, y, width, height, respect_selection, label, provenance } => {
            check_rgba(bytes, width, height)?;
            let id = target_id(doc, id)?;
            let rect = Rect::new(x, y, width as i32, height as i32);
            let sel = if respect_selection { doc.selection.clone() } else { None };
            let l = editable_raster(doc, id)?;
            if let Some(p) = provenance {
                l.provenance.push(p);
            }
            let preserve_alpha = l.locks.transparency;
            let LayerKind::Pixel(r) = &mut l.kind else { unreachable!() };
            r.ensure_covers(rect);
            let local = rect.translate(-r.x, -r.y);
            if sel.is_none() && !preserve_alpha {
                r.plane.write(local, bytes);
            } else {
                let old = r.plane.read_vec(local);
                let mut data = bytes.to_vec();
                for k in 0..(width as usize * height as usize) {
                    let o = k * 4;
                    let (dx, dy) = (x + (k % width as usize) as i32, y + (k / width as usize) as i32);
                    let cov = sel.as_ref().map_or(255, |s| s.get(dx, dy)[0]) as u32;
                    for c in 0..4 {
                        data[o + c] = ((old[o + c] as u32 * (255 - cov) + data[o + c] as u32 * cov + 127) / 255) as u8;
                    }
                    if preserve_alpha {
                        data[o + 3] = old[o + 3];
                    }
                }
                r.plane.write(local, &data);
            }
            r.plane.compact();
            Ok(Applied::step(label.unwrap_or_else(|| "Edit Pixels".into())).dirty(rect))
        }
        Cmd::AddAdjustment { adjustment, name, above } => {
            let id = doc.alloc_id();
            let name = name.unwrap_or_else(|| adjustment.label().to_string());
            let mut l = Layer::new(id, name, LayerKind::Adjustment(adjustment));
            l.mask = mask_from_selection(doc);
            add_layer(doc, l, above);
            Ok(Applied::step("New Adjustment Layer").with_data(serde_json::json!({ "id": id })))
        }
        Cmd::SetAdjustment { id, adjustment } => {
            let l = layer_mut(doc, id)?;
            let LayerKind::Adjustment(a) = &mut l.kind else {
                return Err(EditorError::Invalid(format!("`{}` is not an adjustment layer", l.name)));
            };
            let label = format!("Edit {}", adjustment.label());
            *a = adjustment;
            Ok(Applied::step(label).merge(format!("adjust:{id}")))
        }
        Cmd::AddFill { fill, name, above } => {
            let id = doc.alloc_id();
            let label = match fill {
                Fill::Solid { .. } => "Color Fill",
                Fill::Gradient { .. } => "Gradient Fill",
            };
            let mut l = Layer::new(id, name.unwrap_or_else(|| default_name(doc, label)), LayerKind::Fill(fill));
            l.mask = mask_from_selection(doc);
            add_layer(doc, l, above);
            Ok(Applied::step(format!("New {label} Layer")).with_data(serde_json::json!({ "id": id })))
        }
        Cmd::SetFill { id, fill } => {
            let l = layer_mut(doc, id)?;
            let LayerKind::Fill(f) = &mut l.kind else {
                return Err(EditorError::Invalid(format!("`{}` is not a fill layer", l.name)));
            };
            *f = fill;
            Ok(Applied::step("Edit Fill").merge(format!("fill:{id}")))
        }
        Cmd::AddText { data, width, height, x, y, above } => {
            check_rgba(bytes, width, height)?;
            let id = doc.alloc_id();
            let name: String = data.text.lines().next().unwrap_or("Text").chars().take(40).collect();
            let raster = Raster::new(Plane::from_raw(width, height, 4, bytes, [0; 4]), x, y);
            let l = Layer::new(id, if name.trim().is_empty() { "Text".into() } else { name }, LayerKind::Text { data, raster });
            add_layer(doc, l, above);
            Ok(Applied::step("Add Text").with_data(serde_json::json!({ "id": id })))
        }
        Cmd::SetText { id, data, width, height, x, y } => {
            check_rgba(bytes, width, height)?;
            let l = layer_mut(doc, id)?;
            let LayerKind::Text { data: d, raster } = &mut l.kind else {
                return Err(EditorError::Invalid(format!("`{}` is not a text layer", l.name)));
            };
            let new_name: String = data.text.lines().next().unwrap_or("").chars().take(40).collect();
            *d = data;
            *raster = Raster::new(Plane::from_raw(width, height, 4, bytes, [0; 4]), x, y);
            if !new_name.trim().is_empty() {
                l.name = new_name;
            }
            Ok(Applied::step("Edit Text").merge(format!("text:{id}")))
        }
        Cmd::AddShape { data, name, above } => {
            let id = doc.alloc_id();
            let raster = rasterize_shape(&data);
            let label = match data.kind {
                crate::layer::ShapeKind::Arrow => "Arrow",
                crate::layer::ShapeKind::Line => "Line",
                crate::layer::ShapeKind::Ellipse => "Ellipse",
                crate::layer::ShapeKind::Redact => "Redaction",
                crate::layer::ShapeKind::Polyline => "Drawing",
                crate::layer::ShapeKind::Polygon => "Polygon",
                crate::layer::ShapeKind::Rect => "Rectangle",
            };
            let l = Layer::new(id, name.unwrap_or_else(|| default_name(doc, label)), LayerKind::Shape { data, raster });
            add_layer(doc, l, above);
            Ok(Applied::step(format!("Add {label}")).with_data(serde_json::json!({ "id": id })))
        }
        Cmd::SetShape { id, data } => {
            let l = layer_mut(doc, id)?;
            let LayerKind::Shape { data: d, raster } = &mut l.kind else {
                return Err(EditorError::Invalid(format!("`{}` is not a shape layer", l.name)));
            };
            *raster = rasterize_shape(&data);
            *d = data;
            Ok(Applied::step("Edit Shape").merge(format!("shape:{id}")))
        }
        Cmd::Group { ids, name } => {
            if ids.is_empty() {
                return Err(EditorError::Invalid("nothing to group".into()));
            }
            // Group lands where the topmost member was, among its siblings.
            let slots: Vec<_> = ids.iter().map(|&id| doc.slot_of(id).ok_or(EditorError::NoLayer(id))).collect::<Result<_>>()?;
            let parent = slots[0].parent;
            if slots.iter().any(|s| s.parent != parent) {
                return Err(EditorError::Invalid("layers to group must share a parent".into()));
            }
            let mut ordered: Vec<(usize, LayerId)> = slots.iter().map(|s| s.index).zip(ids.iter().copied()).collect();
            ordered.sort();
            let top_index = ordered.last().unwrap().0;
            let mut children = Vec::new();
            for &(_, id) in &ordered {
                children.push(doc.remove(id).unwrap());
            }
            let insert_at = top_index + 1 - children.len();
            let gid = doc.alloc_id();
            let g = Layer::new(gid, name.unwrap_or_else(|| default_name(doc, "Group")), LayerKind::Group { children, pass_through: true, expanded: true });
            doc.insert(parent, insert_at, g);
            doc.active = Some(gid);
            Ok(Applied::step("Group Layers").with_data(serde_json::json!({ "id": gid })))
        }
        Cmd::Ungroup { id } => {
            let slot = doc.slot_of(id).ok_or(EditorError::NoLayer(id))?;
            let g = doc.remove(id).unwrap();
            let LayerKind::Group { children, .. } = g.kind else {
                doc.insert(slot.parent, slot.index, g);
                return Err(EditorError::Invalid("not a group".into()));
            };
            let first = children.first().map(|c| c.id);
            for (k, c) in children.into_iter().enumerate() {
                doc.insert(slot.parent, slot.index + k, c);
            }
            doc.active = first;
            Ok(Applied::step("Ungroup Layers"))
        }
        Cmd::Delete { ids } => {
            let mut n = 0;
            for id in ids {
                if doc.remove(id).is_some() {
                    n += 1;
                }
            }
            if n == 0 {
                return Err(EditorError::Invalid("no layers deleted".into()));
            }
            if doc.active.is_none() {
                doc.active = doc.layers.last().map(|l| l.id);
            }
            Ok(Applied::step(if n == 1 { "Delete Layer".to_string() } else { format!("Delete {n} Layers") }))
        }
        Cmd::Duplicate { ids } => {
            let mut last = None;
            for id in ids {
                let src = layer(doc, id)?.clone();
                let mut copy = src;
                reassign_ids(doc, &mut copy);
                copy.name = format!("{} copy", copy.name);
                copy.touch();
                let new_id = copy.id;
                doc.insert_above(Some(id), copy);
                last = Some(new_id);
            }
            doc.active = last;
            Ok(Applied::step("Duplicate Layer").with_data(serde_json::json!({ "id": last })))
        }
        Cmd::Move { id, parent, index } => {
            if let Some(p) = parent {
                if p == id || doc.is_ancestor(id, p) {
                    return Err(EditorError::Invalid("cannot move a group into itself".into()));
                }
                if !layer(doc, p)?.is_group() {
                    return Err(EditorError::Invalid("target is not a group".into()));
                }
            }
            let slot = doc.slot_of(id).ok_or(EditorError::NoLayer(id))?;
            let l = doc.remove(id).unwrap();
            let mut index = index;
            if slot.parent == parent && slot.index < index {
                index -= 1;
            }
            doc.insert(parent, index, l);
            touch_parents(doc, id);
            doc.active = Some(id);
            Ok(Applied::step("Move Layer"))
        }
        Cmd::Reorder { id, direction } => {
            let slot = doc.slot_of(id).ok_or(EditorError::NoLayer(id))?;
            let len = doc.siblings(slot.parent).map_or(0, |s| s.len());
            let target = match direction {
                Direction::Up => (slot.index + 1).min(len - 1),
                Direction::Down => slot.index.saturating_sub(1),
                Direction::Top => len - 1,
                Direction::Bottom => 0,
            };
            if target == slot.index {
                return Ok(Applied::quiet());
            }
            let l = doc.remove(id).unwrap();
            doc.insert(slot.parent, target, l);
            touch_parents(doc, id);
            doc.active = Some(id);
            Ok(Applied::step("Arrange Layer"))
        }
        Cmd::Props { id, name, visible, opacity, fill_opacity, blend, clip, locks, color_label, pass_through, expanded } => {
            let l = layer_mut(doc, id)?;
            let mut label = "Layer Properties";
            let mut merge = None;
            let renamed = name.is_some();
            if let Some(v) = name {
                l.name = v;
                label = "Rename Layer";
            }
            if let Some(v) = visible {
                l.visible = v;
                label = if v { "Show Layer" } else { "Hide Layer" };
            }
            if let Some(v) = opacity {
                l.opacity = v.clamp(0.0, 1.0);
                label = "Opacity";
                merge = Some(format!("opacity:{id}"));
            }
            if let Some(v) = fill_opacity {
                l.fill_opacity = v.clamp(0.0, 1.0);
                label = "Fill Opacity";
                merge = Some(format!("fill-opacity:{id}"));
            }
            if let Some(v) = blend {
                l.blend = v;
                label = "Blend Mode";
            }
            if let Some(v) = clip {
                l.clip = v;
                label = if v { "Create Clipping Mask" } else { "Release Clipping Mask" };
            }
            if let Some(v) = locks {
                l.locks = v;
                label = "Lock Layer";
            }
            if let Some(v) = color_label {
                l.color_label = v.min(7);
            }
            if let LayerKind::Group { pass_through: p, expanded: e, .. } = &mut l.kind {
                if let Some(v) = pass_through {
                    *p = v;
                    label = "Blend Mode";
                }
                if let Some(v) = expanded {
                    *e = v;
                    // Folding a group in the panel is not an edit.
                    if !renamed && visible.is_none() && opacity.is_none() && blend.is_none() {
                        return Ok(Applied::quiet());
                    }
                }
            }
            let mut a = Applied::step(label);
            a.merge_key = merge;
            Ok(a)
        }
        Cmd::Offset { ids, dx, dy } => {
            let mut moved = false;
            for id in &ids {
                let l = layer_mut(doc, *id)?;
                if l.locks.all || l.locks.position {
                    return Err(EditorError::Locked(l.name.clone()));
                }
                moved |= offset_layer(l, dx, dy);
            }
            if !moved {
                return Ok(Applied::quiet());
            }
            let key = format!("offset:{ids:?}");
            Ok(Applied::step("Move").merge(key))
        }
        Cmd::SetActive { id } => {
            if let Some(id) = id {
                layer(doc, id)?;
            }
            doc.active = id;
            Ok(Applied::quiet())
        }
        Cmd::MergeDown { id } => {
            let slot = doc.slot_of(id).ok_or(EditorError::NoLayer(id))?;
            if slot.index == 0 {
                return Err(EditorError::Invalid("there is no layer below to merge into".into()));
            }
            let siblings = doc.siblings(slot.parent).unwrap();
            let pair = vec![siblings[slot.index - 1].clone(), siblings[slot.index].clone()];
            let below_name = pair[0].name.clone();
            let below_id = pair[0].id;
            let merged = merged_raster(doc, &pair);
            doc.remove(id);
            let l = layer_mut(doc, below_id)?;
            l.kind = LayerKind::Pixel(merged);
            l.mask = None;
            l.blend = BlendMode::Normal;
            l.opacity = 1.0;
            l.fill_opacity = 1.0;
            l.clip = false;
            l.name = below_name;
            doc.active = Some(below_id);
            Ok(Applied::step("Merge Down"))
        }
        Cmd::MergeVisible => {
            let visible: Vec<Layer> = doc.layers.iter().filter(|l| l.visible).cloned().collect();
            if visible.is_empty() {
                return Err(EditorError::Invalid("no visible layers".into()));
            }
            let raster = merged_raster(doc, &visible);
            let keep_id = visible[0].id;
            doc.layers.retain(|l| !l.visible || l.id == keep_id);
            let l = layer_mut(doc, keep_id)?;
            l.kind = LayerKind::Pixel(raster);
            l.mask = None;
            l.blend = BlendMode::Normal;
            l.opacity = 1.0;
            l.clip = false;
            doc.active = Some(keep_id);
            Ok(Applied::step("Merge Visible"))
        }
        Cmd::Flatten => {
            let layers: Vec<Layer> = doc.layers.clone();
            let mut raster = merged_raster(doc, &layers);
            // Flattening onto nothing gives a transparent background; keep
            // it (unlike Photoshop, which forces white) so cut-outs survive.
            raster.plane.compact();
            doc.layers.clear();
            let id = doc.alloc_id();
            doc.layers.push(Layer::new(id, "Background", LayerKind::Pixel(raster)));
            doc.active = Some(id);
            Ok(Applied::step("Flatten Image"))
        }
        Cmd::StampVisible => {
            let layers: Vec<Layer> = doc.layers.clone();
            let raster = merged_raster(doc, &layers);
            let id = doc.alloc_id();
            let l = Layer::new(id, default_name(doc, "Stamp"), LayerKind::Pixel(raster));
            doc.layers.push(l);
            doc.active = Some(id);
            Ok(Applied::step("Stamp Visible").with_data(serde_json::json!({ "id": id })))
        }
        Cmd::Rasterize { id } => {
            let l = layer(doc, id)?.clone();
            let raster = match &l.kind {
                LayerKind::Text { raster, .. } | LayerKind::Shape { raster, .. } | LayerKind::Smart { raster, .. } => raster.clone(),
                LayerKind::Fill(_) | LayerKind::Group { .. } => {
                    let mut solo = l.clone();
                    solo.mask = None;
                    solo.opacity = 1.0;
                    solo.blend = BlendMode::Normal;
                    merged_raster(doc, &[solo])
                }
                _ => return Err(EditorError::Invalid(format!("`{}` cannot be rasterized", l.name))),
            };
            let m = layer_mut(doc, id)?;
            m.kind = LayerKind::Pixel(raster);
            Ok(Applied::step("Rasterize Layer"))
        }
        Cmd::SetEffects { id, effects } => {
            let l = layer_mut(doc, id)?;
            if l.is_group() && l.children().is_some() {
                if let LayerKind::Group { pass_through: true, .. } = l.kind {
                    // Photoshop renders a styled group in isolation.
                    if let LayerKind::Group { pass_through, .. } = &mut l.kind {
                        *pass_through = effects.is_none();
                    }
                }
            }
            let label = if effects.is_some() { "Layer Style" } else { "Clear Layer Style" };
            l.effects = effects;
            Ok(Applied::step(label).merge(format!("effects:{id}")))
        }
        Cmd::AddMask { id, from } => {
            let id = target_id(doc, id)?;
            let (w, h) = (doc.width, doc.height);
            let mask = match from {
                MaskFrom::RevealAll => LayerMask::reveal_all(w, h),
                MaskFrom::HideAll => LayerMask::hide_all(w, h),
                MaskFrom::Selection | MaskFrom::HideSelection => {
                    let Some(sel) = doc.selection.clone() else {
                        return Err(EditorError::Invalid("there is no selection".into()));
                    };
                    let mut m = LayerMask { raster: Raster::new(sel, 0, 0), enabled: true, linked: true, density: 1.0 };
                    if matches!(from, MaskFrom::HideSelection) {
                        let p = &m.raster.plane;
                        let mut inv = Plane::mask(w, h, 255 - p.fill()[0]);
                        for r in p.present_rects() {
                            let d: Vec<u8> = p.read_vec(r).iter().map(|v| 255 - v).collect();
                            inv.write(r, &d);
                        }
                        inv.compact();
                        m.raster.plane = inv;
                    }
                    m
                }
            };
            let l = layer_mut(doc, id)?;
            if l.mask.is_some() {
                return Err(EditorError::Invalid(format!("`{}` already has a mask", l.name)));
            }
            l.mask = Some(mask);
            Ok(Applied::step("Add Layer Mask"))
        }
        Cmd::DeleteMask { id, apply } => {
            let has_pixels = matches!(layer(doc, id)?.kind, LayerKind::Pixel(_));
            let l = layer_mut(doc, id)?;
            let Some(mask) = l.mask.take() else {
                return Err(EditorError::Invalid("layer has no mask".into()));
            };
            if apply && has_pixels {
                if let LayerKind::Pixel(r) = &mut l.kind {
                    let (rx, ry) = (r.x, r.y);
                    let rects = r.plane.present_rects();
                    for rect in rects {
                        let mut data = r.plane.read_vec(rect);
                        for (k, px) in data.chunks_exact_mut(4).enumerate() {
                            let dx = rx + rect.x + (k as i32 % rect.w);
                            let dy = ry + rect.y + (k as i32 / rect.w);
                            let m = mask.raster.plane.get(dx - mask.raster.x, dy - mask.raster.y)[0] as u32;
                            px[3] = (px[3] as u32 * m / 255) as u8;
                        }
                        r.plane.write(rect, &data);
                    }
                    r.plane.compact();
                }
                return Ok(Applied::step("Apply Layer Mask"));
            }
            Ok(Applied::step("Delete Layer Mask"))
        }
        Cmd::MaskProps { id, enabled, linked, density } => {
            let l = layer_mut(doc, id)?;
            let Some(m) = l.mask.as_mut() else {
                return Err(EditorError::Invalid("layer has no mask".into()));
            };
            if let Some(v) = enabled {
                m.enabled = v;
            }
            if let Some(v) = linked {
                m.linked = v;
            }
            let mut a = Applied::step(if enabled == Some(false) { "Disable Layer Mask" } else { "Layer Mask Properties" });
            if let Some(v) = density {
                m.density = v.clamp(0.0, 1.0);
                a = a.merge(format!("mask-density:{id}"));
            }
            Ok(a)
        }
        Cmd::SetMaskPixels { id, x, y, width, height } => {
            let want = width as usize * height as usize;
            if bytes.len() != want {
                return Err(EditorError::Invalid(format!("expected {want} mask bytes, got {}", bytes.len())));
            }
            let l = layer_mut(doc, id)?;
            let Some(m) = l.mask.as_mut() else {
                return Err(EditorError::Invalid("layer has no mask".into()));
            };
            let rect = Rect::new(x, y, width as i32, height as i32);
            m.raster.ensure_covers(rect);
            let local = rect.translate(-m.raster.x, -m.raster.y);
            m.raster.plane.write(local, bytes);
            m.raster.plane.compact();
            Ok(Applied::step("Edit Layer Mask").dirty(rect))
        }
        Cmd::PlaceSmart { width, height, x, y, name, above } => {
            check_rgba(bytes, width, height)?;
            let id = doc.alloc_id();
            let plane = Plane::from_raw(width, height, 4, bytes, [0; 4]);
            let quad = crate::smart::rect_quad(Rect::new(x, y, width as i32, height as i32));
            let raster = Raster::new(plane.clone(), x, y);
            let l = Layer::new(
                id,
                name.unwrap_or_else(|| default_name(doc, "Smart Object")),
                LayerKind::Smart { source: SmartSource::Pixels(plane), quad, filters: Vec::new(), raster, stale: false },
            );
            add_layer(doc, l, above);
            Ok(Applied::step("Place Embedded").with_data(serde_json::json!({ "id": id })))
        }
        Cmd::ConvertToSmart { ids, name } => {
            if ids.is_empty() {
                return Err(EditorError::Invalid("nothing to convert".into()));
            }
            let slots: Vec<_> = ids.iter().map(|&id| doc.slot_of(id).ok_or(EditorError::NoLayer(id))).collect::<Result<_>>()?;
            let parent = slots[0].parent;
            if slots.iter().any(|s| s.parent != parent) {
                return Err(EditorError::Invalid("layers to convert must share a parent".into()));
            }
            let mut ordered: Vec<(usize, LayerId)> = slots.iter().map(|s| s.index).zip(ids.iter().copied()).collect();
            ordered.sort();
            let top_index = ordered.last().unwrap().0;
            let mut members = Vec::new();
            for &(_, id) in &ordered {
                members.push(doc.remove(id).unwrap());
            }
            let insert_at = top_index + 1 - members.len();
            let (w, h) = (doc.width, doc.height);
            let single_plain = members.len() == 1
                && matches!(members[0].kind, LayerKind::Pixel(_))
                && members[0].mask.is_none()
                && members[0].effects.is_none();
            let sid = doc.alloc_id();
            let mut so = if single_plain {
                let m = &members[0];
                let r = m.raster().unwrap();
                let b = r.plane.content_bounds();
                let b = if b.is_empty() { Rect::new(0, 0, 1, 1) } else { b };
                let plane = r.plane.extract(b);
                let rect = b.translate(r.x, r.y);
                let mut l = Layer::new(
                    sid,
                    name.clone().unwrap_or_else(|| m.name.clone()),
                    LayerKind::Smart { source: SmartSource::Pixels(plane), quad: crate::smart::rect_quad(rect), filters: Vec::new(), raster: Raster::empty(1, 1), stale: true },
                );
                l.opacity = m.opacity;
                l.blend = m.blend;
                l.visible = m.visible;
                l
            } else {
                let mut nested = Document::new(w, h);
                nested.resolution = doc.resolution;
                nested.layers = members;
                nested.reserve_ids();
                nested.active = nested.layers.last().map(|l| l.id);
                Layer::new(
                    sid,
                    name.unwrap_or_else(|| default_name(doc, "Smart Object")),
                    LayerKind::Smart { source: SmartSource::Document(Box::new(nested)), quad: crate::smart::rect_quad(Rect::new(0, 0, w as i32, h as i32)), filters: Vec::new(), raster: Raster::empty(1, 1), stale: true },
                )
            };
            so.touch();
            doc.insert(parent, insert_at, so);
            doc.active = Some(sid);
            Ok(Applied::step("Convert to Smart Object").with_data(serde_json::json!({ "id": sid })))
        }
        Cmd::SmartTransform { id, quad: q } => {
            let l = smart_layer(doc, id)?;
            if l.locks.all || l.locks.position {
                return Err(EditorError::Locked(l.name.clone()));
            }
            if let LayerKind::Smart { quad, stale, .. } = &mut l.kind {
                *quad = q;
                *stale = true;
            }
            Ok(Applied::step("Free Transform").merge(format!("smart-transform:{id}")))
        }
        Cmd::SmartFilterAdd { id, filter, opacity, blend } => {
            if filter.get("op").and_then(|v| v.as_str()).is_none_or(|op| !op.starts_with("filter.")) {
                return Err(EditorError::Invalid("a smart filter must be a filter.* command".into()));
            }
            let l = smart_layer(doc, id)?;
            let label = filter["op"].as_str().unwrap_or("").trim_start_matches("filter.").replace('-', " ");
            if let LayerKind::Smart { filters, stale, .. } = &mut l.kind {
                filters.push(SmartFilter { filter, enabled: true, opacity, blend });
                *stale = true;
            }
            Ok(Applied::step(format!("Smart Filter: {label}")))
        }
        Cmd::SmartFilterSet { id, index, filter, enabled, opacity, blend } => {
            let l = smart_layer(doc, id)?;
            let LayerKind::Smart { filters, stale, .. } = &mut l.kind else { unreachable!() };
            let f = filters.get_mut(index).ok_or_else(|| EditorError::Invalid(format!("no smart filter {index}")))?;
            if let Some(v) = filter {
                f.filter = v;
            }
            if let Some(v) = enabled {
                f.enabled = v;
            }
            if let Some(v) = opacity {
                f.opacity = v.clamp(0.0, 1.0);
            }
            if let Some(v) = blend {
                f.blend = v;
            }
            *stale = true;
            Ok(Applied::step("Edit Smart Filter").merge(format!("smart-filter:{id}:{index}")))
        }
        Cmd::SmartFilterRemove { id, index } => {
            let l = smart_layer(doc, id)?;
            let LayerKind::Smart { filters, stale, .. } = &mut l.kind else { unreachable!() };
            if index >= filters.len() {
                return Err(EditorError::Invalid(format!("no smart filter {index}")));
            }
            filters.remove(index);
            *stale = true;
            Ok(Applied::step("Delete Smart Filter"))
        }
        Cmd::SmartFilterMove { id, from, to } => {
            let l = smart_layer(doc, id)?;
            let LayerKind::Smart { filters, stale, .. } = &mut l.kind else { unreachable!() };
            if from >= filters.len() || to >= filters.len() {
                return Err(EditorError::Invalid("smart filter index out of range".into()));
            }
            let f = filters.remove(from);
            filters.insert(to, f);
            *stale = true;
            Ok(Applied::step("Move Smart Filter"))
        }
        Cmd::SmartReplace { id, width, height } => {
            check_rgba(bytes, width, height)?;
            let l = smart_layer(doc, id)?;
            let LayerKind::Smart { source, stale, .. } = &mut l.kind else { unreachable!() };
            *source = SmartSource::Pixels(Plane::from_raw(width, height, 4, bytes, [0; 4]));
            *stale = true;
            Ok(Applied::step("Replace Contents"))
        }
        Cmd::SmartUnpack { id } => {
            let slot = doc.slot_of(id).ok_or(EditorError::NoLayer(id))?;
            let l = layer(doc, id)?.clone();
            let LayerKind::Smart { source, quad, filters, raster, .. } = l.kind else {
                return Err(EditorError::Invalid(format!("`{}` is not a smart object", l.name)));
            };
            let untouched = filters.iter().all(|f| !f.enabled) && {
                let (w, h) = source.size();
                let q0 = crate::smart::rect_quad(Rect::new(quad[0].x.round() as i32, quad[0].y.round() as i32, w as i32, h as i32));
                quad.iter().zip(q0.iter()).all(|(a, b)| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6)
            };
            doc.remove(id);
            match source {
                SmartSource::Document(nested) if untouched && quad[0].x == 0.0 && quad[0].y == 0.0 => {
                    let mut nested = *nested;
                    let mut restored = Vec::new();
                    for mut child in nested.layers.drain(..) {
                        reassign_ids(doc, &mut child);
                        restored.push(child);
                    }
                    let first = restored.last().map(|c| c.id);
                    for (k, c) in restored.into_iter().enumerate() {
                        doc.insert(slot.parent, slot.index + k, c);
                    }
                    doc.active = first;
                }
                _ => {
                    // Transformed or filtered: keep what it looks like.
                    let mut p = Layer::new(l.id, l.name.clone(), LayerKind::Pixel(raster));
                    p.opacity = l.opacity;
                    p.blend = l.blend;
                    p.mask = l.mask.clone();
                    p.effects = l.effects.clone();
                    doc.insert(slot.parent, slot.index, p);
                    doc.active = Some(l.id);
                }
            }
            Ok(Applied::step("Convert to Layers"))
        }
        Cmd::FromSelection { id, cut } => {
            let id = target_id(doc, id)?;
            let src = layer(doc, id)?.clone();
            let Some(r) = src.raster().cloned() else {
                return Err(EditorError::Invalid(format!("`{}` has no pixels to copy", src.name)));
            };
            let sel_bounds = selection::bounds(doc).intersect(&r.doc_rect());
            if sel_bounds.is_empty() {
                return Err(EditorError::Invalid("the selection does not overlap this layer".into()));
            }
            let mut data = r.plane.read_vec(sel_bounds.translate(-r.x, -r.y));
            for (k, px) in data.chunks_exact_mut(4).enumerate() {
                let dx = sel_bounds.x + (k as i32 % sel_bounds.w);
                let dy = sel_bounds.y + (k as i32 / sel_bounds.w);
                px[3] = (px[3] as f32 * selection::coverage(doc, dx, dy)).round() as u8;
            }
            if cut {
                clear_selected(doc, id)?;
            }
            let new_id = doc.alloc_id();
            let plane = Plane::from_raw(sel_bounds.w as u32, sel_bounds.h as u32, 4, &data, [0; 4]);
            let mut l = Layer::new(new_id, default_name(doc, "Layer"), LayerKind::Pixel(Raster::new(plane, sel_bounds.x, sel_bounds.y)));
            l.provenance = src.provenance.clone();
            doc.insert_above(Some(id), l);
            doc.active = Some(new_id);
            Ok(Applied::step(if cut { "Layer Via Cut" } else { "Layer Via Copy" }).with_data(serde_json::json!({ "id": new_id })))
        }
        Cmd::Clear { id } => {
            let id = target_id(doc, id)?;
            let rect = clear_selected(doc, id)?;
            Ok(Applied::step("Clear").dirty(rect))
        }
        Cmd::FillSelection { id, color, opacity } => {
            let id = target_id(doc, id)?;
            let sel = doc.selection.clone();
            let (dw, dh) = (doc.width, doc.height);
            let bounds = selection::bounds(doc);
            let l = editable_raster(doc, id)?;
            let preserve = l.locks.transparency;
            let LayerKind::Pixel(r) = &mut l.kind else { unreachable!() };
            let area = bounds.intersect(&Rect::new(0, 0, dw as i32, dh as i32));
            r.ensure_covers(area);
            let (rx, ry) = (r.x, r.y);
            let c = color.to_array();
            r.plane.modify(area.translate(-rx, -ry), |x, y, px| {
                let cov = sel.as_ref().map_or(1.0, |s| s.get(x + rx, y + ry)[0] as f32 / 255.0) * opacity * c[3] as f32 / 255.0;
                if cov <= 0.0 {
                    return;
                }
                let mut d = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0, px[3] as f32 / 255.0];
                let a = d[3];
                crate::blend::composite_px(&mut d, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0], cov, BlendMode::Normal);
                if preserve {
                    d[3] = a;
                }
                for k in 0..4 {
                    px[k] = (d[k] * 255.0 + 0.5) as u8;
                }
            });
            r.plane.compact();
            Ok(Applied::step("Fill").dirty(area))
        }
    }
}

fn smart_layer(doc: &mut Document, id: LayerId) -> Result<&mut Layer> {
    let l = layer_mut(doc, id)?;
    if !matches!(l.kind, LayerKind::Smart { .. }) {
        return Err(EditorError::Invalid(format!("`{}` is not a smart object", l.name)));
    }
    Ok(l)
}

fn touch_parents(doc: &mut Document, id: LayerId) {
    // find_mut bumps every ancestor on the way down.
    let _ = doc.find_mut(id);
}

fn reassign_ids(doc: &mut Document, l: &mut Layer) {
    l.id = doc.alloc_id();
    if let LayerKind::Group { children, .. } = &mut l.kind {
        for c in children {
            reassign_ids(doc, c);
        }
    }
}

/// Move a layer's pixels (and its linked mask). Adjustment and fill layers
/// have no position, but their masks do.
pub fn offset_layer(l: &mut Layer, dx: i32, dy: i32) -> bool {
    if dx == 0 && dy == 0 {
        return false;
    }
    let mask_follows = l.mask.as_ref().is_none_or(|m| m.linked);
    match &mut l.kind {
        LayerKind::Pixel(r) => {
            r.x += dx;
            r.y += dy;
        }
        LayerKind::Text { raster, data } => {
            raster.x += dx;
            raster.y += dy;
            data.x += dx as f32;
            data.y += dy as f32;
        }
        LayerKind::Shape { raster, data } => {
            raster.x += dx;
            raster.y += dy;
            for p in &mut data.points {
                p.x += dx as f64;
                p.y += dy as f64;
            }
        }
        LayerKind::Smart { raster, quad, .. } => {
            raster.x += dx;
            raster.y += dy;
            for p in quad.iter_mut() {
                p.x += dx as f64;
                p.y += dy as f64;
            }
        }
        LayerKind::Group { children, .. } => {
            for c in children {
                offset_layer(c, dx, dy);
            }
        }
        LayerKind::Fill(crate::layer::Fill::Gradient { from, to, .. }) => {
            from.x += dx as f64;
            from.y += dy as f64;
            to.x += dx as f64;
            to.y += dy as f64;
        }
        _ => {}
    }
    if mask_follows {
        if let Some(m) = &mut l.mask {
            m.raster.x += dx;
            m.raster.y += dy;
        }
    }
    l.touch();
    true
}

/// Composite some layers into a single canvas-sized raster.
pub fn merged_raster(doc: &Document, layers: &[Layer]) -> Raster {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let mut plane = Plane::transparent(doc.width, doc.height);
    let strip = (4_000_000 / w.max(1)).clamp(1, 1024);
    let mut y = 0;
    while y < h {
        let rows = strip.min(h - y);
        let view = View { x: 0.0, y: y as f64, scale: 1.0, width: w, height: rows };
        let f = render_layers(layers, doc, view);
        let mut out = vec![0u8; w * rows * 4];
        to_u8(&f, &mut out);
        plane.write(Rect::new(0, y as i32, w as i32, rows as i32), &out);
        y += rows;
    }
    plane.compact();
    Raster::new(plane, 0, 0)
}

fn clear_selected(doc: &mut Document, id: LayerId) -> Result<Rect> {
    let sel = doc.selection.clone();
    let bounds = selection::bounds(doc);
    let l = editable_raster(doc, id)?;
    if l.locks.transparency {
        return Err(EditorError::Locked(l.name.clone()));
    }
    let LayerKind::Pixel(r) = &mut l.kind else { unreachable!() };
    let (rx, ry) = (r.x, r.y);
    let area = bounds.translate(-rx, -ry);
    match &sel {
        None => r.plane.clear(),
        Some(s) => r.plane.modify(area, |x, y, px| {
            let cov = s.get(x + rx, y + ry)[0] as u32;
            px[3] = (px[3] as u32 * (255 - cov) / 255) as u8;
        }),
    }
    r.plane.compact();
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::flatten;
    use serde_json::json;

    fn exec(doc: &mut Document, v: serde_json::Value, bytes: &[u8]) -> Applied {
        apply(v, doc, bytes).unwrap()
    }

    #[test]
    fn open_add_adjust_merge() {
        let mut doc = Document::new(1, 1);
        let px: Vec<u8> = (0..4 * 4).flat_map(|_| [100u8, 100, 100, 255]).collect();
        exec(&mut doc, json!({"op": "doc.open-pixels", "width": 4, "height": 4}), &px);
        let a = exec(&mut doc, json!({"op": "layer.add-adjustment", "adjustment": {"kind": "invert"}}), &[]);
        let id = a.data.unwrap()["id"].as_u64().unwrap() as LayerId;
        assert_eq!(flatten(&doc)[0], 155);
        exec(&mut doc, json!({"op": "layer.props", "id": id, "opacity": 0.0}), &[]);
        assert_eq!(flatten(&doc)[0], 100);
        exec(&mut doc, json!({"op": "layer.props", "id": id, "opacity": 1.0}), &[]);
        exec(&mut doc, json!({"op": "layer.merge-down", "id": id}), &[]);
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(flatten(&doc)[0], 155);
    }

    #[test]
    fn group_ungroup_preserves_order() {
        let mut doc = Document::new(8, 8);
        exec(&mut doc, json!({"op": "doc.new", "width": 8, "height": 8}), &[]);
        let a = exec(&mut doc, json!({"op": "layer.add-pixel", "name": "a"}), &[]).data.unwrap()["id"].as_u64().unwrap() as u32;
        let b = exec(&mut doc, json!({"op": "layer.add-pixel", "name": "b"}), &[]).data.unwrap()["id"].as_u64().unwrap() as u32;
        let g = exec(&mut doc, json!({"op": "layer.group", "ids": [a, b]}), &[]).data.unwrap()["id"].as_u64().unwrap() as u32;
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.find(g).unwrap().children().unwrap().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        exec(&mut doc, json!({"op": "layer.ungroup", "id": g}), &[]);
        assert_eq!(doc.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["Layer 1", "a", "b"]);
    }

    #[test]
    fn set_pixels_respects_selection() {
        let mut doc = Document::new(1, 1);
        let px = vec![0u8; 4 * 4 * 4];
        exec(&mut doc, json!({"op": "doc.open-pixels", "width": 4, "height": 4}), &px);
        let mut sel = Plane::mask(4, 4, 0);
        sel.write(Rect::new(0, 0, 2, 4), &[255; 8]);
        doc.selection = Some(sel);
        let white = vec![255u8; 4 * 4 * 4];
        exec(&mut doc, json!({"op": "layer.set-pixels", "x": 0, "y": 0, "width": 4, "height": 4, "respect_selection": true}), &white);
        let out = flatten(&doc);
        assert_eq!(out[0], 255);
        assert_eq!(out[3 * 4], 0);
    }

    #[test]
    fn layer_via_cut_moves_selected_pixels() {
        let mut doc = Document::new(1, 1);
        let px: Vec<u8> = (0..4 * 4).flat_map(|_| [10u8, 20, 30, 255]).collect();
        exec(&mut doc, json!({"op": "doc.open-pixels", "width": 4, "height": 4}), &px);
        let mut sel = Plane::mask(4, 4, 0);
        sel.write(Rect::new(1, 1, 2, 2), &[255; 4]);
        doc.selection = Some(sel);
        exec(&mut doc, json!({"op": "layer.from-selection", "cut": true}), &[]);
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.layers[0].raster().unwrap().plane.get(1, 1)[3], 0);
        let top = doc.layers[1].raster().unwrap();
        assert_eq!((top.x, top.y, top.plane.width()), (1, 1, 2));
        assert_eq!(flatten(&doc)[(1 * 4 + 1) * 4], 10);
    }

    #[test]
    fn offset_moves_content_and_mask() {
        let mut doc = Document::new(1, 1);
        let px: Vec<u8> = (0..4 * 4).flat_map(|_| [10u8, 20, 30, 255]).collect();
        exec(&mut doc, json!({"op": "doc.open-pixels", "width": 4, "height": 4}), &px);
        let id = doc.active.unwrap();
        exec(&mut doc, json!({"op": "layer.add-mask", "from": "reveal-all"}), &[]);
        exec(&mut doc, json!({"op": "layer.offset", "ids": [id], "dx": 2, "dy": 0}), &[]);
        let l = doc.find(id).unwrap();
        assert_eq!(l.raster().unwrap().x, 2);
        assert_eq!(l.mask.as_ref().unwrap().raster.x, 2);
        assert_eq!(flatten(&doc)[3], 0, "column 0 is now empty");
    }

    #[test]
    fn shape_layer_rasterizes() {
        let mut doc = Document::new(1, 1);
        exec(&mut doc, json!({"op": "doc.new", "width": 100, "height": 100, "background": {"r": 255, "g": 255, "b": 255}}), &[]);
        exec(&mut doc, json!({"op": "layer.add-shape", "data": {"kind": "redact", "points": [{"x": 10, "y": 10}, {"x": 50, "y": 50}]}}), &[]);
        let out = flatten(&doc);
        assert_eq!(&out[(30 * 100 + 30) * 4..(30 * 100 + 30) * 4 + 4], &[0, 0, 0, 255]);
    }
}
