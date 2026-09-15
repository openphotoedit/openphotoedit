//! Quick Selection: an edge-aware brush.
//!
//! Each stroke segment re-solves the whole stroke inside a window around it:
//!
//! 1. The composite is rendered over the window, downscaled so the solve
//!    stays under a fixed pixel budget.
//! 2. A foreground colour model comes from the pixels under the brush, a
//!    background model from the window's frame (where it is not the canvas
//!    edge) filtered by that model, over a uniform prior.
//! 3. A seeded watershed (image foresting transform with a max-arc path
//!    cost) grows the brushed pixels against background seeds over edge
//!    weights mixing colour difference and foreground-probability difference.
//!    A max-arc cost puts the boundary on the strongest edge between the
//!    seeds, however far they are from it.
//! 4. The label is upsampled and its boundary band is snapped to the image at
//!    full resolution with a colour-guided filter.
//!
//! Segments sharing a `stroke_id` keep the selection from before the stroke
//! and the brushed points, so the result is independent of how the stroke was
//! split.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use editor_core::document::Document;
use editor_core::geom::{Point, Rect};
use editor_core::plane::Plane;
use editor_core::render::{render_view, View};

use crate::guided::refine_band;

/// Low-resolution solve budget in pixels.
const BUDGET: f64 = 500_000.0;

struct Stroke {
    base: Option<Plane>,
    produced: Vec<usize>,
    points: Vec<Point>,
    /// Window margin the last segment needed.
    ext: f64,
    tick: u64,
}

static STROKES: LazyLock<Mutex<HashMap<String, Stroke>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const MAX_STROKES: usize = 32;

/// Identity of a selection's storage, to notice when it changed under a
/// stroke (an undo, another command).
pub fn signature(sel: &Option<Plane>) -> Vec<usize> {
    match sel {
        None => vec![usize::MAX],
        Some(p) => {
            let mut v = vec![p.width() as usize, p.height() as usize, p.fill()[0] as usize];
            v.extend(p.tile_ptrs());
            v
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickMode {
    /// Start from nothing (the first stroke of a new selection).
    New,
    Add,
    Subtract,
}

/// Apply a quick-selection segment. Returns the document rectangle whose
/// selection changed.
pub fn apply(doc: &mut Document, points: &[Point], radius: f64, mode: QuickMode, stroke_id: Option<&str>) -> Rect {
    let tick = TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut strokes = STROKES.lock().unwrap_or_else(|e| e.into_inner());
    let current = signature(&doc.selection);
    let key = stroke_id.map(|s| format!("{s}|{mode:?}"));
    let reuse = key.as_ref().and_then(|k| strokes.get(k)).is_some_and(|s| s.produced == current);
    let mut stroke = if reuse {
        strokes.remove(key.as_ref().unwrap()).unwrap()
    } else {
        let base = if mode == QuickMode::New { None } else { doc.selection.clone() };
        Stroke { base, produced: Vec::new(), points: Vec::new(), ext: 0.0, tick }
    };
    stroke.tick = tick;
    for p in points {
        if stroke.points.last() != Some(p) {
            stroke.points.push(*p);
        }
    }
    let dirty = solve(doc, &stroke.base, &stroke.points, radius.max(1.0), mode == QuickMode::Subtract, &mut stroke.ext);
    stroke.produced = signature(&doc.selection);
    if let Some(k) = key {
        strokes.insert(k, stroke);
        while strokes.len() > MAX_STROKES {
            let oldest = strokes.iter().min_by_key(|(_, s)| s.tick).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => strokes.remove(&k),
                None => break,
            };
        }
    }
    dirty
}

pub(crate) struct Hist {
    bins: Vec<f32>,
    n: f32,
}

const HB: usize = 16;

impl Hist {
    pub(crate) fn new() -> Hist {
        Hist { bins: vec![0.0; HB * HB * HB], n: 0.0 }
    }

    fn corners(c: [f32; 3]) -> ([usize; 3], [f32; 3]) {
        let mut i = [0usize; 3];
        let mut f = [0f32; 3];
        for k in 0..3 {
            let u = c[k].clamp(0.0, 1.0) * (HB - 1) as f32;
            let b = (u.floor() as usize).min(HB - 2);
            i[k] = b;
            f[k] = u - b as f32;
        }
        (i, f)
    }

    pub(crate) fn add(&mut self, c: [f32; 3]) {
        let (i, f) = Hist::corners(c);
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let w = (if dx == 1 { f[0] } else { 1.0 - f[0] }) * (if dy == 1 { f[1] } else { 1.0 - f[1] }) * (if dz == 1 { f[2] } else { 1.0 - f[2] });
                    self.bins[((i[0] + dx) * HB + i[1] + dy) * HB + i[2] + dz] += w;
                }
            }
        }
        self.n += 1.0;
    }

    /// Probability mass near `c` (sums to one over the bins).
    pub(crate) fn density(&self, c: [f32; 3]) -> f32 {
        if self.n <= 0.0 {
            return 0.0;
        }
        let (i, f) = Hist::corners(c);
        let mut v = 0.0;
        for dz in 0..2 {
            for dy in 0..2 {
                for dx in 0..2 {
                    let w = (if dx == 1 { f[0] } else { 1.0 - f[0] }) * (if dy == 1 { f[1] } else { 1.0 - f[1] }) * (if dz == 1 { f[2] } else { 1.0 - f[2] });
                    v += w * self.bins[((i[0] + dx) * HB + i[1] + dy) * HB + i[2] + dz];
                }
            }
        }
        v / self.n
    }

    /// Spread each bin a little into its neighbours so colours the brush just
    /// missed still count.
    pub(crate) fn smooth(&mut self) {
        let mut out = self.bins.clone();
        for a in 0..HB {
            for b in 0..HB {
                for c in 0..HB {
                    let mut s = 0.0;
                    let mut wsum = 0.0;
                    for (da, db, dc, w) in [(0i32, 0i32, 0i32, 4.0f32), (-1, 0, 0, 1.0), (1, 0, 0, 1.0), (0, -1, 0, 1.0), (0, 1, 0, 1.0), (0, 0, -1, 1.0), (0, 0, 1, 1.0)] {
                        let (x, y, z) = (a as i32 + da, b as i32 + db, c as i32 + dc);
                        if x < 0 || y < 0 || z < 0 || x >= HB as i32 || y >= HB as i32 || z >= HB as i32 {
                            continue;
                        }
                        s += w * self.bins[((x as usize) * HB + y as usize) * HB + z as usize];
                        wsum += w;
                    }
                    out[(a * HB + b) * HB + c] = s / wsum;
                }
            }
        }
        self.bins = out;
    }
}

