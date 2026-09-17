//! Camera colour calibration across illuminants, DNG style.
//!
//! Cameras are profiled under two (sometimes more) lights, typically
//! Standard Illuminant A (2856 K) and D65. For a given white balance the
//! matrices are interpolated in inverse temperature, which matters a lot
//! under tungsten: a D65-only matrix leaves an indoor scene green-yellow.
//! Camera RGB goes to XYZ with that matrix, the white is Bradford-adapted to
//! D65, then into the working space.

use crate::color::{apply, invert, mul, Mat3, REC2020_TO_XYZ};
use crate::wb;

const BRADFORD: Mat3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
pub const D65_XYZ: [f32; 3] = [0.950_47, 1.0, 1.088_83];

#[derive(Debug, Clone)]
pub struct Calibration {
    /// (correlated colour temperature, XYZ → camera), sorted by temperature.
    pub mats: Vec<(f32, Mat3)>,
}

/// Bradford adaptation from white `src` to white `dst` (XYZ).
pub fn bradford(src: [f32; 3], dst: [f32; 3]) -> Mat3 {
    let s = apply(&BRADFORD, src);
    let d = apply(&BRADFORD, dst);
    let scale = [[d[0] / s[0], 0.0, 0.0], [0.0, d[1] / s[1], 0.0], [0.0, 0.0, d[2] / s[2]]];
    mul(&invert(&BRADFORD).expect("Bradford is invertible"), &mul(&scale, &BRADFORD))
}

impl Calibration {
    pub fn single(m: Mat3) -> Self {
        Calibration { mats: vec![(6504.0, m)] }
    }

    /// Matrix for a scene lit at `temp` kelvin.
    pub fn at(&self, temp: f32) -> Mat3 {
        let mats = &self.mats;
        if mats.len() == 1 || temp <= mats[0].0 {
            return mats[0].1;
        }
        let last = mats[mats.len() - 1];
        if temp >= last.0 {
            return last.1;
        }
        for pair in mats.windows(2) {
            let ((t1, m1), (t2, m2)) = (pair[0], pair[1]);
            if temp >= t1 && temp <= t2 {
                let w = ((1.0 / temp - 1.0 / t2) / (1.0 / t1 - 1.0 / t2)).clamp(0.0, 1.0);
                let mut o = [[0f32; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        o[i][j] = w * m1[i][j] + (1.0 - w) * m2[i][j];
                    }
                }
                return o;
            }
        }
        last.1
    }

    pub fn multipliers_for(&self, temperature: f32, tint: f32) -> [f32; 3] {
        wb::multipliers_for(&self.at(temperature), temperature, tint)
    }

    /// Temperature/tint of camera multipliers, iterating because the matrix
    /// itself depends on the temperature.
    pub fn temperature_tint(&self, mult: [f32; 3]) -> (f32, f32) {
        let mut tt = wb::temperature_tint(&self.at(6504.0), mult);
        if self.mats.len() > 1 {
            for _ in 0..6 {
                let next = wb::temperature_tint(&self.at(tt.0), mult);
                let done = (next.0 - tt.0).abs() < 1.0;
                tt = next;
                if done {
                    break;
                }
            }
        }
        tt
    }

    /// White-balanced camera RGB (multipliers `mult` already applied) →
    /// linear Rec.2020, mapping the balanced neutral (1, 1, 1) to white.
    pub fn cam_to_working(&self, mult: [f32; 3]) -> Mat3 {
        let (t, _) = self.temperature_tint(mult);
        let Some(cam_to_xyz) = invert(&self.at(t)) else { return crate::color::IDENTITY };
        let unbalance = [[1.0 / mult[0], 0.0, 0.0], [0.0, 1.0 / mult[1], 0.0], [0.0, 0.0, 1.0 / mult[2]]];
        let to_xyz = mul(&cam_to_xyz, &unbalance);
        let white = apply(&to_xyz, [1.0, 1.0, 1.0]);
        if !(white[1] > 1e-6) || white.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return crate::color::IDENTITY;
        }
        let white_n = [white[0] / white[1], 1.0, white[2] / white[1]];
        let adapt = bradford(white_n, D65_XYZ);
        let xyz_to_working = invert(&REC2020_TO_XYZ).expect("Rec.2020 matrix is invertible");
        let mut m = mul(&xyz_to_working, &mul(&adapt, &to_xyz));
        let s = 1.0 / white[1];
        m.iter_mut().flatten().for_each(|v| *v *= s);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::SRGB_TO_XYZ;

    #[test]
    fn balanced_neutral_is_white_at_any_temperature() {
        let d65 = invert(&SRGB_TO_XYZ).unwrap();
        // Real A and D65 matrices differ by a few percent (same sensor,
        // different fit), so perturb rather than adapt.
        let a = mul(&[[1.04, 0.02, 0.0], [0.0, 1.0, 0.0], [0.0, -0.02, 0.95]], &d65);
        let cal = Calibration { mats: vec![(2856.0, a), (6504.0, d65)] };
        for t in [2500.0, 3200.0, 5000.0, 6504.0, 9000.0] {
            let mult = cal.multipliers_for(t, 5.0);
            let m = cal.cam_to_working(mult);
            let w = apply(&m, [1.0, 1.0, 1.0]);
            for c in w {
                assert!((c - 1.0).abs() < 2e-3, "{t}: {w:?}");
            }
            let (t2, tint2) = cal.temperature_tint(mult);
            assert!((t2 - t).abs() / t < 0.01 && (tint2 - 5.0).abs() < 1.0, "{t} -> {t2} {tint2}");
        }
    }

    #[test]
    fn interpolates_in_inverse_temperature() {
        let a = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [[3.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 3.0]];
        let cal = Calibration { mats: vec![(2856.0, a), (6504.0, b)] };
        assert_eq!(cal.at(2000.0)[0][0], 1.0);
        assert_eq!(cal.at(8000.0)[0][0], 3.0);
        // Halfway in mireds, not kelvin.
        let mid = 2.0 / (1.0 / 2856.0 + 1.0 / 6504.0);
        assert!((cal.at(mid)[0][0] - 2.0).abs() < 1e-3);
    }
}
