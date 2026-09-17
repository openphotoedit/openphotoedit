//! The develop stage: white-balanced camera RGB → 8-bit sRGB.
//!
//! Order: highlight reconstruction → camera matrix into linear Rec.2020 →
//! noise reduction → exposure → local highlights/shadows → blacks/whites and
//! the sigmoid tone curve → vibrance/saturation → capture sharpening → sRGB
//! with gamut clipping → dither → orientation.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::color::{self, OetfLut, REC2020_LUMA};
use crate::filters;
use crate::highlight;
use crate::orient;
use crate::tone::{self, Sigmoid, SigmoidLut};
use crate::Rgb;

/// Develop settings. Sliders are -100..100 unless noted, 0 = default look.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DevelopParams {
    /// Stops, -5..5.
    pub exposure: f32,
    /// "as-shot", "auto", "custom", or a preset: daylight, cloudy, shade,
    /// tungsten, fluorescent, flash.
    pub wb: String,
    /// Kelvin, used when `wb` is "custom".
    pub temperature: f32,
    /// -150..150, used when `wb` is "custom".
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub vibrance: f32,
    pub saturation: f32,
    /// 0..100.
    pub noise_luma: f32,
    /// 0..100.
    pub noise_chroma: f32,
    /// 0..150.
    pub sharpen: f32,
}

impl Default for DevelopParams {
    fn default() -> Self {
        DevelopParams {
            exposure: 0.0,
            wb: "as-shot".into(),
            temperature: 5500.0,
            tint: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            noise_luma: 0.0,
            noise_chroma: 25.0,
            sharpen: 40.0,
        }
    }
}

/// Look constants: what "0" on every slider renders as.
pub mod look {
    /// Most cameras meter middle grey about half a stop under 18%. DNGs with
    /// a BaselineExposure tag use that instead.
    pub const BASELINE_EV: f32 = 0.5;
    pub const CONTRAST: f32 = 1.55;
    pub const WHITE_EV: f32 = 6.0;
    pub const SATURATION: f32 = 1.12;
    pub const HIGHLIGHT_COMPRESSION: f32 = 0.12;
}

pub struct Output {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[inline]
fn luma(p: &[f32]) -> f32 {
    p[0] * REC2020_LUMA[0] + p[1] * REC2020_LUMA[1] + p[2] * REC2020_LUMA[2]
}

/// Chroma and luma noise reduction in a square-root domain (roughly
/// variance-stabilised for photon noise), guided by luminance.
fn denoise(img: &mut Rgb, luma_amt: f32, chroma_amt: f32, scale: f32) {
    let (w, h) = (img.w, img.h);
    if w < 8 || h < 8 || (luma_amt <= 0.0 && chroma_amt <= 0.0) {
        return;
    }
    let s: Vec<f32> = img.data.par_iter().map(|v| v.max(0.0).sqrt()).collect();
    let ys: Vec<f32> = s.par_chunks(3).map(luma).collect();
    let mut ys_out = ys.clone();
    if luma_amt > 0.0 {
        let a = luma_amt / 100.0;
        let r = ((1.0 + 2.0 * a) * scale).round().max(1.0) as usize;
        let eps = (0.006 * a + 0.0005).powi(2);
        let f = filters::guided(&ys, &ys, w, h, r, eps, 1);
        let k = (a * 1.5).min(1.0);
        ys_out.par_iter_mut().zip(f.par_iter()).for_each(|(o, f)| *o += (f - *o) * k);
    }
    let mut c1: Vec<f32> = s.par_chunks(3).zip(ys.par_iter()).map(|(p, y)| p[0] - y).collect();
    let mut c2: Vec<f32> = s.par_chunks(3).zip(ys.par_iter()).map(|(p, y)| p[2] - y).collect();
    if chroma_amt > 0.0 {
        let a = chroma_amt / 100.0;
        let r = ((2.0 + 10.0 * a) * scale).round().max(1.0) as usize;
        let eps = 0.002 * a + 1e-5;
        let sub = if r >= 6 { 2 } else { 1 };
        let k = (a * 2.5).min(1.0);
        for c in [&mut c1, &mut c2] {
            let f = filters::guided(&ys, c, w, h, r, eps, sub);
            c.par_iter_mut().zip(f.par_iter()).for_each(|(o, f)| *o += (f - *o) * k);
        }
    }
    let [wr, wg, wb] = REC2020_LUMA;
    img.data.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
        let y = ys_out[i];
        let r = y + c1[i];
        let b = y + c2[i];
        let g = (y - wr * r - wb * b) / wg;
        p[0] = r.max(0.0).powi(2);
        p[1] = g.max(0.0).powi(2);
        p[2] = b.max(0.0).powi(2);
    });
}

