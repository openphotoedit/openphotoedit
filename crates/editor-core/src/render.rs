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
//!
//! Beyond plain source-over it models Photoshop's group semantics: fill
//! opacity on a group, isolated versus pass-through groups, clipping runs,
//! knockout, and the rule that a pass-through group stops passing its
//! adjustments down once its fill opacity drops. See `Scope` below.

use crate::adjust::{gradient_at, ApplyCtx};
use crate::blend::{composite_px, dissolve_noise, BlendMode};
use crate::document::Document;
use crate::geom::Rect;
use crate::layer::{Fill, GradientKind, Knockout, Layer, LayerKind, LayerMask};

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

        let floors = Floors::of(doc, &ctx);
        let mut scope = floors.root();
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
            i = composite_run_at(&doc.layers, i, &mut buf, &ctx, doc, fresh, &mut scope);
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
    let floors = Floors::of(doc, &ctx);
    composite_list(&doc.layers, &mut buf, &ctx, doc, true, &mut floors.root());
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

/// Composite a list onto a buffer that starts fully transparent, ignoring
/// knockout (for thumbnails and the layer-style apron, neither of which has
/// a document backdrop to punch through to).
fn render_list_fresh(layers: &[Layer], buf: &mut [f32], ctx: &ApplyCtx, doc: &Document) {
    composite_list(layers, buf, ctx, doc, true, &mut Scope::detached());
}

// ---------------------------------------------------------------------------
// Compositing scope
//
// Two of Photoshop's advanced blending settings need to know what the
// backdrop was when the group the layer sits in opened.
//
//  * **Knockout** punches a hole through everything beneath the layer and
//    shows a lower backdrop instead. *Shallow* shows the canvas as it stood
//    when the layer's own group opened; *deep* shows the document's
//    Background layer. Deep knockout travels out through pass-through groups
//    and stops at the first isolated one, because an isolated group is an
//    isolation boundary and a pass-through group is not. The hole is cut by
//    the layer's alpha; its fill opacity only decides how much of the layer
//    paints back over the hole.
//
//  * **Adjustment isolation.** A pass-through group normally lets an
//    adjustment inside it reach everything below the group. Once the group's
//    *fill* opacity drops below 100%, or something is clipped to the group,
//    Photoshop stops that: the adjustment then reaches only the group's own
//    contents, and the group composites as an ordinary source. `shape` is
//    the coverage the group's own layers have accumulated, which is exactly
//    what the adjustment is confined to.

/// The two backdrops knockout can punch through to, held for the whole
/// render so the borrow in every `Scope` can point at them.
struct Floors {
    /// Nothing in the document knocks out, so no scope needs a snapshot.
    live: bool,
    /// Fully transparent: the root scope's floor.
    zero: Vec<f32>,
    /// The Background layer, opaque: deep knockout's floor. `None` when the
    /// document has no Background layer, and deep then degrades to shallow.
    deep: Option<Vec<f32>>,
}

impl Floors {
    fn of(doc: &Document, ctx: &ApplyCtx) -> Floors {
        let live = any_knockout(&doc.layers);
        Floors {
            live,
            zero: if live { vec![0f32; ctx.width * ctx.height * 4] } else { Vec::new() },
            deep: if live { document_backdrop(doc, ctx) } else { None },
        }
    }
    fn root(&self) -> Scope<'_> {
        Scope {
            base: self.live.then_some(self.zero.as_slice()),
            deep: self.deep.as_deref(),
            live: self.live,
            isolate_adjust: false,
            shape: None,
        }
    }
}

struct Scope<'a> {
    /// The canvas as this scope opened: shallow knockout's floor.
    base: Option<&'a [f32]>,
    /// The document backdrop: deep knockout's floor.
    deep: Option<&'a [f32]>,
    /// The document knocks out somewhere, so scopes snapshot their backdrop.
    live: bool,
    /// Adjustments in this scope are confined to `shape`.
    isolate_adjust: bool,
    /// Coverage accumulated by this scope's own layers.
    shape: Option<Vec<f32>>,
}

