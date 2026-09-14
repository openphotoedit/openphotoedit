//! Adjustments: the parameter types adjustment layers carry, and the maths
//! that applies them to a float RGBA buffer.
//!
//! Every adjustment runs through [`Adjustment::apply`], which receives the
//! whole buffer plus where it sits in the document. Point adjustments ignore
//! the geometry; spatial ones (clarity, dehaze, vignette, grain) need it so a
//! zoomed-out preview and the full-resolution export agree.

use serde::{Deserialize, Serialize};

use crate::color::{
    hsl_to_rgb, linear_to_srgb, luma, rgb_to_hsl, smoothstep, srgb_to_linear, Rgba8,
};

/// Where a buffer sits in the document.
#[derive(Clone, Copy, Debug)]
pub struct ApplyCtx {
    /// Document coordinate of the buffer's top-left pixel edge.
    pub x0: f64,
    pub y0: f64,
    /// Document pixels per buffer pixel.
    pub step: f64,
    pub doc_w: u32,
    pub doc_h: u32,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Adjustment {
    BrightnessContrast(BrightnessContrast),
    Levels(Levels),
    Curves(Curves),
    Exposure(Exposure),
    Vibrance(Vibrance),
    HueSaturation(HueSaturation),
    ColorBalance(ColorBalance),
    BlackWhite(BlackWhite),
    PhotoFilter(PhotoFilter),
    ChannelMixer(ChannelMixer),
    Invert,
    Posterize { levels: u32 },
    Threshold { level: u8 },
    GradientMap(GradientMap),
    SelectiveColor(SelectiveColor),
    /// The photographer's panel: exposure, tone, white balance, presence and
    /// effects in one pass. Lite's master sliders drive this.
    Develop(Develop),
    /// A 3D lookup table, `size`³ RGB entries in 0..1.
    ColorLookup(ColorLookup),
}

impl Adjustment {
    pub fn label(&self) -> &'static str {
        match self {
            Adjustment::BrightnessContrast(_) => "Brightness/Contrast",
            Adjustment::Levels(_) => "Levels",
            Adjustment::Curves(_) => "Curves",
            Adjustment::Exposure(_) => "Exposure",
            Adjustment::Vibrance(_) => "Vibrance",
            Adjustment::HueSaturation(_) => "Hue/Saturation",
            Adjustment::ColorBalance(_) => "Color Balance",
            Adjustment::BlackWhite(_) => "Black & White",
            Adjustment::PhotoFilter(_) => "Photo Filter",
            Adjustment::ChannelMixer(_) => "Channel Mixer",
            Adjustment::Invert => "Invert",
            Adjustment::Posterize { .. } => "Posterize",
            Adjustment::Threshold { .. } => "Threshold",
            Adjustment::GradientMap(_) => "Gradient Map",
            Adjustment::SelectiveColor(_) => "Selective Color",
            Adjustment::Develop(_) => "Develop",
            Adjustment::ColorLookup(_) => "Color Lookup",
        }
    }

    /// Default parameters for a kind name, as the UI's "new adjustment layer"
    /// menu offers them.
    pub fn default_for(kind: &str) -> Option<Adjustment> {
        Some(match kind {
            "brightness-contrast" => Adjustment::BrightnessContrast(Default::default()),
            "levels" => Adjustment::Levels(Default::default()),
            "curves" => Adjustment::Curves(Default::default()),
            "exposure" => Adjustment::Exposure(Default::default()),
            "vibrance" => Adjustment::Vibrance(Default::default()),
            "hue-saturation" => Adjustment::HueSaturation(Default::default()),
            "color-balance" => Adjustment::ColorBalance(Default::default()),
            "black-white" => Adjustment::BlackWhite(Default::default()),
            "photo-filter" => Adjustment::PhotoFilter(Default::default()),
            "channel-mixer" => Adjustment::ChannelMixer(Default::default()),
            "invert" => Adjustment::Invert,
            "posterize" => Adjustment::Posterize { levels: 4 },
            "threshold" => Adjustment::Threshold { level: 128 },
            "gradient-map" => Adjustment::GradientMap(Default::default()),
            "selective-color" => Adjustment::SelectiveColor(Default::default()),
            "develop" => Adjustment::Develop(Default::default()),
            _ => return None,
        })
    }

    /// Apply to a straight-alpha RGBA float buffer. Alpha is left alone.
    pub fn apply(&self, buf: &mut [f32], ctx: &ApplyCtx) {
        match self {
            Adjustment::BrightnessContrast(p) => p.apply(buf),
            Adjustment::Levels(p) => per_channel_lut(buf, &p.luts()),
            Adjustment::Curves(p) => per_channel_lut(buf, &p.luts()),
            Adjustment::Exposure(p) => p.apply(buf),
            Adjustment::Vibrance(p) => p.apply(buf),
            Adjustment::HueSaturation(p) => p.apply(buf),
            Adjustment::ColorBalance(p) => p.apply(buf),
            Adjustment::BlackWhite(p) => p.apply(buf),
            Adjustment::PhotoFilter(p) => p.apply(buf),
            Adjustment::ChannelMixer(p) => p.apply(buf),
            Adjustment::Invert => {
                for px in buf.chunks_exact_mut(4) {
                    for c in &mut px[..3] {
                        *c = 1.0 - *c;
                    }
                }
            }
            Adjustment::Posterize { levels } => {
                let n = (*levels).clamp(2, 255) as f32;
                for px in buf.chunks_exact_mut(4) {
                    for c in &mut px[..3] {
                        *c = ((*c * n).floor().min(n - 1.0)) / (n - 1.0);
                    }
                }
            }
            Adjustment::Threshold { level } => {
                let t = *level as f32 / 255.0;
                for px in buf.chunks_exact_mut(4) {
                    let v = if luma([px[0], px[1], px[2]]) >= t { 1.0 } else { 0.0 };
                    px[..3].fill(v);
                }
            }
            Adjustment::GradientMap(p) => p.apply(buf),
            Adjustment::SelectiveColor(p) => p.apply(buf),
            Adjustment::Develop(p) => p.apply(buf, ctx),
            Adjustment::ColorLookup(p) => p.apply(buf),
        }
    }
}

