//! Converting decoded channels of any bit depth and colour mode into the
//! engine's 8-bit RGBA.

use crate::structure::*;

/// Samples of one channel at the file's bit depth → 8-bit. Colour channels
/// of 32-bit documents are linear light and get the sRGB curve; alpha and
/// masks do not.
pub fn to_u8(data: &[u8], depth: u16, pixels: usize, width: usize, color: bool) -> Vec<u8> {
    match depth {
        8 => {
            let mut v = data.to_vec();
            v.resize(pixels, 0);
            v
        }
        16 => {
            let mut out: Vec<u8> = data.as_chunks::<2>().0.iter().map(|c| ((u16::from_be_bytes([c[0], c[1]]) as u32 * 255 + 32767) / 65535) as u8).collect();
            out.resize(pixels, 0);
            out
        }
        32 => {
            let mut out: Vec<u8> = data
                .as_chunks::<4>().0.iter()
                .map(|c| {
                    let f = f32::from_be_bytes([c[0], c[1], c[2], c[3]]);
                    let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 };
                    let g = if color { editor_core::color::linear_to_srgb(f) } else { f };
                    (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                })
                .collect();
            out.resize(pixels, 0);
            out
        }
        1 => {
            // Set bits are black.
            let row = width.div_ceil(8);
            let mut out = vec![255u8; pixels];
            if row == 0 {
                return out;
            }
            for (i, o) in out.iter_mut().enumerate() {
                let (y, x) = (i / width, i % width);
                let byte = data.get(y * row + x / 8).copied().unwrap_or(0);
                if byte & (0x80 >> (x % 8)) != 0 {
                    *o = 0;
                }
            }
            out
        }
        _ => vec![0; pixels],
    }
}

/// Channels (already 8-bit) in the document's colour mode → interleaved RGB.
/// `chans[i]` is channel `i`; missing ones read as zero.
pub fn mode_to_rgb(mode: u16, chans: &[Option<Vec<u8>>], pixels: usize, palette: &[u8]) -> Vec<[u8; 3]> {
    let get = |i: usize, p: usize| chans.get(i).and_then(|c| c.as_ref()).and_then(|c| c.get(p)).copied().unwrap_or(0);
    let mut out = vec![[0u8; 3]; pixels];
    match mode {
        MODE_RGB => {
            for (p, o) in out.iter_mut().enumerate() {
                *o = [get(0, p), get(1, p), get(2, p)];
            }
        }
        MODE_CMYK => {
            // Stored inverted (255 = no ink). A naive conversion without an
            // ICC transform.
            for (p, o) in out.iter_mut().enumerate() {
                let k = get(3, p) as u32;
                *o = [0, 1, 2].map(|i| ((get(i, p) as u32 * k + 127) / 255) as u8);
            }
        }
        MODE_LAB => {
            for (p, o) in out.iter_mut().enumerate() {
                *o = lab_to_srgb(get(0, p) as f32 * 100.0 / 255.0, get(1, p) as f32 - 128.0, get(2, p) as f32 - 128.0);
            }
        }
        MODE_INDEXED => {
            for (p, o) in out.iter_mut().enumerate() {
                let i = get(0, p) as usize;
                *o = [0, 1, 2].map(|c| palette.get(c * 256 + i).copied().unwrap_or(0));
            }
        }
        // Grayscale, duotone, bitmap and multichannel show their first channel.
        _ => {
            for (p, o) in out.iter_mut().enumerate() {
                let v = get(0, p);
                *o = [v, v, v];
            }
        }
    }
    out
}

/// Colour channels a document of this mode has (before extra alpha channels).
pub fn color_channels(mode: u16) -> usize {
    match mode {
        MODE_RGB | MODE_LAB => 3,
        MODE_CMYK => 4,
        _ => 1,
    }
}

/// CIE L*a*b* (D50) → sRGB 8-bit.
pub fn lab_to_srgb(l: f32, a: f32, b: f32) -> [u8; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let finv = |t: f32| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * finv(fx), finv(fy), 0.8249 * finv(fz));
    xyz_d50_to_srgb(x, y, z)
}

pub fn xyz_d50_to_srgb(x: f32, y: f32, z: f32) -> [u8; 3] {
    // Bradford-adapted D50 → linear sRGB.
    let r = 3.133_856 * x - 1.616_867 * y - 0.490_614_6 * z;
    let g = -0.9787684 * x + 1.9161415 * y + 0.0334540 * z;
    let bl = 0.0719453 * x - 0.2289914 * y + 1.4052427 * z;
    [r, g, bl].map(|v| (editor_core::color::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8)
}

pub fn hsb_to_rgb(h_deg: f64, s: f64, b: f64) -> [u8; 3] {
    let c = editor_core::color::hsv_to_rgb([h_deg as f32, (s / 100.0) as f32, (b / 100.0) as f32]);
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_conversion() {
        assert_eq!(to_u8(&[0xff, 0xff, 0x80, 0x00], 16, 2, 2, true), vec![255, 128]);
        assert_eq!(to_u8(&[0b1010_0000], 1, 4, 4, true), vec![0, 255, 0, 255]);
        let one = 1.0f32.to_be_bytes();
        assert_eq!(to_u8(&one, 32, 1, 1, true), vec![255]);
    }

    #[test]
    fn lab_white_and_black() {
        assert_eq!(lab_to_srgb(100.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(lab_to_srgb(0.0, 0.0, 0.0), [0, 0, 0]);
    }
}
