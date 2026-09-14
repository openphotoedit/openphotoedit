//! Layers: pixel, adjustment, fill, group, text and shape.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::adjust::{Adjustment, GradientStop};
use crate::blend::BlendMode;
use crate::color::Rgba8;
use crate::geom::{Point, Rect};
use crate::plane::Plane;

pub type LayerId = u32;

static REVISION: AtomicU64 = AtomicU64::new(1);

/// A fresh, process-unique revision number. Every change to a layer takes
/// one, which is what the render cache compares.
pub fn next_rev() -> u64 {
    REVISION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Locks {
    pub transparency: bool,
    pub pixels: bool,
    pub position: bool,
    pub all: bool,
}

/// Pixels positioned in the document: `plane` pixel (0,0) sits at `(x, y)`.
#[derive(Clone, Debug)]
pub struct Raster {
    pub plane: Plane,
    pub x: i32,
    pub y: i32,
}

impl Raster {
    pub fn new(plane: Plane, x: i32, y: i32) -> Raster {
        Raster { plane, x, y }
    }
    pub fn empty(width: u32, height: u32) -> Raster {
        Raster { plane: Plane::transparent(width, height), x: 0, y: 0 }
    }
    /// The raster's extent in document coordinates.
    pub fn doc_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.plane.width() as i32, self.plane.height() as i32)
    }
    /// Grow the plane (never shrink) so that `doc_rect` fits, keeping existing
    /// pixels where they are in the document. Growth to the left or top is
    /// rounded to whole tiles so existing tiles can be re-indexed rather than
    /// rewritten.
    pub fn ensure_covers(&mut self, doc_rect: Rect) {
        let cur = self.doc_rect();
        let want = cur.union(&doc_rect);
        if want == cur {
            return;
        }
        let tile = crate::plane::TILE as i32;
        let grow_left = ((cur.x - want.x).max(0) + tile - 1) / tile * tile;
        let grow_top = ((cur.y - want.y).max(0) + tile - 1) / tile * tile;
        let new_x = cur.x - grow_left;
        let new_y = cur.y - grow_top;
        let new_w = (want.right() - new_x).max(cur.right() - new_x);
        let new_h = (want.bottom() - new_y).max(cur.bottom() - new_y);
        let mut plane = Plane::new(new_w as u32, new_h as u32, self.plane.channels(), self.plane.fill());
        plane.paste(&self.plane, grow_left, grow_top);
        self.plane = plane;
        self.x = new_x;
        self.y = new_y;
    }
}

#[derive(Clone, Debug)]
pub struct LayerMask {
    /// Single channel; 255 reveals, 0 hides. Pixels outside read as the
    /// plane's fill ("background colour" in PSD terms).
    pub raster: Raster,
    pub enabled: bool,
    /// Moves with the layer when linked.
    pub linked: bool,
    /// 0..1: how strongly the mask hides.
    pub density: f32,
}

