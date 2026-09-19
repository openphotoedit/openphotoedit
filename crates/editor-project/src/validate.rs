//! Limits a project must meet before any of it is built.
//!
//! The loader already refuses damaged archives and budgets decoded pixel
//! bytes. These checks catch files that are well-formed but hostile or
//! corrupt in value: a layer placed two billion pixels away, a smart object
//! whose corners imply a 10⁷ × 10⁷ warp, fifty thousand layers, parameters
//! that are not numbers any slider can produce. Accepting those would let the
//! *next* edit allocate or loop over an absurd area, long after the file
//! "opened". Everything here runs on the raw manifest, before a single plane
//! is inflated and before the open document is replaced.
//!
//! The limits are generous next to anything the editor or Photoshop writes:
//!
//! | limit | value | why |
//! |---|---|---|
//! | canvas / plane side | 300,000 px | PSB's maximum |
//! | layer, mask, text and shape offsets | ±1,000,000 px | Compositor's `LayerTransform.isValid`; more than 3 canvases beyond the largest canvas |
//! | smart-object corners | finite, ±1,000,000; box ≤ 300,000 per side and ≤ 100 MP | the box is what a rebuild allocates (`warp_to_quad`) |
//! | layers in the whole tree (smart documents included) | 10,000 | Compositor's cap; Photoshop's own is 8,000 per group level |
//! | group nesting | 32 | Photoshop allows 10; before this, serde's recursion limit (128 JSON levels, about 40 groups) was the only bound |
//! | layer name | 16 KiB | |
//! | numeric parameters | within each field's range, otherwise ±1,000,000 | JSON cannot hold NaN, but 1e39 turns into ∞ as `f32` |
//! | arrays (curve points, gradient stops, shape points …) | per field; 4,096 by default | |
//! | manifest | 32 MB | about 1000× a large real manifest |

use serde_json::Value;

use crate::Error;

pub const MAX_SIDE: f64 = 300_000.0;
pub const MAX_OFFSET: f64 = 1_000_000.0;
pub const MAX_SMART_AREA: f64 = 100_000_000.0;
pub const MAX_LAYERS: usize = 10_000;
pub const MAX_GROUP_DEPTH: usize = 32;
pub const MAX_NAME_BYTES: usize = 16 * 1024;
pub const MAX_SMART_DEPTH: usize = 16;
pub const MAX_MANIFEST_BYTES: u64 = 32_000_000;
const MAX_PARAM: f64 = 1_000_000.0;
const MAX_ARRAY: usize = 4096;
const MAX_LUT_SIZE: u64 = 65;
const MAX_TEXT_BYTES: usize = 1_000_000;

fn bad(msg: String) -> Error {
    Error::Invalid(msg)
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// Every number inside `v` is finite and within ±`limit`; arrays are at most
/// `max_array` long. `what` names the owner in the error.
fn numbers(v: &Value, limit: f64, max_array: usize, what: &str) -> Result<(), Error> {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(x) if x.is_finite() && x.abs() <= limit => Ok(()),
            _ => Err(bad(format!("{what} has an out-of-range value {n}"))),
        },
        Value::Array(a) => {
            if a.len() > max_array {
                return Err(bad(format!("{what} has {} entries (at most {max_array})", a.len())));
            }
            a.iter().try_for_each(|x| numbers(x, limit, max_array, what))
        }
        Value::Object(o) => o.values().try_for_each(|x| numbers(x, limit, max_array, what)),
        _ => Ok(()),
    }
}

/// `v[key]`, when present, is a number in `lo..=hi`.
fn range(v: &Value, key: &str, lo: f64, hi: f64, what: &str) -> Result<(), Error> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(x) => match num(x) {
            Some(n) if (lo..=hi).contains(&n) => Ok(()),
            _ => Err(bad(format!("{what}: `{key}` is {x}, outside {lo}..{hi}"))),
        },
    }
}

fn offset(v: Option<&Value>, what: &str) -> Result<(), Error> {
    let Some(v) = v else { return Ok(()) };
    for k in ["x", "y"] {
        range(v, k, -MAX_OFFSET, MAX_OFFSET, what).map_err(|_| bad(format!("{what} is placed at {k} = {}, beyond ±{MAX_OFFSET} px", v[k])))?;
    }
    Ok(())
}

pub(crate) struct Validator {
    layers: usize,
}

pub(crate) fn manifest(document: &Value) -> Result<(), Error> {
    Validator { layers: 0 }.doc(document, 0)
}

