//! The photo operations, as plain functions over paths.
//!
//! Every one of them is the same three steps the engine has always taken:
//! open the file as a document, run JSON commands on it, write the result
//! out. The convenience tools differ from `run_operations` only in that they
//! build the command list themselves, which is worth doing because "resize
//! this to 1600 wide" should not require an agent to have read
//! `docs/commands.md` first.
//!
//! Nothing here returns pixels. The callers in `server.rs` add a preview
//! when, and only when, someone asked for one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine::{self, Error, Metadata, OpResult, Result, SaveOptions};
use crate::sandbox::Sandbox;

// --- image_info ----------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ImageInfo {
    pub path: String,
    pub format: String,
    pub family: engine::Family,
    pub width: u32,
    pub height: u32,
    pub megapixels: f64,
    pub resolution: f32,
    pub color_mode: String,
    pub has_alpha: bool,
    pub file_bytes: u64,
    pub layer_count: usize,
    /// The layer tree, one indented line per layer. Present for PSD and
    /// `.opproj`; a flat photograph has one layer and nothing to show.
    pub layers: Vec<String>,
    /// The full tree, for a caller that wants ids and blend modes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_tree: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exif: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub histogram: Option<Value>,
    pub warnings: Vec<String>,
}

/// Everything an agent should know before it decides what to do.
pub fn info(path: &Path) -> Result<(ImageInfo, editor_core::Editor)> {
    let mut opened = engine::open(path)?;
    let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let summary = opened.ed.summary();
    let layered = matches!(opened.family, engine::Family::Psd | engine::Family::Project);
    let (w, h) = (opened.ed.doc.width, opened.ed.doc.height);
    let info = ImageInfo {
        path: path.display().to_string(),
        format: opened.format.clone(),
        family: opened.family,
        width: w,
        height: h,
        megapixels: (w as f64 * h as f64 / 1_000_000.0 * 100.0).round() / 100.0,
        resolution: opened.ed.doc.resolution,
        color_mode: opened.color_mode.clone(),
        has_alpha: engine::has_alpha(&opened.ed.doc),
        file_bytes,
        layer_count: opened.ed.doc.layer_count(),
        layers: engine::layer_lines(&summary),
        layer_tree: layered.then(|| summary["layers"].clone()),
        exif: engine::exif_summary(path),
        histogram: engine::histogram(&mut opened.ed),
        warnings: opened.warnings.clone(),
    };
    Ok((info, opened.ed))
}

// --- the shared edit pipeline --------------------------------------------

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub input_path: String,
    pub source_width: u32,
    pub source_height: u32,
    pub source_format: String,
    #[serde(flatten)]
    pub saved: engine::Saved,
    pub operations: Vec<OpResult>,
    pub layer_count: usize,
    pub warnings: Vec<String>,
}

pub struct EditRequest<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub operations: Vec<Value>,
    pub quality: Option<u8>,
    pub metadata: Metadata,
}

/// Open, run, save. Every convenience tool is this with a different list.
pub fn edit(req: EditRequest) -> Result<(Outcome, editor_core::Editor)> {
    engine::check_operations(&req.operations)?;
    let mut opened = engine::open(req.input)?;
    let (sw, sh) = (opened.ed.doc.width, opened.ed.doc.height);
    let operations = engine::run(&mut opened.ed, &req.operations)?;
    let saved = engine::save(
        &opened.ed,
        req.output,
        &SaveOptions {
            quality: req.quality,
            metadata: req.metadata,
            source: Some(req.input),
        },
    )?;
    let outcome = Outcome {
        input_path: req.input.display().to_string(),
        source_width: sw,
        source_height: sh,
        source_format: opened.format.clone(),
        layer_count: opened.ed.doc.layer_count(),
        warnings: opened.warnings.clone(),
        saved,
        operations,
    };
    Ok((outcome, opened.ed))
}

