//! Content-aware scale by seam carving.
//!
//! Seams are found on the merged image with gradient-magnitude energy plus
//! forward energy (the cost of the new edges a removal creates). Several
//! disjoint seams are taken from each dynamic-programming pass, which keeps a
//! 3 MP image to well under a second. Expansion finds the seams a removal
//! would take and duplicates them (averaging with the right neighbour).
//!
//! The result is a list of [`Stage`]s — per-row column maps — applied to
//! every layer, so all layers stay registered with each other.

/// One pass over one axis. For a width stage on a `w`×`h` image, `codes`
/// holds `out_w`×`h` entries: `2·x` copies input column `x`, `2·x+1`
/// averages columns `x` and `x+1`. Height stages are width stages on the
/// transposed image.
#[derive(Clone, Debug)]
pub struct Stage {
    pub vertical: bool,
    pub in_w: usize,
    pub out_w: usize,
    pub codes: Vec<u32>,
}

pub fn transpose(buf: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let mut out = vec![0u8; buf.len()];
    for y in 0..h {
        for x in 0..w {
            let s = (y * w + x) * ch;
            let d = (x * h + y) * ch;
            out[d..d + ch].copy_from_slice(&buf[s..s + ch]);
        }
    }
    out
}

/// Apply a width map to a `in_w`×`h` buffer.
fn apply_width(codes: &[u32], in_w: usize, out_w: usize, h: usize, buf: &[u8], ch: usize) -> Vec<u8> {
    let mut out = vec![0u8; out_w * h * ch];
    for y in 0..h {
        let row = &codes[y * out_w..(y + 1) * out_w];
        for (x, &code) in row.iter().enumerate() {
            let sx = (code / 2) as usize;
            let s = (y * in_w + sx) * ch;
            let d = (y * out_w + x) * ch;
            if code & 1 == 0 {
                out[d..d + ch].copy_from_slice(&buf[s..s + ch]);
            } else {
                let s2 = (y * in_w + (sx + 1).min(in_w - 1)) * ch;
                if ch == 4 {
                    let (a1, a2) = (buf[s + 3] as u32, buf[s2 + 3] as u32);
                    let a = a1 + a2;
                    for c in 0..3 {
                        out[d + c] = (buf[s + c] as u32 * a1 + buf[s2 + c] as u32 * a2 + a / 2).checked_div(a).unwrap_or(0) as u8;
                    }
                    out[d + 3] = a.div_ceil(2) as u8;
                } else {
                    out[d] = (buf[s] as u32 + buf[s2] as u32).div_ceil(2) as u8;
                }
            }
        }
    }
    out
}

impl Stage {
    /// Apply to a buffer of the stage's input size; returns the new buffer
    /// and its width and height.
    pub fn apply(&self, buf: &[u8], w: usize, h: usize, ch: usize) -> (Vec<u8>, usize, usize) {
        if self.vertical {
            let t = transpose(buf, w, h, ch);
            let o = apply_width(&self.codes, self.in_w, self.out_w, w, &t, ch);
            (transpose(&o, self.out_w, w, ch), w, self.out_w)
        } else {
            (apply_width(&self.codes, self.in_w, self.out_w, h, buf, ch), self.out_w, h)
        }
    }
}

fn luminance(rgba: &[u8]) -> Vec<f32> {
    rgba.chunks_exact(4).map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) * p[3] as f32 / 255.0).collect()
}

