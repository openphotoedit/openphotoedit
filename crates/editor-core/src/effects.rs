//! Layer styles: drop shadow, inner shadow, outer glow, inner glow, bevel and
//! emboss, satin, colour overlay, gradient overlay and stroke.
//!
//! Each effect is derived from the layer's alpha at render resolution:
//! blurs for shadows and glows, an exact Euclidean distance transform for
//! strokes and bevels. Effects composite straight onto the backdrop in
//! Photoshop's order (bottom to top): drop shadow, outer glow, content,
//! satin, colour overlay, gradient overlay, inner glow, inner shadow, bevel,
//! stroke.

use serde::{Deserialize, Serialize};

use crate::adjust::{box_blur_1ch, gradient_at, GradientStop};
use crate::blend::{composite_px, BlendMode};
use crate::color::Rgba8;
use crate::layer::GradientKind;

fn t() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadow {
    #[serde(default = "t")]
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: Rgba8,
    /// 0..1
    pub opacity: f32,
    /// Light angle in degrees (Photoshop: 120 = from the upper left).
    pub angle: f32,
    pub distance: f32,
    /// 0..100 (drop shadow) / choke 0..100 (inner shadow)
    pub spread: f32,
    pub size: f32,
}

impl Shadow {
    fn drop_default() -> Shadow {
        Shadow { enabled: true, blend: BlendMode::Multiply, color: Rgba8::BLACK, opacity: 0.35, angle: 120.0, distance: 5.0, spread: 0.0, size: 5.0 }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow::drop_default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GlowSource {
    #[default]
    Edge,
    Center,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glow {
    #[serde(default = "t")]
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    /// Spread (outer) or choke (inner), 0..100.
    pub spread: f32,
    pub size: f32,
    /// Inner glow only.
    pub source: GlowSource,
}

impl Default for Glow {
    fn default() -> Self {
        Glow { enabled: true, blend: BlendMode::Screen, color: Rgba8::rgb(255, 255, 190), opacity: 0.75, spread: 0.0, size: 5.0, source: GlowSource::Edge }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BevelStyle {
    #[default]
    InnerBevel,
    OuterBevel,
    Emboss,
    PillowEmboss,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BevelTechnique {
    #[default]
    Smooth,
    ChiselHard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bevel {
    #[serde(default = "t")]
    pub enabled: bool,
    pub style: BevelStyle,
    pub technique: BevelTechnique,
    /// 1..1000 (%)
    pub depth: f32,
    /// Down inverts the lighting.
    pub down: bool,
    pub size: f32,
    pub soften: f32,
    pub angle: f32,
    /// Degrees above the horizon, 0..90.
    pub altitude: f32,
    pub highlight_blend: BlendMode,
    pub highlight_color: Rgba8,
    pub highlight_opacity: f32,
    pub shadow_blend: BlendMode,
    pub shadow_color: Rgba8,
    pub shadow_opacity: f32,
}

impl Default for Bevel {
    fn default() -> Self {
        Bevel {
            enabled: true,
            style: BevelStyle::InnerBevel,
            technique: BevelTechnique::Smooth,
            depth: 100.0,
            down: false,
            size: 5.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            highlight_blend: BlendMode::Screen,
            highlight_color: Rgba8::WHITE,
            highlight_opacity: 0.75,
            shadow_blend: BlendMode::Multiply,
            shadow_color: Rgba8::BLACK,
            shadow_opacity: 0.75,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Satin {
    #[serde(default = "t")]
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    pub angle: f32,
    pub distance: f32,
    pub size: f32,
    pub invert: bool,
}

impl Default for Satin {
    fn default() -> Self {
        Satin { enabled: true, blend: BlendMode::Multiply, color: Rgba8::BLACK, opacity: 0.5, angle: 19.0, distance: 11.0, size: 14.0, invert: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorOverlay {
    #[serde(default = "t")]
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
}

impl Default for ColorOverlay {
    fn default() -> Self {
        ColorOverlay { enabled: true, blend: BlendMode::Normal, color: Rgba8::rgb(255, 0, 0), opacity: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientOverlay {
    #[serde(default = "t")]
    pub enabled: bool,
    pub blend: BlendMode,
    pub opacity: f32,
    pub stops: Vec<GradientStop>,
    pub gradient: GradientKind,
    pub angle: f32,
    /// 10..150 (%)
    pub scale: f32,
    pub reverse: bool,
}

impl Default for GradientOverlay {
    fn default() -> Self {
        GradientOverlay {
            enabled: true,
            blend: BlendMode::Normal,
            opacity: 1.0,
            stops: vec![GradientStop { pos: 0.0, color: Rgba8::BLACK }, GradientStop { pos: 1.0, color: Rgba8::WHITE }],
            gradient: GradientKind::Linear,
            angle: 90.0,
            scale: 100.0,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrokePosition {
    #[default]
    Outside,
    Inside,
    Center,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeEffect {
    #[serde(default = "t")]
    pub enabled: bool,
    pub size: f32,
    pub position: StrokePosition,
    pub blend: BlendMode,
    pub opacity: f32,
    pub color: Rgba8,
}

impl Default for StrokeEffect {
    fn default() -> Self {
        StrokeEffect { enabled: true, size: 3.0, position: StrokePosition::Outside, blend: BlendMode::Normal, opacity: 1.0, color: Rgba8::BLACK }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerEffects {
    /// The master switch ("fx" eye in the Layers panel).
    #[serde(default = "t")]
    pub enabled: bool,
    /// Multiplies every size and distance (Layer › Layer Style › Scale Effects).
    pub scale: f32,
    pub drop_shadow: Option<Shadow>,
    pub inner_shadow: Option<Shadow>,
    pub outer_glow: Option<Glow>,
    pub inner_glow: Option<Glow>,
    pub bevel: Option<Bevel>,
    pub satin: Option<Satin>,
    pub color_overlay: Option<ColorOverlay>,
    pub gradient_overlay: Option<GradientOverlay>,
    pub stroke: Option<StrokeEffect>,
}

impl Default for LayerEffects {
    fn default() -> Self {
        LayerEffects {
            enabled: true,
            scale: 1.0,
            drop_shadow: None,
            inner_shadow: None,
            outer_glow: None,
            inner_glow: None,
            bevel: None,
            satin: None,
            color_overlay: None,
            gradient_overlay: None,
            stroke: None,
        }
    }
}

impl LayerEffects {
    pub fn any_active(&self) -> bool {
        self.enabled
            && (self.drop_shadow.as_ref().is_some_and(|e| e.enabled)
                || self.inner_shadow.as_ref().is_some_and(|e| e.enabled)
                || self.outer_glow.as_ref().is_some_and(|e| e.enabled)
                || self.inner_glow.as_ref().is_some_and(|e| e.enabled)
                || self.bevel.as_ref().is_some_and(|e| e.enabled)
                || self.satin.as_ref().is_some_and(|e| e.enabled)
                || self.color_overlay.as_ref().is_some_and(|e| e.enabled)
                || self.gradient_overlay.as_ref().is_some_and(|e| e.enabled)
                || self.stroke.as_ref().is_some_and(|e| e.enabled))
    }

    /// How far effects reach beyond the layer's pixels, in document pixels.
    pub fn reach(&self) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let mut r = 0f32;
        if let Some(s) = self.drop_shadow.as_ref().filter(|e| e.enabled) {
            r = r.max(s.distance + s.size * 1.5 + 2.0);
        }
        if let Some(g) = self.outer_glow.as_ref().filter(|e| e.enabled) {
            r = r.max(g.size * 1.5 + 2.0);
        }
        if let Some(s) = self.stroke.as_ref().filter(|e| e.enabled && e.position != StrokePosition::Inside) {
            r = r.max(s.size + 2.0);
        }
        if let Some(b) = self.bevel.as_ref().filter(|e| e.enabled && matches!(e.style, BevelStyle::OuterBevel | BevelStyle::Emboss | BevelStyle::PillowEmboss)) {
            r = r.max(b.size + b.soften + 2.0);
        }
        r * self.scale.max(0.01)
    }
}

// ---------------------------------------------------------------------------
// Primitives

/// Squared Euclidean distance transform (Felzenszwalb–Huttenlocher) of a
/// binary grid: `inside[i]` cells have distance 0.
pub fn edt(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e20;
    let mut f: Vec<f32> = inside.iter().map(|&b| if b { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let mut d = vec![0f32; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0f32; n + 1];
    let mut col = vec![0f32; n];
    let mut pass = |get: &mut dyn FnMut(usize) -> f32, len: usize, out: &mut [f32]| {
        let mut k = 0usize;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        let fq: Vec<f32> = (0..len).map(|i| get(i)).collect();
        for q in 1..len {
            loop {
                let p = v[k];
                let s = ((fq[q] + (q * q) as f32) - (fq[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                    continue;
                }
                if s <= z[k] {
                    // k == 0 and the new parabola dominates.
                    v[0] = q;
                    z[0] = -INF;
                    z[1] = INF;
                    break;
                }
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = INF;
                break;
            }
        }
        k = 0;
        for q in 0..len {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            d[q] = (q as f32 - p as f32).powi(2) + fq[p];
        }
        out[..len].copy_from_slice(&d[..len]);
    };
    for x in 0..w {
        let mut get = |y: usize| f[y * w + x];
        pass(&mut get, h, &mut col);
        for y in 0..h {
            f[y * w + x] = col[y];
        }
    }
    for y in 0..h {
        let mut get = |x: usize| f[y * w + x];
        pass(&mut get, w, &mut col);
        f[y * w..y * w + w].copy_from_slice(&col[..w]);
    }
    f
}

/// Signed distance to the alpha = 0.5 edge: negative inside, positive
/// outside, with sub-pixel correction from the alpha value itself.
pub fn signed_distance(alpha: &[f32], w: usize, h: usize) -> Vec<f32> {
    let inside: Vec<bool> = alpha.iter().map(|&a| a >= 0.5).collect();
    let outside: Vec<bool> = inside.iter().map(|b| !b).collect();
    let d_out = edt(&inside, w, h);
    let d_in = edt(&outside, w, h);
    alpha
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            // Distance from the nearest opposite cell centre, less half a
            // pixel, refined by the edge pixel's own coverage.
            if inside[i] {
                -(d_in[i].sqrt() - 0.5) - (a - 0.5)
            } else {
                (d_out[i].sqrt() - 0.5) + (0.5 - a)
            }
        })
        .collect()
}

fn gaussianish(buf: &mut [f32], w: usize, h: usize, size: f32) {
    let r = (size / 2.0).round() as usize;
    if r > 0 {
        box_blur_1ch(buf, w, h, r, 3);
    }
}

fn shifted(alpha: &[f32], w: usize, h: usize, dx: f32, dy: f32) -> Vec<f32> {
    let (ix, iy) = (dx.round() as isize, dy.round() as isize);
    let mut out = vec![0f32; alpha.len()];
    for y in 0..h as isize {
        let sy = y - iy;
        if sy < 0 || sy >= h as isize {
            continue;
        }
        for x in 0..w as isize {
            let sx = x - ix;
            if sx < 0 || sx >= w as isize {
                continue;
            }
            out[(y as usize) * w + x as usize] = alpha[sy as usize * w + sx as usize];
        }
    }
    out
}

/// Spread/choke: harden a blurred matte so that `spread`% of it is solid.
fn spread_curve(buf: &mut [f32], spread: f32) {
    let s = (spread / 100.0).clamp(0.0, 0.99);
    if s <= 0.0 {
        return;
    }
    for v in buf.iter_mut() {
        *v = (*v / (1.0 - s)).min(1.0);
    }
}

fn composite_matte(backdrop: &mut [f32], matte: &[f32], color: Rgba8, opacity: f32, mode: BlendMode, strength: &[f32]) {
    let c = color.to_f32();
    let k = opacity * color.alpha_f32();
    for (i, px) in backdrop.chunks_exact_mut(4).enumerate() {
        let a = matte[i] * k * strength[i];
        if a > 0.0 {
            composite_px(px, c, a.min(1.0), mode);
        }
    }
}

/// Geometry of an effects render: the buffer's scale and the layer's
/// document bounds, for gradient overlays.
pub struct EffectCtx {
    /// Output pixels per document pixel.
    pub scale: f32,
    /// Document coordinate of the buffer's top-left pixel edge.
    pub x0: f64,
    pub y0: f64,
    /// Document pixels per buffer pixel.
    pub step: f64,
    /// The layer's content bounds in document pixels.
    pub bounds: (f64, f64, f64, f64),
}

/// Composite a layer with its effects onto `backdrop`.
///
/// `content` is the layer's straight RGBA over the buffer; `strength` is the
/// per-pixel layer opacity times mask (applies to everything);
/// `fill_opacity` scales only the content.
#[allow(clippy::too_many_arguments)]
pub fn composite_with_effects(
    fx: &LayerEffects,
    backdrop: &mut [f32],
    content: &[f32],
    w: usize,
    h: usize,
    strength: &[f32],
    fill_opacity: f32,
    content_blend: BlendMode,
    ctx: &EffectCtx,
) {
    let n = w * h;
    let k = fx.scale.max(0.01) * ctx.scale;
    let alpha: Vec<f32> = content.chunks_exact(4).map(|p| p[3]).collect();
    let light = |angle: f32| {
        let a = angle.to_radians();
        (-a.cos(), a.sin())
    };

    // --- below the content
    if let Some(s) = fx.drop_shadow.as_ref().filter(|e| e.enabled) {
        let (lx, ly) = light(s.angle);
        let mut m = shifted(&alpha, w, h, lx * s.distance * k, ly * s.distance * k);
        gaussianish(&mut m, w, h, s.size * k * (1.0 - s.spread / 100.0));
        spread_curve(&mut m, s.spread);
        composite_matte(backdrop, &m, s.color, s.opacity, s.blend, strength);
    }
    if let Some(g) = fx.outer_glow.as_ref().filter(|e| e.enabled) {
        let mut m = alpha.clone();
        gaussianish(&mut m, w, h, g.size * k * (1.0 - g.spread / 100.0));
        spread_curve(&mut m, g.spread);
        // The glow shows where the content does not cover it.
        for i in 0..n {
            m[i] *= 1.0 - alpha[i];
        }
        composite_matte(backdrop, &m, g.color, g.opacity, g.blend, strength);
    }

    // --- the content itself
    for i in 0..n {
        let o = i * 4;
        let a = content[o + 3] * fill_opacity * strength[i];
        if a > 0.0 {
            composite_px(&mut backdrop[o..o + 4], [content[o], content[o + 1], content[o + 2]], a.min(1.0), content_blend);
        }
    }

    // Interior effects are clipped to the content's alpha.
    let inside_strength: Vec<f32> = (0..n).map(|i| strength[i] * alpha[i]).collect();

    if let Some(s) = fx.satin.as_ref().filter(|e| e.enabled) {
        let (lx, ly) = light(s.angle);
        let d = s.distance * k;
        let a1 = shifted(&alpha, w, h, lx * d, ly * d);
        let a2 = shifted(&alpha, w, h, -lx * d, -ly * d);
        let mut m: Vec<f32> = a1.iter().zip(a2.iter()).map(|(p, q)| (p - q).abs()).collect();
        gaussianish(&mut m, w, h, s.size * k);
        if s.invert {
            m.iter_mut().for_each(|v| *v = 1.0 - *v);
        }
        composite_matte(backdrop, &m, s.color, s.opacity, s.blend, &inside_strength);
    }
    if let Some(o) = fx.color_overlay.as_ref().filter(|e| e.enabled) {
        let ones = vec![1f32; n];
        composite_matte(backdrop, &ones, o.color, o.opacity, o.blend, &inside_strength);
    }
    if let Some(g) = fx.gradient_overlay.as_ref().filter(|e| e.enabled) {
        let (bx, by, bw, bh) = ctx.bounds;
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        let a = (g.angle as f64).to_radians();
        let (dx, dy) = (a.cos(), -a.sin());
        let half = ((bw * dx.abs() + bh * dy.abs()) / 2.0).max(1.0) * (g.scale as f64 / 100.0).max(0.1);
        for j in 0..h {
            let py = ctx.y0 + (j as f64 + 0.5) * ctx.step - cy;
            for i in 0..w {
                let idx = j * w + i;
                let s = inside_strength[idx] * g.opacity;
                if s <= 0.0 {
                    continue;
                }
                let px = ctx.x0 + (i as f64 + 0.5) * ctx.step - cx;
                let tt = match g.gradient {
                    GradientKind::Linear => (px * dx + py * dy) / (2.0 * half) + 0.5,
                    GradientKind::Reflected => ((px * dx + py * dy) / half).abs(),
                    GradientKind::Radial => (px * px + py * py).sqrt() / half,
                    GradientKind::Diamond => (px.abs() + py.abs()) / half,
                    GradientKind::Angle => ((py.atan2(px) - (-dy).atan2(dx)).rem_euclid(std::f64::consts::TAU)) / std::f64::consts::TAU,
                };
                let mut tt = tt.clamp(0.0, 1.0) as f32;
                if g.reverse {
                    tt = 1.0 - tt;
                }
                let c = gradient_at(&g.stops, tt);
                composite_px(&mut backdrop[idx * 4..idx * 4 + 4], [c[0], c[1], c[2]], (c[3] * s).min(1.0), g.blend);
            }
        }
    }
    if let Some(gl) = fx.inner_glow.as_ref().filter(|e| e.enabled) {
        let mut m: Vec<f32> = match gl.source {
            GlowSource::Edge => alpha.iter().map(|a| 1.0 - a).collect(),
            GlowSource::Center => alpha.clone(),
        };
        gaussianish(&mut m, w, h, gl.size * k * (1.0 - gl.spread / 100.0));
        spread_curve(&mut m, gl.spread);
        if gl.source == GlowSource::Center {
            m.iter_mut().for_each(|v| *v = 1.0 - *v);
        }
        composite_matte(backdrop, &m, gl.color, gl.opacity, gl.blend, &inside_strength);
    }
    if let Some(s) = fx.inner_shadow.as_ref().filter(|e| e.enabled) {
        let (lx, ly) = light(s.angle);
        let inv: Vec<f32> = alpha.iter().map(|a| 1.0 - a).collect();
        let mut m = shifted(&inv, w, h, lx * s.distance * k, ly * s.distance * k);
        // Outside the buffer counts as "outside the shape" for the shift.
        gaussianish(&mut m, w, h, s.size * k * (1.0 - s.spread / 100.0));
        spread_curve(&mut m, s.spread);
        composite_matte(backdrop, &m, s.color, s.opacity, s.blend, &inside_strength);
    }
    if let Some(b) = fx.bevel.as_ref().filter(|e| e.enabled) {
        bevel(backdrop, &alpha, w, h, b, k, strength);
    }
    if let Some(st) = fx.stroke.as_ref().filter(|e| e.enabled) {
        let sd = signed_distance(&alpha, w, h);
        let size = st.size * k;
        let (lo, hi) = match st.position {
            StrokePosition::Outside => (0.0, size),
            StrokePosition::Inside => (-size, 0.0),
            StrokePosition::Center => (-size / 2.0, size / 2.0),
        };
        let m: Vec<f32> = sd.iter().map(|&d| ((d - lo + 0.5).clamp(0.0, 1.0)) * ((hi - d + 0.5).clamp(0.0, 1.0))).collect();
        composite_matte(backdrop, &m, st.color, st.opacity, st.blend, strength);
    }
}

fn bevel(backdrop: &mut [f32], alpha: &[f32], w: usize, h: usize, b: &Bevel, k: f32, strength: &[f32]) {
    let size = (b.size * k).max(0.5);
    let sd = signed_distance(alpha, w, h);
    // Height rises from the edge over `size` pixels.
    let mut height: Vec<f32> = sd
        .iter()
        .map(|&d| {
            let t = match b.style {
                BevelStyle::InnerBevel => (-d / size).clamp(0.0, 1.0),
                BevelStyle::OuterBevel => (1.0 - d / size).clamp(0.0, 1.0),
                BevelStyle::Emboss => ((size - d) / (2.0 * size)).clamp(0.0, 1.0),
                BevelStyle::PillowEmboss => {
                    let x = (-d / size).clamp(-1.0, 1.0);
                    1.0 - x.abs()
                }
            };
            match b.technique {
                BevelTechnique::Smooth => {
                    let t2 = t * t * (3.0 - 2.0 * t);
                    t2
                }
                BevelTechnique::ChiselHard => t,
            }
        })
        .collect();
    let soft = (b.soften * k).round() as usize + if b.technique == BevelTechnique::Smooth { (size / 4.0) as usize } else { 0 };
    if soft > 0 {
        box_blur_1ch(&mut height, w, h, soft, 2);
    }
    let depth = b.depth / 100.0 * size;
    let az = b.angle.to_radians();
    let alt = b.altitude.to_radians();
    // Towards the light, in screen space (y down): 120° is up and to the left.
    let (lx, ly, lz) = (az.cos() * alt.cos(), -az.sin() * alt.cos(), alt.sin());
    let flat = lz;
    let sign = if b.down { -1.0 } else { 1.0 };
    let mut hi = vec![0f32; w * h];
    let mut lo = vec![0f32; w * h];
    let region: Vec<f32> = match b.style {
        BevelStyle::InnerBevel => alpha.to_vec(),
        BevelStyle::OuterBevel => alpha.iter().map(|a| 1.0 - a).collect(),
        _ => vec![1.0; w * h],
    };
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let gx = (height[i + 1] - height[i - 1]) * 0.5 * depth * sign;
            let gy = (height[i + w] - height[i - w]) * 0.5 * depth * sign;
            let len = (gx * gx + gy * gy + 1.0).sqrt();
            let (nx, ny, nz) = (-gx / len, -gy / len, 1.0 / len);
            let shade = nx * lx + ny * ly + nz * lz;
            let d = shade - flat;
            let r = region[i];
            if d > 0.0 {
                hi[i] = (d / (1.0 - flat).max(1e-3)).min(1.0) * r;
            } else {
                lo[i] = (-d / flat.max(1e-3)).min(1.0) * r;
            }
        }
    }
    composite_matte(backdrop, &hi, b.highlight_color, b.highlight_opacity, b.highlight_blend, strength);
    composite_matte(backdrop, &lo, b.shadow_color, b.shadow_opacity, b.shadow_blend, strength);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(w: usize, h: usize, r: (usize, usize, usize, usize)) -> Vec<f32> {
        let mut c = vec![0f32; w * h * 4];
        for y in r.1..r.1 + r.3 {
            for x in r.0..r.0 + r.2 {
                let o = (y * w + x) * 4;
                c[o..o + 4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
            }
        }
        c
    }

    fn ctx(w: usize, h: usize) -> EffectCtx {
        EffectCtx { scale: 1.0, x0: 0.0, y0: 0.0, step: 1.0, bounds: (0.0, 0.0, w as f64, h as f64) }
    }

    #[test]
    fn edt_distances() {
        let w = 7;
        let mut inside = vec![false; w];
        inside[3] = true;
        let d = edt(&inside, w, 1);
        assert_eq!(d, vec![9.0, 4.0, 1.0, 0.0, 1.0, 4.0, 9.0]);
    }

    #[test]
    fn stroke_outside_has_width() {
        let (w, h) = (40, 40);
        let content = square(w, h, (10, 10, 20, 20));
        let mut back = vec![0f32; w * h * 4];
        let fx = LayerEffects { stroke: Some(StrokeEffect { size: 4.0, color: Rgba8::rgb(255, 0, 0), ..Default::default() }), ..Default::default() };
        composite_with_effects(&fx, &mut back, &content, w, h, &vec![1.0; w * h], 1.0, BlendMode::Normal, &ctx(w, h));
        let at = |x: usize, y: usize| &back[(y * w + x) * 4..(y * w + x) * 4 + 4];
        assert!(at(8, 20)[0] > 0.9 && at(8, 20)[1] < 0.1, "stroke 2px outside the left edge: {:?}", at(8, 20));
        assert!(at(4, 20)[3] < 0.05, "nothing 6px out");
        assert!(at(20, 20)[1] > 0.9, "content stays white inside");
    }

    #[test]
    fn drop_shadow_is_offset_down_right_for_120_degrees() {
        let (w, h) = (60, 60);
        let content = square(w, h, (20, 20, 10, 10));
        let mut back = vec![0f32; w * h * 4];
        let fx = LayerEffects {
            drop_shadow: Some(Shadow { distance: 10.0, size: 0.0, opacity: 1.0, blend: BlendMode::Normal, ..Default::default() }),
            ..Default::default()
        };
        composite_with_effects(&fx, &mut back, &content, w, h, &vec![1.0; w * h], 1.0, BlendMode::Normal, &ctx(w, h));
        // angle 120: light from upper left, shadow to the lower right.
        let a = |x: usize, y: usize| back[(y * w + x) * 4 + 3];
        let (dx, dy) = ((-(120f32.to_radians().cos()) * 10.0).round() as usize, ((120f32.to_radians().sin()) * 10.0).round() as usize);
        assert!(a(25 + dx, 25 + dy) > 0.9 || a(25 + dx, 25 + dy) > 0.5);
        assert!(a(18, 18) < 0.01, "no shadow above-left");
    }

    #[test]
    fn color_overlay_replaces_colour_inside_only() {
        let (w, h) = (20, 20);
        let content = square(w, h, (5, 5, 10, 10));
        let mut back = vec![0f32; w * h * 4];
        let fx = LayerEffects { color_overlay: Some(ColorOverlay { color: Rgba8::rgb(0, 0, 255), ..Default::default() }), ..Default::default() };
        composite_with_effects(&fx, &mut back, &content, w, h, &vec![1.0; w * h], 1.0, BlendMode::Normal, &ctx(w, h));
        let p = &back[(10 * w + 10) * 4..(10 * w + 10) * 4 + 4];
        assert!(p[2] > 0.99 && p[0] < 0.01);
        assert_eq!(back[3], 0.0);
    }

    #[test]
    fn bevel_lights_upper_left_edge() {
        let (w, h) = (60, 60);
        let mut content = square(w, h, (10, 10, 40, 40));
        for p in content.chunks_exact_mut(4) {
            if p[3] > 0.0 {
                p[..3].copy_from_slice(&[0.5, 0.5, 0.5]);
            }
        }
        let mut back = vec![0f32; w * h * 4];
        let fx = LayerEffects { bevel: Some(Bevel { size: 8.0, ..Default::default() }), ..Default::default() };
        composite_with_effects(&fx, &mut back, &content, w, h, &vec![1.0; w * h], 1.0, BlendMode::Normal, &ctx(w, h));
        let lum = |x: usize, y: usize| back[(y * w + x) * 4];
        assert!(lum(13, 30) > lum(46, 30), "left edge lit ({}) vs right edge ({})", lum(13, 30), lum(46, 30));
    }
}
