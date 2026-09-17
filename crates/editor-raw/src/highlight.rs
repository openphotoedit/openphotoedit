//! Highlight reconstruction on white-balanced camera RGB.
//!
//! After white balance the three channels clip at different levels (green
//! at 1, red and blue at their multipliers), so a blown sky turns magenta.
//! This follows the idea of darktable's "inpaint opposed": a clipped channel
//! is at least what the other two channels say it should be, corrected by
//! the typical chroma offset of unclipped pixels around the clipped regions.
//! Where every channel is clipped the colour is unknowable, and the pixel is
//! blended to neutral at its brightest value.

use rayon::prelude::*;

use crate::Rgb;

/// `thr[c]` is where channel `c` clipped (values were clamped to it).
pub fn reconstruct(img: &mut Rgb, thr: [f32; 3]) {
    let (w, h) = (img.w, img.h);
    let lim = [thr[0] * 0.995, thr[1] * 0.995, thr[2] * 0.995];
    let clipped = |p: &[f32]| [p[0] >= lim[0], p[1] >= lim[1], p[2] >= lim[2]];

    // Blocks that contain clipping, dilated by one block.
    const B: usize = 16;
    let (bw, bh) = (w.div_ceil(B), h.div_ceil(B));
    let mut block = vec![false; bw * bh];
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            let p = &img.data[(y * w + x) * 3..(y * w + x) * 3 + 3];
            if clipped(p).iter().any(|c| *c) {
                block[(y / B) * bw + x / B] = true;
                any = true;
            }
        }
    }
    if !any {
        return;
    }
    let mut near = vec![false; bw * bh];
    for by in 0..bh {
        for bx in 0..bw {
            if block[by * bw + bx] {
                for yy in by.saturating_sub(1)..(by + 2).min(bh) {
                    for xx in bx.saturating_sub(1)..(bx + 2).min(bw) {
                        near[yy * bw + xx] = true;
                    }
                }
            }
        }
    }

    // Chroma offset of each channel against the mean of the other two, over
    // bright unclipped pixels next to clipped areas.
    let mut acc = [0f64; 3];
    let mut n = 0u64;
    for y in 0..h {
        for x in 0..w {
            if !near[(y / B) * bw + x / B] {
                continue;
            }
            let p = &img.data[(y * w + x) * 3..(y * w + x) * 3 + 3];
            if clipped(p).iter().any(|c| *c) {
                continue;
            }
            let rel = (p[0] / thr[0]).max(p[1] / thr[1]).max(p[2] / thr[2]);
            if rel < 0.5 {
                continue;
            }
            for c in 0..3 {
                let opp = (p[(c + 1) % 3] + p[(c + 2) % 3]) * 0.5;
                acc[c] += (p[c] - opp) as f64;
            }
            n += 1;
        }
    }
    let chroma = if n > 0 { [(acc[0] / n as f64) as f32, (acc[1] / n as f64) as f32, (acc[2] / n as f64) as f32] } else { [0.0; 3] };

    img.data.par_chunks_mut(3).for_each(|p| {
        let cl = clipped(p);
        if !cl.iter().any(|c| *c) {
            return;
        }
        let orig = [p[0], p[1], p[2]];
        for c in 0..3 {
            if cl[c] {
                let opp = (orig[(c + 1) % 3] + orig[(c + 2) % 3]) * 0.5;
                p[c] = p[c].max(opp + chroma[c]);
            }
        }
        // All channels near their clip: colour is unknown, go neutral.
        let rel_min = (orig[0] / thr[0]).min(orig[1] / thr[1]).min(orig[2] / thr[2]);
        let k = ((rel_min - 0.85) / 0.15).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        if k > 0.0 {
            let m = p[0].max(p[1]).max(p[2]);
            for v in p.iter_mut() {
                *v += (m - *v) * k;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_clipped_goes_neutral_not_magenta() {
        // Green clips at 1, red at 2.1, blue at 1.6 (typical daylight WB).
        let thr = [2.1, 1.0, 1.6];
        let mut img = Rgb { w: 32, h: 32, data: Vec::new() };
        for y in 0..32 {
            for x in 0..32 {
                let p = if (8..24).contains(&x) && (8..24).contains(&y) { thr } else { [1.2, 0.6, 0.9] };
                img.data.extend_from_slice(&p);
            }
        }
        reconstruct(&mut img, thr);
        let p = &img.data[(16 * 32 + 16) * 3..(16 * 32 + 16) * 3 + 3];
        let spread = p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
        assert!(spread < 1e-3, "{p:?}");
        assert!(p[1] >= 2.0, "brightness kept: {p:?}");
    }

    #[test]
    fn partially_clipped_green_is_raised_from_neighbours() {
        let thr = [2.0, 1.0, 2.0];
        let mut img = Rgb { w: 16, h: 16, data: Vec::new() };
        for i in 0..256 {
            // Neutral-ish surroundings with g ≈ mean(r, b); centre has g clipped.
            let p = if i == 8 * 16 + 8 { [1.5, 1.0, 1.5] } else { [0.8, 0.8, 0.8] };
            img.data.extend_from_slice(&p);
        }
        reconstruct(&mut img, thr);
        let g = img.data[(8 * 16 + 8) * 3 + 1];
        assert!(g > 1.4, "{g}");
    }
}
