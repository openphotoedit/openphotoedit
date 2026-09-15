//! Plumbing every filter shares: which rectangle a filter covers, running
//! it through `pixels::edit_layer`, edge padding, float planes with
//! premultiplied alpha, bilinear sampling and a stable hash.

use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::LayerId;
use editor_core::ops::{target_id, Applied, EditorError, Result};
use editor_core::pixels::{default_rect, edit_layer, EditScope};
use editor_core::selection;

pub fn canvas(doc: &Document) -> Rect {
    Rect::new(0, 0, doc.width as i32, doc.height as i32)
}

/// What part of the layer a filter covers.
#[derive(Clone, Copy, Debug)]
pub enum Area {
    /// The selection's bounds; without a selection, the layer's pixels grown
    /// by `reach` (a blur spreads past the content it started from).
    Content(i32),
    /// The selection's bounds, or the whole canvas (generators, Offset).
    Canvas,
}

pub fn area_rect(doc: &Document, id: LayerId, area: Area) -> Result<Rect> {
    let c = canvas(doc);
    match area {
        Area::Content(reach) => {
            let base = default_rect(doc, id)?;
            if doc.selection.is_some() || base.is_empty() {
                Ok(base)
            } else {
                Ok(base.inflate(reach.max(0)).intersect(&c))
            }
        }
        Area::Canvas => {
            doc.find(id).ok_or(EditorError::NoLayer(id))?;
            Ok(selection::bounds(doc).intersect(&c))
        }
    }
}

/// Where the buffer handed to a filter sits.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    /// Document rectangle of the whole buffer (written rect plus apron).
    pub outer: Rect,
    /// The written rectangle, in buffer coordinates.
    pub inner: Rect,
    /// The part of the buffer inside the canvas, in buffer coordinates.
    pub canvas: Rect,
}

impl Frame {
    /// Document coordinates of a buffer pixel's top-left corner.
    pub fn doc_x(&self, x: usize) -> i64 {
        self.outer.x as i64 + x as i64
    }
    pub fn doc_y(&self, y: usize) -> i64 {
        self.outer.y as i64 + y as i64
    }
}

/// Run a destructive filter on one layer.
///
/// `apron` extra pixels are read around the written rectangle so kernels see
/// real neighbours at selection and tile edges. With `pad`, pixels beyond the
/// canvas repeat the canvas edge, as Photoshop does, so blurs do not darken
/// toward the image border.
pub fn run(
    doc: &mut Document,
    id: Option<LayerId>,
    area: Area,
    apron: i32,
    pad: bool,
    label: &str,
    f: impl FnOnce(&mut [u8], &Frame),
) -> Result<Applied> {
    let id = target_id(doc, id)?;
    let rect = area_rect(doc, id, area)?;
    run_rect(doc, id, rect, apron, pad, label, f)
}

pub fn run_rect(
    doc: &mut Document,
    id: LayerId,
    rect: Rect,
    apron: i32,
    pad: bool,
    label: &str,
    f: impl FnOnce(&mut [u8], &Frame),
) -> Result<Applied> {
    let apron = apron.max(0);
    if rect.is_empty() {
        // Surfaces locks and "not a pixel layer" before the emptiness error.
        edit_layer(doc, id, EditScope { rect: Rect::empty(), apron: 0 }, |_, _, _| {})?;
        let msg = if doc.selection.is_some() {
            "the selected area is empty"
        } else {
            "the layer has no pixels to filter"
        };
        return Err(EditorError::Invalid(msg.into()));
    }
    let outer = rect.inflate(apron);
    let frame = Frame {
        w: outer.w as usize,
        h: outer.h as usize,
        outer,
        inner: Rect::new(apron, apron, rect.w, rect.h),
        canvas: canvas(doc).intersect(&outer).translate(-outer.x, -outer.y),
    };
    let dirty = edit_layer(doc, id, EditScope { rect, apron }, |buf, w, h| {
        debug_assert_eq!((w, h), (frame.w, frame.h));
        if pad {
            pad_clamp(buf, w, h, frame.canvas);
        }
        f(buf, &frame);
    })?;
    Ok(Applied::step(label).dirty(dirty))
}

/// Replace pixels outside `keep` with the nearest pixel inside it.
pub fn pad_clamp(buf: &mut [u8], w: usize, h: usize, keep: Rect) {
    if keep.is_empty() || (keep.x == 0 && keep.y == 0 && keep.w as usize == w && keep.h as usize == h) {
        return;
    }
    let (x0, x1) = (keep.x as usize, keep.right() as usize);
    let (y0, y1) = (keep.y as usize, keep.bottom() as usize);
    for y in y0..y1 {
        let row = y * w;
        let (l, r) = (row + x0, row + x1 - 1);
        let (lp, rp) = ([buf[l * 4], buf[l * 4 + 1], buf[l * 4 + 2], buf[l * 4 + 3]], [buf[r * 4], buf[r * 4 + 1], buf[r * 4 + 2], buf[r * 4 + 3]]);
        for x in 0..x0 {
            buf[(row + x) * 4..(row + x) * 4 + 4].copy_from_slice(&lp);
        }
        for x in x1..w {
            buf[(row + x) * 4..(row + x) * 4 + 4].copy_from_slice(&rp);
        }
    }
    let src_top = buf[y0 * w * 4..(y0 + 1) * w * 4].to_vec();
    for y in 0..y0 {
        buf[y * w * 4..(y + 1) * w * 4].copy_from_slice(&src_top);
    }
    let src_bot = buf[(y1 - 1) * w * 4..y1 * w * 4].to_vec();
    for y in y1..h {
        buf[y * w * 4..(y + 1) * w * 4].copy_from_slice(&src_bot);
    }
}

