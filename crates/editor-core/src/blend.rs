//! Photoshop's 27 layer blend modes.
//!
//! Formulas follow the W3C Compositing and Blending spec where Photoshop
//! agrees with it, and Photoshop's own definitions where it does not (Soft
//! Light, Hard Mix, the Linear/Vivid/Pin Light family, Subtract, Divide, and
//! the 0.30/0.59/0.11 luma weights of the non-separable modes). Blending
//! happens in the document's encoded (gamma) space, which is what Photoshop
//! does for 8- and 16-bit documents.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    #[default]
    Normal,
    Dissolve,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 27] = [
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];

    /// The four-character key PSD layer records use.
    pub fn psd_key(self) -> [u8; 4] {
        *match self {
            BlendMode::Normal => b"norm",
            BlendMode::Dissolve => b"diss",
            BlendMode::Darken => b"dark",
            BlendMode::Multiply => b"mul ",
            BlendMode::ColorBurn => b"idiv",
            BlendMode::LinearBurn => b"lbrn",
            BlendMode::DarkerColor => b"dkCl",
            BlendMode::Lighten => b"lite",
            BlendMode::Screen => b"scrn",
            BlendMode::ColorDodge => b"div ",
            BlendMode::LinearDodge => b"lddg",
            BlendMode::LighterColor => b"lgCl",
            BlendMode::Overlay => b"over",
            BlendMode::SoftLight => b"sLit",
            BlendMode::HardLight => b"hLit",
            BlendMode::VividLight => b"vLit",
            BlendMode::LinearLight => b"lLit",
            BlendMode::PinLight => b"pLit",
            BlendMode::HardMix => b"hMix",
            BlendMode::Difference => b"diff",
            BlendMode::Exclusion => b"smud",
            BlendMode::Subtract => b"fsub",
            BlendMode::Divide => b"fdiv",
            BlendMode::Hue => b"hue ",
            BlendMode::Saturation => b"sat ",
            BlendMode::Color => b"colr",
            BlendMode::Luminosity => b"lum ",
        }
    }

    pub fn from_psd_key(key: &[u8; 4]) -> Option<BlendMode> {
        BlendMode::ALL.into_iter().find(|m| &m.psd_key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Dissolve => "Dissolve",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::LinearBurn => "Linear Burn",
            BlendMode::DarkerColor => "Darker Color",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::LinearDodge => "Linear Dodge (Add)",
            BlendMode::LighterColor => "Lighter Color",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::HardLight => "Hard Light",
            BlendMode::VividLight => "Vivid Light",
            BlendMode::LinearLight => "Linear Light",
            BlendMode::PinLight => "Pin Light",
            BlendMode::HardMix => "Hard Mix",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Subtract => "Subtract",
            BlendMode::Divide => "Divide",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
        }
    }

    fn separable(self) -> Option<fn(f32, f32) -> f32> {
        Some(match self {
            BlendMode::Normal | BlendMode::Dissolve => |_, s| s,
            BlendMode::Darken => |b: f32, s: f32| b.min(s),
            BlendMode::Multiply => |b, s| b * s,
            BlendMode::ColorBurn => color_burn,
            BlendMode::LinearBurn => |b, s| (b + s - 1.0).max(0.0),
            BlendMode::Lighten => |b: f32, s: f32| b.max(s),
            BlendMode::Screen => screen,
            BlendMode::ColorDodge => color_dodge,
            BlendMode::LinearDodge => |b, s| (b + s).min(1.0),
            BlendMode::Overlay => |b, s| hard_light(s, b),
            BlendMode::SoftLight => soft_light,
            BlendMode::HardLight => hard_light,
            BlendMode::VividLight => |b, s| {
                if s <= 0.5 {
                    color_burn(b, 2.0 * s)
                } else {
                    color_dodge(b, 2.0 * (s - 0.5))
                }
            },
            BlendMode::LinearLight => |b, s| (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
            BlendMode::PinLight => |b: f32, s: f32| if s <= 0.5 { b.min(2.0 * s) } else { b.max(2.0 * s - 1.0) },
            BlendMode::HardMix => |b, s| if b + s >= 1.0 - 1e-6 { 1.0 } else { 0.0 },
            BlendMode::Difference => |b: f32, s: f32| (b - s).abs(),
            BlendMode::Exclusion => |b, s| b + s - 2.0 * b * s,
            BlendMode::Subtract => |b, s| (b - s).max(0.0),
            BlendMode::Divide => |b, s| if s <= 0.0 { if b <= 0.0 { 0.0 } else { 1.0 } } else { (b / s).min(1.0) },
            _ => return None,
        })
    }

    /// `B(Cb, Cs)`: the mixed colour before alpha compositing.
    #[inline]
    pub fn mix(self, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
        if let Some(f) = self.separable() {
            return [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
        }
        match self {
            BlendMode::DarkerColor => {
                if lum(cs) < lum(cb) {
                    cs
                } else {
                    cb
                }
            }
            BlendMode::LighterColor => {
                if lum(cs) > lum(cb) {
                    cs
                } else {
                    cb
                }
            }
            BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
            BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
            BlendMode::Color => set_lum(cs, lum(cb)),
            BlendMode::Luminosity => set_lum(cb, lum(cs)),
            _ => unreachable!(),
        }
    }
}

#[inline]
fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

#[inline]
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

#[inline]
fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

#[inline]
fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

/// Photoshop's Soft Light, which is not the W3C one.
#[inline]
fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        2.0 * b * s + b * b * (1.0 - 2.0 * s)
    } else {
        b.sqrt() * (2.0 * s - 1.0) + 2.0 * b * (1.0 - s)
    }
}

