//! The native project format: lossless, layered, versioned.
//!
//! A project is a ZIP archive:
//!
//! - `manifest.json` — format name and version, the document (size,
//!   resolution, guides, metadata) and the full layer tree with every
//!   parameter, text and shape source, smart-object source and provenance.
//!   History is not saved.
//! - `planes/<n>.tiles` — each pixel plane's present tiles, each deflated
//!   on its own. Absent tiles stay absent, so a reload is byte-identical.
//! - `icc.icc`, `extra/<key>.bin` — the source ICC profile and opaque
//!   per-format data (such as the PSD sidecar) kept verbatim.
//!
//! Readers accept any file whose `min_reader` is at most [`VERSION`],
//! ignore fields they do not know, and turn layer kinds they do not know
//! into empty pixel layers with a warning.

pub mod validate;
pub mod zip;

use std::collections::BTreeMap;
use std::sync::Arc;

use editor_core::adjust::Adjustment;
use editor_core::blend::BlendMode;
use editor_core::document::{DocMeta, Document, Guide, Sidecar};
use editor_core::effects::LayerEffects;
use editor_core::geom::Point;
use editor_core::layer::{Fill, Layer, LayerId, LayerKind, LayerMask, Locks, Raster, ShapeData, SmartFilter, SmartSource, TextData};
use editor_core::plane::{Plane, TILE};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use zip::{ZipReader, ZipWriter};

/// The format version this code writes and fully understands.
pub const VERSION: u32 = 1;
pub const FORMAT: &str = "openphotoedit-project";
/// The name written before the 17 September 2026 rename. Still read.
pub const LEGACY_FORMAT: &str = "openphotoshop-project";
pub const EXTENSION: &str = "opproj";
pub const MIME: &str = "application/vnd.openphotoedit.project+zip";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("this is not an OpenPhotoEdit project")]
    NotProject,
    #[error("this project was made by a newer version of OpenPhotoEdit (format {0}); update to open it")]
    TooNew(u32),
    #[error("the project is damaged: {0}")]
    Corrupt(&'static str),
    #[error("the project is damaged: {0}")]
    Invalid(String),
    #[error("the project is too large")]
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Decoded bytes a load may allocate for pixel data.
    pub max_bytes: u64,
}

impl Default for LoadOptions {
    /// On 32-bit wasm the whole heap is at most 4 GB, and the document, its
    /// history, the render cache and the browser's copy of the file share
    /// it; 1.5 GB of decoded pixels is the most a load can take and still
    /// leave room to edit. (The previous 3 GB could never be reached: the
    /// tab ran out of memory first.)
    fn default() -> Self {
        LoadOptions { max_bytes: if cfg!(target_pointer_width = "32") { 1_500_000_000 } else { 32_000_000_000 } }
    }
}

#[derive(Debug)]
pub struct Loaded {
    pub doc: Document,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Manifest

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    #[serde(default = "one")]
    min_reader: u32,
    #[serde(default)]
    generator: String,
    document: DocJson,
}

fn one() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct DocJson {
    width: u32,
    height: u32,
    resolution: f32,
    active: Option<LayerId>,
    guides: Vec<GuideJson>,
    meta: DocMeta,
    icc: Option<String>,
    extra: BTreeMap<String, String>,
    selection: Option<usize>,
    layers: Vec<LayerJson>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct GuideJson {
    vertical: bool,
    position: f64,
}

#[derive(Serialize, Deserialize)]
struct RasterJson {
    plane: usize,
    x: i32,
    y: i32,
}

#[derive(Serialize, Deserialize)]
struct MaskJson {
    plane: usize,
    x: i32,
    y: i32,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default = "yes")]
    linked: bool,
    #[serde(default = "full")]
    density: f32,
}