pub fn is_opaque(buf: &[u8]) -> bool {
    buf.chunks_exact(4).all(|p| p[3] == 255)
}

#[inline]
pub fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 255.0) + 0.5) as u8
}

pub fn channel(buf: &[u8], c: usize) -> Vec<f32> {
    buf.chunks_exact(4).map(|p| p[c] as f32).collect()
}

pub fn store(buf: &mut [u8], c: usize, plane: &[f32]) {
    for (p, v) in buf.chunks_exact_mut(4).zip(plane) {
        p[c] = to_u8(*v);
    }
}

/// Apply a linear, channel-independent operation (a convolution) to RGBA.
/// Colour is filtered premultiplied so transparent pixels do not bleed
/// black into their neighbours; fully opaque buffers skip that work.
pub fn linear_filter(buf: &mut [u8], w: usize, h: usize, mut f: impl FnMut(&mut [f32], usize, usize)) {
    if is_opaque(buf) {
        for c in 0..3 {
            let mut p = channel(buf, c);
            f(&mut p, w, h);
            store(buf, c, &p);
        }
        return;
    }
    let alpha = channel(buf, 3);
    let mut blurred_a = alpha.clone();
    f(&mut blurred_a, w, h);
    for c in 0..3 {
        let mut p: Vec<f32> = buf.chunks_exact(4).zip(&alpha).map(|(px, a)| px[c] as f32 * a / 255.0).collect();
        f(&mut p, w, h);
        for ((px, v), a) in buf.chunks_exact_mut(4).zip(&p).zip(&blurred_a) {
            px[c] = if *a > 0.5 { to_u8(v * 255.0 / a) } else { 0 };
        }
    }
    store(buf, 3, &blurred_a);
}

/// Straight RGBA float planes (0..255) out of a buffer and back, for
/// filters that need all channels together.
pub fn premultiplied(buf: &[u8]) -> Vec<f32> {
    let mut out = vec![0f32; buf.len()];
    for (o, p) in out.chunks_exact_mut(4).zip(buf.chunks_exact(4)) {
        let a = p[3] as f32 / 255.0;
        o[0] = p[0] as f32 * a;
        o[1] = p[1] as f32 * a;
        o[2] = p[2] as f32 * a;
        o[3] = p[3] as f32;
    }
    out
}

/// Write a premultiplied pixel (0..255) as straight RGBA8.
#[inline]
pub fn put_premultiplied(dst: &mut [u8], p: [f32; 4]) {
    let a = p[3];
    if a <= 0.5 {
        dst[..4].fill(0);
        return;
    }
    let k = 255.0 / a;
    dst[0] = to_u8(p[0] * k);
    dst[1] = to_u8(p[1] * k);
    dst[2] = to_u8(p[2] * k);
    dst[3] = to_u8(a);
}

/// Bilinear sampling of a straight RGBA8 buffer, returning premultiplied
/// values. Coordinates are continuous (pixel centres at +0.5) and clamp to
/// the buffer edge.
pub struct Sampler<'a> {
    pub src: &'a [u8],
    pub w: usize,
    pub h: usize,
}

impl Sampler<'_> {
    #[inline]
    pub fn premul(&self, x: usize, y: usize) -> [f32; 4] {
        let i = (y * self.w + x) * 4;
        let p = &self.src[i..i + 4];
        let a = p[3] as f32;
        if a >= 255.0 {
            return [p[0] as f32, p[1] as f32, p[2] as f32, 255.0];
        }
        let k = a / 255.0;
        [p[0] as f32 * k, p[1] as f32 * k, p[2] as f32 * k, a]
    }

    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        let fx = (x - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = (y - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let a = self.premul(x0, y0);
        let b = self.premul(x1, y0);
        let c = self.premul(x0, y1);
        let d = self.premul(x1, y1);
        let mut out = [0f32; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (d[k] - c[k]) * tx;
            out[k] = top + (bot - top) * ty;
        }
        out
    }

    /// Sample with wrap-around (Polar Coordinates, Offset).
    #[inline]
    pub fn sample_wrap_x(&self, x: f32, y: f32) -> [f32; 4] {
        let w = self.w as f32;
        let fx = (x - 0.5).rem_euclid(w);
        let fy = (y - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx as usize % self.w, fy as usize);
        let (x1, y1) = ((x0 + 1) % self.w, (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - fx.floor(), fy - y0 as f32);
        let a = self.premul(x0, y0);
        let b = self.premul(x1, y0);
        let c = self.premul(x0, y1);
        let d = self.premul(x1, y1);
        let mut out = [0f32; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (d[k] - c[k]) * tx;
            out[k] = top + (bot - top) * ty;
        }
        out
    }
}

/// A stable 64-bit hash of a document position and a seed, so noise is the
/// same however the image is tiled or selected.
#[inline]
pub fn hash3(x: i64, y: i64, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    h
}

/// Uniform in [0, 1).
#[inline]
pub fn unit(h: u64) -> f32 {
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Rec. 601 luma of an RGBA8 pixel, 0..255.
#[inline]
pub fn luma8(p: &[u8]) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// A small xorshift generator for algorithms that need randomness but not
/// position-stable noise (PatchMatch).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() >> 11) % n.max(1) as u64) as usize
    }
}