#[inline]
pub fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        let d = l - n;
        for v in out.iter_mut() {
            *v = if d > 0.0 { l + (*v - l) * l / d } else { l };
        }
    }
    if x > 1.0 {
        let d = x - l;
        for v in out.iter_mut() {
            *v = if d > 0.0 { l + (*v - l) * (1.0 - l) / d } else { l };
        }
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let range = mx - mn;
    if range <= 0.0 {
        return [0.0; 3];
    }
    c.map(|v| (v - mn) * s / range)
}

/// A stable per-pixel hash in `[0, 1)`, for Dissolve.
#[inline]
pub fn dissolve_noise(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Composite a straight-alpha source over a straight-alpha backdrop, in
/// place: `co = (as·(1−ab)·Cs + as·ab·B(Cb,Cs) + (1−as)·ab·Cb) / ao`.
#[inline]
pub fn composite_px(dst: &mut [f32], src: [f32; 3], sa: f32, mode: BlendMode) {
    if sa <= 0.0 {
        return;
    }
    let ab = dst[3];
    let ao = sa + ab * (1.0 - sa);
    if ao <= 0.0 {
        dst[..4].fill(0.0);
        return;
    }
    let cb = [dst[0], dst[1], dst[2]];
    let mixed = if ab > 0.0 { mode.mix(cb, src) } else { src };
    for c in 0..3 {
        dst[c] = (sa * (1.0 - ab) * src[c] + sa * ab * mixed[c] + (1.0 - sa) * ab * cb[c]) / ao;
    }
    dst[3] = ao;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn over(bg: [f32; 4], fg: [f32; 4], mode: BlendMode) -> [f32; 4] {
        let mut d = bg;
        composite_px(&mut d, [fg[0], fg[1], fg[2]], fg[3], mode);
        d
    }

    #[test]
    fn psd_keys_round_trip() {
        for m in BlendMode::ALL {
            assert_eq!(BlendMode::from_psd_key(&m.psd_key()), Some(m));
        }
    }

    #[test]
    fn normal_half_over_opaque() {
        let r = over([0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 0.5], BlendMode::Normal);
        assert!((r[0] - 0.5).abs() < 1e-6 && (r[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn multiply_and_screen_identities() {
        let bg = [0.4, 0.6, 0.8, 1.0];
        let m = over(bg, [1.0, 1.0, 1.0, 1.0], BlendMode::Multiply);
        assert!((m[1] - 0.6).abs() < 1e-6, "multiply by white is identity");
        let s = over(bg, [0.0, 0.0, 0.0, 1.0], BlendMode::Screen);
        assert!((s[2] - 0.8).abs() < 1e-6, "screen with black is identity");
    }

    #[test]
    fn source_over_transparent_is_source() {
        for m in BlendMode::ALL {
            let r = over([0.0; 4], [0.2, 0.4, 0.6, 1.0], m);
            assert!((r[0] - 0.2).abs() < 1e-6 && (r[3] - 1.0).abs() < 1e-6, "{m:?}");
        }
    }

    #[test]
    fn luminosity_keeps_backdrop_colour() {
        let r = over([1.0, 0.0, 0.0, 1.0], [0.5, 0.5, 0.5, 1.0], BlendMode::Luminosity);
        assert!((lum([r[0], r[1], r[2]]) - 0.5).abs() < 1e-4);
        assert!(r[0] > r[1] && r[0] > r[2]);
    }

    #[test]
    fn soft_light_grey_is_identity() {
        for b in [0.1f32, 0.5, 0.9] {
            assert!((soft_light(b, 0.5) - b).abs() < 1e-6);
        }
    }
}