impl LayerMask {
    pub fn reveal_all(doc_w: u32, doc_h: u32) -> LayerMask {
        LayerMask { raster: Raster::new(Plane::mask(doc_w, doc_h, 255), 0, 0), enabled: true, linked: true, density: 1.0 }
    }
    pub fn hide_all(doc_w: u32, doc_h: u32) -> LayerMask {
        LayerMask { raster: Raster::new(Plane::mask(doc_w, doc_h, 0), 0, 0), enabled: true, linked: true, density: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
    Angle,
    Reflected,
    Diamond,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Fill {
    Solid { color: Rgba8 },
    Gradient { stops: Vec<GradientStop>, gradient: GradientKind, from: Point, to: Point, #[serde(default)] reverse: bool },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A text layer's source. The browser shapes and rasterises it (it has the
/// fonts); the engine keeps the parameters so the layer stays editable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextData {
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub color: Rgba8,
    pub align: TextAlign,
    pub line_height: f32,
    pub letter_spacing: f32,
    /// Paragraph text wraps at this width; point text when absent.
    pub box_width: Option<f32>,
    /// Anchor (top-left of the text box) in document pixels.
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    /// Annotation style: a filled pill behind the text.
    pub background: Option<Rgba8>,
    pub padding: f32,
    pub stroke: Option<Rgba8>,
    pub stroke_width: f32,
}

impl Default for TextData {
    fn default() -> Self {
        TextData {
            text: String::new(),
            font_family: "Inter, system-ui, sans-serif".into(),
            font_size: 48.0,
            font_weight: 400,
            italic: false,
            color: Rgba8::BLACK,
            align: TextAlign::Left,
            line_height: 1.2,
            letter_spacing: 0.0,
            box_width: None,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            background: None,
            padding: 0.0,
            stroke: None,
            stroke_width: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShapeKind {
    #[default]
    Rect,
    Ellipse,
    Line,
    Arrow,
    Polygon,
    /// Freehand pen / highlighter: an open polyline.
    Polyline,
    /// A filled rectangle that covers content (redaction).
    Redact,
}

/// A vector shape. Rasterised by the engine with tiny-skia.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeData {
    pub kind: ShapeKind,
    /// Document coordinates. Rect/ellipse/redact use the first two as
    /// opposite corners; line and arrow use them as ends.
    pub points: Vec<Point>,
    pub stroke: Option<Rgba8>,
    pub stroke_width: f32,
    pub fill: Option<Rgba8>,
    pub corner_radius: f32,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub dash: Option<[f32; 2]>,
    pub rotation: f32,
}

impl Default for ShapeData {
    fn default() -> Self {
        ShapeData {
            kind: ShapeKind::Rect,
            points: Vec::new(),
            stroke: Some(Rgba8::rgb(255, 59, 48)),
            stroke_width: 4.0,
            fill: None,
            corner_radius: 0.0,
            arrow_start: false,
            arrow_end: true,
            dash: None,
            rotation: 0.0,
        }
    }
}

/// A filter kept live on a smart object: an engine command (`filter.*`)
/// re-run on the object's contents whenever they change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmartFilter {
    /// The command, e.g. `{"op": "filter.gaussian-blur", "radius": 4}`.
    pub filter: serde_json::Value,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "full")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
}

fn yes() -> bool {
    true
}

fn full() -> f32 {
    1.0
}

/// What a smart object contains.
#[derive(Clone, Debug)]
pub enum SmartSource {
    /// Placed pixels, kept at their original resolution.
    Pixels(Plane),
    /// Layers converted to a smart object: a whole nested document.
    Document(Box<crate::document::Document>),
}

impl SmartSource {
    pub fn size(&self) -> (u32, u32) {
        match self {
            SmartSource::Pixels(p) => (p.width(), p.height()),
            SmartSource::Document(d) => (d.width, d.height),
        }
    }
}

#[derive(Clone, Debug)]
pub enum LayerKind {
    Pixel(Raster),
    Adjustment(Adjustment),
    Fill(Fill),
    Group { children: Vec<Layer>, pass_through: bool, expanded: bool },
    Text { data: TextData, raster: Raster },
    Shape { data: ShapeData, raster: Raster },
    /// Non-destructive container: `source` is filtered by `filters`, then
    /// warped so its corners land on `quad` (TL, TR, BR, BL). `raster` caches
    /// the result; `stale` asks the editor to rebuild it.
    Smart { source: SmartSource, quad: [Point; 4], filters: Vec<SmartFilter>, raster: Raster, stale: bool },
}

impl LayerKind {
    pub fn name(&self) -> &'static str {
        match self {
            LayerKind::Pixel(_) => "pixel",
            LayerKind::Adjustment(_) => "adjustment",
            LayerKind::Fill(_) => "fill",
            LayerKind::Group { .. } => "group",
            LayerKind::Text { .. } => "text",
            LayerKind::Shape { .. } => "shape",
            LayerKind::Smart { .. } => "smart",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    /// 0..1
    pub opacity: f32,
    /// 0..1: affects content but not layer effects.
    pub fill_opacity: f32,
    pub blend: BlendMode,
    /// Clipped to the nearest unclipped layer below.
    pub clip: bool,
    pub locks: Locks,
    /// 0 = none, 1..7 = Photoshop's label colours.
    pub color_label: u8,
    pub mask: Option<LayerMask>,
    /// Layer style (drop shadow, stroke, …).
    pub effects: Option<crate::effects::LayerEffects>,
    pub kind: LayerKind,
    pub rev: u64,
    /// Where this layer came from, for Content Credentials: "ai:<model>" etc.
    pub provenance: Vec<String>,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>, kind: LayerKind) -> Layer {
        Layer {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            fill_opacity: 1.0,
            blend: BlendMode::Normal,
            clip: false,
            locks: Locks::default(),
            color_label: 0,
            mask: None,
            effects: None,
            kind,
            rev: next_rev(),
            provenance: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.rev = next_rev();
    }

    pub fn is_group(&self) -> bool {
        matches!(self.kind, LayerKind::Group { .. })
    }

    pub fn children(&self) -> Option<&Vec<Layer>> {
        match &self.kind {
            LayerKind::Group { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Layer>> {
        match &mut self.kind {
            LayerKind::Group { children, .. } => Some(children),
            _ => None,
        }
    }

    /// The raster holding this layer's pixels, if it has one.
    pub fn raster(&self) -> Option<&Raster> {
        match &self.kind {
            LayerKind::Pixel(r) => Some(r),
            LayerKind::Text { raster, .. } | LayerKind::Shape { raster, .. } | LayerKind::Smart { raster, .. } => Some(raster),
            _ => None,
        }
    }

    pub fn raster_mut(&mut self) -> Option<&mut Raster> {
        match &mut self.kind {
            LayerKind::Pixel(r) => Some(r),
            LayerKind::Text { raster, .. } | LayerKind::Shape { raster, .. } | LayerKind::Smart { raster, .. } => Some(raster),
            _ => None,
        }
    }

    /// Document-space bounds of visible content (pixel layers), or `None`
    /// for layers that cover the whole canvas (adjustment, fill).
    pub fn content_bounds(&self) -> Option<Rect> {
        match &self.kind {
            LayerKind::Group { children, .. } => {
                let mut r = Rect::empty();
                for c in children {
                    if let Some(b) = c.content_bounds() {
                        r = r.union(&b);
                    }
                }
                Some(r)
            }
            _ => self.raster().map(|r| r.plane.content_bounds().translate(r.x, r.y)),
        }
    }

    /// Unique tile bytes, for memory accounting.
    pub fn visit_planes<'a>(&'a self, f: &mut impl FnMut(&'a Plane)) {
        if let Some(r) = self.raster() {
            f(&r.plane);
        }
        if let Some(m) = &self.mask {
            f(&m.raster.plane);
        }
        if let Some(children) = self.children() {
            for c in children {
                c.visit_planes(f);
            }
        }
    }
}
