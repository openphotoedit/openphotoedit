//! PSD/PSB → `Document`.

use std::collections::HashSet;

use editor_core::adjust::Adjustment;
use editor_core::blend::BlendMode;
use editor_core::document::{Document, Guide};
use editor_core::geom::Rect;
use editor_core::layer::{Layer, LayerId, LayerKind, LayerMask, Locks, Raster, SmartSource};
use editor_core::plane::Plane;

use crate::color::{color_channels, mode_to_rgb, to_u8};
use crate::compress;
use crate::effects::{self, GlobalLight};
use crate::error::{PsdError, Result};
use crate::io::Reader;
use crate::kinds::{self, ADJUSTMENT_KEYS, FILL_KEYS};
use crate::linked::{self, LinkedFile};
use crate::sidecar::{fnv, LayerExtra, Sidecar};
use crate::structure::*;
use crate::text;

#[derive(Clone, Debug)]
pub struct ImportOptions {
    /// Decoded bytes the import may allocate for pixel data.
    pub max_bytes: u64,
    /// Nesting of embedded smart-object documents to decode.
    pub max_nesting: u32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            max_bytes: if cfg!(target_pointer_width = "32") { 1_600_000_000 } else { 12_000_000_000 },
            max_nesting: 2,
        }
    }
}

#[derive(Debug)]
pub struct Imported {
    pub doc: Document,
    pub warnings: Vec<String>,
}

struct Ctx<'a, 'f> {
    file: &'f PsdFile<'a>,
    budget: u64,
    warnings: Vec<String>,
    light: GlobalLight,
    linked: Vec<LinkedFile<'a>>,
    opts: &'f ImportOptions,
    nesting: u32,
    /// Blocks can be kept verbatim only when nothing was converted.
    native: bool,
    sidecar: Sidecar,
    /// Decoded smart-object contents by linked-file id: many layers can
    /// place the same file, and clones share their tiles.
    sources: std::collections::HashMap<String, Option<SmartSource>>,
}

impl Ctx<'_, '_> {
    fn warn(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if !self.warnings.contains(&m) {
            self.warnings.push(m);
        }
    }
    fn take(&mut self, bytes: u64) -> Result<()> {
        if bytes > self.budget {
            return Err(PsdError::TooLarge { needed: bytes, limit: self.budget });
        }
        self.budget -= bytes;
        Ok(())
    }
}

pub fn import(bytes: &[u8], opts: &ImportOptions) -> Result<Imported> {
    import_nested(bytes, opts, 0, opts.max_bytes).map(|(i, _)| i)
}

