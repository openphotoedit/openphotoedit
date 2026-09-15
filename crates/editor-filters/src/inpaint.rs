//! Exemplar-based inpainting: multi-scale expectation–maximisation over
//! PatchMatch nearest-neighbour fields (Wexler, Shechtman & Irani 2007;
//! Barnes et al. 2009), the family Content-Aware Fill belongs to.
//!
//! At every pyramid level each patch overlapping the hole finds its most
//! similar fully-known patch (PatchMatch: propagation plus random search),
//! then every hole pixel becomes the weighted vote of the patches covering
//! it. The coarsest level starts from a diffusion fill; each finer level
//! starts from the upsampled field of the level below.

use crate::util::Rng;

const PR: isize = 3; // 7×7 patches

struct Level {
    w: usize,
    h: usize,
    /// Straight RGBA 0..255.
    img: Vec<f32>,
    hole: Vec<bool>,
    /// Inside the canvas (pixels outside take no part in comparisons).
    inside: Vec<bool>,
    /// May serve as part of a source patch.
    usable: Vec<bool>,
}

impl Level {
    fn downsample(&self) -> Level {
        let (w, h) = (self.w.div_ceil(2), self.h.div_ceil(2));
        let n = w * h;
        let mut img = vec![0f32; n * 4];
        let mut hole = vec![false; n];
        let mut inside = vec![false; n];
        let mut usable = vec![true; n];
        for y in 0..h {
            for x in 0..w {
                let o = y * w + x;
                let mut acc = [0f32; 4];
                let mut cnt = 0f32;
                let mut any_inside = false;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx).min(self.w - 1), (2 * y + dy).min(self.h - 1));
                    let i = sy * self.w + sx;
                    hole[o] |= self.hole[i];
                    usable[o] &= self.usable[i];
                    any_inside |= self.inside[i];
                    if !self.hole[i] {
                        for c in 0..4 {
                            acc[c] += self.img[i * 4 + c];
                        }
                        cnt += 1.0;
                    }
                }
                inside[o] = any_inside;
                usable[o] &= !hole[o] && any_inside;
                if cnt > 0.0 {
                    for c in 0..4 {
                        img[o * 4 + c] = acc[c] / cnt;
                    }
                }
            }
        }
        Level { w, h, img, hole, inside, usable }
    }

    /// Largest distance (in pixels, chessboard) from a hole pixel to the
    /// nearest known pixel.
    fn hole_depth(&self) -> usize {
        let mut d: Vec<u32> = self.hole.iter().map(|&h| if h { u32::MAX / 2 } else { 0 }).collect();
        let (w, h) = (self.w, self.h);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if d[i] == 0 {
                    continue;
                }
                let mut m = d[i];
                if x > 0 {
                    m = m.min(d[i - 1] + 1);
                }
                if y > 0 {
                    m = m.min(d[i - w] + 1);
                    if x > 0 {
                        m = m.min(d[i - w - 1] + 1);
                    }
                    if x + 1 < w {
                        m = m.min(d[i - w + 1] + 1);
                    }
                }
                d[i] = m;
            }
        }
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let i = y * w + x;
                if d[i] == 0 {
                    continue;
                }
                let mut m = d[i];
                if x + 1 < w {
                    m = m.min(d[i + 1] + 1);
                }
                if y + 1 < h {
                    m = m.min(d[i + w] + 1);
                    if x + 1 < w {
                        m = m.min(d[i + w + 1] + 1);
                    }
                    if x > 0 {
                        m = m.min(d[i + w - 1] + 1);
                    }
                }
                d[i] = m;
            }
        }
        d.iter().copied().filter(|&v| v < u32::MAX / 4).max().unwrap_or(0) as usize
    }

    fn distance_to_known(&self) -> Vec<f32> {
        // Two-pass chamfer (1, √2).
        let (w, h) = (self.w, self.h);
        let mut d: Vec<f32> = self.hole.iter().map(|&h| if h { 1e9 } else { 0.0 }).collect();
        const D: f32 = std::f32::consts::SQRT_2;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if d[i] == 0.0 {
                    continue;
                }
                let mut m = d[i];
                if x > 0 {
                    m = m.min(d[i - 1] + 1.0);
                }
                if y > 0 {
                    m = m.min(d[i - w] + 1.0);
                    if x > 0 {
                        m = m.min(d[i - w - 1] + D);
                    }
                    if x + 1 < w {
                        m = m.min(d[i - w + 1] + D);
                    }
                }
                d[i] = m;
            }
        }
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let i = y * w + x;
                if d[i] == 0.0 {
                    continue;
                }
                let mut m = d[i];
                if x + 1 < w {
                    m = m.min(d[i + 1] + 1.0);
                }
                if y + 1 < h {
                    m = m.min(d[i + w] + 1.0);
                    if x + 1 < w {
                        m = m.min(d[i + w + 1] + D);
                    }
                    if x > 0 {
                        m = m.min(d[i + w - 1] + D);
                    }
                }
                d[i] = m;
            }
        }
        d
    }

    /// Diffusion fill for the coarsest level: peel the hole from its edge
    /// inward, each pixel taking the mean of its known neighbours.
    fn onion_fill(&mut self) {
        let (w, h) = (self.w, self.h);
        let mut known: Vec<bool> = self.hole.iter().map(|&v| !v).collect();
        loop {
            let mut changed = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if known[i] {
                        continue;
                    }
                    let mut acc = [0f32; 4];
                    let mut n = 0f32;
                    for dy in -1isize..=1 {
                        for dx in -1isize..=1 {
                            let (xx, yy) = (x as isize + dx, y as isize + dy);
                            if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                                continue;
                            }
                            let j = yy as usize * w + xx as usize;
                            if known[j] && self.inside[j] {
                                for c in 0..4 {
                                    acc[c] += self.img[j * 4 + c];
                                }
                                n += 1.0;
                            }
                        }
                    }
                    if n > 0.0 {
                        changed.push((i, acc.map(|v| v / n)));
                    }
                }
            }
            if changed.is_empty() {
                break;
            }
            for (i, v) in changed {
                self.img[i * 4..i * 4 + 4].copy_from_slice(&v);
                known[i] = true;
            }
        }
    }
}

