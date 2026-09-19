//! `paint.stroke` and `paint.stroke-end`.
//!
//! A stroke arrives as segments sharing a `stroke_id`. The engine keeps, per
//! stroke:
//! - a snapshot of the document from before the stroke (tiles are shared, so
//!   this is cheap), which is both the "original" the stroke composites over
//!   and the source clone/heal sample from;
//! - a coverage buffer over the canvas (sparse tiles) that dabs build up by
//!   flow toward the opacity cap, so overlapping dabs within one stroke never
//!   exceed the stroke's opacity (Photoshop's flow/opacity semantics);
//! - the path position, so dab spacing runs on across segment boundaries;
//! - tool state: the smudge pickup, the healing correction field.
//!
//! Each segment recomposites only the rectangle its dabs touched, from the
//! snapshot, the coverage and the current selection. Every segment returns
//! the same merge key, so history keeps the pre-stroke snapshot and the whole
//! stroke undoes as one step.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use editor_core::blend::{composite_px, BlendMode};
use editor_core::color::Rgba8;
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::LayerId;
use editor_core::ops::{target_id, Applied, EditorError, Result};
use editor_core::pixels::{read_merged, read_selection};
use serde::Deserialize;
use serde_json::Value;

use crate::brush::{Brush, Dab, PathState, Stamp, StrokePoint};
use crate::surface::{self, Target};
use crate::tone::{dodge_burn, membrane, Range, Sponge};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    #[default]
    Brush,
    Pencil,
    Eraser,
    Dodge,
    Burn,
    Sponge,
    Blur,
    Sharpen,
    Smudge,
    Clone,
    Heal,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Brush => "Brush Tool",
            Tool::Pencil => "Pencil Tool",
            Tool::Eraser => "Eraser",
            Tool::Dodge => "Dodge Tool",
            Tool::Burn => "Burn Tool",
            Tool::Sponge => "Sponge Tool",
            Tool::Blur => "Blur Tool",
            Tool::Sharpen => "Sharpen Tool",
            Tool::Smudge => "Smudge Tool",
            Tool::Clone => "Clone Stamp",
            Tool::Heal => "Healing Brush",
        }
    }
    /// Tools that act on the current pixels dab by dab instead of through
    /// the stroke's coverage buffer.
    fn direct(self) -> bool {
        matches!(self, Tool::Smudge)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Source {
    pub dx: f64,
    pub dy: f64,
    #[serde(default)]
    pub sample_all: bool,
}

fn half() -> f32 {
    0.5
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct StrokeCmd {
    #[serde(default)]
    pub id: Option<LayerId>,
    #[serde(default)]
    pub target: Target,
    #[serde(default)]
    pub tool: Tool,
    #[serde(default)]
    pub brush: Brush,
    pub points: Vec<StrokePoint>,
    #[serde(default)]
    pub stroke_id: Option<Value>,
    #[serde(default)]
    pub range: Range,
    #[serde(default = "half")]
    pub exposure: f32,
    #[serde(default)]
    pub sponge: Sponge,
    #[serde(default = "half")]
    pub strength: f32,
    #[serde(default)]
    pub source: Option<Source>,
    /// Colour the eraser paints on a layer whose transparency is locked.
    #[serde(default)]
    pub background: Option<Rgba8>,
    /// Follow a centripetal Catmull–Rom curve through the points instead of
    /// straight chords (smudge always uses chords).
    #[serde(default = "yes")]
    pub smooth: bool,
}

pub fn stroke_key(v: &Option<Value>) -> Option<String> {
    v.as_ref().map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Sparse float tiles over the canvas.
struct Tiles {
    w: i32,
    h: i32,
    ch: usize,
    cols: i32,
    tiles: Vec<Option<Box<[f32]>>>,
}

const T: i32 = 256;

impl Tiles {
    fn new(w: u32, h: u32, ch: usize) -> Tiles {
        let (w, h) = (w as i32, h as i32);
        let cols = (w + T - 1) / T;
        let rows = (h + T - 1) / T;
        Tiles { w, h, ch, cols: cols.max(1), tiles: (0..(cols.max(1) * rows.max(1))).map(|_| None).collect() }
    }

    fn for_parts(&self, rect: Rect, mut f: impl FnMut(usize, Rect, Rect)) {
        let clip = rect.intersect(&Rect::new(0, 0, self.w, self.h));
        if clip.is_empty() {
            return;
        }
        for ty in clip.y / T..=(clip.bottom() - 1) / T {
            for tx in clip.x / T..=(clip.right() - 1) / T {
                let tr = Rect::new(tx * T, ty * T, T, T);
                f((ty * self.cols + tx) as usize, tr, tr.intersect(&clip));
            }
        }
    }

    fn read(&self, rect: Rect) -> Vec<f32> {
        let mut out = vec![0f32; rect.area().max(0) as usize * self.ch];
        let ch = self.ch;
        self.for_parts(rect, |i, tr, part| {
            if let Some(t) = &self.tiles[i] {
                for y in part.y..part.bottom() {
                    let s = ((y - tr.y) * T + part.x - tr.x) as usize * ch;
                    let d = ((y - rect.y) * rect.w + part.x - rect.x) as usize * ch;
                    let n = part.w as usize * ch;
                    out[d..d + n].copy_from_slice(&t[s..s + n]);
                }
            }
        });
        out
    }

    fn write(&mut self, rect: Rect, data: &[f32]) {
        let ch = self.ch;
        let mut parts = Vec::new();
        self.for_parts(rect, |i, tr, part| parts.push((i, tr, part)));
        for (i, tr, part) in parts {
            let t = self.tiles[i].get_or_insert_with(|| vec![0f32; (T * T) as usize * ch].into_boxed_slice());
            for y in part.y..part.bottom() {
                let d = ((y - tr.y) * T + part.x - tr.x) as usize * ch;
                let s = ((y - rect.y) * rect.w + part.x - rect.x) as usize * ch;
                let n = part.w as usize * ch;
                t[d..d + n].copy_from_slice(&data[s..s + n]);
            }
        }
    }
}

struct Pickup {
    side: i32,
    data: Vec<f32>,
    valid: bool,
}

struct Stroke {
    layer: LayerId,
    target: Target,
    tool: Tool,
    signature: Vec<usize>,
    snapshot: Document,
    cover: Tiles,
    path: PathState,
    footprint: Rect,
    pickup: Option<Pickup>,
    /// Healing correction (RGB, 0..255 units) where the stroke has solved it.
    field: Option<Tiles>,
    source: Source,
    tick: u64,
    /// Coverage under the provisional tail (the newest curve piece) from
    /// before it was drawn, restored when the next segment arrives.
    tail: Option<(Rect, Vec<f32>)>,
    /// Blur/sharpen: the snapshot blurred once per stroke, per tile.
    blur: Option<BlurCache>,
}

/// The stroke's snapshot blurred with one σ, computed lazily per 256² tile
/// (premultiplied RGBA 0..1). Painting mixes toward it through the stroke's
/// coverage, so the result depends on where the stroke went, not on how the
/// pointer path was split into segments.
struct BlurCache {
    sigma: f32,
    tiles: Tiles,
    done: Vec<bool>,
}

static STROKES: LazyLock<Mutex<HashMap<String, Stroke>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static TICK: AtomicU64 = AtomicU64::new(0);
/// Unfinished strokes kept at once (several documents, or a lost stroke-end).
const MAX_STROKES: usize = 32;

fn canvas(doc: &Document) -> Rect {
    Rect::new(0, 0, doc.width as i32, doc.height as i32)
}

pub fn stroke(doc: &mut Document, cmd: StrokeCmd) -> Result<Applied> {
    let id = target_id(doc, cmd.id)?;
    surface::check(doc, id, cmd.target)?;
    if matches!(cmd.tool, Tool::Clone | Tool::Heal) && cmd.source.is_none() {
        return Err(EditorError::Invalid("set a source point first (Alt-click) for the clone and healing tools".into()));
    }
    let key = stroke_key(&cmd.stroke_id);
    let tick = TICK.fetch_add(1, Ordering::Relaxed);
    let mut strokes = STROKES.lock().unwrap_or_else(|e| e.into_inner());
    let current = surface::signature(doc, id, cmd.target);
    let existing = key.as_ref().and_then(|k| strokes.remove(k)).filter(|s| s.layer == id && s.target == cmd.target && s.tool == cmd.tool && s.signature == current);
    let mut st = existing.unwrap_or_else(|| Stroke {
        layer: id,
        target: cmd.target,
        tool: cmd.tool,
        signature: Vec::new(),
        snapshot: doc.clone(),
        cover: Tiles::new(doc.width, doc.height, 1),
        path: PathState::default(),
        footprint: Rect::empty(),
        pickup: None,
        field: if cmd.tool == Tool::Heal { Some(Tiles::new(doc.width, doc.height, 3)) } else { None },
        source: cmd.source.unwrap_or_default(),
        tick,
        tail: None,
        blur: match cmd.tool {
            Tool::Blur => Some(BlurCache::new(doc, blur_sigma(cmd.brush.size))),
            Tool::Sharpen => Some(BlurCache::new(doc, 1.0)),
            _ => None,
        },
    });
    st.tick = tick;

    let (dabs, tail) = if cmd.tool.direct() || !cmd.smooth { (cmd.brush.walk(&mut st.path, &cmd.points), Vec::new()) } else { cmd.brush.walk_smooth(&mut st.path, &cmd.points) };
    let aliased = cmd.tool == Tool::Pencil;
    let stamp = |d: &Dab| {
        // Smudge picks up and lays down dab by dab: plain discs.
        let d = if cmd.tool.direct() { Dab { from: None, ..*d } } else { *d };
        (d, cmd.brush.stamp(&d, aliased))
    };
    let stamps: Vec<(Dab, Stamp)> = dabs.iter().map(stamp).collect();
    let tail: Vec<(Dab, Stamp)> = tail.iter().map(stamp).collect();
    // Take back the previous provisional tail: its area is recomposited.
    let restored = match st.tail.take() {
        Some((rect, cov)) => {
            st.cover.write(rect, &cov);
            rect
        }
        None => Rect::empty(),
    };
    let dirty = stamps.iter().chain(&tail).fold(restored, |acc, (_, s)| acc.union(&s.rect)).intersect(&canvas(doc));

    let result = if dirty.is_empty() {
        Ok(Rect::empty())
    } else if cmd.tool.direct() {
        direct(doc, id, &cmd, &mut st, &stamps, dirty)
    } else {
        buffered(doc, id, &cmd, &mut st, &stamps, &tail, dirty)
    };
    let written = result?;
    st.signature = surface::signature(doc, id, cmd.target);
    let label = cmd.tool.label();
    if let Some(k) = &key {
        strokes.insert(k.clone(), st);
        while strokes.len() > MAX_STROKES {
            let Some(oldest) = strokes.iter().min_by_key(|(_, s)| s.tick).map(|(k, _)| k.clone()) else { break };
            strokes.remove(&oldest);
        }
    } else if cmd.tool == Tool::Heal {
        // A one-shot heal has no stroke-end: finish it now.
        let fin = finish_heal(doc, &mut st)?;
        return Ok(Applied::step(label).dirty(written.union(&fin)));
    }
    let mut applied = Applied::step(label).dirty(written);
    if let Some(k) = key {
        applied = applied.merge(format!("stroke:{k}"));
    }
    Ok(applied)
}

pub fn stroke_end(doc: &mut Document, stroke_id: &Option<Value>) -> Result<Applied> {
    let Some(key) = stroke_key(stroke_id) else { return Ok(Applied::quiet()) };
    let st = STROKES.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
    let Some(mut st) = st else { return Ok(Applied::quiet()) };
    if st.tool != Tool::Heal || surface::signature(doc, st.layer, st.target) != st.signature {
        return Ok(Applied::quiet());
    }
    let dirty = finish_heal(doc, &mut st)?;
    // Same merge key: the final blend belongs to the stroke's undo step.
    Ok(Applied::step(Tool::Heal.label()).merge(format!("stroke:{key}")).dirty(dirty))
}

#[inline]
fn stamp_cap(cmd: &StrokeCmd, dab: &Dab) -> (f32, f32) {
    let b = &cmd.brush;
    let p = if b.pressure_opacity { dab.pressure } else { 1.0 };
    let mut cap = b.opacity.clamp(0.0, 1.0) * p;
    match cmd.tool {
        Tool::Brush | Tool::Pencil => cap *= b.color.alpha_f32(),
        Tool::Dodge | Tool::Burn => cap *= cmd.exposure.clamp(0.0, 1.0),
        _ => {}
    }
    if matches!(cmd.tool, Tool::Blur | Tool::Sharpen) {
        cap *= cmd.strength.clamp(0.0, 1.0);
    }
    let flow = if cmd.tool == Tool::Pencil { 1.0 } else { b.flow.clamp(0.0, 1.0) };
    (cap, flow)
}

/// Blur tool radius: grows with the brush (their σ = d/10, clamped).
fn blur_sigma(size: f32) -> f32 {
    (size / 10.0).clamp(1.5, 30.0)
}

impl BlurCache {
    fn new(doc: &Document, sigma: f32) -> BlurCache {
        let tiles = Tiles::new(doc.width, doc.height, 4);
        let n = tiles.tiles.len();
        BlurCache { sigma, tiles, done: vec![false; n] }
    }

    /// Blurred snapshot over `rect`, computing the tiles it needs.
    fn read(&mut self, snapshot: &Document, id: LayerId, target: Target, rect: Rect) -> Result<Vec<f32>> {
        let mut todo = Vec::new();
        self.tiles.for_parts(rect, |i, tr, _| todo.push((i, tr)));
        for (i, tr) in todo {
            if self.done[i] {
                continue;
            }
            let full = Rect::new(0, 0, self.tiles.w, self.tiles.h);
            let tr = tr.intersect(&full);
            // Three box passes approximate the Gaussian: σ² = ((2r+1)² − 1)/4.
            let r = ((((4.0 * self.sigma * self.sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize).max(1);
            let outer = tr.inflate(3 * r as i32 + 1).intersect(&full);
            let px = surface::read(snapshot, id, target, outer)?;
            let (ow, oh) = (outer.w as usize, outer.h as usize);
            let pre = to_premul(&px);
            let mut out = vec![0f32; ow * oh * 4];
            let mut chan = vec![0f32; ow * oh];
            for k in 0..4 {
                for (j, v) in chan.iter_mut().enumerate() {
                    *v = pre[j * 4 + k];
                }
                editor_core::adjust::box_blur_1ch(&mut chan, ow, oh, r, 3);
                for (j, v) in chan.iter().enumerate() {
                    out[j * 4 + k] = *v;
                }
            }
            let mut inner = vec![0f32; tr.area() as usize * 4];
            for y in 0..tr.h {
                let s = ((y + tr.y - outer.y) as usize * ow + (tr.x - outer.x) as usize) * 4;
                let d = y as usize * tr.w as usize * 4;
                inner[d..d + tr.w as usize * 4].copy_from_slice(&out[s..s + tr.w as usize * 4]);
            }
            self.tiles.write(tr, &inner);
            self.done[i] = true;
        }
        Ok(self.tiles.read(rect))
    }
}

/// Source pixels for clone/heal over `rect` (destination coordinates).
fn source_pixels(st: &Stroke, id: LayerId, rect: Rect) -> Result<Vec<u8>> {
    let (dx, dy) = (st.source.dx.round() as i32, st.source.dy.round() as i32);
    let src = rect.translate(dx, dy);
    if st.source.sample_all && st.target == Target::Pixels {
        Ok(read_merged(&st.snapshot, src))
    } else {
        surface::read(&st.snapshot, id, st.target, src)
    }
}

#[allow(clippy::too_many_arguments)]
fn buffered(doc: &mut Document, id: LayerId, cmd: &StrokeCmd, st: &mut Stroke, stamps: &[(Dab, Stamp)], tail: &[(Dab, Stamp)], dirty: Rect) -> Result<Rect> {
    // Build coverage. Capsules of a hard round tip combine by max, which
    // traces the exact edge; every other tip builds up by flow toward the
    // cap, as Photoshop's dabs do.
    let capsules = cmd.tool != Tool::Pencil && cmd.brush.capsule_tip();
    let deposit = |cov: &mut [f32], stamps: &[(Dab, Stamp)]| {
        for (dab, s) in stamps {
            let (cap, flow) = stamp_cap(cmd, dab);
            let part = s.rect.intersect(&dirty);
            for y in part.y..part.bottom() {
                for x in part.x..part.right() {
                    let a = s.cov[((y - s.rect.y) * s.rect.w + x - s.rect.x) as usize] * flow;
                    if a <= 0.0 {
                        continue;
                    }
                    let c = &mut cov[((y - dirty.y) * dirty.w + x - dirty.x) as usize];
                    if capsules {
                        *c = c.max(cap * a);
                    } else if *c < cap {
                        *c += (cap - *c) * a;
                    }
                }
            }
        }
    };
    let mut cov = st.cover.read(dirty);
    deposit(&mut cov, stamps);
    if !tail.is_empty() {
        let tr = tail.iter().fold(Rect::empty(), |acc, (_, s)| acc.union(&s.rect)).intersect(&dirty);
        let mut saved = vec![0f32; tr.area().max(0) as usize];
        for y in 0..tr.h {
            let s = ((y + tr.y - dirty.y) * dirty.w + tr.x - dirty.x) as usize;
            saved[(y * tr.w) as usize..((y + 1) * tr.w) as usize].copy_from_slice(&cov[s..s + tr.w as usize]);
        }
        st.tail = Some((tr, saved));
        deposit(&mut cov, tail);
    }
    st.cover.write(dirty, &cov);
    st.footprint = st.footprint.union(&dirty);

    if cmd.tool == Tool::Heal {
        let margin = (cmd.brush.size.ceil() as i32).max(16);
        let window = dirty.inflate(margin).intersect(&canvas(doc));
        return heal_region(doc, id, st, window, false);
    }
    compose(doc, id, cmd, st, dirty)?;
    Ok(dirty)
}

/// Recomposite `rect` from the snapshot through the stroke's coverage.
fn compose(doc: &mut Document, id: LayerId, cmd: &StrokeCmd, st: &mut Stroke, rect: Rect) -> Result<()> {
    let orig = surface::read(&st.snapshot, id, st.target, rect)?;
    let blurred = match st.blur.as_mut() {
        Some(b) => b.read(&st.snapshot, id, st.target, rect)?,
        None => Vec::new(),
    };
    let cov = st.cover.read(rect);
    let sel = read_selection(doc, rect);
    let has_sel = doc.selection.is_some();
    let mut out = orig.clone();
    let src = if cmd.tool == Tool::Clone { Some(source_pixels(st, id, rect)?) } else { None };
    let mask = st.target == Target::Mask;
    let color = cmd.brush.color;
    let cs = color.to_f32();
    let grey = 0.3 * cs[0] + 0.59 * cs[1] + 0.11 * cs[2];
    let lock_alpha = !mask && doc.find(id).is_some_and(|l| l.locks.transparency);
    let bg = cmd.background.unwrap_or(Rgba8::WHITE).to_f32();
    for i in 0..cov.len() {
        let mut a = cov[i];
        if has_sel {
            a *= sel[i] as f32 / 255.0;
        }
        if a <= 0.0 {
            continue;
        }
        let o = i * 4;
        let mut d = [orig[o] as f32 / 255.0, orig[o + 1] as f32 / 255.0, orig[o + 2] as f32 / 255.0, orig[o + 3] as f32 / 255.0];
        match cmd.tool {
            Tool::Brush | Tool::Pencil => {
                if mask {
                    let v = d[0] + (grey - d[0]) * a;
                    d = [v, v, v, 1.0];
                } else {
                    composite_px(&mut d, cs, a, cmd.brush.blend);
                }
            }
            Tool::Eraser => {
                if mask {
                    let v = d[0] * (1.0 - a);
                    d = [v, v, v, 1.0];
                } else if lock_alpha {
                    composite_px(&mut d, bg, a, BlendMode::Normal);
                } else {
                    d[3] *= 1.0 - a;
                }
            }
            Tool::Dodge | Tool::Burn => {
                let burn = cmd.tool == Tool::Burn;
                for c in d.iter_mut().take(3) {
                    *c += (dodge_burn(*c, cmd.range, burn) - *c) * a;
                }
            }
            Tool::Sponge => {
                let s = crate::tone::sponge([d[0], d[1], d[2]], cmd.sponge);
                for k in 0..3 {
                    d[k] += (s[k] - d[k]) * a;
                }
            }
            Tool::Blur | Tool::Sharpen => {
                // Premultiplied mix toward the blurred (or unsharp-masked)
                // snapshot.
                let p = [d[0] * d[3], d[1] * d[3], d[2] * d[3], d[3]];
                let b = &blurred[o..o + 4];
                let mut t = [0f32; 4];
                if cmd.tool == Tool::Blur {
                    t.copy_from_slice(b);
                } else {
                    t[3] = (p[3] + (p[3] - b[3]) * 1.5).clamp(0.0, 1.0);
                    for k in 0..3 {
                        t[k] = (p[k] + (p[k] - b[k]) * 1.5).clamp(0.0, t[3]);
                    }
                }
                let m: [f32; 4] = std::array::from_fn(|k| p[k] + (t[k] - p[k]) * a);
                d = if m[3] > 0.0 { [m[0] / m[3], m[1] / m[3], m[2] / m[3], m[3]] } else { [0.0; 4] };
            }
            Tool::Clone => {
                let s = src.as_ref().map_or([0u8; 4], |s| [s[o], s[o + 1], s[o + 2], s[o + 3]]);
                let sc = [s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0];
                if mask {
                    let v = d[0] + (sc[0] - d[0]) * a;
                    d = [v, v, v, 1.0];
                } else {
                    composite_px(&mut d, sc, a * s[3] as f32 / 255.0, cmd.brush.blend);
                }
            }
            _ => {}
        }
        for k in 0..4 {
            out[o + k] = (d[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    surface::write(doc, id, st.target, rect, &out)
}

/// Healing over `window`: the cloned texture plus a harmonic correction that
/// takes the destination's colour and lighting at the edge of the stroke.
/// With `final_pass`, the whole footprint is solved at once.
fn heal_region(doc: &mut Document, id: LayerId, st: &mut Stroke, window: Rect, final_pass: bool) -> Result<Rect> {
    let (w, h) = (window.w as usize, window.h as usize);
    if w == 0 || h == 0 {
        return Ok(Rect::empty());
    }
    let (cw, chh) = (doc.width as i32, doc.height as i32);
    let orig = surface::read(&st.snapshot, id, st.target, window)?;
    let src = source_pixels(st, id, window)?;
    let cov = st.cover.read(window);
    let field_tiles = st.field.as_mut().expect("heal stroke has a field");
    let prev = field_tiles.read(window);
    let mut field = vec![0f32; w * h * 3];
    let mut unknown = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            // Window sides on the canvas edge are not a boundary.
            let on_frame = (x == 0 && window.x > 0) || (y == 0 && window.y > 0) || (x + 1 == w && window.right() < cw) || (y + 1 == h && window.bottom() < chh);
            let inside = cov[i] > 0.0;
            if inside && !(on_frame && !final_pass) {
                unknown[i] = true;
                for k in 0..3 {
                    field[i * 3 + k] = prev[i * 3 + k];
                }
            } else if inside {
                // Window edge inside the stroke: hold the earlier solution.
                for k in 0..3 {
                    field[i * 3 + k] = prev[i * 3 + k];
                }
            } else {
                for k in 0..3 {
                    field[i * 3 + k] = orig[i * 4 + k] as f32 - src[i * 4 + k] as f32;
                }
            }
        }
    }
    membrane(&mut field, &unknown, w, h);
    field_tiles.write(window, &field);

    let sel = read_selection(doc, window);
    let has_sel = doc.selection.is_some();
    let mut out = orig.clone();
    let mask = st.target == Target::Mask;
    for i in 0..w * h {
        let mut a = cov[i];
        if has_sel {
            a *= sel[i] as f32 / 255.0;
        }
        let sa = src[i * 4 + 3] as f32 / 255.0;
        if a <= 0.0 || sa <= 0.0 {
            continue;
        }
        let o = i * 4;
        let f = [0, 1, 2].map(|k| ((src[o + k] as f32 + field[i * 3 + k]) / 255.0).clamp(0.0, 1.0));
        let mut d = [orig[o] as f32 / 255.0, orig[o + 1] as f32 / 255.0, orig[o + 2] as f32 / 255.0, orig[o + 3] as f32 / 255.0];
        if mask {
            let v = d[0] + (f[0] - d[0]) * a;
            d = [v, v, v, 1.0];
        } else {
            composite_px(&mut d, f, a * sa, BlendMode::Normal);
        }
        for k in 0..4 {
            out[o + k] = (d[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    surface::write(doc, id, st.target, window, &out)?;
    Ok(window)
}

fn finish_heal(doc: &mut Document, st: &mut Stroke) -> Result<Rect> {
    if st.footprint.is_empty() {
        return Ok(Rect::empty());
    }
    let region = st.footprint.inflate(2).intersect(&canvas(doc));
    let r = heal_region(doc, st.layer, st, region, true)?;
    st.signature = surface::signature(doc, st.layer, st.target);
    Ok(r)
}

fn to_premul(px: &[u8]) -> Vec<f32> {
    px.chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as f32 / 255.0;
            [p[0] as f32 / 255.0 * a, p[1] as f32 / 255.0 * a, p[2] as f32 / 255.0 * a, a]
        })
        .collect()
}

fn from_premul(buf: &[f32]) -> Vec<u8> {
    buf.chunks_exact(4)
        .flat_map(|p| {
            let a = p[3].clamp(0.0, 1.0);
            let c = |v: f32| if a > 0.0 { ((v / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8 } else { 0 };
            [c(p[0]), c(p[1]), c(p[2]), (a * 255.0 + 0.5) as u8]
        })
        .collect()
}

/// Smudge works on the current pixels, dab by dab: it carries colour picked
/// up along the way.
fn direct(doc: &mut Document, id: LayerId, cmd: &StrokeCmd, st: &mut Stroke, stamps: &[(Dab, Stamp)], dirty: Rect) -> Result<Rect> {
    let outer = dirty;
    let px = surface::read(doc, id, cmd.target, outer)?;
    let mut buf = to_premul(&px);
    let ow = outer.w as usize;
    let sel = read_selection(doc, outer);
    let has_sel = doc.selection.is_some();
    let strength = cmd.strength.clamp(0.0, 1.0);
    for (dab, s) in stamps {
        let p = if cmd.brush.pressure_opacity { dab.pressure } else { 1.0 };
        let k = strength * p;
        let part = s.rect.intersect(&dirty);
        let side = st.pickup.as_ref().map_or((cmd.brush.size.ceil() as i32 + 3).max(3), |p| p.side);
        let pick = st.pickup.get_or_insert_with(|| Pickup { side, data: vec![0.0; (side * side * 4) as usize], valid: false });
        let (cx, cy) = (dab.x.round() as i32 - side / 2, dab.y.round() as i32 - side / 2);
        let first = !pick.valid;
        for y in part.y..part.bottom() {
            for x in part.x..part.right() {
                let (pxl, pyl) = (x - cx, y - cy);
                if pxl < 0 || pyl < 0 || pxl >= side || pyl >= side {
                    continue;
                }
                let bi = ((y - outer.y) as usize * ow + (x - outer.x) as usize) * 4;
                let pi = (pyl * side + pxl) as usize * 4;
                if !first {
                    let mut a = s.cov[((y - s.rect.y) * s.rect.w + x - s.rect.x) as usize] * k;
                    if has_sel {
                        a *= sel[bi / 4] as f32 / 255.0;
                    }
                    if a > 0.0 {
                        for c in 0..4 {
                            buf[bi + c] += (pick.data[pi + c] - buf[bi + c]) * a;
                        }
                    }
                }
                pick.data[pi..pi + 4].copy_from_slice(&buf[bi..bi + 4]);
            }
        }
        pick.valid = true;
    }
    // Write only the inner rectangle.
    let mut inner = vec![0f32; dirty.area() as usize * 4];
    for y in 0..dirty.h {
        let s = ((y + dirty.y - outer.y) as usize * ow + (dirty.x - outer.x) as usize) * 4;
        let d = y as usize * dirty.w as usize * 4;
        inner[d..d + dirty.w as usize * 4].copy_from_slice(&buf[s..s + dirty.w as usize * 4]);
    }
    surface::write(doc, id, cmd.target, dirty, &from_premul(&inner))?;
    Ok(dirty)
}