fn import_nested(bytes: &[u8], opts: &ImportOptions, nesting: u32, budget: u64) -> Result<(Imported, u64)> {
    let file = parse(bytes)?;
    let h = file.header;
    let mut linked = Vec::new();
    for key in [b"lnk2", b"lnk3", b"lnkD"] {
        for b in file.global_blocks.iter().filter(|b| &b.key == key) {
            linked.extend(linked::parse(b.data));
        }
    }
    let light = GlobalLight {
        angle: file.resource(1037).and_then(|d| Reader::new(d).i32().ok()).map(|v| v as f32).unwrap_or(120.0),
        altitude: file.resource(1049).and_then(|d| Reader::new(d).i32().ok()).map(|v| v as f32).unwrap_or(30.0),
    };
    let native = h.depth == 8 && h.mode == MODE_RGB;
    let mut cx = Ctx { file: &file, budget, warnings: Vec::new(), light, linked, opts, nesting, native, sidecar: Sidecar::default(), sources: Default::default() };

    if h.depth != 8 {
        cx.warn(format!("This {}-bit document was converted to 8 bits per channel.", h.depth));
    }
    let mode_name = match h.mode {
        MODE_RGB => None,
        MODE_GRAYSCALE => Some("Grayscale"),
        MODE_CMYK => Some("CMYK"),
        MODE_LAB => Some("Lab"),
        MODE_INDEXED => Some("Indexed Color"),
        MODE_BITMAP => Some("Bitmap"),
        MODE_DUOTONE => Some("Duotone"),
        MODE_MULTICHANNEL => Some("Multichannel"),
        _ => Some("an unknown colour mode"),
    };
    if let Some(m) = mode_name {
        cx.warn(format!("This {m} document was converted to RGB; colours may shift slightly."));
    }

    // Showing and saving the document needs a full-size composite.
    cx.take(h.width as u64 * h.height as u64 * 4)?;
    let mut doc = Document::new(h.width, h.height);
    read_resources(&mut cx, &mut doc);

    // Layer ids: keep the file's where they are unique.
    let mut used = HashSet::new();
    let mut max_id = 0u32;
    for rec in &file.layers {
        if let Some(id) = rec.block(b"lyid").and_then(|d| Reader::new(d).u32().ok()) {
            max_id = max_id.max(id);
        }
    }
    let mut next_fresh = max_id + 1;

    let mut stack: Vec<Vec<Layer>> = vec![Vec::new()];
    for (index, rec) in file.layers.iter().enumerate() {
        let section = rec.block(b"lsct").or(rec.block(b"lsdk")).and_then(|d| Reader::new(d).u32().ok());
        match section {
            Some(3) => {
                stack.push(Vec::new());
                continue;
            }
            Some(1) | Some(2) => {
                let children = if stack.len() > 1 {
                    stack.pop().unwrap_or_default()
                } else {
                    cx.warn("A layer group in this file was damaged; its end marker had no start.");
                    Vec::new()
                };
                let mut layer = common(&mut cx, rec, &mut used, &mut next_fresh);
                let key = rec.block(b"lsct").or(rec.block(b"lsdk")).and_then(|d| d.get(8..12)).map(|k| [k[0], k[1], k[2], k[3]]).unwrap_or(rec.blend);
                let pass_through = &key == b"pass";
                layer.blend = if pass_through { BlendMode::Normal } else { blend_mode(&mut cx, &key) };
                layer.kind = LayerKind::Group { children, pass_through, expanded: section == Some(1) };
                finish_layer(&mut cx, rec, &mut layer, index)?;
                stack.last_mut().unwrap().push(layer);
                continue;
            }
            _ => {}
        }
        let mut layer = common(&mut cx, rec, &mut used, &mut next_fresh);
        build_kind(&mut cx, rec, &mut layer)?;
        finish_layer(&mut cx, rec, &mut layer, index)?;
        stack.last_mut().unwrap().push(layer);
    }
    while stack.len() > 1 {
        // An unterminated group: keep its layers at the parent level.
        let orphans = stack.pop().unwrap();
        stack.last_mut().unwrap().extend(orphans);
        cx.warn("A layer group in this file was never closed; its layers were kept ungrouped.");
    }
    doc.layers = stack.pop().unwrap_or_default();

    if file.layers.is_empty() {
        // A flat file: the merged image is the only picture.
        if let Some(rgba) = merged_rgba_budget(&file, &mut cx.budget)? {
            let id = next_fresh;
            let plane = Plane::from_raw(h.width, h.height, 4, &rgba, [0; 4]);
            doc.layers.push(Layer::new(id, "Background", LayerKind::Pixel(Raster::new(plane, 0, 0))));
        }
    }
    doc.reserve_ids();
    doc.active = doc.layers.last().map(|l| l.id);

    // What to carry to the next export.
    let mut sc = std::mem::take(&mut cx.sidecar);
    for b in &file.global_blocks {
        let drop = matches!(&b.key, b"Lr16" | b"Lr32" | b"Layr" | b"Mt16" | b"Mt32" | b"Mtrn" | b"Alph" | b"FMsk");
        if !drop {
            sc.global_blocks.push((b.key, b.data.to_vec()));
        }
    }
    sc.store(&mut doc);
    let warnings = cx.warnings;
    let left = cx.budget;
    Ok((Imported { doc, warnings }, left))
}

const DROPPED_RESOURCES: [u16; 17] = [1005, 1039, 1032, 1024, 1026, 1033, 1036, 1069, 1072, 1006, 1045, 1053, 1067, 1077, 1007, 1047, 1046];

fn read_resources(cx: &mut Ctx, doc: &mut Document) {
    let file = cx.file;
    if let Some(d) = file.resource(1005) {
        let mut r = Reader::new(d);
        if let (Ok(res), Ok(unit)) = (r.u32(), r.u16()) {
            let mut ppi = res as f32 / 65536.0;
            if unit == 2 {
                ppi *= 2.54;
            }
            if ppi.is_finite() && ppi > 0.0 {
                doc.resolution = ppi;
            }
        }
    }
    if let Some(icc) = file.resource(1039) {
        doc.meta.icc = Some(icc.to_vec());
    }
    if let Some(d) = file.resource(1032) {
        let mut r = Reader::new(d);
        if r.skip(12, "").is_ok() {
            if let Ok(n) = r.u32() {
                for _ in 0..n.min(10_000) {
                    let (Ok(loc), Ok(dir)) = (r.i32(), r.u8()) else { break };
                    doc.guides.push(Guide { vertical: dir == 0, position: loc as f64 / 32.0 });
                }
            }
        }
    }
    for res in &file.resources {
        if &res.sig == b"8BIM" && DROPPED_RESOURCES.contains(&res.id) {
            continue;
        }
        cx.sidecar.resources.push((res.id, res.name.clone(), res.data.to_vec()));
    }
}