struct Field {
    /// Target patch centres, as indices.
    targets: Vec<usize>,
    /// Nearest source centre for each pixel (only meaningful for targets).
    nn: Vec<u32>,
    dist: Vec<f32>,
    /// Valid source centres (whole patch usable).
    src_ok: Vec<bool>,
    src_list: Vec<u32>,
}

fn patch_distance(lv: &Level, p: usize, q: usize, best: f32) -> f32 {
    let w = lv.w as isize;
    let (px, py) = ((p % lv.w) as isize, (p / lv.w) as isize);
    let (qx, qy) = ((q % lv.w) as isize, (q / lv.w) as isize);
    let mut ssd = 0f32;
    for dy in -PR..=PR {
        let (ty, sy) = (py + dy, qy + dy);
        if ty < 0 || ty >= lv.h as isize {
            continue;
        }
        for dx in -PR..=PR {
            let tx = px + dx;
            if tx < 0 || tx >= w {
                continue;
            }
            let ti = (ty * w + tx) as usize;
            if !lv.inside[ti] {
                continue;
            }
            let si = (sy * w + qx + dx) as usize;
            let (a, b) = (&lv.img[ti * 4..ti * 4 + 3], &lv.img[si * 4..si * 4 + 3]);
            let d0 = a[0] - b[0];
            let d1 = a[1] - b[1];
            let d2 = a[2] - b[2];
            ssd += d0 * d0 + d1 * d1 + d2 * d2;
        }
        if ssd >= best {
            return ssd;
        }
    }
    ssd
}