fn yes() -> bool {
    true
}
fn full() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize)]
struct LayerJson {
    id: LayerId,
    #[serde(default)]
    name: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default = "full")]
    opacity: f32,
    #[serde(default = "full")]
    fill_opacity: f32,
    #[serde(default)]
    blend: BlendMode,
    #[serde(default)]
    clip: bool,
    #[serde(default)]
    locks: Locks,
    #[serde(default)]
    color_label: u8,
    #[serde(default)]
    provenance: Vec<String>,
    #[serde(default)]
    effects: Option<LayerEffects>,
    #[serde(default)]
    mask: Option<MaskJson>,
    /// Kept as a value so an unknown kind does not fail the whole file.
    kind: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum KindJson {
    Pixel { raster: RasterJson },
    Adjustment { adjustment: Adjustment },
    Fill { fill: Fill },
    Group { children: Vec<LayerJson>, pass_through: bool, expanded: bool },
    Text { data: TextData, raster: RasterJson },
    Shape { data: ShapeData, raster: RasterJson },
    Smart { source: SourceJson, quad: [Point; 4], filters: Vec<SmartFilter>, raster: RasterJson, stale: bool },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SourceJson {
    Pixels(usize),
    Document(Box<DocJson>),
}

// ---------------------------------------------------------------------------
// Planes

const PLANE_MAGIC: &[u8; 8] = b"OPPLANE1";

fn encode_plane(p: &Plane) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(PLANE_MAGIC);
    out.extend_from_slice(&p.width().to_le_bytes());
    out.extend_from_slice(&p.height().to_le_bytes());
    out.push(p.channels() as u8);
    out.extend_from_slice(&p.fill());
    let cols = p.width().div_ceil(TILE).max(1);
    let rows = p.height().div_ceil(TILE).max(1);
    let mut tiles = Vec::new();
    for ty in 0..rows {
        for tx in 0..cols {
            if let Some(t) = p.tile(tx, ty) {
                tiles.push((tx, ty, miniz_oxide::deflate::compress_to_vec(t.data(), 3)));
            }
        }
    }
    out.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
    for (tx, ty, z) in tiles {
        out.extend_from_slice(&tx.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&(z.len() as u32).to_le_bytes());
        out.extend_from_slice(&z);
    }
    out
}

fn decode_plane(data: &[u8], budget: &mut u64) -> Result<Plane, Error> {
    let bad = || Error::Corrupt("a pixel plane is damaged");
    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(Error::Corrupt("a pixel plane is truncated"));
    if data.get(..8) != Some(PLANE_MAGIC) {
        return Err(bad());
    }
    let (w, h) = (u32_at(8)?, u32_at(12)?);
    let ch = *data.get(16).ok_or_else(bad)? as usize;
    let fill = data.get(17..21).ok_or_else(bad)?;
    if w as f64 > validate::MAX_SIDE || h as f64 > validate::MAX_SIDE || !(ch == 1 || ch == 4) {
        return Err(bad());
    }
    let cols = w.div_ceil(TILE).max(1);
    let rows = h.div_ceil(TILE).max(1);
    let index_bytes = cols as u64 * rows as u64 * 8;
    if index_bytes > *budget {
        return Err(Error::TooLarge);
    }
    let mut plane = Plane::new(w, h, ch, [fill[0], fill[1], fill[2], fill[3]]);
    let count = u32_at(21)? as u64;
    if count > cols as u64 * rows as u64 {
        return Err(bad());
    }
    let tile_bytes = (TILE * TILE) as usize * ch;
    let mut at = 25;
    for _ in 0..count {
        let (tx, ty, len) = (u32_at(at)?, u32_at(at + 4)?, u32_at(at + 8)? as usize);
        at += 12;
        if tx >= cols || ty >= rows {
            return Err(bad());
        }
        let z = at.checked_add(len).and_then(|end| data.get(at..end)).ok_or(Error::Corrupt("a pixel plane is truncated"))?;
        at += len;
        if tile_bytes as u64 > *budget {
            return Err(Error::TooLarge);
        }
        *budget -= tile_bytes as u64;
        let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(z, tile_bytes).map_err(|_| bad())?;
        if raw.len() != tile_bytes {
            return Err(bad());
        }
        plane.tile_mut(tx, ty).copy_from_slice(&raw);
    }
    Ok(plane)
}