fn blend_mode(cx: &mut Ctx, key: &[u8; 4]) -> BlendMode {
    match BlendMode::from_psd_key(key) {
        Some(m) => m,
        None => {
            if key != b"pass" {
                cx.warn(format!("Blend mode \u{201c}{}\u{201d} is not supported; Normal is used.", String::from_utf8_lossy(key)));
            }
            BlendMode::Normal
        }
    }
}

/// Properties every layer kind has.
fn common(cx: &mut Ctx, rec: &LayerRecord, used: &mut HashSet<LayerId>, next_fresh: &mut u32) -> Layer {
    let name = rec.block(b"luni").and_then(|d| Reader::new(d).unicode().ok()).unwrap_or_else(|| rec.name.clone());
    let lyid = rec.block(b"lyid").and_then(|d| Reader::new(d).u32().ok()).filter(|&i| i > 0 && !used.contains(&i));
    let id = lyid.unwrap_or_else(|| {
        let i = *next_fresh;
        *next_fresh += 1;
        i
    });
    used.insert(id);
    let mut l = Layer::new(id, name, LayerKind::Pixel(Raster::empty(0, 0)));
    l.visible = !rec.hidden();
    l.opacity = rec.opacity as f32 / 255.0;
    l.fill_opacity = rec.block(b"iOpa").and_then(|d| d.first()).map(|&v| v as f32 / 255.0).unwrap_or(1.0);
    l.blend = blend_mode(cx, &rec.blend);
    l.clip = rec.clipping == 1;
    if let Some(bits) = rec.block(b"lspf").and_then(|d| Reader::new(d).u32().ok()) {
        l.locks = Locks { transparency: bits & 1 != 0, pixels: bits & 2 != 0, position: bits & 4 != 0, all: bits & 0x8000_0000 != 0 };
    }
    if let Some(c) = rec.block(b"lclr").and_then(|d| Reader::new(d).u16().ok()) {
        l.color_label = if c <= 7 { c as u8 } else { 0 };
    }
    l
}

const HANDLED_KEYS: [&[u8; 4]; 12] = [b"luni", b"lyid", b"lsct", b"lsdk", b"iOpa", b"lspf", b"lclr", b"TySh", b"SoLd", b"SoLE", b"PlLd", b"tySh"];
const EFFECT_KEYS: [&[u8; 4]; 3] = [b"lfx2", b"lmfx", b"lrFX"];
const VECTOR_KEYS: [&[u8; 4]; 4] = [b"vmsk", b"vsms", b"vogk", b"vscg"];

fn rect_size(r: &Rect32) -> Result<(u32, u32)> {
    let (w, h) = (r.width(), r.height());
    if w > 300_000 || h > 300_000 {
        return Err(crate::error::corrupt(format!("a layer of {w}×{h} pixels")));
    }
    Ok((w as u32, h as u32))
}

