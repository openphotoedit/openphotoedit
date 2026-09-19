//! Adjustment and fill layer blocks ⇄ the engine's `Adjustment` and `Fill`.

use editor_core::adjust::*;
use editor_core::color::Rgba8;
use editor_core::geom::Point;
use editor_core::layer::{Fill, GradientKind};

use crate::color::{hsb_to_rgb, lab_to_srgb};
use crate::descriptor::{Descriptor, Value};
use crate::error::{corrupt, Result};
use crate::io::{Reader, Writer};

/// Additional-info keys that make a layer an adjustment layer.
pub const ADJUSTMENT_KEYS: [&[u8; 4]; 18] = [
    b"levl", b"curv", b"hue2", b"hue ", b"CgEd", b"brit", b"blnc", b"blwh", b"mixr", b"selc", b"vibA", b"expA", b"phfl", b"grdm", b"post",
    b"thrs", b"nvrt", b"clrL",
];

pub const FILL_KEYS: [&[u8; 4]; 3] = [b"SoCo", b"GdFl", b"PtFl"];

pub type Blocks = Vec<([u8; 4], Vec<u8>)>;

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

// ---------------------------------------------------------------------------
// Colours in descriptors

pub fn descriptor_color(d: &Descriptor) -> Option<Rgba8> {
    let c = d.class.as_str();
    if let (Some(r), Some(g), Some(b)) = (d.num("Rd  "), d.num("Grn "), d.num("Bl  ")) {
        return Some(Rgba8::rgb(clamp_u8(r), clamp_u8(g), clamp_u8(b)));
    }
    if let (Some(r), Some(g), Some(b)) = (d.num("redFloat"), d.num("greenFloat"), d.num("blueFloat")) {
        return Some(Rgba8::rgb(clamp_u8(r * 255.0), clamp_u8(g * 255.0), clamp_u8(b * 255.0)));
    }
    if let (Some(h), Some(s), Some(b)) = (d.num("H   "), d.num("Strt"), d.num("Brgh")) {
        let [r, g, bl] = hsb_to_rgb(h, s, b);
        return Some(Rgba8::rgb(r, g, bl));
    }
    if let (Some(cy), Some(m), Some(y), Some(k)) = (d.num("Cyn "), d.num("Mgnt"), d.num("Ylw "), d.num("Blck")) {
        let f = |v: f64| clamp_u8((1.0 - v / 100.0) * (1.0 - k / 100.0) * 255.0);
        return Some(Rgba8::rgb(f(cy), f(m), f(y)));
    }
    if let Some(g) = d.num("Gry ") {
        let v = clamp_u8((1.0 - g / 100.0) * 255.0);
        return Some(Rgba8::rgb(v, v, v));
    }
    if let (Some(l), Some(a), Some(b)) = (d.num("Lmnc"), d.num("A   "), d.num("B   ")) {
        let [r, g, bl] = lab_to_srgb(l as f32, a as f32, b as f32);
        return Some(Rgba8::rgb(r, g, bl));
    }
    if c == "BkCl" {
        return None;
    }
    None
}

pub fn color_descriptor(c: Rgba8) -> Value {
    Value::Descriptor(
        Descriptor::new("RGBC")
            .with("Rd  ", Value::Double(c.r as f64))
            .with("Grn ", Value::Double(c.g as f64))
            .with("Bl  ", Value::Double(c.b as f64)),
    )
}

/// A `Color` structure: u16 space id and four u16 components.
fn read_color_struct(r: &mut Reader) -> Result<Rgba8> {
    let space = r.u16()?;
    let v = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
    Ok(color_struct_to_rgb(space, v))
}

fn color_struct_to_rgb(space: u16, v: [u16; 4]) -> Rgba8 {
    let n = |x: u16| x as f64 / 65535.0;
    match space {
        0 => Rgba8::rgb(clamp_u8(n(v[0]) * 255.0), clamp_u8(n(v[1]) * 255.0), clamp_u8(n(v[2]) * 255.0)),
        1 => {
            let [r, g, b] = hsb_to_rgb(n(v[0]) * 360.0, n(v[1]) * 100.0, n(v[2]) * 100.0);
            Rgba8::rgb(r, g, b)
        }
        2 => {
            // 65535 = no ink.
            let k = n(v[3]);
            Rgba8::rgb(clamp_u8(n(v[0]) * k * 255.0), clamp_u8(n(v[1]) * k * 255.0), clamp_u8(n(v[2]) * k * 255.0))
        }
        7 => {
            let [r, g, b] = lab_to_srgb(v[0] as f32 / 100.0, v[1] as i16 as f32 / 100.0, v[2] as i16 as f32 / 100.0);
            Rgba8::rgb(r, g, b)
        }
        8 => {
            let g = clamp_u8((1.0 - v[0] as f64 / 10000.0) * 255.0);
            Rgba8::rgb(g, g, g)
        }
        _ => Rgba8::BLACK,
    }
}

fn write_color_struct(w: &mut Writer, c: Rgba8) {
    w.u16(0);
    w.u16(c.r as u16 * 257);
    w.u16(c.g as u16 * 257);
    w.u16(c.b as u16 * 257);
    w.u16(0);
}

// ---------------------------------------------------------------------------
// Adjustments: reading

