//! White balance: temperature/tint ↔ camera multipliers, presets, and an
//! automatic estimate from the image.
//!
//! Temperature follows the Planckian locus in CIE 1960 uv (Krystek's
//! approximation); tint is the signed distance from the locus along its
//! normal, scaled by 3000 like Adobe's slider (positive = compensating a
//! green cast, i.e. adding magenta).

use crate::color::{apply, invert, Mat3};

pub const TEMP_MIN: f32 = 2000.0;
pub const TEMP_MAX: f32 = 15000.0;
const TINT_SCALE: f64 = 3000.0;

fn planck_uv(t: f64) -> (f64, f64) {
    let u = (0.860_117_757 + 1.541_182_54e-4 * t + 1.286_412_12e-7 * t * t) / (1.0 + 8.424_202_35e-4 * t + 7.081_451_63e-7 * t * t);
    let v = (0.317_398_726 + 4.228_062_45e-5 * t + 4.204_816_91e-8 * t * t) / (1.0 - 2.897_418_16e-5 * t + 1.614_560_53e-7 * t * t);
    (u, v)
}

/// Unit normal to the locus at `t`, pointing to the green (+v) side.
fn planck_normal(t: f64) -> (f64, f64) {
    let (u0, v0) = planck_uv(t - 1.0);
    let (u1, v1) = planck_uv(t + 1.0);
    let (du, dv) = (u1 - u0, v1 - v0);
    let len = (du * du + dv * dv).sqrt().max(1e-12);
    let (mut nu, mut nv) = (-dv / len, du / len);
    if nv < 0.0 {
        nu = -nu;
        nv = -nv;
    }
    (nu, nv)
}

/// CIE xy chromaticity of a temperature/tint pair.
pub fn xy_for(temperature: f32, tint: f32) -> (f64, f64) {
    let t = (temperature as f64).clamp(1000.0, 25000.0);
    let (u, v) = planck_uv(t);
    let (nu, nv) = planck_normal(t);
    let d = tint as f64 / TINT_SCALE;
    let (u, v) = (u + nu * d, v + nv * d);
    let den = 2.0 * u - 8.0 * v + 4.0;
    (3.0 * u / den, 2.0 * v / den)
}

/// Camera multipliers (green = 1) that neutralise light of this temperature/tint.
pub fn multipliers_for(xyz_to_cam: &Mat3, temperature: f32, tint: f32) -> [f32; 3] {
    let (x, y) = xy_for(temperature, tint);
    let xyz = [(x / y) as f32, 1.0, ((1.0 - x - y) / y) as f32];
    let cam = apply(xyz_to_cam, xyz);
    normalise([1.0 / cam[0].max(1e-6), 1.0 / cam[1].max(1e-6), 1.0 / cam[2].max(1e-6)])
}

fn normalise(m: [f32; 3]) -> [f32; 3] {
    let g = if m[1] > 0.0 { m[1] } else { 1.0 };
    [m[0] / g, 1.0, m[2] / g]
}

/// Temperature and tint that the multipliers correspond to.
pub fn temperature_tint(xyz_to_cam: &Mat3, multipliers: [f32; 3]) -> (f32, f32) {
    let Some(cam_to_xyz) = invert(xyz_to_cam) else { return (5000.0, 0.0) };
    let cam = [1.0 / multipliers[0].max(1e-6), 1.0 / multipliers[1].max(1e-6), 1.0 / multipliers[2].max(1e-6)];
    let xyz = apply(&cam_to_xyz, cam);
    let s = (xyz[0] + xyz[1] + xyz[2]) as f64;
    if s.abs() < 1e-9 {
        return (5000.0, 0.0);
    }
    let (x, y) = (xyz[0] as f64 / s, xyz[1] as f64 / s);
    let den = -2.0 * x + 12.0 * y + 3.0;
    let (u, v) = (4.0 * x / den, 6.0 * y / den);
    // Nearest point on the locus, searched in mired (perceptually even).
    let dist = |mired: f64| {
        let (pu, pv) = planck_uv(1e6 / mired);
        (pu - u).powi(2) + (pv - v).powi(2)
    };
    let (lo_m, hi_m) = (1e6 / TEMP_MAX as f64, 1e6 / TEMP_MIN as f64);
    let mut best = lo_m;
    let mut best_d = f64::MAX;
    let steps = 400;
    for i in 0..=steps {
        let m = lo_m + (hi_m - lo_m) * i as f64 / steps as f64;
        let d = dist(m);
        if d < best_d {
            best_d = d;
            best = m;
        }
    }
    // Golden-section refinement around the best sample.
    let step = (hi_m - lo_m) / steps as f64;
    let (mut a, mut b) = ((best - step).max(lo_m), (best + step).min(hi_m));
    let gr = (5f64.sqrt() - 1.0) / 2.0;
    for _ in 0..40 {
        let c = b - gr * (b - a);
        let d = a + gr * (b - a);
        if dist(c) < dist(d) {
            b = d;
        } else {
            a = c;
        }
    }
    let t = 1e6 / ((a + b) / 2.0);
    let (pu, pv) = planck_uv(t);
    let (nu, nv) = planck_normal(t);
    let tint = ((u - pu) * nu + (v - pv) * nv) * TINT_SCALE;
    ((t as f32).clamp(TEMP_MIN, TEMP_MAX), (tint as f32).clamp(-150.0, 150.0))
}

