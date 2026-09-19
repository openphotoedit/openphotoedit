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
    /// Centre of the stroke's previous dab: a hard round tip stamps the
    /// capsule from there, which gives the stroke an exact edge.
    pub from: Option<(f64, f64)>,
}

/// Where the previous segment of a stroke left off.
#[derive(Clone, Debug, Default)]
pub struct PathState {
    last: Option<StrokePoint>,
    /// Distance travelled since the last dab.
    carry: f64,
    /// The previous dab's centre.
    prev_dab: Option<(f64, f64)>,
    /// The latest raw samples (at most four) for smoothing.
    hist: Vec<StrokePoint>,
}

/// Centripetal Catmull–Rom through `p0..p3`, evaluated between `p1` (u = 0)
/// and `p2` (u = 1) with the Barry–Goldman pyramid. Knots advance by the
/// square root of the chord length, which keeps the curve free of cusps and
/// self-intersections. Ported from Compositor's BrushStroke.swift (MIT).
fn catmull_rom(p: [(f64, f64); 4], u: f64) -> (f64, f64) {
    let knot = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).hypot(b.1 - a.1)).sqrt().max(1e-4);
    let t0 = 0.0;
    let t1 = t0 + knot(p[0], p[1]);
    let t2 = t1 + knot(p[1], p[2]);
    let t3 = t2 + knot(p[2], p[3]);
    let t = t1 + (t2 - t1) * u;
    let mix = |a: (f64, f64), b: (f64, f64), ta: f64, tb: f64| {
        let k = (t - ta) / (tb - ta);
        (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
    };
    let a1 = mix(p[0], p[1], t0, t1);
    let a2 = mix(p[1], p[2], t1, t2);
    let a3 = mix(p[2], p[3], t2, t3);
    let b1 = mix(a1, a2, t0, t2);
    let b2 = mix(a2, a3, t1, t3);
    mix(b1, b2, t1, t2)
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - t * vx).hypot(p.1 - a.1 - t * vy)
}

/// The curve piece from `p[1]` to `p[2]` as a polyline (excluding its start),
/// split until every chord is within 0.2 px of the curve.
fn flatten(p: [StrokePoint; 4]) -> Vec<StrokePoint> {
    let xy = p.map(|q| (q.x, q.y));
    let at = |u: f64| {
        let (x, y) = catmull_rom(xy, u);
        StrokePoint { x, y, p: p[1].p + (p[2].p - p[1].p) * u as f32 }
    };
    let mut out = Vec::new();
    fn split(at: &dyn Fn(f64) -> StrokePoint, u0: f64, u1: f64, a: StrokePoint, b: StrokePoint, depth: u32, out: &mut Vec<StrokePoint>) {
        let off = [0.25, 0.5, 0.75].iter().map(|k| {
            let q = at(u0 + (u1 - u0) * k);
            seg_dist((q.x, q.y), (a.x, a.y), (b.x, b.y))
        });
        if depth >= 10 || off.fold(0.0, f64::max) <= 0.2 {
            out.push(b);
            return;
        }
        let um = (u0 + u1) / 2.0;
        let m = at(um);
        split(at, u0, um, a, m, depth + 1, out);
        split(at, um, u1, m, b, depth + 1, out);
    }
    split(&at, 0.0, 1.0, p[1], p[2], 0, &mut out);
    // Pin the end exactly on the sample.
    if let Some(l) = out.last_mut() {
        *l = p[2];
    }
    out
}

/// Mirror `b` through `a`: the phantom neighbour at an open end of the path
/// when only two samples are known.
fn phantom(a: StrokePoint, b: StrokePoint) -> StrokePoint {
    StrokePoint { x: 2.0 * a.x - b.x, y: 2.0 * a.y - b.y, p: a.p }
}