fn stamp_disc(mask: &mut [u8], w: usize, h: usize, cx: f64, cy: f64, r: f64) {
    let x0 = (cx - r).floor().max(0.0) as usize;
    let y0 = (cy - r).floor().max(0.0) as usize;
    let x1 = ((cx + r).ceil() as i64).clamp(0, w as i64 - 1) as usize;
    let y1 = ((cy + r).ceil() as i64).clamp(0, h as i64 - 1) as usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r + 0.25 {
                mask[y * w + x] = 1;
            }
        }
    }
    // A disc narrower than a pixel still marks the pixel it sits in.
    let (px, py) = (cx.floor(), cy.floor());
    if px >= 0.0 && py >= 0.0 && (px as usize) < w && (py as usize) < h {
        mask[py as usize * w + px as usize] = 1;
    }
}

/// Seeded watershed: label 1 grows from `seed == 1`, label 2 from `seed == 2`.
pub(crate) fn watershed(seed: &[u8], img: &[f32], pf: &[f32], w: usize, h: usize) -> Vec<u8> {
    const LEVELS: usize = 1024;
    let n = w * h;
    let mut cost = vec![u16::MAX; n];
    let mut label = seed.to_vec();
    let mut done = vec![false; n];
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); LEVELS];
    for i in 0..n {
        if seed[i] != 0 {
            cost[i] = 0;
            buckets[0].push(i as u32);
        }
    }
    let weight = |p: usize, q: usize| -> u16 {
        let dr = img[p * 3] - img[q * 3];
        let dg = img[p * 3 + 1] - img[q * 3 + 1];
        let db = img[p * 3 + 2] - img[q * 3 + 2];
        let di = (dr * dr + dg * dg + db * db).sqrt();
        let dp = (pf[p] - pf[q]).abs();
        let v = (di * 1.0 + dp * 1.5) * 700.0;
        (v as usize).min(LEVELS - 1) as u16
    };
    for level in 0..LEVELS {
        let mut i = 0;
        while i < buckets[level].len() {
            let p = buckets[level][i] as usize;
            i += 1;
            if done[p] {
                continue;
            }
            done[p] = true;
            let (x, y) = (p % w, p / w);
            let cp = cost[p];
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let q = ny as usize * w + nx as usize;
                if done[q] {
                    continue;
                }
                let c = cp.max(weight(p, q));
                if c < cost[q] {
                    cost[q] = c;
                    label[q] = label[p];
                    buckets[c as usize].push(q as u32);
                }
            }
        }
        buckets[level] = Vec::new();
    }
    label
}

