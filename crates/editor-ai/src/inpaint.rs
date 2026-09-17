//! Remove-object orchestration around a fixed-square inpainting model
//! (MI-GAN or big-LaMa, both 512²).
//!
//! 1. [`plan_region`]: from the selection's bounds, the document rectangle
//!    to read — a square around the hole with room for context.
//! 2. [`Inpainter::new`]: the hole is the selection coverage, binarised and
//!    dilated a few pixels (the edge of a hand-drawn selection almost always
//!    cuts through the object's own soft border; a model shown that border
//!    faithfully rebuilds it as a ghost).
//! 3. Crops, each resized to the model size. Pass 1 is one crop over the
//!    whole hole. Optional refinement passes (`Options::refine`, **off by
//!    default**) re-run tiles at twice the resolution with only a
//!    checkerboard of blocks masked. Measured on a removed street walker it
//!    made things worse — each block was filled from the blocks around it
//!    and the fill turned to a pale patchwork — so it stays as an
//!    experiment; large holes go to LaMa instead (see [`Inpainter::hole_fraction`]).
//! 4. [`Inpainter::finish`]: grain matching — model output is smoother than
//!    a photograph, so the missing noise (measured in a ring around the
//!    hole) is added back, then the patch goes out as RGBA for
//!    `layer.set-pixels` with `respect_selection`.

use crate::img::resize_u8;
use editor_core::transform::{resize_raw, Resample};
use serde::{Deserialize, Serialize};

pub const MODEL_SIZE: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// A crop in region-local pixels, always inside the region, and which hole
/// pixels it may change. Ideally square; where the document is too small
/// for the square it is clipped to a rectangle and resized to the model's
/// square anisotropically. (Replicating edge pixels instead fed the model
/// a frame of streaks, which it faithfully continued into the fill.)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// `None` = the whole hole; `Some((phase, block))` = checkerboard blocks
    /// of `block` region pixels whose parity equals `phase`.
    pub checker: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Upper bound on model runs, refinement included.
    pub max_runs: u32,
    /// Add grain to match the surroundings.
    pub grain: bool,
    pub refine: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_runs: 12, grain: true, refine: false }
    }
}

/// How far the hole is dilated, in document pixels.
pub fn dilation(bw: u32, bh: u32) -> u32 {
    ((bw.max(bh) as f32 * 0.02).round() as u32).clamp(3, 24)
}

fn crop_side(bw: u32, bh: u32) -> u32 {
    let d = dilation(bw, bh);
    let (long, short) = (bw.max(bh) as f32, bw.min(bh) as f32);
    // Context: enough around the hole that the model sees what surrounds
    // it, without shrinking it more than necessary. The *short* side decides
    // how hard the fill is, so a tall narrow hole (a standing person) gets a
    // modest margin along its length rather than a square twice its height,
    // which cost 4× resolution and produced mush. Small holes stay native.
    let want = (long * 1.25).max(short * 2.5) + 2.0 * d as f32;
    MODEL_SIZE.max(want.ceil() as u32)
}

/// The document rectangle to read for a hole with bounds `bbox`.
pub fn plan_region(doc_w: u32, doc_h: u32, bbox: Region) -> Region {
    let side = crop_side(bbox.w, bbox.h) as i32;
    let cx = bbox.x + bbox.w as i32 / 2;
    let cy = bbox.y + bbox.h as i32 / 2;
    let x0 = (cx - side / 2).max(0);
    let y0 = (cy - side / 2).max(0);
    let x1 = (cx - side / 2 + side).min(doc_w as i32);
    let y1 = (cy - side / 2 + side).min(doc_h as i32);
    Region { x: x0, y: y0, w: (x1 - x0).max(1) as u32, h: (y1 - y0).max(1) as u32 }
}

pub struct Inpainter {
    w: u32,
    h: u32,
    orig: Vec<u8>,
    work: Vec<u8>,
    /// Pixels the model may replace (dilated, binary 0/255).
    hole: Vec<u8>,
    /// Blend weight 0..255: the hole with a one-pixel soft edge.
    alpha: Vec<u8>,
    crops: Vec<Crop>,
    opts: Options,
}

