//! RAW file → normalised sensor data plus the metadata the pipeline needs.
//!
//! rawler does the container and compression work. Everything after that —
//! black/white levels, crop, colour matrices, white balance — is done here so
//! the maths is ours and testable.

use std::fmt;

use rawler::decoders::{Orientation, RawDecodeParams};
use rawler::imgop::xyz::Illuminant;
use rawler::rawimage::{RawImage, RawPhotometricInterpretation};
use rawler::rawsource::RawSource;
use serde::Serialize;

use crate::calib::Calibration;
use crate::color::{self, Mat3};

#[derive(Debug, Clone)]
pub struct RawError(pub String);

impl fmt::Display for RawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RawError {}

impl From<rawler::RawlerError> for RawError {
    fn from(e: rawler::RawlerError) -> Self {
        RawError(e.to_string())
    }
}

/// How the samples in `Decoded::data` are laid out.
#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    /// One sample per pixel; `pattern[y % 2][x % 2]` is 0 = R, 1 = G, 2 = B.
    Bayer([[u8; 2]; 2]),
    /// One sample per pixel on Fujifilm's 6×6 pattern.
    XTrans([[u8; 6]; 6]),
    /// Three samples per pixel (linear DNG, sRAW).
    Rgb,
    /// One sample per pixel, no colour filter.
    Mono,
}

impl Layout {
    pub fn color_at(&self, y: usize, x: usize) -> usize {
        match self {
            Layout::Bayer(p) => p[y & 1][x & 1] as usize,
            Layout::XTrans(p) => p[y % 6][x % 6] as usize,
            Layout::Rgb | Layout::Mono => 1,
        }
    }
}

/// White balance as the UI shows it, plus the multipliers behind it.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct WhiteBalance {
    pub temperature: f32,
    pub tint: f32,
    /// Camera-space multipliers, green = 1.
    pub multipliers: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub make: String,
    pub model: String,
    /// Output size after crop and orientation.
    pub width: u32,
    pub height: u32,
    pub iso: Option<u32>,
    /// Seconds.
    pub exposure: Option<f32>,
    /// f-number.
    pub aperture: Option<f32>,
    /// Millimetres.
    pub focal: Option<f32>,
    pub lens: Option<String>,
    pub as_shot_wb: WhiteBalance,
    /// EXIF orientation 1..8.
    pub orientation: u8,
    /// "bayer", "x-trans", "linear" or "mono".
    pub sensor: &'static str,
}

/// Sensor data ready for demosaicing: black subtracted, scaled so the white
/// level is 1.0, cropped to the recommended image area.
pub struct Decoded {
    pub info: Info,
    pub width: usize,
    pub height: usize,
    pub layout: Layout,
    /// `width * height` samples (or `* 3` for `Layout::Rgb`), in 0..=1.
    pub data: Vec<f32>,
    /// Camera matrices by illuminant.
    pub calib: Calibration,
    /// DNG BaselineExposure, when the file carries one.
    pub baseline_ev: Option<f32>,
}

fn orientation_u8(o: Orientation) -> u8 {
    match o.to_u16() {
        v @ 1..=8 => v as u8,
        _ => 1,
    }
}

/// DNG-only extras rawler exposes but does not apply.
#[derive(Default)]
struct DngExtras {
    opcode_list2: Option<Vec<u8>>,
    baseline_ev: Option<f32>,
}

fn load(bytes: &[u8], dummy: bool) -> Result<(RawImage, rawler::decoders::RawMetadata, DngExtras), RawError> {
    use rawler::decoders::WellKnownIFD;
    use rawler::tags::DngTag;
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src)?;
    let params = RawDecodeParams::default();
    let raw = decoder.raw_image(&src, &params, dummy)?;
    let meta = decoder.raw_metadata(&src, &params)?;
    let mut extras = DngExtras::default();
    if let Ok(Some(ifd)) = decoder.ifd(WellKnownIFD::VirtualDngRawTags) {
        if let Some(e) = ifd.get_entry(DngTag::OpcodeList2) {
            if let rawler::formats::tiff::Value::Undefined(d) | rawler::formats::tiff::Value::Byte(d) = &e.value {
                extras.opcode_list2 = Some(d.clone());
            }
        }
    }
    if let Ok(Some(ifd)) = decoder.ifd(WellKnownIFD::VirtualDngRootTags) {
        let get = |tag| ifd.get_entry(tag).map(|e| e.value.force_f32(0)).filter(|v| v.is_finite());
        if let Some(b) = get(DngTag::BaselineExposure) {
            extras.baseline_ev = Some(b + get(DngTag::BaselineExposureOffset).unwrap_or(0.0));
        }
    }
    Ok((raw, meta, extras))
}