/// Capture sharpening: unsharp mask on perceptual luminance with a small
/// radius and a soft noise threshold, applied as a luminance ratio.
fn sharpen(data: &mut [f32], w: usize, h: usize, amount: f32, scale: f32) {
    if amount <= 0.0 || w < 8 || h < 8 {
        return;
    }
    let p: Vec<f32> = data.par_chunks(3).map(|q| luma(q).max(0.0).sqrt()).collect();
    let sigma = (0.9 * scale).max(0.45);
    let blur = filters::small_gauss(&p, w, h, sigma);
    let k = amount / 100.0 * 1.6;
    data.par_chunks_mut(3).enumerate().for_each(|(i, q)| {
        let d = p[i] - blur[i];
        // Suppress sub-threshold detail (noise), keep edges.
        let t = (d.abs() / 0.012).min(1.0);
        let d = d * t * t;
        let np = (p[i] + k * d).max(0.0);
        if p[i] > 1e-4 {
            let ratio = ((np / p[i]).powi(2)).clamp(0.5, 2.0);
            q[0] *= ratio;
            q[1] *= ratio;
            q[2] *= ratio;
        }
    });
}

#[inline]
fn hash(x: u32, y: u32, c: u32) -> f32 {
    let mut v = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ c.wrapping_mul(0xcb1a_b31f);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v & 0xFFFF) as f32 / 65536.0
}