fn build_field(lv: &Level) -> Field {
    let (w, h) = (lv.w, lv.h);
    let n = w * h;
    // Targets: centres within PR of a hole pixel.
    let mut near = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            if !lv.hole[y * w + x] {
                continue;
            }
            for yy in y.saturating_sub(PR as usize)..(y + PR as usize + 1).min(h) {
                for xx in x.saturating_sub(PR as usize)..(x + PR as usize + 1).min(w) {
                    near[yy * w + xx] = true;
                }
            }
        }
    }
    let targets: Vec<usize> = (0..n).filter(|&i| near[i]).collect();
    // Sources: every pixel of the patch usable.
    let mut src_ok = vec![false; n];
    let pr = PR as usize;
    if w > 2 * pr && h > 2 * pr {
        // Row-wise run lengths of usable pixels, then column-wise.
        let mut row_ok = vec![false; n];
        for y in 0..h {
            let mut run = 0usize;
            for x in 0..w {
                run = if lv.usable[y * w + x] { run + 1 } else { 0 };
                if run > 2 * pr {
                    row_ok[y * w + x - pr] = true;
                }
            }
        }
        for x in 0..w {
            let mut run = 0usize;
            for y in 0..h {
                run = if row_ok[y * w + x] { run + 1 } else { 0 };
                if run > 2 * pr {
                    src_ok[(y - pr) * w + x] = true;
                }
            }
        }
    }
    let src_list: Vec<u32> = (0..n).filter(|&i| src_ok[i]).map(|i| i as u32).collect();
    Field { targets, nn: vec![0; n], dist: vec![f32::MAX; n], src_ok, src_list }
}

fn patchmatch(lv: &Level, f: &mut Field, iters: usize, rng: &mut Rng) {
    let (w, h) = (lv.w as isize, lv.h as isize);
    for &p in &f.targets {
        let q = f.nn[p] as usize;
        f.dist[p] = patch_distance(lv, p, q, f32::MAX);
    }
    let order_len = f.targets.len();
    for it in 0..iters {
        let forward = it % 2 == 0;
        for k in 0..order_len {
            let p = if forward { f.targets[k] } else { f.targets[order_len - 1 - k] };
            let (px, py) = ((p as isize) % w, (p as isize) / w);
            let mut best_q = f.nn[p] as usize;
            let mut best_d = f.dist[p];
            let step: isize = if forward { -1 } else { 1 };
            // Propagation from the previously visited neighbours.
            for (nx, ny) in [(px + step, py), (px, py + step)] {
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let nb = (ny * w + nx) as usize;
                if f.dist[nb] == f32::MAX {
                    continue;
                }
                let nq = f.nn[nb] as isize;
                let (cx, cy) = (nq % w - (nx - px), nq / w - (ny - py));
                if cx < 0 || cy < 0 || cx >= w || cy >= h {
                    continue;
                }
                let cand = (cy * w + cx) as usize;
                if cand == best_q || !f.src_ok[cand] {
                    continue;
                }
                let d = patch_distance(lv, p, cand, best_d);
                if d < best_d {
                    best_d = d;
                    best_q = cand;
                }
            }
            // Random search in shrinking windows around the current best.
            let mut radius = w.max(h);
            let (bx, by) = ((best_q as isize) % w, (best_q as isize) / w);
            while radius >= 1 {
                let x0 = (bx - radius).max(0);
                let x1 = (bx + radius).min(w - 1);
                let y0 = (by - radius).max(0);
                let y1 = (by + radius).min(h - 1);
                let cx = x0 + rng.below((x1 - x0 + 1) as usize) as isize;
                let cy = y0 + rng.below((y1 - y0 + 1) as usize) as isize;
                let cand = (cy * w + cx) as usize;
                if f.src_ok[cand] && cand != best_q {
                    let d = patch_distance(lv, p, cand, best_d);
                    if d < best_d {
                        best_d = d;
                        best_q = cand;
                    }
                }
                radius /= 2;
            }
            f.nn[p] = best_q as u32;
            f.dist[p] = best_d;
        }
    }
}

