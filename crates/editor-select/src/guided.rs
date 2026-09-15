//! The colour-guided filter (He, Sun & Tang) that snaps a rough mask to the
//! edges of the image, and a block-wise driver that runs it only where a
//! band of pixels needs it, so a 12 MP subject mask never allocates a
//! 12 MP stack of float images.

use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::pixels::read_merged;

/// Mean over the `(2r+1)²` window clipped to the buffer, in O(n).
pub fn box_mean(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0f32; src.len()];
    let mut out = vec![0f32; src.len()];
    if w == 0 || h == 0 {
        return out;
    }
    // Horizontal sums.
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let mut prefix = 0f64;
        let mut acc = Vec::with_capacity(w + 1);
        acc.push(0f64);
        for &v in row {
            prefix += v as f64;
            acc.push(prefix);
        }
        for x in 0..w {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r + 1).min(w);
            tmp[y * w + x] = ((acc[x1] - acc[x0]) / (x1 - x0) as f64) as f32;
        }
    }
    // Vertical sums of the horizontal means.
    let mut acc = vec![0f64; h + 1];
    for x in 0..w {
        let mut prefix = 0f64;
        for y in 0..h {
            prefix += tmp[y * w + x] as f64;
            acc[y + 1] = prefix;
        }
        for y in 0..h {
            let y0 = y.saturating_sub(r);
            let y1 = (y + r + 1).min(h);
            out[y * w + x] = ((acc[y1] - acc[y0]) / (y1 - y0) as f64) as f32;
        }
    }
    out
}

/// Guided filter of `p` with a colour guide `guide` (RGB triples, 0..1).
pub fn guided_rgb(guide: &[f32], p: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let [ar, ag, ab, b] = coefficients(guide, p, w, h, r, eps);
    (0..w * h).map(|i| ar[i] * guide[i * 3] + ag[i] * guide[i * 3 + 1] + ab[i] * guide[i * 3 + 2] + b[i]).collect()
}

/// The fast guided filter: coefficients solved on a grid subsampled by
/// `sub`, upsampled bilinearly and applied to the full-resolution guide.
/// Costs about `1/sub²` of [`guided_rgb`] with near-identical output for
/// radii well above `sub`.
pub fn guided_rgb_fast(guide: &[f32], p: &[f32], w: usize, h: usize, r: usize, eps: f32, sub: usize) -> Vec<f32> {
    if sub <= 1 || w < 2 * sub || h < 2 * sub {
        return guided_rgb(guide, p, w, h, r, eps);
    }
    let (sw, sh) = (w.div_ceil(sub), h.div_ceil(sub));
    let mut g = vec![0f32; sw * sh * 3];
    let mut q = vec![0f32; sw * sh];
    let mut n = vec![0f32; sw * sh];
    for y in 0..h {
        for x in 0..w {
            let si = (y / sub) * sw + x / sub;
            let i = y * w + x;
            g[si * 3] += guide[i * 3];
            g[si * 3 + 1] += guide[i * 3 + 1];
            g[si * 3 + 2] += guide[i * 3 + 2];
            q[si] += p[i];
            n[si] += 1.0;
        }
    }
    for i in 0..sw * sh {
        g[i * 3] /= n[i];
        g[i * 3 + 1] /= n[i];
        g[i * 3 + 2] /= n[i];
        q[i] /= n[i];
    }
    let coef = coefficients(&g, &q, sw, sh, (r / sub).max(1), eps);
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) / sub as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let y0 = (fy.floor() as usize).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5) / sub as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let x0 = (fx.floor() as usize).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f32;
            let lerp = |c: &[f32]| {
                let top = c[y0 * sw + x0] * (1.0 - tx) + c[y0 * sw + x1] * tx;
                let bot = c[y1 * sw + x0] * (1.0 - tx) + c[y1 * sw + x1] * tx;
                top * (1.0 - ty) + bot * ty
            };
            let i = y * w + x;
            out[i] = lerp(&coef[0]) * guide[i * 3] + lerp(&coef[1]) * guide[i * 3 + 1] + lerp(&coef[2]) * guide[i * 3 + 2] + lerp(&coef[3]);
        }
    }
    out
}