// --- the convenience command builders ------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct ResizeSpec {
    /// Target width in pixels. With only one of width and height, the other
    /// follows the aspect ratio.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Fit inside width × height instead of stretching to exactly it.
    pub fit: Option<bool>,
    /// `nearest`, `bilinear`, `bicubic` or `lanczos`. Lanczos by default,
    /// which is the right answer for photographs.
    pub resample: Option<String>,
}

pub fn resize_ops(spec: &ResizeSpec, w: u32, h: u32) -> Result<Vec<Value>> {
    if w == 0 || h == 0 {
        return Err(Error::Input("the source has no pixels".into()));
    }
    let aspect = w as f64 / h as f64;
    let (tw, th) = match (spec.width, spec.height) {
        (None, None) => {
            return Err(Error::Input(
                "give a width, a height, or both".into(),
            ))
        }
        (Some(tw), None) => (tw, (tw as f64 / aspect).round().max(1.0) as u32),
        (None, Some(th)) => ((th as f64 * aspect).round().max(1.0) as u32, th),
        (Some(tw), Some(th)) if spec.fit.unwrap_or(false) => {
            let s = (tw as f64 / w as f64).min(th as f64 / h as f64);
            (
                (w as f64 * s).round().max(1.0) as u32,
                (h as f64 * s).round().max(1.0) as u32,
            )
        }
        (Some(tw), Some(th)) => (tw, th),
    };
    if tw == 0 || th == 0 {
        return Err(Error::Input("a resize to zero pixels is not a resize".into()));
    }
    let resample = spec.resample.clone().unwrap_or_else(|| "lanczos".into());
    if !["nearest", "bilinear", "bicubic", "lanczos"].contains(&resample.as_str()) {
        return Err(Error::Input(format!(
            "`{resample}` is not a resampling method — use nearest, bilinear, bicubic or lanczos"
        )));
    }
    Ok(vec![
        json!({ "op": "image.resize", "width": tw, "height": th, "resample": resample }),
    ])
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CropSpec {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn crop_ops(spec: &CropSpec, w: u32, h: u32) -> Result<Vec<Value>> {
    if spec.width == 0 || spec.height == 0 {
        return Err(Error::Input("a crop needs a non-zero width and height".into()));
    }
    // The engine would clamp, but an agent that asked for a rectangle off the
    // edge has miscounted something and should hear about it.
    if spec.x < 0 || spec.y < 0 {
        return Err(Error::Input(
            "a crop rectangle starts inside the image; x and y cannot be negative".into(),
        ));
    }
    let (right, bottom) = (
        spec.x as i64 + spec.width as i64,
        spec.y as i64 + spec.height as i64,
    );
    if right > w as i64 || bottom > h as i64 {
        return Err(Error::Input(format!(
            "the crop rectangle reaches {right}×{bottom}, outside the {w}×{h} image"
        )));
    }
    Ok(vec![json!({
        "op": "image.crop",
        "x": spec.x, "y": spec.y, "width": spec.width, "height": spec.height
    })])
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct RotateFlipSpec {
    /// Clockwise degrees. A multiple of 90 is exact and lossless; anything
    /// else resamples.
    pub rotate: Option<f64>,
    /// Keep the original canvas size when rotating by an odd angle
    /// (straightening rather than rotating).
    pub expand: Option<bool>,
    pub flip_horizontal: Option<bool>,
    pub flip_vertical: Option<bool>,
}

pub fn rotate_flip_ops(spec: &RotateFlipSpec) -> Result<Vec<Value>> {
    let mut ops = Vec::new();
    if let Some(deg) = spec.rotate.filter(|d| *d != 0.0) {
        let normalised = deg.rem_euclid(360.0);
        if (normalised / 90.0).fract().abs() < 1e-9 {
            let turns = (normalised / 90.0).round() as i64;
            if turns != 0 {
                ops.push(json!({ "op": "image.rotate", "turns": turns }));
            }
        } else {
            ops.push(json!({
                "op": "image.rotate-arbitrary",
                "degrees": deg,
                "expand": spec.expand.unwrap_or(true)
            }));
        }
    }
    if spec.flip_horizontal.unwrap_or(false) {
        ops.push(json!({ "op": "image.flip", "horizontal": true }));
    }
    if spec.flip_vertical.unwrap_or(false) {
        ops.push(json!({ "op": "image.flip", "horizontal": false }));
    }
    if ops.is_empty() {
        return Err(Error::Input(
            "nothing to do: give a rotation, or flip_horizontal, or flip_vertical".into(),
        ));
    }
    Ok(ops)
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct AdjustSpec {
    /// Stops, -5..5.
    pub exposure: Option<f32>,
    /// -100..100.
    pub contrast: Option<f32>,
    pub saturation: Option<f32>,
    pub vibrance: Option<f32>,
    /// Warmer above zero, cooler below. -100..100.
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub highlights: Option<f32>,
    pub shadows: Option<f32>,
    pub whites: Option<f32>,
    pub blacks: Option<f32>,
    pub clarity: Option<f32>,
    pub dehaze: Option<f32>,
    /// `{master: {in_black, in_white, gamma, out_black, out_white}, red, green, blue}`.
    /// Any channel may be left out.
    pub levels: Option<Value>,
    /// `{master: [[0,0],[255,255]], red, green, blue}` — control points in
    /// 0..255, through which a monotone curve is fitted.
    pub curves: Option<Value>,
}

/// Build the adjustment layers for an `adjust_image` call.
///
/// Adjustment *layers*, not destructive edits: saving the result as `.psd`
/// or `.opproj` then keeps them live and re-editable, and flattening to a
/// JPEG gives the same pixels either way.
pub fn adjust_ops(spec: &AdjustSpec) -> Result<Vec<Value>> {
    let mut ops = Vec::new();
    let mut develop = serde_json::Map::new();
    let mut put = |k: &str, v: Option<f32>| {
        if let Some(v) = v {
            develop.insert(k.into(), json!(v));
        }
    };
    put("exposure", spec.exposure);
    put("contrast", spec.contrast);
    put("saturation", spec.saturation);
    put("vibrance", spec.vibrance);
    put("temperature", spec.temperature);
    put("tint", spec.tint);
    put("highlights", spec.highlights);
    put("shadows", spec.shadows);
    put("whites", spec.whites);
    put("blacks", spec.blacks);
    put("clarity", spec.clarity);
    put("dehaze", spec.dehaze);
    if !develop.is_empty() {
        develop.insert("kind".into(), json!("develop"));
        ops.push(json!({
            "op": "layer.add-adjustment",
            "name": "Develop",
            "adjustment": Value::Object(develop)
        }));
    }
    if let Some(levels) = &spec.levels {
        let mut a = levels
            .as_object()
            .cloned()
            .ok_or_else(|| Error::Input("levels must be an object".into()))?;
        a.insert("kind".into(), json!("levels"));
        ops.push(json!({ "op": "layer.add-adjustment", "name": "Levels", "adjustment": a }));
    }
    if let Some(curves) = &spec.curves {
        let mut a = curves
            .as_object()
            .cloned()
            .ok_or_else(|| Error::Input("curves must be an object".into()))?;
        a.insert("kind".into(), json!("curves"));
        ops.push(json!({ "op": "layer.add-adjustment", "name": "Curves", "adjustment": a }));
    }
    if ops.is_empty() {
        return Err(Error::Input(
            "nothing to adjust: give at least one of exposure, contrast, saturation, \
             temperature, levels, curves and the rest"
                .into(),
        ));
    }
    Ok(ops)
}

/// Build the command for `filter_image`.
///
/// The name may be given with or without its `filter.` prefix, because both
/// are what someone would type.
pub fn filter_op(name: &str, params: Option<&Value>) -> Result<Vec<Value>> {
    let op = if name.contains('.') {
        name.to_string()
    } else {
        format!("filter.{name}")
    };
    if crate::catalog::lookup(&op).is_none() {
        let near: Vec<&str> = crate::catalog::filtered(Some("filter"))
            .iter()
            .map(|o| o.op)
            .collect();
        return Err(Error::Input(format!(
            "`{op}` is not a filter this engine has. Available: {}",
            near.join(", ")
        )));
    }
    if !op.starts_with("filter.") && !op.starts_with("analyze.") {
        return Err(Error::Input(format!(
            "`{op}` is not a filter — use run_operations for other domains"
        )));
    }
    let mut cmd = match params {
        Some(Value::Object(map)) => map.clone(),
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(_) => return Err(Error::Input("filter params must be an object".into())),
    };
    cmd.insert("op".into(), json!(op));
    Ok(vec![Value::Object(cmd)])
}

// --- PSD -----------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ExportedLayer {
    pub index: usize,
    pub name: String,
    pub kind: String,
    pub output_path: String,
    pub bytes: u64,
}

/// Write each top-level layer of a layered document to its own PNG.
///
/// Each layer is rendered at document size on transparency, with its own
/// blend mode and opacity ignored, so the files line up when stacked and a
/// layer that was set to Multiply does not come out dark on its own.
pub fn export_layers(
    input: &Path,
    out_dir: &Path,
    prefix: &str,
    overwrite: bool,
    sandbox: &Sandbox,
) -> Result<(Vec<ExportedLayer>, Vec<String>)> {
    let opened = engine::open(input)?;
    let doc = &opened.ed.doc;
    if doc.layers.is_empty() {
        return Err(Error::Input("this document has no layers".into()));
    }
    let mut written = Vec::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        let name = sanitise(&layer.name);
        let file = out_dir.join(format!("{prefix}{:02}-{name}.png", i + 1));
        let target = sandbox
            .write_path(&file.display().to_string(), overwrite)
            .map_err(|e| Error::Input(e.to_string()))?;
        let png = engine::layer_png(doc, layer)?;
        let bytes = png.len() as u64;
        std::fs::write(&target, &png).map_err(|source| Error::Write {
            path: target.display().to_string(),
            source,
        })?;
        written.push(ExportedLayer {
            index: i,
            name: layer.name.clone(),
            kind: layer.kind.name().to_string(),
            output_path: target.display().to_string(),
            bytes,
        });
    }
    Ok((written, opened.warnings))
}

/// A layer name that is safe as a file name on every platform we ship to.
fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    let short: String = trimmed.chars().take(48).collect();
    if short.is_empty() {
        "layer".into()
    } else {
        short
    }
}

// --- RAW -----------------------------------------------------------------

/// Develop a RAW file and write it out.
///
/// Separate from `convert_image` because the settings are different in kind:
/// a RAW file has no pixels until somebody decides on a white balance.
pub fn raw_develop(
    input: &Path,
    output: &Path,
    params: &Value,
    quality: Option<u8>,
) -> Result<(Outcome, editor_core::Editor)> {
    if engine::family_of(input) != engine::Family::Raw {
        return Err(Error::Input(format!(
            "{} is not a camera RAW file. Use convert_image for ordinary images.",
            input.display()
        )));
    }
    let bytes = engine::read_file(input)?;
    let developed = engine::develop_raw(&bytes, params)?;
    let mut ed = engine::new_editor();
    let name = input.file_name().map(|n| n.to_string_lossy().into_owned());
    ed.exec(
        json!({ "op": "doc.open-pixels", "width": developed.width, "height": developed.height, "name": name }),
        &developed.rgba,
    )
    .map_err(|e| Error::Engine(e.to_string()))?;
    let saved = engine::save(
        &ed,
        output,
        &SaveOptions {
            quality,
            metadata: Metadata::Strip,
            source: Some(input),
        },
    )?;
    Ok((
        Outcome {
            input_path: input.display().to_string(),
            source_width: developed.width,
            source_height: developed.height,
            source_format: engine::extension(input),
            layer_count: ed.doc.layer_count(),
            warnings: Vec::new(),
            saved,
            operations: Vec::new(),
        },
        ed,
    ))
}

/// What the camera recorded, without decoding the sensor data.
pub fn raw_probe(input: &Path) -> Result<Value> {
    let bytes = engine::read_file(input)?;
    let info = editor_raw::probe(&bytes)
        .map_err(|e| Error::Decode(format!("{}: {e}", input.display())))?;
    serde_json::to_value(info).map_err(|e| Error::Decode(e.to_string()))
}

// --- batch ---------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct BatchFile {
    pub input_path: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchReport {
    pub pattern: String,
    pub matched: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub files: Vec<BatchFile>,
}

pub struct BatchRequest<'a> {
    pub pattern: &'a str,
    pub out_dir: &'a Path,
    pub operations: Vec<Value>,
    /// Added before the extension, so `sunset.jpg` becomes `sunset-web.jpg`.
    pub suffix: &'a str,
    /// Change the extension too, e.g. "webp". Empty keeps the source's.
    pub format: Option<&'a str>,
    pub quality: Option<u8>,
    pub metadata: Metadata,
    pub overwrite: bool,
    pub limit: usize,
}

