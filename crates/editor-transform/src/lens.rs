//! Lens correction about the canvas centre: Brown–Conrady radial distortion
//! (k1), lateral chromatic aberration as a per-channel radial scale, and a
//! vignette in linear light.

use editor_core::color::{linear_to_srgb, srgb_to_linear};
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{LayerId, LayerKind};
use editor_core::ops::{EditorError, Result};
use editor_core::pixels::{edit_layer, EditScope};
use editor_core::transform::Resample;

use crate::sample::{finish, Source};

#[derive(Clone, Copy, Debug)]
pub struct LensParams {
    /// -100..100; positive removes barrel distortion.
    pub distortion: f64,
    /// -100..100: red/cyan and blue/yellow fringe.
    pub chromatic_rc: f64,
    pub chromatic_by: f64,
    /// -100..100; negative darkens the corners.
    pub vignette: f64,
    /// 0..100.
    pub vignette_midpoint: f64,
    /// Percent; above 100 zooms in.
    pub scale: f64,
    pub auto_scale: bool,
}

/// Strongest k1 at ±100.
const K_MAX: f64 = 0.3;
const CA_MAX: f64 = 0.004;

impl LensParams {
    pub fn k1(&self) -> f64 {
        -self.distortion.clamp(-100.0, 100.0) / 100.0 * K_MAX
    }

    pub fn is_identity(&self) -> bool {
        self.distortion == 0.0 && self.chromatic_rc == 0.0 && self.chromatic_by == 0.0 && self.vignette == 0.0 && self.effective_scale() == 1.0
    }

    /// Source radius per output radius from the scale setting.
    pub fn effective_scale(&self) -> f64 {
        if self.auto_scale {
            let k = self.k1();
            let ca = 1.0 + (self.chromatic_rc.abs().max(self.chromatic_by.abs()) / 100.0 * CA_MAX);
            // The corner must sample inside the frame: s(1 + k s²)·ca ≤ 1.
            let f = |s: f64| s * (1.0 + k * s * s) * ca;
            if f(1.0) <= 1.0 {
                return 1.0;
            }
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if f(mid) <= 1.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lo
        } else {
            100.0 / self.scale.clamp(10.0, 1000.0)
        }
    }
}

pub fn apply(doc: &mut Document, id: LayerId, p: LensParams) -> Result<Rect> {
    let l = doc.find(id).ok_or(EditorError::NoLayer(id))?;
    let raster = match &l.kind {
        LayerKind::Pixel(r) => r.clone(),
        LayerKind::Text { .. } | LayerKind::Shape { .. } | LayerKind::Smart { .. } => {
            return Err(EditorError::Invalid(format!("`{}` is a {} layer; rasterize it first", l.name, l.kind.name())))
        }
        _ => return Err(EditorError::Invalid(format!("`{}` has no pixels to correct", l.name))),
    };
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let rect = if doc.selection.is_some() { editor_core::selection::bounds(doc) } else { raster.doc_rect() }.intersect(&canvas);
    if rect.is_empty() || p.is_identity() {
        return Ok(Rect::empty());
    }
    let src = Source::from_raster(&raster);
    let (cx, cy) = (doc.width as f64 / 2.0, doc.height as f64 / 2.0);
    let norm = cx.hypot(cy);
    let k = p.k1();
    let s = p.effective_scale();
    let cr = 1.0 + p.chromatic_rc.clamp(-100.0, 100.0) / 100.0 * CA_MAX;
    let cb = 1.0 + p.chromatic_by.clamp(-100.0, 100.0) / 100.0 * CA_MAX;
    let vig = p.vignette.clamp(-100.0, 100.0) / 100.0 * 2.0;
    let m0 = p.vignette_midpoint.clamp(0.0, 100.0) / 100.0 * 0.9;
    let ca = cr != 1.0 || cb != 1.0;
    edit_layer(doc, id, EditScope { rect, apron: 0 }, |buf, w, _h| {
        for (i, px) in buf.chunks_exact_mut(4).enumerate() {
            let x = rect.x as f64 + (i % w) as f64 + 0.5;
            let y = rect.y as f64 + (i / w) as f64 + 0.5;
            let (dx, dy) = ((x - cx) / norm * s, (y - cy) / norm * s);
            let r2 = dx * dx + dy * dy;
            let f = 1.0 + k * r2;
            // Local radial magnification, for antialiasing strong squeezes.
            let g = s * (1.0 + 3.0 * k * r2).abs().max(1e-3);
            let jac = [g, 0.0, 0.0, g];
            let (sx, sy) = (cx + dx * f * norm, cy + dy * f * norm);
            let mut acc = src.sample(sx, sy, &jac, Resample::Bicubic);
            if ca {
                let red = src.sample(cx + dx * f * cr * norm, cy + dy * f * cr * norm, &jac, Resample::Bicubic);
                let blue = src.sample(cx + dx * f * cb * norm, cy + dy * f * cb * norm, &jac, Resample::Bicubic);
                if acc[3] > 0.0 {
                    if red[3] > 0.0 {
                        acc[0] = red[0] / red[3] * acc[3];
                    }
                    if blue[3] > 0.0 {
                        acc[2] = blue[2] / blue[3] * acc[3];
                    }
                }
            }
            let mut out = finish(acc, 4);
            if vig != 0.0 && out[3] > 0 {
                let rs = (r2.sqrt() * f).min(1.5);
                let t = ((rs - m0) / (1.0 - m0).max(1e-3)).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t);
                let gain = 2f64.powf(vig * t) as f32;
                for c in &mut out[..3] {
                    *c = (linear_to_srgb(srgb_to_linear(*c as f32 / 255.0) * gain) * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            px.copy_from_slice(&out);
        }
    })
}