/// Replace hole pixels with the weighted vote of overlapping patches.
fn vote(lv: &mut Level, f: &Field, conf: &[f32]) {
    let (w, h) = (lv.w as isize, lv.h as isize);
    let n = lv.w * lv.h;
    // Similarity weights scaled by the 75th percentile distance (Wexler).
    let mut ds: Vec<f32> = f.targets.iter().map(|&p| f.dist[p]).filter(|d| d.is_finite() && *d < f32::MAX).collect();
    let sigma2 = if ds.is_empty() {
        1.0
    } else {
        let k = (ds.len() * 3 / 4).min(ds.len() - 1);
        let (_, v, _) = ds.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
        (*v).max(1.0)
    };
    let mut acc = vec![0f32; n * 4];
    let mut wsum = vec![0f32; n];
    for &p in &f.targets {
        let q = f.nn[p] as isize;
        let d = f.dist[p];
        if d == f32::MAX {
            continue;
        }
        let wgt = (-d / (2.0 * sigma2)).exp() * conf[p] + 1e-6;
        let (px, py) = (p as isize % w, p as isize / w);
        let (qx, qy) = (q % w, q / w);
        for dy in -PR..=PR {
            let ty = py + dy;
            if ty < 0 || ty >= h {
                continue;
            }
            for dx in -PR..=PR {
                let tx = px + dx;
                if tx < 0 || tx >= w {
                    continue;
                }
                let ti = (ty * w + tx) as usize;
                if !lv.hole[ti] {
                    continue;
                }
                let si = ((qy + dy) * w + qx + dx) as usize;
                for c in 0..4 {
                    acc[ti * 4 + c] += wgt * lv.img[si * 4 + c];
                }
                wsum[ti] += wgt;
            }
        }
    }
    for i in 0..n {
        if lv.hole[i] && wsum[i] > 0.0 {
            for c in 0..4 {
                lv.img[i * 4 + c] = acc[i * 4 + c] / wsum[i];
            }
        }
    }
}

