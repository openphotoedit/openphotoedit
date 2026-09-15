//! What an import keeps for the next export: blocks this engine does not
//! model, and the original bytes of blocks it does, with a fingerprint of
//! what they described so an untouched layer is written back bit for bit.
//! Stored in `DocMeta::extra["psd"]`.

use std::collections::BTreeMap;

use editor_core::document::Document;
use editor_core::layer::LayerId;

use crate::error::Result;
use crate::io::{Reader, Writer};

pub const KEY: &str = "psd";
const MAGIC: &[u8; 8] = b"OPSDSC01";

pub type Blocks = Vec<([u8; 4], Vec<u8>)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerExtra {
    /// Blocks re-emitted verbatim, in file order.
    pub blocks: Blocks,
    /// Blocks that describe the layer's kind (adjustment, fill, type, smart
    /// object) and the fingerprint of the kind at import.
    pub kind_blocks: Blocks,
    pub kind_sig: u64,
    /// `lfx2`/`lrFX`/`lmfx` and the fingerprint of the effects at import.
    pub effects_blocks: Blocks,
    pub effects_sig: u64,
    /// Vector mask blocks (`vmsk`, `vsms`, `vogk`, `vscg`) and the
    /// fingerprint of the mask they were rasterised into.
    pub vector_blocks: Blocks,
    pub vector_sig: u64,
    /// Blending ranges ("Blend If"), as stored.
    pub blending_ranges: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sidecar {
    /// Image resources to re-emit: `(id, name, data)`.
    pub resources: Vec<(u16, String, Vec<u8>)>,
    pub global_blocks: Blocks,
    pub layers: BTreeMap<LayerId, LayerExtra>,
}

/// 64-bit FNV-1a.
pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn fnv_extend(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn write_blocks(w: &mut Writer, b: &Blocks) {
    w.u32(b.len() as u32);
    for (k, d) in b {
        w.bytes(k);
        w.u64(d.len() as u64);
        w.bytes(d);
    }
}

fn read_blocks(r: &mut Reader) -> Result<Blocks> {
    let n = r.u32()? as usize;
    let mut out = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let k = r.sig()?;
        let len = r.u64()?;
        if len > r.remaining() as u64 {
            return Err(crate::error::corrupt("PSD sidecar"));
        }
        out.push((k, r.bytes(len as usize, "sidecar")?.to_vec()));
    }
    Ok(out)
}

impl Sidecar {
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.global_blocks.is_empty() && self.layers.is_empty()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(MAGIC);
        w.u32(self.resources.len() as u32);
        for (id, name, data) in &self.resources {
            w.u16(*id);
            w.unicode(name, false);
            w.u64(data.len() as u64);
            w.bytes(data);
        }
        write_blocks(&mut w, &self.global_blocks);
        w.u32(self.layers.len() as u32);
        for (id, l) in &self.layers {
            w.u32(*id);
            write_blocks(&mut w, &l.blocks);
            write_blocks(&mut w, &l.kind_blocks);
            w.u64(l.kind_sig);
            write_blocks(&mut w, &l.effects_blocks);
            w.u64(l.effects_sig);
            write_blocks(&mut w, &l.vector_blocks);
            w.u64(l.vector_sig);
            w.u64(l.blending_ranges.len() as u64);
            w.bytes(&l.blending_ranges);
        }
        w.buf
    }

    pub fn decode(bytes: &[u8]) -> Result<Sidecar> {
        let mut r = Reader::new(bytes);
        if r.bytes(8, "sidecar")? != MAGIC {
            return Err(crate::error::corrupt("PSD sidecar version"));
        }
        let mut s = Sidecar::default();
        let n = r.u32()? as usize;
        for _ in 0..n {
            let id = r.u16()?;
            let name = r.unicode()?;
            let len = r.u64()? as usize;
            if len > r.remaining() {
                return Err(crate::error::corrupt("PSD sidecar"));
            }
            s.resources.push((id, name, r.bytes(len, "sidecar")?.to_vec()));
        }
        s.global_blocks = read_blocks(&mut r)?;
        let n = r.u32()? as usize;
        for _ in 0..n {
            let id = r.u32()?;
            let blocks = read_blocks(&mut r)?;
            let kind_blocks = read_blocks(&mut r)?;
            let kind_sig = r.u64()?;
            let effects_blocks = read_blocks(&mut r)?;
            let effects_sig = r.u64()?;
            let vector_blocks = read_blocks(&mut r)?;
            let vector_sig = r.u64()?;
            let len = r.u64()? as usize;
            if len > r.remaining() {
                return Err(crate::error::corrupt("PSD sidecar"));
            }
            let blending_ranges = r.bytes(len, "sidecar")?.to_vec();
            s.layers.insert(id, LayerExtra { blocks, kind_blocks, kind_sig, effects_blocks, effects_sig, vector_blocks, vector_sig, blending_ranges });
        }
        Ok(s)
    }

    pub fn from_doc(doc: &Document) -> Sidecar {
        doc.meta.extra.get(KEY).and_then(|b| Sidecar::decode(&b.0).ok()).unwrap_or_default()
    }

    pub fn store(&self, doc: &mut Document) {
        if self.is_empty() {
            doc.meta.extra.remove(KEY);
        } else {
            doc.meta.extra.insert(KEY.into(), editor_core::document::Sidecar(std::sync::Arc::new(self.encode())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_round_trip() {
        let mut s = Sidecar::default();
        s.resources.push((1060, "".into(), b"<xmp/>".to_vec()));
        s.global_blocks.push((*b"Patt", vec![1, 2, 3]));
        s.layers.insert(
            7,
            LayerExtra { blocks: vec![(*b"shmd", vec![9; 10])], kind_sig: 42, blending_ranges: vec![0, 1], ..Default::default() },
        );
        let back = Sidecar::decode(&s.encode()).unwrap();
        assert_eq!(back, s);
        let enc = s.encode();
        for cut in 0..enc.len() {
            assert!(Sidecar::decode(&enc[..cut]).is_err());
        }
    }
}