/// Window-averaged linear coefficients `(a_r, a_g, a_b, b)` of the guided
/// filter.
fn coefficients(guide: &[f32], p: &[f32], w: usize, h: usize, r: usize, eps: f32) -> [Vec<f32>; 4] {
    let n = w * h;
    let ch = |c: usize| -> Vec<f32> { (0..n).map(|i| guide[i * 3 + c]).collect() };
    let (ir, ig, ib) = (ch(0), ch(1), ch(2));
    let mean = |v: &[f32]| box_mean(v, w, h, r);
    let prod = |a: &[f32], b: &[f32]| -> Vec<f32> { a.iter().zip(b.iter()).map(|(x, y)| x * y).collect() };
    let (mr, mg, mb) = (mean(&ir), mean(&ig), mean(&ib));
    let mp = mean(p);
    let (mrp, mgp, mbp) = (mean(&prod(&ir, p)), mean(&prod(&ig, p)), mean(&prod(&ib, p)));
    let (mrr, mrg, mrb) = (mean(&prod(&ir, &ir)), mean(&prod(&ir, &ig)), mean(&prod(&ir, &ib)));
    let (mgg, mgb, mbb) = (mean(&prod(&ig, &ig)), mean(&prod(&ig, &ib)), mean(&prod(&ib, &ib)));
    let mut ar = vec![0f32; n];
    let mut ag = vec![0f32; n];
    let mut ab = vec![0f32; n];
    let mut b = vec![0f32; n];
    for i in 0..n {
        let (cr, cg, cb) = (mrp[i] - mr[i] * mp[i], mgp[i] - mg[i] * mp[i], mbp[i] - mb[i] * mp[i]);
        let srr = (mrr[i] - mr[i] * mr[i] + eps) as f64;
        let srg = (mrg[i] - mr[i] * mg[i]) as f64;
        let srb = (mrb[i] - mr[i] * mb[i]) as f64;
        let sgg = (mgg[i] - mg[i] * mg[i] + eps) as f64;
        let sgb = (mgb[i] - mg[i] * mb[i]) as f64;
        let sbb = (mbb[i] - mb[i] * mb[i] + eps) as f64;
        // Inverse of the symmetric 3×3 covariance.
        let i00 = sgg * sbb - sgb * sgb;
        let i01 = srb * sgb - srg * sbb;
        let i02 = srg * sgb - srb * sgg;
        let i11 = srr * sbb - srb * srb;
        let i12 = srb * srg - srr * sgb;
        let i22 = srr * sgg - srg * srg;
        let det = srr * i00 + srg * i01 + srb * i02;
        let (cr, cg, cb) = (cr as f64, cg as f64, cb as f64);
        let (a0, a1, a2) = if det.abs() > 1e-18 {
            ((i00 * cr + i01 * cg + i02 * cb) / det, (i01 * cr + i11 * cg + i12 * cb) / det, (i02 * cr + i12 * cg + i22 * cb) / det)
        } else {
            (0.0, 0.0, 0.0)
        };
        ar[i] = a0 as f32;
        ag[i] = a1 as f32;
        ab[i] = a2 as f32;
        b[i] = mp[i] - ar[i] * mr[i] - ag[i] * mg[i] - ab[i] * mb[i];
    }
    [mean(&ar), mean(&ag), mean(&ab), mean(&b)]
}

/// Merged image over `rect` as RGB floats 0..1.
pub fn merged_rgb(doc: &Document, rect: Rect) -> Vec<f32> {
    let px = read_merged(doc, rect);
    let mut out = Vec::with_capacity(rect.area().max(0) as usize * 3);
    for p in px.chunks_exact(4) {
        out.extend_from_slice(&[p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]);
    }
    out
}

