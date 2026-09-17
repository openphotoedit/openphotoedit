//! Tiling, so a super-resolution model never sees more than a fixed patch,
//! and the seams between patches are invisible. Ported from OpenPixels
//! (`pixels-core/src/tile.rs`), where every rule below was learned.
//!
//! Each tile is a `tile × tile` core expanded by `overlap` on every side that
//! is not an image border. On merge every pixel is weighted by a ramp from 0
//! at a non-border edge to 1 at `overlap × scale` in, and the sum normalised,
//! so a seam is a cross-fade rather than a cut.
//!
//! **Every tile is padded to one constant square** (`tile + 2 × overlap`) by
//! replicating edge pixels. onnxruntime-web's WebGPU backend plans buffers on
//! the first run and reuses them; a later run with a different shape fails
//! with `Shape mismatch attempting to re-use buffer`. The square is a pure
//! function of the tile settings, never of the photo, so a session is built
//! once per model and reused across documents.
//!
//! The merger accumulates in 16-bit fixed point (half the memory of `f32`).
//! [`Merger::add_downscaled`] area-averages a 4× output back down *per tile*
//! so a 1× denoise or a 2× upscale never holds the 4× image in memory.

use crate::Rgb;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    /// Expanded region, in input pixels.
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TilePlan {
    pub width: u32,
    pub height: u32,
    pub tile: u32,
    pub overlap: u32,
    /// The square every tile is padded to before inference.
    pub pad: u32,
    pub tiles: Vec<Tile>,
}

/// Tile core starts along one axis. The last start is pulled back to a full
/// tile only when that does not duplicate the previous tile (a duplicate at
/// full weight overflows the `u16` sums; see the white-image test).
fn starts(length: u32, tile: u32, overlap: u32) -> Vec<u32> {
    if length <= tile {
        return vec![0];
    }
    let mut out = Vec::new();
    let mut x = 0;
    while x + tile < length {
        out.push(x);
        x += tile;
    }
    let last = length - tile;
    if out.last().is_none_or(|&prev| last > prev + overlap) {
        out.push(last);
    }
    out
}

