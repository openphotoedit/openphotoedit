//! `ai` entry points: the wasm surface of `editor-ai`, called by the worker's
//! AI jobs (`apps/web/src/engine/jobs/ai.ts`). See docs/architecture.md for
//! who owns this file.
//!
//! These are free functions and two small stateful helpers rather than
//! `Engine` methods: they are pure pixel maths around a model's forward pass
//! and never touch the document. Results reach the document through ordinary
//! commands (`layer.set-pixels`, `select.mask`, …) so every AI step is one
//! undo step with provenance.

use editor_ai::{blur, canary, color, face, inpaint, matte, plan, sam, scene, tile, Rgb};
use editor_core::transform::{resize_raw, Resample};
use wasm_bindgen::prelude::*;

fn rgb(rgba: &[u8], w: u32, h: u32) -> Result<Rgb, JsError> {
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return Err(JsError::new(&format!("expected {w}×{h} RGBA ({} bytes), got {}", w as usize * h as usize * 4, rgba.len())));
    }
    Ok(Rgb::from_rgba(w, h, rgba))
}

fn alpha_of(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).map(|p| p[3]).collect()
}

fn resample(name: &str) -> Resample {
    match name {
        "nearest" => Resample::Nearest,
        "bilinear" => Resample::Bilinear,
        "lanczos" => Resample::Lanczos,
        _ => Resample::Bicubic,
    }
}

/// Resize straight RGBA.
#[wasm_bindgen]
pub fn ai_resize_rgba(rgba: &[u8], w: u32, h: u32, dw: u32, dh: u32, method: &str) -> Vec<u8> {
    resize_raw(rgba, w as usize, h as usize, 4, dw as usize, dh as usize, resample(method))
}

// ---------------------------------------------------------------- tiling

/// Super-resolution over constant-square tiles.
#[wasm_bindgen]
pub struct AiUpscaler {
    img: Rgb,
    alpha: Vec<u8>,
    plan: tile::TilePlan,
    merger: tile::Merger,
    model_scale: u32,
    factor: u32,
}

#[wasm_bindgen]
impl AiUpscaler {
    /// `model_scale` is the network's own factor (4); `out_scale` what the
    /// merge keeps (1, 2 or 4).
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &[u8], w: u32, h: u32, tile_size: u32, overlap: u32, model_scale: u32, out_scale: u32) -> Result<AiUpscaler, JsError> {
        if out_scale == 0 || model_scale % out_scale != 0 {
            return Err(JsError::new("out_scale must divide the model scale"));
        }
        let img = rgb(rgba, w, h)?;
        let plan = tile::plan(w, h, tile_size, overlap);
        Ok(AiUpscaler { alpha: alpha_of(rgba), merger: tile::Merger::new(w * out_scale, h * out_scale), img, plan, model_scale, factor: model_scale / out_scale })
    }

    pub fn count(&self) -> u32 {
        self.plan.tiles.len() as u32
    }

    pub fn pad(&self) -> u32 {
        self.plan.pad
    }

    pub fn input(&self, i: u32) -> Vec<f32> {
        tile::extract(&self.img, &self.plan.tiles[i as usize], self.plan.pad)
    }

    pub fn add(&mut self, i: u32, chw: &[f32]) {
        let t = self.plan.tiles[i as usize];
        self.merger.add_downscaled(&self.plan, &t, self.model_scale, self.factor, chw);
    }

    pub fn out_width(&self) -> u32 {
        self.merger.width()
    }

    pub fn out_height(&self) -> u32 {
        self.merger.height()
    }

    /// The merged image as RGBA; alpha is the source's, resampled.
    pub fn finish(&self) -> Vec<u8> {
        let out = self.merger.finish();
        let (w, h) = (self.img.width, self.img.height);
        let alpha = if out.width == w && out.height == h {
            self.alpha.clone()
        } else {
            resize_raw(&self.alpha, w as usize, h as usize, 1, out.width as usize, out.height as usize, Resample::Lanczos)
        };
        out.to_rgba(Some(&alpha))
    }
}

// ---------------------------------------------------------------- inpaint

/// Document rectangle to read for a selection with these bounds: JSON `{x,y,w,h}`.
#[wasm_bindgen]
pub fn ai_inpaint_region(doc_w: u32, doc_h: u32, x: i32, y: i32, w: u32, h: u32) -> String {
    serde_json::to_string(&inpaint::plan_region(doc_w, doc_h, inpaint::Region { x, y, w, h })).unwrap()
}

#[wasm_bindgen]
pub struct AiInpainter {
    inner: Option<inpaint::Inpainter>,
}