// ---------------------------------------------------------------------------
// Save

/// Size, channels, fill and tile identities.
type PlaneKey = (u32, u32, usize, [u8; 4], Vec<usize>);

struct Saver {
    planes: Vec<Vec<u8>>,
    /// Planes that share every tile (a smart object's source and its cached
    /// raster, say) are stored once.
    seen: std::collections::HashMap<PlaneKey, usize>,
}

impl Saver {
    fn plane(&mut self, p: &Plane) -> usize {
        let cols = p.width().div_ceil(TILE).max(1);
        let rows = p.height().div_ceil(TILE).max(1);
        let ptrs: Vec<usize> = (0..rows).flat_map(|ty| (0..cols).map(move |tx| (tx, ty))).map(|(tx, ty)| p.tile(tx, ty).map_or(0, |t| Arc::as_ptr(t) as usize)).collect();
        let key = (p.width(), p.height(), p.channels(), p.fill(), ptrs);
        if let Some(&i) = self.seen.get(&key) {
            return i;
        }
        self.planes.push(encode_plane(p));
        self.seen.insert(key, self.planes.len() - 1);
        self.planes.len() - 1
    }
    fn raster(&mut self, r: &Raster) -> RasterJson {
        RasterJson { plane: self.plane(&r.plane), x: r.x, y: r.y }
    }

    fn doc(&mut self, doc: &Document, extra_files: &mut Vec<(String, Vec<u8>)>, prefix: &str) -> DocJson {
        let mut extra = BTreeMap::new();
        for (k, v) in &doc.meta.extra {
            let safe: String = k.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
            let path = format!("{prefix}extra/{safe}.bin");
            extra_files.push((path.clone(), v.0.as_ref().clone()));
            extra.insert(k.clone(), path);
        }
        let icc = doc.meta.icc.as_ref().map(|icc| {
            let path = format!("{prefix}icc.icc");
            extra_files.push((path.clone(), icc.clone()));
            path
        });
        DocJson {
            width: doc.width,
            height: doc.height,
            resolution: doc.resolution,
            active: doc.active,
            guides: doc.guides.iter().map(|g| GuideJson { vertical: g.vertical, position: g.position }).collect(),
            meta: doc.meta.clone(),
            icc,
            extra,
            selection: doc.selection.as_ref().map(|s| self.plane(s)),
            layers: doc.layers.iter().enumerate().map(|(i, l)| self.layer(l, extra_files, &format!("{prefix}{i}/"))).collect(),
        }
    }

    fn layer(&mut self, l: &Layer, extra_files: &mut Vec<(String, Vec<u8>)>, prefix: &str) -> LayerJson {
        let kind = match &l.kind {
            LayerKind::Pixel(r) => KindJson::Pixel { raster: self.raster(r) },
            LayerKind::Adjustment(a) => KindJson::Adjustment { adjustment: a.clone() },
            LayerKind::Fill(f) => KindJson::Fill { fill: f.clone() },
            LayerKind::Group { children, pass_through, expanded } => KindJson::Group {
                children: children.iter().enumerate().map(|(i, c)| self.layer(c, extra_files, &format!("{prefix}{i}/"))).collect(),
                pass_through: *pass_through,
                expanded: *expanded,
            },
            LayerKind::Text { data, raster } => KindJson::Text { data: data.clone(), raster: self.raster(raster) },
            LayerKind::Shape { data, raster } => KindJson::Shape { data: data.clone(), raster: self.raster(raster) },
            LayerKind::Smart { source, quad, filters, raster, stale } => KindJson::Smart {
                source: match source {
                    SmartSource::Pixels(p) => SourceJson::Pixels(self.plane(p)),
                    SmartSource::Document(d) => SourceJson::Document(Box::new(self.doc(d, extra_files, &format!("smart/{prefix}")))),
                },
                quad: *quad,
                filters: filters.clone(),
                raster: self.raster(raster),
                stale: *stale,
            },
        };
        LayerJson {
            id: l.id,
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.opacity,
            fill_opacity: l.fill_opacity,
            blend: l.blend,
            clip: l.clip,
            locks: l.locks,
            color_label: l.color_label,
            provenance: l.provenance.clone(),
            effects: l.effects.clone(),
            mask: l.mask.as_ref().map(|m| MaskJson { plane: self.plane(&m.raster.plane), x: m.raster.x, y: m.raster.y, enabled: m.enabled, linked: m.linked, density: m.density }),
            kind: serde_json::to_value(kind).unwrap_or(Value::Null),
        }
    }
}