/// Fill `hole` pixels of a straight RGBA8 buffer from the known pixels.
/// `inside[i]` marks pixels that exist (within the canvas); pixels outside
/// neither compare nor serve as sources. Returns false when there is not
/// enough known texture to copy from.
pub fn inpaint(buf: &mut [u8], w: usize, h: usize, hole: &[bool], inside: &[bool], seed: u64) -> bool {
    let n = w * h;
    if !hole.iter().any(|&v| v) {
        return true;
    }
    let base = Level {
        w,
        h,
        img: buf.iter().map(|&v| v as f32).collect(),
        hole: hole.to_vec(),
        inside: inside.to_vec(),
        usable: (0..n).map(|i| inside[i] && !hole[i]).collect(),
    };
    let mut levels = vec![base];
    loop {
        let lv = levels.last().unwrap();
        if lv.hole_depth() <= 3 || lv.w < 8 * PR as usize || lv.h < 8 * PR as usize {
            break;
        }
        let next = lv.downsample();
        levels.push(next);
    }
    let mut rng = Rng::new(seed ^ 0xC0FF_EE00_D15E_A5E5);
    let top = levels.len() - 1;
    let mut prev: Option<(Vec<u32>, usize)> = None;
    let mut synthesised = false;
    for li in (0..=top).rev() {
        let lv = &mut levels[li];
        let mut field = build_field(lv);
        let fresh = prev.is_none();
        if fresh {
            lv.onion_fill();
        }
        if field.src_list.is_empty() {
            prev = None;
        } else {
            // Initial field: random at the coarsest level, otherwise the
            // coarser field with offsets doubled.
            for &p in &field.targets {
                let (px, py) = (p % lv.w, p / lv.w);
                let mut q = None;
                if let Some((pnn, pw)) = &prev {
                    let cp = (py / 2) * pw + px / 2;
                    let cq = pnn[cp] as usize;
                    let (cqx, cqy) = ((cq % pw) as isize, (cq / pw) as isize);
                    let (offx, offy) = (2 * (cqx - (px / 2) as isize), 2 * (cqy - (py / 2) as isize));
                    let (qx, qy) = (px as isize + offx, py as isize + offy);
                    if qx >= 0 && qy >= 0 && (qx as usize) < lv.w && (qy as usize) < lv.h {
                        let cand = qy as usize * lv.w + qx as usize;
                        if field.src_ok[cand] {
                            q = Some(cand);
                        }
                    }
                }
                let q = q.unwrap_or_else(|| field.src_list[rng.below(field.src_list.len())] as usize);
                field.nn[p] = q as u32;
            }
            let conf: Vec<f32> = lv.distance_to_known().iter().map(|d| 1.3f32.powf(-d)).collect();
            let (em, pm) = if fresh {
                (8, 4)
            } else {
                // Seed the hole from the upsampled field, then refine.
                patchmatch_distances(lv, &mut field);
                vote(lv, &field, &conf);
                if li == 0 {
                    (2, 2)
                } else {
                    (4, 3)
                }
            };
            for _ in 0..em {
                patchmatch(lv, &mut field, pm, &mut rng);
                vote(lv, &field, &conf);
            }
            if li == 0 {
                poisson_refine(lv, &field.nn);
                synthesised = true;
            }
            prev = Some((field.nn, lv.w));
        }
        if li > 0 {
            // Upsample this level's result into the next finer level's hole
            // as a fallback for pixels no patch reaches.
            let (cw, fine_w) = (lv.w, levels[li - 1].w);
            let coarse = levels[li].img.clone();
            let fine = &mut levels[li - 1];
            for y in 0..fine.h {
                for x in 0..fine.w {
                    let i = y * fine_w + x;
                    if fine.hole[i] {
                        let ci = (y / 2) * cw + x / 2;
                        fine.img[i * 4..i * 4 + 4].copy_from_slice(&coarse[ci * 4..ci * 4 + 4]);
                    }
                }
            }
        }
    }
    let lv = &levels[0];
    for i in 0..n {
        if hole[i] {
            for c in 0..4 {
                buf[i * 4 + c] = (lv.img[i * 4 + c].clamp(0.0, 255.0) + 0.5) as u8;
            }
        }
    }
    synthesised
}