#[wasm_bindgen]
impl AiInpainter {
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &[u8], coverage: &[u8], w: u32, h: u32, options: &str) -> Result<AiInpainter, JsError> {
        let opts: inpaint::Options = if options.is_empty() { Default::default() } else { serde_json::from_str(options).map_err(|e| JsError::new(&e.to_string()))? };
        let inner = inpaint::Inpainter::new(rgba.to_vec(), coverage, w, h, opts).map_err(|e| JsError::new(&e))?;
        Ok(AiInpainter { inner: Some(inner) })
    }

    fn get(&self) -> &inpaint::Inpainter {
        self.inner.as_ref().expect("inpainter already finished")
    }

    pub fn count(&self) -> u32 {
        self.get().crops().len() as u32
    }

    /// Share of the first model input that is hole (engine choice).
    pub fn hole_fraction(&self) -> f32 {
        self.get().hole_fraction()
    }

    pub fn crops(&self) -> String {
        serde_json::to_string(self.get().crops()).unwrap()
    }

    /// 512² RGB bytes followed by 512² hole bytes (255 = fill).
    pub fn input(&self, i: u32) -> Vec<u8> {
        let (mut rgb, hole) = self.get().input(i as usize);
        rgb.extend_from_slice(&hole);
        rgb
    }

    /// The model's 512² RGB output for crop `i`.
    pub fn apply(&mut self, i: u32, rgb: &[u8]) {
        self.inner.as_mut().expect("inpainter already finished").apply(i as usize, rgb);
    }

    pub fn finish(&mut self) -> Vec<u8> {
        self.inner.take().expect("inpainter already finished").finish()
    }
}

// ---------------------------------------------------------------- mattes

#[wasm_bindgen]
pub fn ai_modnet_input(rgba: &[u8], w: u32, h: u32) -> Result<Vec<f32>, JsError> {
    Ok(matte::modnet_input(&rgb(rgba, w, h)?))
}

#[wasm_bindgen]
pub fn ai_u2net_input(rgba: &[u8], w: u32, h: u32) -> Result<Vec<f32>, JsError> {
    Ok(matte::u2net_input(&rgb(rgba, w, h)?))
}

/// A model matte (`kind`: "modnet" 512² alpha or "u2net" 320² saliency) →
/// an 8-bit document matte, refined against `rgba` when `refine`.
#[wasm_bindgen]
pub fn ai_matte_finish(out: &[f32], kind: &str, rgba: &[u8], w: u32, h: u32, refine: bool) -> Result<Vec<u8>, JsError> {
    let img = rgb(rgba, w, h)?;
    let (small, size) = match kind {
        "u2net" => (matte::normalise_u2net(out), matte::U2NET_SIZE),
        _ => (out[..(matte::MODNET_SIZE * matte::MODNET_SIZE) as usize].to_vec(), matte::MODNET_SIZE),
    };
    Ok(matte::finish_matte(&small, size, &img, w, h, refine))
}

/// Boxes `[x0, y0, x1, y1, …]` around a matte's main blobs (EdgeTAM prompts).
#[wasm_bindgen]
pub fn ai_subject_boxes(m: &[u8], w: u32, h: u32, max_boxes: u32) -> Vec<f32> {
    matte::subject_boxes(m, w, h, max_boxes as usize).into_iter().flatten().collect()
}

/// Remove Background onto a layer that may already have a mask, limited to a
/// selection: `existing` and `selection` are empty when absent. See
/// `matte::combine_with_existing`.
#[wasm_bindgen]
pub fn ai_combine_masks(matte: &[u8], existing: &[u8], selection: &[u8]) -> Result<Vec<u8>, JsError> {
    let opt = |b: &'static str, v: &[u8]| -> Result<(), JsError> {
        if !v.is_empty() && v.len() != matte.len() {
            return Err(JsError::new(&format!("{b} is {} bytes; the matte is {}", v.len(), matte.len())));
        }
        Ok(())
    };
    opt("existing mask", existing)?;
    opt("selection", selection)?;
    let e = (!existing.is_empty()).then_some(existing);
    let s = (!selection.is_empty()).then_some(selection);
    Ok(matte::combine_with_existing(matte, e, s))
}

#[wasm_bindgen]
pub fn ai_matte_coverage(m: &[u8]) -> f32 {
    matte::coverage(m)
}

#[wasm_bindgen]
pub fn ai_looks_like_subject(m: &[u8]) -> bool {
    matte::looks_like_subject(m)
}

/// Background-weighted blur; `background` is 255 where the background is.
#[wasm_bindgen]
pub fn ai_background_blur(rgba: &[u8], w: u32, h: u32, background: &[u8], sigma: f32) -> Result<Vec<u8>, JsError> {
    let img = rgb(rgba, w, h)?;
    Ok(blur::background_blur(&img, background, sigma).to_rgba(None))
}

#[wasm_bindgen]
pub fn ai_sky_fraction(rgba: &[u8], w: u32, h: u32) -> Result<f32, JsError> {
    Ok(scene::sky_fraction(&rgb(rgba, w, h)?))
}

// ---------------------------------------------------------------- SAM

#[wasm_bindgen]
pub fn ai_sam_input(rgba: &[u8], w: u32, h: u32) -> Result<Vec<f32>, JsError> {
    Ok(sam::encoder_input(&rgb(rgba, w, h)?))
}

#[wasm_bindgen]
pub fn ai_sam_points(points: &[f32], w: u32, h: u32) -> Vec<f32> {
    sam::point_coords(points, w, h)
}

