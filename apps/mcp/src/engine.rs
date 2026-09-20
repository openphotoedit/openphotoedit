//! The engine, natively: open a file as a document, run JSON commands on it,
//! write it back out.
//!
//! `crates/editor-server/src/render.rs` already does this for one command
//! line; this is the same three steps with every domain crate registered, so
//! that the whole of `docs/commands.md` is reachable and not just the core
//! ops. Nothing in here knows what MCP is, which is what lets it be tested
//! without a protocol.
//!
//! **Paths in, paths out.** No function here hands back a full-resolution
//! image. [`preview`] exists, it is capped at [`MAX_PREVIEW_SIDE`] pixels on
//! the long side, and it is only ever called when someone asked for it.

use std::path::Path;

use editor_core::render::{flatten, render_layer_alone, render_view, to_u8, View};
use editor_core::{Document, Editor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The longest side a preview may have. A preview is for looking at, not for
/// working on; anything bigger belongs in a file.
pub const MAX_PREVIEW_SIDE: u32 = 768;
/// JPEG quality for previews. Low enough to stay small, high enough to judge
/// an edit by.
pub const PREVIEW_QUALITY: u8 = 70;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Encode(String),
    /// A command the engine refused. The message is the engine's own.
    #[error("{0}")]
    Engine(String),
    /// Something the caller could have got right.
    #[error("{0}")]
    Input(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// An editor with every domain crate registered.
///
/// Without this, `filter.*`, `paint.*`, `transform.*` and the harder half of
/// `select.*` are unknown operations: core only handles `doc`, `layer`,
/// `image` and the marquee shapes, and the rest register themselves.
/// `editor-ai` is deliberately absent — see the README.
pub fn new_editor() -> Editor {
    let mut ed = Editor::new(1, 1);
    editor_filters::register(&mut ed);
    editor_paint::register(&mut ed);
    editor_select::register(&mut ed);
    editor_transform::register(&mut ed);
    ed
}

// --- what a file is ------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// An ordinary flat image the `image` crate can decode.
    Raster,
    /// PSD or PSB.
    Psd,
    /// Our own `.opproj`.
    Project,
    /// A camera RAW file.
    Raw,
}

const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "crw", "dcr", "dng", "erf", "gpr", "iiq", "kdc", "mef", "mos",
    "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw", "x3f",
];

pub fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn family_of(path: &Path) -> Family {
    match extension(path).as_str() {
        "psd" | "psb" => Family::Psd,
        "opproj" => Family::Project,
        e if RAW_EXTENSIONS.contains(&e) => Family::Raw,
        _ => Family::Raster,
    }
}

// --- opening -------------------------------------------------------------

pub struct Opened {
    pub ed: Editor,
    pub family: Family,
    /// What the file turned out to be: "jpeg", "png", "psd", "opproj", "raw".
    pub format: String,
    /// How the source stored colour, before the engine's 8-bit straight RGBA.
    pub color_mode: String,
    /// Anything that could not be represented faithfully.
    pub warnings: Vec<String>,
}

pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}

/// Decode a flat image into upright straight RGBA8.
///
/// The EXIF orientation is applied here, as the browser does it, so a
/// portrait photograph taken on a phone is portrait in the document and
/// every coordinate an agent works out from `image_info` is the coordinate
/// it meant.
pub fn decode_raster(path: &Path) -> Result<(u32, u32, Vec<u8>, String, String)> {
    use image::ImageDecoder;
    let reader = image::ImageReader::open(path)
        .map_err(|e| Error::Read {
            path: path.display().to_string(),
            source: e,
        })?
        .with_guessed_format()
        .map_err(|e| Error::Read {
            path: path.display().to_string(),
            source: e,
        })?;
    let format = reader
        .format()
        .map(|f| format!("{f:?}").to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".into());
    let mut decoder = reader.into_decoder().map_err(|e| {
        Error::Decode(format!(
            "{} is not an image this build can read: {e}",
            path.display()
        ))
    })?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| Error::Decode(format!("could not decode {}: {e}", path.display())))?;
    let color_mode = format!("{:?}", img.color());
    img.apply_orientation(orientation);
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw(), format, color_mode))
}