/// Remove the seams where neighbouring hole pixels were copied from different
/// places. The hole is re-solved in the gradient domain: each pixel's
/// gradients are those around its own source pixel, and the known pixels
/// around the hole are the boundary. Texture is kept; brightness steps
/// between copied regions and at the hole's edge disappear.
fn poisson_refine(lv: &mut Level, nn: &[u32]) {
    let (w, h) = (lv.w, lv.h);
    let n = w * h;
    let cells: Vec<usize> = (0..n).filter(|&i| lv.hole[i]).collect();
    if cells.is_empty() {
        return;
    }
    let src = lv.img.clone();
    let depth = lv.hole_depth().max(1) as f32;
    let omega = 2.0 / (1.0 + (std::f32::consts::PI / (2.0 * depth + 2.0)).sin());
    let iters = (depth * 6.0) as usize + 40;
    let mut solved: Vec<Vec<f32>> = Vec::with_capacity(3);
    // Guidance differences along each edge: ex[i] is the wanted f(i+1) − f(i).
    for c in 0..3 {
        let at = |i: usize| src[i * 4 + c];
        let mut ex = vec![0f32; n];
        let mut ey = vec![0f32; n];
        for &i in &cells {
            let q = nn[i] as usize;
            let (x, y) = (i % w, i / w);
            if x + 1 < w {
                ex[i] = at(q + 1) - at(q);
            }
            if x > 0 && !lv.hole[i - 1] {
                ex[i - 1] = at(q) - at(q - 1);
            }
            if y + 1 < h {
                ey[i] = at(q + w) - at(q);
            }
            if y > 0 && !lv.hole[i - w] {
                ey[i - w] = at(q) - at(q - w);
            }
        }
        let mut f: Vec<f32> = (0..n).map(|i| src[i * 4 + c]).collect();
        for _ in 0..iters {
            let mut change = 0f32;
            for &i in &cells {
                let (x, y) = (i % w, i / w);
                let (mut s, mut k) = (0f32, 0f32);
                if x + 1 < w && lv.inside[i + 1] {
                    s += f[i + 1] - ex[i];
                    k += 1.0;
                }
                if x > 0 && lv.inside[i - 1] {
                    s += f[i - 1] + ex[i - 1];
                    k += 1.0;
                }
                if y + 1 < h && lv.inside[i + w] {
                    s += f[i + w] - ey[i];
                    k += 1.0;
                }
                if y > 0 && lv.inside[i - w] {
                    s += f[i - w] + ey[i - w];
                    k += 1.0;
                }
                if k == 0.0 {
                    continue;
                }
                let delta = omega * (s / k - f[i]);
                f[i] += delta;
                change = change.max(delta.abs());
            }
            if change < 0.02 {
                break;
            }
        }
        solved.push(f);
    }
    // Small corrections are seams between copied regions: take them. A large
    // correction means a real structure edge meets the hole, where the
    // membrane would smear one side into the other: keep the vote there.
    for &i in &cells {
        let m = (0..3).map(|c| (solved[c][i] - src[i * 4 + c]).abs()).fold(0f32, f32::max);
        let t = ((m - 10.0) / 18.0).clamp(0.0, 1.0);
        let keep = 1.0 - t * t * (3.0 - 2.0 * t);
        for c in 0..3 {
            lv.img[i * 4 + c] = src[i * 4 + c] + (solved[c][i] - src[i * 4 + c]) * keep;
        }
    }
}

fn patchmatch_distances(lv: &Level, f: &mut Field) {
    for &p in &f.targets {
        f.dist[p] = patch_distance(lv, p, f.nn[p] as usize, f32::MAX);
    }
}

/// Harmonic interpolation: solve Laplace's equation over `region` with the
/// values of `field` outside it held fixed (successive over-relaxation from
/// a coarse-to-fine start). `field` is one channel, row-major.
pub fn harmonic_fill(field: &mut [f32], w: usize, h: usize, region: &[bool]) {
    let cells: Vec<usize> = (0..w * h).filter(|&i| region[i]).collect();
    if cells.is_empty() {
        return;
    }
    // Start from a diffusion guess so SOR has little distance to travel.
    let mut known: Vec<bool> = region.iter().map(|&r| !r).collect();
    let mut frontier = true;
    while frontier {
        frontier = false;
        let mut updates = Vec::new();
        for &i in &cells {
            if known[i] {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let mut s = 0f32;
            let mut n = 0f32;
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h && known[ny * w + nx] {
                    s += field[ny * w + nx];
                    n += 1.0;
                }
            }
            if n > 0.0 {
                updates.push((i, s / n));
            }
        }
        for (i, v) in updates {
            field[i] = v;
            known[i] = true;
            frontier = true;
        }
    }
    let extent = (cells.len() as f32).sqrt().max(2.0);
    let omega = 2.0 / (1.0 + (std::f32::consts::PI / extent).sin());
    let iters = (extent * 3.0) as usize + 20;
    for _ in 0..iters {
        let mut change = 0f32;
        for &i in &cells {
            let (x, y) = (i % w, i / w);
            let mut s = 0f32;
            let mut n = 0f32;
            if x > 0 {
                s += field[i - 1];
                n += 1.0;
            }
            if x + 1 < w {
                s += field[i + 1];
                n += 1.0;
            }
            if y > 0 {
                s += field[i - w];
                n += 1.0;
            }
            if y + 1 < h {
                s += field[i + w];
                n += 1.0;
            }
            let target = s / n;
            let delta = omega * (target - field[i]);
            field[i] += delta;
            change = change.max(delta.abs());
        }
        if change < 0.01 {
            break;
        }
    }
}
