//! Liquify: brush strokes edit a backward displacement field, and the layer
//! is always resampled once from the pixels it had when the liquify session
//! began, so strokes never accumulate blur and Reconstruct can return to the
//! original exactly.
//!
//! A session is kept per layer in a small static cache. It stays valid while
//! the layer's pixels are the ones liquify last wrote (compared by tile
//! identity); undoing a stroke puts back the pixels a checkpoint remembers,
//! which restores the field that went with them.

use std::sync::{Arc, Mutex};

use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{LayerId, LayerKind, Raster};
use editor_core::ops::{EditorError, Result};
use editor_core::plane::Plane;
use editor_core::transform::Resample;
use serde::Deserialize;

use crate::sample::Source;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Forward,
    Reconstruct,
    TwirlCw,
    TwirlCcw,
    Pucker,
    Bloat,
    PushLeft,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub p: Option<f64>,
}

const FT: i32 = 128;

/// Sparse full-resolution displacement field over a document rectangle.
/// Each value is the offset from an output pixel to where it samples the
/// original.
#[derive(Clone)]
struct Field {
    rect: Rect,
    cols: i32,
    tiles: Vec<Option<Arc<Vec<[f32; 2]>>>>,
}

impl Field {
    fn new(rect: Rect) -> Field {
        let cols = (rect.w + FT - 1) / FT;
        let rows = (rect.h + FT - 1) / FT;
        Field { rect, cols, tiles: vec![None; (cols.max(0) * rows.max(0)) as usize] }
    }

    #[inline]
    fn get(&self, x: i32, y: i32) -> [f32; 2] {
        let (lx, ly) = (x - self.rect.x, y - self.rect.y);
        if lx < 0 || ly < 0 || lx >= self.rect.w || ly >= self.rect.h {
            return [0.0; 2];
        }
        match &self.tiles[((ly / FT) * self.cols + lx / FT) as usize] {
            None => [0.0; 2],
            Some(t) => t[((ly % FT) * FT + lx % FT) as usize],
        }
    }

    fn read(&self, r: Rect) -> Vec<[f32; 2]> {
        let mut out = Vec::with_capacity(r.area().max(0) as usize);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                out.push(self.get(x, y));
            }
        }
        out
    }

    /// Write `data` over `r` (clipped to the field).
    fn write(&mut self, r: Rect, data: &[[f32; 2]]) {
        let clip = r.intersect(&self.rect);
        for y in clip.y..clip.bottom() {
            for x in clip.x..clip.right() {
                let v = data[((y - r.y) * r.w + (x - r.x)) as usize];
                let (lx, ly) = (x - self.rect.x, y - self.rect.y);
                let ti = ((ly / FT) * self.cols + lx / FT) as usize;
                if self.tiles[ti].is_none() {
                    if v == [0.0; 2] {
                        continue;
                    }
                    self.tiles[ti] = Some(Arc::new(vec![[0.0; 2]; (FT * FT) as usize]));
                }
                let t = Arc::make_mut(self.tiles[ti].as_mut().unwrap());
                t[((ly % FT) * FT + lx % FT) as usize] = v;
            }
        }
    }
}

struct Sig {
    ptrs: Vec<usize>,
    w: u32,
    h: u32,
    x: i32,
    y: i32,
    /// Holds the tiles alive so their addresses cannot be reused.
    _keep: Plane,
}

impl Sig {
    fn of(r: &Raster) -> Sig {
        Sig { ptrs: r.plane.tile_ptrs().collect(), w: r.plane.width(), h: r.plane.height(), x: r.x, y: r.y, _keep: r.plane.clone() }
    }
    fn matches(&self, r: &Raster) -> bool {
        self.w == r.plane.width() && self.h == r.plane.height() && self.x == r.x && self.y == r.y && r.plane.tile_ptrs().eq(self.ptrs.iter().copied())
    }
}

struct Session {
    layer: LayerId,
    original: Raster,
    field: Field,
    last: Sig,
    stroke: String,
    /// Pixels at the start of each stroke, with the field at that moment.
    checkpoints: Vec<(Sig, Field, String)>,
    last_dab: Option<(f64, f64)>,
    stamp: u64,
}

static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());
static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
const MAX_SESSIONS: usize = 4;