/// Open any file we understand as an editable document.
pub fn open(path: &Path) -> Result<Opened> {
    let family = family_of(path);
    match family {
        Family::Psd => {
            let bytes = read_file(path)?;
            let imported = editor_psd::import(&bytes, &editor_psd::ImportOptions::default())
                .map_err(|e| Error::Decode(format!("{}: {e}", path.display())))?;
            let mut ed = new_editor();
            ed.doc = imported.doc;
            ed.revision += 1;
            Ok(Opened {
                ed,
                family,
                format: extension(path),
                // The importer converts everything to 8-bit RGB and says so
                // in its warnings; repeating the guess here would be worse
                // than pointing at them.
                color_mode: "rgba8 (converted on import)".into(),
                warnings: imported.warnings,
            })
        }
        Family::Project => {
            let bytes = read_file(path)?;
            let loaded = editor_project::load(&bytes, &editor_project::LoadOptions::default())
                .map_err(|e| Error::Decode(format!("{}: {e}", path.display())))?;
            let mut ed = new_editor();
            ed.doc = loaded.doc;
            ed.revision += 1;
            Ok(Opened {
                ed,
                family,
                format: "opproj".into(),
                color_mode: "rgba8".into(),
                warnings: loaded.warnings,
            })
        }
        Family::Raw => {
            let bytes = read_file(path)?;
            let developed = develop_raw(&bytes, &Value::Null)?;
            let mut ed = new_editor();
            open_pixels(
                &mut ed,
                developed.width,
                developed.height,
                &developed.rgba,
                path,
            )?;
            Ok(Opened {
                ed,
                family,
                format: extension(path),
                color_mode: "raw sensor data, developed to rgba8".into(),
                warnings: Vec::new(),
            })
        }
        Family::Raster => {
            let (w, h, rgba, format, color_mode) = decode_raster(path)?;
            let mut ed = new_editor();
            open_pixels(&mut ed, w, h, &rgba, path)?;
            Ok(Opened {
                ed,
                family,
                format,
                color_mode,
                warnings: Vec::new(),
            })
        }
    }
}

fn open_pixels(ed: &mut Editor, w: u32, h: u32, rgba: &[u8], path: &Path) -> Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let cmd = json!({ "op": "doc.open-pixels", "width": w, "height": h, "name": name });
    ed.exec(cmd, rgba)
        .map_err(|e| Error::Engine(format!("the engine refused to open the image: {e}")))?;
    Ok(())
}

// --- running commands ----------------------------------------------------