/// A channel of a layer record decoded to 8 bits, or `None` when absent.
fn channel(cx: &mut Ctx, ch: Option<&Channel>, w: u32, h: u32, color: bool) -> Result<Option<Vec<u8>>> {
    let Some(ch) = ch else { return Ok(None) };
    if ch.raw.len() < 2 || w == 0 || h == 0 {
        return Ok(None);
    }
    let hd = cx.file.header;
    let comp = u16::from_be_bytes([ch.raw[0], ch.raw[1]]);
    let pixels = w as u64 * h as u64;
    cx.take(pixels * (hd.depth as u64).div_ceil(8).max(1) + pixels)?;
    match compress::decode(&ch.raw[2..], comp, w as usize, h as usize, hd.depth, hd.psb()) {
        Ok(d) => Ok(Some(to_u8(&d, hd.depth, pixels as usize, w as usize, color))),
        Err(PsdError::Corrupt(msg)) => {
            cx.warn(format!("Some layer pixels were damaged and are shown as transparent ({msg})."));
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// The record's colour and transparency channels as an RGBA raster.
fn layer_raster(cx: &mut Ctx, rec: &LayerRecord) -> Result<Raster> {
    let (w, h) = rect_size(&rec.rect)?;
    let hd = cx.file.header;
    if w == 0 || h == 0 {
        return Ok(Raster::new(Plane::transparent(0, 0), rec.rect.left, rec.rect.top));
    }
    let cc = color_channels(hd.mode);
    let mut chans = Vec::with_capacity(cc);
    for i in 0..cc {
        chans.push(channel(cx, rec.channel(i as i16), w, h, true)?);
    }
    let alpha = channel(cx, rec.channel(-1), w, h, false)?;
    let pixels = w as usize * h as usize;
    cx.take(pixels as u64 * 4)?;
    let rgb = mode_to_rgb(hd.mode, &chans, pixels, cx.file.color_mode_data);
    drop(chans);
    let mut plane = Plane::transparent(w, h);
    let strip = 256usize;
    let mut row = vec![0u8; w as usize * strip * 4];
    for y0 in (0..h as usize).step_by(strip) {
        let rows = strip.min(h as usize - y0);
        let n = w as usize * rows;
        for i in 0..n {
            let p = y0 * w as usize + i;
            let c = rgb[p];
            row[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], alpha.as_ref().map_or(255, |a| a[p])]);
        }
        plane.write(Rect::new(0, y0 as i32, w as i32, rows as i32), &row[..n * 4]);
    }
    plane.compact();
    Ok(Raster::new(plane, rec.rect.left, rec.rect.top))
}

/// A single-channel mask raster.
fn mask_raster(cx: &mut Ctx, rec: &LayerRecord, id: i16, rect: Rect32, default: u8) -> Result<Option<Raster>> {
    let Some(ch) = rec.channel(id) else { return Ok(None) };
    let (w, h) = rect_size(&rect)?;
    let mut plane = Plane::mask(w, h, default);
    if let Some(data) = channel(cx, Some(ch), w, h, false)? {
        plane.write(Rect::new(0, 0, w as i32, h as i32), &data);
        plane.compact();
    }
    Ok(Some(Raster::new(plane, rect.left, rect.top)))
}

/// Bake mask density into the values: `1 - d·(1 - m)`.
fn apply_density(r: &Raster, d: f32) -> Raster {
    let f = r.plane.fill()[0];
    let fill = (255.0 - d * (255.0 - f as f32)).round() as u8;
    let rect = r.plane.bounds();
    let data: Vec<u8> = r.plane.read_vec(rect).iter().map(|&m| (255.0 - d * (255.0 - m as f32)).round() as u8).collect();
    let mut p = Plane::mask(rect.w.max(0) as u32, rect.h.max(0) as u32, fill);
    if !rect.is_empty() {
        p.write(rect, &data);
    }
    p.compact();
    Raster::new(p, r.x, r.y)
}

/// Multiply two masks into one covering both rectangles.
fn combine_masks(a: &Raster, b: &Raster) -> Raster {
    let ra = a.doc_rect();
    let rb = b.doc_rect();
    let fa = a.plane.fill()[0];
    let fb = b.plane.fill()[0];
    let fill = ((fa as u32 * fb as u32 + 127) / 255) as u8;
    let u = if ra.is_empty() { rb } else if rb.is_empty() { ra } else { ra.union(&rb) };
    let mut plane = Plane::mask(u.w.max(0) as u32, u.h.max(0) as u32, fill);
    if !u.is_empty() {
        let da = a.plane.read_vec(u.translate(-a.x, -a.y));
        let db = b.plane.read_vec(u.translate(-b.x, -b.y));
        let out: Vec<u8> = da.iter().zip(db.iter()).map(|(&x, &y)| ((x as u32 * y as u32 + 127) / 255) as u8).collect();
        plane.write(Rect::new(0, 0, u.w, u.h), &out);
        plane.compact();
    }
    Raster::new(plane, u.x, u.y)
}

pub fn plane_sig(r: &Raster) -> u64 {
    let p = &r.plane;
    let mut h = fnv(&[p.width().to_le_bytes(), p.height().to_le_bytes(), (r.x as u32).to_le_bytes(), (r.y as u32).to_le_bytes()].concat());
    h = crate::sidecar::fnv_extend(h, &p.fill());
    let cols = p.width().div_ceil(256).max(1);
    let rows = p.height().div_ceil(256).max(1);
    for ty in 0..rows {
        for tx in 0..cols {
            if let Some(t) = p.tile(tx, ty) {
                h = crate::sidecar::fnv_extend(h, &[tx.to_le_bytes(), ty.to_le_bytes()].concat());
                h = crate::sidecar::fnv_extend(h, t.data());
            }
        }
    }
    h
}

pub fn kind_sig(kind: &LayerKind) -> u64 {
    let json = |v: serde_json::Result<String>| fnv(v.unwrap_or_default().as_bytes());
    match kind {
        LayerKind::Pixel(r) => plane_sig(r),
        LayerKind::Adjustment(a) => json(serde_json::to_string(a)),
        LayerKind::Fill(f) => json(serde_json::to_string(f)),
        LayerKind::Group { .. } => 0,
        LayerKind::Text { data, .. } => json(serde_json::to_string(data)),
        LayerKind::Shape { data, raster } => json(serde_json::to_string(data)) ^ plane_sig(raster),
        LayerKind::Smart { source, quad, filters, raster, .. } => {
            let (w, hh) = source.size();
            let q: Vec<f64> = quad.iter().flat_map(|p| [p.x, p.y]).collect();
            json(serde_json::to_string(&(w, hh, q, filters))) ^ plane_sig(raster)
        }
    }
}

pub fn effects_sig(fx: &Option<editor_core::effects::LayerEffects>) -> u64 {
    fx.as_ref().map(|f| fnv(serde_json::to_string(f).unwrap_or_default().as_bytes())).unwrap_or(0)
}

fn build_kind<'a>(cx: &mut Ctx<'a, '_>, rec: &LayerRecord<'a>, layer: &mut Layer) -> Result<()> {
    let file = cx.file;
    let get = |k: &[u8; 4]| -> Option<&'a [u8]> { rec.block(k) };
    let mut extra = LayerExtra::default();
    // Gradients align to the shape for shape layers, else to the canvas.
    let has_vector = get(b"vmsk").is_some() || get(b"vsms").is_some();
    let doc_bounds = if has_vector && rec.rect.width() > 0 && rec.rect.height() > 0 {
        (rec.rect.left as f64, rec.rect.top as f64, rec.rect.width() as f64, rec.rect.height() as f64)
    } else {
        (0.0, 0.0, file.header.width as f64, file.header.height as f64)
    };
    let prov = |layer: &mut Layer, p: &str| layer.provenance.push(p.to_string());

    if let Some(ty) = get(b"TySh") {
        let raster = layer_raster(cx, rec)?;
        match text::read(ty, rec.rect.left, rec.rect.top) {
            Ok(t) => {
                layer.kind = LayerKind::Text { data: t.data, raster };
            }
            Err(e) => {
                cx.warn(format!("A text layer could not be read as text and was kept as pixels ({e})."));
                layer.kind = LayerKind::Pixel(raster);
                prov(layer, "psd:text");
            }
        }
        extra.kind_blocks.push((*b"TySh", ty.to_vec()));
    } else if let Some((key, data)) = [b"SoLd", b"SoLE", b"PlLd"].iter().find_map(|k| get(k).map(|d| (**k, d))) {
        let raster = layer_raster(cx, rec)?;
        let placement = if &key == b"PlLd" { linked::read_plld(data) } else { linked::read_sold(data) };
        let mut source = None;
        let mut quad = editor_core::smart::rect_quad(raster.doc_rect());
        if let Ok(p) = &placement {
            if p.has_filters {
                cx.warn("Smart filters are shown as Photoshop rendered them and are not editable here.");
            }
            if p.has_warp {
                cx.warn("Warped smart objects are shown as Photoshop rendered them; the warp is not editable here.");
            }
            if let Some(cached) = cx.sources.get(&p.id) {
                source = cached.clone();
            } else if let Some(f) = cx.linked.iter().find(|f| f.id == p.id).cloned() {
                source = decode_linked(cx, &f);
                cx.sources.insert(p.id.clone(), source.clone());
            }
            if source.is_some() {
                if let Some(q) = p.quad {
                    quad = q;
                }
            }
        }
        let source = match source {
            Some(s) => s,
            None => {
                // The rendered pixels stand in for the contents.
                quad = editor_core::smart::rect_quad(raster.doc_rect());
                SmartSource::Pixels(raster.plane.clone())
            }
        };
        layer.kind = LayerKind::Smart { source, quad, filters: Vec::new(), raster, stale: false };
        prov(layer, "psd:smart-object");
        for k in [b"SoLd", b"SoLE", b"PlLd"] {
            if let Some(d) = get(k) {
                extra.kind_blocks.push((*k, d.to_vec()));
            }
        }
    } else if let Some((key, data)) = FILL_KEYS.iter().find_map(|k| get(k).map(|d| (**k, d))) {
        let stroked = get(b"vstk").is_some_and(|d| {
            Reader::new(d).u32().ok().and_then(|_| crate::descriptor::Descriptor::read_versioned(&mut Reader::new(&d[4..])).ok()).is_some_and(|s| s.boolean("strokeEnabled").unwrap_or(false))
        });
        let fill = if stroked { None } else { kinds::read_fill(&key, data, doc_bounds).ok() };
        match fill {
            Some(f) => layer.kind = LayerKind::Fill(f),
            None => {
                let raster = layer_raster(cx, rec)?;
                layer.kind = LayerKind::Pixel(raster);
                prov(layer, if &key == b"PtFl" { "psd:pattern-fill" } else if stroked { "psd:shape" } else { "psd:gradient-fill" });
                if &key == b"PtFl" {
                    cx.warn("Pattern fill layers are shown as pixels; Photoshop will still see them as pattern fills.");
                } else if stroked {
                    cx.warn("Shape layers with strokes are shown as pixels; Photoshop will still see them as shapes.");
                } else {
                    cx.warn("Noise gradient fill layers are shown as pixels; Photoshop will still see them as gradient fills.");
                }
            }
        }
        extra.kind_blocks.push((key, data.to_vec()));
        if let Some(d) = get(b"vstk") {
            extra.kind_blocks.push((*b"vstk", d.to_vec()));
        }
    } else if let Some(res) = kinds::read_adjustment(&get) {
        match res {
            Ok(a) => layer.kind = LayerKind::Adjustment(a),
            Err(what) => {
                cx.warn(format!("An adjustment layer ({what}) is not supported and has no effect here; Photoshop will still apply it."));
                layer.kind = LayerKind::Pixel(layer_raster(cx, rec)?);
                prov(layer, "psd:unsupported-adjustment");
            }
        }
        for k in ADJUSTMENT_KEYS {
            if let Some(d) = get(k) {
                extra.kind_blocks.push((*k, d.to_vec()));
            }
        }
    } else {
        layer.kind = LayerKind::Pixel(layer_raster(cx, rec)?);
    }
    if let Some(Adjustment::ColorLookup(_)) = match &layer.kind {
        LayerKind::Adjustment(a) => Some(a),
        _ => None,
    } {
        // Nothing extra: the LUT is fully modelled.
    }
    extra.kind_sig = kind_sig(&layer.kind);
    let slot = cx.sidecar.layers.entry(layer.id).or_default();
    slot.kind_blocks = extra.kind_blocks;
    slot.kind_sig = extra.kind_sig;
    Ok(())
}

