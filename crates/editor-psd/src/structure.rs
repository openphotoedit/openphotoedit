//! The file's sections as they are on disk: header, colour-mode data, image
//! resources, layer records with their tagged blocks and channel bytes,
//! global tagged blocks and the merged image. Nothing is decompressed here.

use crate::error::{corrupt, PsdError, Result};
use crate::io::Reader;

pub const MODE_BITMAP: u16 = 0;
pub const MODE_GRAYSCALE: u16 = 1;
pub const MODE_INDEXED: u16 = 2;
pub const MODE_RGB: u16 = 3;
pub const MODE_CMYK: u16 = 4;
pub const MODE_MULTICHANNEL: u16 = 7;
pub const MODE_DUOTONE: u16 = 8;
pub const MODE_LAB: u16 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub channels: u16,
    pub height: u32,
    pub width: u32,
    pub depth: u16,
    pub mode: u16,
}

impl Header {
    pub fn psb(&self) -> bool {
        self.version == 2
    }
}

#[derive(Clone, Debug)]
pub struct Resource<'a> {
    pub sig: [u8; 4],
    pub id: u16,
    pub name: String,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct Block<'a> {
    pub key: [u8; 4],
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct Channel<'a> {
    pub id: i16,
    /// Compression method followed by the data, as stored.
    pub raw: &'a [u8],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect32 {
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
}

impl Rect32 {
    pub fn width(&self) -> i64 {
        (self.right as i64 - self.left as i64).max(0)
    }
    pub fn height(&self) -> i64 {
        (self.bottom as i64 - self.top as i64).max(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskInfo {
    pub rect: Rect32,
    pub default_color: u8,
    pub flags: u8,
    pub real: Option<(u8, u8, Rect32)>,
    pub user_density: Option<u8>,
    pub user_feather: Option<f64>,
    pub vector_density: Option<u8>,
    pub vector_feather: Option<f64>,
}

impl MaskInfo {
    pub const DISABLED: u8 = 2;
    pub const FROM_RENDER: u8 = 8;
    pub const HAS_PARAMS: u8 = 16;
}

#[derive(Clone, Debug)]
pub struct LayerRecord<'a> {
    pub rect: Rect32,
    pub channels: Vec<Channel<'a>>,
    pub blend: [u8; 4],
    pub opacity: u8,
    pub clipping: u8,
    pub flags: u8,
    pub mask: Option<MaskInfo>,
    pub blending_ranges: &'a [u8],
    pub name: String,
    pub blocks: Vec<Block<'a>>,
}

impl<'a> LayerRecord<'a> {
    pub fn block(&self, key: &[u8; 4]) -> Option<&'a [u8]> {
        self.blocks.iter().find(|b| &b.key == key).map(|b| b.data)
    }
    pub fn channel(&self, id: i16) -> Option<&Channel<'a>> {
        self.channels.iter().find(|c| c.id == id)
    }
    pub fn hidden(&self) -> bool {
        self.flags & 2 != 0
    }
}

#[derive(Clone, Debug)]
pub struct PsdFile<'a> {
    pub header: Header,
    pub color_mode_data: &'a [u8],
    pub resources: Vec<Resource<'a>>,
    pub layers: Vec<LayerRecord<'a>>,
    /// A negative layer count: the merged image's first alpha channel is
    /// the composite's transparency.
    pub merged_alpha: bool,
    pub global_mask: &'a [u8],
    pub global_blocks: Vec<Block<'a>>,
    /// Compression method followed by the merged image data.
    pub merged: &'a [u8],
}

impl<'a> PsdFile<'a> {
    pub fn resource(&self, id: u16) -> Option<&'a [u8]> {
        self.resources.iter().find(|r| r.id == id && &r.sig == b"8BIM").map(|r| r.data)
    }
    pub fn global_block(&self, key: &[u8; 4]) -> Option<&'a [u8]> {
        self.global_blocks.iter().find(|b| &b.key == key).map(|b| b.data)
    }
}

/// Keys whose length field is 8 bytes in PSB files (the documented list
/// plus the ones psd-tools and ag-psd found in the wild).
pub fn is_wide_key(key: &[u8; 4]) -> bool {
    const WIDE: [&[u8; 4]; 21] = [
        b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2", b"lnk3", b"lnkE", b"FEid", b"FXid",
        b"FELS", b"PxSD", b"pths", b"extd", b"extn", b"cinf", b"artd",
    ];
    WIDE.contains(&key)
}

fn is_block_sig(b: &[u8]) -> bool {
    b == b"8BIM" || b == b"8B64"
}

