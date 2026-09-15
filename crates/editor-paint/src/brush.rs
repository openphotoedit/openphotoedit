//! Brush tips and stroke paths: where dabs land along a pointer path and how
//! much each covers every pixel.

use editor_core::color::Rgba8;
use editor_core::blend::BlendMode;
use editor_core::geom::Rect;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Brush {
    /// Diameter in pixels.
    pub size: f32,
    /// 0 = Gaussian-soft, 1 = hard (anti-aliased) edge.
    pub hardness: f32,
    /// 0..1: the most a single stroke can cover.
    pub opacity: f32,
    /// 0..1: how much each dab adds toward `opacity`.
    pub flow: f32,
    /// Distance between dabs as a fraction of the diameter.
    pub spacing: f32,
    pub color: Rgba8,
    pub blend: BlendMode,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    /// Degrees.
    pub angle: f32,
    /// 0..1: minor/major axis ratio.
    pub roundness: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Brush {
            size: 20.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.1,
            color: Rgba8::BLACK,
            blend: BlendMode::Normal,
            pressure_size: false,
            pressure_opacity: false,
            angle: 0.0,
            roundness: 1.0,
        }
    }
}

fn full() -> f32 {
    1.0
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    #[serde(default = "full")]
    pub p: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub x: f64,
    pub y: f64,
    pub size: f32,
    pub pressure: f32,
}

/// Where the previous segment of a stroke left off.
#[derive(Clone, Debug, Default)]
pub struct PathState {
    last: Option<StrokePoint>,
    /// Distance travelled since the last dab.
    carry: f64,
}

impl Brush {
    pub fn size_at(&self, pressure: f32) -> f32 {
        let s = if self.pressure_size { self.size * pressure.clamp(0.0, 1.0) } else { self.size };
        s.max(1.0)
    }

    /// Dab positions for the next points of a stroke, spaced by
    /// `spacing × size` along the path with pressure interpolated between
    /// points. The first point of a stroke always gets a dab.
    pub fn walk(&self, state: &mut PathState, points: &[StrokePoint]) -> Vec<Dab> {
        let mut dabs = Vec::new();
        let spacing = self.spacing.max(0.01) as f64;
        let step = |p: f32| (spacing * self.size_at(p) as f64).max(0.5);
        for &pt in points {
            let pt = StrokePoint { p: pt.p.clamp(0.0, 1.0), ..pt };
            let Some(a) = state.last else {
                dabs.push(Dab { x: pt.x, y: pt.y, size: self.size_at(pt.p), pressure: pt.p });
                state.last = Some(pt);
                state.carry = 0.0;
                continue;
            };
            let (dx, dy) = (pt.x - a.x, pt.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                state.last = Some(pt);
                continue;
            }
            let mut t = 0.0;
            loop {
                let p_here = a.p + (pt.p - a.p) * (t / len) as f32;
                let need = step(p_here) - state.carry;
                if t + need > len {
                    state.carry += len - t;
                    break;
                }
                t += need.max(0.0);
                state.carry = 0.0;
                let k = t / len;
                let p = a.p + (pt.p - a.p) * k as f32;
                dabs.push(Dab { x: a.x + dx * k, y: a.y + dy * k, size: self.size_at(p), pressure: p });
            }
            state.last = Some(pt);
        }
        dabs
    }
}

/// A rasterised dab: coverage 0..1 over `rect`.
pub struct Stamp {
    pub rect: Rect,
    pub cov: Vec<f32>,
}

/// Gaussian falloff normalised to reach 0 at `t = 1`.
fn falloff_lut() -> &'static [f32; 257] {
    use std::sync::OnceLock;
    static LUT: OnceLock<[f32; 257]> = OnceLock::new();
    LUT.get_or_init(|| {
        let k = 4.5f32;
        let floor = (-k).exp();
        std::array::from_fn(|i| {
            let t = i as f32 / 256.0;
            (((-k * t * t).exp() - floor) / (1.0 - floor)).max(0.0)
        })
    })
}