/// Forget cached liquify sessions for a layer (or all when `None`).
pub fn release(layer: Option<LayerId>) {
    let mut s = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    s.retain(|x| layer.is_some_and(|l| x.layer != l));
}

pub struct Params {
    pub tool: Tool,
    pub size: f64,
    pub pressure: f64,
    pub density: f64,
    pub points: Vec<StrokePoint>,
    pub stroke_id: String,
}

fn unit(v: f64) -> f64 {
    if v > 1.0 {
        (v / 100.0).clamp(0.0, 1.0)
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// Brush falloff at normalised distance `t` (0 centre, 1 edge). Density 1
/// keeps full strength further out; 0 fades from the centre.
#[inline]
fn falloff(t: f64, density: f64) -> f64 {
    if t >= 1.0 {
        return 0.0;
    }
    let core = density * 0.75;
    let s = ((t - core) / (1.0 - core)).clamp(0.0, 1.0);
    0.5 * (1.0 + (std::f64::consts::PI * s).cos())
}

pub fn apply(doc: &mut Document, id: LayerId, p: Params) -> Result<Rect> {
    let layer = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    if layer.locks.all || layer.locks.pixels {
        return Err(EditorError::Locked(layer.name.clone()));
    }
    let raster = match &layer.kind {
        LayerKind::Pixel(r) => r.clone(),
        LayerKind::Text { .. } | LayerKind::Shape { .. } | LayerKind::Smart { .. } => {
            return Err(EditorError::Invalid(format!("`{}` is a {} layer; rasterize it first", layer.name, layer.kind.name())))
        }
        _ => return Err(EditorError::Invalid(format!("`{}` has no pixels to liquify", layer.name))),
    };
    let lock_alpha = layer.locks.transparency;
    if p.points.is_empty() || p.size <= 0.0 {
        return Ok(Rect::empty());
    }
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);

    let mut sessions = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let now = CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut idx = sessions.iter().position(|s| s.layer == id && s.last.matches(&raster));
    if idx.is_none() {
        // Undo landed on the start of an earlier stroke?
        for (si, s) in sessions.iter_mut().enumerate() {
            if s.layer != id {
                continue;
            }
            if let Some(ci) = s.checkpoints.iter().position(|(sig, _, _)| sig.matches(&raster)) {
                let (sig, field, _) = s.checkpoints.remove(ci);
                s.checkpoints.truncate(ci);
                s.field = field;
                s.last = sig;
                s.stroke = String::new();
                idx = Some(si);
                break;
            }
        }
    }
    let si = match idx {
        Some(i) => i,
        None => {
            let rect = canvas.union(&raster.doc_rect());
            sessions.push(Session {
                layer: id,
                original: raster.clone(),
                field: Field::new(rect),
                last: Sig::of(&raster),
                stroke: String::new(),
                checkpoints: Vec::new(),
                last_dab: None,
                stamp: now,
            });
            if sessions.len() > MAX_SESSIONS {
                let oldest = sessions.iter().enumerate().min_by_key(|(_, s)| s.stamp).map(|(i, _)| i).unwrap();
                sessions.remove(oldest);
            }
            sessions.len() - 1
        }
    };
    let sess = &mut sessions[si];
    sess.stamp = now;
    if sess.stroke != p.stroke_id {
        sess.checkpoints.push((Sig::of(&raster), sess.field.clone(), p.stroke_id.clone()));
        if sess.checkpoints.len() > 64 {
            sess.checkpoints.remove(0);
        }
        sess.stroke = p.stroke_id.clone();
        sess.last_dab = None;
    }

    let sel = doc.selection.clone();
    let dirty = stroke(sess, &p, sel.as_ref());
    let dirty = dirty.intersect(&sess.field.rect);
    if dirty.is_empty() {
        return Ok(dirty);
    }
    let out = render(&sess.original, &sess.field, dirty, lock_alpha);

    let layer = doc.find_mut(id).ok_or(EditorError::NoLayer(id))?;
    let LayerKind::Pixel(r) = &mut layer.kind else { unreachable!() };
    r.ensure_covers(dirty);
    r.plane.write(dirty.translate(-r.x, -r.y), &out);
    r.plane.compact();
    sess.last = Sig::of(r);
    Ok(dirty)
}

/// Lay dabs along the points; returns the field area that changed.
fn stroke(sess: &mut Session, p: &Params, sel: Option<&Plane>) -> Rect {
    let radius = p.size / 2.0;
    let spacing = (p.size * 0.08).max(1.0);
    let pressure = unit(p.pressure);
    let density = unit(p.density);
    let mut dirty = Rect::empty();
    let mut dabs: Vec<(f64, f64, f64, f64, f64)> = Vec::new(); // x, y, dx, dy, pressure
    let mut prev = sess.last_dab;
    for pt in &p.points {
        let pp = pressure * pt.p.map_or(1.0, |v| v.clamp(0.0, 1.0));
        match prev {
            None => {
                dabs.push((pt.x, pt.y, 0.0, 0.0, pp));
                prev = Some((pt.x, pt.y));
            }
            Some((px, py)) => {
                let (dx, dy) = (pt.x - px, pt.y - py);
                let dist = dx.hypot(dy);
                let steps = (dist / spacing).floor() as usize;
                let (ux, uy) = if dist > 0.0 { (dx / dist * spacing, dy / dist * spacing) } else { (0.0, 0.0) };
                let mut cur = (px, py);
                for _ in 0..steps {
                    let next = (cur.0 + ux, cur.1 + uy);
                    dabs.push((next.0, next.1, ux, uy, pp));
                    cur = next;
                }
                // Twirl, pucker, bloat and reconstruct keep working while the
                // pointer holds still.
                if steps == 0 && !matches!(p.tool, Tool::Forward | Tool::PushLeft) {
                    dabs.push((pt.x, pt.y, 0.0, 0.0, pp));
                }
                prev = Some(cur);
            }
        }
    }
    sess.last_dab = prev;
    for (cx, cy, mx, my, pp) in dabs {
        let move_len = mx.hypot(my);
        // Stationary tools need no motion; motion tools need some.
        if matches!(p.tool, Tool::Forward | Tool::PushLeft) && move_len == 0.0 {
            continue;
        }
        let r = Rect::cover(cx - radius - 1.0, cy - radius - 1.0, 2.0 * radius + 2.0, 2.0 * radius + 2.0).intersect(&sess.field.rect);
        if r.is_empty() {
            continue;
        }
        dab(&mut sess.field, r, (cx, cy), radius, density, pp, p.tool, (mx, my), sel);
        dirty = dirty.union(&r);
    }
    dirty
}

#[allow(clippy::too_many_arguments)]
fn dab(field: &mut Field, r: Rect, c: (f64, f64), radius: f64, density: f64, pp: f64, tool: Tool, mv: (f64, f64), sel: Option<&Plane>) {
    // Motion vectors read the old field up to one brush radius away.
    let apron = (radius.ceil() as i32 + 2).max(2);
    let read_rect = r.inflate(apron);
    let old = field.read(read_rect);
    let rw = read_rect.w as usize;
    let sample_old = |x: f64, y: f64| -> [f64; 2] {
        let fx = x - read_rect.x as f64;
        let fy = y - read_rect.y as f64;
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let (ix, iy) = (ix as i64, iy as i64);
        let get = |xx: i64, yy: i64| -> [f64; 2] {
            if xx < 0 || yy < 0 || xx >= read_rect.w as i64 || yy >= read_rect.h as i64 {
                return [0.0; 2];
            }
            let v = old[yy as usize * rw + xx as usize];
            [v[0] as f64, v[1] as f64]
        };
        let (a, b, c2, d) = (get(ix, iy), get(ix + 1, iy), get(ix, iy + 1), get(ix + 1, iy + 1));
        [
            a[0] * (1.0 - tx) * (1.0 - ty) + b[0] * tx * (1.0 - ty) + c2[0] * (1.0 - tx) * ty + d[0] * tx * ty,
            a[1] * (1.0 - tx) * (1.0 - ty) + b[1] * tx * (1.0 - ty) + c2[1] * (1.0 - tx) * ty + d[1] * tx * ty,
        ]
    };
    let mut out = Vec::with_capacity(r.area() as usize);
    let rate = 0.06 * pp;
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let i = ((y - read_rect.y) as usize) * rw + (x - read_rect.x) as usize;
            let cur = old[i];
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let (ox, oy) = (px - c.0, py - c.1);
            let mut w = falloff(ox.hypot(oy) / radius, density);
            if let Some(s) = sel {
                w *= s.get(x, y)[0] as f64 / 255.0;
            }
            if w <= 0.0 {
                out.push(cur);
                continue;
            }
            // Output p now shows what previously sat at q: D'(p) = q - p + D(q).
            let q = match tool {
                Tool::Reconstruct => {
                    let k = (1.0 - pp * w * 0.5).clamp(0.0, 1.0);
                    let mut v = [cur[0] * k as f32, cur[1] * k as f32];
                    if v[0].abs() < 0.02 && v[1].abs() < 0.02 {
                        v = [0.0; 2];
                    }
                    out.push(v);
                    continue;
                }
                Tool::Forward => (px - mv.0 * pp * w, py - mv.1 * pp * w),
                Tool::PushLeft => {
                    // Left of the direction of travel (y down): (dy, -dx).
                    let (lx, ly) = (mv.1, -mv.0);
                    (px - lx * pp * w, py - ly * pp * w)
                }
                Tool::TwirlCw | Tool::TwirlCcw => {
                    let th = rate * 1.5 * w * if tool == Tool::TwirlCw { -1.0 } else { 1.0 };
                    let (s, co) = th.sin_cos();
                    (c.0 + co * ox - s * oy, c.1 + s * ox + co * oy)
                }
                Tool::Pucker => (c.0 + ox * (1.0 + rate * w), c.1 + oy * (1.0 + rate * w)),
                Tool::Bloat => (c.0 + ox * (1.0 - rate * w), c.1 + oy * (1.0 - rate * w)),
            };
            let d = sample_old(q.0 - 0.5, q.1 - 0.5);
            out.push([(q.0 - px + d[0]) as f32, (q.1 - py + d[1]) as f32]);
        }
    }
    field.write(r, &out);
}

