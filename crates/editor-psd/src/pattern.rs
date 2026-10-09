//! The file's pattern library: the global `Patt` / `Pat2` / `Pat3` tagged
//! blocks. Pattern overlays, pattern strokes and pattern fills name a pattern
//! by id; the pixels live here and nowhere else.
//!
//! A block is a run of length-prefixed records, each padded to four bytes:
//!
//! ```text
//! u32 version (1)  u32 colour mode  i16 height  i16 width
//! unicode name     pascal id (pad 1)
//! [256 * 3 palette bytes + 4 pad]        (indexed mode only)
//! u32 version (3)  u32 length {
//!     u32 * 4 rectangle (top, left, bottom, right)   u32 colour channels
//!     (channels + 2) * {
//!         u32 written; if written {
//!             u32 length; if length {
//!                 u32 depth  u32 * 4 rectangle  u16 pixel depth  u8 compression
//!                 (length - 23) data bytes
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! The channel list is a fixed set of slots, not a list: `channels` colour
//! slots (Photoshop writes 24 whatever the mode), then two more of which the
//! last holds transparency. So the colour planes are the written slots inside
//! the colour region and any alpha is a written slot after it — psd-tools'
//! `get_pattern_color_channels` reads it the same way.

use std::collections::HashMap;

use editor_core::effects::PatternImage;

use crate::color::{self, to_u8};
use crate::compress;
use crate::io::Reader;
use crate::structure::{PsdFile, MODE_INDEXED};

/// Patterns bigger than this are not worth carrying through the document (and
/// a damaged record must not make us allocate).
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: u64 = 16_000_000;

/// Every pattern in the file, by id (the `Idnt` string of a `Ptrn`
/// descriptor). Later blocks win, as Photoshop's own lookup does.
pub type Patterns = HashMap<String, PatternImage>;

pub fn read_all(file: &PsdFile<'_>) -> Patterns {
    let mut out = Patterns::new();
    for key in [b"Patt", b"Pat2", b"Pat3"] {
        for b in file.global_blocks.iter().filter(|b| &b.key == key) {
            read_block(b.data, &mut out);
        }
    }
    out
}

fn read_block(data: &[u8], out: &mut Patterns) {
    let mut r = Reader::new(data);
    while r.remaining() >= 4 {
        let Ok(len) = r.u32() else { return };
        let len = len as usize;
        if len == 0 || len > r.remaining() {
            return;
        }
        let Ok(body) = r.bytes(len, "a pattern") else { return };
        if let Some((id, img)) = read_one(body) {
            out.insert(id, img);
        }
        // Each record is padded to a four-byte boundary.
        let pad = (4 - len % 4) % 4;
        if r.skip(pad.min(r.remaining()), "pattern padding").is_err() {
            return;
        }
    }
}