/// The adjustment a layer's blocks describe, if any. `Ok(None)` means no
/// adjustment key; `Err` means one is present but not understood.
pub fn read_adjustment<'a>(get: &dyn Fn(&[u8; 4]) -> Option<&'a [u8]>) -> Option<std::result::Result<Adjustment, String>> {
    let present = ADJUSTMENT_KEYS.iter().find(|k| get(k).is_some())?;
    // Brightness/contrast writes both `brit` and `CgEd`; CgEd wins.
    if let Some(d) = get(b"CgEd") {
        let r = read_cged(d).map_err(|e| e.to_string());
        if let Ok(Some(a)) = r {
            return Some(Ok(a));
        }
        if present == &b"CgEd" && ADJUSTMENT_KEYS.iter().filter(|k| **k != b"CgEd").all(|k| get(k).is_none()) {
            return Some(Err("brightness/contrast settings".into()));
        }
    }
    for key in ADJUSTMENT_KEYS {
        if key == b"CgEd" {
            continue;
        }
        let Some(data) = get(key) else { continue };
        let res = read_one(key, data);
        return Some(res.map_err(|e| format!("{} ({e})", String::from_utf8_lossy(key))));
    }
    None
}

fn read_cged(data: &[u8]) -> Result<Option<Adjustment>> {
    let d = Descriptor::read_versioned(&mut Reader::new(data))?;
    match (d.num("Brgh"), d.num("Cntr")) {
        (Some(b), Some(c)) => Ok(Some(Adjustment::BrightnessContrast(BrightnessContrast {
            brightness: b as f32,
            contrast: c as f32,
            legacy: d.boolean("useLegacy").unwrap_or(false),
        }))),
        _ => Ok(None),
    }
}

