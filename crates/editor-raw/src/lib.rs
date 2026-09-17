//! Camera RAW for OpenPhotoshop: decode (via rawler), demosaic, colour and a
//! Camera-Raw-style develop stage, all in f32 linear light, ending in 8-bit
//! sRGB for the document.
//!
//! ```text
//! bytes ─ decode ─▶ normalised mosaic ─┬─ superpixel ─▶ preview cache (½ or ⅓ size)
//!                                     └─ WB → RCD / Markesteijn ─▶ full size
//!         ─▶ highlight reconstruction ─▶ camera matrix ─▶ develop ─▶ RGBA8
//! ```
//!
//! [`Session`] keeps one decoded file so slider changes only re-run develop.

pub mod calib;
pub mod color;
pub mod decode;
pub mod demosaic;
pub mod develop;
pub mod filters;
pub mod highlight;
pub mod orient;
pub mod preview;
pub mod tone;
pub mod wb;

pub use decode::{probe, Decoded, Info, Layout, RawError, WhiteBalance};
pub use develop::{DevelopParams, Output};

/// Interleaved f32 RGB.
#[derive(Clone)]
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

/// File extensions (lower case) this crate is meant to open.
pub const EXTENSIONS: &[&str] = &[
    "3fr", "ari", "arw", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "erf", "iiq", "kdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "ori", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw", "x3f",
];

/// A cheap identity for a file's bytes: length plus a hash of samples.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ bytes.len() as u64;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h ^= x as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    let n = bytes.len();
    eat(&bytes[..n.min(65536)]);
    eat(&bytes[n.saturating_sub(65536)..]);
    let step = (n / 64).max(1);
    for i in (0..n).step_by(step) {
        eat(&bytes[i..(i + 64).min(n)]);
    }
    h
}

/// One decoded RAW file and its preview-sized camera RGB.
pub struct Session {
    pub key: u64,
    pub dec: Decoded,
    half: Option<Rgb>,
    auto_wb: Option<[f32; 3]>,
}

impl Session {
    pub fn open(bytes: &[u8]) -> Result<Session, RawError> {
        let dec = decode::decode(bytes)?;
        Ok(Session { key: fingerprint(bytes), dec, half: None, auto_wb: None })
    }

    pub fn info(&self) -> &Info {
        &self.dec.info
    }

    fn half(&mut self) -> &Rgb {
        if self.half.is_none() {
            self.half = Some(demosaic::superpixel(&self.dec));
        }
        self.half.as_ref().expect("just built")
    }

    /// Resolve the white balance a parameter set asks for.
    pub fn white_balance(&mut self, p: &DevelopParams) -> WhiteBalance {
        let cal = self.dec.calib.clone();
        let (temperature, tint) = match p.wb.as_str() {
            "as-shot" | "" => return self.dec.info.as_shot_wb,
            "auto" => {
                if self.auto_wb.is_none() {
                    let est = wb::auto_multipliers(&self.half().data, &cal);
                    self.auto_wb = Some(est);
                }
                let mult = self.auto_wb.expect("just set");
                let (t, tn) = cal.temperature_tint(mult);
                return WhiteBalance { temperature: t, tint: tn, multipliers: mult };
            }
            name => wb::preset(name).unwrap_or((p.temperature, p.tint)),
        };
        let t = temperature.clamp(wb::TEMP_MIN, wb::TEMP_MAX);
        let tint = tint.clamp(-150.0, 150.0);
        WhiteBalance { temperature: t, tint, multipliers: cal.multipliers_for(t, tint) }
    }

    /// Develop at preview size (`half`: superpixel binning) or full resolution.
    pub fn develop(&mut self, p: &DevelopParams, half: bool) -> Output {
        self.develop_at(p, if half { Size::Half } else { Size::Full })
    }

    pub fn develop_at(&mut self, p: &DevelopParams, size: Size) -> Output {
        let wbal = self.white_balance(p);
        let mult = wbal.multipliers;
        let orientation = self.dec.info.orientation;
        let to_working = self.dec.calib.cam_to_working(mult);
        let baseline = self.dec.baseline_ev.unwrap_or(develop::look::BASELINE_EV);
        let full_w = self.dec.width as f32;
        let mut img = match size {
            Size::Full => {
                let img = demosaic::full(&self.dec, mult);
                return develop::run(img, mult, &to_working, p, baseline, orientation, 1.0);
            }
            Size::Half => self.half().clone(),
            Size::Fit(max) => {
                let h = self.half();
                let f = h.w.max(h.h).div_ceil(max.max(16)).max(1);
                if f == 1 {
                    h.clone()
                } else {
                    downsample_rgb(h, f)
                }
            }
        };
        img.data.chunks_exact_mut(3).for_each(|q| (0..3).for_each(|c| q[c] *= mult[c]));
        let scale = img.w as f32 / full_w;
        develop::run(img, mult, &to_working, p, baseline, orientation, scale)
    }
}

/// Output size for [`Session::develop_at`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    Full,
    /// Superpixel binning: ½ (Bayer) or ⅓ (X-Trans).
    Half,
    /// Superpixel, then box-downsampled so the long side is at most this.
    Fit(usize),
}

/// Integer box downsample of camera RGB. A channel that clipped anywhere in
/// the box stays clipped (1.0), so highlight reconstruction still sees it.
fn downsample_rgb(src: &Rgb, f: usize) -> Rgb {
    use rayon::prelude::*;
    let (w, h) = (src.w / f, src.h / f);
    let (w, h) = (w.max(1), h.max(1));
    let mut data = vec![0f32; w * h * 3];
    data.par_chunks_mut(w * 3).enumerate().for_each(|(oy, row)| {
        for ox in 0..w {
            let mut s = [0f32; 3];
            let mut clip = [false; 3];
            let mut n = 0;
            for y in oy * f..((oy + 1) * f).min(src.h) {
                for x in ox * f..((ox + 1) * f).min(src.w) {
                    let i = (y * src.w + x) * 3;
                    for c in 0..3 {
                        let v = src.data[i + c];
                        s[c] += v;
                        clip[c] |= v >= 1.0;
                    }
                    n += 1;
                }
            }
            for c in 0..3 {
                row[ox * 3 + c] = if clip[c] { 1.0 } else { s[c] / n.max(1) as f32 };
            }
        }
    });
    Rgb { w, h, data }
}
