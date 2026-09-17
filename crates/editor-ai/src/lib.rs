//! editor-ai: everything around an AI model's forward pass, in pure Rust.
//!
//! The browser runs ONNX graphs through onnxruntime-web (the only way to
//! reach WebGPU), in the engine worker. Everything else lives here, once,
//! tested natively with `cargo test`:
//!
//! - [`tile`]: constant-square tiling and seam cross-fade for the
//!   super-resolution models (from OpenPixels, where WebGPU taught us why).
//! - [`inpaint`]: remove-object orchestration — crop the hole with context,
//!   resize to the model's fixed square, paste back with a feathered blend
//!   and grain matching.
//! - [`matte`]: subject masks (MODNet, U²-Net-p) and guided-filter
//!   refinement (from OpenPhotoId).
//! - [`sam`]: click-to-select pre- and post-processing for EdgeTAM.
//! - [`face`]: YuNet decoding and GFPGAN alignment and paste-back.
//! - [`color`]: LAB and the colourisation chroma merge.
//! - [`blur`]: background-weighted blur, so a blurred background does not
//!   smear the subject into a halo.
//! - [`scene`]: cheap content heuristics (sky) for quick actions.
//! - [`canary`]: output signatures for the per-backend pixel-parity canary.
//! - [`plan`]: the rule-based "describe an edit" planner.
//!
//! Pixel layout: [`Rgb`] is 8-bit interleaved RGB, row-major; the engine
//! hands us straight RGBA8, converted at the edges. Model tensors are `f32`
//! NCHW with a batch of one.

pub mod blur;
pub mod canary;
pub mod color;
pub mod face;
pub mod img;
pub mod inpaint;
pub mod matte;
pub mod plan;
pub mod sam;
pub mod scene;
pub mod tile;

pub use img::Rgb;

/// Round and clamp to a byte.
#[inline]
pub(crate) fn q8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::Rgb;

    /// A deterministic test image with edges, gradients and texture.
    pub fn synthetic(w: u32, h: u32) -> Rgb {
        let mut img = Rgb::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                let checker = ((x / 8 + y / 8) % 2) as f32 * 120.0;
                let grad = 255.0 * x as f32 / w.max(1) as f32;
                let tex = 40.0 * ((x as f32 * 0.7).sin() * (y as f32 * 0.5).cos());
                img.data[i] = (checker + tex).clamp(0.0, 255.0) as u8;
                img.data[i + 1] = (grad * 0.6 + tex).clamp(0.0, 255.0) as u8;
                img.data[i + 2] = (255.0 - grad + checker * 0.3).clamp(0.0, 255.0) as u8;
            }
        }
        img
    }
}
