//! `Document` → PSD/PSB.

use editor_core::adjust::Adjustment;
use editor_core::document::Document;
use editor_core::geom::Rect;
use editor_core::layer::{Layer, LayerKind, LayerMask, Raster, SmartSource};
use editor_core::render::{flatten, render_layer_alone, to_u8, View};
use serde::Deserialize;

use crate::compress;
use crate::effects;
use crate::error::{PsdError, Result};
use crate::import::{effects_sig, kind_sig, plane_sig};
use crate::io::Writer;
use crate::kinds;
use crate::linked;
use crate::sidecar::{Blocks, LayerExtra, Sidecar};
use crate::structure::is_wide_key;
use crate::text;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    /// Force PSB (`Some(true)`) or PSD (`Some(false)`). PSB is always used
    /// when a side exceeds 30,000 pixels.
    pub psb: Option<bool>,
    /// Write the merged composite (Photoshop's "Maximize Compatibility").
    /// Without it the composite is left blank, which most readers other
    /// than Photoshop show as white.
    pub max_compat: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { psb: None, max_compat: true }
    }
}

pub struct Exported {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

struct Out<'d> {
    doc: &'d Document,
    psb: bool,
    sidecar: Sidecar,
    warnings: Vec<String>,
    text_index: i32,
    /// New `lnk2` entries for smart objects created in the editor.
    linked: Vec<u8>,
    nesting: u32,
}

impl Out<'_> {
    fn warn(&mut self, m: impl Into<String>) {
        let m = m.into();
        if !self.warnings.contains(&m) {
            self.warnings.push(m);
        }
    }
}

/// Mask record fields: rectangle, default colour, flags, density.
type MaskFields = (Rect, u8, u8, Option<u8>);

struct Record {
    rect: Rect,
    /// (channel id, compression + data)
    channels: Vec<(i16, Vec<u8>)>,
    blend: [u8; 4],
    opacity: u8,
    clipping: u8,
    flags: u8,
    mask: Option<MaskFields>,
    blending_ranges: Vec<u8>,
    name: String,
    blocks: Blocks,
}

pub fn export(doc: &Document, opts: &ExportOptions) -> Result<Exported> {
    export_nested(doc, opts, 0)
}