/// One DNG GainMap opcode (DNG 1.3 §6, opcode 9).
#[derive(Debug, Clone, PartialEq)]
pub struct GainMap {
    pub top: usize,
    pub left: usize,
    pub bottom: usize,
    pub right: usize,
    pub plane: usize,
    pub planes: usize,
    pub row_pitch: usize,
    pub col_pitch: usize,
    pub points_v: usize,
    pub points_h: usize,
    pub spacing_v: f64,
    pub spacing_h: f64,
    pub origin_v: f64,
    pub origin_h: f64,
    pub map_planes: usize,
    pub gains: Vec<f32>,
}

/// Parse the GainMap opcodes out of an OpcodeList (big-endian). Other
/// opcodes are skipped.
pub fn parse_gain_maps(list: &[u8]) -> Vec<GainMap> {
    let be32 = |b: &[u8], o: usize| b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let be64f = |b: &[u8], o: usize| b.get(o..o + 8).map(|s| f64::from_be_bytes(s.try_into().expect("8 bytes")));
    let mut out = Vec::new();
    let Some(count) = be32(list, 0) else { return out };
    let mut off = 4;
    for _ in 0..count.min(1024) {
        let (Some(id), Some(len)) = (be32(list, off), be32(list, off + 12)) else { break };
        let body = off + 16;
        let len = len as usize;
        if body + len > list.len() {
            break;
        }
        if id == 9 && len >= 76 {
            let p = &list[body..body + len];
            let u = |o: usize| be32(p, o).unwrap_or(0) as usize;
            let f = |o: usize| be64f(p, o).unwrap_or(0.0);
            let (points_v, points_h, map_planes) = (u(32), u(36), u(72));
            let n = points_v * points_h * map_planes;
            if n > 0 && 76 + n * 4 <= p.len() {
                let gains = (0..n).map(|i| f32::from_be_bytes(p[76 + i * 4..80 + i * 4].try_into().expect("4 bytes"))).collect();
                out.push(GainMap {
                    top: u(0),
                    left: u(4),
                    bottom: u(8),
                    right: u(12),
                    plane: u(16),
                    planes: u(20),
                    row_pitch: u(24).max(1),
                    col_pitch: u(28).max(1),
                    points_v,
                    points_h,
                    spacing_v: f(40),
                    spacing_h: f(48),
                    origin_v: f(56),
                    origin_h: f(64),
                    map_planes,
                    gains,
                });
            }
        }
        off = body + len;
    }
    out
}

impl GainMap {
    /// Gain at sensor position (row, col) for map plane `mp`, bilinear.
    pub fn gain(&self, row: usize, col: usize, mp: usize) -> f32 {
        let hgt = (self.bottom.saturating_sub(self.top)).max(1) as f64;
        let wid = (self.right.saturating_sub(self.left)).max(1) as f64;
        let rv = ((row as f64 - self.top as f64) / hgt - self.origin_v) / self.spacing_v.max(1e-9);
        let rh = ((col as f64 - self.left as f64) / wid - self.origin_h) / self.spacing_h.max(1e-9);
        let rv = rv.clamp(0.0, (self.points_v - 1) as f64);
        let rh = rh.clamp(0.0, (self.points_h - 1) as f64);
        let (r0, c0) = (rv as usize, rh as usize);
        let (r1, c1) = ((r0 + 1).min(self.points_v - 1), (c0 + 1).min(self.points_h - 1));
        let (tv, th) = ((rv - r0 as f64) as f32, (rh - c0 as f64) as f32);
        let mp = mp.min(self.map_planes - 1);
        let at = |r: usize, c: usize| self.gains[(r * self.points_h + c) * self.map_planes + mp];
        let a = at(r0, c0) + (at(r0, c1) - at(r0, c0)) * th;
        let b = at(r1, c0) + (at(r1, c1) - at(r1, c0)) * th;
        a + (b - a) * tv
    }
}


