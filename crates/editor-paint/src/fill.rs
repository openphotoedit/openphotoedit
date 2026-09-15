//! Paint bucket, magic eraser and gradient.

use editor_core::adjust::{gradient_at, GradientStop};
use editor_core::blend::{composite_px, dissolve_noise, BlendMode};
use editor_core::color::Rgba8;
use editor_core::document::Document;
use editor_core::geom::{Point, Rect};
use editor_core::layer::{GradientKind, LayerId};
use editor_core::ops::{target_id, Applied, EditorError, Result};
use editor_core::pixels::read_selection;
use editor_core::selection;
use editor_select::sample::Sampler;
use editor_select::wand::{self, Region};
use serde::Deserialize;

use crate::surface::{self, Target};

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}
fn tol() -> f32 {
    32.0
}
fn four() -> u8 {
    4
}

#[derive(Debug, Deserialize)]
pub struct FillCmd {
    #[serde(default)]
    pub id: Option<LayerId>,
    pub x: f64,
    pub y: f64,
    #[serde(default = "tol")]
    pub tolerance: f32,
    #[serde(default = "yes")]
    pub contiguous: bool,
    #[serde(default)]
    pub sample_all: bool,
    #[serde(default = "yes")]
    pub anti_alias: bool,
    #[serde(default)]
    pub color: Option<Rgba8>,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default = "four")]
    pub connectivity: u8,
    /// Magic eraser on a transparency-locked layer paints this instead.
    #[serde(default)]
    pub background: Option<Rgba8>,
}

fn region(doc: &Document, id: LayerId, cmd: &FillCmd) -> Result<Region> {
    let (px, py) = (cmd.x.floor() as i32, cmd.y.floor() as i32);
    if px < 0 || py < 0 || px >= doc.width as i32 || py >= doc.height as i32 {
        return Err(EditorError::Invalid("click inside the image".into()));
    }
    let t = cmd.tolerance.round().clamp(0.0, 255.0) as u8;
    let mut s = Sampler::new(doc, Some(id), cmd.sample_all);
    let seed = wand::seed_color(&mut s, px, py, 1);
    let mut reg = if cmd.contiguous { wand::contiguous(&mut s, px, py, seed, t, cmd.connectivity == 8) } else { wand::global(&s, seed, t) };
    if cmd.anti_alias {
        wand::antialias(&mut reg);
    }
    Ok(reg)
}