/// Parse a run of tagged blocks until the reader ends or the data stops
/// looking like blocks.
pub fn parse_blocks<'a>(r: &mut Reader<'a>, psb: bool) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    loop {
        // Tolerate up to three bytes of padding between blocks.
        let mut skipped = 0;
        while skipped < 4 && r.remaining() >= 12 && !r.peek(4).is_some_and(is_block_sig) {
            if r.skip(1, "padding").is_err() {
                return out;
            }
            skipped += 1;
        }
        if r.remaining() < 12 || !r.peek(4).is_some_and(is_block_sig) {
            return out;
        }
        let start = r.pos();
        let _ = r.sig();
        let Ok(key) = r.sig() else { return out };
        let body_at = r.pos();
        let mut widths = if psb && is_wide_key(&key) { vec![true, false] } else { vec![false] };
        if psb && !is_wide_key(&key) {
            widths.push(true);
        }
        let mut parsed = None;
        for wide in widths {
            let mut t = r.clone();
            if t.seek(body_at).is_err() {
                break;
            }
            let Ok(len) = t.len_v(wide) else { continue };
            if len > t.remaining() as u64 {
                continue;
            }
            let data_at = t.pos();
            let end = data_at + len as usize;
            // Accept if what follows is another block, padding then a block,
            // or the end.
            let mut probe = t.clone();
            let _ = probe.seek(end);
            let ok = probe.remaining() < 12
                || (0..4).any(|p| {
                    let mut q = probe.clone();
                    q.skip(p, "").is_ok() && q.peek(4).is_some_and(is_block_sig)
                })
                || probe.peek(probe.remaining().min(4)).is_some_and(|b| b.iter().all(|&x| x == 0));
            if ok {
                parsed = Some((data_at, end));
                break;
            }
        }
        let Some((data_at, end)) = parsed else {
            let _ = r.seek(start);
            return out;
        };
        let _ = r.seek(data_at);
        let Ok(data) = r.bytes(end - data_at, "tagged block") else { return out };
        out.push(Block { key, data });
    }
}

pub fn parse(bytes: &'_ [u8]) -> Result<PsdFile<'_>> {
    let mut r = Reader::new(bytes);
    if r.remaining() < 26 || &r.sig()? != b"8BPS" {
        return Err(PsdError::NotPsd);
    }
    let version = r.u16()?;
    if version != 1 && version != 2 {
        return Err(PsdError::Unsupported(format!("file format version {version}")));
    }
    r.skip(6, "header")?;
    let header = Header { version, channels: r.u16()?, height: r.u32()?, width: r.u32()?, depth: r.u16()?, mode: r.u16()? };
    let max_dim = if version == 2 { 300_000 } else { 30_000 };
    if header.width == 0 || header.height == 0 || header.width > max_dim || header.height > max_dim {
        return Err(corrupt(format!("document size {}×{}", header.width, header.height)));
    }
    if header.channels == 0 || header.channels > 56 {
        return Err(corrupt(format!("{} channels", header.channels)));
    }
    if !matches!(header.depth, 1 | 8 | 16 | 32) {
        return Err(PsdError::Unsupported(format!("{}-bit documents", header.depth)));
    }
    let psb = header.psb();

    let color_mode_data = r.block_v(false, "colour mode data")?;

    let res_bytes = r.block_v(false, "image resources")?;
    let resources = parse_resources(res_bytes);

    let lm = r.block_v(psb, "layer and mask information")?;
    let merged = r.rest();

    let mut layers = Vec::new();
    let mut merged_alpha = false;
    let mut global_mask: &[u8] = &[];
    let mut global_blocks = Vec::new();
    if !lm.is_empty() {
        let mut lr = Reader::new(lm);
        let info = lr.block_v(psb, "layer info")?;
        // The layer info length is padded; whatever is left is the global
        // mask and then the global tagged blocks.
        if !info.is_empty() {
            let (l, neg) = parse_layer_info(info, psb)?;
            layers = l;
            merged_alpha = neg;
        }
        if lr.remaining() >= 4 {
            let n = lr.u32()? as usize;
            if n <= lr.remaining() {
                global_mask = lr.bytes(n, "global layer mask")?;
            } else {
                return Err(corrupt("global layer mask info runs past its section"));
            }
        }
        global_blocks = parse_blocks(&mut lr, psb);
        if layers.is_empty() {
            for key in [b"Lr16", b"Lr32", b"Layr"] {
                if let Some(b) = global_blocks.iter().find(|b| &b.key == key) {
                    if !b.data.is_empty() {
                        let (l, neg) = parse_layer_info(b.data, psb)?;
                        layers = l;
                        merged_alpha = neg;
                    }
                    break;
                }
            }
        }
    }
    Ok(PsdFile { header, color_mode_data, resources, layers, merged_alpha, global_mask, global_blocks, merged })
}

fn parse_resources(data: &[u8]) -> Vec<Resource<'_>> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    while r.remaining() >= 12 {
        let Ok(sig) = r.sig() else { break };
        if !matches!(&sig, b"8BIM" | b"MeSa" | b"AgHg" | b"PHUT" | b"DCSR") {
            break;
        }
        let Ok(id) = r.u16() else { break };
        let Ok(name) = r.pascal(2) else { break };
        let Ok(len) = r.u32() else { break };
        let Ok(body) = r.bytes(len as usize, "resource") else { break };
        if len % 2 == 1 {
            let _ = r.skip(1, "padding");
        }
        out.push(Resource { sig, id, name, data: body });
    }
    out
}