/// Resample `dirty` from the original through the field.
fn render(original: &Raster, field: &Field, dirty: Rect, lock_alpha: bool) -> Vec<u8> {
    let disp = field.read(dirty.inflate(1));
    let dw = dirty.w as usize + 2;
    // Source area: every point the field reads from.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for y in 0..dirty.h {
        for x in 0..dirty.w {
            let d = disp[(y as usize + 1) * dw + x as usize + 1];
            let (sx, sy) = ((dirty.x + x) as f64 + d[0] as f64, (dirty.y + y) as f64 + d[1] as f64);
            x0 = x0.min(sx);
            y0 = y0.min(sy);
            x1 = x1.max(sx);
            y1 = y1.max(sy);
        }
    }
    let need = Rect::cover(x0, y0, x1 - x0 + 1.0, y1 - y0 + 1.0).inflate(4).intersect(&original.doc_rect());
    let src = Source::from_plane_rect(&original.plane, need.translate(-original.x, -original.y), original.x, original.y);
    let mut out = vec![0u8; dirty.area() as usize * 4];
    for y in 0..dirty.h as usize {
        for x in 0..dirty.w as usize {
            let at = |dx: isize, dy: isize| disp[((y as isize + 1 + dy) as usize) * dw + (x as isize + 1 + dx) as usize];
            let d = at(0, 0);
            let (l, r, u, b) = (at(-1, 0), at(1, 0), at(0, -1), at(0, 1));
            let jac = [
                1.0 + (r[0] - l[0]) as f64 * 0.5,
                (b[0] - u[0]) as f64 * 0.5,
                (r[1] - l[1]) as f64 * 0.5,
                1.0 + (b[1] - u[1]) as f64 * 0.5,
            ];
            let (px, py) = ((dirty.x as usize + x) as f64 + 0.5, (dirty.y as usize + y) as f64 + 0.5);
            let o = (y * dirty.w as usize + x) * 4;
            let mut v = if d == [0.0; 2] {
                original.plane.get(dirty.x + x as i32 - original.x, dirty.y + y as i32 - original.y)
            } else {
                src.finish(src.sample(px + d[0] as f64, py + d[1] as f64, &jac, Resample::Bicubic))
            };
            if lock_alpha {
                v[3] = original.plane.get(dirty.x + x as i32 - original.x, dirty.y + y as i32 - original.y)[3];
            }
            out[o..o + 4].copy_from_slice(&v);
        }
    }
    out
}
