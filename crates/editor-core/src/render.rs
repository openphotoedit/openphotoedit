//! The CPU compositor.
//!
//! Renders any rectangle of the document at any scale into a straight-alpha
//! RGBA buffer. The same code produces the zoomed-out viewport, a 1:1 patch
//! under the brush and the full-resolution export, which is what makes it
//! the reference the GPU path (later) is tested against.
//!
//! A viewport render keeps a checkpoint of the composite *below* the layer
//! that last changed, so dragging a slider on one adjustment layer
//! recomposites only from that layer up.

use crate::adjust::{gradient_at, ApplyCtx};
use crate::blend::{composite_px, dissolve_noise, BlendMode};
use crate::document::Document;
use crate::geom::Rect;
use crate::layer::{Fill, GradientKind, Layer, LayerKind, LayerMask};

/// A request for pixels: document rectangle `(x, y)` onward, at `scale`
/// output pixels per document pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub width: usize,
    pub height: usize,
}

impl View {
    pub fn full(doc: &Document) -> View {
        View { x: 0.0, y: 0.0, scale: 1.0, width: doc.width as usize, height: doc.height as usize }
    }
    fn ctx(&self, doc: &Document) -> ApplyCtx {
        ApplyCtx {
            x0: self.x,
            y0: self.y,
            step: 1.0 / self.scale,
            doc_w: doc.width,
            doc_h: doc.height,
            width: self.width,
            height: self.height,
        }
    }
}

struct Checkpoint {
    view: View,
    /// Signatures of the top-level layers below `index`.
    sigs: Vec<u64>,
    index: usize,
    buf: Vec<f32>,
}

#[derive(Default)]
pub struct Renderer {
    checkpoints: Vec<Checkpoint>,
    last: Option<(View, Vec<u64>)>,
}

fn signature(l: &Layer) -> u64 {
    l.rev ^ ((l.id as u64) << 40)
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    pub fn invalidate(&mut self) {
        self.checkpoints.clear();
        self.last = None;
    }

    /// Render into straight RGBA8.
    pub fn render(&mut self, doc: &Document, view: View, out: &mut [u8]) {
        let buf = self.render_f32(doc, view);
        to_u8(&buf, out);
    }

    pub fn render_f32(&mut self, doc: &Document, view: View) -> Vec<f32> {
        let sigs: Vec<u64> = doc.layers.iter().map(signature).collect();
        let ctx = view.ctx(doc);

        // First index whose signature differs from the previous render of
        // the same view: the layer being edited.
        let changed_at = match &self.last {
            Some((v, prev)) if *v == view => {
                let n = prev.len().min(sigs.len());
                (0..n).find(|&i| prev[i] != sigs[i]).unwrap_or(n)
            }
            _ => 0,
        };

        let mut start = 0;
        let mut buf = None;
        if let Some(cp) = self
            .checkpoints
            .iter()
            .filter(|cp| cp.view == view && cp.index <= sigs.len() && cp.sigs[..] == sigs[..cp.index])
            .max_by_key(|cp| cp.index)
        {
            start = cp.index;
            buf = Some(cp.buf.clone());
        }
        let mut buf = buf.unwrap_or_else(|| vec![0f32; view.width * view.height * 4]);

        // Checkpoint just below the edited layer, provided it is the start
        // of a clipping run (clipped layers render with their base).
        let mut save_at = if changed_at > start && changed_at < sigs.len() { Some(changed_at) } else { None };
        if let Some(i) = save_at {
            if doc.layers[i].clip {
                save_at = (1..=i).rev().find(|&k| !doc.layers[k].clip).filter(|&k| k > start);
            }
        }

        let mut i = start;
        let mut fresh = start == 0;
        while i < doc.layers.len() {
            if save_at == Some(i) {
                self.checkpoints.retain(|cp| cp.view == view);
                self.checkpoints.push(Checkpoint { view, sigs: sigs[..i].to_vec(), index: i, buf: buf.clone() });
                if self.checkpoints.len() > 2 {
                    self.checkpoints.remove(0);
                }
            }
            let drew = doc.layers[i].visible;
            i = composite_run_at(&doc.layers, i, &mut buf, &ctx, doc, fresh);
            fresh &= !drew;
        }
        self.last = Some((view, sigs));
        buf
    }
}