/// The recommended crop in full-sensor coordinates: (x, y, w, h).
fn crop_rect(raw: &RawImage) -> (usize, usize, usize, usize) {
    let full = (0, 0, raw.width, raw.height);
    let r = match raw.crop_area.or(raw.active_area) {
        Some(r) => (r.p.x, r.p.y, r.d.w, r.d.h),
        None => return full,
    };
    if r.2 == 0 || r.3 == 0 || r.0 + r.2 > raw.width || r.1 + r.3 > raw.height {
        return full;
    }
    r
}

fn layout_of(raw: &RawImage, cx: usize, cy: usize) -> Result<Layout, RawError> {
    match &raw.photometric {
        RawPhotometricInterpretation::Cfa(cfg) => {
            let cfa = &cfg.cfa;
            if !cfa.is_rgb() {
                return Err(RawError(format!("Colour filter pattern {cfa} is not supported yet")));
            }
            let map = |c: usize| -> Result<u8, RawError> {
                match c {
                    0..=2 => Ok(c as u8),
                    _ => Err(RawError("Four-colour sensors are not supported yet".into())),
                }
            };
            match (cfa.width, cfa.height) {
                (2, 2) => {
                    let mut p = [[0u8; 2]; 2];
                    for (y, row) in p.iter_mut().enumerate() {
                        for (x, v) in row.iter_mut().enumerate() {
                            *v = map(cfa.color_at(y + cy, x + cx))?;
                        }
                    }
                    Ok(Layout::Bayer(p))
                }
                (6, 6) => {
                    let mut p = [[0u8; 6]; 6];
                    for (y, row) in p.iter_mut().enumerate() {
                        for (x, v) in row.iter_mut().enumerate() {
                            *v = map(cfa.color_at(y + cy, x + cx))?;
                        }
                    }
                    Ok(Layout::XTrans(p))
                }
                (w, h) => Err(RawError(format!("{w}×{h} colour filter patterns are not supported yet"))),
            }
        }
        RawPhotometricInterpretation::LinearRaw if raw.cpp == 3 => Ok(Layout::Rgb),
        RawPhotometricInterpretation::BlackIsZero if raw.cpp == 1 => Ok(Layout::Mono),
        _ => Err(RawError(format!("{} samples per pixel are not supported yet", raw.cpp))),
    }
}

fn illuminant_temperature(i: Illuminant) -> Option<f32> {
    Some(match i {
        Illuminant::A | Illuminant::Tungsten | Illuminant::IsoStudioTungsten => 2856.0,
        Illuminant::WhiteFluorescent => 3450.0,
        Illuminant::CoolWhiteFluorescent => 4150.0,
        Illuminant::Fluorescent => 4230.0,
        Illuminant::D50 => 5003.0,
        Illuminant::B => 4874.0,
        Illuminant::D55 | Illuminant::Daylight | Illuminant::FineWeather | Illuminant::Flash => 5503.0,
        Illuminant::DaylightWhiteFluorescent => 5050.0,
        Illuminant::DaylightFluorescent => 6430.0,
        Illuminant::D65 | Illuminant::CloudyWeather => 6504.0,
        Illuminant::C => 6774.0,
        Illuminant::D75 | Illuminant::Shade => 7504.0,
        Illuminant::Unknown => return None,
    })
}

/// Every usable XYZ → camera matrix, keyed by the temperature it was
/// profiled at.
fn calibration(raw: &RawImage) -> Calibration {
    let mut mats: Vec<(f32, Mat3)> = Vec::new();
    for (illu, m) in &raw.color_matrix {
        let Some(t) = illuminant_temperature(*illu) else { continue };
        if m.len() < 9 || !m[..9].iter().all(|v| v.is_finite()) || m[..9].iter().all(|v| *v == 0.0) {
            continue;
        }
        if mats.iter().any(|(t2, _)| (*t2 - t).abs() < 1.0) {
            continue;
        }
        mats.push((t, [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]]));
    }
    mats.sort_by(|a, b| a.0.total_cmp(&b.0));
    if !mats.is_empty() {
        return Calibration { mats };
    }
    // Fall back to the legacy field, then to "camera RGB is sRGB".
    let l = raw.xyz_to_cam;
    let mat = [l[0], l[1], l[2]];
    if mat.iter().flatten().any(|v| *v != 0.0) && mat.iter().flatten().all(|v| v.is_finite()) {
        return Calibration::single(mat);
    }
    Calibration::single(color::invert(&color::SRGB_TO_XYZ).unwrap_or(color::IDENTITY))
}