// ---------------------------------------------------------------------------
// Lookup tables

const LUT_SIZE: usize = 1024;

/// A 1D curve sampled at `LUT_SIZE` points over 0..1.
#[derive(Clone)]
pub struct Lut(Vec<f32>);

impl Lut {
    pub fn identity() -> Lut {
        Lut((0..LUT_SIZE).map(|i| i as f32 / (LUT_SIZE - 1) as f32).collect())
    }
    pub fn from_fn(f: impl Fn(f32) -> f32) -> Lut {
        Lut((0..LUT_SIZE).map(|i| f(i as f32 / (LUT_SIZE - 1) as f32)).collect())
    }
    #[inline]
    pub fn eval(&self, v: f32) -> f32 {
        let x = v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32;
        let i = x as usize;
        if i >= LUT_SIZE - 1 {
            return self.0[LUT_SIZE - 1];
        }
        let t = x - i as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * t
    }
    /// `outer(self(v))`.
    pub fn then(&self, outer: &Lut) -> Lut {
        Lut(self.0.iter().map(|&v| outer.eval(v)).collect())
    }
}

/// `[master, red, green, blue]`: each channel goes through its own table,
/// then through the master.
fn per_channel_lut(buf: &mut [f32], luts: &[Lut; 3]) {
    for px in buf.chunks_exact_mut(4) {
        for c in 0..3 {
            px[c] = luts[c].eval(px[c]);
        }
    }
}

// ---------------------------------------------------------------------------
// Brightness / Contrast

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightnessContrast {
    /// -150..150
    pub brightness: f32,
    /// -50..100
    pub contrast: f32,
    /// Photoshop's pre-CS3 linear behaviour.
    pub legacy: bool,
}

impl BrightnessContrast {
    fn apply(&self, buf: &mut [f32]) {
        let b = self.brightness / 150.0;
        let c = self.contrast / 100.0;
        let lut = if self.legacy {
            Lut::from_fn(|v| {
                let v = v + b * 150.0 / 255.0;
                ((v - 0.5) * (1.0 + c) + 0.5).clamp(0.0, 1.0)
            })
        } else {
            Lut::from_fn(|v| {
                // Brightness bends the curve (keeping black and white fixed),
                // contrast is an S-curve around the midpoint.
                let g = if b >= 0.0 { 1.0 / (1.0 + 1.5 * b) } else { 1.0 - 1.5 * b };
                let v = v.powf(g);
                let k = if c >= 0.0 { 1.0 + 3.0 * c } else { 1.0 / (1.0 - 1.5 * c) };
                s_curve(v, k)
            })
        };
        per_channel_lut(buf, &[lut.clone(), lut.clone(), lut]);
    }
}

/// A symmetric contrast curve through (0,0), (0.5,0.5), (1,1); `k > 1` adds
/// contrast.
fn s_curve(v: f32, k: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v < 0.5 {
        0.5 * (2.0 * v).powf(k)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - v)).powf(k)
    }
}

// ---------------------------------------------------------------------------
// Levels

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LevelsChannel {
    pub in_black: f32,
    pub in_white: f32,
    pub gamma: f32,
    pub out_black: f32,
    pub out_white: f32,
}

impl Default for LevelsChannel {
    fn default() -> Self {
        Self { in_black: 0.0, in_white: 255.0, gamma: 1.0, out_black: 0.0, out_white: 255.0 }
    }
}