/// Render without caching.
pub fn render_view(doc: &Document, view: View) -> Vec<f32> {
    let ctx = view.ctx(doc);
    let mut buf = vec![0f32; view.width * view.height * 4];
    render_list_fresh(&doc.layers, &mut buf, &ctx, doc);
    buf
}

/// Render a list of layers (a group's children, or a single layer) onto a
/// transparent buffer.
pub fn render_layers(layers: &[Layer], doc: &Document, view: View) -> Vec<f32> {
    let ctx = view.ctx(doc);
    let mut buf = vec![0f32; view.width * view.height * 4];
    render_list_fresh(layers, &mut buf, &ctx, doc);
    buf
}

/// Composite a list onto a buffer that starts fully transparent.
fn render_list_fresh(layers: &[Layer], buf: &mut [f32], ctx: &ApplyCtx, doc: &Document) {
    let mut i = 0;
    let mut fresh = true;
    while i < layers.len() {
        let drew = layers[i].visible;
        i = composite_run_at(layers, i, buf, ctx, doc, fresh);
        fresh &= !drew;
    }
}

/// Flatten the whole document at full resolution, in horizontal strips so
/// peak float memory stays bounded.
pub fn flatten(doc: &Document) -> Vec<u8> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let mut out = vec![0u8; w * h * 4];
    let strip = (4_000_000 / w.max(1)).clamp(1, 1024);
    let mut y = 0;
    while y < h {
        let rows = strip.min(h - y);
        let view = View { x: 0.0, y: y as f64, scale: 1.0, width: w, height: rows };
        let buf = render_view(doc, view);
        to_u8(&buf, &mut out[y * w * 4..(y + rows) * w * 4]);
        y += rows;
    }
    out
}

pub fn to_u8(buf: &[f32], out: &mut [u8]) {
    for (o, v) in out.iter_mut().zip(buf.iter()) {
        *o = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
}

/// Composite layer `i` and any layers clipped to it. Returns the index of
/// the next unprocessed layer.
fn composite_run(layers: &[Layer], i: usize, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document) -> usize {
    composite_run_at(layers, i, buf, ctx, doc, false)
}

/// `fresh`: the buffer is known to be fully transparent.
fn composite_run_at(layers: &[Layer], i: usize, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document, fresh: bool) -> usize {
    let base = &layers[i];
    let mut end = i + 1;
    while end < layers.len() && layers[end].clip {
        end += 1;
    }
    let clipped = &layers[i + 1..end];
    let base_takes_clip = !matches!(base.kind, LayerKind::Adjustment(_)) && !base.clip;
    if clipped.is_empty() || !base_takes_clip {
        if base.visible {
            composite_layer(base, buf, ctx, doc, fresh);
        }
        if !base_takes_clip {
            // An adjustment layer cannot be a clipping base: the clipped
            // layers composite normally.
            for l in clipped {
                if l.visible {
                    composite_layer(l, buf, ctx, doc, false);
                }
            }
        }
        return end;
    }
    if !base.visible {
        // Hiding the base hides everything clipped to it.
        return end;
    }
    // "Blend clipped layers as group": base content alone, clipped layers on
    // top of it with alpha locked to the base, then the result composites
    // with the base's blend mode and opacity.
    let n = ctx.width * ctx.height;
    let mut group = vec![0f32; n * 4];
    let mut plain = base.clone_shallow_for_clip();
    plain.opacity = base.fill_opacity;
    plain.blend = BlendMode::Normal;
    composite_layer(&plain, &mut group, ctx, doc, true);
    let alpha: Vec<f32> = group.chunks_exact(4).map(|p| p[3]).collect();
    for l in clipped.iter().filter(|l| l.visible) {
        composite_layer(l, &mut group, ctx, doc, false);
        for (p, a) in group.chunks_exact_mut(4).zip(alpha.iter()) {
            p[3] = *a;
        }
    }
    blend_buffer(buf, &group, None, base.opacity, base.blend, ctx);
    end
}

impl Layer {
    /// A copy for rendering the clip base on its own. Pixel data is shared.
    fn clone_shallow_for_clip(&self) -> Layer {
        self.clone()
    }
}

fn composite_layer(layer: &Layer, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document, fresh: bool) {
    let n = ctx.width * ctx.height;
    let mask = layer.mask.as_ref().filter(|m| m.enabled).map(|m| sample_mask(m, ctx));
    match &layer.kind {
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. }
            if fresh && mask.is_none() && layer.blend == BlendMode::Normal && layer.opacity * layer.fill_opacity >= 1.0 =>
        {
            // First layer onto a transparent buffer: the result is the layer.
            r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, buf);
        }
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } => {
            let mut src = vec![0f32; n * 4];
            r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, &mut src);
            blend_buffer(buf, &src, mask.as_deref(), layer.opacity * layer.fill_opacity, layer.blend, ctx);
        }
        LayerKind::Fill(fill) => {
            let src = render_fill(fill, ctx);
            blend_buffer(buf, &src, mask.as_deref(), layer.opacity * layer.fill_opacity, layer.blend, ctx);
        }
        LayerKind::Adjustment(adj) if mask.is_none() && layer.blend == BlendMode::Normal && layer.opacity * layer.fill_opacity >= 1.0 => {
            // The common case: full strength, no mask. Adjust in place.
            adj.apply(buf, ctx);
        }
        LayerKind::Adjustment(adj) => {
            let mut adjusted = buf.to_vec();
            adj.apply(&mut adjusted, ctx);
            let op = layer.opacity * layer.fill_opacity;
            for k in 0..n {
                let o = k * 4;
                let a = op * mask.as_ref().map_or(1.0, |m| m[k]);
                if a <= 0.0 || buf[o + 3] <= 0.0 {
                    continue;
                }
                let cb = [buf[o], buf[o + 1], buf[o + 2]];
                let cs = [adjusted[o], adjusted[o + 1], adjusted[o + 2]];
                let mixed = if layer.blend == BlendMode::Normal { cs } else { layer.blend.mix(cb, cs) };
                for c in 0..3 {
                    buf[o + c] = cb[c] + (mixed[c] - cb[c]) * a;
                }
            }
        }
        LayerKind::Group { children, pass_through, .. } => {
            let op = layer.opacity;
            if *pass_through && layer.blend == BlendMode::Normal {
                let before = if op < 1.0 || mask.is_some() { Some(buf.to_vec()) } else { None };
                let mut i = 0;
                while i < children.len() {
                    i = composite_run(children, i, buf, ctx, doc);
                }
                if let Some(before) = before {
                    for k in 0..n {
                        let a = op * mask.as_ref().map_or(1.0, |m| m[k]);
                        let o = k * 4;
                        for c in 0..4 {
                            buf[o + c] = before[o + c] + (buf[o + c] - before[o + c]) * a;
                        }
                    }
                }
            } else {
                let mut group = vec![0f32; n * 4];
                render_list_fresh(children, &mut group, ctx, doc);
                blend_buffer(buf, &group, mask.as_deref(), op, layer.blend, ctx);
            }
        }
    }
}

