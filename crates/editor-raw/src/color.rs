//! Colour matrices and transfer functions.
//!
//! The working space is linear Rec.2020 (D65). It is wide enough that camera
//! colours rarely go negative, and its D65 white matches the camera matrices,
//! so no chromatic adaptation is needed on the way in or out.

pub type Mat3 = [[f32; 3]; 3];

pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Linear sRGB (D65) → XYZ.
pub const SRGB_TO_XYZ: Mat3 = [[0.412_456_4, 0.357_576_1, 0.180_437_5], [0.212_672_9, 0.715_152_2, 0.072_175], [0.019_333_9, 0.119_192, 0.950_304_1]];

/// Linear Rec.2020 (D65) → XYZ.
pub const REC2020_TO_XYZ: Mat3 = [[0.636_958, 0.144_616_9, 0.168_881], [0.262_700_2, 0.677_998, 0.059_301_7], [0.0, 0.028_072_7, 1.060_985]];

/// Luminance weights of Rec.2020 primaries.
pub const REC2020_LUMA: [f32; 3] = [0.262_700_2, 0.677_998, 0.059_301_7];

pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut o = [[0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] as f64 * b[k][j] as f64).sum::<f64>() as f32;
        }
    }
    o
}

pub fn apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

pub fn invert(m: &Mat3) -> Option<Mat3> {
    let a = |i: usize, j: usize| m[i][j] as f64;
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0)) + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let inv = [
        [a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1), a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2), a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)],
        [a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2), a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0), a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)],
        [a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0), a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1), a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)],
    ];
    let mut o = [[0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (inv[i][j] / det) as f32;
        }
    }
    Some(o)
}

/// White-balanced camera RGB → linear Rec.2020.
///
/// The camera matrix is row-normalised so that a white-balanced neutral
/// (1, 1, 1) lands on the working-space white (the dcraw construction).
pub fn cam_to_working(xyz_to_cam: &Mat3) -> Mat3 {
    let mut rgb_to_cam = mul(xyz_to_cam, &REC2020_TO_XYZ);
    for row in rgb_to_cam.iter_mut() {
        let s: f32 = row.iter().sum();
        if s.abs() > 1e-6 {
            row.iter_mut().for_each(|v| *v /= s);
        }
    }
    invert(&rgb_to_cam).unwrap_or(IDENTITY)
}

/// Linear Rec.2020 → linear sRGB.
pub fn working_to_srgb() -> Mat3 {
    mul(&invert(&SRGB_TO_XYZ).expect("sRGB matrix is invertible"), &REC2020_TO_XYZ)
}

#[inline]
pub fn srgb_oetf(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// A 4096-entry lookup for the sRGB curve over 0..=1 (linear interpolation).
pub struct OetfLut(Vec<f32>);

impl OetfLut {
    pub fn new() -> Self {
        OetfLut((0..=4096).map(|i| srgb_oetf(i as f32 / 4096.0)).collect())
    }
    #[inline]
    pub fn get(&self, v: f32) -> f32 {
        let x = v.clamp(0.0, 1.0) * 4096.0;
        let i = (x as usize).min(4095);
        let f = x - i as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * f
    }
}

impl Default for OetfLut {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_round_trips() {
        let m = mul(&SRGB_TO_XYZ, &invert(&SRGB_TO_XYZ).unwrap());
        for (i, row) in m.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                assert!((v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn neutral_maps_to_white() {
        // A made-up but plausible camera matrix.
        let xyz_to_cam = [[0.68, -0.12, -0.07], [-0.47, 1.25, 0.24], [-0.08, 0.17, 0.62]];
        let m = cam_to_working(&xyz_to_cam);
        let w = apply(&m, [1.0, 1.0, 1.0]);
        for c in w {
            assert!((c - 1.0).abs() < 1e-3, "{w:?}");
        }
        let s = apply(&working_to_srgb(), [1.0, 1.0, 1.0]);
        for c in s {
            assert!((c - 1.0).abs() < 1e-3, "{s:?}");
        }
    }
}