pub fn plan(width: u32, height: u32, tile: u32, overlap: u32) -> TilePlan {
    let tile = tile.max(16);
    let mut tiles = Vec::new();
    for &y in &starts(height, tile, overlap) {
        let core_h = tile.min(height - y);
        for &x in &starts(width, tile, overlap) {
            let core_w = tile.min(width - x);
            let x0 = x.saturating_sub(overlap);
            let y0 = y.saturating_sub(overlap);
            let x1 = (x + core_w + overlap).min(width);
            let y1 = (y + core_h + overlap).min(height);
            tiles.push(Tile { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
        }
    }
    TilePlan { width, height, tile, overlap, pad: tile + 2 * overlap, tiles }
}

/// The expanded tile as NCHW `f32` in `[0, 1]`, padded to `pad × pad` by
/// replicating edge pixels (black padding would be sharpened into an edge).
pub fn extract(img: &Rgb, t: &Tile, pad: u32) -> Vec<f32> {
    let pad = pad.max(t.w).max(t.h);
    let plane = (pad * pad) as usize;
    let mut out = vec![0.0f32; plane * 3];
    for y in 0..pad {
        let sy = t.y + y.min(t.h - 1);
        for x in 0..pad {
            let sx = t.x + x.min(t.w - 1);
            let px = img.px(sx, sy);
            let i = (y * pad + x) as usize;
            out[i] = px[0] as f32 / 255.0;
            out[plane + i] = px[1] as f32 / 255.0;
            out[2 * plane + i] = px[2] as f32 / 255.0;
        }
    }
    out
}

/// Accumulates model outputs for every tile and produces the final image.
pub struct Merger {
    width: u32,
    height: u32,
    acc: Vec<u16>,
    wsum: Vec<u16>,
}

const ACC_SCALE: f32 = 64.0;
const W_SCALE: f32 = 8192.0;

impl Merger {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width * height) as usize;
        Merger { width, height, acc: vec![0; n * 3], wsum: vec![0; n] }
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Add the model's output for `t`: NCHW `f32` in `[0, 1]` at
    /// `pad × scale` square. Only the top-left `t.w×scale × t.h×scale` is real.
    pub fn add(&mut self, plan: &TilePlan, t: &Tile, scale: u32, chw: &[f32]) {
        let ow = t.w * scale;
        let oh = t.h * scale;
        let stride = plan.pad.max(t.w).max(t.h) * scale;
        let plane = (stride * stride) as usize;
        assert!(chw.len() >= plane * 3, "tile output is {} values, expected {} for {stride}²", chw.len(), plane * 3);
        let ramp = (plan.overlap * scale).max(1) as f32;
        let left_border = t.x == 0;
        let top_border = t.y == 0;
        let right_border = t.x + t.w >= plan.width;
        let bottom_border = t.y + t.h >= plan.height;
        let weights = |n: u32, first: bool, last: bool| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let a = if first { 1.0 } else { ((i as f32 + 0.5) / ramp).min(1.0) };
                    let b = if last { 1.0 } else { (((n - i) as f32 - 0.5) / ramp).min(1.0) };
                    a.min(b).max(1.0 / W_SCALE)
                })
                .collect()
        };
        let wx = weights(ow, left_border, right_border);
        let wy = weights(oh, top_border, bottom_border);
        let ox = t.x * scale;
        let oy = t.y * scale;
        for y in 0..oh {
            let gy = oy + y;
            if gy >= self.height {
                break;
            }
            for x in 0..ow {
                let gx = ox + x;
                if gx >= self.width {
                    break;
                }
                let w = wx[x as usize] * wy[y as usize];
                let src = (y * stride + x) as usize;
                let dst = (gy * self.width + gx) as usize;
                for c in 0..3 {
                    let v = (chw[c * plane + src] * 255.0).clamp(0.0, 255.0);
                    self.acc[dst * 3 + c] = self.acc[dst * 3 + c].saturating_add((v * w * ACC_SCALE).round() as u16);
                }
                self.wsum[dst] = self.wsum[dst].saturating_add((w * W_SCALE).round() as u16);
            }
        }
    }

    /// Add a `model_scale`× output merged at `model_scale / factor`×: the
    /// tile is area-averaged by `factor` first.
    pub fn add_downscaled(&mut self, plan: &TilePlan, t: &Tile, model_scale: u32, factor: u32, chw: &[f32]) {
        if factor <= 1 {
            return self.add(plan, t, model_scale, chw);
        }
        let stride = plan.pad.max(t.w).max(t.h) * model_scale;
        let small = crate::img::box_down_chw(chw, 3, stride, stride, factor);
        self.add(plan, t, model_scale / factor, &small);
    }

    pub fn finish(&self) -> Rgb {
        let n = (self.width * self.height) as usize;
        let mut data = vec![0u8; n * 3];
        for i in 0..n {
            let w = self.wsum[i] as f32 / W_SCALE;
            if w <= 0.0 {
                continue;
            }
            for c in 0..3 {
                data[i * 3 + c] = crate::q8(self.acc[i * 3 + c] as f32 / ACC_SCALE / w);
            }
        }
        Rgb { width: self.width, height: self.height, data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::img::resize;
    use crate::testutil::synthetic;
    use editor_core::transform::Resample;

    #[test]
    fn small_image_is_one_tile() {
        let p = plan(100, 80, 192, 16);
        assert_eq!(p.tiles, vec![Tile { x: 0, y: 0, w: 100, h: 80 }]);
    }

    #[test]
    fn every_tile_pads_to_one_constant_square() {
        let mut sizes = std::collections::BTreeSet::new();
        for (w, h) in [(500u32, 300u32), (288, 360), (37, 900), (60, 60), (2000, 1500)] {
            let p = plan(w, h, 192, 16);
            assert_eq!(p.pad, 224);
            let img = synthetic(w, h);
            for t in &p.tiles {
                assert!(t.w <= p.pad && t.h <= p.pad);
                sizes.insert(extract(&img, t, p.pad).len());
            }
        }
        assert_eq!(sizes.len(), 1, "shapes varied: {sizes:?}");
    }

    /// An identity "model" merged back reproduces the input: the seam
    /// weights normalise away. Swept over awkward sizes.
    #[test]
    fn identity_model_round_trip_is_lossless() {
        for (w, h, tile, overlap) in [(211u32, 137u32, 64u32, 8u32), (257, 257, 128, 16), (600, 90, 192, 16), (1000, 1000, 192, 16)] {
            let img = synthetic(w, h);
            let p = plan(w, h, tile, overlap);
            let mut m = Merger::new(w, h);
            for t in &p.tiles {
                m.add(&p, t, 1, &extract(&img, t, p.pad));
            }
            let out = m.finish();
            let max_err = out.data.iter().zip(&img.data).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(max_err <= 1, "{w}x{h}: max error {max_err}");
        }
    }

    #[test]
    fn flat_white_survives_the_merge_at_every_size() {
        for size in 250..=280u32 {
            let img = Rgb::from_vec(size, size, vec![255u8; (size * size * 3) as usize]);
            let p = plan(size, size, 128, 16);
            let mut m = Merger::new(size, size);
            for t in &p.tiles {
                m.add(&p, t, 1, &extract(&img, t, p.pad));
            }
            let worst = m.finish().data.iter().map(|v| 255 - *v as i32).max().unwrap();
            assert!(worst <= 1, "{size}: {worst}");
        }
    }

    /// A nearest-neighbour 4× "model" merged at 1× through the per-tile
    /// downscale reproduces the input: the path denoise and 2× use.
    #[test]
    fn four_x_model_downscaled_per_tile_round_trips() {
        let img = synthetic(150, 110);
        let p = plan(150, 110, 48, 8);
        for (factor, out_scale) in [(4u32, 1u32), (2, 2)] {
            let mut m = Merger::new(150 * out_scale, 110 * out_scale);
            for t in &p.tiles {
                let padded = Rgb::from_chw01(p.pad, p.pad, &extract(&img, t, p.pad));
                let up = resize(&padded, p.pad * 4, p.pad * 4, Resample::Nearest);
                m.add_downscaled(&p, t, 4, factor, &up.to_chw01());
            }
            let out = m.finish();
            let want = resize(&img, 150 * out_scale, 110 * out_scale, Resample::Nearest);
            let max_err = out.data.iter().zip(&want.data).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(max_err <= 1, "factor {factor}: max error {max_err}");
        }
    }

    #[test]
    fn disagreeing_tiles_crossfade() {
        let p = plan(64, 8, 32, 8);
        assert_eq!(p.tiles.len(), 2);
        let mut m = Merger::new(64, 8);
        for (i, t) in p.tiles.iter().enumerate() {
            let v = if i == 0 { 0.0 } else { 1.0 };
            m.add(&p, t, 1, &vec![v; (p.pad * p.pad * 3) as usize]);
        }
        let out = m.finish();
        let row: Vec<u8> = (0..64).map(|x| out.px(x, 4)[0]).collect();
        assert_eq!(row[0], 0);
        assert_eq!(row[63], 255);
        assert!(row.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 60));
    }
}