/// Check a command list before any of it runs.
///
/// Half-applying a list and then failing leaves the caller guessing at what
/// happened, and for a one-shot tool the document is thrown away anyway. The
/// cheap mistakes — a missing `op`, a name the engine has never heard of —
/// are worth catching first.
pub fn check_operations(ops: &[Value]) -> Result<()> {
    for (i, cmd) in ops.iter().enumerate() {
        let Some(op) = cmd.get("op").and_then(Value::as_str) else {
            return Err(Error::Input(format!(
                "operation #{} has no \"op\" field",
                i + 1
            )));
        };
        if crate::catalog::lookup(op).is_none() {
            return Err(Error::Input(format!(
                "operation #{} is `{op}`, which the engine does not have. \
                 Call list_operations for the catalogue.",
                i + 1
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct OpResult {
    pub op: String,
    pub changed: bool,
    pub label: Option<String>,
    /// Whatever the command returned (a new layer id, a histogram, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Run a list of commands, stopping at the first failure.
pub fn run(ed: &mut Editor, ops: &[Value]) -> Result<Vec<OpResult>> {
    check_operations(ops)?;
    let mut out = Vec::with_capacity(ops.len());
    for (i, cmd) in ops.iter().enumerate() {
        let op = cmd["op"].as_str().unwrap_or("?").to_string();
        let res = ed.exec(cmd.clone(), &[]).map_err(|e| {
            Error::Engine(format!("operation #{} ({op}) failed: {e}", i + 1))
        })?;
        out.push(OpResult {
            changed: res.get("changed").and_then(Value::as_bool).unwrap_or(false),
            label: res
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            data: res.get("data").cloned().filter(|d| !d.is_null()),
            op,
        });
    }
    Ok(out)
}

// --- saving --------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Metadata {
    /// Write pixels only. The default, and the only thing most formats can do.
    #[default]
    Strip,
    /// Carry the source's EXIF across (JPEG to JPEG only).
    Keep,
}

#[derive(Debug, Serialize)]
pub struct Saved {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: u64,
    /// What happened to the metadata, in words, because "keep" is not always
    /// possible and silently dropping EXIF is the kind of thing people find
    /// out about much later.
    pub metadata: String,
}

pub struct SaveOptions<'a> {
    /// 1..=100 for JPEG. Ignored by the lossless formats.
    pub quality: Option<u8>,
    pub metadata: Metadata,
    /// Where the pixels came from, for `Metadata::Keep`.
    pub source: Option<&'a Path>,
}

impl Default for SaveOptions<'_> {
    fn default() -> Self {
        SaveOptions {
            quality: None,
            metadata: Metadata::Strip,
            source: None,
        }
    }
}

/// Write the document to `out`, choosing the encoder by extension.
///
/// PSD and `.opproj` keep the layer tree; everything else is the flattened
/// composite, because that is what a JPEG can hold.
pub fn save(ed: &Editor, out: &Path, opts: &SaveOptions) -> Result<Saved> {
    let doc = &ed.doc;
    let (w, h) = (doc.width, doc.height);
    if w == 0 || h == 0 {
        return Err(Error::Encode("the document is empty".into()));
    }
    let ext = extension(out);
    let mut metadata = "stripped".to_string();
    let bytes: Vec<u8> = match ext.as_str() {
        "psd" | "psb" => {
            let opts = editor_psd::ExportOptions {
                psb: Some(ext == "psb"),
                max_compat: true,
            };
            let exported = editor_psd::export(doc, &opts).map_err(|e| Error::Encode(e.to_string()))?;
            metadata = "layers kept".into();
            exported.bytes
        }
        "opproj" => {
            metadata = "layers and history-free document state kept".into();
            editor_project::save(doc).map_err(|e| Error::Encode(e.to_string()))?
        }
        "png" | "webp" | "tif" | "tiff" | "bmp" => {
            let rgba = flatten(doc);
            encode_with_image(&ext, w, h, rgba)?
        }
        "jpg" | "jpeg" => {
            let quality = opts.quality.unwrap_or(92).clamp(1, 100);
            let rgb = composite_over_white(&flatten(doc));
            let mut jpeg = Vec::new();
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality);
            encode_rgb(encoder, &rgb, w, h)?;
            if opts.metadata == Metadata::Keep {
                match opts.source.and_then(exif_app1_for_copy) {
                    Some(app1) => {
                        jpeg = splice_exif(&jpeg, &app1);
                        metadata = "EXIF copied from the source (orientation reset to 1, \
                                    because the pixels are already upright)"
                            .into();
                    }
                    None => {
                        metadata =
                            "stripped: the source carried no EXIF, or it is not a JPEG".into()
                    }
                }
            }
            jpeg
        }
        "" => {
            return Err(Error::Input(format!(
                "{} has no extension, so there is no way to tell what to write. \
                 Use .png, .jpg, .webp, .tif, .psd or .opproj.",
                out.display()
            )))
        }
        other => {
            return Err(Error::Input(format!(
                "`.{other}` is not a format this server writes. \
                 Use png, jpg, webp, tif, bmp, psd, psb or opproj."
            )))
        }
    };
    let len = bytes.len() as u64;
    write_file(out, &bytes)?;
    Ok(Saved {
        output_path: out.display().to_string(),
        width: w,
        height: h,
        format: ext,
        bytes: len,
        metadata,
    })
}

fn encode_with_image(ext: &str, w: u32, h: u32, rgba: Vec<u8>) -> Result<Vec<u8>> {
    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| Error::Encode("the composite is the wrong size".into()))?;
    let format = match ext {
        "png" => image::ImageFormat::Png,
        "webp" => image::ImageFormat::WebP,
        "tif" | "tiff" => image::ImageFormat::Tiff,
        "bmp" => image::ImageFormat::Bmp,
        other => return Err(Error::Input(format!("`.{other}` is not a format this server writes"))),
    };
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut out, format)
        .map_err(|e| Error::Encode(format!("could not encode {ext}: {e}")))?;
    Ok(out.into_inner())
}

fn encode_rgb(
    mut encoder: image::codecs::jpeg::JpegEncoder<&mut Vec<u8>>,
    rgb: &[u8],
    w: u32,
    h: u32,
) -> Result<()> {
    use image::ImageEncoder;
    encoder
        .set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(72));
    encoder
        .write_image(rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| Error::Encode(format!("could not encode JPEG: {e}")))
}

