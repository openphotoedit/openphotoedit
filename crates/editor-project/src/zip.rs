//! A minimal ZIP container (stored and deflated entries, no ZIP64), so a
//! project is an ordinary archive any unzip tool can list.

use crate::Error;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (i, e) in t.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    t
}

pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

struct Entry {
    name: String,
    method: u16,
    crc: u32,
    compressed: u32,
    size: u32,
    offset: u32,
}

#[derive(Default)]
pub struct ZipWriter {
    buf: Vec<u8>,
    entries: Vec<Entry>,
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

impl ZipWriter {
    pub fn new() -> ZipWriter {
        ZipWriter::default()
    }

    /// Add an entry; `deflate` compresses it (worth it for JSON, not for
    /// already-compressed tile streams).
    pub fn add(&mut self, name: &str, data: &[u8], deflate: bool) -> Result<(), Error> {
        let (method, body) = if deflate { (8u16, miniz_oxide::deflate::compress_to_vec(data, 6)) } else { (0u16, data.to_vec()) };
        if body.len() > u32::MAX as usize || data.len() > u32::MAX as usize || self.buf.len() > u32::MAX as usize {
            return Err(Error::TooLarge);
        }
        let crc = crc32(data);
        let offset = self.buf.len() as u32;
        let b = &mut self.buf;
        put32(b, LOCAL);
        put16(b, 20);
        put16(b, 0x0800); // UTF-8 names
        put16(b, method);
        put16(b, 0);
        put16(b, 0x21); // 1980-01-01
        put32(b, crc);
        put32(b, body.len() as u32);
        put32(b, data.len() as u32);
        put16(b, name.len() as u16);
        put16(b, 0);
        b.extend_from_slice(name.as_bytes());
        b.extend_from_slice(&body);
        self.entries.push(Entry { name: name.to_string(), method, crc, compressed: body.len() as u32, size: data.len() as u32, offset });
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        let start = self.buf.len();
        for e in &self.entries {
            let b = &mut self.buf;
            put32(b, CENTRAL);
            put16(b, 20);
            put16(b, 20);
            put16(b, 0x0800);
            put16(b, e.method);
            put16(b, 0);
            put16(b, 0x21);
            put32(b, e.crc);
            put32(b, e.compressed);
            put32(b, e.size);
            put16(b, e.name.len() as u16);
            put16(b, 0);
            put16(b, 0);
            put16(b, 0);
            put16(b, 0);
            put32(b, 0);
            put32(b, e.offset);
            b.extend_from_slice(e.name.as_bytes());
        }
        let size = self.buf.len() - start;
        if self.buf.len() > u32::MAX as usize || self.entries.len() > u16::MAX as usize {
            return Err(Error::TooLarge);
        }
        let n = self.entries.len() as u16;
        let b = &mut self.buf;
        put32(b, END);
        put16(b, 0);
        put16(b, 0);
        put16(b, n);
        put16(b, n);
        put32(b, size as u32);
        put32(b, start as u32);
        put16(b, 0);
        Ok(self.buf)
    }
}

pub struct ZipReader<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
}

fn get16(d: &[u8], at: usize) -> Result<u16, Error> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(Error::Corrupt("the archive ends early"))
}
fn get32(d: &[u8], at: usize) -> Result<u32, Error> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(Error::Corrupt("the archive ends early"))
}

impl<'a> ZipReader<'a> {
    pub fn open(data: &'a [u8]) -> Result<ZipReader<'a>, Error> {
        if data.len() < 22 {
            return Err(Error::NotProject);
        }
        let lo = data.len().saturating_sub(22 + 65_535);
        let eocd = (lo..=data.len() - 22).rev().find(|&i| get32(data, i).ok() == Some(END)).ok_or(Error::NotProject)?;
        let count = get16(data, eocd + 10)? as usize;
        let mut at = get32(data, eocd + 16)? as usize;
        let mut entries = Vec::with_capacity(count.min(65_535));
        for _ in 0..count {
            if get32(data, at)? != CENTRAL {
                return Err(Error::Corrupt("the archive directory is damaged"));
            }
            let method = get16(data, at + 10)?;
            let crc = get32(data, at + 16)?;
            let compressed = get32(data, at + 20)?;
            let size = get32(data, at + 24)?;
            let nlen = get16(data, at + 28)? as usize;
            let xlen = get16(data, at + 30)? as usize;
            let clen = get16(data, at + 32)? as usize;
            let offset = get32(data, at + 42)?;
            let name = data.get(at + 46..at + 46 + nlen).ok_or(Error::Corrupt("the archive directory is damaged"))?;
            entries.push(Entry { name: String::from_utf8_lossy(name).into_owned(), method, crc, compressed, size, offset });
            at += 46 + nlen + xlen + clen;
        }
        Ok(ZipReader { data, entries })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// An entry's bytes, refusing anything larger than `limit`.
    pub fn read(&self, name: &str, limit: u64) -> Result<Option<Vec<u8>>, Error> {
        let Some(e) = self.entries.iter().find(|e| e.name == name) else { return Ok(None) };
        if e.size as u64 > limit {
            return Err(Error::TooLarge);
        }
        let at = e.offset as usize;
        if get32(self.data, at)? != LOCAL {
            return Err(Error::Corrupt("an archive entry is damaged"));
        }
        let nlen = get16(self.data, at + 26)? as usize;
        let xlen = get16(self.data, at + 28)? as usize;
        let start = at + 30 + nlen + xlen;
        let end = start.checked_add(e.compressed as usize).ok_or(Error::Corrupt("an archive entry is truncated"))?;
        let body = self.data.get(start..end).ok_or(Error::Corrupt("an archive entry is truncated"))?;
        let out = match e.method {
            0 => body.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(body, e.size as usize).map_err(|_| Error::Corrupt("an archive entry does not inflate"))?,
            _ => return Err(Error::Corrupt("an archive entry uses an unknown compression")),
        };
        if out.len() != e.size as usize || crc32(&out) != e.crc {
            return Err(Error::Corrupt("an archive entry fails its checksum"));
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn round_trip() {
        let mut w = ZipWriter::new();
        w.add("a.json", br#"{"hello": "world world world world"}"#, true).unwrap();
        w.add("b/c.bin", &[1, 2, 3], false).unwrap();
        let bytes = w.finish().unwrap();
        let r = ZipReader::open(&bytes).unwrap();
        assert_eq!(r.names().collect::<Vec<_>>(), ["a.json", "b/c.bin"]);
        assert_eq!(r.read("b/c.bin", 10).unwrap().unwrap(), vec![1, 2, 3]);
        assert!(r.read("a.json", 5).is_err());
        assert!(r.read("nope", 5).unwrap().is_none());
    }
}