fn read_one(key: &[u8; 4], data: &[u8]) -> Result<Adjustment> {
    let mut r = Reader::new(data);
    Ok(match key {
        b"levl" => {
            let v = r.u16()?;
            if v != 2 {
                return Err(corrupt(format!("levels version {v}")));
            }
            let ch = |r: &mut Reader| -> Result<LevelsChannel> {
                let (ib, iw, ob, ow, g) = (r.i16()?, r.i16()?, r.i16()?, r.i16()?, r.i16()?);
                Ok(LevelsChannel { in_black: ib as f32, in_white: iw as f32, gamma: g as f32 / 100.0, out_black: ob as f32, out_white: ow as f32 })
            };
            let master = ch(&mut r)?;
            let red = ch(&mut r)?;
            let green = ch(&mut r)?;
            let blue = ch(&mut r)?;
            Adjustment::Levels(Levels { master, red, green, blue })
        }
        b"curv" => Adjustment::Curves(read_curves(&mut r)?),
        b"hue2" | b"hue " => {
            let _version = r.u16()?;
            let colorize = r.u8()? != 0;
            r.u8()?;
            let (ch, cs, cl) = (r.i16()?, r.i16()?, r.i16()?);
            let master = HslShift { hue: r.i16()? as f32, saturation: r.i16()? as f32, lightness: r.i16()? as f32 };
            let mut ranges: [HslShift; 6] = Default::default();
            let mut bands = HUE_BANDS_DEFAULT;
            for (range, band) in ranges.iter_mut().zip(bands.iter_mut()) {
                // Photoshop's order: falloff start, range start, range end,
                // falloff end (psd-tools reads the same four i16 values).
                // Kept as written, so a re-export reproduces the bytes.
                *band = [r.i16()? as f32, r.i16()? as f32, r.i16()? as f32, r.i16()? as f32];
                *range = HslShift { hue: r.i16()? as f32, saturation: r.i16()? as f32, lightness: r.i16()? as f32 };
            }
            Adjustment::HueSaturation(HueSaturation {
                master,
                ranges,
                colorize,
                colorize_hue: (ch as f32).rem_euclid(360.0),
                colorize_saturation: cs as f32,
                colorize_lightness: cl as f32,
                bands,
            })
        }
        b"brit" => {
            let (b, c) = (r.i16()?, r.i16()?);
            Adjustment::BrightnessContrast(BrightnessContrast { brightness: b as f32, contrast: c as f32, legacy: true })
        }
        b"blnc" => {
            let three = |r: &mut Reader| -> Result<[f32; 3]> { Ok([r.i16()? as f32, r.i16()? as f32, r.i16()? as f32]) };
            let shadows = three(&mut r)?;
            let midtones = three(&mut r)?;
            let highlights = three(&mut r)?;
            let preserve_luminosity = r.u8().map(|v| v != 0).unwrap_or(true);
            Adjustment::ColorBalance(ColorBalance { shadows, midtones, highlights, preserve_luminosity })
        }
        b"blwh" => {
            let d = Descriptor::read_versioned(&mut r)?;
            let def = BlackWhite::default();
            let w = |k: &str, i: usize| d.num(k).map(|v| v as f32).unwrap_or(def.weights[i]);
            Adjustment::BlackWhite(BlackWhite {
                weights: [w("Rd  ", 0), w("Yllw", 1), w("Grn ", 2), w("Cyn ", 3), w("Bl  ", 4), w("Mgnt", 5)],
                tint: d.boolean("useTint").unwrap_or(false),
                tint_color: d.obj("tintColor").and_then(descriptor_color).unwrap_or(def.tint_color),
            })
        }
        b"mixr" => {
            let _v = r.u16()?;
            let mono = r.u16()? != 0;
            let row = |r: &mut Reader| -> Result<[f32; 4]> {
                let (a, b, c) = (r.i16()?, r.i16()?, r.i16()?);
                r.skip(2, "mixer")?;
                Ok([a as f32, b as f32, c as f32, r.i16()? as f32])
            };
            if mono {
                let gray = row(&mut r)?;
                Adjustment::ChannelMixer(ChannelMixer { red: gray, green: gray, blue: gray, monochrome: true })
            } else {
                let red = row(&mut r)?;
                let green = row(&mut r)?;
                let blue = row(&mut r)?;
                Adjustment::ChannelMixer(ChannelMixer { red, green, blue, monochrome: false })
            }
        }
        b"selc" => {
            let _v = r.u16()?;
            let absolute = r.u16()? != 0;
            r.skip(8, "selective color")?;
            let mut colors = [[0f32; 4]; 9];
            for c in colors.iter_mut() {
                *c = [r.i16()? as f32, r.i16()? as f32, r.i16()? as f32, r.i16()? as f32];
            }
            Adjustment::SelectiveColor(SelectiveColor { colors, absolute })
        }
        b"vibA" => {
            let d = Descriptor::read_versioned(&mut r)?;
            Adjustment::Vibrance(Vibrance { vibrance: d.num("vibrance").unwrap_or(0.0) as f32, saturation: d.num("Strt").unwrap_or(0.0) as f32 })
        }
        b"expA" => {
            let _v = r.u16()?;
            Adjustment::Exposure(Exposure { exposure: r.f32()?, offset: r.f32()?, gamma: r.f32()? })
        }
        b"phfl" => {
            let v = r.u16()?;
            let color = match v {
                2 => read_color_struct(&mut r)?,
                3 => {
                    // Lab ×100, as Photoshop CC writes it.
                    let (l, a, b) = (r.i32()?, r.i32()?, r.i32()?);
                    let [cr, cg, cb] = lab_to_srgb(l as f32 / 100.0, a as f32 / 100.0, b as f32 / 100.0);
                    Rgba8::rgb(cr, cg, cb)
                }
                _ => return Err(corrupt(format!("photo filter version {v}"))),
            };
            let density = r.u32()? as f32;
            let preserve_luminosity = r.u8().map(|v| v != 0).unwrap_or(true);
            Adjustment::PhotoFilter(PhotoFilter { color, density, preserve_luminosity })
        }
        b"grdm" => {
            let v = r.u16()?;
            let reverse = r.u8()? != 0;
            let _dither = r.u8()?;
            if v == 3 {
                r.sig()?;
            }
            let _name = r.unicode()?;
            let n = r.u16()? as usize;
            let mut color_stops = Vec::new();
            for _ in 0..n.min(1024) {
                let loc = r.u32()?;
                let mid = r.u32()?;
                let c = read_color_struct(&mut r)?;
                r.skip(2, "gradient stop")?;
                color_stops.push((loc as f32 / 4096.0, mid as f32 / 100.0, c));
            }
            let n = r.u16()? as usize;
            let mut alpha_stops = Vec::new();
            for _ in 0..n.min(1024) {
                let loc = r.u32()?;
                let mid = r.u32()?;
                let o = r.u16()?;
                alpha_stops.push((loc as f32 / 4096.0, mid as f32 / 100.0, (o as f32 / 255.0).min(1.0)));
            }
            let stops = merge_stops(&color_stops, &alpha_stops);
            Adjustment::GradientMap(GradientMap { stops, reverse })
        }
        b"post" => Adjustment::Posterize { levels: r.u16()?.clamp(2, 255) as u32 },
        b"thrs" => Adjustment::Threshold { level: r.u16()?.clamp(1, 255) as u8 },
        b"nvrt" => Adjustment::Invert,
        b"clrL" => {
            let _v = r.u16()?;
            let d = Descriptor::read_versioned(&mut r)?;
            let fmt = d.enum_value("LUTFormat").unwrap_or_default();
            match d.raw("LUT3DFileData") {
                Some(bytes) if fmt == "LUTFormatCUBE" => {
                    let name = d.text("LUT3DFileName").or(d.text("Nm  ")).unwrap_or("LUT").to_string();
                    let text = String::from_utf8_lossy(bytes);
                    Adjustment::ColorLookup(ColorLookup::parse_cube(&name, &text).map_err(corrupt)?)
                }
                _ => return Err(corrupt(format!("colour lookup format {:?}", if fmt.is_empty() { "profile" } else { &fmt }))),
            }
        }
        other => return Err(corrupt(format!("adjustment {:?}", String::from_utf8_lossy(other)))),
    })
}