impl Brush {
    /// Rasterise a dab. `aliased` is the pencil: pixel-snapped, hard, no
    /// partial coverage.
    pub fn stamp(&self, dab: &Dab, aliased: bool) -> Stamp {
        let r = dab.size as f64 / 2.0;
        let (cx, cy) = if aliased { (dab.x.floor() + 0.5, dab.y.floor() + 0.5) } else { (dab.x, dab.y) };
        let rect = Rect::cover(cx - r - 1.0, cy - r - 1.0, 2.0 * r + 2.0, 2.0 * r + 2.0);
        let (sin, cos) = (self.angle as f64).to_radians().sin_cos();
        let round = self.roundness.clamp(0.01, 1.0) as f64;
        let hard = self.hardness.clamp(0.0, 1.0) as f64;
        let inner = hard * r;
        let ramp = r + 0.5 - inner;
        let lut = falloff_lut();
        let mut cov = vec![0f32; rect.area() as usize];
        for j in 0..rect.h {
            let py = (rect.y + j) as f64 + 0.5 - cy;
            for i in 0..rect.w {
                let px = (rect.x + i) as f64 + 0.5 - cx;
                let u = px * cos + py * sin;
                let v = (-px * sin + py * cos) / round;
                let d = (u * u + v * v).sqrt();
                let c = if aliased {
                    (d <= r.max(0.5)) as u8 as f32
                } else if ramp <= 1.5 {
                    // Hard tip: one pixel of anti-aliasing centred on the edge.
                    (r + 0.5 - d).clamp(0.0, 1.0) as f32
                } else if d <= inner {
                    1.0
                } else {
                    let t = ((d - inner) / ramp).min(1.0);
                    let x = t * 256.0;
                    let k = (x as usize).min(255);
                    let f = (x - k as f64) as f32;
                    lut[k] + (lut[k + 1] - lut[k]) * f
                };
                cov[(j * rect.w + i) as usize] = c;
            }
        }
        Stamp { rect, cov }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f64, f64)]) -> Vec<StrokePoint> {
        v.iter().map(|&(x, y)| StrokePoint { x, y, p: 1.0 }).collect()
    }

    #[test]
    fn spacing_is_even_across_segment_boundaries() {
        let b = Brush { size: 10.0, spacing: 0.25, ..Default::default() };
        let mut whole = PathState::default();
        let one = b.walk(&mut whole, &pts(&[(0.0, 0.0), (7.0, 0.0), (21.0, 0.0)]));
        let mut split = PathState::default();
        let mut two = b.walk(&mut split, &pts(&[(0.0, 0.0), (7.0, 0.0)]));
        two.extend(b.walk(&mut split, &pts(&[(7.0, 0.0), (21.0, 0.0)])));
        let xs: Vec<f64> = one.iter().map(|d| d.x).collect();
        assert_eq!(xs, vec![0.0, 2.5, 5.0, 7.5, 10.0, 12.5, 15.0, 17.5, 20.0]);
        assert_eq!(one, two);
    }

    #[test]
    fn pressure_interpolates_and_scales_size() {
        let b = Brush { size: 20.0, spacing: 0.1, pressure_size: true, ..Default::default() };
        let mut st = PathState::default();
        let d = b.walk(&mut st, &[StrokePoint { x: 0.0, y: 0.0, p: 0.2 }, StrokePoint { x: 40.0, y: 0.0, p: 1.0 }]);
        assert!((d[0].size - 4.0).abs() < 1e-4);
        assert!(d.windows(2).all(|w| w[1].pressure >= w[0].pressure));
        let mid = d.iter().find(|d| d.x >= 20.0).unwrap();
        assert!((mid.pressure - 0.6).abs() < 0.06, "{}", mid.pressure);
    }

    #[test]
    fn hard_and_soft_tips() {
        let hard = Brush { size: 20.0, hardness: 1.0, ..Default::default() };
        let s = hard.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0 }, false);
        let at = |s: &Stamp, x: i32, y: i32| s.cov[((y - s.rect.y) * s.rect.w + x - s.rect.x) as usize];
        assert_eq!(at(&s, 50, 50), 1.0);
        assert_eq!(at(&s, 58, 50), 1.0);
        assert_eq!(at(&s, 61, 50), 0.0);
        // Area close to πr².
        let area: f32 = s.cov.iter().sum();
        assert!((area - std::f32::consts::PI * 100.0).abs() < 3.0, "{area}");
        let soft = Brush { size: 20.0, hardness: 0.0, ..Default::default() };
        let t = soft.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0 }, false);
        assert!(at(&t, 50, 50) > 0.95);
        let mid = at(&t, 55, 50);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
        assert_eq!(at(&t, 61, 50), 0.0);
        // Subpixel offsets move coverage smoothly.
        let a = hard.stamp(&Dab { x: 50.0, y: 50.5, size: 3.0, pressure: 1.0 }, false);
        let b2 = hard.stamp(&Dab { x: 50.25, y: 50.5, size: 3.0, pressure: 1.0 }, false);
        assert!(at(&a, 51, 50) < at(&b2, 51, 50));
    }

    #[test]
    fn pencil_is_aliased_and_roundness_squashes() {
        let b = Brush { size: 1.0, ..Default::default() };
        let s = b.stamp(&Dab { x: 10.3, y: 7.9, size: 1.0, pressure: 1.0 }, true);
        let lit: Vec<(i32, i32)> = (0..s.rect.h).flat_map(|j| (0..s.rect.w).map(move |i| (i, j))).filter(|&(i, j)| s.cov[(j * s.rect.w + i) as usize] > 0.0).map(|(i, j)| (s.rect.x + i, s.rect.y + j)).collect();
        assert_eq!(lit, vec![(10, 7)]);
        assert!(s.cov.iter().all(|&c| c == 0.0 || c == 1.0));
        let flat = Brush { size: 20.0, roundness: 0.25, angle: 0.0, ..Default::default() };
        let f = flat.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0 }, false);
        let at = |x: i32, y: i32| f.cov[((y - f.rect.y) * f.rect.w + x - f.rect.x) as usize];
        assert_eq!(at(58, 50), 1.0);
        assert_eq!(at(50, 55), 0.0);
    }
}