/// Re-estimate `mask` (0..1 over `area`) with a guided filter where
/// `weight` is positive: each iteration sets `p = known·(1−w) + q·w`, where
/// `known` is the incoming mask and `q` the filtered `p`, so confident pixels
/// outside the band propagate their colours inward. Works in blocks with an
/// apron so each block's filter sees its neighbourhood.
pub fn refine_band(doc: &Document, area: Rect, mask: &mut [f32], weight: &[f32], r: usize, eps: f32, iterations: usize) {
    // Small blocks skip more of the area a thin band does not touch.
    let block_side = ((r as i32) * 8).clamp(64, 256);
    let apron = (r as i32) * 2 + 1;
    let sub = if r >= 8 { r / 4 } else if r >= 4 { 2 } else { 1 };
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    let src = mask.to_vec();
    let mut by = 0;
    while by < area.h {
        let bh = block_side.min(area.h - by);
        let mut bx = 0;
        while bx < area.w {
            let bw = block_side.min(area.w - bx);
            let block = Rect::new(area.x + bx, area.y + by, bw, bh);
            let any = (0..bh).any(|y| (0..bw).any(|x| weight[((by + y) * area.w + bx + x) as usize] > 0.0));
            if any {
                let outer = block.inflate(apron).intersect(&canvas).intersect(&area);
                let (ow, oh) = (outer.w as usize, outer.h as usize);
                let guide = merged_rgb(doc, outer);
                let mut known = vec![0f32; ow * oh];
                let mut wt = vec![0f32; ow * oh];
                for y in 0..oh {
                    for x in 0..ow {
                        let ai = ((outer.y - area.y) as usize + y) * area.w as usize + (outer.x - area.x) as usize + x;
                        known[y * ow + x] = src[ai];
                        wt[y * ow + x] = weight[ai].clamp(0.0, 1.0);
                    }
                }
                let mut p = known.clone();
                for _ in 0..iterations.max(1) {
                    let q = guided_rgb_fast(&guide, &p, ow, oh, r, eps, sub);
                    for i in 0..ow * oh {
                        if wt[i] > 0.0 {
                            p[i] = known[i] + (q[i].clamp(0.0, 1.0) - known[i]) * wt[i];
                        }
                    }
                }
                for y in 0..bh {
                    for x in 0..bw {
                        let ai = ((by + y) * area.w + bx + x) as usize;
                        if weight[ai] > 0.0 {
                            mask[ai] = p[((block.y + y - outer.y) * outer.w + (block.x + x - outer.x)) as usize];
                        }
                    }
                }
            }
            bx += bw;
        }
        by += bh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mean_of_constant_is_constant() {
        let v = vec![0.25f32; 7 * 5];
        let m = box_mean(&v, 7, 5, 3);
        assert!(m.iter().all(|x| (x - 0.25).abs() < 1e-6));
    }

    #[test]
    fn box_mean_matches_brute_force() {
        let (w, h, r) = (9usize, 6usize, 2usize);
        let v: Vec<f32> = (0..w * h).map(|i| ((i * 37) % 11) as f32).collect();
        let m = box_mean(&v, w, h, r);
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.0;
                let mut n = 0.0;
                for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                    for xx in x.saturating_sub(r)..(x + r + 1).min(w) {
                        s += v[yy * w + xx];
                        n += 1.0;
                    }
                }
                assert!((m[y * w + x] - s / n).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn fast_guided_filter_tracks_the_exact_one() {
        let (w, h) = (96usize, 64usize);
        let guide: Vec<f32> = (0..w * h * 3).map(|i| (((i / 3) % w) as f32 / w as f32 + ((i % 3) as f32) * 0.1).fract()).collect();
        let p: Vec<f32> = (0..w * h).map(|i| if (i % w) < 50 { 1.0 } else { 0.0 }).collect();
        let exact = guided_rgb(&guide, &p, w, h, 16, 1e-3);
        let fast = guided_rgb_fast(&guide, &p, w, h, 16, 1e-3, 4);
        let err = exact.iter().zip(fast.iter()).map(|(a, b)| (a - b).abs()).sum::<f32>() / (w * h) as f32;
        assert!(err < 0.03, "mean abs error {err}");
    }

    #[test]
    fn guided_filter_snaps_mask_to_colour_edge() {
        // Guide: red left of x = 12, blue right. Mask: wrongly split at x = 9.
        let (w, h) = (24usize, 8usize);
        let mut guide = vec![0f32; w * h * 3];
        let mut p = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x < 12 {
                    guide[i * 3] = 0.8;
                    guide[i * 3 + 2] = 0.1;
                } else {
                    guide[i * 3] = 0.1;
                    guide[i * 3 + 2] = 0.8;
                }
                p[i] = if x < 9 { 1.0 } else { 0.0 };
            }
        }
        let q = guided_rgb(&guide, &p, w, h, 6, 1e-4);
        let row = 4 * w;
        assert!(q[row + 10] > 0.5, "red pixel past the rough edge joins: {}", q[row + 10]);
        assert!(q[row + 13] < 0.2, "blue stays out: {}", q[row + 13]);
    }
}