fn export_nested(doc: &Document, opts: &ExportOptions, nesting: u32) -> Result<Exported> {
    if doc.width == 0 || doc.height == 0 {
        return Err(PsdError::Write("the document is empty".into()));
    }
    if doc.width > 300_000 || doc.height > 300_000 {
        return Err(PsdError::Write("documents larger than 300,000 pixels on a side cannot be saved as PSB".into()));
    }
    let psb = opts.psb.unwrap_or(false) || doc.width > 30_000 || doc.height > 30_000;
    let mut out = Out { doc, psb, sidecar: Sidecar::from_doc(doc), warnings: Vec::new(), text_index: 0, linked: Vec::new(), nesting };

    let mut records = Vec::new();
    for l in &doc.layers {
        layer_records(&mut out, l, &mut records)?;
    }

    let composite = if opts.max_compat { flatten(doc) } else { vec![255u8; doc.width as usize * doc.height as usize * 4] };
    let transparent = composite.as_chunks::<4>().0.iter().any(|p| p[3] < 255);

    let mut w = Writer::new();
    // Header.
    w.bytes(b"8BPS");
    w.u16(if psb { 2 } else { 1 });
    w.zeros(6);
    w.u16(if transparent { 4 } else { 3 });
    w.u32(doc.height);
    w.u32(doc.width);
    w.u16(8);
    w.u16(3);
    // Colour mode data.
    w.u32(0);

    // Image resources.
    let at = w.begin_len(false);
    let resource = |w: &mut Writer, id: u16, name: &str, data: &[u8]| {
        w.bytes(b"8BIM");
        w.u16(id);
        w.pascal(name, 2);
        w.u32(data.len() as u32);
        w.bytes(data);
        w.pad_to(2);
    };
    {
        let mut r = Writer::new();
        let fixed = (doc.resolution.clamp(1.0, 30000.0) as f64 * 65536.0).round() as u32;
        r.u32(fixed);
        r.u16(1);
        r.u16(2);
        r.u32(fixed);
        r.u16(1);
        r.u16(2);
        resource(&mut w, 1005, "", &r.buf);
    }
    if let Some(icc) = &doc.meta.icc {
        resource(&mut w, 1039, "", icc);
    }
    if !doc.guides.is_empty() {
        let mut g = Writer::new();
        g.u32(1);
        g.u32(576);
        g.u32(576);
        g.u32(doc.guides.len() as u32);
        for guide in &doc.guides {
            g.i32((guide.position * 32.0).round() as i32);
            g.u8(if guide.vertical { 0 } else { 1 });
        }
        resource(&mut w, 1032, "", &g.buf);
    }
    for (id, name, data) in &out.sidecar.resources {
        if matches!(id, 1005 | 1039 | 1032) {
            continue;
        }
        resource(&mut w, *id, name, data);
    }
    w.end_len(at, false, 1, false);

    // Layer and mask information.
    let lm = w.begin_len(psb);
    let li = w.begin_len(psb);
    if !records.is_empty() {
        let n = records.len() as i16;
        w.i16(if transparent { -n } else { n });
        for rec in &records {
            write_record(&mut w, rec, psb);
        }
        for rec in &records {
            for (_, data) in &rec.channels {
                w.bytes(data);
            }
        }
    }
    w.end_len(li, psb, if records.is_empty() { 1 } else { 4 }, true);
    w.u32(0); // global layer mask info
    let mut globals = std::mem::take(&mut out.sidecar.global_blocks);
    if !out.linked.is_empty() {
        match globals.iter_mut().find(|(k, _)| k == b"lnk2") {
            Some((_, d)) => d.extend_from_slice(&out.linked),
            None => globals.push((*b"lnk2", std::mem::take(&mut out.linked))),
        }
    }
    for (key, data) in &globals {
        write_block(&mut w, key, data, psb, true);
    }
    w.end_len(lm, psb, 2, true);

    // Merged image, RLE, colours matted against white where transparent.
    let (dw, dh) = (doc.width as usize, doc.height as usize);
    let nch = if transparent { 4 } else { 3 };
    let pixels = dw * dh;
    let mut planes = vec![0u8; pixels * nch];
    for (p, px) in composite.as_chunks::<4>().0.iter().enumerate() {
        let a = px[3] as u32;
        for c in 0..3 {
            planes[c * pixels + p] = if a == 255 { px[c] } else { ((px[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8 };
        }
        if transparent {
            planes[3 * pixels + p] = px[3];
        }
    }
    w.u16(compress::RLE);
    let rle = compress::encode_rle(&planes, dw, dh * nch, psb);
    w.bytes(&rle);

    Ok(Exported { bytes: w.buf, warnings: out.warnings })
}

fn write_block(w: &mut Writer, key: &[u8; 4], data: &[u8], psb: bool, global: bool) {
    let wide = psb && is_wide_key(key);
    w.bytes(if wide { b"8B64" } else { b"8BIM" });
    w.bytes(key);
    let at = w.begin_len(wide);
    w.bytes(data);
    let pad = if global { 4 } else { 2 };
    w.end_len(at, wide, pad, true);
}

fn write_record(w: &mut Writer, r: &Record, psb: bool) {
    w.i32(r.rect.y);
    w.i32(r.rect.x);
    w.i32(r.rect.bottom());
    w.i32(r.rect.right());
    w.u16(r.channels.len() as u16);
    for (id, data) in &r.channels {
        w.i16(*id);
        w.len_v(psb, data.len() as u64);
    }
    w.bytes(b"8BIM");
    w.bytes(&r.blend);
    w.u8(r.opacity);
    w.u8(r.clipping);
    w.u8(r.flags);
    w.u8(0);
    let extra = w.begin_len(false);
    // Mask data.
    match &r.mask {
        None => w.u32(0),
        Some((rect, default, flags, density)) => {
            let at = w.begin_len(false);
            w.i32(rect.y);
            w.i32(rect.x);
            w.i32(rect.bottom());
            w.i32(rect.right());
            w.u8(*default);
            w.u8(*flags | if density.is_some() { 16 } else { 0 });
            if let Some(d) = density {
                w.u8(1);
                w.u8(*d);
            }
            w.end_len(at, false, 4, true);
            // A bare 18-byte block is written as 20, as Photoshop does.
        }
    }
    w.u32(r.blending_ranges.len() as u32);
    w.bytes(&r.blending_ranges);
    w.pascal(&r.name, 4);
    for (key, data) in &r.blocks {
        write_block(w, key, data, psb, false);
    }
    w.end_len(extra, false, 2, true);
}

/// A channel: RLE unless empty.
fn channel_bytes(data: &[u8], width: usize, height: usize, psb: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(2 + data.len() / 2);
    if width == 0 || height == 0 {
        v.extend_from_slice(&compress::RAW.to_be_bytes());
        return v;
    }
    v.extend_from_slice(&compress::RLE.to_be_bytes());
    v.extend(compress::encode_rle(data, width, height, psb));
    v
}

/// RGBA pixels over `rect` as channels -1, 0, 1, 2.
fn rgba_channels(rgba: &[u8], rect: Rect, psb: bool) -> Vec<(i16, Vec<u8>)> {
    let (w, h) = (rect.w.max(0) as usize, rect.h.max(0) as usize);
    let n = w * h;
    let mut out = Vec::with_capacity(4);
    for (id, c) in [(-1i16, 3usize), (0, 0), (1, 1), (2, 2)] {
        let ch: Vec<u8> = if n == 0 { Vec::new() } else { (0..n).map(|p| rgba[p * 4 + c]).collect() };
        out.push((id, channel_bytes(&ch, w, h, psb)));
    }
    out
}

fn empty_channels(psb: bool) -> Vec<(i16, Vec<u8>)> {
    [-1i16, 0, 1, 2].iter().map(|&id| (id, channel_bytes(&[], 0, 0, psb))).collect()
}

fn raster_channels(r: &Raster, psb: bool) -> (Rect, Vec<(i16, Vec<u8>)>) {
    let b = r.plane.content_bounds();
    if b.is_empty() {
        return (Rect::new(0, 0, 0, 0), empty_channels(psb));
    }
    let data = r.plane.read_vec(b);
    let rect = b.translate(r.x, r.y);
    (rect, rgba_channels(&data, rect, psb))
}

fn default_blending_ranges() -> Vec<u8> {
    [0u8, 0, 255, 255].repeat(10)
}

fn common_blocks(l: &Layer) -> Blocks {
    let mut b: Blocks = Vec::new();
    let mut w = Writer::new();
    w.unicode(&l.name, false);
    w.pad_to(4);
    b.push((*b"luni", w.buf));
    b.push((*b"lyid", l.id.to_be_bytes().to_vec()));
    if l.fill_opacity < 1.0 {
        b.push((*b"iOpa", vec![(l.fill_opacity.clamp(0.0, 1.0) * 255.0).round() as u8, 0, 0, 0]));
    }
    let locks = &l.locks;
    let bits = (locks.transparency as u32) | (locks.pixels as u32) << 1 | (locks.position as u32) << 2 | if locks.all { 0x8000_0000 } else { 0 };
    if bits != 0 {
        b.push((*b"lspf", bits.to_be_bytes().to_vec()));
    }
    if l.color_label != 0 {
        let mut c = (l.color_label as u16).to_be_bytes().to_vec();
        c.extend([0; 6]);
        b.push((*b"lclr", c));
    }
    b
}

fn mask_record(m: &LayerMask, psb: bool, as_render: bool) -> (MaskFields, (i16, Vec<u8>)) {
    let fill = m.raster.plane.fill()[0];
    let b = m.raster.plane.content_bounds();
    let (rect, data) = if b.is_empty() { (Rect::new(0, 0, 0, 0), Vec::new()) } else { (b.translate(m.raster.x, m.raster.y), m.raster.plane.read_vec(b)) };
    let flags = if m.enabled { 0 } else { 2 } | if as_render { 8 } else { 0 };
    let density = (m.density < 1.0).then(|| (m.density.clamp(0.0, 1.0) * 255.0).round() as u8);
    ((rect, fill, flags, density), (-2, channel_bytes(&data, rect.w as usize, rect.h as usize, psb)))
}

fn layer_records(out: &mut Out, l: &Layer, records: &mut Vec<Record>) -> Result<()> {
    let psb = out.psb;
    let extra: LayerExtra = out.sidecar.layers.get(&l.id).cloned().unwrap_or_default();
    let mut blocks = common_blocks(l);
    let mut rec = Record {
        rect: Rect::new(0, 0, 0, 0),
        channels: Vec::new(),
        blend: l.blend.psd_key(),
        opacity: (l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        clipping: l.clip as u8,
        flags: 8 | (l.locks.transparency as u8) | if l.visible { 0 } else { 2 },
        mask: None,
        blending_ranges: if extra.blending_ranges.is_empty() { default_blending_ranges() } else { extra.blending_ranges.clone() },
        name: l.name.clone(),
        blocks: Vec::new(),
    };

    let kind_unchanged = !extra.kind_blocks.is_empty() && extra.kind_sig == kind_sig(&l.kind);
    let vector_kept = !extra.vector_blocks.is_empty() && l.mask.as_ref().is_some_and(|m| plane_sig(&m.raster) == extra.vector_sig && extra.vector_sig != 0);
    // A shape whose coverage fits its transparency channel (nothing shows
    // outside the shape) is written the way Photoshop writes shape layers.
    let shape_channels = vector_kept && matches!(l.kind, LayerKind::Fill(_)) && l.mask.as_ref().is_some_and(|m| m.raster.plane.fill()[0] == 0);

    if let LayerKind::Group { children, pass_through, expanded } = &l.kind {
        // End marker first (lowest), then the children, then the group.
        let mut marker = Record {
            rect: Rect::new(0, 0, 0, 0),
            channels: empty_channels(psb),
            blend: *b"norm",
            opacity: 255,
            clipping: 0,
            flags: 8 | 16,
            mask: None,
            blending_ranges: default_blending_ranges(),
            name: "</Layer group>".into(),
            blocks: Vec::new(),
        };
        let mut m = Writer::new();
        m.unicode("</Layer group>", false);
        m.pad_to(4);
        marker.blocks.push((*b"luni", m.buf));
        marker.blocks.push((*b"lsct", 3u32.to_be_bytes().to_vec()));
        records.push(marker);
        for c in children {
            layer_records(out, c, records)?;
        }
        let key = if *pass_through { *b"pass" } else { l.blend.psd_key() };
        rec.blend = key;
        rec.flags |= 16;
        rec.channels = empty_channels(psb);
        let mut s = Writer::new();
        s.u32(if *expanded { 1 } else { 2 });
        s.bytes(b"8BIM");
        s.bytes(&key);
        blocks.push((*b"lsct", s.buf));
    } else {
        match &l.kind {
            LayerKind::Pixel(r) => {
                let (rect, ch) = raster_channels(r, psb);
                rec.rect = rect;
                rec.channels = ch;
                if kind_unchanged {
                    blocks.extend(extra.kind_blocks.iter().cloned());
                }
            }
            LayerKind::Shape { raster, .. } => {
                let (rect, ch) = raster_channels(raster, psb);
                rec.rect = rect;
                rec.channels = ch;
                out.warn("Shape layers are saved as pixel layers.");
            }
            LayerKind::Text { data, raster } => {
                let (rect, ch) = raster_channels(raster, psb);
                rec.rect = rect;
                rec.channels = ch;
                if kind_unchanged {
                    blocks.extend(extra.kind_blocks.iter().cloned());
                } else {
                    blocks.push((*b"TySh", text::write(data, out.text_index, raster.plane.height() as f32)));
                }
                out.text_index += 1;
            }
            LayerKind::Smart { source, quad, raster, filters, .. } => {
                let (rect, ch) = raster_channels(raster, psb);
                rec.rect = rect;
                rec.channels = ch;
                if kind_unchanged {
                    blocks.extend(extra.kind_blocks.iter().cloned());
                } else if out.nesting < 2 {
                    let inner = match source {
                        SmartSource::Document(d) => export_nested(d, &ExportOptions::default(), out.nesting + 1),
                        SmartSource::Pixels(p) => {
                            let mut d = Document::new(p.width().max(1), p.height().max(1));
                            let id = d.alloc_id();
                            d.layers.push(Layer::new(id, "Layer 1", LayerKind::Pixel(Raster::new(p.clone(), 0, 0))));
                            export_nested(&d, &ExportOptions::default(), out.nesting + 1)
                        }
                    };
                    match inner {
                        Ok(e) => {
                            let (sw, sh) = source.size();
                            let uuid = linked::uuid_from(crate::sidecar::fnv(&e.bytes) ^ l.id as u64);
                            out.linked.extend(linked::write_entry(&uuid, &format!("{}.psd", l.name), b"8BPS", &e.bytes));
                            blocks.push((*b"SoLd", linked::write_sold(&uuid, quad, sw, sh)));
                            if !filters.is_empty() {
                                out.warn("Smart filters are saved applied to the smart object's pixels; Photoshop will not see them as filters.");
                            }
                        }
                        Err(_) => out.warn("A smart object's contents could not be embedded; it is saved as pixels."),
                    }
                }
            }
            LayerKind::Fill(fill) => {
                rec.flags |= 16;
                if kind_unchanged {
                    blocks.extend(extra.kind_blocks.iter().cloned());
                } else {
                    let bounds = match l.mask.as_ref().filter(|_| shape_channels) {
                        Some(m) => {
                            let b = m.raster.doc_rect();
                            (b.x as f64, b.y as f64, b.w as f64, b.h as f64)
                        }
                        None => (0.0, 0.0, out.doc.width as f64, out.doc.height as f64),
                    };
                    blocks.extend(kinds::write_fill(fill, bounds));
                }
                if shape_channels {
                    // Photoshop re-renders the shape from its path; other
                    // readers get the rendered pixels.
                    let m = l.mask.as_ref().unwrap();
                    // The mask's own rectangle: gradients align to it.
                    let b = m.raster.doc_rect();
                    let mut shape = l.clone();
                    shape.effects = None;
                    shape.mask = None;
                    let view = View { x: b.x as f64, y: b.y as f64, scale: 1.0, width: b.w.max(0) as usize, height: b.h.max(0) as usize };
                    let f = render_layer_alone(&shape, out.doc, view);
                    let mut px = vec![0u8; f.len()];
                    to_u8(&f, &mut px);
                    let alpha = m.raster.plane.read_vec(b.translate(-m.raster.x, -m.raster.y));
                    for (p, a) in px.as_chunks_mut::<4>().0.iter_mut().zip(alpha) {
                        p[3] = a;
                    }
                    rec.rect = b;
                    rec.channels = rgba_channels(&px, b, psb);
                    rec.flags &= !16;
                } else {
                    rec.channels = empty_channels(psb);
                }
            }
            LayerKind::Adjustment(Adjustment::Grain(g)) => {
                // Photoshop has no grain adjustment layer. Write the grain as
                // a canvas-sized Linear Light pixel layer (mid-grey ± half the
                // delta, so Linear Light adds exactly the delta Grain gives a
                // midtone), and keep the parameters in a private block so
                // OpenPhotoEdit reads it back as the live adjustment.
                let (w, h) = (out.doc.width as usize, out.doc.height as usize);
                let rgba = kinds::grain_layer_pixels(g, w, h);
                let rect = Rect::new(0, 0, w as i32, h as i32);
                rec.rect = rect;
                rec.channels = rgba_channels(&rgba, rect, psb);
                rec.blend = *b"lLit";
                blocks.push((kinds::GRAIN_KEY, kinds::write_grain(g, l.blend.psd_key())));
                out.warn("Grain is saved as a Linear Light noise layer for Photoshop; it stays an editable Grain adjustment in OpenPhotoEdit.");
            }
            LayerKind::Adjustment(adj) => {
                rec.flags |= 16;
                rec.channels = empty_channels(psb);
                if kind_unchanged {
                    blocks.extend(extra.kind_blocks.iter().cloned());
                } else {
                    match kinds::write_adjustment(adj) {
                        Some(b) => blocks.extend(b),
                        None => {
                            let lut = kinds::bake_lut(adj, &l.name, 33);
                            blocks.push((*b"clrL", kinds::color_lookup_block(&lut.name, &kinds::cube_text(&lut))));
                            if let Adjustment::Develop(_) = adj {
                                out.warn("Develop adjustments are saved as a Color Lookup layer; clarity, dehaze, vignette and grain are left out.");
                            }
                        }
                    }
                }
            }
            LayerKind::Group { .. } => unreachable!(),
        }
    }

    // Mask.
    if let Some(m) = &l.mask {
        if vector_kept {
            blocks.extend(extra.vector_blocks.iter().cloned());
            if !shape_channels {
                let (info, ch) = mask_record(m, psb, true);
                rec.mask = Some(info);
                rec.channels.push(ch);
            }
        } else {
            let (info, ch) = mask_record(m, psb, false);
            rec.mask = Some(info);
            rec.channels.push(ch);
        }
    }

    // Layer style.
    if !extra.effects_blocks.is_empty() && extra.effects_sig == effects_sig(&l.effects) {
        blocks.extend(extra.effects_blocks.iter().cloned());
    } else if let Some(fx) = &l.effects {
        blocks.push((*b"lfx2", effects::write(fx)));
    }

    blocks.extend(extra.blocks.iter().cloned());
    rec.blocks = blocks;
    records.push(rec);
    Ok(())
}