fn parse_layer_info(info: &[u8], psb: bool) -> Result<(Vec<LayerRecord<'_>>, bool)> {
    let mut r = Reader::new(info);
    let count = r.i16()?;
    let neg = count < 0;
    let n = count.unsigned_abs() as usize;
    // A record is at least 34 bytes; reject counts the section cannot hold.
    if n * 34 > r.remaining() + 34 {
        return Err(corrupt("layer count is larger than the layer section"));
    }
    let mut layers = Vec::with_capacity(n);
    let mut lengths: Vec<Vec<u64>> = Vec::with_capacity(n);
    for _ in 0..n {
        let rect = Rect32 { top: r.i32()?, left: r.i32()?, bottom: r.i32()?, right: r.i32()? };
        let nch = r.u16()? as usize;
        if nch > 56 {
            return Err(corrupt(format!("a layer with {nch} channels")));
        }
        let mut ids = Vec::with_capacity(nch);
        let mut lens = Vec::with_capacity(nch);
        for _ in 0..nch {
            ids.push(r.i16()?);
            lens.push(r.len_v(psb)?);
        }
        let sig = r.sig()?;
        if &sig != b"8BIM" {
            return Err(corrupt("layer record signature"));
        }
        let blend = r.sig()?;
        let opacity = r.u8()?;
        let clipping = r.u8()?;
        let flags = r.u8()?;
        r.skip(1, "layer record")?;
        let extra_len = r.u32()? as usize;
        let mut er = r.sub(extra_len, "layer extra data")?;
        let has_real = ids.contains(&-3);
        let mask = parse_mask(&mut er, has_real)?;
        let br_len = er.u32()? as usize;
        let blending_ranges = er.bytes(br_len, "blending ranges")?;
        let name = er.pascal(4).unwrap_or_default();
        let blocks = parse_blocks(&mut er, psb);
        layers.push(LayerRecord {
            rect,
            channels: ids.iter().map(|&id| Channel { id, raw: &[] }).collect(),
            blend,
            opacity,
            clipping,
            flags,
            mask,
            blending_ranges,
            name,
            blocks,
        });
        lengths.push(lens);
    }
    for (layer, lens) in layers.iter_mut().zip(lengths) {
        for (ch, len) in layer.channels.iter_mut().zip(lens) {
            if len > r.remaining() as u64 {
                return Err(PsdError::Truncated("layer channel data"));
            }
            ch.raw = r.bytes(len as usize, "layer channel data")?;
        }
    }
    Ok((layers, neg))
}

fn parse_mask(r: &mut Reader, has_real: bool) -> Result<Option<MaskInfo>> {
    let len = r.u32()? as usize;
    if len == 0 {
        return Ok(None);
    }
    let mut m = r.sub(len, "layer mask data")?;
    if len < 18 {
        return Ok(None);
    }
    let rect = Rect32 { top: m.i32()?, left: m.i32()?, bottom: m.i32()?, right: m.i32()? };
    let default_color = m.u8()?;
    let flags = m.u8()?;
    let mut info = MaskInfo { rect, default_color, flags, ..Default::default() };
    if has_real && len >= 36 {
        let rf = m.u8()?;
        let rd = m.u8()?;
        let rr = Rect32 { top: m.i32()?, left: m.i32()?, bottom: m.i32()?, right: m.i32()? };
        info.real = Some((rf, rd, rr));
    }
    if flags & MaskInfo::HAS_PARAMS != 0 {
        if let Ok(p) = m.u8() {
            if p & 1 != 0 {
                info.user_density = m.u8().ok();
            }
            if p & 2 != 0 {
                info.user_feather = m.f64().ok();
            }
            if p & 4 != 0 {
                info.vector_density = m.u8().ok();
            }
            if p & 8 != 0 {
                info.vector_feather = m.f64().ok();
            }
        }
    }
    Ok(Some(info))
}