impl<'a> Scope<'a> {
    fn detached() -> Scope<'static> {
        Scope { base: None, deep: None, live: false, isolate_adjust: false, shape: None }
    }
    /// What `knockout` shows through the hole.
    fn floor(&self, knockout: Knockout) -> Option<&'a [f32]> {
        match knockout {
            Knockout::None => None,
            Knockout::Deep => self.deep.or(self.base),
            Knockout::Shallow => self.base,
        }
    }
    fn wants_shape(&self) -> bool {
        self.shape.is_some()
    }
    fn add_shape(&mut self, cover: &[f32]) {
        if let Some(s) = self.shape.as_mut() {
            for (a, b) in s.iter_mut().zip(cover.iter()) {
                *a += (1.0 - *a) * *b;
            }
        }
    }
}

fn any_knockout(layers: &[Layer]) -> bool {
    layers.iter().any(|l| l.knockout.is_on() || l.children().is_some_and(|c| any_knockout(c)))
}

/// The document's Background layer as an opaque backdrop: what deep knockout
/// cuts through to. `None` when the bottom layer is not a Background layer,
/// in which case deep knockout cuts through to transparency.
fn document_backdrop(doc: &Document, ctx: &ApplyCtx) -> Option<Vec<f32>> {
    let l = doc.layers.first().filter(|l| l.background)?;
    let r = l.raster()?;
    let mut buf = vec![0f32; ctx.width * ctx.height * 4];
    r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, &mut buf);
    // A Background layer is opaque by construction; outside its own pixels
    // the backdrop reads as opaque white, which is how Photoshop pads it.
    for p in buf.chunks_exact_mut(4) {
        if p[3] <= 0.0 {
            p.copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        } else {
            p[3] = 1.0;
        }
    }
    Some(buf)
}