fn read_one(data: &[u8]) -> Option<(String, PatternImage)> {
    let mut r = Reader::new(data);
    if r.u32().ok()? != 1 {
        return None;
    }
    let mode = r.u32().ok()? as u16;
    let _point = (r.i16().ok()?, r.i16().ok()?);
    let _name = r.unicode().ok()?;
    let id = r.pascal(1).ok()?;
    let mut palette = Vec::new();
    if mode == MODE_INDEXED {
        // Stored as 256 RGB triples; `mode_to_rgb` wants three planes.
        let t = r.bytes(256 * 3, "a pattern palette").ok()?;
        palette = vec![0u8; 768];
        for i in 0..256 {
            for c in 0..3 {
                palette[c * 256 + i] = t[i * 3 + c];
            }
        }
        r.skip(4, "pattern palette padding").ok()?;
    }
    if r.u32().ok()? != 3 {
        return None;
    }
    let vlen = r.u32().ok()? as usize;
    let mut v = Reader::new(r.bytes(vlen.min(r.remaining()), "a pattern's channels").ok()?);
    let rect = [v.u32().ok()?, v.u32().ok()?, v.u32().ok()?, v.u32().ok()?];
    let (top, left, bottom, right) = (rect[0], rect[1], rect[2], rect[3]);
    let (w, h) = (right.checked_sub(left)?, bottom.checked_sub(top)?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || w as u64 * h as u64 > MAX_PIXELS {
        return None;
    }
    let pixels = w as usize * h as usize;
    let slots = v.u32().ok()?.checked_add(2)?;
    if slots > 64 {
        return None;
    }
    // Which slots are colour: everything before the last two.
    let color_slots = slots as usize - 2;
    let mut planes: Vec<(usize, Vec<u8>)> = Vec::new();
    for slot in 0..slots as usize {
        let written = v.u32().ok()?;
        if written == 0 {
            continue;
        }
        let len = v.u32().ok()? as usize;
        if len == 0 {
            continue;
        }
        if len < 23 || len - 4 > v.remaining() {
            return None;
        }
        let depth = v.u32().ok()?;
        let cr = [v.u32().ok()?, v.u32().ok()?, v.u32().ok()?, v.u32().ok()?];
        let (cw, ch) = (cr[3].checked_sub(cr[1])?, cr[2].checked_sub(cr[0])?);
        let pixel_depth = v.u16().ok()?;
        let comp = v.u8().ok()? as u16;
        let body = v.bytes(len - 23, "pattern channel data").ok()?;
        if cw == 0 || ch == 0 || cw > MAX_SIDE || ch > MAX_SIDE {
            return None;
        }
        let bits = if pixel_depth != 0 { pixel_depth } else { depth as u16 };
        // The channel decompressed against its own rectangle, which is only
        // incidentally the pattern's.
        let raw = compress::decode(body, comp, cw as usize, ch as usize, bits, false).ok()?;
        let n = cw as usize * ch as usize;
        let mut plane = to_u8(&raw, bits, n, cw as usize, slot < color_slots);
        plane.resize(pixels, 0);
        planes.push((slot, plane));
    }
    if planes.is_empty() {
        return None;
    }
    let color_count = planes.iter().filter(|(s, _)| *s < color_slots).count();
    // A pattern that writes nothing into the colour region says nothing about
    // where its boundary is: take every written plane as colour.
    let color_count = if color_count == 0 { planes.len() } else { color_count };
    let chans: Vec<Option<Vec<u8>>> = planes.iter().take(color_count).map(|(_, p)| Some(p.clone())).collect();
    let rgb = color::mode_to_rgb(mode, &chans, pixels, &palette);
    let alpha = planes.get(color_count).map(|(_, p)| p.clone());
    let mut rgba = vec![255u8; pixels * 4];
    for (i, c) in rgb.iter().enumerate() {
        rgba[i * 4..i * 4 + 3].copy_from_slice(c);
        if let Some(a) = &alpha {
            rgba[i * 4 + 3] = a[i];
        }
    }
    Some((id, PatternImage { width: w, height: h, rgba }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Writer;

    /// A two-by-one RGB pattern, raw, written the way Photoshop lays one out.
    fn one_pattern(id: &str) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(1);
        w.u32(3);
        w.i16(1);
        w.i16(2);
        w.unicode("Test", true);
        w.u8(id.len() as u8);
        w.bytes(id.as_bytes());
        w.u32(3);
        let mut v = Writer::new();
        v.u32(0);
        v.u32(0);
        v.u32(1);
        v.u32(2);
        v.u32(3);
        for plane in [[10u8, 20], [30, 40], [50, 60]] {
            v.u32(1);
            v.u32(23 + 2);
            v.u32(8);
            v.u32(0);
            v.u32(0);
            v.u32(1);
            v.u32(2);
            v.u16(8);
            v.u8(0);
            v.bytes(&plane);
        }
        for _ in 0..2 {
            v.u32(0);
        }
        w.u32(v.len() as u32);
        w.bytes(&v.buf);
        w.buf
    }

    #[test]
    fn reads_a_raw_rgb_pattern() {
        let rec = one_pattern("abc");
        let mut block = Writer::new();
        block.u32(rec.len() as u32);
        block.bytes(&rec);
        block.zeros((4 - rec.len() % 4) % 4);
        let mut out = Patterns::new();
        read_block(&block.buf, &mut out);
        let p = out.get("abc").expect("pattern read");
        assert_eq!((p.width, p.height), (2, 1));
        assert_eq!(p.rgba, vec![10, 30, 50, 255, 20, 40, 60, 255]);
    }

    #[test]
    fn a_truncated_block_is_ignored() {
        let rec = one_pattern("abc");
        let mut block = Writer::new();
        block.u32(rec.len() as u32);
        block.bytes(&rec[..rec.len() / 2]);
        let mut out = Patterns::new();
        read_block(&block.buf, &mut out);
        assert!(out.is_empty());
    }
}