#[wasm_bindgen]
pub fn ai_sam_choose(pred: &[f32], iou: &[f32], prefer_whole: bool) -> u32 {
    sam::choose(pred, iou, prefer_whole) as u32
}

/// Mask candidate → document mask. `rgba` may be empty (no refinement).
#[wasm_bindgen]
pub fn ai_sam_mask(pred: &[f32], index: u32, w: u32, h: u32, rgba: &[u8], gw: u32, gh: u32) -> Result<Vec<u8>, JsError> {
    let guide = if rgba.is_empty() { None } else { Some(rgb(rgba, gw, gh)?) };
    Ok(sam::to_mask(pred, index as usize, w, h, guide.as_ref()))
}

// ---------------------------------------------------------------- faces

#[wasm_bindgen]
pub fn ai_face_detect_input(rgba: &[u8], w: u32, h: u32) -> Result<Vec<f32>, JsError> {
    Ok(face::detect_input(&rgb(rgba, w, h)?))
}

#[wasm_bindgen]
pub fn ai_face_detect_scale(w: u32, h: u32) -> f32 {
    face::detect_scale(w, h)
}

/// YuNet heads → JSON `[{score, bbox:[x,y,w,h], landmarks:[[x,y]×5]}]`.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn ai_face_detect_decode(
    scale: f32,
    cls8: &[f32], obj8: &[f32], bbox8: &[f32], kps8: &[f32],
    cls16: &[f32], obj16: &[f32], bbox16: &[f32], kps16: &[f32],
    cls32: &[f32], obj32: &[f32], bbox32: &[f32], kps32: &[f32],
) -> String {
    let heads = [
        face::StrideHeads { stride: 8, cls: cls8, obj: obj8, bbox: bbox8, kps: kps8 },
        face::StrideHeads { stride: 16, cls: cls16, obj: obj16, bbox: bbox16, kps: kps16 },
        face::StrideHeads { stride: 32, cls: cls32, obj: obj32, bbox: bbox32, kps: kps32 },
    ];
    serde_json::to_string(&face::detect_decode(&heads, scale)).unwrap()
}

fn landmarks(flat: &[f32]) -> Result<[[f32; 2]; 5], JsError> {
    if flat.len() < 10 {
        return Err(JsError::new("expected 10 landmark coordinates"));
    }
    Ok(std::array::from_fn(|i| [flat[i * 2], flat[i * 2 + 1]]))
}

/// The aligned 512² GFPGAN input for a face (landmarks in `rgba`'s frame).
#[wasm_bindgen]
pub fn ai_face_crop(rgba: &[u8], w: u32, h: u32, lm: &[f32]) -> Result<Vec<f32>, JsError> {
    Ok(face::face_crop(&rgb(rgba, w, h)?, &landmarks(lm)?))
}

/// Paste a restored face back into `rgba` (same frame as the landmarks).
#[wasm_bindgen]
pub fn ai_face_paste(rgba: &[u8], w: u32, h: u32, lm: &[f32], chw: &[f32], feather: f32, strength: f32) -> Result<Vec<u8>, JsError> {
    let mut img = rgb(rgba, w, h)?;
    face::face_paste(&mut img, 1.0, &landmarks(lm)?, chw, feather, strength);
    Ok(img.to_rgba(Some(&alpha_of(rgba))))
}

// ---------------------------------------------------------------- colour

#[wasm_bindgen]
pub fn ai_deoldify_input(rgba: &[u8], w: u32, h: u32, size: u32) -> Result<Vec<f32>, JsError> {
    Ok(color::deoldify_input(&rgb(rgba, w, h)?, size))
}

#[wasm_bindgen]
pub fn ai_colorize_merge(rgba: &[u8], w: u32, h: u32, chw: &[f32], size: u32, saturation: f32) -> Result<Vec<u8>, JsError> {
    let out = color::colorize_merge(&rgb(rgba, w, h)?, chw, size, saturation);
    Ok(out.to_rgba(Some(&alpha_of(rgba))))
}

#[wasm_bindgen]
pub fn ai_mean_chroma(rgba: &[u8], w: u32, h: u32) -> Result<f32, JsError> {
    Ok(color::mean_chroma(&rgb(rgba, w, h)?))
}

// ---------------------------------------------------------------- canary

#[wasm_bindgen]
pub fn ai_canary_signature(chw: &[f32], c: u32, h: u32, w: u32) -> Vec<f32> {
    canary::signature(chw, c as usize, h as usize, w as usize)
}

#[wasm_bindgen]
pub fn ai_canary_deviation(sig: &[f32], reference: &[f32]) -> f32 {
    canary::deviation(sig, reference)
}

#[wasm_bindgen]
pub fn ai_degenerate(values: &[f32]) -> bool {
    canary::degenerate(values)
}

// ---------------------------------------------------------------- planner

/// Sentence → JSON `{steps:[{label, cmd?|action?, params?}], unsupported?, ignored?}`.
#[wasm_bindgen]
pub fn ai_plan(text: &str) -> String {
    serde_json::to_string(&plan::plan(text)).unwrap()
}