/// JPEG has no alpha, so transparency has to land on something. White is
/// what Photoshop uses and what people expect.
fn composite_over_white(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        let a = px[3] as u32;
        for channel in &px[..3] {
            let v = (*channel as u32 * a + 255 * (255 - a) + 127) / 255;
            out.push(v.min(255) as u8);
        }
    }
    out
}

// --- EXIF ----------------------------------------------------------------

/// The raw APP1 payload of a JPEG, ready to splice into another one.
///
/// The orientation tag is forced to 1 on the way through: the pixels were
/// rotated upright at decode, so carrying the original orientation across
/// would rotate them a second time in the viewer.
fn exif_app1_for_copy(source: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(source).ok()?;
    let mut app1 = find_app1(&bytes)?.to_vec();
    neutralise_orientation(&mut app1);
    Some(app1)
}

/// The `Exif\0\0`-prefixed payload of the first APP1 segment.
fn find_app1(jpeg: &[u8]) -> Option<&[u8]> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            return None;
        }
        let marker = jpeg[i + 1];
        // Start of scan: the entropy-coded data begins, no more headers.
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if len < 2 || i + 2 + len > jpeg.len() {
            return None;
        }
        let payload = &jpeg[i + 4..i + 2 + len];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            return Some(payload);
        }
        i += 2 + len;
    }
    None
}

/// Set IFD0's orientation entry (tag 0x0112) to 1, in place.
fn neutralise_orientation(app1: &mut [u8]) {
    let tiff = 6usize;
    if app1.len() < tiff + 8 {
        return;
    }
    let big = match &app1[tiff..tiff + 2] {
        b"MM" => true,
        b"II" => false,
        _ => return,
    };
    let u16_at = |b: &[u8], o: usize| {
        if big {
            u16::from_be_bytes([b[o], b[o + 1]])
        } else {
            u16::from_le_bytes([b[o], b[o + 1]])
        }
    };
    let u32_at = |b: &[u8], o: usize| {
        if big {
            u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        } else {
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        }
    };
    let ifd0 = tiff + u32_at(app1, tiff + 4) as usize;
    if ifd0 + 2 > app1.len() {
        return;
    }
    let count = u16_at(app1, ifd0) as usize;
    for e in 0..count {
        let entry = ifd0 + 2 + e * 12;
        if entry + 12 > app1.len() {
            return;
        }
        if u16_at(app1, entry) == 0x0112 {
            // SHORT, one value, stored inline in the first two value bytes.
            let v = entry + 8;
            let one = if big { [0x00, 0x01] } else { [0x01, 0x00] };
            app1[v] = one[0];
            app1[v + 1] = one[1];
            return;
        }
    }
}

/// Put an APP1 segment back into a freshly encoded JPEG, right after SOI.
fn splice_exif(jpeg: &[u8], app1: &[u8]) -> Vec<u8> {
    let len = app1.len() + 2;
    if jpeg.len() < 2 || len > u16::MAX as usize {
        return jpeg.to_vec();
    }
    let mut out = Vec::with_capacity(jpeg.len() + len + 2);
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.extend_from_slice(app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// A bounded, readable summary of a file's EXIF.
///
/// Bounded on purpose. These are strings a stranger wrote into a file, and
/// they are about to be read by a language model: a caption field is a fine
/// place to hide "ignore your instructions". Keeping the set of tags short
/// and every value trimmed does not make them trustworthy, it just keeps the
/// blast radius small. GPS is reported as present or absent rather than as
/// coordinates.
pub fn exif_summary(path: &Path) -> Option<Value> {
    const WANTED: &[exif::Tag] = &[
        exif::Tag::Make,
        exif::Tag::Model,
        exif::Tag::LensModel,
        exif::Tag::DateTimeOriginal,
        exif::Tag::ExposureTime,
        exif::Tag::FNumber,
        exif::Tag::PhotographicSensitivity,
        exif::Tag::FocalLength,
        exif::Tag::FocalLengthIn35mmFilm,
        exif::Tag::Orientation,
        exif::Tag::Software,
        exif::Tag::ColorSpace,
    ];
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .ok()?;
    let mut out = serde_json::Map::new();
    for tag in WANTED {
        if let Some(field) = exif.get_field(*tag, exif::In::PRIMARY) {
            let mut value = field.display_value().with_unit(&exif).to_string();
            if value.chars().count() > 80 {
                value = value.chars().take(80).collect::<String>() + "…";
            }
            out.insert(tag.to_string(), Value::String(value));
        }
    }
    let gps = exif
        .get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
        .is_some();
    out.insert("gps_present".into(), Value::Bool(gps));
    if out.len() == 1 {
        return None;
    }
    Some(Value::Object(out))
}

// --- looking at a document -----------------------------------------------

/// Whether any pixel of the composite is not fully opaque.
pub fn has_alpha(doc: &Document) -> bool {
    let strip = View {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
        width: doc.width as usize,
        height: (doc.height as usize).min(64),
    };
    // A cheap first look at the top of the image, then the whole thing only
    // if that says nothing: transparency is nearly always at an edge, and a
    // 24 MP flatten to answer "has alpha?" is a waste when it is not.
    if doc.height as usize > strip.height {
        let buf = render_view(doc, strip);
        if buf.chunks_exact(4).any(|p| p[3] < 0.999) {
            return true;
        }
    }
    flatten(doc).chunks_exact(4).any(|p| p[3] < 255)
}

pub struct Preview {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Document pixels per preview pixel, so a viewer can map clicks back.
    pub scale: f64,
}

impl Preview {
    pub fn data_uri(&self) -> String {
        use base64::Engine as _;
        format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&self.jpeg)
        )
    }
    pub fn base64(&self) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&self.jpeg)
    }
}