impl LevelsChannel {
    pub fn lut(&self) -> Lut {
        let ib = self.in_black / 255.0;
        let iw = (self.in_white / 255.0).max(ib + 1e-3);
        let g = self.gamma.clamp(0.01, 9.99);
        let ob = self.out_black / 255.0;
        let ow = self.out_white / 255.0;
        Lut::from_fn(|v| {
            let t = ((v - ib) / (iw - ib)).clamp(0.0, 1.0).powf(1.0 / g);
            ob + (ow - ob) * t
        })
    }
    pub fn is_identity(&self) -> bool {
        *self == LevelsChannel::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub master: LevelsChannel,
    pub red: LevelsChannel,
    pub green: LevelsChannel,
    pub blue: LevelsChannel,
}

impl Levels {
    fn luts(&self) -> [Lut; 3] {
        let m = self.master.lut();
        [self.red.lut().then(&m), self.green.lut().then(&m), self.blue.lut().then(&m)]
    }
}

// ---------------------------------------------------------------------------
// Curves

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoints(pub Vec<[f32; 2]>);

impl Default for CurvePoints {
    fn default() -> Self {
        CurvePoints(vec![[0.0, 0.0], [255.0, 255.0]])
    }
}

impl CurvePoints {
    /// Monotone cubic (Fritsch–Carlson) through the control points, so a
    /// curve never overshoots into posterisation between points.
    pub fn lut(&self) -> Lut {
        let mut pts: Vec<[f32; 2]> = self.0.iter().map(|p| [p[0] / 255.0, p[1] / 255.0]).collect();
        pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6);
        if pts.len() < 2 {
            return Lut::identity();
        }
        let n = pts.len();
        let mut d = vec![0f32; n - 1];
        for i in 0..n - 1 {
            d[i] = (pts[i + 1][1] - pts[i][1]) / (pts[i + 1][0] - pts[i][0]);
        }
        let mut m = vec![0f32; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
        }
        for i in 0..n - 1 {
            if d[i].abs() < 1e-9 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
                continue;
            }
            let a = m[i] / d[i];
            let b = m[i + 1] / d[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                m[i] = t * a * d[i];
                m[i + 1] = t * b * d[i];
            }
        }
        Lut::from_fn(|x| {
            if x <= pts[0][0] {
                return pts[0][1].clamp(0.0, 1.0);
            }
            if x >= pts[n - 1][0] {
                return pts[n - 1][1].clamp(0.0, 1.0);
            }
            let mut i = 0;
            while i + 1 < n - 1 && x > pts[i + 1][0] {
                i += 1;
            }
            let h = pts[i + 1][0] - pts[i][0];
            let t = (x - pts[i][0]) / h;
            let t2 = t * t;
            let t3 = t2 * t;
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * pts[i][1]
                + (t3 - 2.0 * t2 + t) * h * m[i]
                + (-2.0 * t3 + 3.0 * t2) * pts[i + 1][1]
                + (t3 - t2) * h * m[i + 1];
            y.clamp(0.0, 1.0)
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curves {
    pub master: CurvePoints,
    pub red: CurvePoints,
    pub green: CurvePoints,
    pub blue: CurvePoints,
}

impl Curves {
    fn luts(&self) -> [Lut; 3] {
        let m = self.master.lut();
        [self.red.lut().then(&m), self.green.lut().then(&m), self.blue.lut().then(&m)]
    }
}

// ---------------------------------------------------------------------------
// Exposure

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exposure {
    /// Stops, -20..20
    pub exposure: f32,
    /// -0.5..0.5, added in linear light
    pub offset: f32,
    /// 0.01..9.99
    pub gamma: f32,
}

impl Default for Exposure {
    fn default() -> Self {
        Self { exposure: 0.0, offset: 0.0, gamma: 1.0 }
    }
}

impl Exposure {
    fn apply(&self, buf: &mut [f32]) {
        let mul = 2f32.powf(self.exposure);
        let g = 1.0 / self.gamma.clamp(0.01, 9.99);
        let off = self.offset;
        let lut = Lut::from_fn(|v| {
            let lin = (srgb_to_linear(v) * mul + off).max(0.0);
            linear_to_srgb(lin.powf(g)).clamp(0.0, 1.0)
        });
        per_channel_lut(buf, &[lut.clone(), lut.clone(), lut]);
    }
}

// ---------------------------------------------------------------------------
// Vibrance / Saturation

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vibrance {
    /// -100..100
    pub vibrance: f32,
    /// -100..100
    pub saturation: f32,
}

impl Vibrance {
    fn apply(&self, buf: &mut [f32]) {
        for px in buf.chunks_exact_mut(4) {
            let c = [px[0], px[1], px[2]];
            let out = vibrance_saturation(c, self.vibrance / 100.0, self.saturation / 100.0);
            px[..3].copy_from_slice(&out);
        }
    }
}

/// Saturation scales chroma about luma; vibrance does too, but weighted
/// towards colours that are not yet saturated, and eased off skin-like
/// orange hues so faces do not go red.
pub fn vibrance_saturation(c: [f32; 3], vib: f32, sat: f32) -> [f32; 3] {
    let l = luma(c);
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let s = mx - mn;
    let mut k = 1.0 + sat;
    if vib != 0.0 {
        let skin = if c[0] >= c[1] && c[1] >= c[2] { 0.5 } else { 1.0 };
        let v = if vib > 0.0 { vib * (1.0 - s) * skin } else { vib };
        k *= 1.0 + v;
    }
    let k = k.max(0.0);
    [
        (l + (c[0] - l) * k).clamp(0.0, 1.0),
        (l + (c[1] - l) * k).clamp(0.0, 1.0),
        (l + (c[2] - l) * k).clamp(0.0, 1.0),
    ]
}

// ---------------------------------------------------------------------------
// Hue / Saturation

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HslShift {
    /// -180..180
    pub hue: f32,
    /// -100..100
    pub saturation: f32,
    /// -100..100
    pub lightness: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HueSaturation {
    pub master: HslShift,
    /// Reds, yellows, greens, cyans, blues, magentas.
    pub ranges: [HslShift; 6],
    pub colorize: bool,
    /// Colorize parameters: hue 0..360, saturation 0..100, lightness -100..100.
    pub colorize_hue: f32,
    pub colorize_saturation: f32,
    pub colorize_lightness: f32,
}

impl Default for HueSaturation {
    fn default() -> Self {
        Self {
            master: HslShift::default(),
            ranges: Default::default(),
            colorize: false,
            colorize_hue: 0.0,
            colorize_saturation: 25.0,
            colorize_lightness: 0.0,
        }
    }
}

fn apply_lightness(c: [f32; 3], l: f32) -> [f32; 3] {
    if l > 0.0 {
        c.map(|v| v + (1.0 - v) * l)
    } else {
        c.map(|v| v * (1.0 + l))
    }
}

impl HueSaturation {
    fn apply(&self, buf: &mut [f32]) {
        if self.colorize {
            let h = self.colorize_hue;
            let s = self.colorize_saturation / 100.0;
            let l = self.colorize_lightness / 100.0;
            for px in buf.chunks_exact_mut(4) {
                let y = luma([px[0], px[1], px[2]]);
                let rgb = hsl_to_rgb([h, s, y]);
                let out = apply_lightness(rgb, l);
                px[..3].copy_from_slice(&out);
            }
            return;
        }
        let centres = [0.0f32, 60.0, 120.0, 180.0, 240.0, 300.0];
        let any_range = self.ranges.iter().any(|r| *r != HslShift::default());
        for px in buf.chunks_exact_mut(4) {
            let mut hsl = rgb_to_hsl([px[0], px[1], px[2]]);
            let (mut dh, mut ds, mut dl) = (self.master.hue, self.master.saturation, self.master.lightness);
            if any_range && hsl[1] > 0.0 {
                for (i, r) in self.ranges.iter().enumerate() {
                    if *r == HslShift::default() {
                        continue;
                    }
                    // Full weight within 15° of the centre, fading to zero
                    // at 45°, like Photoshop's default range sliders.
                    let d = (hsl[0] - centres[i]).rem_euclid(360.0);
                    let d = d.min(360.0 - d);
                    let w = 1.0 - smoothstep(15.0, 45.0, d);
                    dh += r.hue * w;
                    ds += r.saturation * w;
                    dl += r.lightness * w;
                }
            }
            hsl[0] = (hsl[0] + dh).rem_euclid(360.0);
            let s = ds / 100.0;
            hsl[1] = if s > 0.0 { hsl[1] + (1.0 - hsl[1]) * s * hsl[1].sqrt() } else { hsl[1] * (1.0 + s) };
            let rgb = hsl_to_rgb([hsl[0], hsl[1].clamp(0.0, 1.0), hsl[2]]);
            let out = apply_lightness(rgb, dl / 100.0);
            px[..3].copy_from_slice(&out.map(|v| v.clamp(0.0, 1.0)));
        }
    }
}

// ---------------------------------------------------------------------------
// Color Balance

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorBalance {
    /// Each is `[cyan↔red, magenta↔green, yellow↔blue]`, -100..100.
    pub shadows: [f32; 3],
    pub midtones: [f32; 3],
    pub highlights: [f32; 3],
    pub preserve_luminosity: bool,
}

impl Default for ColorBalance {
    fn default() -> Self {
        Self { shadows: [0.0; 3], midtones: [0.0; 3], highlights: [0.0; 3], preserve_luminosity: true }
    }
}

impl ColorBalance {
    fn apply(&self, buf: &mut [f32]) {
        let luts: [Lut; 3] = std::array::from_fn(|ch| {
            let (s, m, h) = (self.shadows[ch] / 100.0, self.midtones[ch] / 100.0, self.highlights[ch] / 100.0);
            Lut::from_fn(move |v| {
                // Tonal weights overlap the way GIMP's colour balance does.
                let ws = (1.0 - v / 0.333).clamp(0.0, 1.0) * 0.7 + (1.0 - smoothstep(0.0, 0.5, v)) * 0.3;
                let wh = ((v - 0.667) / 0.333).clamp(0.0, 1.0) * 0.7 + smoothstep(0.5, 1.0, v) * 0.3;
                let wm = (1.0 - (v - 0.5).abs() * 2.0).clamp(0.0, 1.0);
                (v + (s * ws + m * wm + h * wh) * 0.25).clamp(0.0, 1.0)
            })
        });
        for px in buf.chunks_exact_mut(4) {
            let before = [px[0], px[1], px[2]];
            let mut out = [luts[0].eval(before[0]), luts[1].eval(before[1]), luts[2].eval(before[2])];
            if self.preserve_luminosity {
                let d = luma(before) - luma(out);
                out = out.map(|v| (v + d).clamp(0.0, 1.0));
            }
            px[..3].copy_from_slice(&out);
        }
    }
}

// ---------------------------------------------------------------------------
// Black & White

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlackWhite {
    /// Percent weights, -200..300: reds, yellows, greens, cyans, blues, magentas.
    pub weights: [f32; 6],
    pub tint: bool,
    pub tint_color: Rgba8,
}

impl Default for BlackWhite {
    fn default() -> Self {
        // Photoshop's defaults.
        Self { weights: [40.0, 60.0, 40.0, 60.0, 20.0, 80.0], tint: false, tint_color: Rgba8::rgb(225, 211, 179) }
    }
}

impl BlackWhite {
    /// `min + (max − mid)·w(primary) + (mid − min)·w(secondary)`, the widely
    /// reproduced reading of Photoshop's Black & White.
    pub fn grey(&self, c: [f32; 3]) -> f32 {
        let w = self.weights.map(|v| v / 100.0);
        let (r, g, b) = (c[0], c[1], c[2]);
        let mut idx = [(r, 0usize), (g, 1), (b, 2)];
        idx.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mx, imx) = idx[0];
        let (mid, imid) = idx[1];
        let mn = idx[2].0;
        // primary weights: red 0, green 2, blue 4
        let primary = [0usize, 2, 4][imx];
        // secondary: red+green yellow(1), green+blue cyan(3), red+blue magenta(5)
        let secondary = match (imx.min(imid), imx.max(imid)) {
            (0, 1) => 1,
            (1, 2) => 3,
            _ => 5,
        };
        (mn + (mx - mid) * w[primary] + (mid - mn) * w[secondary]).clamp(0.0, 1.0)
    }

    fn apply(&self, buf: &mut [f32]) {
        let tint = self.tint_color.to_f32();
        let tint_l = luma(tint).max(1e-3);
        for px in buf.chunks_exact_mut(4) {
            let y = self.grey([px[0], px[1], px[2]]);
            if self.tint {
                let out = tint.map(|t| (t / tint_l * y).clamp(0.0, 1.0));
                // Keep the grey value as luminance.
                let d = y - luma(out);
                px[..3].copy_from_slice(&out.map(|v| (v + d).clamp(0.0, 1.0)));
            } else {
                px[..3].fill(y);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Photo Filter

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoFilter {
    pub color: Rgba8,
    /// 0..100
    pub density: f32,
    pub preserve_luminosity: bool,
}

impl Default for PhotoFilter {
    fn default() -> Self {
        // "Warming Filter (85)"
        Self { color: Rgba8::rgb(236, 138, 0), density: 25.0, preserve_luminosity: true }
    }
}

impl PhotoFilter {
    fn apply(&self, buf: &mut [f32]) {
        let f = self.color.to_f32();
        let d = self.density / 100.0;
        for px in buf.chunks_exact_mut(4) {
            let c = [px[0], px[1], px[2]];
            let mut out = [0f32; 3];
            for k in 0..3 {
                out[k] = c[k] + (c[k] * f[k] - c[k]) * d;
            }
            if self.preserve_luminosity {
                let dl = luma(c) - luma(out);
                out = out.map(|v| v + dl);
            }
            px[..3].copy_from_slice(&out.map(|v| v.clamp(0.0, 1.0)));
        }
    }
}

// ---------------------------------------------------------------------------
// Channel Mixer

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelMixer {
    /// Output rows red, green, blue; each `[from red, from green, from blue,
    /// constant]` in percent, -200..200.
    pub red: [f32; 4],
    pub green: [f32; 4],
    pub blue: [f32; 4],
    pub monochrome: bool,
}

impl Default for ChannelMixer {
    fn default() -> Self {
        Self { red: [100.0, 0.0, 0.0, 0.0], green: [0.0, 100.0, 0.0, 0.0], blue: [0.0, 0.0, 100.0, 0.0], monochrome: false }
    }
}

impl ChannelMixer {
    fn apply(&self, buf: &mut [f32]) {
        let row = |r: [f32; 4], c: [f32; 3]| {
            ((r[0] * c[0] + r[1] * c[1] + r[2] * c[2]) / 100.0 + r[3] / 100.0).clamp(0.0, 1.0)
        };
        for px in buf.chunks_exact_mut(4) {
            let c = [px[0], px[1], px[2]];
            if self.monochrome {
                let v = row(self.red, c);
                px[..3].fill(v);
            } else {
                px[0] = row(self.red, c);
                px[1] = row(self.green, c);
                px[2] = row(self.blue, c);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gradient Map

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// 0..1
    pub pos: f32,
    pub color: Rgba8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientMap {
    pub stops: Vec<GradientStop>,
    pub reverse: bool,
}

impl Default for GradientMap {
    fn default() -> Self {
        Self {
            stops: vec![
                GradientStop { pos: 0.0, color: Rgba8::BLACK },
                GradientStop { pos: 1.0, color: Rgba8::WHITE },
            ],
            reverse: false,
        }
    }
}

/// Colour of a gradient at `t` (0..1), with alpha.
pub fn gradient_at(stops: &[GradientStop], t: f32) -> [f32; 4] {
    if stops.is_empty() {
        return [0.0; 4];
    }
    let mut sorted: Vec<&GradientStop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let col = |s: &GradientStop| {
        let c = s.color.to_f32();
        [c[0], c[1], c[2], s.color.alpha_f32()]
    };
    if t <= sorted[0].pos {
        return col(sorted[0]);
    }
    for w in sorted.windows(2) {
        if t <= w[1].pos {
            let span = (w[1].pos - w[0].pos).max(1e-6);
            let k = (t - w[0].pos) / span;
            let (a, b) = (col(w[0]), col(w[1]));
            return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
        }
    }
    col(sorted[sorted.len() - 1])
}

impl GradientMap {
    fn apply(&self, buf: &mut [f32]) {
        let table: Vec<[f32; 4]> = (0..256)
            .map(|i| {
                let t = i as f32 / 255.0;
                gradient_at(&self.stops, if self.reverse { 1.0 - t } else { t })
            })
            .collect();
        for px in buf.chunks_exact_mut(4) {
            let y = luma([px[0], px[1], px[2]]).clamp(0.0, 1.0);
            let x = y * 255.0;
            let i = (x as usize).min(254);
            let k = x - i as f32;
            for c in 0..3 {
                px[c] = table[i][c] + (table[i + 1][c] - table[i][c]) * k;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Selective Color

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectiveColor {
    /// Reds, yellows, greens, cyans, blues, magentas, whites, neutrals,
    /// blacks — each `[cyan, magenta, yellow, black]` in percent.
    pub colors: [[f32; 4]; 9],
    /// Absolute instead of relative.
    pub absolute: bool,
}

impl Default for SelectiveColor {
    fn default() -> Self {
        Self { colors: [[0.0; 4]; 9], absolute: false }
    }
}

impl SelectiveColor {
    fn apply(&self, buf: &mut [f32]) {
        for px in buf.chunks_exact_mut(4) {
            let c = [px[0], px[1], px[2]];
            let mx = c[0].max(c[1]).max(c[2]);
            let mn = c[0].min(c[1]).min(c[2]);
            let mid = c[0] + c[1] + c[2] - mx - mn;
            let mut weights = [0f32; 9];
            let imax = if mx == c[0] { 0 } else if mx == c[1] { 1 } else { 2 };
            let imin = if mn == c[2] { 2 } else if mn == c[1] { 1 } else { 0 };
            weights[[0usize, 2, 4][imax]] = mx - mid;
            // The secondary is the colour made of the two largest channels.
            let secondary = match imin {
                2 => 1, // r,g → yellows
                0 => 3, // g,b → cyans
                _ => 5, // r,b → magentas
            };
            weights[secondary] = mid - mn;
            weights[6] = ((mn - 0.5) * 2.0).clamp(0.0, 1.0);
            weights[8] = ((0.5 - mx) * 2.0).clamp(0.0, 1.0);
            weights[7] = (1.0 - ((mx - 0.5).abs() + (mn - 0.5).abs())).clamp(0.0, 1.0);
            let mut out = c;
            for (k, w) in weights.iter().enumerate() {
                if *w <= 0.0 {
                    continue;
                }
                let adj = self.colors[k].map(|v| v / 100.0);
                for ch in 0..3 {
                    let ink = adj[ch] + adj[3];
                    if ink == 0.0 {
                        continue;
                    }
                    let scale = if self.absolute { 1.0 } else { 1.0 - c[ch] + if ink < 0.0 { c[ch] } else { 0.0 } };
                    out[ch] -= w * ink * scale;
                }
            }
            px[..3].copy_from_slice(&out.map(|v| v.clamp(0.0, 1.0)));
        }
    }
}

// ---------------------------------------------------------------------------
// Color Lookup

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorLookup {
    pub name: String,
    pub size: usize,
    /// `size³` RGB triples, red varying fastest (the .cube convention).
    pub table: Vec<f32>,
    /// 0..1, blends the looked-up colour with the original.
    #[serde(default = "one")]
    pub strength: f32,
}

fn one() -> f32 {
    1.0
}

impl ColorLookup {
    pub fn sample(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        if n < 2 || self.table.len() < n * n * n * 3 {
            return c;
        }
        let f = |v: f32| {
            let x = v.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x as usize).min(n - 2);
            (i, x - i as f32)
        };
        let (ri, rt) = f(c[0]);
        let (gi, gt) = f(c[1]);
        let (bi, bt) = f(c[2]);
        let at = |r: usize, g: usize, b: usize, k: usize| self.table[((b * n + g) * n + r) * 3 + k];
        std::array::from_fn(|k| {
            let c00 = at(ri, gi, bi, k) * (1.0 - rt) + at(ri + 1, gi, bi, k) * rt;
            let c10 = at(ri, gi + 1, bi, k) * (1.0 - rt) + at(ri + 1, gi + 1, bi, k) * rt;
            let c01 = at(ri, gi, bi + 1, k) * (1.0 - rt) + at(ri + 1, gi, bi + 1, k) * rt;
            let c11 = at(ri, gi + 1, bi + 1, k) * (1.0 - rt) + at(ri + 1, gi + 1, bi + 1, k) * rt;
            let c0 = c00 * (1.0 - gt) + c10 * gt;
            let c1 = c01 * (1.0 - gt) + c11 * gt;
            c0 * (1.0 - bt) + c1 * bt
        })
    }

    fn apply(&self, buf: &mut [f32]) {
        let s = self.strength.clamp(0.0, 1.0);
        for px in buf.chunks_exact_mut(4) {
            let c = [px[0], px[1], px[2]];
            let o = self.sample(c);
            for k in 0..3 {
                px[k] = (c[k] + (o[k] - c[k]) * s).clamp(0.0, 1.0);
            }
        }
    }

    /// Parse an Adobe/Resolve `.cube` 3D LUT.
    pub fn parse_cube(name: &str, text: &str) -> Result<ColorLookup, String> {
        let mut size = 0usize;
        let mut table = Vec::new();
        let (mut dmin, mut dmax) = ([0f32; 3], [1f32; 3]);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let head = parts.next().unwrap_or("");
            match head {
                "LUT_3D_SIZE" => size = parts.next().and_then(|v| v.parse().ok()).ok_or("bad LUT_3D_SIZE")?,
                "LUT_1D_SIZE" => return Err("1D .cube files are not supported".into()),
                "DOMAIN_MIN" => {
                    let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if v.len() == 3 {
                        dmin = [v[0], v[1], v[2]];
                    }
                }
                "DOMAIN_MAX" => {
                    let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if v.len() == 3 {
                        dmax = [v[0], v[1], v[2]];
                    }
                }
                "TITLE" => {}
                _ => {
                    if let Ok(r) = head.parse::<f32>() {
                        let rest: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                        if rest.len() >= 2 {
                            table.extend_from_slice(&[
                                (r - dmin[0]) / (dmax[0] - dmin[0]),
                                (rest[0] - dmin[1]) / (dmax[1] - dmin[1]),
                                (rest[1] - dmin[2]) / (dmax[2] - dmin[2]),
                            ]);
                        }
                    }
                }
            }
        }
        if size < 2 || table.len() != size * size * size * 3 {
            return Err(format!("expected {} entries, found {}", size * size * size, table.len() / 3));
        }
        Ok(ColorLookup { name: name.to_string(), size, table, strength: 1.0 })
    }
}

// ---------------------------------------------------------------------------
// Develop

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Develop {
    /// Stops, -5..5
    pub exposure: f32,
    /// All of the rest are -100..100 unless noted.
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub vignette: f32,
    /// 0..100
    pub grain: f32,
    /// Fades to greyscale and applies the B&W mix (Lite's "B&W" slider), 0..100.
    pub black_white: f32,
}

impl Develop {
    pub fn is_identity(&self) -> bool {
        *self == Develop::default()
    }

    fn apply(&self, buf: &mut [f32], ctx: &ApplyCtx) {
        if self.is_identity() {
            return;
        }
        let (w, h) = (ctx.width, ctx.height);
        let ev = 2f32.powf(self.exposure);
        let t = self.temperature / 100.0;
        let tn = self.tint / 100.0;
        let wb = [1.0 + 0.22 * t - 0.05 * tn, 1.0 - 0.18 * tn, 1.0 - 0.22 * t - 0.05 * tn];

        // Radius of local operations, in document pixels, scaled to this
        // buffer. Tied to the document so preview and export match.
        let doc_min = ctx.doc_w.min(ctx.doc_h).max(1) as f64;
        let local = (self.clarity != 0.0 || self.dehaze != 0.0 || self.highlights != 0.0 || self.shadows != 0.0) && w > 2 && h > 2;
        let blurred = if local {
            let mut lum: Vec<f32> = buf.chunks_exact(4).map(|p| luma([p[0], p[1], p[2]])).collect();
            let r = ((doc_min * 0.02) / ctx.step).max(1.0) as usize;
            box_blur_1ch(&mut lum, w, h, r, 3);
            Some(lum)
        } else {
            None
        };

        let contrast = self.contrast / 100.0;
        let k = if contrast >= 0.0 { 1.0 + 1.2 * contrast } else { 1.0 / (1.0 - 0.8 * contrast) };
        let (hi, sh, wh, bl) = (self.highlights / 100.0, self.shadows / 100.0, self.whites / 100.0, self.blacks / 100.0);
        let tone = Lut::from_fn(|x| {
            let mut y = s_curve(x, k);
            y += wh * 0.18 * x.powi(3) * (1.0 - x) * 4.0 + wh * 0.06 * x;
            y += bl * 0.18 * (1.0 - x).powi(3) * x * 4.0 - bl * 0.06 * (1.0 - x);
            y.clamp(0.0, 1.0)
        });
        let bw = BlackWhite::default();
        let (cx, cy) = (ctx.doc_w as f64 / 2.0, ctx.doc_h as f64 / 2.0);
        let diag = (cx * cx + cy * cy).sqrt().max(1.0);
        // Exposure and white balance happen in linear light; precompute them
        // as one table per channel instead of two powf calls per sample.
        let light = if self.exposure != 0.0 || t != 0.0 || tn != 0.0 {
            Some([0usize, 1, 2].map(|k| Lut::from_fn(|v| linear_to_srgb((srgb_to_linear(v) * ev * wb[k]).max(0.0)).min(1.0))))
        } else {
            None
        };
        for j in 0..h {
            let doc_y = ctx.y0 + (j as f64 + 0.5) * ctx.step;
            for i in 0..w {
                let o = (j * w + i) * 4;
                let px = &mut buf[o..o + 4];
                let mut c = [px[0], px[1], px[2]];
                if let Some(l) = &light {
                    c = [l[0].eval(c[0]), l[1].eval(c[1]), l[2].eval(c[2])];
                }
                let l0 = luma(c);
                let mut l = tone.eval(l0);
                if let Some(bl) = &blurred {
                    let base = bl[j * w + i];
                    // Shadows/highlights react to the neighbourhood's
                    // brightness, which is what stops them flattening.
                    l += sh * 0.35 * (1.0 - smoothstep(0.0, 0.6, base)) * (1.0 - l) * l.sqrt();
                    l += hi * 0.35 * smoothstep(0.4, 1.0, base) * l * (1.0 - l).sqrt() * if hi < 0.0 { 1.0 } else { 0.6 };
                    let detail = l - base;
                    let mid = 1.0 - (2.0 * l - 1.0).powi(2);
                    l += self.clarity / 100.0 * 0.6 * detail * mid;
                    if self.dehaze != 0.0 {
                        let dh = self.dehaze / 100.0;
                        l = base + (l - base) * (1.0 + 0.4 * dh);
                        l = ((l - 0.08 * dh * base) / (1.0 - 0.08 * dh)).max(0.0);
                    }
                }
                let l = l.clamp(0.0, 1.0);
                // Scale colour to the new luminance, pulling towards grey
                // where that would push a channel past white.
                if l0 > 1e-5 {
                    let ratio = l / l0;
                    c = c.map(|v| v * ratio);
                } else {
                    c = [l, l, l];
                }
                let mx = c[0].max(c[1]).max(c[2]);
                if mx > 1.0 {
                    let k = (1.0 - l) / (mx - l).max(1e-5);
                    c = c.map(|v| l + (v - l) * k);
                }
                let sat = self.saturation / 100.0 + if self.dehaze > 0.0 { self.dehaze / 400.0 } else { 0.0 };
                if self.vibrance != 0.0 || sat != 0.0 {
                    c = vibrance_saturation(c, self.vibrance / 100.0, sat);
                }
                if self.black_white > 0.0 {
                    let g = bw.grey(c);
                    let a = self.black_white / 100.0;
                    c = c.map(|v| v + (g - v) * a);
                }
                if self.vignette != 0.0 {
                    let doc_x = ctx.x0 + (i as f64 + 0.5) * ctx.step;
                    let d = (((doc_x - cx).powi(2) + (doc_y - cy).powi(2)).sqrt() / diag) as f32;
                    let fall = smoothstep(0.35, 1.05, d);
                    let v = self.vignette / 100.0;
                    c = if v < 0.0 { c.map(|x| x * (1.0 + v * 0.85 * fall)) } else { c.map(|x| x + (1.0 - x) * v * 0.85 * fall) };
                }
                if self.grain > 0.0 {
                    let doc_x = ctx.x0 + (i as f64 + 0.5) * ctx.step;
                    let n = crate::blend::dissolve_noise(doc_x as i64, doc_y as i64) - 0.5;
                    let amp = self.grain / 100.0 * 0.12 * (1.0 - (2.0 * luma(c) - 1.0).powi(2)).max(0.2);
                    c = c.map(|x| x + n * amp);
                }
                px[0] = c[0].clamp(0.0, 1.0);
                px[1] = c[1].clamp(0.0, 1.0);
                px[2] = c[2].clamp(0.0, 1.0);
            }
        }
    }
}

/// Repeated box blur of a single-channel buffer (three passes approximate a
/// Gaussian). Edges clamp.
pub fn box_blur_1ch(buf: &mut [f32], w: usize, h: usize, radius: usize, passes: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0f32; buf.len()];
    for _ in 0..passes {
        blur_axis(buf, &mut tmp, w, h, radius, true);
        blur_axis(&tmp, buf, w, h, radius, false);
    }
}

fn blur_axis(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let norm = 1.0 / (2 * r + 1) as f32;
    let at = |o: usize, i: usize| if horizontal { o * w + i } else { i * w + o };
    for o in 0..outer {
        let clampi = |i: isize| i.clamp(0, inner as isize - 1) as usize;
        let mut sum = 0f32;
        for k in -(r as isize)..=(r as isize) {
            sum += src[at(o, clampi(k))];
        }
        for i in 0..inner {
            dst[at(o, i)] = sum * norm;
            let add = clampi(i as isize + r as isize + 1);
            let sub = clampi(i as isize - r as isize);
            sum += src[at(o, add)] - src[at(o, sub)];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(w: usize, h: usize) -> ApplyCtx {
        ApplyCtx { x0: 0.0, y0: 0.0, step: 1.0, doc_w: w as u32, doc_h: h as u32, width: w, height: h }
    }

    fn run(adj: &Adjustment, c: [f32; 3]) -> [f32; 3] {
        let mut buf = vec![c[0], c[1], c[2], 1.0];
        adj.apply(&mut buf, &ctx(1, 1));
        [buf[0], buf[1], buf[2]]
    }

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
    }

    #[test]
    fn defaults_are_identity() {
        let c = [0.2, 0.5, 0.8];
        for kind in ["brightness-contrast", "levels", "curves", "exposure", "vibrance", "hue-saturation", "color-balance", "channel-mixer", "selective-color", "develop"] {
            let adj = Adjustment::default_for(kind).unwrap();
            let out = run(&adj, c);
            assert!(close(out, c, 2.0 / 255.0), "{kind}: {out:?}");
        }
    }

    #[test]
    fn levels_input_white_brightens() {
        let mut l = Levels::default();
        l.master.in_white = 128.0;
        let out = run(&Adjustment::Levels(l), [0.25, 0.25, 0.25]);
        assert!((out[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn curves_through_points() {
        let mut c = Curves::default();
        c.master = CurvePoints(vec![[0.0, 0.0], [128.0, 180.0], [255.0, 255.0]]);
        let lut = c.master.lut();
        assert!((lut.eval(128.0 / 255.0) - 180.0 / 255.0).abs() < 0.005);
        // monotone
        let mut prev = -1.0;
        for i in 0..=100 {
            let v = lut.eval(i as f32 / 100.0);
            assert!(v >= prev - 1e-6);
            prev = v;
        }
    }

    #[test]
    fn invert_twice_is_identity() {
        let c = [0.1, 0.6, 0.9];
        let out = run(&Adjustment::Invert, run(&Adjustment::Invert, c));
        assert!(close(out, c, 1e-6));
    }

    #[test]
    fn black_white_defaults_match_photoshop_primaries() {
        let bw = BlackWhite::default();
        assert!((bw.grey([1.0, 0.0, 0.0]) - 0.4).abs() < 1e-6);
        assert!((bw.grey([1.0, 1.0, 0.0]) - 0.6).abs() < 1e-6);
        assert!((bw.grey([0.0, 0.0, 1.0]) - 0.2).abs() < 1e-6);
        assert!((bw.grey([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn exposure_one_stop_doubles_linear() {
        let e = Adjustment::Exposure(Exposure { exposure: 1.0, ..Default::default() });
        let v = linear_to_srgb(0.2);
        let out = run(&e, [v, v, v]);
        assert!((srgb_to_linear(out[0]) - 0.4).abs() < 0.01);
    }

    #[test]
    fn hue_shift_180_turns_red_cyan() {
        let mut hs = HueSaturation::default();
        hs.master.hue = 180.0;
        let out = run(&Adjustment::HueSaturation(hs), [1.0, 0.0, 0.0]);
        assert!(close(out, [0.0, 1.0, 1.0], 0.01), "{out:?}");
    }

    #[test]
    fn cube_parses() {
        let text = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = ColorLookup::parse_cube("id", text).unwrap();
        let c = [0.3, 0.6, 0.9];
        assert!(close(lut.sample(c), c, 1e-5));
    }

    #[test]
    fn develop_exposure_brightens_and_vignette_darkens_corners() {
        let d = Develop { exposure: 1.0, ..Default::default() };
        let out = run(&Adjustment::Develop(d), [0.3, 0.3, 0.3]);
        assert!(out[0] > 0.35);
        let d = Develop { vignette: -100.0, ..Default::default() };
        let mut buf = vec![0.5f32; 32 * 32 * 4];
        Adjustment::Develop(d).apply(&mut buf, &ctx(32, 32));
        assert!(buf[3 * 0 + 0] < buf[(16 * 32 + 16) * 4]);
    }
}