fn decode_linked(cx: &mut Ctx, f: &LinkedFile) -> Option<SmartSource> {
    match &f.file_type {
        b"8BPS" if cx.nesting < cx.opts.max_nesting => match import_nested(f.data, cx.opts, cx.nesting + 1, cx.budget) {
            Ok((inner, left)) => {
                cx.budget = left;
                Some(SmartSource::Document(Box::new(inner.doc)))
            }
            Err(_) => None,
        },
        b"png " => {
            let dec = png::Decoder::new(std::io::Cursor::new(f.data));
            let mut reader = dec.read_info().ok()?;
            let (w, h) = (reader.info().width, reader.info().height);
            if cx.take(w as u64 * h as u64 * 8).is_err() {
                return None;
            }
            let mut buf = vec![0u8; reader.output_buffer_size()];
            let info = reader.next_frame(&mut buf).ok()?;
            let data = &buf[..info.buffer_size()];
            let rgba: Vec<u8> = match (info.color_type, info.bit_depth) {
                (png::ColorType::Rgba, png::BitDepth::Eight) => data.to_vec(),
                (png::ColorType::Rgb, png::BitDepth::Eight) => data.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
                (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => data.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
                (png::ColorType::Grayscale, png::BitDepth::Eight) => data.iter().flat_map(|&c| [c, c, c, 255]).collect(),
                _ => return None,
            };
            (rgba.len() == w as usize * h as usize * 4).then(|| SmartSource::Pixels(Plane::from_raw(w, h, 4, &rgba, [0; 4])))
        }
        _ => None,
    }
}

/// Mask, effects, vector mask and the verbatim blocks.
fn finish_layer<'a>(cx: &mut Ctx<'a, '_>, rec: &LayerRecord<'a>, layer: &mut Layer, _index: usize) -> Result<()> {
    let get = |k: &[u8; 4]| -> Option<&'a [u8]> { rec.block(k) };

    // Pixel mask (the real user mask when a vector mask is also present).
    let mut pixel_mask: Option<(Raster, bool, f32)> = None;
    let mut vector: Option<Raster> = None;
    let vector_setting = get(b"vmsk").or(get(b"vsms"));
    let vector_disabled = vector_setting.and_then(|d| Reader::new(d.get(4..).unwrap_or(&[])).u32().ok()).is_some_and(|f| f & 4 != 0);
    if let Some(info) = rec.mask.clone() {
        let density = info.user_density.map(|d| d as f32 / 255.0).unwrap_or(1.0);
        if let Some((rflags, rdefault, rrect)) = info.real {
            if let Some(r) = mask_raster(cx, rec, -3, rrect, rdefault)? {
                pixel_mask = Some((r, rflags & MaskInfo::DISABLED == 0, density));
            }
            if let Some(v) = mask_raster(cx, rec, -2, info.rect, info.default_color)? {
                vector = Some(v);
            }
        } else if info.flags & MaskInfo::FROM_RENDER != 0 && vector_setting.is_some() {
            vector = mask_raster(cx, rec, -2, info.rect, info.default_color)?;
        } else if let Some(r) = mask_raster(cx, rec, -2, info.rect, info.default_color)? {
            pixel_mask = Some((r, info.flags & MaskInfo::DISABLED == 0, density));
        }
        if info.user_feather.is_some_and(|f| f > 0.0) {
            cx.warn("Mask feather is not supported; masks are shown unfeathered.");
        }
    }
    let is_fill = matches!(layer.kind, LayerKind::Fill(_));
    if vector.is_none() && vector_setting.is_some() && is_fill && rec.rect.width() > 0 && rec.rect.height() > 0 {
        // A shape layer: its transparency channel is the rendered shape.
        let (w, h) = rect_size(&rec.rect)?;
        let mut plane = Plane::mask(w, h, 0);
        if let Some(a) = channel(cx, rec.channel(-1), w, h, false)? {
            plane.write(Rect::new(0, 0, w as i32, h as i32), &a);
            plane.compact();
            vector = Some(Raster::new(plane, rec.rect.left, rec.rect.top));
        }
    }
    if vector.is_none() && !layer.is_group() {
        if let Some(vm) = vector_setting.and_then(|d| crate::vector::parse(d, cx.file.header.width, cx.file.header.height)) {
            if !vm.disabled {
                let (dw, dh) = (cx.file.header.width, cx.file.header.height);
                cx.take(dw as u64 * dh as u64 * 5)?;
                vector = Some(vm.rasterize(dw, dh));
            }
        }
    }
    let mut vector = if vector_disabled { None } else { vector };
    if let (Some(v), Some(d)) = (vector.as_mut(), rec.mask.as_ref().and_then(|m| m.vector_density)) {
        if d < 255 {
            *v = apply_density(v, d as f32 / 255.0);
        }
    }
    let mut vector_sig = 0;
    layer.mask = match (pixel_mask, vector) {
        (None, None) => None,
        (Some((r, enabled, density)), None) => Some(LayerMask { raster: r, enabled, linked: true, density }),
        (None, Some(v)) => {
            let m = LayerMask { raster: v, enabled: true, linked: true, density: 1.0 };
            vector_sig = plane_sig(&m.raster);
            Some(m)
        }
        (Some((mut r, enabled, density)), Some(v)) => {
            if density < 1.0 {
                // Density applies to the pixel mask only.
                r = apply_density(&r, density);
            }
            let raster = if enabled { combine_masks(&r, &v) } else { v };
            Some(LayerMask { raster, enabled: true, linked: true, density: 1.0 })
        }
    };

    // Layer style.
    let mut effects_blocks = Vec::new();
    for k in EFFECT_KEYS {
        if let Some(d) = get(k) {
            effects_blocks.push((*k, d.to_vec()));
        }
    }
    if let Some(d) = get(b"lfx2").or(get(b"lmfx")) {
        match effects::read(d, cx.light) {
            Ok((fx, dropped)) => {
                for what in dropped {
                    cx.warn(format!("Layer style: {what} is not supported."));
                }
                layer.effects = Some(fx);
            }
            Err(e) => cx.warn(format!("A layer style could not be read ({e}); Photoshop will still show it.")),
        }
    } else if get(b"lrFX").is_some() {
        cx.warn("An old-style layer effect is not shown here; Photoshop will still show it.");
    }

    let non_default_ranges = rec.blending_ranges.chunks(4).any(|c| c.len() == 4 && c != [0, 0, 255, 255]);
    if non_default_ranges {
        cx.warn("\u{201c}Blend If\u{201d} settings are kept for Photoshop but not shown here.");
    }
    if get(b"knko").is_some_and(|d| d.first().is_some_and(|&v| v != 0)) {
        cx.warn("Knockout settings are kept for Photoshop but not shown here.");
    }
    if get(b"clbl").is_some_and(|d| d.first() == Some(&0)) {
        cx.warn("\u{201c}Blend clipped layers as group\u{201d} is off in this file; clipped layers are blended as a group here.");
    }

    let native = cx.native;
    let slot = cx.sidecar.layers.entry(layer.id).or_default();
    if !native {
        slot.kind_blocks.clear();
    }
    slot.effects_sig = effects_sig(&layer.effects);
    slot.effects_blocks = effects_blocks;
    if vector_sig != 0 || vector_setting.is_some() {
        slot.vector_sig = vector_sig;
        for k in VECTOR_KEYS {
            if let Some(d) = get(k) {
                slot.vector_blocks.push((*k, d.to_vec()));
            }
        }
    }
    slot.blending_ranges = rec.blending_ranges.to_vec();
    for b in &rec.blocks {
        let known = HANDLED_KEYS.contains(&&b.key)
            || EFFECT_KEYS.contains(&&b.key)
            || VECTOR_KEYS.contains(&&b.key)
            || ADJUSTMENT_KEYS.contains(&&b.key)
            || FILL_KEYS.contains(&&b.key)
            || &b.key == b"vstk";
        if !known {
            slot.blocks.push((b.key, b.data.to_vec()));
        }
    }
    if *slot == LayerExtra::default() {
        cx.sidecar.layers.remove(&layer.id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The merged image

/// Whether the merged image's first extra channel is transparency (the
/// rules psd-tools uses).
pub fn merged_has_transparency(file: &PsdFile) -> bool {
    let cc = color_channels(file.header.mode);
    if file.header.channels as usize <= cc {
        return false;
    }
    if file.global_blocks.iter().any(|b| matches!(&b.key, b"Mtrn" | b"Mt16" | b"Mt32")) || file.merged_alpha {
        return true;
    }
    if let Some(ids) = file.resource(1053) {
        if ids.len() >= 4 && ids.as_chunks::<4>().0.iter().all(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]) > 0) {
            return false;
        }
    }
    file.layers.is_empty()
}

/// The file's own composite as straight 8-bit RGBA.
pub fn merged_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let file = parse(bytes)?;
    let mut budget = ImportOptions::default().max_bytes;
    let rgba = merged_rgba_budget(&file, &mut budget)?.ok_or_else(|| crate::error::corrupt("no merged image"))?;
    Ok((file.header.width, file.header.height, rgba))
}