/// Composite `src` over `buf` with a blend mode, scalar opacity and optional
/// per-pixel mask.
fn blend_buffer(buf: &mut [f32], src: &[f32], mask: Option<&[f32]>, opacity: f32, mode: BlendMode, ctx: &ApplyCtx) {
    let w = ctx.width;
    if mode == BlendMode::Normal {
        // Source-over without the general blend machinery.
        for k in 0..(ctx.width * ctx.height) {
            let o = k * 4;
            let mut sa = src[o + 3] * opacity;
            if let Some(m) = mask {
                sa *= m[k];
            }
            if sa <= 0.0 {
                continue;
            }
            let d = &mut buf[o..o + 4];
            if sa >= 1.0 {
                d.copy_from_slice(&[src[o], src[o + 1], src[o + 2], 1.0]);
                continue;
            }
            let ab = d[3] * (1.0 - sa);
            let ao = sa + ab;
            let inv = 1.0 / ao;
            d[0] = (src[o] * sa + d[0] * ab) * inv;
            d[1] = (src[o + 1] * sa + d[1] * ab) * inv;
            d[2] = (src[o + 2] * sa + d[2] * ab) * inv;
            d[3] = ao;
        }
        return;
    }
    for k in 0..(ctx.width * ctx.height) {
        let o = k * 4;
        let mut sa = src[o + 3] * opacity;
        if let Some(m) = mask {
            sa *= m[k];
        }
        if sa <= 0.0 {
            continue;
        }
        if mode == BlendMode::Dissolve {
            let dx = (ctx.x0 + ((k % w) as f64 + 0.5) * ctx.step).floor() as i64;
            let dy = (ctx.y0 + ((k / w) as f64 + 0.5) * ctx.step).floor() as i64;
            sa = if dissolve_noise(dx, dy) < sa { 1.0 } else { 0.0 };
            if sa <= 0.0 {
                continue;
            }
        }
        composite_px(&mut buf[o..o + 4], [src[o], src[o + 1], src[o + 2]], sa, mode);
    }
}

