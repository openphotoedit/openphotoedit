//! Select › Color Range: a soft selection by colour distance (CIE76 ΔE in
//! Lab) from a sampled colour, or by tonal/skin presets. Samples the visible
//! composite, like Photoshop's default.

use editor_core::color::Rgba8;
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::pixels::read_merged;
use editor_core::plane::Plane;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    Highlights,
    Midtones,
    Shadows,
    #[serde(alias = "skin-tones")]
    Skin,
}

fn srgb_lut() -> [f32; 256] {
    std::array::from_fn(|i| editor_core::color::srgb_to_linear(i as f32 / 255.0))
}

fn lab(lut: &[f32; 256], p: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// Coverage 0..1 of one pixel.
pub struct Scorer {
    lut: [f32; 256],
    target: Option<[f32; 3]>,
    preset: Option<Preset>,
    fuzziness: f32,
}

impl Scorer {
    pub fn new(color: Option<Rgba8>, preset: Option<Preset>, fuzziness: f32) -> Scorer {
        let lut = srgb_lut();
        let target = color.map(|c| lab(&lut, [c.r, c.g, c.b]));
        Scorer { lut, target, preset, fuzziness: fuzziness.clamp(0.0, 200.0) }
    }

    #[inline]
    pub fn score(&self, p: [u8; 3]) -> f32 {
        if let Some(preset) = self.preset {
            return preset_score(preset, p, self.fuzziness);
        }
        let Some(t) = self.target else { return 0.0 };
        let l = lab(&self.lut, p);
        let de = ((l[0] - t[0]).powi(2) + (l[1] - t[1]).powi(2) + (l[2] - t[2]).powi(2)).sqrt();
        // Fuzziness 0 keeps (near-)exact matches; 200 reaches ΔE 120.
        let reach = (self.fuzziness * 0.6).max(0.5);
        let k = (1.0 - de / reach).clamp(0.0, 1.0);
        // Ease the tail so the falloff reads as soft rather than banded.
        k * k * (3.0 - 2.0 * k)
    }
}

fn ramp(e0: f32, e1: f32, x: f32) -> f32 {
    editor_core::color::smoothstep(e0, e1, x)
}

fn preset_score(preset: Preset, p: [u8; 3], fuzziness: f32) -> f32 {
    let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    let soft = 24.0 + fuzziness * 0.24;
    match preset {
        Preset::Shadows => 1.0 - ramp(64.0 - soft * 0.5, 64.0 + soft, l),
        Preset::Highlights => ramp(192.0 - soft, 192.0 + soft * 0.5, l),
        Preset::Midtones => ramp(64.0, 64.0 + soft * 1.5, l) * (1.0 - ramp(192.0 - soft * 1.5, 192.0, l)),
        Preset::Skin => {
            // Skin clusters tightly in YCbCr chroma, across ethnicities.
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let cb = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
            let cr = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
            let widen = 1.0 + fuzziness / 100.0;
            let e = (((cb - 105.0) / (22.0 * widen)).powi(2) + ((cr - 152.0) / (18.0 * widen)).powi(2)).sqrt();
            let chroma = 1.0 - ramp(1.0, 1.8, e);
            let tone = ramp(25.0, 60.0, l) * (1.0 - ramp(235.0, 252.0, l));
            chroma * tone
        }
    }
}

pub fn color_range(doc: &Document, scorer: &Scorer) -> Plane {
    let (w, h) = (doc.width as i32, doc.height as i32);
    let mut out = Plane::mask(w as u32, h as u32, 0);
    let strip = 256;
    let mut y = 0;
    while y < h {
        let rows = strip.min(h - y);
        let r = Rect::new(0, y, w, rows);
        let px = read_merged(doc, r);
        let cov: Vec<u8> = px.chunks_exact(4).map(|p| (scorer.score([p[0], p[1], p[2]]) * p[3] as f32).round() as u8).collect();
        out.write(r, &cov);
        y += rows;
    }
    out.compact();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_colour_is_fully_selected_and_falloff_is_monotone() {
        let s = Scorer::new(Some(Rgba8::rgb(200, 40, 40)), None, 40.0);
        assert_eq!(s.score([200, 40, 40]), 1.0);
        let a = s.score([190, 45, 45]);
        let b = s.score([160, 60, 60]);
        let c = s.score([40, 40, 200]);
        assert!(a > b && b >= c, "{a} {b} {c}");
        assert_eq!(c, 0.0);
        let exact = Scorer::new(Some(Rgba8::rgb(200, 40, 40)), None, 0.0);
        assert_eq!(exact.score([210, 40, 40]), 0.0);
        let wide = Scorer::new(Some(Rgba8::rgb(200, 40, 40)), None, 200.0);
        assert!(wide.score([160, 60, 60]) > b);
    }

    #[test]
    fn tonal_presets() {
        let hi = Scorer::new(None, Some(Preset::Highlights), 0.0);
        let mid = Scorer::new(None, Some(Preset::Midtones), 0.0);
        let sh = Scorer::new(None, Some(Preset::Shadows), 0.0);
        assert!(hi.score([250, 250, 250]) > 0.99 && hi.score([10, 10, 10]) == 0.0);
        assert!(sh.score([10, 10, 10]) > 0.99 && sh.score([250, 250, 250]) == 0.0);
        assert!(mid.score([128, 128, 128]) > 0.99 && mid.score([250, 250, 250]) == 0.0 && mid.score([5, 5, 5]) == 0.0);
    }

    #[test]
    fn skin_preset_prefers_skin_over_sky_and_grass() {
        let s = Scorer::new(None, Some(Preset::Skin), 0.0);
        for skin in [[224u8, 172, 140], [141, 85, 36], [198, 134, 66], [255, 219, 172]] {
            assert!(s.score(skin) > 0.5, "{skin:?} {}", s.score(skin));
        }
        for other in [[80u8, 140, 220], [60, 160, 60], [128, 128, 128], [20, 20, 90]] {
            assert!(s.score(other) < 0.1, "{other:?} {}", s.score(other));
        }
    }
}