fn merged_rgba_budget(file: &PsdFile, budget: &mut u64) -> Result<Option<Vec<u8>>> {
    let h = file.header;
    if file.merged.len() < 2 {
        return Ok(None);
    }
    let comp = u16::from_be_bytes([file.merged[0], file.merged[1]]);
    let (w, hh) = (h.width as usize, h.height as usize);
    let cc = color_channels(h.mode);
    let transparent = merged_has_transparency(file);
    let nch = (cc + transparent as usize).min(h.channels as usize);
    let bps = (h.depth as usize).div_ceil(8).max(1);
    let need = (w * hh * (h.channels as usize) * bps + w * hh * 8) as u64;
    if need > *budget {
        return Err(PsdError::TooLarge { needed: need, limit: *budget });
    }
    *budget -= need;
    // RLE and raw store all channels back to back; decode them together.
    let all = compress::decode(&file.merged[2..], comp, w, hh * h.channels as usize, h.depth, h.psb())?;
    let plane_len = compress::row_bytes(w, h.depth) * hh;
    let pixels = w * hh;
    let mut chans: Vec<Option<Vec<u8>>> = Vec::new();
    for i in 0..nch {
        let s = all.get(i * plane_len..(i + 1) * plane_len).ok_or_else(|| crate::error::corrupt("merged image is short"))?;
        chans.push(Some(to_u8(s, h.depth, pixels, w, i < cc)));
    }
    let alpha = if transparent { chans.get(cc).cloned().flatten() } else { None };
    let rgb = mode_to_rgb(h.mode, &chans[..cc.min(chans.len())], pixels, file.color_mode_data);
    let mut out = vec![0u8; pixels * 4];
    for p in 0..pixels {
        let a = alpha.as_ref().map_or(255, |a| a[p]);
        let mut c = rgb[p];
        if a < 255 && matches!(h.mode, MODE_RGB | MODE_GRAYSCALE) {
            // Photoshop mattes the merged colours against white.
            if a == 0 {
                c = [0, 0, 0];
            } else {
                let af = a as f32 / 255.0;
                c = c.map(|v| (((v as f32 / 255.0 - (1.0 - af)) / af).clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        out[p * 4..p * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert_eq!(import(b"hello", &ImportOptions::default()).unwrap_err(), PsdError::NotPsd);
        assert!(import(&[], &ImportOptions::default()).is_err());
    }
}