fn sample_mask(mask: &LayerMask, ctx: &ApplyCtx) -> Vec<f32> {
    let mut m = vec![0f32; ctx.width * ctx.height];
    let r = &mask.raster;
    r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, &mut m);
    if mask.density < 1.0 {
        for v in m.iter_mut() {
            *v = 1.0 - mask.density * (1.0 - *v);
        }
    }
    m
}

fn render_fill(fill: &Fill, ctx: &ApplyCtx) -> Vec<f32> {
    let n = ctx.width * ctx.height;
    let mut out = vec![0f32; n * 4];
    match fill {
        Fill::Solid { color } => {
            let c = color.to_f32();
            let a = color.alpha_f32();
            for px in out.chunks_exact_mut(4) {
                px.copy_from_slice(&[c[0], c[1], c[2], a]);
            }
        }
        Fill::Gradient { stops, gradient, from, to, reverse } => {
            let table: Vec<[f32; 4]> = (0..512).map(|i| gradient_at(stops, i as f32 / 511.0)).collect();
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            let len2 = (dx * dx + dy * dy).max(1e-9);
            let len = len2.sqrt();
            for j in 0..ctx.height {
                let py = ctx.y0 + (j as f64 + 0.5) * ctx.step - from.y;
                for i in 0..ctx.width {
                    let px = ctx.x0 + (i as f64 + 0.5) * ctx.step - from.x;
                    let t = match gradient {
                        GradientKind::Linear => (px * dx + py * dy) / len2,
                        GradientKind::Reflected => ((px * dx + py * dy) / len2).abs(),
                        GradientKind::Radial => (px * px + py * py).sqrt() / len,
                        GradientKind::Diamond => {
                            // Distance along the axis and across it, max-norm.
                            let u = (px * dx + py * dy) / len;
                            let v = (-px * dy + py * dx) / len;
                            u.abs().max(v.abs()) / len
                        }
                        GradientKind::Angle => {
                            let a0 = dy.atan2(dx);
                            let a = py.atan2(px);
                            (a - a0).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
                        }
                    };
                    let mut t = t.clamp(0.0, 1.0) as f32;
                    if *reverse {
                        t = 1.0 - t;
                    }
                    let c = table[(t * 511.0 + 0.5) as usize];
                    let o = (j * ctx.width + i) * 4;
                    out[o..o + 4].copy_from_slice(&c);
                }
            }
        }
    }
    out
}

/// Render a single layer's own content (for thumbnails and "sample current
/// layer" tools), ignoring its blend mode and opacity.
pub fn render_layer_alone(layer: &Layer, doc: &Document, view: View) -> Vec<f32> {
    let mut l = layer.clone();
    l.opacity = 1.0;
    l.fill_opacity = 1.0;
    l.blend = BlendMode::Normal;
    l.visible = true;
    l.clip = false;
    if let LayerKind::Adjustment(_) = l.kind {
        return vec![0f32; view.width * view.height * 4];
    }
    render_layers(std::slice::from_ref(&l), doc, view)
}