struct Labelled {
    low: Vec<f32>,
    s: f64,
    lw: usize,
    lh: usize,
    /// The selected region reaches a window side that is not the canvas
    /// edge: the window cut it off.
    clipped: bool,
}

/// Solve the stroke's labels over `window` at reduced resolution.
fn label_window(doc: &Document, base: &Option<Plane>, points: &[Point], radius: f64, subtract: bool, window: Rect) -> Labelled {
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let s = (BUDGET / window.area() as f64).sqrt().min(1.0);
    let lw = ((window.w as f64 * s).ceil() as usize).max(1);
    let lh = ((window.h as f64 * s).ceil() as usize).max(1);
    let view = View { x: window.x as f64, y: window.y as f64, scale: s, width: lw, height: lh };
    let rgba = render_view(doc, view);
    let n = lw * lh;
    let mut img = vec![0f32; n * 3];
    for i in 0..n {
        img[i * 3..i * 3 + 3].copy_from_slice(&rgba[i * 4..i * 4 + 3]);
    }

    // Brushed pixels.
    let mut seeds = vec![0u8; n];
    let rl = (radius * s).max(0.75);
    let to_low = |p: &Point| ((p.x - window.x as f64) * s, (p.y - window.y as f64) * s);
    let first = to_low(&points[0]);
    stamp_disc(&mut seeds, lw, lh, first.0, first.1, rl);
    for pair in points.windows(2) {
        let (ax, ay) = to_low(&pair[0]);
        let (bx, by) = to_low(&pair[1]);
        let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let steps = (len / (rl * 0.5).max(0.5)).ceil().max(1.0) as usize;
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            stamp_disc(&mut seeds, lw, lh, ax + (bx - ax) * t, ay + (by - ay) * t, rl);
        }
    }

    // Pixels already selected (add) never act as background.
    let known = match base {
        Some(b) if !subtract => {
            let mut v = vec![0f32; n];
            b.resample(window.x as f64, window.y as f64, 1.0 / s, lw, lh, &mut v);
            v.iter().map(|&c| c >= 0.5).collect::<Vec<bool>>()
        }
        _ => vec![false; n],
    };

    let mut fg = Hist::new();
    for i in 0..n {
        if seeds[i] != 0 {
            fg.add([img[i * 3], img[i * 3 + 1], img[i * 3 + 2]]);
        }
    }
    fg.smooth();

    // Frame pixels on sides that are not the canvas edge.
    let frame = 2usize;
    let open = [window.x > 0, window.y > 0, window.right() < canvas.w, window.bottom() < canvas.h];
    let on_frame = |i: usize| {
        let (x, y) = (i % lw, i / lw);
        (open[0] && x < frame) || (open[1] && y < frame) || (open[2] && x + frame >= lw) || (open[3] && y + frame >= lh)
    };
    let uniform = 1.0 / (HB * HB * HB) as f32;
    let pf_with = |bg: &Hist, c: [f32; 3]| -> f32 {
        let f = fg.density(c);
        let b = 0.7 * bg.density(c) + 0.3 * uniform;
        let b = if bg.n > 0.0 { b } else { uniform };
        f / (f + b + 1e-9)
    };
    let col = |i: usize| [img[i * 3], img[i * 3 + 1], img[i * 3 + 2]];
    let mut bg0 = Hist::new();
    for i in 0..n {
        if on_frame(i) && seeds[i] == 0 && !known[i] {
            bg0.add(col(i));
        }
    }
    let mut bg = Hist::new();
    for i in 0..n {
        if on_frame(i) && seeds[i] == 0 && !known[i] && pf_with(&bg0, col(i)) < 0.5 {
            bg.add(col(i));
        }
    }
    bg.smooth();
    let mut pf: Vec<f32> = (0..n).map(|i| pf_with(&bg, col(i))).collect();
    // A light blur keeps single noisy pixels from becoming seeds or walls.
    editor_core::adjust::box_blur_1ch(&mut pf, lw, lh, 1, 1);

    let mut label_seed = vec![0u8; n];
    for i in 0..n {
        if seeds[i] != 0 {
            label_seed[i] = 1;
        } else if !known[i] && (pf[i] < 0.12 || (on_frame(i) && pf[i] < 0.5)) {
            label_seed[i] = 2;
        }
    }
    let label = watershed(&label_seed, &img, &pf, lw, lh);
    let low: Vec<f32> = label.iter().map(|&l| (l == 1) as u8 as f32).collect();
    let touching = (0..n).filter(|&i| label[i] == 1 && on_frame(i)).count();
    let clipped = touching > (lw + lh) / 100 + 2;
    Labelled { low, s, lw, lh, clipped }
}

