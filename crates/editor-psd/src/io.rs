//! Big-endian byte reading and writing with bounds checks everywhere. A
//! damaged file must produce an error, never a panic.

use crate::error::{PsdError, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn at_end(&self) -> bool {
        self.remaining() == 0
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(PsdError::Truncated("a section"));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(PsdError::Truncated(what));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// A reader over the next `n` bytes; the parent skips past them.
    pub fn sub(&mut self, n: usize, what: &'static str) -> Result<Reader<'a>> {
        Ok(Reader::new(self.bytes(n, what)?))
    }

    pub fn skip(&mut self, n: usize, what: &'static str) -> Result<()> {
        self.bytes(n, what).map(|_| ())
    }

    pub fn peek(&self, n: usize) -> Option<&'a [u8]> {
        self.data.get(self.pos..self.pos + n)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.data[self.pos.min(self.data.len())..];
        self.pos = self.data.len();
        s
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1, "a byte")?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2, "a number")?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4, "a number")?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8, "a number")?;
        Ok(u64::from_be_bytes(b.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.u64()?))
    }
    pub fn sig(&mut self) -> Result<[u8; 4]> {
        let b = self.bytes(4, "a signature")?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    /// A length that is 4 bytes in PSD and 8 in PSB.
    pub fn len_v(&mut self, psb: bool) -> Result<u64> {
        if psb {
            self.u64()
        } else {
            Ok(self.u32()? as u64)
        }
    }

    /// Length-prefixed bytes, bounds-checked against what is left.
    pub fn block_v(&mut self, psb: bool, what: &'static str) -> Result<&'a [u8]> {
        let n = self.len_v(psb)?;
        if n > self.remaining() as u64 {
            return Err(PsdError::Truncated(what));
        }
        self.bytes(n as usize, what)
    }

    /// Pascal string (length byte + MacRoman bytes), padded so that the
    /// whole thing (length byte included) is a multiple of `pad`.
    pub fn pascal(&mut self, pad: usize) -> Result<String> {
        let n = self.u8()? as usize;
        let b = self.bytes(n, "a name")?;
        let total = 1 + n;
        let extra = (pad - total % pad) % pad;
        let extra = extra.min(self.remaining());
        self.skip(extra, "padding")?;
        Ok(macroman_to_string(b))
    }

    /// UTF-16BE string with a u32 character count; trailing NULs trimmed.
    pub fn unicode(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        if n.saturating_mul(2) > self.remaining() {
            return Err(PsdError::Truncated("a text string"));
        }
        let b = self.bytes(n * 2, "a text string")?;
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        let mut s = String::from_utf16_lossy(&units);
        while s.ends_with('\0') {
            s.pop();
        }
        Ok(s)
    }
}

/// MacRoman is ASCII-compatible; the upper half maps to Unicode.
pub fn macroman_to_string(b: &[u8]) -> String {
    const HIGH: [char; 128] = [
        'Ä', 'Å', 'Ç', 'É', 'Ñ', 'Ö', 'Ü', 'á', 'à', 'â', 'ä', 'ã', 'å', 'ç', 'é', 'è', 'ê', 'ë', 'í', 'ì', 'î', 'ï', 'ñ', 'ó', 'ò', 'ô', 'ö', 'õ', 'ú', 'ù', 'û',
        'ü', '†', '°', '¢', '£', '§', '•', '¶', 'ß', '®', '©', '™', '´', '¨', '≠', 'Æ', 'Ø', '∞', '±', '≤', '≥', '¥', 'µ', '∂', '∑', '∏', 'π', '∫', 'ª', 'º',
        'Ω', 'æ', 'ø', '¿', '¡', '¬', '√', 'ƒ', '≈', '∆', '«', '»', '…', '\u{a0}', 'À', 'Ã', 'Õ', 'Œ', 'œ', '–', '—', '“', '”', '‘', '’', '÷', '◊', 'ÿ', 'Ÿ',
        '⁄', '€', '‹', '›', 'ﬁ', 'ﬂ', '‡', '·', '‚', '„', '‰', 'Â', 'Ê', 'Á', 'Ë', 'È', 'Í', 'Î', 'Ï', 'Ì', 'Ó', 'Ô', '\u{f8ff}', 'Ò', 'Ú', 'Û', 'Ù', 'ı',
        'ˆ', '˜', '¯', '˘', '˙', '˚', '¸', '˝', '˛', 'ˇ',
    ];
    b.iter().map(|&c| if c < 128 { c as char } else { HIGH[(c - 128) as usize] }).collect()
}

/// The reverse of [`macroman_to_string`]; unmappable characters become `?`.
pub fn string_to_macroman(s: &str) -> Vec<u8> {
    s.chars()
        .map(|ch| {
            if (ch as u32) < 128 {
                ch as u8
            } else {
                (128u8..=255).find(|&c| macroman_to_string(&[c]).starts_with(ch)).unwrap_or(b'?')
            }
        })
        .collect()
}

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { buf: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    pub fn zeros(&mut self, n: usize) {
        self.buf.resize(self.buf.len() + n, 0);
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.bytes(&v.to_be_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    pub fn len_v(&mut self, psb: bool, v: u64) {
        if psb {
            self.u64(v)
        } else {
            self.u32(v as u32)
        }
    }

    /// Reserve a length field; returns its position.
    pub fn begin_len(&mut self, wide: bool) -> usize {
        let at = self.buf.len();
        self.zeros(if wide { 8 } else { 4 });
        at
    }

    /// Fill in a length reserved with `begin_len`, padding the body to a
    /// multiple of `pad`. `include_pad` decides whether the padding counts
    /// in the stored length.
    pub fn end_len(&mut self, at: usize, wide: bool, pad: usize, include_pad: bool) {
        let head = if wide { 8 } else { 4 };
        let body = self.buf.len() - at - head;
        let extra = if pad > 1 { (pad - body % pad) % pad } else { 0 };
        self.zeros(extra);
        let stored = if include_pad { body + extra } else { body } as u64;
        if wide {
            self.buf[at..at + 8].copy_from_slice(&stored.to_be_bytes());
        } else {
            self.buf[at..at + 4].copy_from_slice(&(stored as u32).to_be_bytes());
        }
    }

    pub fn pascal(&mut self, s: &str, pad: usize) {
        let mut b = string_to_macroman(s);
        b.truncate(255);
        self.u8(b.len() as u8);
        self.bytes(&b);
        let total = 1 + b.len();
        self.zeros((pad - total % pad) % pad);
    }

    /// UTF-16BE with a u32 count. `nul` appends a terminating NUL that is
    /// counted, as descriptors do.
    pub fn unicode(&mut self, s: &str, nul: bool) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.u32((units.len() + nul as usize) as u32);
        for u in units {
            self.u16(u);
        }
        if nul {
            self.u16(0);
        }
    }

    pub fn pad_to(&mut self, pad: usize) {
        let extra = (pad - self.buf.len() % pad) % pad;
        self.zeros(extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_reads_error() {
        let mut r = Reader::new(&[0, 1, 2]);
        assert_eq!(r.u16().unwrap(), 1);
        assert!(r.u16().is_err());
        let mut r = Reader::new(&[0, 0, 0, 9, 0]);
        assert!(r.unicode().is_err());
    }

    #[test]
    fn strings_round_trip() {
        let mut w = Writer::new();
        w.pascal("Layér 1", 4);
        w.unicode("Ebene ✓", true);
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.pascal(4).unwrap(), "Layér 1");
        assert_eq!(r.unicode().unwrap(), "Ebene ✓");
        assert!(r.at_end());
    }
}