fn as_shot_multipliers(raw: &RawImage, calib: &Calibration) -> [f32; 3] {
    let c = raw.wb_coeffs;
    if c[..3].iter().all(|v| v.is_finite() && *v > 0.0) {
        return [c[0] / c[1], 1.0, c[2] / c[1]];
    }
    calib.multipliers_for(6504.0, 0.0)
}

fn info_from(raw: &RawImage, meta: &rawler::decoders::RawMetadata, calib: &Calibration, crop: (usize, usize, usize, usize), sensor: &'static str) -> Info {
    let ex = &meta.exif;
    let orientation = ex.orientation.map(|o| o as u8).filter(|o| (1..=8).contains(o)).unwrap_or_else(|| orientation_u8(raw.orientation));
    let (mut w, mut h) = (crop.2 as u32, crop.3 as u32);
    if orientation >= 5 {
        std::mem::swap(&mut w, &mut h);
    }
    let mult = as_shot_multipliers(raw, calib);
    let (temperature, tint) = calib.temperature_tint(mult);
    let rat = |r: Option<rawler::formats::tiff::Rational>| r.map(|r| r.n as f32 / r.d.max(1) as f32).filter(|v| v.is_finite() && *v > 0.0);
    Info {
        make: if meta.make.is_empty() { raw.clean_make.clone() } else { meta.make.clone() },
        model: if meta.model.is_empty() { raw.clean_model.clone() } else { meta.model.clone() },
        width: w,
        height: h,
        iso: ex.iso_speed_ratings.map(u32::from).or(ex.iso_speed).or(ex.recommended_exposure_index).filter(|v| *v > 0),
        exposure: rat(ex.exposure_time),
        aperture: rat(ex.fnumber),
        focal: rat(ex.focal_length),
        lens: ex.lens_model.clone().filter(|s| !s.trim().is_empty()),
        as_shot_wb: WhiteBalance { temperature, tint, multipliers: mult },
        orientation,
        sensor,
    }
}

fn sensor_name(layout: &Layout) -> &'static str {
    match layout {
        Layout::Bayer(_) => "bayer",
        Layout::XTrans(_) => "x-trans",
        Layout::Rgb => "linear",
        Layout::Mono => "mono",
    }
}

/// Metadata only; does not decompress the image.
pub fn probe(bytes: &[u8]) -> Result<Info, RawError> {
    let (raw, meta, _) = load(bytes, true)?;
    let crop = crop_rect(&raw);
    let layout = layout_of(&raw, crop.0, crop.1)?;
    let calib = calibration(&raw);
    Ok(info_from(&raw, &meta, &calib, crop, sensor_name(&layout)))
}

/// Per-sample black level at full-sensor position (x, y), channel `c`.
fn black_at(levels: &[f32], bw: usize, bh: usize, cpp: usize, x: usize, y: usize, c: usize) -> f32 {
    if levels.len() == 1 {
        return levels[0];
    }
    let i = ((y % bh.max(1)) * bw.max(1) + (x % bw.max(1))) * cpp + c;
    levels.get(i).copied().unwrap_or(levels[0])
}