fn read_curves(r: &mut Reader) -> Result<Curves> {
    let is_map = r.u8()? != 0;
    let version = r.u16()?;
    let count_map = r.u32()?;
    let ids: Vec<u16> = if version == 1 { (0..32).filter(|b| count_map & (1 << b) != 0).collect() } else { (0..count_map.min(32) as u16).collect() };
    let mut curves: Vec<(u16, Vec<[f32; 2]>)> = Vec::new();
    let read_points = |r: &mut Reader| -> Result<Vec<[f32; 2]>> {
        let n = r.u16()? as usize;
        if n > 64 {
            return Err(corrupt("curve with too many points"));
        }
        let mut pts = Vec::with_capacity(n);
        for _ in 0..n {
            let out = r.i16()? as f32;
            let inp = r.i16()? as f32;
            pts.push([inp, out]);
        }
        Ok(pts)
    };
    let read_map = |r: &mut Reader| -> Result<Vec<[f32; 2]>> {
        let lut = r.bytes(256, "curve map")?;
        Ok((0..=16).map(|i| {
            let x = (i * 255 / 16) as usize;
            [x as f32, lut[x] as f32]
        })
        .collect())
    };
    for id in &ids {
        let pts = if is_map { read_map(r)? } else { read_points(r)? };
        curves.push((*id, pts));
    }
    // The newer `Crv ` section carries explicit channel ids; prefer it.
    if r.remaining() >= 10 && r.peek(4) == Some(b"Crv ") {
        let mut t = r.clone();
        t.skip(4, "")?;
        let _v = t.u16()?;
        let n = t.u32()? as usize;
        let mut extra = Vec::new();
        let mut ok = true;
        for _ in 0..n.min(32) {
            let Ok(id) = t.u16() else {
                ok = false;
                break;
            };
            match if is_map { read_map(&mut t) } else { read_points(&mut t) } {
                Ok(p) => extra.push((id, p)),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !extra.is_empty() {
            curves = extra;
        }
    }
    let mut c = Curves::default();
    for (id, pts) in curves {
        if pts.len() < 2 {
            continue;
        }
        let slot = match id {
            0 => &mut c.master,
            1 => &mut c.red,
            2 => &mut c.green,
            3 => &mut c.blue,
            _ => continue,
        };
        *slot = CurvePoints(pts);
    }
    Ok(c)
}

/// Combine separate colour and opacity stop lists into stops carrying both.
fn merge_stops(colors: &[(f32, f32, Rgba8)], alphas: &[(f32, f32, f32)]) -> Vec<GradientStop> {
    let mut positions: Vec<f32> = colors.iter().map(|c| c.0).chain(alphas.iter().map(|a| a.0)).map(|p| p.clamp(0.0, 1.0)).collect();
    // Midpoints away from 50 % bend the ramp; approximate with an extra stop.
    for pair in colors.windows(2) {
        if (pair[1].1 - 0.5).abs() > 0.01 {
            positions.push(pair[0].0 + (pair[1].0 - pair[0].0) * pair[1].1);
        }
    }
    positions.sort_by(f32::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    if positions.is_empty() {
        return GradientMap::default().stops;
    }
    let color_at = |t: f32| -> [f32; 3] {
        if colors.is_empty() {
            return [0.0; 3];
        }
        let f = |c: Rgba8| [c.r as f32, c.g as f32, c.b as f32];
        if t <= colors[0].0 {
            return f(colors[0].2);
        }
        for w in colors.windows(2) {
            if t <= w[1].0 {
                let span = (w[1].0 - w[0].0).max(1e-6);
                let mut k = (t - w[0].0) / span;
                k = midpoint_bend(k, w[1].1);
                let (a, b) = (f(w[0].2), f(w[1].2));
                return std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
            }
        }
        f(colors[colors.len() - 1].2)
    };
    let alpha_at = |t: f32| -> f32 {
        if alphas.is_empty() {
            return 1.0;
        }
        if t <= alphas[0].0 {
            return alphas[0].2;
        }
        for w in alphas.windows(2) {
            if t <= w[1].0 {
                let span = (w[1].0 - w[0].0).max(1e-6);
                let k = midpoint_bend((t - w[0].0) / span, w[1].1);
                return w[0].2 + (w[1].2 - w[0].2) * k;
            }
        }
        alphas[alphas.len() - 1].2
    };
    positions
        .into_iter()
        .map(|p| {
            let c = color_at(p);
            let a = alpha_at(p);
            GradientStop { pos: p, color: Rgba8 { r: clamp_u8(c[0] as f64), g: clamp_u8(c[1] as f64), b: clamp_u8(c[2] as f64), a: clamp_u8(a as f64 * 255.0) } }
        })
        .collect()
}

/// Photoshop's midpoint: at `k = mid` the ramp is halfway.
fn midpoint_bend(k: f32, mid: f32) -> f32 {
    let mid = mid.clamp(0.05, 0.95);
    if (mid - 0.5).abs() < 1e-3 {
        return k;
    }
    k.clamp(0.0, 1.0).powf((0.5f32).ln() / mid.ln())
}

// ---------------------------------------------------------------------------
// Adjustments: writing

/// Blocks for an adjustment. `None` for kinds Photoshop has no layer for.
pub fn write_adjustment(adj: &Adjustment) -> Option<Blocks> {
    let mut w = Writer::new();
    let key: &[u8; 4] = match adj {
        Adjustment::Levels(l) => {
            w.u16(2);
            let ch = |w: &mut Writer, c: &LevelsChannel| {
                w.i16(c.in_black.round() as i16);
                w.i16(c.in_white.round() as i16);
                w.i16(c.out_black.round() as i16);
                w.i16(c.out_white.round() as i16);
                w.i16((c.gamma * 100.0).round() as i16);
            };
            for c in [&l.master, &l.red, &l.green, &l.blue] {
                ch(&mut w, c);
            }
            for _ in 4..29 {
                ch(&mut w, &LevelsChannel::default());
            }
            b"levl"
        }
        Adjustment::Curves(c) => {
            let chans = [&c.master, &c.red, &c.green, &c.blue];
            w.u8(0);
            w.u16(1);
            w.u32(0b1111);
            let pts = |w: &mut Writer, p: &CurvePoints| {
                let mut v = p.0.clone();
                v.sort_by(|a, b| a[0].total_cmp(&b[0]));
                w.u16(v.len() as u16);
                for q in v {
                    w.i16(q[1].round() as i16);
                    w.i16(q[0].round() as i16);
                }
            };
            for p in chans {
                pts(&mut w, p);
            }
            w.bytes(b"Crv ");
            w.u16(4);
            w.u32(4);
            for (i, p) in chans.iter().enumerate() {
                w.u16(i as u16);
                pts(&mut w, p);
            }
            b"curv"
        }
        Adjustment::HueSaturation(h) => {
            w.u16(2);
            w.u8(h.colorize as u8);
            w.u8(0);
            // Photoshop stores the colorize hue as -180..180.
            let ch = h.colorize_hue.round().rem_euclid(360.0);
            w.i16(if ch > 180.0 { ch - 360.0 } else { ch } as i16);
            w.i16(h.colorize_saturation.round() as i16);
            w.i16(h.colorize_lightness.round() as i16);
            w.i16(h.master.hue.round() as i16);
            w.i16(h.master.saturation.round() as i16);
            w.i16(h.master.lightness.round() as i16);
            for (band, s) in h.bands.iter().zip(h.ranges.iter()) {
                for v in band {
                    w.i16(v.round() as i16);
                }
                w.i16(s.hue.round() as i16);
                w.i16(s.saturation.round() as i16);
                w.i16(s.lightness.round() as i16);
            }
            b"hue2"
        }
        Adjustment::BrightnessContrast(b) => {
            w.i16(b.brightness.round() as i16);
            w.i16(b.contrast.round() as i16);
            w.i16(127);
            w.u8(0);
            w.u8(0);
            let mut d = Writer::new();
            Descriptor::new("null")
                .with("Vrsn", Value::Integer(1))
                .with("Brgh", Value::Integer(b.brightness.round() as i32))
                .with("Cntr", Value::Integer(b.contrast.round() as i32))
                .with("means", Value::Integer(127))
                .with("Lab ", Value::Bool(false))
                .with("useLegacy", Value::Bool(b.legacy))
                .with("Auto", Value::Bool(false))
                .write_versioned(&mut d);
            return Some(vec![(*b"brit", w.buf), (*b"CgEd", d.buf)]);
        }
        Adjustment::ColorBalance(c) => {
            for tri in [c.shadows, c.midtones, c.highlights] {
                for v in tri {
                    w.i16(v.round() as i16);
                }
            }
            w.u8(c.preserve_luminosity as u8);
            w.u8(0);
            b"blnc"
        }
        Adjustment::BlackWhite(b) => {
            let keys = ["Rd  ", "Yllw", "Grn ", "Cyn ", "Bl  ", "Mgnt"];
            let mut d = Descriptor::new("null");
            for (k, v) in keys.iter().zip(b.weights) {
                d.set(k, Value::Integer(v.round() as i32));
            }
            d.set("useTint", Value::Bool(b.tint));
            d.set("tintColor", color_descriptor(b.tint_color));
            d.set("bwPresetKind", Value::Integer(1));
            d.set("blackAndWhitePresetFileName", Value::text(""));
            d.write_versioned(&mut w);
            b"blwh"
        }
        Adjustment::ChannelMixer(m) => {
            w.u16(1);
            w.u16(m.monochrome as u16);
            let row = |w: &mut Writer, r: &[f32; 4]| {
                w.i16(r[0].round() as i16);
                w.i16(r[1].round() as i16);
                w.i16(r[2].round() as i16);
                w.i16(0);
                w.i16(r[3].round() as i16);
            };
            if m.monochrome {
                row(&mut w, &m.red);
                w.zeros(30);
            } else {
                row(&mut w, &m.red);
                row(&mut w, &m.green);
                row(&mut w, &m.blue);
                row(&mut w, &[0.0; 4]);
            }
            b"mixr"
        }
        Adjustment::SelectiveColor(s) => {
            w.u16(1);
            w.u16(s.absolute as u16);
            w.zeros(8);
            for c in s.colors {
                for v in c {
                    w.i16(v.round() as i16);
                }
            }
            b"selc"
        }
        Adjustment::Vibrance(v) => {
            Descriptor::new("null")
                .with("vibrance", Value::Integer(v.vibrance.round() as i32))
                .with("Strt", Value::Integer(v.saturation.round() as i32))
                .write_versioned(&mut w);
            b"vibA"
        }
        Adjustment::Exposure(e) => {
            w.u16(1);
            w.f32(e.exposure);
            w.f32(e.offset);
            w.f32(e.gamma);
            w.zeros(2);
            b"expA"
        }
        Adjustment::PhotoFilter(p) => {
            w.u16(2);
            write_color_struct(&mut w, p.color);
            w.u32(p.density.round().clamp(0.0, 100.0) as u32);
            w.u8(p.preserve_luminosity as u8);
            w.zeros(3);
            b"phfl"
        }
        Adjustment::GradientMap(g) => {
            w.u16(1);
            w.u8(g.reverse as u8);
            w.u8(0);
            w.unicode("Custom", true);
            let mut stops = g.stops.clone();
            stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            w.u16(stops.len() as u16);
            for s in &stops {
                w.u32((s.pos.clamp(0.0, 1.0) * 4096.0).round() as u32);
                w.u32(50);
                write_color_struct(&mut w, s.color);
                w.zeros(2);
            }
            w.u16(stops.len() as u16);
            for s in &stops {
                w.u32((s.pos.clamp(0.0, 1.0) * 4096.0).round() as u32);
                w.u32(50);
                w.u16(s.color.a as u16);
            }
            w.u16(2);
            w.u16(4096);
            w.u16(32);
            w.u16(0);
            w.u32(0);
            w.u16(0);
            w.u16(0);
            w.u32(4096);
            w.u16(3);
            w.zeros(8);
            for _ in 0..4 {
                w.u16(0x8000);
            }
            w.zeros(4);
            b"grdm"
        }
        Adjustment::Posterize { levels } => {
            w.u16((*levels).clamp(2, 255) as u16);
            w.zeros(2);
            b"post"
        }
        Adjustment::Threshold { level } => {
            w.u16(*level as u16);
            w.zeros(2);
            b"thrs"
        }
        Adjustment::Invert => b"nvrt",
        Adjustment::ColorLookup(l) => return Some(vec![(*b"clrL", color_lookup_block(&l.name, &cube_text(l)))]),
        Adjustment::Develop(_) | Adjustment::Grain(_) => return None,
    };
    Some(vec![(*key, w.buf)])
}

// ---------------------------------------------------------------------------
// Grain (no Photoshop equivalent)

/// Private block holding a Grain adjustment's parameters (JSON). The layer
/// itself is written as Linear Light noise pixels for Photoshop.
pub const GRAIN_KEY: [u8; 4] = *b"opGr";

/// The parameters plus the layer's own blend mode (the record says Linear
/// Light for Photoshop's sake).
pub fn write_grain(g: &Grain, blend: [u8; 4]) -> Vec<u8> {
    let json = serde_json::json!({ "grain": g, "blend": String::from_utf8_lossy(&blend) });
    let mut v = serde_json::to_vec(&json).unwrap_or_default();
    while v.len() % 4 != 0 {
        v.push(b' ');
    }
    v
}

pub fn read_grain(data: &[u8]) -> Option<(Grain, [u8; 4])> {
    let v: serde_json::Value = serde_json::from_slice(data.trim_ascii_end()).ok()?;
    let g = serde_json::from_value(v.get("grain")?.clone()).ok()?;
    let b = v.get("blend").and_then(|b| b.as_str()).map(|b| b.as_bytes()).filter(|b| b.len() == 4).map(|b| [b[0], b[1], b[2], b[3]]).unwrap_or(*b"norm");
    Some((g, b))
}

/// Straight RGBA of the grain as a Linear Light layer: Linear Light gives
/// `base + 2·(blend − ½)`, so a blend value of `½ + δ/2` adds exactly δ, the
/// delta Grain applies at mid-grey (where its midtone weight is 1).
pub fn grain_layer_pixels(g: &Grain, w: usize, h: usize) -> Vec<u8> {
    let strength = (g.amount / 100.0).clamp(0.0, 1.0) * 0.35;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let d = g.noise(x as f64 + 0.5, y as f64 + 0.5, 1.0) * strength;
            let v = ((0.5 + d / 2.0) * 255.0).round().clamp(0.0, 255.0) as u8;
            out[(y * w + x) * 4..][..4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    out
}

pub fn cube_text(l: &ColorLookup) -> String {
    let mut s = format!("TITLE \"{}\"\nLUT_3D_SIZE {}\n", l.name.replace('"', "'"), l.size);
    for t in l.table.as_chunks::<3>().0.iter() {
        s.push_str(&format!("{:.6} {:.6} {:.6}\n", t[0], t[1], t[2]));
    }
    s
}

pub fn color_lookup_block(name: &str, cube: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u16(1);
    let file = if name.to_lowercase().ends_with(".cube") { name.to_string() } else { format!("{name}.cube") };
    Descriptor::new("null")
        .with("lookupType", Value::enumv("colorLookupType", "3DLUT"))
        .with("Nm  ", Value::text(&file))
        .with("Dthr", Value::Bool(true))
        .with("LUTFormat", Value::enumv("LUTFormatType", "LUTFormatCUBE"))
        .with("LUT3DFileData", Value::RawData(cube.as_bytes().to_vec()))
        .with("LUT3DFileName", Value::text(&file))
        .write_versioned(&mut w);
    w.buf
}

/// Sample the point-wise part of any adjustment into a 3D LUT, for kinds
/// Photoshop has no adjustment layer for. Spatial effects are dropped.
pub fn bake_lut(adj: &Adjustment, name: &str, size: usize) -> ColorLookup {
    let mut a = adj.clone();
    if let Adjustment::Develop(d) = &mut a {
        d.clarity = 0.0;
        d.dehaze = 0.0;
        d.vignette = 0.0;
        d.grain = 0.0;
    }
    let n = size * size * size;
    let mut buf = vec![0f32; n * 4];
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let i = ((b * size + g) * size + r) * 4;
                let f = |v: usize| v as f32 / (size - 1) as f32;
                buf[i..i + 4].copy_from_slice(&[f(r), f(g), f(b), 1.0]);
            }
        }
    }
    let ctx = ApplyCtx { x0: 0.0, y0: 0.0, step: 1.0, doc_w: n as u32, doc_h: 1, width: n, height: 1 };
    a.apply(&mut buf, &ctx);
    let table = buf.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    ColorLookup { name: name.to_string(), size, table, strength: 1.0 }
}

// ---------------------------------------------------------------------------
// Fill layers

/// Photoshop's gradient geometry → the engine's two points. The gradient
/// spans the box's extent along the angle, scaled, centred (plus offset).
pub fn gradient_points(angle_deg: f64, scale_pct: f64, offset: (f64, f64), kind: GradientKind, bounds: (f64, f64, f64, f64)) -> (Point, Point) {
    let (x, y, w, h) = bounds;
    let a = angle_deg.to_radians();
    let (dx, dy) = (a.cos(), -a.sin());
    let s = scale_pct / 100.0;
    let cx = x + w / 2.0 + offset.0 / 100.0 * w;
    let cy = y + h / 2.0 + offset.1 / 100.0 * h;
    let half = (w * dx.abs() + h * dy.abs()) / 2.0 * s;
    match kind {
        GradientKind::Linear => (Point::new(cx - dx * half, cy - dy * half), Point::new(cx + dx * half, cy + dy * half)),
        _ => (Point::new(cx, cy), Point::new(cx + dx * half, cy + dy * half)),
    }
}

pub fn read_fill(key: &[u8; 4], data: &[u8], bounds: (f64, f64, f64, f64)) -> Result<Fill> {
    let d = Descriptor::read_versioned(&mut Reader::new(data))?;
    match key {
        b"SoCo" => {
            let c = d.obj("Clr ").and_then(descriptor_color).ok_or_else(|| corrupt("solid colour fill without a colour"))?;
            Ok(Fill::Solid { color: c })
        }
        b"GdFl" => {
            let grad = d.obj("Grad").ok_or_else(|| corrupt("gradient fill without a gradient"))?;
            if grad.enum_value("GrdF").as_deref() == Some("ClNs") {
                return Err(corrupt("noise gradients"));
            }
            let stops = read_gradient_stops(grad);
            let kind = match d.enum_value("Type").as_deref() {
                Some("Rdl ") => GradientKind::Radial,
                Some("Angl") => GradientKind::Angle,
                Some("Rflc") => GradientKind::Reflected,
                Some("Dmnd") => GradientKind::Diamond,
                _ => GradientKind::Linear,
            };
            let angle = d.num("Angl").unwrap_or(90.0);
            let scale = d.num("Scl ").unwrap_or(100.0);
            let offset = d.obj("Ofst").map(|o| (o.num("Hrzn").unwrap_or(0.0), o.num("Vrtc").unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            let (from, to) = gradient_points(angle, scale, offset, kind, bounds);
            Ok(Fill::Gradient { stops, gradient: kind, from, to, reverse: d.boolean("Rvrs").unwrap_or(false) })
        }
        _ => Err(corrupt("pattern fills")),
    }
}

pub fn read_gradient_stops(grad: &Descriptor) -> Vec<GradientStop> {
    let interp = grad.num("Intr").unwrap_or(4096.0).max(1.0);
    let colors: Vec<(f32, f32, Rgba8)> = grad
        .list("Clrs")
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_descriptor)
        .map(|s| {
            let c = s.obj("Clr ").and_then(descriptor_color).unwrap_or(match s.enum_value("Type").as_deref() {
                Some("BckC") => Rgba8::WHITE,
                _ => Rgba8::BLACK,
            });
            (s.num("Lctn").unwrap_or(0.0) as f32 / 4096.0, s.num("Mdpn").unwrap_or(50.0) as f32 / 100.0, c)
        })
        .collect();
    let _ = interp;
    let alphas: Vec<(f32, f32, f32)> = grad
        .list("Trns")
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_descriptor)
        .map(|s| (s.num("Lctn").unwrap_or(0.0) as f32 / 4096.0, s.num("Mdpn").unwrap_or(50.0) as f32 / 100.0, (s.num("Opct").unwrap_or(100.0) / 100.0) as f32))
        .collect();
    merge_stops(&colors, &alphas)
}

pub fn gradient_descriptor(stops: &[GradientStop]) -> Descriptor {
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let loc = |p: f32| Value::Integer((p.clamp(0.0, 1.0) * 4096.0).round() as i32);
    Descriptor::new("Grdn")
        .with("Nm  ", Value::text("Custom"))
        .with("GrdF", Value::enumv("GrdF", "CstS"))
        .with("Intr", Value::Double(4096.0))
        .with(
            "Clrs",
            Value::List(
                sorted
                    .iter()
                    .map(|s| {
                        Value::Descriptor(
                            Descriptor::new("Clrt")
                                .with("Clr ", color_descriptor(s.color))
                                .with("Type", Value::enumv("Clry", "UsrS"))
                                .with("Lctn", loc(s.pos))
                                .with("Mdpn", Value::Integer(50)),
                        )
                    })
                    .collect(),
            ),
        )
        .with(
            "Trns",
            Value::List(
                sorted
                    .iter()
                    .map(|s| {
                        Value::Descriptor(
                            Descriptor::new("TrnS")
                                .with("Opct", Value::unit("#Prc", s.color.a as f64 / 255.0 * 100.0))
                                .with("Lctn", loc(s.pos))
                                .with("Mdpn", Value::Integer(50)),
                        )
                    })
                    .collect(),
            ),
        )
}

pub fn gradient_kind_enum(kind: GradientKind) -> &'static str {
    match kind {
        GradientKind::Linear => "Lnr ",
        GradientKind::Radial => "Rdl ",
        GradientKind::Angle => "Angl",
        GradientKind::Reflected => "Rflc",
        GradientKind::Diamond => "Dmnd",
    }
}

