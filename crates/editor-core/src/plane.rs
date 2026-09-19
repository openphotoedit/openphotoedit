//! Sparse, copy-on-write tiled pixel storage.
//!
//! Every layer, mask and selection is a [`Plane`]: a grid of 256×256 tiles
//! held behind `Arc`. A tile that was never written is absent and reads as the
//! plane's fill value, so a transparent 12,000 px layer costs a vector of
//! `None`. Cloning a plane clones the `Arc`s, which is what makes history
//! snapshots cheap: an undo step only pays for the tiles that changed.
//!
//! Pixels are 8-bit and straight (not premultiplied), which is how PSD stores
//! them and how Photoshop composites 8-bit documents.

use std::sync::{Arc, OnceLock};

use crate::geom::Rect;

pub const TILE: u32 = 256;
const TILE_SHIFT: u32 = 8;
/// Mip levels kept per tile: level 8 is a single pixel.
const MAX_LEVEL: u32 = 8;

pub struct Tile {
    data: Box<[u8]>,
    /// Levels 1..=8, built on first zoomed-out read and dropped on write.
    mips: OnceLock<Vec<Box<[u8]>>>,
}

impl Clone for Tile {
    fn clone(&self) -> Self {
        Tile { data: self.data.clone(), mips: OnceLock::new() }
    }
}

impl Tile {
    fn filled(channels: usize, fill: [u8; 4]) -> Tile {
        let n = (TILE * TILE) as usize;
        let mut data = vec![0u8; n * channels].into_boxed_slice();
        if fill[..channels].iter().any(|&v| v != 0) {
            for px in data.chunks_exact_mut(channels) {
                px.copy_from_slice(&fill[..channels]);
            }
        }
        Tile { data, mips: OnceLock::new() }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    fn is_uniform(&self, channels: usize, value: [u8; 4]) -> bool {
        self.data.chunks_exact(channels).all(|px| px == &value[..channels])
    }

    fn level(&self, channels: usize, level: u32) -> &[u8] {
        if level == 0 {
            return &self.data;
        }
        let mips = self.mips.get_or_init(|| build_mips(&self.data, channels));
        &mips[(level - 1) as usize]
    }
}

fn build_mips(base: &[u8], channels: usize) -> Vec<Box<[u8]>> {
    let mut out: Vec<Box<[u8]>> = Vec::with_capacity(MAX_LEVEL as usize);
    let mut prev: &[u8] = base;
    let mut size = TILE as usize;
    for _ in 0..MAX_LEVEL {
        let half = size / 2;
        let mut next = vec![0u8; half * half * channels];
        for y in 0..half {
            for x in 0..half {
                let idx = |xx: usize, yy: usize| (yy * size + xx) * channels;
                let s = [idx(2 * x, 2 * y), idx(2 * x + 1, 2 * y), idx(2 * x, 2 * y + 1), idx(2 * x + 1, 2 * y + 1)];
                let d = (y * half + x) * channels;
                if channels == 4 {
                    // Average premultiplied, so transparent pixels do not
                    // bleed their (meaningless) colour into the edge.
                    let mut a = 0u32;
                    let mut c = [0u32; 3];
                    for &i in &s {
                        let pa = prev[i + 3] as u32;
                        a += pa;
                        for k in 0..3 {
                            c[k] += prev[i + k] as u32 * pa;
                        }
                    }
                    if a > 0 {
                        for k in 0..3 {
                            next[d + k] = ((c[k] + a / 2) / a) as u8;
                        }
                    }
                    next[d + 3] = ((a + 2) / 4) as u8;
                } else {
                    for k in 0..channels {
                        let sum: u32 = s.iter().map(|&i| prev[i + k] as u32).sum();
                        next[d + k] = ((sum + 2) / 4) as u8;
                    }
                }
            }
        }
        out.push(next.into_boxed_slice());
        prev = out.last().unwrap();
        size = half;
    }
    out
}

#[derive(Clone)]
pub struct Plane {
    width: u32,
    height: u32,
    channels: usize,
    cols: u32,
    rows: u32,
    tiles: Vec<Option<Arc<Tile>>>,
    fill: [u8; 4],
}

impl std::fmt::Debug for Plane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plane({}x{}x{}, {} tiles present)", self.width, self.height, self.channels, self.present_tiles())
    }
}

impl Plane {
    pub fn new(width: u32, height: u32, channels: usize, fill: [u8; 4]) -> Plane {
        assert!(channels == 1 || channels == 4, "planes are RGBA or single-channel");
        let cols = width.div_ceil(TILE).max(1);
        let rows = height.div_ceil(TILE).max(1);
        Plane { width, height, channels, cols, rows, tiles: vec![None; (cols * rows) as usize], fill }
    }