/// Adobe Camera Raw's preset values.
pub fn preset(name: &str) -> Option<(f32, f32)> {
    Some(match name {
        "daylight" => (5500.0, 10.0),
        "cloudy" => (6500.0, 10.0),
        "shade" => (7500.0, 10.0),
        "tungsten" => (2850.0, 0.0),
        "fluorescent" => (3800.0, 21.0),
        "flash" => (5500.0, 0.0),
        _ => return None,
    })
}

/// Estimate the illuminant from camera RGB (not white balanced, 0..1, clip
/// at 1) and return multipliers. Mixes grey-world with a bright-pixel
/// estimate, which is more robust than either on colourful scenes.
pub fn auto_multipliers(cam: &[f32], calib: &crate::calib::Calibration) -> [f32; 3] {
    const BINS: usize = 1024;
    let mut hist = [0u32; BINS];
    let mut gw = [0f64; 3];
    let mut n = 0u64;
    for p in cam.chunks_exact(3) {
        let mx = p[0].max(p[1]).max(p[2]);
        if mx >= 0.98 || mx < 0.01 {
            continue;
        }
        hist[((mx * BINS as f32) as usize).min(BINS - 1)] += 1;
        for c in 0..3 {
            gw[c] += p[c] as f64;
        }
        n += 1;
    }
    if n < 64 {
        return [1.0, 1.0, 1.0];
    }
    // Brightest 3% of valid pixels.
    let target = (n as f64 * 0.03).max(1.0) as u64;
    let mut acc = 0u64;
    let mut cut = BINS - 1;
    for i in (0..BINS).rev() {
        acc += hist[i] as u64;
        if acc >= target {
            cut = i;
            break;
        }
    }
    let thr = cut as f32 / BINS as f32;
    let mut br = [0f64; 3];
    for p in cam.chunks_exact(3) {
        let mx = p[0].max(p[1]).max(p[2]);
        if mx >= 0.98 || mx < thr {
            continue;
        }
        for c in 0..3 {
            br[c] += p[c] as f64;
        }
    }
    let illum: Vec<f64> = (0..3).map(|c| ((gw[c] / n as f64).max(1e-9) * br[c].max(1e-9) / n as f64).sqrt()).collect();
    let raw = normalise([(illum[1] / illum[0]) as f32, 1.0, (illum[1] / illum[2]) as f32]);
    // Keep the estimate on believable light: snap to the locus region.
    let (t, tint) = calib.temperature_tint(raw);
    calib.multipliers_for(t, tint.clamp(-40.0, 40.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::SRGB_TO_XYZ;

    fn cam() -> Mat3 {
        // Camera = sRGB primaries: xyz_to_cam = inverse(sRGB→XYZ).
        invert(&SRGB_TO_XYZ).unwrap()
    }

    #[test]
    fn d65_is_neutral_for_srgb_camera() {
        // D65 sits about +0.0032 Duv above the Planckian locus at 6504 K.
        let (t, tint) = temperature_tint(&cam(), [1.0, 1.0, 1.0]);
        assert!((t - 6504.0).abs() < 150.0, "{t}");
        assert!((tint - 9.5).abs() < 3.0, "{tint}");
    }

    #[test]
    fn multipliers_round_trip() {
        let m = cam();
        for &(t, tint) in &[(2850.0, 0.0), (5500.0, 10.0), (7500.0, -20.0), (4000.0, 30.0)] {
            let mult = multipliers_for(&m, t, tint);
            let (t2, tint2) = temperature_tint(&m, mult);
            assert!((t2 - t).abs() / t < 0.01, "{t} -> {t2}");
            assert!((tint2 - tint).abs() < 1.0, "{tint} -> {tint2}");
        }
    }

    #[test]
    fn warmer_light_needs_more_blue() {
        let m = cam();
        let tungsten = multipliers_for(&m, 2850.0, 0.0);
        let daylight = multipliers_for(&m, 5500.0, 0.0);
        assert!(tungsten[2] > daylight[2]);
        assert!(tungsten[0] < daylight[0]);
    }

    #[test]
    fn auto_recovers_a_cast() {
        let m = cam();
        // A grey scene lit by 3200 K light, seen by an sRGB camera.
        let mult = multipliers_for(&m, 3200.0, 0.0);
        let mut img = Vec::new();
        for i in 0..10_000 {
            let l = 0.05 + 0.6 * ((i % 97) as f32 / 97.0);
            img.extend_from_slice(&[l / mult[0], l, l / mult[2]]);
        }
        let got = auto_multipliers(&img, &crate::calib::Calibration::single(m));
        let (t, _) = temperature_tint(&m, got);
        assert!((t - 3200.0).abs() < 100.0, "{t}");
    }
}