/// Inverse of [`gradient_points`]: angle, scale and offset for two points.
pub fn gradient_params(from: Point, to: Point, kind: GradientKind, bounds: (f64, f64, f64, f64)) -> (f64, f64, (f64, f64)) {
    let (x, y, w, h) = bounds;
    let (vx, vy) = (to.x - from.x, to.y - from.y);
    let len = (vx * vx + vy * vy).sqrt().max(1e-9);
    let (dx, dy) = (vx / len, vy / len);
    let angle = (-dy).atan2(dx).to_degrees();
    let extent = ((w * dx.abs() + h * dy.abs()) / 2.0).max(1e-9);
    let (half, cx, cy) = match kind {
        GradientKind::Linear => (len / 2.0, (from.x + to.x) / 2.0, (from.y + to.y) / 2.0),
        _ => (len, from.x, from.y),
    };
    let scale = half / extent * 100.0;
    let off = ((cx - (x + w / 2.0)) / w.max(1e-9) * 100.0, (cy - (y + h / 2.0)) / h.max(1e-9) * 100.0);
    (angle, scale, off)
}

pub fn write_fill(fill: &Fill, bounds: (f64, f64, f64, f64)) -> Blocks {
    let mut w = Writer::new();
    match fill {
        Fill::Solid { color } => {
            Descriptor::new("null").with("Clr ", color_descriptor(*color)).write_versioned(&mut w);
            vec![(*b"SoCo", w.buf)]
        }
        Fill::Gradient { stops, gradient, from, to, reverse } => {
            let (angle, scale, off) = gradient_params(*from, *to, *gradient, bounds);
            Descriptor::new("null")
                .with("Grad", Value::Descriptor(gradient_descriptor(stops)))
                .with("Angl", Value::unit("#Ang", angle))
                .with("Type", Value::enumv("GrdT", gradient_kind_enum(*gradient)))
                .with("Rvrs", Value::Bool(*reverse))
                .with("Dthr", Value::Bool(false))
                .with("Algn", Value::Bool(true))
                .with("Scl ", Value::unit("#Prc", scale))
                .with(
                    "Ofst",
                    Value::Descriptor(Descriptor::new("Pnt ").with("Hrzn", Value::unit("#Prc", off.0)).with("Vrtc", Value::unit("#Prc", off.1))),
                )
                .write_versioned(&mut w);
            vec![(*b"GdFl", w.buf)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(a: Adjustment) {
        let blocks = write_adjustment(&a).expect("representable");
        let get = |k: &[u8; 4]| blocks.iter().find(|(bk, _)| bk == k).map(|(_, d)| d.as_slice());
        let back = read_adjustment(&get).unwrap().unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn adjustments_round_trip() {
        round_trip(Adjustment::Levels(Levels {
            master: LevelsChannel { in_black: 10.0, in_white: 240.0, gamma: 1.25, out_black: 5.0, out_white: 250.0 },
            ..Default::default()
        }));
        round_trip(Adjustment::Curves(Curves { master: CurvePoints(vec![[0.0, 0.0], [128.0, 150.0], [255.0, 255.0]]), ..Default::default() }));
        let mut hs = HueSaturation { master: HslShift { hue: 20.0, saturation: -30.0, lightness: 5.0 }, ..Default::default() };
        hs.ranges[2].hue = 15.0;
        round_trip(Adjustment::HueSaturation(hs));
        round_trip(Adjustment::BrightnessContrast(BrightnessContrast { brightness: 30.0, contrast: -20.0, legacy: false }));
        round_trip(Adjustment::ColorBalance(ColorBalance { shadows: [10.0, -5.0, 0.0], ..Default::default() }));
        round_trip(Adjustment::BlackWhite(BlackWhite { weights: [10.0, 20.0, 30.0, 40.0, 50.0, 60.0], tint: true, tint_color: Rgba8::rgb(200, 100, 50) }));
        round_trip(Adjustment::ChannelMixer(ChannelMixer { red: [80.0, 20.0, 0.0, 5.0], ..Default::default() }));
        round_trip(Adjustment::ChannelMixer(ChannelMixer { red: [40.0, 40.0, 20.0, 0.0], green: [40.0, 40.0, 20.0, 0.0], blue: [40.0, 40.0, 20.0, 0.0], monochrome: true }));
        let mut sc = SelectiveColor::default();
        sc.colors[0] = [10.0, -10.0, 5.0, 0.0];
        round_trip(Adjustment::SelectiveColor(sc));
        round_trip(Adjustment::Vibrance(Vibrance { vibrance: 40.0, saturation: -10.0 }));
        round_trip(Adjustment::Exposure(Exposure { exposure: 1.5, offset: -0.1, gamma: 0.8 }));
        round_trip(Adjustment::PhotoFilter(PhotoFilter { color: Rgba8::rgb(236, 138, 0), density: 25.0, preserve_luminosity: true }));
        round_trip(Adjustment::GradientMap(GradientMap::default()));
        round_trip(Adjustment::Posterize { levels: 6 });
        round_trip(Adjustment::Threshold { level: 100 });
        round_trip(Adjustment::Invert);
    }

    #[test]
    fn fills_round_trip() {
        let b = (0.0, 0.0, 200.0, 100.0);
        let solid = Fill::Solid { color: Rgba8::rgb(1, 2, 3) };
        let blocks = write_fill(&solid, b);
        assert_eq!(read_fill(&blocks[0].0, &blocks[0].1, b).unwrap(), solid);
        let grad = Fill::Gradient {
            stops: GradientMap::default().stops,
            gradient: GradientKind::Linear,
            from: Point::new(20.0, 50.0),
            to: Point::new(180.0, 50.0),
            reverse: false,
        };
        let blocks = write_fill(&grad, b);
        let back = read_fill(&blocks[0].0, &blocks[0].1, b).unwrap();
        let Fill::Gradient { from, to, .. } = back else { panic!() };
        assert!((from.x - 20.0).abs() < 1e-6 && (to.x - 180.0).abs() < 1e-6 && (to.y - 50.0).abs() < 1e-6, "{from:?} {to:?}");
    }

    #[test]
    fn baked_lut_matches_invert() {
        let l = bake_lut(&Adjustment::Invert, "inv", 5);
        assert_eq!(&l.table[..3], &[1.0, 1.0, 1.0]);
    }
}