/// Run the develop stage. `img` is white-balanced camera RGB whose channels
/// clipped at `thr`; `cam_to_working` takes it to linear Rec.2020; `scale` is the size of
/// `img` relative to the full-resolution image (radii follow it).
pub fn run(mut img: Rgb, thr: [f32; 3], cam_to_working: &color::Mat3, p: &DevelopParams, baseline_ev: f32, orientation: u8, scale: f32) -> Output {
    let (w, h) = (img.w, img.h);
    highlight::reconstruct(&mut img, thr);

    let m = *cam_to_working;
    img.data.par_chunks_mut(3).for_each(|q| {
        let o = color::apply(&m, [q[0], q[1], q[2]]);
        q.copy_from_slice(&o);
    });

    denoise(&mut img, p.noise_luma.clamp(0.0, 100.0), p.noise_chroma.clamp(0.0, 100.0), scale);

    let gain = 2f32.powf(p.exposure.clamp(-5.0, 5.0) + baseline_ev);
    img.data.par_iter_mut().for_each(|v| *v *= gain);

    tone::local_tone(&mut img.data, w, h, p.highlights / 100.0, p.shadows / 100.0, look::HIGHLIGHT_COMPRESSION, scale);

    let curve = SigmoidLut::new(Sigmoid::new(look::CONTRAST * 2f32.powf(p.contrast.clamp(-100.0, 100.0) / 100.0 * 0.55), look::WHITE_EV - p.whites.clamp(-100.0, 100.0) / 100.0 * 2.0));
    let blacks = p.blacks.clamp(-100.0, 100.0) / 100.0;
    let crush = (-blacks).max(0.0) * 0.012;
    let lift = blacks.max(0.0) * 0.004;
    let sat = look::SATURATION * (1.0 + p.saturation.clamp(-100.0, 100.0) / 100.0);
    let vib = p.vibrance.clamp(-100.0, 100.0) / 100.0;
    img.data.par_chunks_mut(3).for_each(|q| {
        let mut x = [q[0], q[1], q[2]];
        if crush > 0.0 || lift > 0.0 {
            for v in x.iter_mut() {
                let c = v.max(0.0);
                *v = c * c / (c + crush) + lift;
            }
        }
        let mut o = tone::map_preserving_hue(x, |v| curve.map(v));
        let y = luma(&o);
        let mx = o[0].max(o[1]).max(o[2]);
        let mn = o[0].min(o[1]).min(o[2]);
        let s_now = if mx > 1e-6 { (mx - mn) / mx } else { 0.0 };
        let k = sat * (1.0 + vib * (1.0 - s_now).powi(2));
        for v in o.iter_mut() {
            *v = y + (*v - y) * k;
        }
        q.copy_from_slice(&o);
    });

    sharpen(&mut img.data, w, h, p.sharpen.clamp(0.0, 150.0), scale);

    let to_srgb = color::working_to_srgb();
    let lut = OetfLut::new();
    let srgb_luma = [0.212_672_9f32, 0.715_152_2, 0.072_175];
    let mut rgba = vec![255u8; w * h * 4];
    rgba.par_chunks_mut(w * 4).enumerate().for_each(|(yy, row)| {
        for xx in 0..w {
            let i = (yy * w + xx) * 3;
            let mut s = color::apply(&to_srgb, [img.data[i], img.data[i + 1], img.data[i + 2]]);
            // Gamut clip towards luminance, preserving hue.
            let y = (s[0] * srgb_luma[0] + s[1] * srgb_luma[1] + s[2] * srgb_luma[2]).clamp(0.0, 1.0);
            let mx = s[0].max(s[1]).max(s[2]);
            let mn = s[0].min(s[1]).min(s[2]);
            let mut t = 1.0f32;
            if mn < 0.0 {
                t = t.min(y / (y - mn));
            }
            if mx > 1.0 && y < 1.0 {
                t = t.min((1.0 - y) / (mx - y));
            }
            if t < 1.0 {
                for v in s.iter_mut() {
                    *v = y + (*v - y) * t.max(0.0);
                }
            }
            for c in 0..3 {
                let e = lut.get(s[c]) * 255.0;
                let d = hash(xx as u32, yy as u32, c as u32) + hash(xx as u32 + 7919, yy as u32 + 104_729, c as u32) - 1.0;
                row[xx * 4 + c] = (e + d + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    });
    drop(img);
    let (rgba, ow, oh) = orient::apply(&rgba, w, h, 4, orientation);
    Output { width: ow, height: oh, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_parse_with_defaults() {
        let p: DevelopParams = serde_json::from_str(r#"{"exposure": 1.5, "wb": "auto"}"#).unwrap();
        assert_eq!(p.exposure, 1.5);
        assert_eq!(p.wb, "auto");
        assert_eq!(p.noise_chroma, 25.0);
    }

    #[test]
    fn neutral_grey_develops_neutral_and_mid() {
        let (w, h) = (24, 24);
        let v = 0.18 * 2f32.powf(-look::BASELINE_EV);
        let img = Rgb { w, h, data: vec![v; w * h * 3] };
        let out = run(img, [1.0; 3], &color::IDENTITY, &DevelopParams { sharpen: 0.0, noise_chroma: 0.0, ..Default::default() }, look::BASELINE_EV, 1, 1.0);
        let px = &out.rgba[(12 * w + 12) * 4..(12 * w + 12) * 4 + 4];
        assert!((px[0] as i32 - px[1] as i32).abs() <= 2 && (px[2] as i32 - px[1] as i32).abs() <= 2, "{px:?}");
        // 0.18 linear is 118 in sRGB; the default local compression does not
        // touch grey.
        assert!((px[1] as i32 - 118).abs() <= 4, "{px:?}");
    }
}