/// Serialise a document to project bytes.
pub fn save(doc: &Document) -> Result<Vec<u8>, Error> {
    let mut saver = Saver { planes: Vec::new(), seen: Default::default() };
    let mut files = Vec::new();
    let document = saver.doc(doc, &mut files, "");
    let manifest = Manifest { format: FORMAT.into(), version: VERSION, min_reader: 1, generator: format!("OpenPhotoEdit {}", env!("CARGO_PKG_VERSION")), document };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut z = ZipWriter::new();
    // First entry, stored, so the archive's start identifies the format.
    z.add("mimetype", MIME.as_bytes(), false)?;
    z.add("manifest.json", &json, true)?;
    for (i, p) in saver.planes.iter().enumerate() {
        z.add(&format!("planes/{i}.tiles"), p, false)?;
    }
    for (name, data) in files {
        z.add(&name, &data, true)?;
    }
    z.finish()
}

// ---------------------------------------------------------------------------
// Load

struct Loader<'a> {
    zip: ZipReader<'a>,
    budget: u64,
    warnings: Vec<String>,
    depth: usize,
}

impl Loader<'_> {
    fn plane(&mut self, i: usize, channels: usize) -> Result<Plane, Error> {
        let name = format!("planes/{i}.tiles");
        let data = self.zip.read(&name, self.budget)?.ok_or_else(|| Error::Invalid(format!("{name} is missing")))?;
        let p = decode_plane(&data, &mut self.budget)?;
        if p.channels() != channels {
            return Err(Error::Invalid(format!("{name} has {} channels, expected {channels}", p.channels())));
        }
        Ok(p)
    }

    fn raster(&mut self, r: &RasterJson) -> Result<Raster, Error> {
        Ok(Raster::new(self.plane(r.plane, 4)?, r.x, r.y))
    }

    fn doc(&mut self, d: DocJson) -> Result<Document, Error> {
        if d.width == 0 || d.height == 0 || d.width > 300_000 || d.height > 300_000 {
            return Err(Error::Invalid(format!("document size {}×{}", d.width, d.height)));
        }
        self.depth += 1;
        if self.depth > validate::MAX_SMART_DEPTH {
            return Err(Error::Corrupt("smart objects are nested too deeply"));
        }
        let mut doc = Document::new(d.width, d.height);
        doc.resolution = if d.resolution.is_finite() && d.resolution > 0.0 { d.resolution } else { 72.0 };
        doc.guides = d.guides.iter().map(|g| Guide { vertical: g.vertical, position: g.position }).collect();
        doc.meta = d.meta;
        if let Some(path) = &d.icc {
            doc.meta.icc = self.zip.read(path, 64_000_000)?;
        }
        for (k, path) in &d.extra {
            if let Some(bytes) = self.zip.read(path, self.budget)? {
                doc.meta.extra.insert(k.clone(), Sidecar(Arc::new(bytes)));
            }
        }
        if let Some(s) = d.selection {
            doc.selection = Some(self.plane(s, 1)?);
        }
        let mut ids = std::collections::HashSet::new();
        for l in d.layers {
            let layer = self.layer(l, &mut ids)?;
            doc.layers.push(layer);
        }
        doc.reserve_ids();
        doc.active = d.active.filter(|&a| doc.find(a).is_some());
        self.depth -= 1;
        Ok(doc)
    }

    fn layer(&mut self, l: LayerJson, ids: &mut std::collections::HashSet<LayerId>) -> Result<Layer, Error> {
        if !ids.insert(l.id) {
            return Err(Error::Invalid(format!("layer id {} is used twice", l.id)));
        }
        let kind = match serde_json::from_value::<KindJson>(l.kind.clone()) {
            Ok(KindJson::Pixel { raster }) => LayerKind::Pixel(self.raster(&raster)?),
            Ok(KindJson::Adjustment { adjustment }) => LayerKind::Adjustment(adjustment),
            Ok(KindJson::Fill { fill }) => LayerKind::Fill(fill),
            Ok(KindJson::Group { children, pass_through, expanded }) => {
                let mut out = Vec::with_capacity(children.len());
                for c in children {
                    out.push(self.layer(c, ids)?);
                }
                LayerKind::Group { children: out, pass_through, expanded }
            }
            Ok(KindJson::Text { data, raster }) => LayerKind::Text { data, raster: self.raster(&raster)? },
            Ok(KindJson::Shape { data, raster }) => LayerKind::Shape { data, raster: self.raster(&raster)? },
            Ok(KindJson::Smart { source, quad, filters, raster, stale }) => LayerKind::Smart {
                source: match source {
                    SourceJson::Pixels(p) => SmartSource::Pixels(self.plane(p, 4)?),
                    SourceJson::Document(d) => SmartSource::Document(Box::new(self.doc(*d)?)),
                },
                quad,
                filters,
                raster: self.raster(&raster)?,
                stale,
            },
            Err(e) => {
                let ty = l.kind.get("type").and_then(Value::as_str).unwrap_or("unknown");
                self.warnings.push(format!("Layer \u{201c}{}\u{201d} is a kind this version cannot show ({ty}); it was opened as an empty layer.", l.name));
                let _ = e;
                LayerKind::Pixel(Raster::empty(0, 0))
            }
        };
        let mut layer = Layer::new(l.id, l.name, kind);
        layer.visible = l.visible;
        layer.opacity = l.opacity.clamp(0.0, 1.0);
        layer.fill_opacity = l.fill_opacity.clamp(0.0, 1.0);
        layer.blend = l.blend;
        layer.clip = l.clip;
        layer.locks = l.locks;
        layer.color_label = l.color_label;
        layer.provenance = l.provenance;
        layer.effects = l.effects;
        if let Some(m) = l.mask {
            layer.mask = Some(LayerMask { raster: Raster::new(self.plane(m.plane, 1)?, m.x, m.y), enabled: m.enabled, linked: m.linked, density: m.density.clamp(0.0, 1.0) });
        }
        Ok(layer)
    }
}

/// Read project bytes into a document.
pub fn load(bytes: &[u8], opts: &LoadOptions) -> Result<Loaded, Error> {
    let zip = ZipReader::open(bytes)?;
    let json = zip.read("manifest.json", validate::MAX_MANIFEST_BYTES)?.ok_or(Error::NotProject)?;
    let manifest: Manifest = serde_json::from_slice(&json).map_err(|e| Error::Invalid(format!("manifest: {e}")))?;
    if manifest.format != FORMAT && manifest.format != LEGACY_FORMAT {
        return Err(Error::NotProject);
    }
    if manifest.min_reader > VERSION {
        return Err(Error::TooNew(manifest.version));
    }
    // Value limits, on the manifest as written (before `f32` turns 1e39
    // into infinity), before any plane is inflated.
    let raw: Value = serde_json::from_slice(&json).map_err(|e| Error::Invalid(format!("manifest: {e}")))?;
    validate::manifest(&raw["document"])?;
    let mut loader = Loader { zip, budget: opts.max_bytes, warnings: Vec::new(), depth: 0 };
    let doc = loader.doc(manifest.document)?;
    Ok(Loaded { doc, warnings: loader.warnings })
}

#[cfg(test)]
mod tests;