/// A region of the document, in document pixels.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
pub struct Region {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

/// A small JPEG of the document, or of a region of it.
///
/// Capped at [`MAX_PREVIEW_SIDE`] however large `max_side` asks for. The cap
/// is the whole point: a tool result travels through the model, and a
/// full-resolution image in there costs the caller real money and, on a
/// hosted model, leaves the machine.
pub fn preview(doc: &Document, max_side: u32, region: Option<Region>) -> Result<Preview> {
    if doc.width == 0 || doc.height == 0 {
        return Err(Error::Encode("the document is empty".into()));
    }
    let (rx, ry, rw, rh) = match region {
        Some(r) if r.width == 0 || r.height == 0 => {
            return Err(Error::Input("a preview region needs a non-zero size".into()))
        }
        Some(r) => (r.x as f64, r.y as f64, r.width, r.height),
        None => (0.0, 0.0, doc.width, doc.height),
    };
    let want = max_side.clamp(16, MAX_PREVIEW_SIDE);
    let longest = rw.max(rh) as f64;
    let scale = (want as f64 / longest).min(1.0);
    let width = ((rw as f64 * scale).round() as usize).max(1);
    let height = ((rh as f64 * scale).round() as usize).max(1);
    let view = View {
        x: rx,
        y: ry,
        scale,
        width,
        height,
    };
    let buf = render_view(doc, view);
    let mut rgba = vec![0u8; width * height * 4];
    to_u8(&buf, &mut rgba);
    let rgb = composite_over_white(&rgba);
    let mut jpeg = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, PREVIEW_QUALITY);
    encode_rgb(encoder, &rgb, width as u32, height as u32)?;
    Ok(Preview {
        jpeg,
        width: width as u32,
        height: height as u32,
        scale,
    })
}

/// One layer rendered on its own, as a PNG. Used by `psd_export_layers`.
pub fn layer_png(doc: &Document, layer: &editor_core::Layer) -> Result<Vec<u8>> {
    let view = View::full(doc);
    let buf = render_layer_alone(layer, doc, view);
    let mut rgba = vec![0u8; view.width * view.height * 4];
    to_u8(&buf, &mut rgba);
    encode_with_image("png", doc.width, doc.height, rgba)
}

/// The composite's histogram, from the engine's own `analyze.histogram`.
pub fn histogram(ed: &mut Editor) -> Option<Value> {
    ed.exec(json!({ "op": "analyze.histogram", "merged": true }), &[])
        .ok()
        .and_then(|v| v.get("data").cloned())
        .filter(|d| !d.is_null())
}