/// Decode pixels and normalise them.
pub fn decode(bytes: &[u8]) -> Result<Decoded, RawError> {
    let (raw, meta, extras) = load(bytes, false)?;
    if raw.fuji_rotation_width.is_some() {
        return Err(RawError("Fujifilm SuperCCD (rotated sensor) files are not supported yet".into()));
    }
    let crop = crop_rect(&raw);
    let layout = layout_of(&raw, crop.0, crop.1)?;
    let calib = calibration(&raw);
    let info = info_from(&raw, &meta, &calib, crop, sensor_name(&layout));

    let cpp = raw.cpp;
    let blacks = raw.blacklevel.as_vec();
    let (bw, bh) = (raw.blacklevel.width, raw.blacklevel.height);
    let whites = raw.whitelevel.as_vec();
    let white_at = |x: usize, y: usize, c: usize| -> f32 {
        match whites.len() {
            1 => whites[0],
            n if n == cpp => whites[c],
            4 if cpp == 1 => whites[(y & 1) * 2 + (x & 1)],
            _ => whites[0],
        }
    };
    let (cx, cy, w, h) = crop;
    let src_w = raw.width * cpp;
    let px = |i: usize| -> f32 {
        match &raw.data {
            rawler::RawImageData::Integer(d) => d[i] as f32,
            rawler::RawImageData::Float(d) => d[i],
        }
    };
    let mut data = vec![0f32; w * h * cpp];
    {
        use rayon::prelude::*;
        data.par_chunks_mut(w * cpp).enumerate().for_each(|(y, row)| {
            let sy = y + cy;
            for x in 0..w {
                let sx = x + cx;
                for c in 0..cpp {
                    let b = black_at(&blacks, bw, bh, cpp, sx, sy, c);
                    let range = (white_at(sx, sy, c) - b).max(1e-6);
                    let v = (px(sy * src_w + sx * cpp + c) - b) / range;
                    row[x * cpp + c] = v.clamp(0.0, 1.0);
                }
            }
        });
    }
    // Lens shading on phones: DNG OpcodeList2 gain maps, applied to the
    // normalised (stage 2) image in sensor coordinates.
    let maps = extras.opcode_list2.as_deref().map(parse_gain_maps).unwrap_or_default();
    if !maps.is_empty() {
        use rayon::prelude::*;
        data.par_chunks_mut(w * cpp).enumerate().for_each(|(y, row)| {
            let sy = y + cy;
            for m in &maps {
                if sy < m.top || sy >= m.bottom || (sy - m.top) % m.row_pitch != 0 {
                    continue;
                }
                let x0 = m.left.max(cx);
                for sx in x0..m.right.min(cx + w) {
                    if (sx - m.left) % m.col_pitch != 0 {
                        continue;
                    }
                    for c in m.plane..(m.plane + m.planes).min(cpp) {
                        let g = m.gain(sy, sx, c - m.plane);
                        let v = &mut row[(sx - cx) * cpp + c];
                        *v = (*v * g).clamp(0.0, 1.0);
                    }
                }
            }
        });
    }
    Ok(Decoded { info, width: w, height: h, layout, data, calib, baseline_ev: extras.baseline_ev })
}

#[cfg(test)]
pub(crate) fn tests_info() -> Info {
    Info {
        make: "Test".into(),
        model: "Synthetic".into(),
        width: 0,
        height: 0,
        iso: None,
        exposure: None,
        aperture: None,
        focal: None,
        lens: None,
        as_shot_wb: WhiteBalance { temperature: 5000.0, tint: 0.0, multipliers: [1.0, 1.0, 1.0] },
        orientation: 1,
        sensor: "bayer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_level_pattern_lookup() {
        // 2×2 repeating black levels, one channel.
        let levels = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(black_at(&levels, 2, 2, 1, 0, 0, 0), 10.0);
        assert_eq!(black_at(&levels, 2, 2, 1, 1, 0, 0), 20.0);
        assert_eq!(black_at(&levels, 2, 2, 1, 0, 1, 0), 30.0);
        assert_eq!(black_at(&levels, 2, 2, 1, 3, 3, 0), 40.0);
        assert_eq!(black_at(&[512.0], 1, 1, 1, 7, 9, 0), 512.0);
    }

    #[test]
    fn gain_map_parses_and_interpolates() {
        let mut list = Vec::new();
        list.extend_from_slice(&1u32.to_be_bytes());
        let mut body = Vec::new();
        for v in [0u32, 0, 10, 10, 0, 1, 1, 1, 2, 2] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        for v in [1.0f64, 1.0, 0.0, 0.0] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&1u32.to_be_bytes());
        for g in [1.0f32, 2.0, 3.0, 4.0] {
            body.extend_from_slice(&g.to_be_bytes());
        }
        list.extend_from_slice(&9u32.to_be_bytes());
        list.extend_from_slice(&0x0103_0000u32.to_be_bytes());
        list.extend_from_slice(&0u32.to_be_bytes());
        list.extend_from_slice(&(body.len() as u32).to_be_bytes());
        list.extend_from_slice(&body);
        let maps = parse_gain_maps(&list);
        assert_eq!(maps.len(), 1);
        let m = &maps[0];
        assert_eq!(m.gain(0, 0, 0), 1.0);
        assert_eq!(m.gain(10, 10, 0), 4.0);
        assert!((m.gain(5, 5, 0) - 2.5).abs() < 1e-6);
    }

    #[test]
    fn layout_color_lookup() {
        let l = Layout::Bayer([[0, 1], [1, 2]]);
        assert_eq!(l.color_at(0, 0), 0);
        assert_eq!(l.color_at(0, 1), 1);
        assert_eq!(l.color_at(3, 3), 2);
    }
}