    /// A transparent RGBA plane.
    pub fn transparent(width: u32, height: u32) -> Plane {
        Plane::new(width, height, 4, [0; 4])
    }

    /// A single-channel plane where every pixel reads `value`.
    pub fn mask(width: u32, height: u32, value: u8) -> Plane {
        Plane::new(width, height, 1, [value, 0, 0, 0])
    }

    pub fn from_raw(width: u32, height: u32, channels: usize, raw: &[u8], fill: [u8; 4]) -> Plane {
        assert_eq!(raw.len(), width as usize * height as usize * channels, "raw buffer size");
        let mut p = Plane::new(width, height, channels, fill);
        p.write(Rect::new(0, 0, width as i32, height as i32), raw);
        p.compact();
        p
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn fill(&self) -> [u8; 4] {
        self.fill
    }
    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }
    pub fn present_tiles(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_some()).count()
    }
    /// Bytes held by tiles, counting shared tiles once per plane.
    pub fn byte_size(&self) -> usize {
        self.present_tiles() * (TILE * TILE) as usize * self.channels
    }

    /// Identity of every tile, for counting memory shared between snapshots.
    pub fn tile_ptrs(&self) -> impl Iterator<Item = usize> + '_ {
        self.tiles.iter().flatten().map(|t| Arc::as_ptr(t) as usize)
    }

    fn tile_index(&self, tx: u32, ty: u32) -> usize {
        (ty * self.cols + tx) as usize
    }

    pub fn tile(&self, tx: u32, ty: u32) -> Option<&Arc<Tile>> {
        if tx >= self.cols || ty >= self.rows {
            return None;
        }
        self.tiles[self.tile_index(tx, ty)].as_ref()
    }

    /// Mutable tile data, materialising an absent tile from the fill value and
    /// un-sharing a tile another snapshot still holds.
    pub fn tile_mut(&mut self, tx: u32, ty: u32) -> &mut [u8] {
        let i = self.tile_index(tx, ty);
        let (channels, fill) = (self.channels, self.fill);
        let slot = self.tiles[i].get_or_insert_with(|| Arc::new(Tile::filled(channels, fill)));
        let tile = Arc::make_mut(slot);
        tile.mips = OnceLock::new();
        &mut tile.data
    }

    /// Drop tiles that hold nothing but the fill value.
    pub fn compact(&mut self) {
        let (channels, fill) = (self.channels, self.fill);
        for slot in self.tiles.iter_mut() {
            if slot.as_ref().is_some_and(|t| t.is_uniform(channels, fill)) {
                *slot = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.tiles.iter_mut().for_each(|t| *t = None);
    }

    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return self.fill;
        }
        let (x, y) = (x as u32, y as u32);
        match self.tile(x >> TILE_SHIFT, y >> TILE_SHIFT) {
            None => self.fill,
            Some(t) => {
                let i = (((y & (TILE - 1)) * TILE + (x & (TILE - 1))) as usize) * self.channels;
                let mut px = [0u8; 4];
                px[..self.channels].copy_from_slice(&t.data[i..i + self.channels]);
                px
            }
        }
    }

    /// Copy a rectangle into `out` (row-major, `channels` per pixel). Pixels
    /// outside the plane read as the fill value.
    pub fn read(&self, rect: Rect, out: &mut [u8]) {
        let ch = self.channels;
        assert_eq!(out.len(), rect.w.max(0) as usize * rect.h.max(0) as usize * ch);
        if rect.is_empty() {
            return;
        }
        // Start from the fill everywhere, then copy tile data over it.
        if self.fill[..ch].iter().all(|&v| v == 0) {
            out.fill(0);
        } else {
            for px in out.chunks_exact_mut(ch) {
                px.copy_from_slice(&self.fill[..ch]);
            }
        }
        let clip = rect.intersect(&self.bounds());
        if clip.is_empty() {
            return;
        }
        let ty0 = clip.y as u32 >> TILE_SHIFT;
        let ty1 = (clip.bottom() as u32 - 1) >> TILE_SHIFT;
        let tx0 = clip.x as u32 >> TILE_SHIFT;
        let tx1 = (clip.right() as u32 - 1) >> TILE_SHIFT;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let Some(tile) = self.tile(tx, ty) else { continue };
                let tr = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32);
                let part = tr.intersect(&clip);
                for y in part.y..part.bottom() {
                    let src_row = ((y - tr.y) as usize * TILE as usize + (part.x - tr.x) as usize) * ch;
                    let dst_row = ((y - rect.y) as usize * rect.w as usize + (part.x - rect.x) as usize) * ch;
                    let n = part.w as usize * ch;
                    out[dst_row..dst_row + n].copy_from_slice(&tile.data[src_row..src_row + n]);
                }
            }
        }
    }

    pub fn read_vec(&self, rect: Rect) -> Vec<u8> {
        let mut v = vec![0u8; rect.w.max(0) as usize * rect.h.max(0) as usize * self.channels];
        self.read(rect, &mut v);
        v
    }

    pub fn to_raw(&self) -> Vec<u8> {
        self.read_vec(self.bounds())
    }

    /// Write a rectangle of pixels. Parts outside the plane are ignored.
    pub fn write(&mut self, rect: Rect, data: &[u8]) {
        let ch = self.channels;
        assert_eq!(data.len(), rect.w.max(0) as usize * rect.h.max(0) as usize * ch);
        let clip = rect.intersect(&self.bounds());
        if clip.is_empty() {
            return;
        }
        let ty0 = clip.y as u32 >> TILE_SHIFT;
        let ty1 = (clip.bottom() as u32 - 1) >> TILE_SHIFT;
        let tx0 = clip.x as u32 >> TILE_SHIFT;
        let tx1 = (clip.right() as u32 - 1) >> TILE_SHIFT;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let tr = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32);
                let part = tr.intersect(&clip);
                let tile = self.tile_mut(tx, ty);
                for y in part.y..part.bottom() {
                    let dst = ((y - tr.y) as usize * TILE as usize + (part.x - tr.x) as usize) * ch;
                    let src = ((y - rect.y) as usize * rect.w as usize + (part.x - rect.x) as usize) * ch;
                    let n = part.w as usize * ch;
                    tile[dst..dst + n].copy_from_slice(&data[src..src + n]);
                }
            }
        }
    }

    /// Visit every pixel of `rect` (clipped to the plane) mutably, tile by
    /// tile. `f(x, y, pixel)` receives plane coordinates.
    pub fn modify(&mut self, rect: Rect, mut f: impl FnMut(i32, i32, &mut [u8])) {
        let ch = self.channels;
        let clip = rect.intersect(&self.bounds());
        if clip.is_empty() {
            return;
        }
        let ty0 = clip.y as u32 >> TILE_SHIFT;
        let ty1 = (clip.bottom() as u32 - 1) >> TILE_SHIFT;
        let tx0 = clip.x as u32 >> TILE_SHIFT;
        let tx1 = (clip.right() as u32 - 1) >> TILE_SHIFT;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let tr = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32);
                let part = tr.intersect(&clip);
                let tile = self.tile_mut(tx, ty);
                for y in part.y..part.bottom() {
                    for x in part.x..part.right() {
                        let i = ((y - tr.y) as usize * TILE as usize + (x - tr.x) as usize) * ch;
                        f(x, y, &mut tile[i..i + ch]);
                    }
                }
            }
        }
    }

    /// Rectangles of the tiles that hold data, clipped to the plane.
    pub fn present_rects(&self) -> Vec<Rect> {
        let mut out = Vec::new();
        for ty in 0..self.rows {
            for tx in 0..self.cols {
                if self.tiles[self.tile_index(tx, ty)].is_some() {
                    let r = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32);
                    out.push(r.intersect(&self.bounds()));
                }
            }
        }
        out
    }

    /// Exact bounds of pixels that differ from transparent (RGBA: alpha > 0)
    /// or from the fill (single channel).
    pub fn content_bounds(&self) -> Rect {
        let mut bounds = Rect::empty();
        for r in self.present_rects() {
            let data = self.read_vec(r);
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for y in 0..r.h {
                for x in 0..r.w {
                    let i = (y * r.w + x) as usize * self.channels;
                    let differs = if self.channels == 4 { data[i + 3] != 0 } else { data[i] != self.fill[0] };
                    if differs {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            if x1 >= x0 {
                bounds = bounds.union(&Rect::new(r.x + x0, r.y + y0, x1 - x0 + 1, y1 - y0 + 1));
            }
        }
        bounds
    }

    /// Number of mip levels that make sense for this plane.
    fn level_dims(&self, level: u32) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    #[allow(dead_code)]

    fn level_pixel(&self, level: u32, lx: u32, ly: u32) -> [u8; 4] {
        let shift = TILE_SHIFT - level;
        let size = TILE >> level;
        match self.tile(lx >> shift, ly >> shift) {
            None => self.fill,
            Some(t) => {
                let data = t.level(self.channels, level);
                let i = (((ly & (size - 1)) * size + (lx & (size - 1))) as usize) * self.channels;
                let mut px = [0u8; 4];
                px[..self.channels].copy_from_slice(&data[i..i + self.channels]);
                px
            }
        }
    }

    /// Resample into a float grid. Output pixel `(i, j)` covers source
    /// coordinates starting at `x0 + i * step`. Zoomed in (`step <= 1`) uses
    /// nearest-neighbour, which is how an editor shows pixels; zoomed out uses
    /// a mip level plus bilinear filtering. Values are 0..1, straight alpha.
    /// Pixels beyond the plane read as the fill value.
    pub fn resample(&self, x0: f64, y0: f64, step: f64, out_w: usize, out_h: usize, out: &mut [f32]) {
        let ch = self.channels;
        debug_assert_eq!(out.len(), out_w * out_h * ch);
        let fill: [f32; 4] = self.fill.map(|v| v as f32 / 255.0);
        if self.present_tiles() == 0 {
            for px in out.chunks_exact_mut(ch) {
                px.copy_from_slice(&fill[..ch]);
            }
            return;
        }
        if step <= 1.0 {
            // Nearest. Walk tile rows directly for speed.
            let (w, h) = (self.width as i64, self.height as i64);
            for j in 0..out_h {
                let sy = (y0 + (j as f64 + 0.5) * step).floor() as i64;
                let row = &mut out[j * out_w * ch..(j + 1) * out_w * ch];
                if sy < 0 || sy >= h {
                    for px in row.chunks_exact_mut(ch) {
                        px.copy_from_slice(&fill[..ch]);
                    }
                    continue;
                }
                let ty = (sy as u32) >> TILE_SHIFT;
                let ly = (sy as u32 & (TILE - 1)) as usize;
                for i in 0..out_w {
                    let sx = (x0 + (i as f64 + 0.5) * step).floor() as i64;
                    let px = &mut row[i * ch..(i + 1) * ch];
                    if sx < 0 || sx >= w {
                        px.copy_from_slice(&fill[..ch]);
                        continue;
                    }
                    match self.tile((sx as u32) >> TILE_SHIFT, ty) {
                        None => px.copy_from_slice(&fill[..ch]),
                        Some(t) => {
                            let k = (ly * TILE as usize + (sx as u32 & (TILE - 1)) as usize) * ch;
                            for c in 0..ch {
                                px[c] = t.data[k + c] as f32 / 255.0;
                            }
                        }
                    }
                }
            }
            return;
        }
        let level = (step.log2().floor() as u32).min(MAX_LEVEL);
        let lscale = (1u32 << level) as f64;
        // What is left after the mip: 1 at exact powers of two, where the
        // mip pixels are the answer and bilinear below reads them directly.
        // Anything in between needs a real low-pass, or detail finer than
        // the output grid aliases into moiré (1-px stripes at step 1.3 came
        // out as bands with a standard deviation of 73).
        let rest = step / lscale;
        if rest > 1.0 + 1e-9 && rest <= 4.0 {
            self.resample_filtered(level, rest, x0, y0, step, out_w, out_h, out);
            return;
        }
        let (lw, lh) = self.level_dims(level);
        let size = (TILE >> level) as usize;
        let shift = TILE_SHIFT - level;
        let cols = self.cols as usize;
        let fill_u8 = self.fill;

        // Horizontal sample positions are the same on every row.
        let xs: Vec<(i64, f32)> = (0..out_w)
            .map(|i| {
                let fx = (x0 + (i as f64 + 0.5) * step) / lscale - 0.5;
                let ix = fx.floor();
                (ix as i64, (fx - ix) as f32)
            })
            .collect();

        // One level-row of tiles: the data slice for each tile column plus
        // the row's offset inside the tile. Rebuilt only when the tile row
        // changes, so the inner loop is plain indexing.
        type Row<'a> = Option<(Vec<Option<&'a [u8]>>, usize)>;
        let make_row = |ly: i64| -> Row<'_> {
            if ly < 0 || ly >= lh as i64 {
                return None;
            }
            let ty = (ly as u32) >> shift;
            let tiles = (0..cols).map(|tx| self.tile(tx as u32, ty).map(|t| t.level(ch, level))).collect();
            Some((tiles, (ly as usize) & (size - 1)))
        };
        let fetch = |row: &Row<'_>, lx: i64| -> [u8; 4] {
            let Some((tiles, local_y)) = row else { return fill_u8 };
            if lx < 0 || lx >= lw as i64 {
                return fill_u8;
            }
            match tiles[(lx as usize) >> shift] {
                None => fill_u8,
                Some(data) => {
                    let k = (local_y * size + ((lx as usize) & (size - 1))) * ch;
                    let mut px = [0u8; 4];
                    px[..ch].copy_from_slice(&data[k..k + ch]);
                    px
                }
            }
        };

        let mut cached: [(i64, Row<'_>); 2] = [(i64::MIN, None), (i64::MIN, None)];
        let tile_row = |ly: i64| -> i64 { if ly < 0 || ly >= lh as i64 { -1 } else { ((ly as u32) >> shift) as i64 } };
        for j in 0..out_h {
            let fy = (y0 + (j as f64 + 0.5) * step) / lscale - 0.5;
            let iyf = fy.floor();
            let wy = (fy - iyf) as f32;
            let iy = iyf as i64;
            // Rows iy and iy+1, each possibly in a different tile row.
            for (slot, ly) in [(0usize, iy), (1usize, iy + 1)] {
                let key = tile_row(ly);
                if cached[slot].0 != key || key < 0 {
                    // Reuse the other slot when it holds this tile row.
                    let other = 1 - slot;
                    if cached[other].0 == key && key >= 0 {
                        let (tiles, _) = cached[other].1.clone().unwrap();
                        cached[slot] = (key, Some((tiles, (ly as usize) & (size - 1))));
                    } else {
                        cached[slot] = (key, make_row(ly));
                    }
                } else if let Some((_, local)) = cached[slot].1.as_mut() {
                    *local = (ly as usize) & (size - 1);
                }
            }
            let (r0, r1) = (&cached[0].1, &cached[1].1);
            let orow = &mut out[j * out_w * ch..(j + 1) * out_w * ch];
            for (i, &(ix, wx)) in xs.iter().enumerate() {
                let p00 = fetch(r0, ix);
                let p10 = fetch(r0, ix + 1);
                let p01 = fetch(r1, ix);
                let p11 = fetch(r1, ix + 1);
                let w00 = (1.0 - wx) * (1.0 - wy);
                let w10 = wx * (1.0 - wy);
                let w01 = (1.0 - wx) * wy;
                let w11 = wx * wy;
                let o = i * ch;
                if ch == 4 {
                    let (a00, a10, a01, a11) = (p00[3] as f32 * w00, p10[3] as f32 * w10, p01[3] as f32 * w01, p11[3] as f32 * w11);
                    let sa = a00 + a10 + a01 + a11;
                    if sa > 0.0 {
                        let inv = 1.0 / (sa * 255.0);
                        for c in 0..3 {
                            let v = p00[c] as f32 * a00 + p10[c] as f32 * a10 + p01[c] as f32 * a01 + p11[c] as f32 * a11;
                            orow[o + c] = v * inv;
                        }
                    } else {
                        orow[o..o + 3].fill(0.0);
                    }
                    orow[o + 3] = sa / 255.0;
                } else {
                    let v = p00[0] as f32 * w00 + p10[0] as f32 * w10 + p01[0] as f32 * w01 + p11[0] as f32 * w11;
                    orow[o] = v / 255.0;
                }
            }
        }
    }

    /// Zoomed-out resample from mip `level` with a cubic B-spline scaled to
    /// the output grid (`rest` = step in level pixels, > 1). Separable: each
    /// level row is gathered once (premultiplied), filtered horizontally and
    /// kept in a small ring while the output rows that need it are summed.
    ///
    /// The B-spline is non-negative (no ringing, colour never exceeds alpha)
    /// and its response above the output Nyquist frequency is small enough
    /// that 1-px stripes average to flat grey (standard deviation 6.5 at
    /// step 1.3, 1.4 at 1.5, ~0 at 1.9; bilinear gave 73). Between a power
    /// of two and 1.25× it the kernel fades in from bilinear (see `blend`),
    /// trading some moiré there for sharpness.
    #[allow(clippy::too_many_arguments)]
    fn resample_filtered(&self, level: u32, rest: f64, x0: f64, y0: f64, step: f64, out_w: usize, out_h: usize, out: &mut [f32]) {
        fn bspline(u: f64) -> f32 {
            let u = u.abs();
            (if u < 1.0 {
                (4.0 - 6.0 * u * u + 3.0 * u * u * u) / 6.0
            } else if u < 2.0 {
                let v = 2.0 - u;
                v * v * v / 6.0
            } else {
                0.0
            }) as f32
        }
        let ch = self.channels;
        let lscale = (1u32 << level) as f64;
        let (lw, lh) = self.level_dims(level);
        let size = (TILE >> level) as usize;
        let shift = TILE_SHIFT - level;
        let taps = (4.0 * rest).ceil() as usize + 1;
        // Just above a power of two the mip is nearly the right size and
        // already box-filtered; a full B-spline there would blur it (a
        // Refine Edge analysing at step 2.1 lost edges it needs). Fade from
        // bilinear (a tent one level pixel wide) at rest = 1 to the full
        // B-spline by rest = 1.25, so the result is continuous with the
        // exact-mip path and fully filtered where moiré is strongest.
        let blend = {
            let t = ((rest - 1.0) / 0.25).clamp(0.0, 1.0);
            (t * t * (3.0 - 2.0 * t)) as f32
        };
        // Tap start and normalised weights for output index `i` along an axis
        // whose output origin is `o0` (document units).
        let axis = |o0: f64, n: usize| -> (Vec<i64>, Vec<f32>) {
            let mut starts = Vec::with_capacity(n);
            let mut weights = vec![0f32; n * taps];
            for i in 0..n {
                let c = (o0 + (i as f64 + 0.5) * step) / lscale; // centre, level units
                let k0 = (c - 0.5 - 2.0 * rest).ceil() as i64;
                let ws = &mut weights[i * taps..(i + 1) * taps];
                let mut sum = 0f32;
                for (t, w) in ws.iter_mut().enumerate() {
                    let d = k0 as f64 + t as f64 + 0.5 - c;
                    let tent = (1.0 - d.abs()).max(0.0) as f32;
                    *w = (1.0 - blend) * tent + blend * bspline(d / rest);
                    sum += *w;
                }
                if sum > 0.0 {
                    ws.iter_mut().for_each(|w| *w /= sum);
                }
                starts.push(k0);
            }
            (starts, weights)
        };
        let (xs, wx) = axis(x0, out_w);
        let (ys, wy) = axis(y0, out_h);
        let kmin = xs.first().copied().unwrap_or(0);
        let span = (xs.last().copied().unwrap_or(0) + taps as i64 - kmin).max(0) as usize;

        // Premultiplied fill (alpha in 0..255, colour in 0..255²).
        let pm = |p: &[u8]| -> [f32; 4] {
            if ch == 4 {
                let a = p[3] as f32;
                [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a]
            } else {
                [p[0] as f32, 0.0, 0.0, 0.0]
            }
        };
        let fill_pm = pm(&self.fill[..]);
        let mut srow = vec![0f32; span * ch];
        // Gather level row `ly` over [kmin, kmin + span) and filter it
        // horizontally into `dst` (out_w × ch, premultiplied).
        let mut hfilter = |ly: i64, dst: &mut [f32]| {
            let row_in = ly >= 0 && ly < lh as i64;
            let ty = if row_in { (ly as u32) >> shift } else { 0 };
            let local_y = if row_in { (ly as usize) & (size - 1) } else { 0 };
            let mut k = 0usize;
            while k < span {
                let lx = kmin + k as i64;
                if !row_in || lx < 0 || lx >= lw as i64 {
                    srow[k * ch..(k + 1) * ch].copy_from_slice(&fill_pm[..ch]);
                    k += 1;
                    continue;
                }
                // A run inside one tile.
                let tx = (lx as u32) >> shift;
                let tile_end = (((tx as i64) + 1) << shift).min(lw as i64);
                let run = ((tile_end - lx) as usize).min(span - k);
                match self.tile(tx, ty) {
                    None => {
                        for px in srow[k * ch..(k + run) * ch].chunks_exact_mut(ch) {
                            px.copy_from_slice(&fill_pm[..ch]);
                        }
                    }
                    Some(t) => {
                        let data = t.level(ch, level);
                        let base = (local_y * size + ((lx as usize) & (size - 1))) * ch;
                        let src = &data[base..base + run * ch];
                        let dst = &mut srow[k * ch..(k + run) * ch];
                        if ch == 4 {
                            for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                                let a = s[3] as f32;
                                d[0] = s[0] as f32 * a;
                                d[1] = s[1] as f32 * a;
                                d[2] = s[2] as f32 * a;
                                d[3] = a;
                            }
                        } else {
                            for (d, s) in dst.iter_mut().zip(src) {
                                *d = *s as f32;
                            }
                        }
                    }
                }
                k += run;
            }
            for i in 0..out_w {
                let s0 = (xs[i] - kmin) as usize;
                let w = &wx[i * taps..(i + 1) * taps];
                if ch == 4 {
                    let mut acc = [0f32; 4];
                    for (t, &wt) in w.iter().enumerate() {
                        let p = &srow[(s0 + t) * 4..(s0 + t) * 4 + 4];
                        acc[0] += wt * p[0];
                        acc[1] += wt * p[1];
                        acc[2] += wt * p[2];
                        acc[3] += wt * p[3];
                    }
                    dst[i * 4..i * 4 + 4].copy_from_slice(&acc);
                } else {
                    let mut acc = 0f32;
                    for (t, &wt) in w.iter().enumerate() {
                        acc += wt * srow[s0 + t];
                    }
                    dst[i] = acc;
                }
            }
        };

        // Ring of horizontally filtered rows, keyed by level row.
        let ring_len = taps + rest.ceil() as usize + 1;
        let row_len = out_w * ch;
        let mut ring = vec![0f32; ring_len * row_len];
        let mut keys = vec![i64::MIN; ring_len];
        let mut acc = vec![0f32; row_len];
        for j in 0..out_h {
            acc.fill(0.0);
            for t in 0..taps {
                let w = wy[j * taps + t];
                let ly = ys[j] + t as i64;
                let slot = ly.rem_euclid(ring_len as i64) as usize;
                let row = &mut ring[slot * row_len..(slot + 1) * row_len];
                if keys[slot] != ly {
                    hfilter(ly, row);
                    keys[slot] = ly;
                }
                if w != 0.0 {
                    for (a, v) in acc.iter_mut().zip(row.iter()) {
                        *a += w * *v;
                    }
                }
            }
            let orow = &mut out[j * row_len..(j + 1) * row_len];
            if ch == 4 {
                for (o, a) in orow.chunks_exact_mut(4).zip(acc.chunks_exact(4)) {
                    let alpha = a[3];
                    if alpha > 1e-3 {
                        let inv = 1.0 / (alpha * 255.0);
                        o[0] = (a[0] * inv).min(1.0);
                        o[1] = (a[1] * inv).min(1.0);
                        o[2] = (a[2] * inv).min(1.0);
                    } else {
                        o[..3].fill(0.0);
                    }
                    o[3] = (alpha / 255.0).clamp(0.0, 1.0);
                }
            } else {
                for (o, a) in orow.iter_mut().zip(acc.iter()) {
                    *o = (a / 255.0).clamp(0.0, 1.0);
                }
            }
        }
    }

    /// A new plane holding `rect` of this one.
    pub fn extract(&self, rect: Rect) -> Plane {
        let data = self.read_vec(rect);
        Plane::from_raw(rect.w.max(0) as u32, rect.h.max(0) as u32, self.channels, &data, self.fill)
    }

    /// Copy a whole plane in at `(dx, dy)`, replacing what is there.
    pub fn paste(&mut self, src: &Plane, dx: i32, dy: i32) {
        assert_eq!(src.channels, self.channels);
        for r in src.present_rects() {
            let data = src.read_vec(r);
            self.write(r.translate(dx, dy), &data);
        }
        // Absent source tiles are fill; if the fills differ they must be
        // written too.
        if src.fill != self.fill {
            let full = src.bounds();
            let present = src.present_rects();
            for ty in 0..src.rows {
                for tx in 0..src.cols {
                    let r = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32).intersect(&full);
                    if !present.contains(&r) {
                        let data = src.read_vec(r);
                        self.write(r.translate(dx, dy), &data);
                    }
                }
            }
        }
    }

    /// A same-sized copy with every tile's pixels passed through `f`, which
    /// receives the whole row-major tile buffer. Absent tiles stay absent when
    /// `f` maps the fill to itself (callers state this via `keep_absent`).
    pub fn map_tiles(&mut self, keep_absent: bool, mut f: impl FnMut(Rect, &mut [u8])) {
        for ty in 0..self.rows {
            for tx in 0..self.cols {
                let present = self.tiles[self.tile_index(tx, ty)].is_some();
                if !present && keep_absent {
                    continue;
                }
                let r = Rect::new((tx * TILE) as i32, (ty * TILE) as i32, TILE as i32, TILE as i32);
                let data = self.tile_mut(tx, ty);
                f(r, data);
            }
        }
        self.compact();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_tiles_read_as_fill() {
        let p = Plane::mask(600, 300, 255);
        assert_eq!(p.present_tiles(), 0);
        assert_eq!(p.get(599, 299)[0], 255);
        assert_eq!(p.get(-1, 0)[0], 255);
    }

    #[test]
    fn write_read_round_trip_across_tiles() {
        let mut p = Plane::transparent(700, 520);
        let r = Rect::new(250, 250, 20, 20);
        let data: Vec<u8> = (0..20 * 20 * 4).map(|i| (i % 251) as u8).collect();
        p.write(r, &data);
        assert_eq!(p.present_tiles(), 4);
        assert_eq!(p.read_vec(r), data);
        assert_eq!(p.content_bounds().w, 20);
    }

    #[test]
    fn clone_is_copy_on_write() {
        let mut a = Plane::transparent(300, 300);
        a.write(Rect::new(0, 0, 1, 1), &[1, 2, 3, 255]);
        let b = a.clone();
        a.write(Rect::new(0, 0, 1, 1), &[9, 9, 9, 255]);
        assert_eq!(b.get(0, 0), [1, 2, 3, 255]);
        assert_eq!(a.get(0, 0), [9, 9, 9, 255]);
    }

    #[test]
    fn compact_drops_uniform_tiles() {
        let mut p = Plane::transparent(256, 256);
        p.write(Rect::new(0, 0, 1, 1), &[0, 0, 0, 0]);
        assert_eq!(p.present_tiles(), 1);
        p.compact();
        assert_eq!(p.present_tiles(), 0);
    }

    #[test]
    fn zoomed_out_resample_averages_premultiplied() {
        // Left half opaque red, right half transparent: a 2x downsample of
        // the boundary must stay red, at half alpha.
        let mut p = Plane::transparent(4, 4);
        for y in 0..4 {
            p.write(Rect::new(0, y, 2, 1), &[255, 0, 0, 255, 255, 0, 0, 255]);
            p.write(Rect::new(2, y, 2, 1), &[0, 255, 0, 0, 0, 255, 0, 0]);
        }
        let mut out = vec![0f32; 2 * 2 * 4];
        p.resample(0.0, 0.0, 2.0, 2, 2, &mut out);
        assert!((out[0] - 1.0).abs() < 1e-3, "red kept: {:?}", &out[..4]);
        assert!(out[3] > 0.99);
        assert!(out[7] < 0.01, "right pixel transparent");
    }

    fn stripes(w: u32, h: u32) -> Plane {
        let raw: Vec<u8> = (0..h).flat_map(|_| (0..w).flat_map(|x| if x % 2 == 0 { [0, 0, 0, 255] } else { [255; 4] })).collect();
        Plane::from_raw(w, h, 4, &raw, [0; 4])
    }

    fn mean_sd(v: &[f64]) -> (f64, f64) {
        let m = v.iter().sum::<f64>() / v.len() as f64;
        (m, (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64).sqrt())
    }

    /// 1-px black/white stripes have no frequency an output grid coarser
    /// than the source can show, so every zoom-out must read flat grey.
    /// Before the fix steps in (1, 2) sampled level 0 bilinearly and gave a
    /// standard deviation of 64-73 (bands of moiré). The bound of 8 (3% of
    /// full scale, invisible as texture) leaves room for the B-spline's small
    /// residual response above Nyquist (6.5 at step 1.3). Steps within 1.25×
    /// above a power of two fade in from bilinear on purpose (sharpness near
    /// exact mip sizes) and are not held to this bound.
    #[test]
    fn zoomed_out_stripes_do_not_alias() {
        let p = stripes(2048, 64);
        for step in [1.3f64, 1.5, 1.9, 2.6, 3.0, 3.9] {
            let ow = (2048.0 / step) as usize - 4;
            let mut out = vec![0f32; ow * 4 * 4];
            p.resample(0.3, 16.0, step, ow, 4, &mut out);
            let row: Vec<f64> = (2..ow - 2).map(|i| out[(ow + i) * 4] as f64 * 255.0).collect();
            let (m, sd) = mean_sd(&row);
            assert!(sd < 8.0, "step {step}: stripes alias, sd {sd:.1}");
            assert!((m - 127.5).abs() < 2.0, "step {step}: mean {m:.1}");
        }
    }

    #[test]
    fn zoomed_out_filter_keeps_edges_and_alpha() {
        // A hard edge shrunk by 1.5 stays within a few output pixels, and a
        // transparent neighbour does not darken opaque red.
        let mut p = Plane::transparent(600, 8);
        let red: Vec<u8> = (0..300 * 8).flat_map(|_| [255, 0, 0, 255]).collect();
        p.write(Rect::new(0, 0, 300, 8), &red);
        let ow = 400;
        let mut out = vec![0f32; ow * 2 * 4];
        p.resample(0.0, 2.0, 1.5, ow, 2, &mut out);
        let soft = (0..ow).filter(|&i| (0.05..0.95).contains(&out[i * 4 + 3])).count();
        assert!(soft <= 4, "edge smeared over {soft} px");
        for i in 0..ow {
            let px = &out[i * 4..i * 4 + 4];
            if px[3] > 0.01 {
                assert!(px[0] > 0.999 && px[1] < 1e-3, "colour bled at {i}: {px:?}");
            }
        }
        // Single-channel planes use the same path.
        let mut m = Plane::mask(600, 8, 0);
        m.write(Rect::new(0, 0, 300, 8), &[255; 300 * 8]);
        let mut mo = vec![0f32; ow * 2];
        m.resample(0.0, 2.0, 1.5, ow, 2, &mut mo);
        assert!((mo[10] - 1.0).abs() < 1e-4 && mo[390] < 1e-4);
    }

    #[test]
    fn nearest_resample_matches_pixels() {
        let data: Vec<u8> = (0..16).flat_map(|i| [i as u8 * 10, 0, 0, 255]).collect();
        let p = Plane::from_raw(4, 4, 4, &data, [0; 4]);
        let mut out = vec![0f32; 4 * 4 * 4];
        p.resample(0.0, 0.0, 1.0, 4, 4, &mut out);
        assert!((out[4 * 5] * 255.0 - 50.0).abs() < 0.01);
    }
}