/// The phantom sample after `r`, continuing the turn from `p→q` to `q→r`
/// (at most 90°) with the last chord's length, so the open end of a curve
/// keeps its curvature instead of straightening.
fn extrapolate(p: StrokePoint, q: StrokePoint, r: StrokePoint) -> StrokePoint {
    let (d1, d2) = ((q.x - p.x, q.y - p.y), (r.x - q.x, r.y - q.y));
    let turn = (d1.0 * d2.1 - d1.1 * d2.0).atan2(d1.0 * d2.0 + d1.1 * d2.1).clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
    let (sin, cos) = turn.sin_cos();
    StrokePoint { x: r.x + d2.0 * cos - d2.1 * sin, y: r.y + d2.0 * sin + d2.1 * cos, p: r.p }
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
                dabs.push(Dab { x: pt.x, y: pt.y, size: self.size_at(pt.p), pressure: pt.p, from: None });
                state.last = Some(pt);
                state.carry = 0.0;
                state.prev_dab = Some((pt.x, pt.y));
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
                let (x, y) = (a.x + dx * k, a.y + dy * k);
                dabs.push(Dab { x, y, size: self.size_at(p), pressure: p, from: state.prev_dab });
                state.prev_dab = Some((x, y));
            }
            state.last = Some(pt);
        }
        dabs
    }

    /// Like [`walk`](Self::walk) but through a centripetal Catmull–Rom curve
    /// instead of straight chords. Returns the settled dabs and the
    /// provisional tail: the newest piece has no following sample yet, so it
    /// is drawn with a mirrored end tangent and redrawn (from `state`, which
    /// the tail does not advance) when the next sample arrives. A stroke's
    /// final tail is its true end.
    pub fn walk_smooth(&self, state: &mut PathState, points: &[StrokePoint]) -> (Vec<Dab>, Vec<Dab>) {
        let mut dabs = Vec::new();
        for &pt in points {
            let pt = StrokePoint { p: pt.p.clamp(0.0, 1.0), ..pt };
            if let Some(l) = state.hist.last_mut() {
                if (pt.x - l.x).hypot(pt.y - l.y) < 1e-6 {
                    l.p = pt.p;
                    continue;
                }
            }
            state.hist.push(pt);
            if state.hist.len() > 4 {
                state.hist.remove(0);
            }
            let h = &state.hist;
            let n = h.len();
            if n == 1 && state.last.is_none() {
                dabs.extend(self.walk(state, &[pt]));
            } else if n >= 3 {
                let (a, b) = (h[n - 3], h[n - 2]);
                let before = if n >= 4 { h[n - 4] } else { extrapolate(h[n - 1], b, a) };
                let piece = flatten([before, a, b, h[n - 1]]);
                dabs.extend(self.walk(state, &piece));
            }
        }
        let h = &state.hist;
        let n = h.len();
        let tail = if n >= 2 {
            let (a, b) = (h[n - 2], h[n - 1]);
            let (before, after) = if n >= 3 { (h[n - 3], extrapolate(h[n - 3], a, b)) } else { (phantom(a, b), phantom(b, a)) };
            let piece = flatten([before, a, b, after]);
            self.walk(&mut state.clone(), &piece)
        } else {
            Vec::new()
        };
        (dabs, tail)
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
        if let (Some(from), false) = (dab.from, aliased) {
            if self.capsule_tip() {
                return self.capsule(dab, from);
            }
        }
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

impl Brush {
    /// A hard, round, fixed-size tip: its stroke is a union of capsules
    /// whose coverage combines by `max` (see `stroke.rs`), which gives an
    /// exact antialiased edge instead of the scalloped rim that overlapping
    /// discs leave. Their continuous kernel does the same with the distance
    /// to the segment (Compositor, MetalBrushCoverage.swift, MIT).
    /// Full flow only: with less, each dab adds a share and the capsule's
    /// larger footprint would build up faster than the disc it replaces.
    pub fn capsule_tip(&self) -> bool {
        self.hardness >= 0.99 && self.roundness >= 0.999 && !self.pressure_size && self.flow >= 0.999
    }

    /// The capsule from `from` to the dab: one pixel of antialiasing on the
    /// distance to the segment.
    fn capsule(&self, dab: &Dab, from: (f64, f64)) -> Stamp {
        let r = dab.size as f64 / 2.0;
        let (x0, y0) = (from.0.min(dab.x), from.1.min(dab.y));
        let (x1, y1) = (from.0.max(dab.x), from.1.max(dab.y));
        let rect = Rect::cover(x0 - r - 1.0, y0 - r - 1.0, x1 - x0 + 2.0 * r + 2.0, y1 - y0 + 2.0 * r + 2.0);
        let mut cov = vec![0f32; rect.area() as usize];
        for j in 0..rect.h {
            let py = (rect.y + j) as f64 + 0.5;
            for i in 0..rect.w {
                let px = (rect.x + i) as f64 + 0.5;
                let d = seg_dist((px, py), from, (dab.x, dab.y));
                cov[(j * rect.w + i) as usize] = (r + 0.5 - d).clamp(0.0, 1.0) as f32;
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
        let s = hard.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0, from: None }, false);
        let at = |s: &Stamp, x: i32, y: i32| s.cov[((y - s.rect.y) * s.rect.w + x - s.rect.x) as usize];
        assert_eq!(at(&s, 50, 50), 1.0);
        assert_eq!(at(&s, 58, 50), 1.0);
        assert_eq!(at(&s, 61, 50), 0.0);
        // Area close to πr².
        let area: f32 = s.cov.iter().sum();
        assert!((area - std::f32::consts::PI * 100.0).abs() < 3.0, "{area}");
        let soft = Brush { size: 20.0, hardness: 0.0, ..Default::default() };
        let t = soft.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0, from: None }, false);
        assert!(at(&t, 50, 50) > 0.95);
        let mid = at(&t, 55, 50);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
        assert_eq!(at(&t, 61, 50), 0.0);
        // Subpixel offsets move coverage smoothly.
        let a = hard.stamp(&Dab { x: 50.0, y: 50.5, size: 3.0, pressure: 1.0, from: None }, false);
        let b2 = hard.stamp(&Dab { x: 50.25, y: 50.5, size: 3.0, pressure: 1.0, from: None }, false);
        assert!(at(&a, 51, 50) < at(&b2, 51, 50));
    }

    #[test]
    fn pencil_is_aliased_and_roundness_squashes() {
        let b = Brush { size: 1.0, ..Default::default() };
        let s = b.stamp(&Dab { x: 10.3, y: 7.9, size: 1.0, pressure: 1.0, from: None }, true);
        let lit: Vec<(i32, i32)> = (0..s.rect.h).flat_map(|j| (0..s.rect.w).map(move |i| (i, j))).filter(|&(i, j)| s.cov[(j * s.rect.w + i) as usize] > 0.0).map(|(i, j)| (s.rect.x + i, s.rect.y + j)).collect();
        assert_eq!(lit, vec![(10, 7)]);
        assert!(s.cov.iter().all(|&c| c == 0.0 || c == 1.0));
        let flat = Brush { size: 20.0, roundness: 0.25, angle: 0.0, ..Default::default() };
        let f = flat.stamp(&Dab { x: 50.0, y: 50.0, size: 20.0, pressure: 1.0, from: None }, false);
        let at = |x: i32, y: i32| f.cov[((y - f.rect.y) * f.rect.w + x - f.rect.x) as usize];
        assert_eq!(at(58, 50), 1.0);
        assert_eq!(at(50, 55), 0.0);
    }
}