/// Upsample a low-resolution 0/1 label over `window` to full resolution and
/// snap its boundary band to the image with a colour-guided filter.
pub(crate) fn upsample_and_snap(doc: &Document, window: Rect, low: &[f32], s: f64, lw: usize, lh: usize) -> Vec<f32> {
    let n = lw * lh;
    // Boundary cells at low resolution, dilated.
    let mut edge = vec![false; n];
    for y in 0..lh {
        for x in 0..lw {
            let v = low[y * lw + x];
            let mut differs = false;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < lw && (ny as usize) < lh && low[ny as usize * lw + nx as usize] != v {
                    differs = true;
                }
            }
            edge[y * lw + x] = differs;
        }
    }
    let grow = if s < 0.999 { 2 } else { 1 };
    let mut band_low = edge.clone();
    for _ in 0..grow {
        let prev = band_low.clone();
        for y in 0..lh {
            for x in 0..lw {
                if prev[y * lw + x] {
                    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < lw && (ny as usize) < lh {
                            band_low[ny as usize * lw + nx as usize] = true;
                        }
                    }
                }
            }
        }
    }

    // Full resolution: bilinear label, band weights.
    let (ww, wh) = (window.w as usize, window.h as usize);
    let mut mask = vec![0f32; ww * wh];
    let mut weight = vec![0f32; ww * wh];
    for y in 0..wh {
        let ly = ((y as f64 + 0.5) * s - 0.5).max(0.0);
        let iy = (ly.floor() as usize).min(lh - 1);
        let iy1 = (iy + 1).min(lh - 1);
        let ty = (ly - iy as f64) as f32;
        let ny = (((y as f64 + 0.5) * s) as usize).min(lh - 1);
        for x in 0..ww {
            let lx = ((x as f64 + 0.5) * s - 0.5).max(0.0);
            let ix = (lx.floor() as usize).min(lw - 1);
            let ix1 = (ix + 1).min(lw - 1);
            let tx = (lx - ix as f64) as f32;
            let v = low[iy * lw + ix] * (1.0 - tx) * (1.0 - ty) + low[iy * lw + ix1] * tx * (1.0 - ty) + low[iy1 * lw + ix] * (1.0 - tx) * ty + low[iy1 * lw + ix1] * tx * ty;
            mask[y * ww + x] = v;
            let nx = (((x as f64 + 0.5) * s) as usize).min(lw - 1);
            if band_low[ny * lw + nx] {
                weight[y * ww + x] = 1.0;
            }
        }
    }
    let r = ((2.0 / s).round() as usize).clamp(2, 16);
    refine_band(doc, window, &mut mask, &weight, r, 2e-3, 1);
    for (m, w) in mask.iter_mut().zip(weight.iter()) {
        if *w > 0.0 {
            *m = ((*m - 0.5) * 2.0 + 0.5).clamp(0.0, 1.0);
        }
    }

    mask
}

