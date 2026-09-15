//! `analyze.*`: histogram, automatic corrections, image facts for the
//! planner, and the eyedropper. None of these change the document.

use editor_core::adjust::{Develop, Levels, LevelsChannel};
use editor_core::color::{linear_to_srgb, srgb_to_linear, Rgba8};
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::LayerId;
use editor_core::ops::{target_id, Applied, EditorError, Result};
use editor_core::pixels::{read_layer, read_merged, read_selection};
use editor_core::render::{render_view, to_u8 as f32_to_u8, View};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::noise::immerkaer_sigma;
use crate::parse_cmd;
use crate::util::canvas;

fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoStyle {
    #[default]
    Auto,
    Vivid,
    Natural,
    Bw,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LevelsMode {
    #[default]
    Tone,
    Contrast,
    Color,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Cmd {
    #[serde(rename = "analyze.histogram")]
    Histogram { #[serde(default = "yes")] merged: bool, #[serde(default)] rect: Option<Rect> },
    #[serde(rename = "analyze.auto")]
    Auto { #[serde(default)] style: AutoStyle },
    #[serde(rename = "analyze.auto-levels")]
    AutoLevels { #[serde(default)] mode: LevelsMode },
    #[serde(rename = "analyze.facts")]
    Facts {},
    #[serde(rename = "analyze.pick")]
    Pick { x: f64, y: f64, #[serde(default = "one")] size: u32, #[serde(default = "yes")] merged: bool },
}

pub fn apply(v: Value, doc: &mut Document, _bytes: &[u8]) -> Result<Applied> {
    let id = v.get("id").and_then(Value::as_u64).map(|i| i as LayerId);
    let cmd: Cmd = parse_cmd(v)?;
    let data = match cmd {
        Cmd::Histogram { merged, rect } => histogram(doc, id, merged, rect)?,
        Cmd::Auto { style } => {
            let img = Sample::merged(doc, 1_500_000);
            json!({ "develop": auto_develop(&img, style) })
        }
        Cmd::AutoLevels { mode } => {
            let img = Sample::merged(doc, 4_000_000);
            json!({ "levels": auto_levels(&img, mode) })
        }
        Cmd::Facts {} => facts(doc),
        Cmd::Pick { x, y, size, merged } => json!({ "color": pick(doc, id, x, y, size, merged)? }),
    };
    Ok(Applied::quiet().with_data(data))
}

/// Straight RGBA8 pixels of some view of the image.
pub struct Sample {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

impl Sample {
    /// The visible composite, downscaled to at most `max_pixels`.
    pub fn merged(doc: &Document, max_pixels: usize) -> Sample {
        let (dw, dh) = (doc.width as f64, doc.height as f64);
        let s = (max_pixels as f64 / (dw * dh)).sqrt().min(1.0);
        let (w, h) = (((dw * s).round() as usize).max(1), ((dh * s).round() as usize).max(1));
        let scale = w as f64 / dw;
        let mut px = vec![0u8; w * h * 4];
        let strip = (2_000_000 / w).max(1);
        let mut y = 0;
        while y < h {
            let rows = strip.min(h - y);
            let view = View { x: 0.0, y: y as f64 / scale, scale, width: w, height: rows };
            let f = render_view(doc, view);
            f32_to_u8(&f, &mut px[y * w * 4..(y + rows) * w * 4]);
            y += rows;
        }
        Sample { px, w, h }
    }

    fn opaque_pixels(&self) -> impl Iterator<Item = &[u8]> {
        self.px.chunks_exact(4).filter(|p| p[3] >= 128)
    }
}

// ---------------------------------------------------------------------------
// Histogram

fn histogram(doc: &Document, id: Option<LayerId>, merged: bool, rect: Option<Rect>) -> Result<Value> {
    let area = match rect {
        Some(r) => r.intersect(&canvas(doc)),
        None => editor_core::selection::bounds(doc).intersect(&canvas(doc)),
    };
    let use_sel = rect.is_none() && doc.selection.is_some();
    let layer = if merged { None } else { Some(target_id(doc, id)?) };
    let mut hist = [[0u64; 256]; 4];
    let strip_rows = (4_000_000 / area.w.max(1)).max(1);
    let mut y = area.y;
    while y < area.bottom() {
        let rows = strip_rows.min(area.bottom() - y);
        let strip = Rect::new(area.x, y, area.w, rows);
        let px = match layer {
            None => read_merged(doc, strip),
            Some(l) => read_layer(doc, l, strip)?,
        };
        let sel = if use_sel { Some(read_selection(doc, strip)) } else { None };
        for (i, p) in px.chunks_exact(4).enumerate() {
            if p[3] == 0 || sel.as_ref().is_some_and(|s| s[i] < 128) {
                continue;
            }
            hist[0][p[0] as usize] += 1;
            hist[1][p[1] as usize] += 1;
            hist[2][p[2] as usize] += 1;
            let l = (0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32).round() as usize;
            hist[3][l.min(255)] += 1;
        }
        y += rows;
    }
    Ok(json!({ "r": hist[0].to_vec(), "g": hist[1].to_vec(), "b": hist[2].to_vec(), "l": hist[3].to_vec() }))
}

// ---------------------------------------------------------------------------
// Statistics shared by auto corrections

fn percentile(hist: &[u64; 256], total: u64, q: f64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    let target = q * total as f64;
    let mut acc = 0f64;
    for (i, &c) in hist.iter().enumerate() {
        let next = acc + c as f64;
        if next >= target && c > 0 {
            // Interpolate within the bin.
            let t = ((target - acc) / c as f64).clamp(0.0, 1.0);
            return ((i as f64 + t) / 255.0) as f32;
        }
        acc = next;
    }
    1.0
}

/// White balance correction as Develop `(temperature, tint)` that would
/// neutralise the estimated illuminant, and the confidence behind it.
///
/// The illuminant is a grey-world estimate over near-neutral pixels (surfaces
/// likely to be grey, so a red curtain does not drag the estimate), blended
/// with a white-patch estimate from the brightest unclipped pixels.
fn white_balance(img: &Sample) -> (f32, f32) {
    let lin: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let mut grey = [0f64; 3];
    let mut grey_n = 0f64;
    let mut all = [0f64; 3];
    let mut all_n = 0f64;
    let mut lum_hist = [0u64; 256];
    let mut total = 0u64;
    for p in img.opaque_pixels() {
        let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as usize;
        lum_hist[l.min(255)] += 1;
        total += 1;
    }
    let bright_cut = (percentile(&lum_hist, total, 0.985) * 255.0) as usize;
    let mut white = [0f64; 3];
    let mut white_n = 0f64;
    for p in img.opaque_pixels() {
        let mx = p[0].max(p[1]).max(p[2]);
        let mn = p[0].min(p[1]).min(p[2]);
        if !(40..250).contains(&mx) {
            continue;
        }
        let c = [lin[p[0] as usize] as f64, lin[p[1] as usize] as f64, lin[p[2] as usize] as f64];
        for k in 0..3 {
            all[k] += c[k];
        }
        all_n += 1.0;
        let sat = (mx - mn) as f32 / mx as f32;
        if sat < 0.22 {
            // Nearer neutral counts more.
            let wgt = (1.0 - sat / 0.22) as f64;
            for k in 0..3 {
                grey[k] += c[k] * wgt;
            }
            grey_n += wgt;
        }
        let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as usize;
        if l >= bright_cut && sat < 0.35 {
            for k in 0..3 {
                white[k] += c[k];
            }
            white_n += 1.0;
        }
    }
    if all_n < 16.0 {
        return (0.0, 0.0);
    }
    let norm = |v: [f64; 3]| {
        let g = v[1].max(1e-9);
        [v[0] / g, 1.0, v[2] / g]
    };
    let grey_frac = grey_n / all_n;
    let (mut est, strength) = if grey_frac > 0.02 {
        (norm(grey), 0.85)
    } else {
        // Few neutral surfaces: trust plain grey world only a little.
        (norm(all), 0.3)
    };
    if white_n > 8.0 {
        let wp = norm(white);
        for k in 0..3 {
            est[k] = est[k] * 0.65 + wp[k] * 0.35;
        }
    }
    // Gains that bring the illuminant to neutral.
    let (g_r, g_b) = ((1.0 / est[0]) as f32, (1.0 / est[2]) as f32);
    // Develop: wb = [1 + .22t − .05n, 1 − .18n, 1 − .22t − .05n].
    let s = g_r + g_b;
    let n = if (0.1 - 0.18 * s).abs() > 1e-4 { (2.0 - s) / (0.1 - 0.18 * s) } else { 0.0 };
    let d = 1.0 - 0.18 * n;
    let t = (g_r - g_b) * d / 0.44;
    // Small estimated casts are more likely scene colour than illuminant:
    // leave a dead zone, and treat tint (whose errors look worse) gently.
    let shrink = |v: f32, dead: f32| v.signum() * (v.abs() - dead).max(0.0);
    let temperature = shrink(t * 100.0 * strength, 5.0).clamp(-60.0, 60.0);
    let tint = (shrink(n * 100.0 * strength, 6.0) * 0.7).clamp(-40.0, 40.0);
    (temperature, tint)
}

/// Luminance statistics after an exposure and white balance are applied.
struct Tone {
    hist: [u64; 256],
    total: u64,
    key: f32,
    p99_lin: f32,
    sat: f32,
}

fn tone_stats(img: &Sample, ev: f32, temperature: f32, tint: f32) -> Tone {
    let (t, n) = (temperature / 100.0, tint / 100.0);
    let wb = [1.0 + 0.22 * t - 0.05 * n, 1.0 - 0.18 * n, 1.0 - 0.22 * t - 0.05 * n];
    let mul = 2f32.powf(ev);
    let lin: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let mut hist = [0u64; 256];
    let mut total = 0u64;
    let mut log_sum = 0f64;
    let mut lin_hist = [0u64; 1024];
    let mut sat_sum = 0f64;
    for p in img.opaque_pixels() {
        let c: [f32; 3] = std::array::from_fn(|k| (lin[p[k] as usize] * mul * wb[k]).min(1.0));
        let yl = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        log_sum += (yl as f64 + 1e-3).ln();
        lin_hist[((yl * 1023.0) as usize).min(1023)] += 1;
        let s: [f32; 3] = c.map(linear_to_srgb);
        let l = 0.299 * s[0] + 0.587 * s[1] + 0.114 * s[2];
        hist[((l * 255.0).round() as usize).min(255)] += 1;
        let mx = s[0].max(s[1]).max(s[2]);
        let mn = s[0].min(s[1]).min(s[2]);
        if mx > 0.05 {
            sat_sum += ((mx - mn) / mx) as f64;
        }
        total += 1;
    }
    let key = if total > 0 { ((log_sum / total as f64).exp() - 1e-3).max(1e-4) as f32 } else { 0.18 };
    let mut acc = 0u64;
    let mut p99_lin = 1.0;
    for (i, &c) in lin_hist.iter().enumerate() {
        acc += c;
        if acc as f64 >= 0.99 * total as f64 {
            p99_lin = (i as f32 + 1.0) / 1024.0;
            break;
        }
    }
    Tone { hist, total, key, p99_lin, sat: if total > 0 { (sat_sum / total as f64) as f32 } else { 0.0 } }
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn round0(v: f32) -> f32 {
    v.round() + 0.0
}

/// Automatic Develop settings for Lite's "Auto" and its alternatives.
pub fn auto_develop(img: &Sample, style: AutoStyle) -> Develop {
    let (temperature, tint) = white_balance(img);
    let base = tone_stats(img, 0.0, temperature, tint);
    if base.total == 0 {
        return Develop::default();
    }
    // Exposure: move the log-average luminance part of the way to mid grey,
    // without pushing the brightest percent far past white.
    let mut ev = (0.18 / base.key).log2() * 0.45;
    let headroom = (0.95 / base.p99_lin.max(1e-3)).log2() + 0.1;
    ev = ev.min(headroom.max(0.0)).clamp(-1.0, 1.5);
    if ev.abs() < 0.08 {
        ev = 0.0;
    }
    let t = tone_stats(img, ev, temperature, tint);
    let p = |q: f64| percentile(&t.hist, t.total, q);
    let (p005, p05, p50, p95, p995) = (p(0.005), p(0.05), p(0.5), p(0.95), p(0.995));
    let spread = p95 - p05;
    let contrast = if spread < 0.72 { ((0.72 - spread) / 0.72 * 70.0).min(30.0) } else { -((spread - 0.9).max(0.0) * 100.0).min(15.0) };
    let highlights = if p995 > 0.95 { -((p995 - 0.88) * 450.0).clamp(0.0, 45.0) } else { 0.0 };
    let dark_frac = {
        let cut = (0.18 * 255.0) as usize;
        t.hist[..cut].iter().sum::<u64>() as f32 / t.total as f32
    };
    let shadows = if p05 < 0.08 { (dark_frac * 70.0).clamp(0.0, 35.0) } else { 0.0 };
    let whites = if p995 < 0.9 { ((0.95 - p995) * 160.0).clamp(0.0, 35.0) } else { 0.0 };
    let blacks = if p005 > 0.05 { -((p005 - 0.03) * 250.0).clamp(0.0, 35.0) } else { 0.0 };
    let vibrance = ((0.38 - t.sat) * 60.0).clamp(4.0, 22.0);
    let _ = p50;

    let mut d = Develop { exposure: ev, contrast, highlights, shadows, whites, blacks, temperature, tint, vibrance, ..Default::default() };
    match style {
        AutoStyle::Auto => {}
        AutoStyle::Natural => {
            d.exposure *= 0.8;
            d.contrast *= 0.5;
            d.highlights *= 0.7;
            d.shadows *= 0.7;
            d.whites *= 0.6;
            d.blacks *= 0.6;
            d.temperature *= 0.8;
            d.tint *= 0.8;
            d.vibrance *= 0.5;
        }
        AutoStyle::Vivid => {
            d.contrast = (d.contrast + 18.0).min(45.0);
            d.vibrance = (d.vibrance + 25.0).min(50.0);
            d.saturation = 8.0;
            d.clarity = 15.0;
            d.blacks = (d.blacks - 8.0).max(-45.0);
            d.shadows *= 0.8;
        }
        AutoStyle::Bw => {
            d.black_white = 100.0;
            d.vibrance = 0.0;
            d.saturation = 0.0;
            d.temperature = 0.0;
            d.tint = 0.0;
            d.contrast = (d.contrast + 15.0).clamp(5.0, 40.0);
            d.clarity = 12.0;
            d.blacks = (d.blacks - 6.0).max(-40.0);
        }
    }
    d.exposure = (d.exposure * 100.0).round() / 100.0;
    for v in [&mut d.contrast, &mut d.highlights, &mut d.shadows, &mut d.whites, &mut d.blacks, &mut d.temperature, &mut d.tint, &mut d.vibrance, &mut d.saturation, &mut d.clarity] {
        *v = round0(*v);
    }
    d
}

/// Photoshop's Auto Tone / Auto Contrast / Auto Color, clipping 0.1 %.
pub fn auto_levels(img: &Sample, mode: LevelsMode) -> Levels {
    const CLIP: f64 = 0.001;
    let mut hist = [[0u64; 256]; 3];
    let mut total = 0u64;
    for p in img.opaque_pixels() {
        for c in 0..3 {
            hist[c][p[c] as usize] += 1;
        }
        total += 1;
    }
    let mut lv = Levels::default();
    if total == 0 {
        return lv;
    }
    let bounds = |h: &[u64; 256], n: u64| -> (f32, f32) {
        let cut = (CLIP * n as f64) as u64;
        let mut acc = 0u64;
        let mut lo = 0usize;
        for (i, &c) in h.iter().enumerate() {
            acc += c;
            if acc > cut {
                lo = i;
                break;
            }
        }
        acc = 0;
        let mut hi = 255usize;
        for (i, &c) in h.iter().enumerate().rev() {
            acc += c;
            if acc > cut {
                hi = i;
                break;
            }
        }
        if hi <= lo {
            (0.0, 255.0)
        } else {
            (lo as f32, hi as f32)
        }
    };
    let chan = |lo: f32, hi: f32, gamma: f32| LevelsChannel { in_black: lo, in_white: hi, gamma, out_black: 0.0, out_white: 255.0 };
    match mode {
        LevelsMode::Tone => {
            let b: Vec<(f32, f32)> = hist.iter().map(|h| bounds(h, total)).collect();
            lv.red = chan(b[0].0, b[0].1, 1.0);
            lv.green = chan(b[1].0, b[1].1, 1.0);
            lv.blue = chan(b[2].0, b[2].1, 1.0);
        }
        LevelsMode::Contrast => {
            let mut combined = [0u64; 256];
            for h in &hist {
                for (c, v) in combined.iter_mut().zip(h) {
                    *c += v;
                }
            }
            let (lo, hi) = bounds(&combined, total * 3);
            lv.master = chan(lo, hi, 1.0);
        }
        LevelsMode::Color => {
            // Darkest and lightest colours: the mean of the 0.1 % extremes
            // by luminance, then neutral midtones snapped to grey.
            let mut lum_hist = [0u64; 256];
            for p in img.opaque_pixels() {
                lum_hist[crate::util::luma8(p) as usize] += 1;
            }
            let (llo, lhi) = bounds(&lum_hist, total);
            let (mut dark, mut dark_n, mut light, mut light_n) = ([0f64; 3], 0f64, [0f64; 3], 0f64);
            for p in img.opaque_pixels() {
                let l = crate::util::luma8(p);
                if l <= llo {
                    for c in 0..3 {
                        dark[c] += p[c] as f64;
                    }
                    dark_n += 1.0;
                }
                if l >= lhi {
                    for c in 0..3 {
                        light[c] += p[c] as f64;
                    }
                    light_n += 1.0;
                }
            }
            let dark: [f32; 3] = std::array::from_fn(|c| if dark_n > 0.0 { (dark[c] / dark_n) as f32 } else { 0.0 });
            let light: [f32; 3] = std::array::from_fn(|c| if light_n > 0.0 { (light[c] / light_n) as f32 } else { 255.0 });
            let mut lo = dark;
            let mut hi = light;
            for c in 0..3 {
                if hi[c] - lo[c] < 16.0 {
                    lo[c] = 0.0;
                    hi[c] = 255.0;
                }
            }
            // Mean of near-neutral midtones, normalised per channel.
            let mut mid = [0f64; 3];
            let mut mid_n = 0f64;
            for p in img.opaque_pixels() {
                let l = crate::util::luma8(p);
                let mx = p[0].max(p[1]).max(p[2]) as f32;
                let mn = p[0].min(p[1]).min(p[2]) as f32;
                if (60.0..=200.0).contains(&l) && (mx - mn) / mx.max(1.0) < 0.2 {
                    for c in 0..3 {
                        mid[c] += (((p[c] as f32 - lo[c]) / (hi[c] - lo[c])).clamp(0.01, 0.99)) as f64;
                    }
                    mid_n += 1.0;
                }
            }
            let mut gammas = [1f32; 3];
            if mid_n > total as f64 * 0.005 {
                let m: [f32; 3] = std::array::from_fn(|c| (mid[c] / mid_n) as f32);
                let target = (m[0] + m[1] + m[2]) / 3.0;
                for c in 0..3 {
                    // out = t^(1/g): pick g so the channel's mean lands on target.
                    gammas[c] = (m[c].ln() / target.ln()).clamp(0.6, 1.6);
                }
            }
            lv.red = chan(lo[0].round(), hi[0].round(), round2(gammas[0]));
            lv.green = chan(lo[1].round(), hi[1].round(), round2(gammas[1]));
            lv.blue = chan(lo[2].round(), hi[2].round(), round2(gammas[2]));
        }
    }
    lv
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Facts

/// Straight RGBA8 of a 1:1 centre crop of the composite, for measurements
/// that downscaling would distort (noise, sharpness).
fn centre_crop(doc: &Document, max_side: i32) -> (Vec<u8>, usize, usize) {
    let (w, h) = (doc.width as i32, doc.height as i32);
    let (cw, ch) = (w.min(max_side), h.min(max_side));
    let rect = Rect::new((w - cw) / 2, (h - ch) / 2, cw, ch);
    (read_merged(doc, rect), cw as usize, ch as usize)
}

fn luma_plane(px: &[u8]) -> Vec<f32> {
    px.chunks_exact(4).map(crate::util::luma8).collect()
}

/// Variance of the 4-neighbour Laplacian of luminance.
pub fn laplacian_variance(y: &[f32], w: usize, h: usize) -> f32 {
    if w < 3 || h < 3 {
        return 0.0;
    }
    let (mut sum, mut sum2, mut n) = (0f64, 0f64, 0f64);
    for yy in 1..h - 1 {
        for x in 1..w - 1 {
            let i = yy * w + x;
            let l = (y[i - 1] + y[i + 1] + y[i - w] + y[i + w] - 4.0 * y[i]) as f64;
            sum += l;
            sum2 += l * l;
            n += 1.0;
        }
    }
    let mean = sum / n;
    (sum2 / n - mean * mean) as f32
}

/// Dominant tilt of strong straight edges near horizontal or vertical, via
/// a gradient-directed Hough transform. Returns the degrees to pass to
/// `image.rotate-arbitrary` to level them (0 when the evidence is weak).
pub fn tilt_degrees(y: &[f32], w: usize, h: usize) -> f32 {
    const MAX_DEV: f32 = 12.0;
    const STEP: f32 = 0.1;
    if w < 16 || h < 16 {
        return 0.0;
    }
    let bins = (2.0 * MAX_DEV / STEP) as usize + 1;
    let diag = ((w * w + h * h) as f32).sqrt();
    let rho_bins = (2.0 * diag) as usize + 2;
    // Two families: near-horizontal lines (normal ≈ 90°) and near-vertical
    // (normal ≈ 0°).
    let mut acc = [vec![0f32; bins * rho_bins], vec![0f32; bins * rho_bins]];
    let mut mags = Vec::with_capacity(w * h);
    let mut grads = Vec::with_capacity(w * h);
    for yy in 1..h - 1 {
        for x in 1..w - 1 {
            let i = yy * w + x;
            let gx = (y[i - w + 1] + 2.0 * y[i + 1] + y[i + w + 1]) - (y[i - w - 1] + 2.0 * y[i - 1] + y[i + w - 1]);
            let gy = (y[i + w - 1] + 2.0 * y[i + w] + y[i + w + 1]) - (y[i - w - 1] + 2.0 * y[i - w] + y[i - w + 1]);
            let m = (gx * gx + gy * gy).sqrt();
            mags.push(m);
            grads.push((x, yy, gx, gy, m));
        }
    }
    let cut = crate::noise::quantile(&mags, 0.92).max(60.0);
    let trig: Vec<(f32, f32)> = (0..bins)
        .flat_map(|b| {
            let dev = (b as f32 * STEP - MAX_DEV).to_radians();
            [(90f32.to_radians() + dev), dev].into_iter()
        })
        .map(|a| (a.cos(), a.sin()))
        .collect();
    for &(x, yy, gx, gy, m) in &grads {
        if m < cut {
            continue;
        }
        let normal = gy.atan2(gx).to_degrees().rem_euclid(180.0);
        // Horizontal lines: normal near 90°; vertical: near 0° or 180°. The
        // family test is loose because Sobel directions on aliased
        // staircase edges snap toward the axes; every angle in the family
        // gets a vote.
        const FAMILY: f32 = 20.0;
        const WINDOW: f32 = 6.0;
        let (family, dev) = if (normal - 90.0).abs() <= FAMILY {
            (0, normal - 90.0)
        } else if normal <= FAMILY {
            (1, normal)
        } else if normal >= 180.0 - FAMILY {
            (1, normal - 180.0)
        } else {
            continue;
        };
        let lo = (((dev - WINDOW + MAX_DEV) / STEP).floor().max(0.0)) as usize;
        let hi = (((dev + WINDOW + MAX_DEV) / STEP).ceil() as usize).min(bins - 1);
        for b in lo..=hi {
            let (c, s) = trig[b * 2 + family];
            let rho = x as f32 * c + yy as f32 * s + diag;
            let r0 = (rho.floor() as usize).min(rho_bins - 2);
            let t = rho - r0 as f32;
            acc[family][b * rho_bins + r0] += 1.0 - t;
            acc[family][b * rho_bins + r0 + 1] += t;
        }
    }
    let mut peaks: Vec<(f32, f32)> = Vec::new();
    for (family, a) in acc.iter_mut().enumerate() {
        // Only long lines count: a fifth of the image in their direction
        // (Sobel marks each edge about two pixels thick).
        let span = if family == 0 { w } else { h };
        let min_votes = (span as f32 * 0.2 * 1.5).max(24.0);
        for _ in 0..12 {
            let (idx, &best) = a.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).unwrap();
            if best < min_votes {
                break;
            }
            let (b, r) = (idx / rho_bins, idx % rho_bins);
            peaks.push((b as f32 * STEP - MAX_DEV, best));
            // Suppress the neighbourhood of this line.
            for bb in b.saturating_sub(15)..(b + 16).min(bins) {
                for rr in r.saturating_sub(6)..(r + 7).min(rho_bins) {
                    a[bb * rho_bins + rr] = 0.0;
                }
            }
        }
    }
    if peaks.is_empty() {
        return 0.0;
    }
    // Weighted median deviation.
    peaks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f32 = peaks.iter().map(|p| p.1).sum();
    let mut acc_w = 0f32;
    let mut dev = 0.0;
    for p in &peaks {
        acc_w += p.1;
        if acc_w * 2.0 >= total {
            dev = p.0;
            break;
        }
    }
    // The lines must agree: most of the evidence within a degree of the
    // median, or the image has no single tilt worth correcting.
    let agree: f32 = peaks.iter().filter(|p| (p.0 - dev).abs() <= 1.0).map(|p| p.1).sum();
    if agree < 0.6 * total {
        return 0.0;
    }
    // A positive deviation is a clockwise tilt; level it by turning back.
    let out = -dev;
    // Under a third of a degree is not worth resampling the image for.
    if out.abs() < 0.3 {
        0.0
    } else {
        round1(out)
    }
}

fn facts(doc: &Document) -> Value {
    let img = Sample::merged(doc, 2_000_000);
    let lin: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let (mut log_sum, mut n, mut low, mut high) = (0f64, 0u64, 0u64, 0u64);
    for p in img.opaque_pixels() {
        let yl = 0.2126 * lin[p[0] as usize] + 0.7152 * lin[p[1] as usize] + 0.0722 * lin[p[2] as usize];
        log_sum += (yl as f64 + 1e-3).ln();
        n += 1;
        if p[0].max(p[1]).max(p[2]) <= 2 {
            low += 1;
        }
        if p[0].max(p[1]).max(p[2]) >= 254 {
            high += 1;
        }
    }
    let key = if n > 0 { ((log_sum / n as f64).exp() - 1e-3).max(1e-4) as f32 } else { 0.18 };
    let (temperature, tint) = white_balance(&img);
    let small_luma = luma_plane(&img.px);
    let (crop, cw, ch) = centre_crop(doc, 1024);
    let crop_luma = luma_plane(&crop);
    let frac = |v: u64| if n > 0 { (v as f64 / n as f64 * 10000.0).round() / 10000.0 } else { 0.0 };
    json!({
        // Stops relative to a mid-grey average: negative is underexposed.
        "exposure": round2((key / 0.18).log2()),
        "clipping_low": frac(low),
        "clipping_high": frac(high),
        // The cast present in the image, in Develop units (the correction
        // is the negation).
        "cast": { "temperature": -temperature, "tint": -tint },
        "noise_sigma": round2(immerkaer_sigma(&crop_luma, cw, ch, Some(0.1))),
        "sharpness": round1(laplacian_variance(&crop_luma, cw, ch)),
        "tilt_degrees": tilt_degrees(&small_luma, img.w, img.h),
    })
}

// ---------------------------------------------------------------------------
// Pick

fn pick(doc: &Document, id: Option<LayerId>, x: f64, y: f64, size: u32, merged: bool) -> Result<Rgba8> {
    let size = match size {
        0 | 1 => 1,
        2 | 3 => 3,
        _ => 5,
    };
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let half = size / 2;
    let rect = Rect::new(cx - half, cy - half, size, size).intersect(&canvas(doc));
    if rect.is_empty() {
        return Err(EditorError::Invalid("the point is outside the image".into()));
    }
    let px = if merged { read_merged(doc, rect) } else { read_layer(doc, target_id(doc, id)?, rect)? };
    let mut acc = [0f64; 4];
    for p in px.chunks_exact(4) {
        let a = p[3] as f64;
        for c in 0..3 {
            acc[c] += p[c] as f64 * a;
        }
        acc[3] += a;
    }
    let n = (px.len() / 4) as f64;
    if acc[3] <= 0.0 {
        return Ok(Rgba8::TRANSPARENT);
    }
    Ok(Rgba8 {
        r: (acc[0] / acc[3]).round() as u8,
        g: (acc[1] / acc[3]).round() as u8,
        b: (acc[2] / acc[3]).round() as u8,
        a: (acc[3] / n).round() as u8,
    })
}