pub fn fill(doc: &mut Document, cmd: FillCmd, erase: bool) -> Result<Applied> {
    let id = target_id(doc, cmd.id)?;
    surface::check(doc, id, Target::Pixels)?;
    let color = cmd.color.unwrap_or(Rgba8::BLACK);
    if !erase && cmd.color.is_none() {
        return Err(EditorError::Invalid("the paint bucket needs a colour".into()));
    }
    let reg = region(doc, id, &cmd)?;
    let rect = reg.bounds.intersect(&selection::bounds(doc));
    if rect.is_empty() {
        return Ok(Applied::quiet());
    }
    let lock_alpha = doc.find(id).is_some_and(|l| l.locks.transparency);
    let bg = cmd.background.unwrap_or(Rgba8::WHITE).to_f32();
    let cs = color.to_f32();
    let cov = reg.read(rect);
    let sel = read_selection(doc, rect);
    let mut out = surface::read(doc, id, Target::Pixels, rect)?;
    let opacity = cmd.opacity.clamp(0.0, 1.0) * if erase { 1.0 } else { color.alpha_f32() };
    for i in 0..cov.len() {
        let a = cov[i] as f32 / 255.0 * sel[i] as f32 / 255.0 * opacity;
        if a <= 0.0 {
            continue;
        }
        let o = i * 4;
        let mut d = [out[o] as f32 / 255.0, out[o + 1] as f32 / 255.0, out[o + 2] as f32 / 255.0, out[o + 3] as f32 / 255.0];
        if erase {
            if lock_alpha {
                composite_px(&mut d, bg, a, BlendMode::Normal);
            } else {
                d[3] *= 1.0 - a;
            }
        } else {
            composite_px(&mut d, cs, a, cmd.blend);
        }
        for k in 0..4 {
            out[o + k] = (d[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    surface::write(doc, id, Target::Pixels, rect, &out)?;
    Ok(Applied::step(if erase { "Magic Eraser" } else { "Paint Bucket" }).dirty(rect))
}

#[derive(Debug, Deserialize)]
pub struct GradientCmd {
    #[serde(default)]
    pub id: Option<LayerId>,
    #[serde(default)]
    pub target: Target,
    pub from: Point,
    pub to: Point,
    #[serde(default)]
    pub gradient: GradientKind,
    pub stops: Vec<GradientStop>,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default)]
    pub reverse: bool,
    /// Break up 8-bit banding with ±½ level of noise.
    #[serde(default = "yes")]
    pub dither: bool,
}

/// Position 0..1 along the gradient for a pixel centre offset `(px, py)` from
/// the start point. Same geometry as gradient fill layers.
#[inline]
fn gradient_t(kind: GradientKind, px: f64, py: f64, dx: f64, dy: f64) -> f64 {
    let len2 = (dx * dx + dy * dy).max(1e-9);
    let len = len2.sqrt();
    match kind {
        GradientKind::Linear => (px * dx + py * dy) / len2,
        GradientKind::Reflected => ((px * dx + py * dy) / len2).abs(),
        GradientKind::Radial => (px * px + py * py).sqrt() / len,
        GradientKind::Diamond => {
            let u = (px * dx + py * dy) / len;
            let v = (-px * dy + py * dx) / len;
            u.abs().max(v.abs()) / len
        }
        GradientKind::Angle => {
            let a0 = dy.atan2(dx);
            let a = py.atan2(px);
            (a - a0).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
        }
    }
}

pub fn gradient(doc: &mut Document, cmd: GradientCmd) -> Result<Applied> {
    let id = target_id(doc, cmd.id)?;
    surface::check(doc, id, cmd.target)?;
    if cmd.stops.is_empty() {
        return Err(EditorError::Invalid("a gradient needs at least one colour stop".into()));
    }
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let rect = selection::bounds(doc).intersect(&canvas);
    if rect.is_empty() {
        return Ok(Applied::quiet());
    }
    const N: usize = 1024;
    let table: Vec<[f32; 4]> = (0..N).map(|i| gradient_at(&cmd.stops, i as f32 / (N - 1) as f32)).collect();
    let (dx, dy) = (cmd.to.x - cmd.from.x, cmd.to.y - cmd.from.y);
    let opacity = cmd.opacity.clamp(0.0, 1.0);
    let mask = cmd.target == Target::Mask;
    let strip = 256;
    let mut y0 = rect.y;
    while y0 < rect.bottom() {
        let rows = strip.min(rect.bottom() - y0);
        let r = Rect::new(rect.x, y0, rect.w, rows);
        let mut out = surface::read(doc, id, cmd.target, r)?;
        let sel = read_selection(doc, r);
        for j in 0..rows {
            let py = (y0 + j) as f64 + 0.5 - cmd.from.y;
            for i in 0..rect.w {
                let k = (j * rect.w + i) as usize;
                let s = sel[k] as f32 / 255.0;
                if s <= 0.0 {
                    continue;
                }
                let px = (rect.x + i) as f64 + 0.5 - cmd.from.x;
                let mut t = gradient_t(cmd.gradient, px, py, dx, dy).clamp(0.0, 1.0) as f32;
                if cmd.reverse {
                    t = 1.0 - t;
                }
                let f = t * (N - 1) as f32;
                let lo = (f as usize).min(N - 2);
                let w = f - lo as f32;
                let mut c: [f32; 4] = std::array::from_fn(|q| table[lo][q] + (table[lo + 1][q] - table[lo][q]) * w);
                if cmd.dither {
                    let n = (dissolve_noise((rect.x + i) as i64, (y0 + j) as i64) - 0.5) / 255.0;
                    for v in c.iter_mut().take(3) {
                        *v += n;
                    }
                }
                let a = c[3] * opacity * s;
                if a <= 0.0 {
                    continue;
                }
                let o = k * 4;
                let mut d = [out[o] as f32 / 255.0, out[o + 1] as f32 / 255.0, out[o + 2] as f32 / 255.0, out[o + 3] as f32 / 255.0];
                if mask {
                    let g = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
                    let v = d[0] + (g - d[0]) * a;
                    d = [v, v, v, 1.0];
                } else {
                    composite_px(&mut d, [c[0], c[1], c[2]], a, cmd.blend);
                }
                for q in 0..4 {
                    out[o + q] = (d[q].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        surface::write(doc, id, cmd.target, r, &out)?;
        y0 += rows;
    }
    Ok(Applied::step("Gradient").dirty(rect))
}