impl Validator {
    fn doc(&mut self, d: &Value, smart_depth: usize) -> Result<(), Error> {
        if smart_depth > MAX_SMART_DEPTH {
            return Err(Error::Corrupt("smart objects are nested too deeply"));
        }
        for k in ["width", "height"] {
            range(d, k, 1.0, MAX_SIDE, "the document").map_err(|_| bad(format!("document {k} {} is outside 1..{MAX_SIDE}", d[k])))?;
        }
        if let Some(g) = d.get("guides") {
            numbers(g, MAX_OFFSET, 10_000, "the guides")?;
        }
        if let Some(r) = d.get("resolution") {
            numbers(r, MAX_PARAM, 0, "the resolution")?;
        }
        for l in d.get("layers").and_then(Value::as_array).into_iter().flatten() {
            self.layer(l, 1, smart_depth)?;
        }
        Ok(())
    }

    fn layer(&mut self, l: &Value, depth: usize, smart_depth: usize) -> Result<(), Error> {
        self.layers += 1;
        if self.layers > MAX_LAYERS {
            return Err(bad(format!("the project has more than {MAX_LAYERS} layers")));
        }
        if depth > MAX_GROUP_DEPTH {
            return Err(bad(format!("groups are nested more than {MAX_GROUP_DEPTH} deep")));
        }
        let name = l.get("name").and_then(Value::as_str).unwrap_or("");
        if name.len() > MAX_NAME_BYTES {
            return Err(bad(format!("a layer name is {} bytes long (at most {MAX_NAME_BYTES})", name.len())));
        }
        let what = format!("layer \u{201c}{}\u{201d}", name.chars().take(60).collect::<String>());
        for k in ["opacity", "fill_opacity"] {
            range(l, k, 0.0, 1.0, &what)?;
        }
        if l.get("provenance").and_then(Value::as_array).is_some_and(|p| p.len() > 1000) {
            return Err(bad(format!("{what} has too many provenance entries")));
        }
        if let Some(m) = l.get("mask").filter(|m| !m.is_null()) {
            offset(Some(m), &format!("the mask of {what}"))?;
            range(m, "density", 0.0, 1.0, &what)?;
        }
        if let Some(fx) = l.get("effects").filter(|e| !e.is_null()) {
            self.effects(fx, &what)?;
        }
        let kind = l.get("kind").unwrap_or(&Value::Null);
        offset(kind.get("raster"), &what)?;
        match kind.get("type").and_then(Value::as_str).unwrap_or("") {
            "group" => {
                for c in kind.get("children").and_then(Value::as_array).into_iter().flatten() {
                    self.layer(c, depth + 1, smart_depth)?;
                }
            }
            "adjustment" => adjustment(kind.get("adjustment").unwrap_or(&Value::Null), &what)?,
            "fill" => numbers(kind.get("fill").unwrap_or(&Value::Null), MAX_OFFSET, 256, &what)?,
            "text" => text(kind.get("data").unwrap_or(&Value::Null), &what)?,
            "shape" => shape(kind.get("data").unwrap_or(&Value::Null), &what)?,
            "smart" => {
                quad(kind.get("quad").unwrap_or(&Value::Null), &what)?;
                let filters = kind.get("filters").and_then(Value::as_array).map_or(&[][..], |f| f.as_slice());
                if filters.len() > 64 {
                    return Err(bad(format!("{what} has {} smart filters (at most 64)", filters.len())));
                }
                for f in filters {
                    numbers(f, MAX_PARAM, MAX_ARRAY, &format!("a smart filter on {what}"))?;
                    range(f, "opacity", 0.0, 1.0, &what)?;
                }
                if let Some(d) = kind.get("source").and_then(|s| s.get("document")) {
                    self.doc(d, smart_depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn effects(&self, fx: &Value, what: &str) -> Result<(), Error> {
        let what = format!("the layer style of {what}");
        numbers(fx, MAX_PARAM, MAX_ARRAY, &what)?;
        range(fx, "scale", 0.0, 1000.0, &what)?;
        // Each effect's own sizes drive blur radii and distance fields.
        if let Some(o) = fx.as_object() {
            for e in o.values().filter(|e| e.is_object()) {
                range(e, "size", 0.0, 1000.0, &what)?;
                range(e, "soften", 0.0, 1000.0, &what)?;
                range(e, "distance", -30_000.0, 30_000.0, &what)?;
                range(e, "opacity", 0.0, 1.0, &what)?;
            }
        }
        Ok(())
    }
}

/// Smart-object corners: finite and bounded, and a bounding box a rebuild
/// can afford (the warp allocates the whole box).
fn quad(q: &Value, what: &str) -> Result<(), Error> {
    let pts = q.as_array().filter(|a| a.len() == 4).ok_or_else(|| bad(format!("{what} has no valid corner quad")))?;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        let (x, y) = match (p.get("x").and_then(num), p.get("y").and_then(num)) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() && x.abs() <= MAX_OFFSET && y.abs() <= MAX_OFFSET => (x, y),
            _ => return Err(bad(format!("{what} has a corner at {p}, beyond ±{MAX_OFFSET} px"))),
        };
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    if w > MAX_SIDE || h > MAX_SIDE || w * h > MAX_SMART_AREA {
        return Err(bad(format!("{what} is transformed to {w:.0}×{h:.0} px, larger than a smart object can be rebuilt ({} MP)", MAX_SMART_AREA / 1e6)));
    }
    Ok(())
}

fn text(d: &Value, what: &str) -> Result<(), Error> {
    if d.get("text").and_then(Value::as_str).is_some_and(|t| t.len() > MAX_TEXT_BYTES) {
        return Err(bad(format!("{what} holds more than {MAX_TEXT_BYTES} bytes of text")));
    }
    numbers(d, MAX_OFFSET, MAX_ARRAY, what)?;
    range(d, "font_size", 0.0, 10_000.0, what)?;
    range(d, "stroke_width", 0.0, 10_000.0, what)?;
    range(d, "box_width", 0.0, MAX_SIDE, what)?;
    Ok(())
}

fn shape(d: &Value, what: &str) -> Result<(), Error> {
    numbers(d, MAX_OFFSET, 100_000, what)?;
    range(d, "stroke_width", 0.0, 10_000.0, what)?;
    Ok(())
}

/// Adjustment parameters: every number finite and bounded, and the fields
/// whose range the engine relies on checked against it.
fn adjustment(a: &Value, what: &str) -> Result<(), Error> {
    let kind = a.get("kind").and_then(Value::as_str).unwrap_or("");
    let what = format!("the {kind} adjustment on {what}");
    if kind == "color-lookup" {
        let size = a.get("size").and_then(Value::as_u64).unwrap_or(0);
        if !(2..=MAX_LUT_SIZE).contains(&size) {
            return Err(bad(format!("{what} has a {size}³ table (2..{MAX_LUT_SIZE})")));
        }
        let n = (size * size * size * 3) as usize;
        let table = a.get("table").and_then(Value::as_array).ok_or_else(|| bad(format!("{what} has no table")))?;
        if table.len() != n {
            return Err(bad(format!("{what} has {} table entries, expected {n}", table.len())));
        }
        numbers(&a["table"], 1000.0, n, &what)?;
        range(a, "strength", 0.0, 1.0, &what)?;
        return Ok(());
    }
    if kind == "grain" {
        // The seed is any u32, beyond the generic numeric limit.
        if let Some(seed) = a.get("seed") {
            if seed.as_u64().is_none_or(|n| n > u32::MAX as u64) {
                return Err(bad(format!("{what}: `seed` is {seed}, not a 32-bit seed")));
            }
        }
        let mut rest = a.clone();
        if let Some(o) = rest.as_object_mut() {
            o.remove("seed");
        }
        numbers(&rest, MAX_PARAM, 16, &what)?;
        range(a, "amount", 0.0, 100.0, &what)?;
        range(a, "size", 0.0, 100.0, &what)?;
        range(a, "roughness", 0.0, 100.0, &what)?;
        return Ok(());
    }
    numbers(a, MAX_PARAM, 256, &what)?;
    match kind {
        "levels" => {
            for ch in ["master", "red", "green", "blue"] {
                let Some(c) = a.get(ch) else { continue };
                for k in ["in_black", "in_white", "out_black", "out_white"] {
                    range(c, k, 0.0, 255.0, &what)?;
                }
                range(c, "gamma", 0.01, 10.0, &what)?;
            }
        }
        "curves" => {
            for ch in ["master", "red", "green", "blue"] {
                numbers(a.get(ch).unwrap_or(&Value::Null), 255.0, 256, &what)?;
                if let Some(p) = a.get(ch).and_then(Value::as_array) {
                    if p.iter().flat_map(|q| q.as_array().into_iter().flatten()).any(|v| num(v).is_none_or(|n| n < 0.0)) {
                        return Err(bad(format!("{what}: curve points must be within 0..255")));
                    }
                }
            }
        }
        "hue-saturation" => {
            // Range bands (absent in projects saved before they existed):
            // six ranges of four handles, in degrees.
            if let Some(b) = a.get("bands") {
                let rows = b.as_array().filter(|r| r.len() == 6).ok_or_else(|| bad(format!("{what}: `bands` must hold 6 ranges")))?;
                for r in rows {
                    let h = r.as_array().filter(|h| h.len() == 4).ok_or_else(|| bad(format!("{what}: each band has 4 handles")))?;
                    if h.iter().any(|v| num(v).is_none_or(|n| !(-360.0..=720.0).contains(&n))) {
                        return Err(bad(format!("{what}: band handles must be degrees")));
                    }
                }
            }
        }
        "posterize" => range(a, "levels", 2.0, 255.0, &what)?,
        "threshold" => range(a, "level", 0.0, 255.0, &what)?,
        "gradient-map" => {
            for s in a.get("stops").and_then(Value::as_array).into_iter().flatten() {
                range(s, "pos", 0.0, 1.0, &what)?;
            }
        }
        _ => {}
    }
    Ok(())
}
