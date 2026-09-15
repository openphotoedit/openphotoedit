//! `filter.*` and `analyze.*` commands. Registered with the editor by
//! [`register`].
//!
//! Destructive filters read their layer through `pixels::edit_layer` with an
//! apron as wide as the kernel reaches, so a filtered selection or tile edge
//! is seamless, feathered selections blend, and locks are respected. The
//! pixel maths lives in the topic modules; this file parses commands and
//! picks rectangles.

// `as_chunks` (1.88) and `is_multiple_of` (1.87) are newer than the
// workspace's Rust 1.85. Channel loops index several parallel buffers, which
// reads more plainly than zipped iterators.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks, clippy::manual_is_multiple_of, clippy::needless_range_loop)]

pub mod analyze;
pub mod blur;
pub mod distort;
pub mod inpaint;
pub mod noise;
pub mod retouch;
pub mod sharpen;
pub mod stylize;
pub mod util;

use editor_core::adjust::Adjustment;
use editor_core::color::Rgba8;
use editor_core::document::Document;
use editor_core::editor::Editor;
use editor_core::layer::LayerId;
use editor_core::ops::{parse, Applied, EditorError, Result};
use serde::Deserialize;
use serde_json::Value;

use util::{run, Area};

pub fn register(ed: &mut Editor) {
    ed.register_domain("filter", apply);
    ed.register_domain("analyze", analyze::apply);
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RadialMode {
    #[default]
    Spin,
    Zoom,
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RippleSize {
    Small,
    #[default]
    Medium,
    Large,
}

fn d50() -> f32 {
    50.0
}
fn d100() -> f32 {
    100.0
}
fn d135() -> f32 {
    135.0
}
fn d3() -> f32 {
    3.0
}
fn d1() -> f32 {
    1.0
}
fn d6() -> f32 {
    6.0
}
fn d60() -> f32 {
    60.0
}
fn d45() -> f32 {
    45.0
}
fn black() -> Rgba8 {
    Rgba8::BLACK
}
fn white() -> Rgba8 {
    Rgba8::WHITE
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Cmd {
    #[serde(rename = "filter.gaussian-blur")]
    GaussianBlur { radius: f32 },
    #[serde(rename = "filter.box-blur")]
    BoxBlur { radius: f32 },
    #[serde(rename = "filter.motion-blur")]
    MotionBlur { #[serde(default)] angle: f32, distance: f32 },
    /// `amount` 1..100; `cx, cy` in document pixels (default: the centre of
    /// the filtered area).
    #[serde(rename = "filter.radial-blur")]
    RadialBlur { amount: f32, #[serde(default)] mode: RadialMode, #[serde(default)] cx: Option<f32>, #[serde(default)] cy: Option<f32> },
    #[serde(rename = "filter.surface-blur")]
    SurfaceBlur { radius: f32, threshold: f32 },
    /// A depth map arrives as single-channel `bytes`, document-sized. Blur
    /// at a pixel is `radius · |depth − focal| / 255`; `focal` defaults to 0.
    #[serde(rename = "filter.lens-blur")]
    LensBlur { radius: f32, #[serde(default)] depth: Option<Value>, #[serde(default)] focal: f32 },
    #[serde(rename = "filter.tilt-shift")]
    TiltShift { center_y: f32, band: f32, #[serde(default)] feather: f32, radius: f32 },

    #[serde(rename = "filter.unsharp-mask")]
    UnsharpMask { amount: f32, radius: f32, #[serde(default)] threshold: f32 },
    #[serde(rename = "filter.smart-sharpen")]
    SmartSharpen { amount: f32, radius: f32, #[serde(default)] reduce_noise: f32 },
    #[serde(rename = "filter.high-pass")]
    HighPass { radius: f32 },

    #[serde(rename = "filter.add-noise")]
    AddNoise { amount: f32, #[serde(default)] gaussian: bool, #[serde(default)] monochromatic: bool, #[serde(default)] seed: u64 },
    #[serde(rename = "filter.reduce-noise")]
    ReduceNoise { #[serde(default = "d6")] strength: f32, #[serde(default = "d60")] preserve_details: f32, #[serde(default = "d45")] reduce_color_noise: f32 },
    #[serde(rename = "filter.median")]
    Median { radius: f32 },
    #[serde(rename = "filter.dust-and-scratches")]
    DustAndScratches { radius: f32, #[serde(default)] threshold: f32 },
    #[serde(rename = "filter.minimum")]
    Minimum { radius: f32 },
    #[serde(rename = "filter.maximum")]
    Maximum { radius: f32 },

    #[serde(rename = "filter.pixelate")]
    Pixelate { cell: u32 },
    #[serde(rename = "filter.emboss")]
    Emboss { #[serde(default = "d135")] angle: f32, #[serde(default = "d3")] height: f32, #[serde(default = "d100")] amount: f32 },
    #[serde(rename = "filter.find-edges")]
    FindEdges {},
    #[serde(rename = "filter.solarize")]
    Solarize {},
    #[serde(rename = "filter.invert")]
    Invert {},
    #[serde(rename = "filter.desaturate")]
    Desaturate {},
    #[serde(rename = "filter.twirl")]
    Twirl { angle: f32 },
    #[serde(rename = "filter.pinch")]
    Pinch { amount: f32 },
    #[serde(rename = "filter.spherize")]
    Spherize { amount: f32 },
    #[serde(rename = "filter.ripple")]
    Ripple { amount: f32, #[serde(default)] size: RippleSize },
    #[serde(rename = "filter.polar-coordinates")]
    PolarCoordinates { #[serde(default)] to_polar: bool },
    #[serde(rename = "filter.offset")]
    Offset { dx: i32, dy: i32, #[serde(default)] wrap: bool },
    #[serde(rename = "filter.clouds")]
    Clouds { #[serde(default)] seed: u64, #[serde(default = "black")] fg: Rgba8, #[serde(default = "white")] bg: Rgba8 },
    #[serde(rename = "filter.custom")]
    Custom { kernel: Vec<f32>, #[serde(default = "d1")] scale: f32, #[serde(default)] offset: f32 },
    #[serde(rename = "filter.apply-adjustment")]
    ApplyAdjustment { adjustment: Adjustment },
    #[serde(rename = "filter.vignette")]
    Vignette { amount: f32, #[serde(default = "d50")] midpoint: f32, #[serde(default = "d50")] feather: f32 },

    #[serde(rename = "filter.content-aware-fill")]
    ContentAwareFill { #[serde(default)] sample: Option<String>, #[serde(default)] seed: u64 },
    #[serde(rename = "filter.spot-heal")]
    SpotHeal { x: f32, y: f32, radius: f32 },
    #[serde(rename = "filter.red-eye")]
    RedEye { x: i32, y: i32, width: i32, height: i32, #[serde(default = "d50")] pupil_size: f32, #[serde(default = "d50")] darken: f32 },
    #[serde(rename = "filter.frequency-separation")]
    FrequencySeparation { radius: f32 },
}

/// Parse a command of this crate, or report `UnknownOp` so the registry can
/// try another handler.
pub(crate) fn parse_cmd<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
    match parse::<T>(v) {
        Err(EditorError::Json(msg)) if msg.contains("unknown variant") && msg.contains(&format!("`{op}`")) => Err(EditorError::UnknownOp(op)),
        other => other,
    }
}

fn layer_id(v: &Value) -> Option<LayerId> {
    v.get("id").and_then(Value::as_u64).map(|id| id as LayerId)
}

fn finite(name: &str, v: f32) -> Result<f32> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(EditorError::Invalid(format!("`{name}` must be a number")))
    }
}

pub fn apply(v: Value, doc: &mut Document, bytes: &[u8]) -> Result<Applied> {
    let id = layer_id(&v);
    let cmd: Cmd = parse_cmd(v)?;
    run_cmd(cmd, id, doc, bytes)
}

fn run_cmd(cmd: Cmd, id: Option<LayerId>, doc: &mut Document, bytes: &[u8]) -> Result<Applied> {
    use util::linear_filter;
    match cmd {
        Cmd::GaussianBlur { radius } => {
            let sigma = finite("radius", radius)?.clamp(0.0, 1000.0);
            let reach = blur::gaussian_reach(sigma);
            run(doc, id, Area::Content(reach), reach, true, "Gaussian Blur", |buf, f| {
                linear_filter(buf, f.w, f.h, |p, w, h| blur::gaussian(p, w, h, sigma));
            })
        }
        Cmd::BoxBlur { radius } => {
            let r = finite("radius", radius)?.clamp(0.0, 2000.0);
            let reach = r.ceil() as i32 + 1;
            run(doc, id, Area::Content(reach), reach, true, "Box Blur", |buf, f| {
                linear_filter(buf, f.w, f.h, |p, w, h| blur::box_blur(p, w, h, r));
            })
        }
        Cmd::MotionBlur { angle, distance } => {
            let (angle, dist) = (finite("angle", angle)?, finite("distance", distance)?.clamp(0.0, 2000.0));
            let reach = (dist / 2.0).ceil() as i32 + 2;
            run(doc, id, Area::Content(reach), reach, true, "Motion Blur", |buf, f| {
                linear_filter(buf, f.w, f.h, |p, w, h| blur::motion(p, w, h, angle, dist));
            })
        }
        Cmd::RadialBlur { amount, mode, cx, cy } => {
            let amount = finite("amount", amount)?.clamp(0.0, 100.0);
            let rect = util::area_rect(doc, util_target(doc, id)?, Area::Content(0))?;
            let center = (cx.unwrap_or(rect.x as f32 + rect.w as f32 / 2.0), cy.unwrap_or(rect.y as f32 + rect.h as f32 / 2.0));
            // With a selection, streaks still sample the pixels around it:
            // as far as the longest half-streak reaches. Without one the
            // canvas edge is simply repeated.
            let apron = if doc.selection.is_some() {
                let far = [(rect.x, rect.y), (rect.right(), rect.y), (rect.x, rect.bottom()), (rect.right(), rect.bottom())]
                    .iter()
                    .map(|&(x, y)| ((x as f32 - center.0).powi(2) + (y as f32 - center.1).powi(2)).sqrt())
                    .fold(0f32, f32::max);
                let half = if mode == RadialMode::Zoom { far * amount / 100.0 * 0.3 } else { far * (amount.to_radians() / 2.0).min(1.0) };
                (half.ceil() as i32 + 2).min(1024)
            } else {
                0
            };
            run(doc, id, Area::Content(0), apron, true, "Radial Blur", |buf, f| {
                let (bx, by) = (center.0 - f.outer.x as f32, center.1 - f.outer.y as f32);
                blur::radial(buf, f, amount, mode == RadialMode::Zoom, bx, by);
            })
        }
        Cmd::SurfaceBlur { radius, threshold } => {
            let r = finite("radius", radius)?.clamp(0.0, 100.0).round() as usize;
            let t = finite("threshold", threshold)?.clamp(0.0, 255.0);
            run(doc, id, Area::Content(0), r as i32, true, "Surface Blur", |buf, f| {
                let inner = f.inner;
                blur::surface(buf, f.w, f.h, r, t, (inner.x as usize, inner.y as usize, inner.right() as usize, inner.bottom() as usize));
            })
        }
        Cmd::LensBlur { radius, focal, .. } => {
            let r = finite("radius", radius)?.clamp(0.0, 250.0);
            let (dw, dh) = (doc.width as usize, doc.height as usize);
            let depth = if bytes.is_empty() {
                None
            } else if bytes.len() == dw * dh {
                Some(bytes)
            } else {
                return Err(EditorError::Invalid(format!("depth map must be {dw}×{dh} single-channel bytes, got {}", bytes.len())));
            };
            let reach = r.ceil() as i32 + 1;
            run(doc, id, Area::Content(reach), reach, true, "Lens Blur", |buf, f| {
                let inner = f.inner;
                let (ox, oy) = (f.outer.x as i64, f.outer.y as i64);
                blur::lens(buf, f.w, f.h, r, (inner.x as usize, inner.y as usize, inner.right() as usize, inner.bottom() as usize), depth.is_none(), |x, y| match depth {
                    None => r,
                    Some(d) => {
                        let dx = (ox + x as i64).clamp(0, dw as i64 - 1) as usize;
                        let dy = (oy + y as i64).clamp(0, dh as i64 - 1) as usize;
                        r * (d[dy * dw + dx] as f32 - focal).abs() / 255.0
                    }
                });
            })
        }
        Cmd::TiltShift { center_y, band, feather, radius } => {
            let sigma = finite("radius", radius)?.clamp(0.0, 500.0);
            let (cy, band, feather) = (finite("center_y", center_y)?, finite("band", band)?.max(0.0), finite("feather", feather)?.max(0.0));
            let reach = blur::gaussian_reach(sigma);
            run(doc, id, Area::Content(reach), reach, true, "Tilt-Shift", |buf, f| {
                let oy = f.outer.y as f32;
                blur::tilt_shift(
                    buf,
                    f.w,
                    f.h,
                    |y| {
                        let d = ((oy + y as f32 + 0.5) - cy).abs() - band / 2.0;
                        if d <= 0.0 {
                            0.0
                        } else if feather <= 0.0 {
                            sigma
                        } else {
                            let t = (d / feather).min(1.0);
                            sigma * t * t * (3.0 - 2.0 * t)
                        }
                    },
                    sigma,
                );
            })
        }

        Cmd::UnsharpMask { amount, radius, threshold } => {
            let (amount, sigma, t) = (finite("amount", amount)?.clamp(0.0, 1000.0), finite("radius", radius)?.clamp(0.0, 1000.0), finite("threshold", threshold)?.clamp(0.0, 255.0));
            let reach = blur::gaussian_reach(sigma);
            run(doc, id, Area::Content(0), reach, true, "Unsharp Mask", |buf, f| sharpen::unsharp(buf, f.w, f.h, amount, sigma, t))
        }
        Cmd::SmartSharpen { amount, radius, reduce_noise } => {
            let (amount, sigma, nr) = (finite("amount", amount)?.clamp(0.0, 500.0), finite("radius", radius)?.clamp(0.0, 64.0), finite("reduce_noise", reduce_noise)?.clamp(0.0, 100.0));
            let reach = blur::gaussian_reach(sigma) + 2;
            run(doc, id, Area::Content(0), reach, true, "Smart Sharpen", |buf, f| sharpen::smart(buf, f.w, f.h, amount, sigma, nr))
        }
        Cmd::HighPass { radius } => {
            let sigma = finite("radius", radius)?.clamp(0.0, 1000.0);
            let reach = blur::gaussian_reach(sigma);
            run(doc, id, Area::Content(0), reach, true, "High Pass", |buf, f| sharpen::high_pass(buf, f.w, f.h, sigma))
        }

        Cmd::AddNoise { amount, gaussian, monochromatic, seed } => {
            let amount = finite("amount", amount)?.clamp(0.0, 400.0);
            run(doc, id, Area::Content(0), 0, false, "Add Noise", |buf, f| noise::add(buf, f, amount, gaussian, monochromatic, seed))
        }
        Cmd::ReduceNoise { strength, preserve_details, reduce_color_noise } => {
            let s = finite("strength", strength)?.clamp(0.0, 10.0);
            let pd = finite("preserve_details", preserve_details)?.clamp(0.0, 100.0);
            let rc = finite("reduce_color_noise", reduce_color_noise)?.clamp(0.0, 100.0);
            run(doc, id, Area::Content(0), noise::DENOISE_REACH, true, "Reduce Noise", |buf, f| noise::reduce(buf, f.w, f.h, s, pd, rc))
        }
        Cmd::Median { radius } => {
            let r = finite("radius", radius)?.clamp(0.0, 500.0).round() as usize;
            run(doc, id, Area::Content(0), r as i32, true, "Median", |buf, f| noise::median(buf, f, r, 0.0))
        }
        Cmd::DustAndScratches { radius, threshold } => {
            let r = finite("radius", radius)?.clamp(0.0, 500.0).round() as usize;
            let t = finite("threshold", threshold)?.clamp(0.0, 255.0);
            run(doc, id, Area::Content(0), r as i32, true, "Dust & Scratches", |buf, f| noise::median(buf, f, r, t))
        }
        Cmd::Minimum { radius } | Cmd::Maximum { radius } => {
            let is_max = matches!(cmd, Cmd::Maximum { .. });
            let r = finite("radius", radius)?.clamp(0.0, 500.0).round() as usize;
            run(doc, id, Area::Content(r as i32), r as i32, true, if is_max { "Maximum" } else { "Minimum" }, |buf, f| noise::min_max(buf, f.w, f.h, r, is_max))
        }

        Cmd::Pixelate { cell } => {
            if cell == 0 {
                return Err(EditorError::Invalid("`cell` must be at least 1".into()));
            }
            let cell = cell.min(4096) as usize;
            run(doc, id, Area::Content(0), 0, false, "Mosaic", |buf, f| stylize::pixelate(buf, f.w, f.h, cell))
        }
        Cmd::Emboss { angle, height, amount } => {
            let (angle, height, amount) = (finite("angle", angle)?, finite("height", height)?.clamp(1.0, 100.0), finite("amount", amount)?.clamp(1.0, 500.0));
            let reach = height.ceil() as i32 + 2;
            run(doc, id, Area::Content(0), reach, true, "Emboss", |buf, f| stylize::emboss(buf, f, angle, height, amount))
        }
        Cmd::FindEdges {} => run(doc, id, Area::Content(0), 1, true, "Find Edges", |buf, f| stylize::find_edges(buf, f.w, f.h)),
        Cmd::Solarize {} => run(doc, id, Area::Content(0), 0, false, "Solarize", |buf, _| stylize::point(buf, |v| if v > 127 { 255 - v } else { v })),
        Cmd::Invert {} => run(doc, id, Area::Content(0), 0, false, "Invert", |buf, _| stylize::point(buf, |v| 255 - v)),
        Cmd::Desaturate {} => run(doc, id, Area::Content(0), 0, false, "Desaturate", |buf, _| stylize::desaturate(buf)),
        Cmd::Twirl { angle } => {
            let angle = finite("angle", angle)?.clamp(-999.0, 999.0);
            run(doc, id, Area::Content(0), 0, false, "Twirl", |buf, f| distort::twirl(buf, f, angle))
        }
        Cmd::Pinch { amount } => {
            let amount = finite("amount", amount)?.clamp(-100.0, 100.0);
            run(doc, id, Area::Content(0), 0, false, "Pinch", |buf, f| distort::pinch(buf, f, amount))
        }
        Cmd::Spherize { amount } => {
            let amount = finite("amount", amount)?.clamp(-100.0, 100.0);
            run(doc, id, Area::Content(0), 0, false, "Spherize", |buf, f| distort::spherize(buf, f, amount))
        }
        Cmd::Ripple { amount, size } => {
            let amount = finite("amount", amount)?.clamp(-999.0, 999.0);
            let (wave, amp) = distort::ripple_params(amount, size);
            let reach = amp.abs().ceil() as i32 + 2;
            let _ = wave;
            run(doc, id, Area::Content(0), reach, true, "Ripple", |buf, f| distort::ripple(buf, f, amount, size))
        }
        Cmd::PolarCoordinates { to_polar } => run(doc, id, Area::Content(0), 0, false, "Polar Coordinates", |buf, f| distort::polar(buf, f, to_polar)),
        Cmd::Offset { dx, dy, wrap } => run(doc, id, Area::Canvas, 0, false, "Offset", |buf, f| stylize::offset(buf, f.w, f.h, dx, dy, wrap)),
        Cmd::Clouds { seed, fg, bg } => {
            let (dw, dh) = (doc.width, doc.height);
            run(doc, id, Area::Canvas, 0, false, "Clouds", |buf, f| stylize::clouds(buf, f, seed, fg, bg, dw.max(dh)))
        }
        Cmd::Custom { kernel, scale, offset } => {
            let n = (kernel.len() as f64).sqrt() as usize;
            if n * n != kernel.len() || n % 2 == 0 || n == 0 {
                return Err(EditorError::Invalid("`kernel` must hold an odd square number of weights (9, 25, …)".into()));
            }
            if kernel.iter().any(|v| !v.is_finite()) || !scale.is_finite() || !offset.is_finite() {
                return Err(EditorError::Invalid("kernel weights, `scale` and `offset` must be numbers".into()));
            }
            let scale = if scale == 0.0 { 1.0 } else { scale };
            let reach = (n / 2) as i32;
            run(doc, id, Area::Content(0), reach, true, "Custom", |buf, f| stylize::custom(buf, f.w, f.h, &kernel, n, scale, offset))
        }
        Cmd::ApplyAdjustment { adjustment } => {
            let label = adjustment.label();
            let (dw, dh) = (doc.width, doc.height);
            // Develop's local contrast looks at a neighbourhood tied to the
            // document size; give it that much context.
            let apron = match &adjustment {
                Adjustment::Develop(_) => ((dw.min(dh) as f32 * 0.02).ceil() as i32 * 3 + 2).min(512),
                _ => 0,
            };
            run(doc, id, Area::Content(0), apron, true, label, |buf, f| stylize::apply_adjustment(buf, f, &adjustment, dw, dh))
        }
        Cmd::Vignette { amount, midpoint, feather } => {
            let (a, m, fe) = (finite("amount", amount)?.clamp(-100.0, 100.0), finite("midpoint", midpoint)?.clamp(0.0, 100.0), finite("feather", feather)?.clamp(0.0, 100.0));
            let target = util_target(doc, id)?;
            let area = util::area_rect(doc, target, Area::Canvas)?;
            run(doc, id, Area::Canvas, 0, false, "Vignette", |buf, f| stylize::vignette(buf, f, area, a, m, fe))
        }

        Cmd::ContentAwareFill { sample, seed } => {
            if let Some(s) = sample.as_deref() {
                if s != "auto" {
                    return Err(EditorError::Invalid(format!("unknown sampling area `{s}`; use \"auto\"")));
                }
            }
            retouch::content_aware_fill(doc, id, seed)
        }
        Cmd::SpotHeal { x, y, radius } => retouch::spot_heal(doc, id, finite("x", x)?, finite("y", y)?, finite("radius", radius)?),
        Cmd::RedEye { x, y, width, height, pupil_size, darken } => retouch::red_eye(doc, id, editor_core::geom::Rect::new(x, y, width, height), pupil_size.clamp(1.0, 100.0), darken.clamp(1.0, 100.0)),
        Cmd::FrequencySeparation { radius } => retouch::frequency_separation(doc, id, finite("radius", radius)?.clamp(0.0, 500.0)),
    }
}

fn util_target(doc: &Document, id: Option<LayerId>) -> Result<LayerId> {
    editor_core::ops::target_id(doc, id)
}

#[cfg(test)]
mod tests;