/// Bounds the renderer would draw for this layer (whole canvas for layers
/// without their own pixels).
pub fn layer_extent(layer: &Layer, doc: &Document) -> Rect {
    let canvas = Rect::new(0, 0, doc.width as i32, doc.height as i32);
    layer.content_bounds().unwrap_or(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adjust::{Adjustment, Levels};
    use crate::color::Rgba8;
    use crate::layer::{LayerMask, Raster};
    use crate::plane::Plane;

    fn solid_layer(doc: &mut Document, rgba: [u8; 4]) -> Layer {
        let (w, h) = (doc.width, doc.height);
        let raw: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
        let id = doc.alloc_id();
        Layer::new(id, "px", LayerKind::Pixel(Raster::new(Plane::from_raw(w, h, 4, &raw, [0; 4]), 0, 0)))
    }

    fn px_at(doc: &Document, x: usize, y: usize) -> [u8; 4] {
        let out = flatten(doc);
        let i = (y * doc.width as usize + x) * 4;
        [out[i], out[i + 1], out[i + 2], out[i + 3]]
    }

    #[test]
    fn empty_document_is_transparent() {
        let doc = Document::new(8, 8);
        assert_eq!(px_at(&doc, 3, 3), [0, 0, 0, 0]);
    }

    #[test]
    fn half_opacity_red_over_white() {
        let mut doc = Document::new(4, 4);
        let bg = solid_layer(&mut doc, [255, 255, 255, 255]);
        doc.layers.push(bg);
        let mut red = solid_layer(&mut doc, [255, 0, 0, 255]);
        red.opacity = 0.5;
        doc.layers.push(red);
        let p = px_at(&doc, 1, 1);
        assert_eq!(p[0], 255);
        assert!((p[1] as i32 - 128).abs() <= 1, "{p:?}");
        assert_eq!(p[3], 255);
    }

    #[test]
    fn adjustment_layer_with_mask() {
        let mut doc = Document::new(4, 1);
        let bg = solid_layer(&mut doc, [100, 100, 100, 255]);
        doc.layers.push(bg);
        let id = doc.alloc_id();
        let mut adj = Layer::new(id, "inv", LayerKind::Adjustment(Adjustment::Invert));
        let mut mask = LayerMask::hide_all(4, 1);
        mask.raster.plane.write(Rect::new(0, 0, 2, 1), &[255, 255]);
        adj.mask = Some(mask);
        doc.layers.push(adj);
        assert_eq!(px_at(&doc, 0, 0)[0], 155);
        assert_eq!(px_at(&doc, 3, 0)[0], 100);
    }

    #[test]
    fn clipping_mask_limits_to_base_alpha() {
        let mut doc = Document::new(4, 1);
        let id = doc.alloc_id();
        let mut base_plane = Plane::transparent(4, 1);
        base_plane.write(Rect::new(0, 0, 2, 1), &[0, 0, 255, 255, 0, 0, 255, 255]);
        doc.layers.push(Layer::new(id, "base", LayerKind::Pixel(Raster::new(base_plane, 0, 0))));
        let mut top = solid_layer(&mut doc, [255, 0, 0, 255]);
        top.clip = true;
        doc.layers.push(top);
        assert_eq!(px_at(&doc, 0, 0), [255, 0, 0, 255]);
        assert_eq!(px_at(&doc, 3, 0)[3], 0, "outside the base stays transparent");
    }

    #[test]
    fn isolated_group_multiply() {
        let mut doc = Document::new(2, 2);
        let bg = solid_layer(&mut doc, [200, 200, 200, 255]);
        doc.layers.push(bg);
        let child = solid_layer(&mut doc, [128, 128, 128, 255]);
        let gid = doc.alloc_id();
        let mut g = Layer::new(gid, "g", LayerKind::Group { children: vec![child], pass_through: false, expanded: true });
        g.blend = BlendMode::Multiply;
        doc.layers.push(g);
        let p = px_at(&doc, 0, 0);
        assert!((p[0] as i32 - (200 * 128 / 255)).abs() <= 1, "{p:?}");
    }

    #[test]
    fn solid_fill_layer() {
        let mut doc = Document::new(2, 2);
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "fill", LayerKind::Fill(Fill::Solid { color: Rgba8::rgb(10, 20, 30) })));
        assert_eq!(px_at(&doc, 1, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn checkpoint_render_matches_uncached() {
        let mut doc = Document::new(64, 48);
        let bg = solid_layer(&mut doc, [90, 120, 200, 255]);
        doc.layers.push(bg);
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "levels", LayerKind::Adjustment(Adjustment::Levels(Levels::default()))));
        let id2 = doc.alloc_id();
        doc.layers.push(Layer::new(id2, "inv", LayerKind::Adjustment(Adjustment::Invert)));
        let view = View { x: 3.0, y: 2.0, scale: 0.5, width: 30, height: 20 };
        let mut r = Renderer::new();
        let _ = r.render_f32(&doc, view);
        for gamma in [0.5f32, 1.7, 2.2] {
            if let Some(LayerKind::Adjustment(Adjustment::Levels(l))) = doc.find_mut(id).map(|l| &mut l.kind) {
                l.master.gamma = gamma;
            }
            let cached = r.render_f32(&doc, view);
            let fresh = render_view(&doc, view);
            assert_eq!(cached, fresh);
        }
        assert!(!r.checkpoints.is_empty(), "a checkpoint was kept");
    }
}