/// Energy bias for skin tones (YCbCr box), dilated and softened.
fn skin_bias(rgba: &[u8], w: usize, h: usize) -> Vec<f32> {
    let mut m: Vec<f32> = rgba
        .chunks_exact(4)
        .map(|p| {
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let cb = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
            let cr = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
            // Red furnishings share skin's Cr; skin keeps more green relative
            // to red.
            if p[3] > 128 && y > 40.0 && (85.0..=125.0).contains(&cb) && (138.0..=170.0).contains(&cr) && r > g && g > b && (0.52..=0.9).contains(&(g / r)) {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    // Box blur (radius 4) twice so skin regions become solid blobs.
    let rad = 4usize;
    for _ in 0..2 {
        let mut tmp = vec![0f32; m.len()];
        for y in 0..h {
            let mut acc = 0.0;
            for x in 0..w + rad {
                if x < w {
                    acc += m[y * w + x];
                }
                if x > 2 * rad && x - 2 * rad - 1 < w {
                    acc -= m[y * w + x - 2 * rad - 1];
                }
                if x >= rad && x - rad < w {
                    tmp[y * w + x - rad] = acc / (2 * rad + 1) as f32;
                }
            }
        }
        for x in 0..w {
            let mut acc = 0.0;
            for y in 0..h + rad {
                if y < h {
                    acc += tmp[y * w + x];
                }
                if y > 2 * rad && y - 2 * rad - 1 < h {
                    acc -= tmp[(y - 2 * rad - 1) * w + x];
                }
                if y >= rad && y - rad < h {
                    m[(y - rad) * w + x] = acc / (2 * rad + 1) as f32;
                }
            }
        }
    }
    // Only solid blobs count; stray pixels on curtains and wood fade out.
    m.iter().map(|v| if *v > 0.35 { 1500.0 * ((*v - 0.35) / 0.3).min(1.0) } else { 0.0 }).collect()
}

/// Remove `n` vertical seams. Returns, per row, the original columns removed.
pub fn find_seams(lum: &[f32], bias: &[f32], w: usize, h: usize, n: usize) -> Vec<Vec<u32>> {
    let mut cw = w;
    let mut lum = lum.to_vec();
    let mut bias = bias.to_vec();
    let mut idx: Vec<u32> = (0..h).flat_map(|_| 0..w as u32).collect();
    let mut removed: Vec<Vec<u32>> = vec![Vec::with_capacity(n); h];
    let mut energy = vec![0f32; w * h];
    let mut cost = vec![0f32; w * h];
    let mut used = vec![false; w * h];
    let mut remaining = n.min(w.saturating_sub(1));
    while remaining > 0 {
        let k = remaining.min((cw / 50).max(1));
        // Gradient magnitude energy.
        for y in 0..h {
            let up = y.saturating_sub(1);
            let dn = (y + 1).min(h - 1);
            for x in 0..cw {
                let l = lum[y * cw + x.saturating_sub(1)];
                let r = lum[y * cw + (x + 1).min(cw - 1)];
                let u = lum[up * cw + x];
                let d = lum[dn * cw + x];
                energy[y * cw + x] = (r - l).abs() + (d - u).abs() + bias[y * cw + x];
            }
        }
        // Forward-energy DP.
        cost[..cw].copy_from_slice(&energy[..cw]);
        for y in 1..h {
            let (prev, cur) = cost.split_at_mut(y * cw);
            let prev = &prev[(y - 1) * cw..];
            let row = &lum[y * cw..(y + 1) * cw];
            let above = &lum[(y - 1) * cw..y * cw];
            for x in 0..cw {
                let xl = x.saturating_sub(1);
                let xr = (x + 1).min(cw - 1);
                let cu = (row[xr] - row[xl]).abs();
                let mut best = prev[x] + cu;
                if x > 0 {
                    best = best.min(prev[x - 1] + cu + (above[x] - row[xl]).abs());
                }
                if x + 1 < cw {
                    best = best.min(prev[x + 1] + cu + (above[x] - row[xr]).abs());
                }
                cur[x] = energy[y * cw + x] + best;
            }
        }
        // Take up to k disjoint seams, cheapest ends first.
        let last = (h - 1) * cw;
        let mut order: Vec<usize> = (0..cw).collect();
        order.sort_by(|a, b| cost[last + a].total_cmp(&cost[last + b]));
        used[..cw * h].iter_mut().for_each(|u| *u = false);
        let mut path = vec![0usize; h];
        let mut taken = 0;
        for &start in order.iter().take((k * 6).max(16)) {
            if taken == k {
                break;
            }
            if used[last + start] {
                continue;
            }
            path[h - 1] = start;
            let mut ok = true;
            for y in (1..h).rev() {
                let x = path[y];
                let row = &lum[y * cw..(y + 1) * cw];
                let above = &lum[(y - 1) * cw..y * cw];
                let xl = x.saturating_sub(1);
                let xr = (x + 1).min(cw - 1);
                let cu = (row[xr] - row[xl]).abs();
                let mut best: Option<(f32, usize)> = None;
                let mut consider = |px: usize, extra: f32| {
                    if used[(y - 1) * cw + px] {
                        return;
                    }
                    let c = cost[(y - 1) * cw + px] + cu + extra;
                    if best.is_none_or(|(b, _)| c < b) {
                        best = Some((c, px));
                    }
                };
                consider(x, 0.0);
                if x > 0 {
                    consider(x - 1, (above[x] - row[xl]).abs());
                }
                if x + 1 < cw {
                    consider(x + 1, (above[x] - row[xr]).abs());
                }
                match best {
                    Some((_, px)) => path[y - 1] = px,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            for (y, &x) in path.iter().enumerate() {
                used[y * cw + x] = true;
            }
            taken += 1;
        }
        if taken == 0 {
            break;
        }
        // Compact every row.
        let nw = cw - taken;
        for (y, gone) in removed.iter_mut().enumerate() {
            let (mut o, base) = (y * nw, y * cw);
            for x in 0..cw {
                if used[base + x] {
                    gone.push(idx[base + x]);
                } else {
                    lum[o] = lum[base + x];
                    bias[o] = bias[base + x];
                    idx[o] = idx[base + x];
                    o += 1;
                }
            }
        }
        cw = nw;
        remaining -= taken;
    }
    for r in &mut removed {
        r.sort_unstable();
    }
    removed
}

/// Stages that change the width of a `w`×`h` RGBA image to `target`.
fn width_stages(rgba: &[u8], w: usize, h: usize, target: usize, protect_skin: bool) -> Vec<Stage> {
    let mut stages = Vec::new();
    let mut cur = rgba.to_vec();
    let mut cw = w;
    while cw != target {
        let lum = luminance(&cur);
        let bias = if protect_skin { skin_bias(&cur, cw, h) } else { vec![0.0; cw * h] };
        let stage = if target < cw {
            let n = cw - target;
            let removed = find_seams(&lum, &bias, cw, h, n);
            let out_w = cw - removed[0].len();
            let mut codes = Vec::with_capacity(out_w * h);
            for r in &removed {
                let mut it = r.iter().peekable();
                for x in 0..cw as u32 {
                    if it.peek() == Some(&&x) {
                        it.next();
                    } else {
                        codes.push(2 * x);
                    }
                }
            }
            Stage { vertical: false, in_w: cw, out_w, codes }
        } else {
            // Insert at most half the width per round, so duplicates spread.
            let n = (target - cw).min((cw / 2).max(1));
            let dup = find_seams(&lum, &bias, cw, h, n);
            let out_w = cw + dup[0].len();
            let mut codes = Vec::with_capacity(out_w * h);
            for r in &dup {
                let mut it = r.iter().peekable();
                for x in 0..cw as u32 {
                    codes.push(2 * x);
                    if it.peek() == Some(&&x) {
                        it.next();
                        codes.push(2 * x + 1);
                    }
                }
            }
            Stage { vertical: false, in_w: cw, out_w, codes }
        };
        if stage.out_w == cw {
            break;
        }
        cur = apply_width(&stage.codes, stage.in_w, stage.out_w, h, &cur, 4);
        cw = stage.out_w;
        stages.push(stage);
    }
    stages
}

/// Plan the stages that take the merged image from `w`×`h` to
/// `tw`×`th`.
pub fn plan(rgba: &[u8], w: usize, h: usize, tw: usize, th: usize, protect_skin: bool) -> Vec<Stage> {
    let mut stages = width_stages(rgba, w, h, tw, protect_skin);
    if th != h {
        let mut cur = rgba.to_vec();
        let (mut cw, mut ch) = (w, h);
        for s in &stages {
            (cur, cw, ch) = s.apply(&cur, cw, ch, 4);
        }
        let t = transpose(&cur, cw, ch, 4);
        for mut s in width_stages(&t, ch, cw, th, protect_skin) {
            s.vertical = true;
            stages.push(s);
        }
    }
    stages
}