fn solve(doc: &mut Document, base: &Option<Plane>, points: &[Point], radius: f64, subtract: bool, ext: &mut f64) -> Rect {
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    if points.is_empty() || canvas.is_empty() {
        return Rect::empty();
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        x0 = x0.min(p.x - radius);
        y0 = y0.min(p.y - radius);
        x1 = x1.max(p.x + radius);
        y1 = y1.max(p.y + radius);
    }
    let seeds_rect = Rect::cover(x0, y0, x1 - x0, y1 - y0);
    *ext = ext.max((radius * 5.0).max(96.0));
    // Widen the window while the region runs into its sides, so a large
    // object is not cut off along a straight line.
    let (window, lab) = loop {
        let window = seeds_rect.inflate(*ext as i32).intersect(&canvas);
        if window.is_empty() {
            return Rect::empty();
        }
        let lab = label_window(doc, base, points, radius, subtract, window);
        if !lab.clipped || window == canvas || *ext > (canvas.w.max(canvas.h) as f64) {
            break (window, lab);
        }
        *ext *= 2.5;
    };
    let Labelled { low, s, lw, lh, .. } = lab;

    let mask = upsample_and_snap(doc, window, &low, s, lw, lh);

    // Combine with the selection from before the stroke.
    let (dw, dh) = (doc.width, doc.height);
    let mut result = match (base, subtract) {
        (Some(b), _) => b.clone(),
        (None, false) => Plane::mask(dw, dh, 0),
        (None, true) => Plane::mask(dw, dh, 255),
    };
    let before = result.read_vec(window);
    let out: Vec<u8> = before
        .iter()
        .zip(mask.iter())
        .map(|(&b, &m)| {
            let m = (m * 255.0).round() as u8;
            if subtract {
                ((b as u32 * (255 - m as u32) + 127) / 255) as u8
            } else {
                b.max(m)
            }
        })
        .collect();
    result.write(window, &out);
    result.compact();
    doc.selection = Some(result);
    window
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::layer::{Layer, LayerKind, Raster};

    /// A dark disc on a light, slightly noisy ground.
    fn disc_doc() -> Document {
        let (w, h) = (200u32, 160u32);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let d = ((x as f32 - 100.0).powi(2) + (y as f32 - 80.0).powi(2)).sqrt();
                let noise = ((x * 7 + y * 13) % 9) as u8;
                if d < 50.0 {
                    data.extend_from_slice(&[30 + noise, 40, 90 + noise, 255]);
                } else {
                    data.extend_from_slice(&[220 - noise, 200, 170, 255]);
                }
            }
        }
        let mut doc = Document::new(w, h);
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "px", LayerKind::Pixel(Raster::new(Plane::from_raw(w, h, 4, &data, [0; 4]), 0, 0))));
        doc.active = Some(id);
        doc
    }

    #[test]
    fn a_short_stroke_selects_the_whole_object_and_nothing_else() {
        let mut doc = disc_doc();
        apply(&mut doc, &[Point::new(95.0, 80.0), Point::new(105.0, 82.0)], 6.0, QuickMode::New, Some("qs-disc"));
        let sel = doc.selection.as_ref().unwrap();
        let mut wrong = 0;
        for y in 0..160 {
            for x in 0..200 {
                let d = ((x as f32 + 0.5 - 100.0).powi(2) + (y as f32 + 0.5 - 80.0).powi(2)).sqrt();
                let v = sel.get(x, y)[0];
                if (d < 48.0 && v < 128) || (d > 52.0 && v >= 128) {
                    wrong += 1;
                }
            }
        }
        assert!(wrong < 20, "{wrong} misclassified pixels");
    }

    #[test]
    fn segments_accumulate_and_subtract_removes() {
        let mut doc = disc_doc();
        apply(&mut doc, &[Point::new(100.0, 80.0)], 5.0, QuickMode::New, Some("qs-seg"));
        let first = signature(&doc.selection);
        apply(&mut doc, &[Point::new(110.0, 80.0)], 5.0, QuickMode::New, Some("qs-seg"));
        assert_ne!(first, signature(&doc.selection));
        assert!(doc.selection.as_ref().unwrap().get(100, 80)[0] == 255);
        // Brushing the background in subtract mode removes nothing selected.
        apply(&mut doc, &[Point::new(10.0, 10.0)], 5.0, QuickMode::Subtract, Some("qs-sub"));
        assert_eq!(doc.selection.as_ref().unwrap().get(100, 80)[0], 255);
        // Brushing the disc in subtract mode removes it.
        apply(&mut doc, &[Point::new(100.0, 80.0)], 5.0, QuickMode::Subtract, Some("qs-sub2"));
        assert!(doc.selection.as_ref().unwrap().get(100, 80)[0] < 128);
    }
}