/// Composite a list of layers onto `buf` within one scope.
fn composite_list(layers: &[Layer], buf: &mut [f32], ctx: &ApplyCtx, doc: &Document, fresh: bool, scope: &mut Scope) {
    let mut i = 0;
    let mut fresh = fresh;
    while i < layers.len() {
        let drew = layers[i].visible;
        i = composite_run_at(layers, i, buf, ctx, doc, fresh, scope);
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

/// `fresh`: the buffer is known to be fully transparent.
fn composite_run_at(layers: &[Layer], i: usize, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document, fresh: bool, scope: &mut Scope) -> usize {
    let base = &layers[i];
    let mut end = i + 1;
    while end < layers.len() && layers[end].clip {
        end += 1;
    }
    let clipped = &layers[i + 1..end];
    let base_takes_clip = !matches!(base.kind, LayerKind::Adjustment(_)) && !base.clip;
    if clipped.is_empty() || !base_takes_clip {
        if base.visible {
            composite_layer(base, buf, ctx, doc, fresh, false, scope);
        }
        if !base_takes_clip {
            // An adjustment layer cannot be a clipping base: the clipped
            // layers composite normally.
            for l in clipped {
                if l.visible {
                    composite_layer(l, buf, ctx, doc, false, false, scope);
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
    if plain.is_group() {
        // A group now honours its own fill opacity when it composites, and
        // the line above has already spent it.
        plain.fill_opacity = 1.0;
    }
    plain.blend = BlendMode::Normal;
    plain.knockout = Knockout::None;
    let mut inner = Scope::detached();
    composite_layer(&plain, &mut group, ctx, doc, true, true, &mut inner);
    let alpha: Vec<f32> = group.chunks_exact(4).map(|p| p[3]).collect();
    for l in clipped.iter().filter(|l| l.visible) {
        composite_layer(l, &mut group, ctx, doc, false, false, &mut inner);
        for (p, a) in group.chunks_exact_mut(4).zip(alpha.iter()) {
            p[3] = *a;
        }
    }
    // The base's own mask and fill opacity are already in `group`; its
    // opacity, blend mode and knockout apply to the whole stack.
    match scope.floor(base.knockout) {
        Some(floor) => {
            knockout_blend(buf, &group, &alpha, None, base.opacity, base.blend, floor, ctx);
            scope.add_shape(&alpha);
        }
        None => {
            blend_buffer(buf, &group, None, base.opacity, base.blend, ctx);
            if scope.wants_shape() {
                scope.add_shape(&alpha);
            }
        }
    }
    end
}

impl Layer {
    /// A copy for rendering the clip base on its own. Pixel data is shared.
    fn clone_shallow_for_clip(&self) -> Layer {
        self.clone()
    }
}

/// `has_clip`: layers are clipped to this one, which makes a group isolate
/// its adjustments.
#[allow(clippy::too_many_arguments)]
fn composite_layer(layer: &Layer, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document, fresh: bool, has_clip: bool, scope: &mut Scope) {
    let n = ctx.width * ctx.height;
    if let Some(fx) = layer.effects.as_ref().filter(|fx| fx.any_active()) {
        if composite_styled(layer, fx, buf, ctx, doc) {
            return;
        }
    }
    let mask = layer.mask.as_ref().filter(|m| m.enabled).map(|m| sample_mask(m, ctx));
    let floor = scope.floor(layer.knockout);
    match &layer.kind {
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } | LayerKind::Smart { raster: r, .. }
            if fresh
                && mask.is_none()
                && layer.blend == BlendMode::Normal
                && layer.opacity * layer.fill_opacity >= 1.0
                && floor.is_none()
                && !scope.wants_shape() =>
        {
            // First layer onto a transparent buffer: the result is the layer.
            r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, buf);
        }
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } | LayerKind::Smart { raster: r, .. } => {
            let mut src = vec![0f32; n * 4];
            r.plane.resample(ctx.x0 - r.x as f64, ctx.y0 - r.y as f64, ctx.step, ctx.width, ctx.height, &mut src);
            place_source(buf, &src, mask.as_deref(), layer, floor, ctx, scope);
        }
        LayerKind::Fill(fill) => {
            let src = render_fill(fill, ctx);
            place_source(buf, &src, mask.as_deref(), layer, floor, ctx, scope);
        }
        LayerKind::Adjustment(adj)
            if mask.is_none() && layer.blend == BlendMode::Normal && layer.opacity * layer.fill_opacity >= 1.0 && !scope.isolate_adjust =>
        {
            // The common case: full strength, no mask. Adjust in place.
            adj.apply(buf, ctx);
        }
        LayerKind::Adjustment(adj) => {
            let mut adjusted = buf.to_vec();
            adj.apply(&mut adjusted, ctx);
            let op = layer.opacity * layer.fill_opacity;
            // Inside a group that isolates its adjustments, the adjustment
            // still reads the backdrop under the group but only writes where
            // the group's own layers cover.
            let confine = scope.isolate_adjust.then(|| scope.shape.as_deref()).flatten();
            for k in 0..n {
                let o = k * 4;
                let mut a = op * mask.as_ref().map_or(1.0, |m| m[k]);
                if let Some(s) = confine {
                    a *= s[k];
                }
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
            let fill = layer.fill_opacity;
            // Photoshop stops an adjustment inside a pass-through group from
            // reaching below it as soon as the group's fill opacity is not
            // 100% or something is clipped to the group.
            let isolate = scope.isolate_adjust || fill < 1.0 || has_clip;
            if *pass_through && layer.blend == BlendMode::Normal && !layer.knockout.is_on() {
                let a = layer.opacity * fill;
                let keep = a < 1.0 || mask.is_some() || isolate || scope.live || scope.wants_shape();
                let before = keep.then(|| buf.to_vec());
                let cover = {
                    let mut inner = Scope {
                        base: before.as_deref().or(scope.base),
                        deep: scope.deep,
                        live: scope.live,
                        isolate_adjust: isolate,
                        shape: (isolate || scope.wants_shape()).then(|| vec![0f32; n]),
                    };
                    composite_list(children, buf, ctx, doc, false, &mut inner);
                    inner.shape
                };
                if let Some(before) = before.as_deref() {
                    if a < 1.0 || mask.is_some() {
                        for k in 0..n {
                            let f = a * mask.as_ref().map_or(1.0, |m| m[k]);
                            let o = k * 4;
                            for c in 0..4 {
                                buf[o + c] = before[o + c] + (buf[o + c] - before[o + c]) * f;
                            }
                        }
                    }
                }
                if scope.wants_shape() {
                    if let Some(mut cover) = cover {
                        for (k, c) in cover.iter_mut().enumerate() {
                            *c *= fill * mask.as_ref().map_or(1.0, |m| m[k]);
                        }
                        scope.add_shape(&cover);
                    }
                }
            } else {
                // An isolated group — and any group with a knockout, which
                // composites as an ordinary source over the hole it cuts.
                let zero = scope.live.then(|| vec![0f32; n * 4]);
                let mut group = vec![0f32; n * 4];
                {
                    let mut inner = Scope {
                        base: zero.as_deref(),
                        // A pass-through group is not an isolation boundary,
                        // so deep knockout inside it still reaches the
                        // document backdrop.
                        deep: if *pass_through { scope.deep } else { None },
                        live: scope.live,
                        isolate_adjust: isolate,
                        shape: isolate.then(|| vec![0f32; n]),
                    };
                    composite_list(children, &mut group, ctx, doc, true, &mut inner);
                }
                place_source(buf, &group, mask.as_deref(), layer, floor, ctx, scope);
            }
        }
    }
}

/// Composite one resolved source with the layer's mask, opacity, fill
/// opacity, blend mode and knockout, and record its coverage in the scope.
fn place_source(buf: &mut [f32], src: &[f32], mask: Option<&[f32]>, layer: &Layer, floor: Option<&[f32]>, ctx: &ApplyCtx, scope: &mut Scope) {
    let op = layer.opacity * layer.fill_opacity;
    match floor {
        Some(k) => {
            // The hole is cut by the source's alpha and mask alone: fill
            // opacity decides only how much paints back over it.
            let cover = coverage(src, mask, 1.0);
            knockout_blend(buf, src, &cover, mask, op, layer.blend, k, ctx);
            scope.add_shape(&cover);
        }
        None => {
            blend_buffer(buf, src, mask, op, layer.blend, ctx);
            if scope.wants_shape() {
                let cover = coverage(src, mask, layer.fill_opacity);
                scope.add_shape(&cover);
            }
        }
    }
}

/// A source's coverage: its own alpha, times the mask, times `scale`.
fn coverage(src: &[f32], mask: Option<&[f32]>, scale: f32) -> Vec<f32> {
    src.chunks_exact(4).enumerate().map(|(i, p)| p[3] * scale * mask.map_or(1.0, |m| m[i])).collect()
}

/// Photoshop's knockout. `cover` is the hole the source cuts; inside it the
/// result is the source composited over `floor` rather than over `buf`, and
/// the two are mixed by `cover` in premultiplied space.
#[allow(clippy::too_many_arguments)]
fn knockout_blend(
    buf: &mut [f32],
    src: &[f32],
    cover: &[f32],
    mask: Option<&[f32]>,
    opacity: f32,
    mode: BlendMode,
    floor: &[f32],
    ctx: &ApplyCtx,
) {
    let mut through = floor.to_vec();
    blend_buffer(&mut through, src, mask, opacity, mode, ctx);
    for k in 0..(ctx.width * ctx.height) {
        let s = cover[k];
        if s <= 0.0 {
            continue;
        }
        let o = k * 4;
        let (ad, at) = (buf[o + 3], through[o + 3]);
        let ar = ad + (at - ad) * s;
        if ar <= 0.0 {
            buf[o..o + 4].copy_from_slice(&[0.0; 4]);
            continue;
        }
        for c in 0..3 {
            let pd = buf[o + c] * ad;
            let pt = through[o + c] * at;
            buf[o + c] = ((pd + (pt - pd) * s) / ar).clamp(0.0, 1.0);
        }
        buf[o + 3] = ar;
    }
}

/// A layer with a style: render its content with an apron wide enough for
/// the effects, composite effects and content over a padded copy of the
/// backdrop, and copy the visible part back. Returns false for layer kinds
/// that take no style.
fn composite_styled(layer: &Layer, fx: &crate::effects::LayerEffects, buf: &mut [f32], ctx: &ApplyCtx, doc: &Document) -> bool {
    use crate::effects::{composite_with_effects, EffectCtx};
    let apron = match &layer.kind {
        LayerKind::Pixel(_) | LayerKind::Text { .. } | LayerKind::Shape { .. } | LayerKind::Smart { .. } => ((fx.reach() as f64 / ctx.step).ceil() as usize + 2).min(2048),
        LayerKind::Group { .. } | LayerKind::Fill(_) => ((fx.reach() as f64 / ctx.step).ceil() as usize + 2).min(2048),
        LayerKind::Adjustment(_) => return false,
    };
    let (w, h) = (ctx.width + 2 * apron, ctx.height + 2 * apron);
    let ext = ApplyCtx { x0: ctx.x0 - apron as f64 * ctx.step, y0: ctx.y0 - apron as f64 * ctx.step, width: w, height: h, ..*ctx };
    let content: Vec<f32> = match &layer.kind {
        LayerKind::Pixel(r) | LayerKind::Text { raster: r, .. } | LayerKind::Shape { raster: r, .. } | LayerKind::Smart { raster: r, .. } => {
            let mut c = vec![0f32; w * h * 4];
            r.plane.resample(ext.x0 - r.x as f64, ext.y0 - r.y as f64, ext.step, w, h, &mut c);
            c
        }
        LayerKind::Fill(f) => render_fill(f, &ext),
        LayerKind::Group { children, .. } => {
            let mut g = vec![0f32; w * h * 4];
            render_list_fresh(children, &mut g, &ext, doc);
            g
        }
        LayerKind::Adjustment(_) => unreachable!(),
    };
    let mut strength = vec![layer.opacity; w * h];
    if let Some(m) = layer.mask.as_ref().filter(|m| m.enabled) {
        let mv = sample_mask(m, &ext);
        for (s, v) in strength.iter_mut().zip(mv.iter()) {
            *s *= v;
        }
    }
    let mut back = vec![0f32; w * h * 4];
    for y in 0..ctx.height {
        let src = y * ctx.width * 4;
        let dst = ((y + apron) * w + apron) * 4;
        back[dst..dst + ctx.width * 4].copy_from_slice(&buf[src..src + ctx.width * 4]);
    }
    let bounds = layer.content_bounds().unwrap_or(Rect::new(0, 0, doc.width as i32, doc.height as i32));
    let ectx = EffectCtx {
        scale: (1.0 / ctx.step) as f32,
        x0: ext.x0,
        y0: ext.y0,
        step: ext.step,
        bounds: (bounds.x as f64, bounds.y as f64, bounds.w as f64, bounds.h as f64),
    };
    composite_with_effects(fx, &mut back, &content, w, h, &strength, layer.fill_opacity, layer.blend, &ectx);
    for y in 0..ctx.height {
        let dst = y * ctx.width * 4;
        let src = ((y + apron) * w + apron) * 4;
        buf[dst..dst + ctx.width * 4].copy_from_slice(&back[src..src + ctx.width * 4]);
    }
    true
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
    l.knockout = Knockout::None;
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

    fn group(doc: &mut Document, name: &str, children: Vec<Layer>, pass_through: bool) -> Layer {
        let id = doc.alloc_id();
        Layer::new(id, name, LayerKind::Group { children, pass_through, expanded: true })
    }

    #[test]
    fn group_fill_opacity_scales_the_group() {
        // A group at 50% fill over red shows half of each.
        let mut doc = Document::new(2, 2);
        let red = solid_layer(&mut doc, [255, 0, 0, 255]);
        doc.layers.push(red);
        let blue = solid_layer(&mut doc, [0, 0, 255, 255]);
        let mut g = group(&mut doc, "g", vec![blue], false);
        g.fill_opacity = 0.5;
        doc.layers.push(g);
        assert_eq!(px_at(&doc, 0, 0), [128, 0, 128, 255]);
    }

    #[test]
    fn deep_knockout_shows_the_background_layer() {
        let mut doc = Document::new(2, 2);
        let mut bg = solid_layer(&mut doc, [255, 255, 255, 255]);
        bg.background = true;
        doc.layers.push(bg);
        let red = solid_layer(&mut doc, [255, 0, 0, 255]);
        doc.layers.push(red);
        let blue = solid_layer(&mut doc, [0, 0, 255, 255]);
        let mut g = group(&mut doc, "knock", vec![blue], false);
        g.fill_opacity = 0.5;
        g.knockout = Knockout::Deep;
        doc.layers.push(g);
        // The red is punched out; half blue over the white Background.
        assert_eq!(px_at(&doc, 0, 0), [128, 128, 255, 255]);
    }

    #[test]
    fn deep_knockout_without_a_background_cuts_to_transparency() {
        let mut doc = Document::new(2, 2);
        let cyan = solid_layer(&mut doc, [0, 255, 255, 255]);
        doc.layers.push(cyan);
        let red = solid_layer(&mut doc, [255, 0, 0, 255]);
        doc.layers.push(red);
        let blue = solid_layer(&mut doc, [0, 0, 255, 255]);
        let mut g = group(&mut doc, "knock", vec![blue], false);
        g.fill_opacity = 0.5;
        g.knockout = Knockout::Deep;
        doc.layers.push(g);
        assert_eq!(px_at(&doc, 0, 0), [0, 0, 255, 128]);
    }

    #[test]
    fn shallow_knockout_stops_at_the_enclosing_group() {
        // Shallow knockout punches the green away and shows what is under
        // the group it sits in — the red, never the white Background that
        // deep knockout would reach.
        for pass_through in [true, false] {
            let mut doc = Document::new(2, 2);
            let mut bg = solid_layer(&mut doc, [255, 255, 255, 255]);
            bg.background = true;
            doc.layers.push(bg);
            let red = solid_layer(&mut doc, [255, 0, 0, 255]);
            doc.layers.push(red);
            let green = solid_layer(&mut doc, [0, 255, 0, 255]);
            let blue = solid_layer(&mut doc, [0, 0, 255, 255]);
            let mut inner = group(&mut doc, "inner", vec![blue], false);
            inner.fill_opacity = 0.5;
            inner.knockout = Knockout::Shallow;
            let outer = group(&mut doc, "outer", vec![green, inner], pass_through);
            doc.layers.push(outer);
            assert_eq!(px_at(&doc, 0, 0), [128, 0, 128, 255], "pass_through={pass_through}");
        }
    }

    #[test]
    fn deep_knockout_stops_at_an_isolated_group() {
        // The same tree twice: deep knockout escapes a pass-through parent
        // and reaches the white Background, but an isolated parent is an
        // isolation boundary and the hole only reaches transparency there.
        for (pass_through, want) in [(true, [128, 128, 255, 255]), (false, [128, 0, 128, 255])] {
            let mut doc = Document::new(2, 2);
            let mut bg = solid_layer(&mut doc, [255, 255, 255, 255]);
            bg.background = true;
            doc.layers.push(bg);
            let red = solid_layer(&mut doc, [255, 0, 0, 255]);
            doc.layers.push(red);
            let green = solid_layer(&mut doc, [0, 255, 0, 255]);
            let blue = solid_layer(&mut doc, [0, 0, 255, 255]);
            let mut inner = group(&mut doc, "inner", vec![blue], false);
            inner.fill_opacity = 0.5;
            inner.knockout = Knockout::Deep;
            let outer = group(&mut doc, "outer", vec![green, inner], pass_through);
            doc.layers.push(outer);
            assert_eq!(px_at(&doc, 0, 0), want, "pass_through={pass_through}");
        }
    }

    #[test]
    fn fill_opacity_confines_a_pass_through_adjustment() {
        // A pass-through group normally lets an adjustment reach below it;
        // once its fill opacity drops, the adjustment sees the backdrop but
        // only writes where the group's own layers cover.
        let mut doc = Document::new(2, 1);
        let bg = solid_layer(&mut doc, [255, 255, 255, 255]);
        doc.layers.push(bg);
        let id = doc.alloc_id();
        let mut half = Plane::transparent(2, 1);
        half.write(Rect::new(0, 0, 1, 1), &[255, 0, 0, 255]);
        let red = Layer::new(id, "half", LayerKind::Pixel(Raster::new(half, 0, 0)));
        let aid = doc.alloc_id();
        let inv = Layer::new(aid, "inv", LayerKind::Adjustment(Adjustment::Invert));
        let mut g = group(&mut doc, "g", vec![red, inv], true);
        g.fill_opacity = 0.5;
        doc.layers.push(g);
        // Covered: invert(255,0,0) = (0,255,255), mixed half with white.
        assert_eq!(px_at(&doc, 0, 0), [128, 255, 255, 255]);
        // Uncovered: the adjustment does not reach the white below.
        assert_eq!(px_at(&doc, 1, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn pass_through_adjustment_still_reaches_below_at_full_fill() {
        let mut doc = Document::new(2, 1);
        let bg = solid_layer(&mut doc, [255, 255, 255, 255]);
        doc.layers.push(bg);
        let aid = doc.alloc_id();
        let inv = Layer::new(aid, "inv", LayerKind::Adjustment(Adjustment::Invert));
        let g = group(&mut doc, "g", vec![inv], true);
        doc.layers.push(g);
        assert_eq!(px_at(&doc, 1, 0), [0, 0, 0, 255]);
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