/// Run one operation list over everything a glob matches.
///
/// One file's failure is reported and the rest carry on: a batch that stops
/// dead on the seventh of two hundred photographs, having written six, is
/// worse than useless — nobody knows where it got to.
pub fn batch(req: BatchRequest, sandbox: &Sandbox) -> Result<BatchReport> {
    engine::check_operations(&req.operations)?;
    let matches = glob::glob(req.pattern)
        .map_err(|e| Error::Input(format!("`{}` is not a valid glob: {e}", req.pattern)))?;
    let mut inputs: Vec<PathBuf> = Vec::new();
    for entry in matches {
        let Ok(path) = entry else { continue };
        if !path.is_file() {
            continue;
        }
        // Every match goes through the sandbox: a glob is a path expression
        // like any other, and `**` is a fine way to walk out of a root.
        match sandbox.read_path(&path.display().to_string()) {
            Ok(real) => inputs.push(real),
            Err(_) => continue,
        }
    }
    inputs.sort();
    inputs.dedup();
    if inputs.is_empty() {
        return Err(Error::Input(format!(
            "`{}` matched no readable file inside the allowed roots ({})",
            req.pattern,
            sandbox.root_names().join(", ")
        )));
    }
    if inputs.len() > req.limit {
        return Err(Error::Input(format!(
            "`{}` matched {} files, over the limit of {}. Narrow the pattern, \
             or raise `limit` if you meant it.",
            req.pattern,
            inputs.len(),
            req.limit
        )));
    }

    let mut files = Vec::new();
    for input in &inputs {
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".into());
        let ext = match req.format {
            Some(f) if !f.is_empty() => f.trim_start_matches('.').to_ascii_lowercase(),
            _ => engine::extension(input),
        };
        let out = req.out_dir.join(format!("{stem}{}.{ext}", req.suffix));
        let result = (|| -> Result<engine::Saved> {
            let target = sandbox
                .write_path(&out.display().to_string(), req.overwrite)
                .map_err(|e| Error::Input(e.to_string()))?;
            if target == *input {
                return Err(Error::Input(
                    "that would write over the source; give a suffix or another directory".into(),
                ));
            }
            let (outcome, _) = edit(EditRequest {
                input,
                output: &target,
                operations: req.operations.clone(),
                quality: req.quality,
                metadata: req.metadata,
            })?;
            Ok(outcome.saved)
        })();
        match result {
            Ok(saved) => files.push(BatchFile {
                input_path: input.display().to_string(),
                ok: true,
                output_path: Some(saved.output_path),
                width: Some(saved.width),
                height: Some(saved.height),
                error: None,
            }),
            Err(e) => files.push(BatchFile {
                input_path: input.display().to_string(),
                ok: false,
                output_path: None,
                width: None,
                height: None,
                error: Some(e.to_string()),
            }),
        }
    }
    let succeeded = files.iter().filter(|f| f.ok).count();
    Ok(BatchReport {
        pattern: req.pattern.to_string(),
        matched: files.len(),
        succeeded,
        failed: files.len() - succeeded,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn photo(at: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = at.join(name);
        let mut img = image::RgbaImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]);
        }
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn info_reads_a_photograph() {
        let d = dir();
        let p = photo(d.path(), "a.png", 64, 32);
        let (i, _) = info(&p).unwrap();
        assert_eq!((i.width, i.height), (64, 32));
        assert_eq!(i.layer_count, 1);
        assert!(!i.has_alpha);
        assert!(i.histogram.is_some());
        assert!(i.file_bytes > 0);
    }

    #[test]
    fn info_on_something_that_is_not_an_image_fails_clearly() {
        let d = dir();
        let p = d.path().join("notes.jpg");
        std::fs::write(&p, b"dear diary").unwrap();
        let err = match info(&p) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a text file is not an image"),
        };
        assert!(err.contains("notes.jpg"), "{err}");
    }

    #[test]
    fn resize_keeps_the_aspect_ratio_when_given_one_side() {
        let ops = resize_ops(
            &ResizeSpec {
                width: Some(50),
                ..Default::default()
            },
            100,
            40,
        )
        .unwrap();
        assert_eq!(ops[0]["width"], 50);
        assert_eq!(ops[0]["height"], 20);
    }

    #[test]
    fn resize_fit_stays_inside_the_box() {
        let ops = resize_ops(
            &ResizeSpec {
                width: Some(100),
                height: Some(100),
                fit: Some(true),
                resample: None,
            },
            200,
            50,
        )
        .unwrap();
        assert_eq!((ops[0]["width"].as_u64(), ops[0]["height"].as_u64()), (Some(100), Some(25)));
    }

    #[test]
    fn resize_with_no_target_is_refused() {
        assert!(resize_ops(&ResizeSpec::default(), 10, 10).is_err());
        assert!(resize_ops(
            &ResizeSpec { width: Some(10), resample: Some("magic".into()), ..Default::default() },
            10,
            10
        )
        .is_err());
    }

    #[test]
    fn a_crop_outside_the_image_is_refused_rather_than_clamped() {
        let spec = CropSpec { x: 10, y: 10, width: 200, height: 10 };
        let err = crop_ops(&spec, 100, 100).unwrap_err().to_string();
        assert!(err.contains("outside"), "{err}");
        assert!(crop_ops(&CropSpec { x: 0, y: 0, width: 100, height: 100 }, 100, 100).is_ok());
    }

    #[test]
    fn rotation_by_a_quarter_turn_is_exact_and_other_angles_are_not() {
        let quarter = rotate_flip_ops(&RotateFlipSpec { rotate: Some(90.0), ..Default::default() }).unwrap();
        assert_eq!(quarter[0]["op"], "image.rotate");
        assert_eq!(quarter[0]["turns"], 1);
        let tilt = rotate_flip_ops(&RotateFlipSpec { rotate: Some(2.5), ..Default::default() }).unwrap();
        assert_eq!(tilt[0]["op"], "image.rotate-arbitrary");
        assert!(rotate_flip_ops(&RotateFlipSpec::default()).is_err());
    }

    #[test]
    fn negative_rotation_normalises() {
        let ops = rotate_flip_ops(&RotateFlipSpec { rotate: Some(-90.0), ..Default::default() }).unwrap();
        assert_eq!(ops[0]["turns"], 3);
    }

    #[test]
    fn adjustments_become_adjustment_layers() {
        let ops = adjust_ops(&AdjustSpec {
            exposure: Some(0.5),
            contrast: Some(10.0),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "layer.add-adjustment");
        assert_eq!(ops[0]["adjustment"]["kind"], "develop");
        assert_eq!(ops[0]["adjustment"]["exposure"], 0.5);
        assert!(adjust_ops(&AdjustSpec::default()).is_err());
    }

    #[test]
    fn levels_and_curves_each_get_their_own_layer() {
        let ops = adjust_ops(&AdjustSpec {
            exposure: Some(0.2),
            levels: Some(json!({"master": {"in_black": 10.0}})),
            curves: Some(json!({"master": [[0.0, 0.0], [128.0, 160.0], [255.0, 255.0]]})),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[1]["adjustment"]["kind"], "levels");
        assert_eq!(ops[2]["adjustment"]["kind"], "curves");
    }

    #[test]
    fn a_filter_name_works_with_or_without_its_prefix() {
        let a = filter_op("gaussian-blur", Some(&json!({"radius": 2.0}))).unwrap();
        let b = filter_op("filter.gaussian-blur", Some(&json!({"radius": 2.0}))).unwrap();
        assert_eq!(a, b);
        assert_eq!(a[0]["radius"], 2.0);
    }

    #[test]
    fn an_unknown_filter_lists_the_real_ones() {
        let err = filter_op("deep-fry", None).unwrap_err().to_string();
        assert!(err.contains("filter.gaussian-blur"), "{err}");
    }

    #[test]
    fn the_whole_pipeline_writes_the_file_it_says_it_did() {
        let d = dir();
        let input = photo(d.path(), "in.png", 80, 60);
        let out = d.path().join("out.jpg");
        let (outcome, _) = edit(EditRequest {
            input: &input,
            output: &out,
            operations: resize_ops(&ResizeSpec { width: Some(40), ..Default::default() }, 80, 60).unwrap(),
            quality: Some(80),
            metadata: Metadata::Strip,
        })
        .unwrap();
        assert_eq!((outcome.saved.width, outcome.saved.height), (40, 30));
        let back = image::open(&out).unwrap();
        assert_eq!((back.width(), back.height()), (40, 30));
        assert_eq!(outcome.saved.bytes, std::fs::metadata(&out).unwrap().len());
    }

    #[test]
    fn a_filter_actually_changes_the_pixels() {
        // A hard edge, not a gradient: blurring a linear ramp leaves its
        // middle exactly where it was, and the test would pass for the wrong
        // reason (or fail for a good one).
        let d = dir();
        let input = d.path().join("edge.png");
        let mut img = image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 255]));
        for y in 0..64 {
            for x in 32..64 {
                img.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        img.save(&input).unwrap();
        let out = d.path().join("blurred.png");
        edit(EditRequest {
            input: &input,
            output: &out,
            operations: filter_op("gaussian-blur", Some(&json!({"radius": 4.0}))).unwrap(),
            quality: None,
            metadata: Metadata::Strip,
        })
        .unwrap();
        let after = image::open(&out).unwrap().into_rgba8();
        let edge = after.get_pixel(30, 32)[0];
        assert!(
            edge > 5 && edge < 250,
            "the edge should have softened, got {edge}"
        );
        assert_eq!(after.get_pixel(2, 2)[0], 0, "far from the edge, nothing moves");
    }

    #[test]
    fn exposure_brightens() {
        let d = dir();
        let input = d.path().join("grey.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([100, 100, 100, 255]))
            .save(&input)
            .unwrap();
        let out = d.path().join("bright.png");
        edit(EditRequest {
            input: &input,
            output: &out,
            operations: adjust_ops(&AdjustSpec { exposure: Some(1.0), ..Default::default() }).unwrap(),
            quality: None,
            metadata: Metadata::Strip,
        })
        .unwrap();
        let after = image::open(&out).unwrap().into_rgba8();
        assert!(after.get_pixel(8, 8)[0] > 130, "{:?}", after.get_pixel(8, 8));
    }

    #[test]
    fn layer_names_become_safe_file_names() {
        assert_eq!(sanitise("Background copy 2"), "Background-copy-2");
        assert_eq!(sanitise("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitise("///"), "layer");
        assert!(sanitise(&"x".repeat(200)).len() <= 48);
    }

    #[test]
    fn a_batch_reports_every_file_and_keeps_going_past_a_bad_one() {
        let d = dir();
        let src = d.path().join("src");
        let out = d.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        photo(&src, "one.png", 40, 40);
        photo(&src, "two.png", 60, 20);
        std::fs::write(src.join("three.png"), b"not a png at all").unwrap();

        let sandbox = Sandbox::new(vec![d.path().to_path_buf()]).unwrap();
        let report = batch(
            BatchRequest {
                pattern: &format!("{}/*.png", src.display()),
                out_dir: &out,
                operations: resize_ops(&ResizeSpec { width: Some(20), ..Default::default() }, 40, 40).unwrap(),
                suffix: "-small",
                format: Some("jpg"),
                quality: Some(70),
                metadata: Metadata::Strip,
                overwrite: false,
                limit: 100,
            },
            &sandbox,
        )
        .unwrap();
        assert_eq!(report.matched, 3);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 1);
        assert!(out.join("one-small.jpg").exists());
        let bad = report.files.iter().find(|f| !f.ok).unwrap();
        assert!(bad.error.is_some());
    }

    #[test]
    fn a_batch_that_matches_nothing_says_so() {
        let d = dir();
        let sandbox = Sandbox::new(vec![d.path().to_path_buf()]).unwrap();
        let err = batch(
            BatchRequest {
                pattern: &format!("{}/*.tif", d.path().display()),
                out_dir: d.path(),
                operations: vec![],
                suffix: "-x",
                format: None,
                quality: None,
                metadata: Metadata::Strip,
                overwrite: false,
                limit: 10,
            },
            &sandbox,
        )
        .unwrap_err();
        assert!(err.to_string().contains("matched no readable file"));
    }

    #[test]
    fn a_batch_will_not_write_over_its_own_source() {
        let d = dir();
        photo(d.path(), "a.png", 20, 20);
        let sandbox = Sandbox::new(vec![d.path().to_path_buf()]).unwrap();
        let report = batch(
            BatchRequest {
                pattern: &format!("{}/*.png", d.path().display()),
                out_dir: d.path(),
                operations: vec![],
                suffix: "",
                format: None,
                quality: None,
                metadata: Metadata::Strip,
                overwrite: true,
                limit: 10,
            },
            &sandbox,
        )
        .unwrap();
        assert_eq!(report.failed, 1);
        assert!(report.files[0].error.as_ref().unwrap().contains("source"));
    }

    #[test]
    fn a_batch_glob_cannot_reach_outside_the_roots() {
        let outer = dir();
        let root = outer.path().join("inside");
        std::fs::create_dir(&root).unwrap();
        photo(outer.path(), "secret.png", 10, 10);
        photo(&root, "fine.png", 10, 10);
        let sandbox = Sandbox::new(vec![root.clone()]).unwrap();
        let report = batch(
            BatchRequest {
                pattern: &format!("{}/**/*.png", outer.path().display()),
                out_dir: &root,
                operations: vec![],
                suffix: "-copy",
                format: None,
                quality: None,
                metadata: Metadata::Strip,
                overwrite: false,
                limit: 10,
            },
            &sandbox,
        )
        .unwrap();
        assert_eq!(report.matched, 1, "only the file inside the root");
        assert!(report.files[0].input_path.contains("fine.png"));
    }

    #[test]
    fn raw_develop_refuses_an_ordinary_photograph() {
        let d = dir();
        let p = photo(d.path(), "a.png", 8, 8);
        let err = match raw_develop(&p, &d.path().join("o.png"), &Value::Null, None) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a PNG is not a RAW file"),
        };
        assert!(err.contains("convert_image"), "{err}");
    }
}