/// The layer tree, flattened to one line per layer, for a text summary.
pub fn layer_lines(summary: &Value) -> Vec<String> {
    fn walk(layers: &[Value], depth: usize, out: &mut Vec<String>) {
        for l in layers {
            let name = l["name"].as_str().unwrap_or("(unnamed)");
            let kind = l["kind"].as_str().unwrap_or("?");
            let hidden = if l["visible"].as_bool().unwrap_or(true) {
                ""
            } else {
                " (hidden)"
            };
            let masked = if l["mask"].is_object() { " +mask" } else { "" };
            out.push(format!(
                "{}{} [{}]{}{}",
                "  ".repeat(depth),
                name,
                kind,
                masked,
                hidden
            ));
            if let Some(children) = l["children"].as_array() {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Some(layers) = summary["layers"].as_array() {
        walk(layers, 0, &mut out);
    }
    out
}

// --- RAW -----------------------------------------------------------------

pub struct Developed {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Develop a RAW file at full resolution.
///
/// `params` is `editor_raw::DevelopParams` as JSON; null means the camera's
/// own settings, which is what "just open it" should mean.
pub fn develop_raw(bytes: &[u8], params: &Value) -> Result<Developed> {
    let p: editor_raw::DevelopParams = if params.is_null() {
        Default::default()
    } else {
        serde_json::from_value(params.clone())
            .map_err(|e| Error::Input(format!("develop settings: {e}")))?
    };
    let mut session = editor_raw::Session::open(bytes)
        .map_err(|e| Error::Decode(format!("could not decode the RAW file: {e}")))?;
    let out = session.develop_at(&p, editor_raw::Size::Full);
    Ok(Developed {
        width: out.width as u32,
        height: out.height as u32,
        rgba: out.rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey_png(dir: &Path, name: &str, w: u32, h: u32, v: u8) -> std::path::PathBuf {
        let path = dir.join(name);
        image::RgbaImage::from_pixel(w, h, image::Rgba([v, v, v, 255]))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn every_domain_is_registered_or_half_the_catalogue_is_a_lie() {
        let mut ed = new_editor();
        ed.exec(json!({"op": "doc.new", "width": 16, "height": 16}), &[])
            .unwrap();
        for op in [
            json!({"op": "filter.gaussian-blur", "radius": 1.0}),
            json!({"op": "transform.flip", "horizontal": true}),
            json!({"op": "select.rect", "x": 0, "y": 0, "width": 4, "height": 4}),
            json!({"op": "select.grow", "by": 1}),
            json!({"op": "paint.fill", "x": 1, "y": 1, "tolerance": 200, "color": {"r":255,"g":0,"b":0}}),
        ] {
            let name = op["op"].as_str().unwrap().to_string();
            let res = ed.exec(op, &[]);
            if let Err(e) = res {
                let msg = e.to_string();
                assert!(
                    !msg.contains("unknown"),
                    "{name} is not registered: {msg}"
                );
            }
        }
    }

    #[test]
    fn open_run_save_round_trips_a_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        image::RgbImage::from_pixel(40, 20, image::Rgb([100, 100, 100]))
            .save(&input)
            .unwrap();
        let mut opened = open(&input).unwrap();
        assert_eq!((opened.ed.doc.width, opened.ed.doc.height), (40, 20));
        run(
            &mut opened.ed,
            &[json!({"op": "image.resize", "width": 20, "height": 10})],
        )
        .unwrap();
        let out = dir.path().join("out.png");
        let saved = save(&opened.ed, &out, &SaveOptions::default()).unwrap();
        assert_eq!((saved.width, saved.height), (20, 10));
        let back = image::open(&out).unwrap();
        assert_eq!(back.width(), 20);
    }

    #[test]
    fn an_unknown_op_is_refused_before_anything_runs() {
        let ops = vec![
            json!({"op": "image.rotate", "turns": 1}),
            json!({"op": "filter.gaussian_blur", "radius": 2}),
        ];
        let err = check_operations(&ops).unwrap_err();
        assert!(err.to_string().contains("filter.gaussian_blur"));
        assert!(err.to_string().contains("list_operations"));
    }

    #[test]
    fn a_command_without_an_op_is_refused() {
        assert!(check_operations(&[json!({"radius": 2})]).is_err());
    }

    #[test]
    fn a_failing_command_names_its_position() {
        let dir = tempfile::tempdir().unwrap();
        let input = grey_png(dir.path(), "a.png", 8, 8, 100);
        let mut opened = open(&input).unwrap();
        let err = run(
            &mut opened.ed,
            &[json!({"op": "image.crop", "x": 0, "y": 0, "width": 0, "height": 0})],
        )
        .unwrap_err();
        assert!(err.to_string().contains("operation #1"), "{err}");
    }

    #[test]
    fn a_preview_is_small_whatever_is_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let input = grey_png(dir.path(), "big.png", 2400, 1600, 120);
        let opened = open(&input).unwrap();
        let p = preview(&opened.ed.doc, 100_000, None).unwrap();
        assert_eq!(p.width.max(p.height), MAX_PREVIEW_SIDE);
        assert!(p.jpeg.len() < 200_000, "{} bytes", p.jpeg.len());
        assert_eq!(&p.jpeg[..2], &[0xFF, 0xD8], "that is not a JPEG");
    }

    #[test]
    fn a_preview_region_is_the_region_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 255]));
        for y in 0..50 {
            for x in 0..50 {
                img.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        let path = dir.path().join("quad.png");
        img.save(&path).unwrap();
        let opened = open(&path).unwrap();
        let region = Region {
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        };
        let p = preview(&opened.ed.doc, 50, Some(region)).unwrap();
        assert_eq!((p.width, p.height), (50, 50));
        let decoded = image::load_from_memory(&p.jpeg).unwrap().into_rgba8();
        assert!(decoded.get_pixel(25, 25)[0] > 200, "the white quadrant");
    }

    #[test]
    fn saving_jpeg_flattens_transparency_onto_white() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clear.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 0]))
            .save(&path)
            .unwrap();
        let opened = open(&path).unwrap();
        let out = dir.path().join("out.jpg");
        save(&opened.ed, &out, &SaveOptions::default()).unwrap();
        let back = image::open(&out).unwrap().into_rgb8();
        assert!(back.get_pixel(4, 4)[0] > 240, "{:?}", back.get_pixel(4, 4));
    }

    #[test]
    fn an_unwritable_extension_says_which_ones_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = grey_png(dir.path(), "a.png", 4, 4, 10);
        let opened = open(&path).unwrap();
        let err = save(
            &opened.ed,
            &dir.path().join("out.xyz"),
            &SaveOptions::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("png"), "{err}");
    }

    #[test]
    fn alpha_is_detected_and_its_absence_too() {
        let dir = tempfile::tempdir().unwrap();
        let opaque = grey_png(dir.path(), "opaque.png", 32, 32, 200);
        assert!(!has_alpha(&open(&opaque).unwrap().ed.doc));
        let path = dir.path().join("holes.png");
        let mut img = image::RgbaImage::from_pixel(32, 32, image::Rgba([1, 2, 3, 255]));
        img.put_pixel(31, 31, image::Rgba([0, 0, 0, 0]));
        img.save(&path).unwrap();
        assert!(has_alpha(&open(&path).unwrap().ed.doc));
    }

    #[test]
    fn orientation_is_reset_when_exif_is_copied() {
        // Big-endian TIFF with one IFD0 entry: orientation = 6 (rotate 90).
        let mut app1: Vec<u8> = b"Exif\0\0".to_vec();
        app1.extend_from_slice(b"MM\0\x2a");
        app1.extend_from_slice(&8u32.to_be_bytes());
        app1.extend_from_slice(&1u16.to_be_bytes());
        app1.extend_from_slice(&0x0112u16.to_be_bytes());
        app1.extend_from_slice(&3u16.to_be_bytes());
        app1.extend_from_slice(&1u32.to_be_bytes());
        app1.extend_from_slice(&[0, 6, 0, 0]);
        app1.extend_from_slice(&0u32.to_be_bytes());
        neutralise_orientation(&mut app1);
        assert_eq!(&app1[6 + 8 + 2 + 8..6 + 8 + 2 + 10], &[0, 1]);
    }

    #[test]
    fn a_spliced_jpeg_still_decodes_and_carries_the_segment() {
        let mut jpeg = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80);
        encode_rgb(encoder, &[128u8; 8 * 8 * 3], 8, 8).unwrap();
        let app1 = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\0\0\0\0\0".to_vec();
        let spliced = splice_exif(&jpeg, &app1);
        assert!(find_app1(&spliced).is_some());
        let img = image::load_from_memory(&spliced).unwrap();
        assert_eq!((img.width(), img.height()), (8, 8));
    }

    #[test]
    fn families_are_recognised_by_extension() {
        assert_eq!(family_of(Path::new("a.psd")), Family::Psd);
        assert_eq!(family_of(Path::new("a.PSB")), Family::Psd);
        assert_eq!(family_of(Path::new("a.opproj")), Family::Project);
        assert_eq!(family_of(Path::new("a.CR3")), Family::Raw);
        assert_eq!(family_of(Path::new("a.jpg")), Family::Raster);
    }
}