fn dilate(mask: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if r == 0 {
        return mask.to_vec();
    }
    // Separable square max filter via prefix counts of set pixels.
    let mut tmp = vec![0u8; w * h];
    for y in 0..h {
        let mut prefix = vec![0u32; w + 1];
        for x in 0..w {
            prefix[x + 1] = prefix[x] + (mask[y * w + x] > 0) as u32;
        }
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r + 1).min(w));
            tmp[y * w + x] = if prefix[b] > prefix[a] { 255 } else { 0 };
        }
    }
    let mut out = vec![0u8; w * h];
    for x in 0..w {
        let mut prefix = vec![0u32; h + 1];
        for y in 0..h {
            prefix[y + 1] = prefix[y] + (tmp[y * w + x] > 0) as u32;
        }
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r + 1).min(h));
            out[y * w + x] = if prefix[b] > prefix[a] { 255 } else { 0 };
        }
    }
    out
}

fn bounds(mask: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

impl Inpainter {
    /// `rgba` and `coverage` cover the region returned by [`plan_region`].
    pub fn new(rgba: Vec<u8>, coverage: &[u8], w: u32, h: u32, opts: Options) -> Result<Inpainter, String> {
        let n = (w * h) as usize;
        if rgba.len() != n * 4 || coverage.len() != n {
            return Err(format!("expected {}×{} RGBA and coverage", w, h));
        }
        // A pixel less than half selected is not the object: soft tails of
        // an AI selection (a guided filter's halo, sigmoid tails) would
        // otherwise turn the whole region into a hole. A selection that is
        // soft everywhere (a heavy feather) keeps anything selected.
        let cut = if coverage.iter().any(|&v| v >= 128) { 128 } else { 1 };
        let binary: Vec<u8> = coverage.iter().map(|&v| if v >= cut { 255 } else { 0 }).collect();
        let Some((bx, by, bw, bh)) = bounds(&binary, w, h) else {
            return Err("the selection is empty".into());
        };
        let d = dilation(bw, bh) as usize;
        let hole = dilate(&binary, w as usize, h as usize, d);
        // Soft one-pixel edge on the blend so it never shows a jagged line.
        let soft = crate::blur::gaussian_plane(&hole.iter().map(|v| *v as f32).collect::<Vec<_>>(), w as usize, h as usize, 1.0);
        let alpha: Vec<u8> = soft.iter().zip(&hole).map(|(s, hv)| if *hv > 0 { 255 } else { crate::q8(*s) }).collect();

        let mut crops = Vec::new();
        let side = crop_side(bw, bh);
        let (cx, cy) = ((bx + bw / 2) as i32, (by + bh / 2) as i32);
        let fit = |x: i32, len: u32, max: u32| -> (i32, u32) {
            let len = len.min(max);
            (x.clamp(0, (max - len) as i32), len)
        };
        let square = |x: i32, y: i32, side: u32, checker: Option<(u32, u32)>| {
            let (x, cw) = fit(x, side, w);
            let (y, ch) = fit(y, side, h);
            Crop { x, y, w: cw, h: ch, checker }
        };
        crops.push(square(cx - side as i32 / 2, cy - side as i32 / 2, side, None));

        if opts.refine {
            let mut scale = side as f32 / MODEL_SIZE as f32;
            let (hx, hy, hw, hh) = (bx as i32 - d as i32, by as i32 - d as i32, bw + 2 * d as u32, bh + 2 * d as u32);
            while scale > 2.0 {
                scale = (scale / 2.0).max(1.0);
                let t = (MODEL_SIZE as f32 * scale).round() as u32;
                let step = (t as f32 * 0.7) as u32;
                let count = |len: u32| if len <= t { 1 } else { (len - t).div_ceil(step) + 1 };
                let (nx, ny) = (count(hw), count(hh));
                let runs = 2 * nx * ny;
                if crops.len() as u32 + runs > opts.max_runs {
                    break;
                }
                let block = (48.0 * scale).round().max(8.0) as u32;
                for phase in 0..2 {
                    for j in 0..ny {
                        for i in 0..nx {
                            // Centre small spans; pull the last tile back to the far edge.
                            let place = |start: i32, len: u32, k: u32, n: u32| -> i32 {
                                if len <= t {
                                    start + len as i32 / 2 - t as i32 / 2
                                } else if k + 1 == n {
                                    start + len as i32 - t as i32
                                } else {
                                    start + (k * step) as i32
                                }
                            };
                            crops.push(square(place(hx, hw, i, nx), place(hy, hh, j, ny), t, Some((phase, block))));
                        }
                    }
                }
            }
        }
        Ok(Inpainter { w, h, work: rgba.clone(), orig: rgba, hole, alpha, crops, opts })
    }

    /// Share of pass 1's model input that is hole. MI-GAN is good up to a
    /// few percent (brush strokes, small objects); beyond that LaMa's
    /// global receptive field is what keeps structure (roads, kerbs) going.
    pub fn hole_fraction(&self) -> f32 {
        let c = self.crops[0];
        let mut n = 0u64;
        for y in c.y..c.y + c.h as i32 {
            for x in c.x..c.x + c.w as i32 {
                n += (self.hole[(y as u32 * self.w + x as u32) as usize] > 0) as u64;
            }
        }
        n as f32 / (c.w as f32 * c.h as f32)
    }

    pub fn crops(&self) -> &[Crop] {
        &self.crops
    }

    fn in_pattern(c: &Crop, x: i32, y: i32) -> bool {
        match c.checker {
            None => true,
            Some((phase, block)) => ((x.div_euclid(block as i32) + y.div_euclid(block as i32)).rem_euclid(2)) as u32 == phase,
        }
    }

    /// The crop as model input: `(rgb, hole)` at 512², `rgb` interleaved
    /// bytes, `hole` 255 where the model must invent pixels.
    pub fn input(&self, i: usize) -> (Vec<u8>, Vec<u8>) {
        let c = self.crops[i];
        let (cw, ch) = (c.w as usize, c.h as usize);
        let mut rgba = vec![0u8; cw * ch * 4];
        let mut m = vec![0u8; cw * ch];
        for yy in 0..ch {
            let ry = c.y + yy as i32;
            for xx in 0..cw {
                let rx = c.x + xx as i32;
                let src = (ry as u32 * self.w + rx as u32) as usize;
                let o = yy * cw + xx;
                rgba[o * 4..o * 4 + 3].copy_from_slice(&self.work[src * 4..src * 4 + 3]);
                rgba[o * 4 + 3] = 255;
                if self.hole[src] > 0 && Self::in_pattern(&c, rx, ry) {
                    m[o] = 255;
                }
            }
        }
        let ms = MODEL_SIZE as usize;
        let down = cw > ms || ch > ms;
        let small = resize_raw(&rgba, cw, ch, 4, ms, ms, if down { Resample::Lanczos } else { Resample::Bicubic });
        // Any coverage at all is a hole: a half-covered model pixel still
        // contains part of the object.
        let hole: Vec<u8> = resize_u8(&m, c.w, c.h, MODEL_SIZE, MODEL_SIZE, Resample::Bilinear).iter().map(|&v| if v > 0 { 255 } else { 0 }).collect();
        // Both models leave a faint seam at the boundary of what they were
        // asked to fill; asking for two model pixels more than is pasted
        // puts that seam where it is never used.
        let hole = dilate(&hole, ms, ms, 2);
        // The object itself never reaches the model: MI-GAN's pipeline
        // leaks hole pixels into its output, and a removed walker came back
        // as a ghost. Mid-grey is the value its training masked to.
        let rgb: Vec<u8> = small
            .chunks_exact(4)
            .zip(&hole)
            .flat_map(|(p, &hv)| if hv > 0 { [128, 128, 128] } else { [p[0], p[1], p[2]] })
            .collect();
        (rgb, hole)
    }

    /// Blend the model's output (`rgb`, 512² interleaved) for crop `i` into
    /// the working buffer.
    pub fn apply(&mut self, i: usize, rgb: &[u8]) {
        let c = self.crops[i];
        let ms = MODEL_SIZE as usize;
        let (cw, ch) = (c.w as usize, c.h as usize);
        let rgba: Vec<u8> = rgb[..ms * ms * 3].chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let up = cw > ms || ch > ms;
        let big = resize_raw(&rgba, ms, ms, 4, cw, ch, if up { Resample::Bicubic } else { Resample::Lanczos });
        for yy in 0..ch {
            let ry = c.y + yy as i32;
            for xx in 0..cw {
                let rx = c.x + xx as i32;
                if !Self::in_pattern(&c, rx, ry) {
                    continue;
                }
                let dst = (ry as u32 * self.w + rx as u32) as usize;
                let a = self.alpha[dst] as u32;
                if a == 0 {
                    continue;
                }
                let o = (yy * cw + xx) * 4;
                for k in 0..3 {
                    let base = self.orig[dst * 4 + k] as u32;
                    self.work[dst * 4 + k] = ((base * (255 - a) + big[o + k] as u32 * a + 127) / 255) as u8;
                }
            }
        }
    }

    /// The finished region as straight RGBA.
    pub fn finish(mut self) -> Vec<u8> {
        if self.opts.grain {
            self.match_grain();
        }
        self.work
    }

    fn match_grain(&mut self) {
        let (w, h) = (self.w as usize, self.h as usize);
        let Some((_, _, bw, bh)) = bounds(&self.hole, self.w, self.h) else { return };
        let ring_r = (bw.max(bh) as usize / 6).clamp(6, 64);
        let outer = dilate(&self.hole, w, h, ring_r);
        let residual_sigma = |buf: &[u8], select: &dyn Fn(usize) -> bool| -> f32 {
            let luma: Vec<f32> = buf.chunks_exact(4).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect();
            let blurred = crate::blur::gaussian_plane(&luma, w, h, 1.0);
            let (mut s, mut n) = (0.0f64, 0u64);
            for i in 0..w * h {
                if select(i) {
                    let r = (luma[i] - blurred[i]) as f64;
                    s += r * r;
                    n += 1;
                }
            }
            if n < 16 { 0.0 } else { (s / n as f64).sqrt() as f32 }
        };
        let hole = &self.hole;
        let ring = residual_sigma(&self.orig, &|i| outer[i] > 0 && hole[i] == 0);
        let fill = residual_sigma(&self.work, &|i| hole[i] > 0);
        let add = (ring * ring - fill * fill).max(0.0).sqrt();
        if add < 0.4 {
            return;
        }
        // Residual sigma after a σ=1 blur underestimates white noise by about
        // this much; calibrated on synthetic Gaussian noise in the tests.
        let add = add * 1.25;
        let mut seed: u32 = 0x9E37_79B9;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 + 0.5) / (u32::MAX as f32 + 1.0)
        };
        for i in 0..w * h {
            let a = self.alpha[i];
            if a == 0 {
                continue;
            }
            let (u1, u2) = (next(), next());
            let g = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
            let v = g * add * a as f32 / 255.0;
            for ch in 0..3 {
                self.work[i * 4 + ch] = crate::q8(self.work[i * 4 + ch] as f32 + v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region_with_hole(w: u32, h: u32, hole: (u32, u32, u32, u32)) -> (Vec<u8>, Vec<u8>) {
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let mut cov = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[(x % 256) as u8, (y % 256) as u8, 100, 255]);
                if x >= hole.0 && x < hole.0 + hole.2 && y >= hole.1 && y < hole.1 + hole.3 {
                    cov[i] = 255;
                }
            }
        }
        (rgba, cov)
    }

    #[test]
    fn region_is_a_context_square_clamped_to_the_document() {
        let r = plan_region(4000, 3000, Region { x: 1000, y: 1000, w: 60, h: 40 });
        assert_eq!((r.w, r.h), (512, 512), "small holes read a native 512 square");
        assert!(r.x <= 1000 && r.x + r.w as i32 >= 1060 && r.y <= 1000);
        let edge = plan_region(800, 600, Region { x: 0, y: 0, w: 30, h: 30 });
        assert_eq!((edge.x, edge.y), (0, 0));
        assert!(edge.w <= 800 && edge.h <= 600);
        let big = plan_region(4000, 3000, Region { x: 1000, y: 1000, w: 600, h: 400 });
        assert!(big.w >= 1000, "context around a large hole: {big:?}");
    }

    /// The crop → model → paste geometry: an identity "model" (it returns its
    /// input) must leave every pixel where it was, and nothing outside the
    /// dilated hole may change even with a model that paints white.
    #[test]
    fn crop_and_paste_geometry() {
        let (w, h) = (700u32, 520u32);
        let (rgba, cov) = region_with_hole(w, h, (300, 200, 50, 40));
        let opts = Options { grain: false, ..Default::default() };
        let mut ip = Inpainter::new(rgba.clone(), &cov, w, h, opts).unwrap();
        assert_eq!(ip.crops().len(), 1);
        let (rgb, hole) = ip.input(0);
        assert_eq!(rgb.len(), 512 * 512 * 3);
        // The hole lands where the crop maps it: centred horizontally, and
        // shifted up where the square was pushed inside the region's top.
        let c = ip.crops()[0];
        assert_eq!((c.x, c.y, c.w, c.h), (69, 0, 512, 512));
        let hb = bounds(&hole, 512, 512).unwrap();
        let (hx, hy) = (hb.0 + hb.2 / 2, hb.1 + hb.3 / 2);
        assert!((hx as i32 - 256).abs() <= 2 && (hy as i32 - 220).abs() <= 2, "{hb:?}");
        // An identity "model": the photo itself, as the crop sees it (the
        // input has the hole greyed out, so it cannot serve).
        let crop = ip.crops()[0];
        let ident: Vec<u8> = (0..512usize * 512)
            .flat_map(|k| {
                let (x, y) = ((k % 512) as i32 + crop.x, (k / 512) as i32 + crop.y);
                let i = (y as u32 * w + x as u32) as usize * 4;
                [rgba[i], rgba[i + 1], rgba[i + 2]]
            })
            .collect();
        assert!(rgb.chunks_exact(3).zip(&hole).all(|(p, h)| *h == 0 || p == [128, 128, 128]), "hole pixels reach the model");
        ip.apply(0, &ident);
        let out = ip.finish();
        let max_err = out.iter().zip(&rgba).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(max_err <= 2, "identity model moved pixels by {max_err}");

        let mut ip = Inpainter::new(rgba.clone(), &cov, w, h, opts).unwrap();
        ip.apply(0, &vec![255u8; 512 * 512 * 3]);
        let out = ip.finish();
        let d = dilation(50, 40) as i32;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as u32 * w + x as u32) as usize * 4;
                let inside_hole = (300..350).contains(&x) && (200..240).contains(&y);
                let near = (300 - d - 4..350 + d + 4).contains(&x) && (200 - d - 4..240 + d + 4).contains(&y);
                if inside_hole {
                    assert_eq!(&out[i..i + 3], &[255, 255, 255], "hole pixel {x},{y} not filled");
                } else if !near {
                    assert_eq!(&out[i..i + 4], &rgba[i..i + 4], "pixel {x},{y} outside the hole changed");
                }
            }
        }
    }

    #[test]
    fn crops_never_leave_the_region_and_soft_tails_are_not_holes() {
        // A tall hole in a short document: the ideal square does not fit.
        let (w, h) = (1200u32, 800u32);
        let (rgba, mut cov) = region_with_hole(w, h, (500, 50, 120, 700));
        // A faint halo over the whole region, as a guided filter leaves.
        for v in cov.iter_mut() {
            if *v == 0 {
                *v = 20;
            }
        }
        let ip = Inpainter::new(rgba, &cov, w, h, Options::default()).unwrap();
        for c in ip.crops() {
            assert!(c.x >= 0 && c.y >= 0 && c.x as u32 + c.w <= w && c.y as u32 + c.h <= h, "{c:?}");
        }
        let (_, hole) = ip.input(0);
        let frac = hole.iter().filter(|v| **v > 0).count() as f32 / hole.len() as f32;
        assert!(frac < 0.2, "the faint halo became a hole: {frac}");
    }

    #[test]
    fn large_holes_get_bounded_refinement_passes() {
        let (w, h) = (3000u32, 2400u32);
        let (rgba, cov) = region_with_hole(w, h, (700, 600, 1400, 1100));
        let ip = Inpainter::new(rgba, &cov, w, h, Options { refine: true, ..Default::default() }).unwrap();
        assert!(ip.hole_fraction() > 0.2);
        let crops = ip.crops();
        assert!(crops.len() > 1 && crops.len() <= 12, "{} crops", crops.len());
        assert!(crops[0].checker.is_none());
        // Refinement tiles cover the whole dilated hole, both phases.
        for phase in 0..2 {
            let tiles: Vec<_> = crops.iter().filter(|c| c.checker.map(|p| p.0) == Some(phase)).collect();
            assert!(!tiles.is_empty());
            for (x, y) in [(700, 600), (2099, 1699), (1400, 1150)] {
                assert!(tiles.iter().any(|c| x >= c.x && x < c.x + c.w as i32 && y >= c.y && y < c.y + c.h as i32), "({x},{y}) uncovered in phase {phase}");
            }
        }
    }

    #[test]
    fn grain_is_added_to_a_smooth_fill_in_a_noisy_photo() {
        let (w, h) = (400u32, 400u32);
        let mut seed = 12345u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_exact_mut(4) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let n = (seed % 41) as i32 - 20;
            let v = (128 + n) as u8;
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let mut cov = vec![0u8; (w * h) as usize];
        for y in 150..250 {
            for x in 150..250 {
                cov[(y * w + x) as usize] = 255;
            }
        }
        let mut ip = Inpainter::new(rgba.clone(), &cov, w, h, Options::default()).unwrap();
        ip.apply(0, &vec![128u8; 512 * 512 * 3]);
        let out = ip.finish();
        let sd = |buf: &[u8]| {
            let vals: Vec<f32> = (170..230).flat_map(|y| (170..230).map(move |x| (y, x))).map(|(y, x)| buf[((y * w + x) * 4) as usize] as f32).collect();
            let m = vals.iter().sum::<f32>() / vals.len() as f32;
            (vals.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / vals.len() as f32).sqrt()
        };
        let (got, want) = (sd(&out), sd(&rgba));
        assert!(got > want * 0.6 && got < want * 1.5, "fill noise {got}, photo noise {want}");
    }
}
