//! Embedded JPEG preview extraction.
//!
//! Every mainstream RAW container carries at least one camera-rendered JPEG.
//! Rather than follow each container's own pointers, scan for JPEG streams,
//! walk their marker structure to find where each ends, and keep the ones
//! that are ordinary 8-bit colour JPEGs (lossless-JPEG raw data uses SOF3 and
//! is skipped). The browser then decodes the bytes itself, which is far
//! faster than decoding in wasm.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JpegRef {
    pub offset: usize,
    pub len: usize,
    pub width: u32,
    pub height: u32,
}

/// Parse a JPEG stream starting at `start` (at FF D8). Returns its extent
/// and frame size, or None if it is not a displayable colour JPEG.
fn parse(data: &[u8], start: usize) -> Option<JpegRef> {
    let mut i = start + 2;
    let mut size = None;
    loop {
        // Skip fill bytes.
        while i < data.len() && data[i] == 0xFF && data.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if i + 4 > data.len() || data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        match marker {
            0xD8 | 0x01 | 0xD0..=0xD7 => {
                i += 2;
                continue;
            }
            0xD9 => return None,
            _ => {}
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 || i + 2 + seg_len > data.len() {
            return None;
        }
        match marker {
            // Baseline, extended, progressive Huffman.
            0xC0 | 0xC1 | 0xC2 => {
                let s = &data[i + 4..i + 2 + seg_len];
                if s.len() < 6 || s[0] != 8 {
                    return None;
                }
                let h = u16::from_be_bytes([s[1], s[2]]) as u32;
                let w = u16::from_be_bytes([s[3], s[4]]) as u32;
                if s[5] != 3 || w == 0 || h == 0 {
                    return None;
                }
                size = Some((w, h));
            }
            // Lossless, arithmetic and hierarchical frames: not a preview.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
            0xDA => {
                let (width, height) = size?;
                // Entropy-coded data runs to the EOI marker.
                let mut j = i + 2 + seg_len;
                while j + 1 < data.len() {
                    if data[j] == 0xFF {
                        let m = data[j + 1];
                        if m == 0xD9 {
                            return Some(JpegRef { offset: start, len: j + 2 - start, width, height });
                        }
                        // Progressive files have further scans; keep walking.
                        if m != 0x00 && !(0xD0..=0xD7).contains(&m) && m != 0xFF {
                            j = walk_segments(data, j)?;
                            continue;
                        }
                    }
                    j += 1;
                }
                return None;
            }
            _ => {}
        }
        i += 2 + seg_len;
    }
}

/// Skip marker segments between progressive scans; returns the index of the
/// next entropy-coded byte.
fn walk_segments(data: &[u8], mut i: usize) -> Option<usize> {
    loop {
        if i + 4 > data.len() || data[i] != 0xFF {
            return Some(i);
        }
        let m = data[i + 1];
        if m == 0xD9 {
            return Some(i);
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 {
            return None;
        }
        i += 2 + seg_len;
        if m == 0xDA {
            return Some(i);
        }
    }
}

/// All embedded colour JPEGs, largest first.
pub fn find_jpegs(data: &[u8]) -> Vec<JpegRef> {
    let mut out: Vec<JpegRef> = Vec::new();
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD8 && data[i + 2] == 0xFF {
            if let Some(j) = parse(data, i) {
                out.push(j);
                i += j.len;
                continue;
            }
        }
        i += 1;
    }
    out.sort_by_key(|j| std::cmp::Reverse(j.width as u64 * j.height as u64));
    out
}

/// The best preview: the smallest JPEG whose long side is at least
/// `min_long_side`, else the largest one.
pub fn best_jpeg(data: &[u8], min_long_side: u32) -> Option<JpegRef> {
    let all = find_jpegs(data);
    all.iter().rev().find(|j| j.width.max(j.height) >= min_long_side).or(all.first()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jpeg(w: u16, h: u16, sof: u8) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        v.extend_from_slice(&[0xFF, sof, 0, 17, 8]);
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        v.extend_from_slice(&[0xFF, 0xDA, 0, 12, 3, 1, 0, 2, 0x11, 3, 0x11, 0, 63, 0]);
        v.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn finds_colour_jpegs_and_skips_lossless() {
        let mut file = vec![0u8; 100];
        let small = fake_jpeg(160, 120, 0xC0);
        let big = fake_jpeg(1600, 1200, 0xC0);
        let lossless = fake_jpeg(6000, 4000, 0xC3);
        file.extend_from_slice(&small);
        file.extend_from_slice(&[1, 2, 3]);
        file.extend_from_slice(&lossless);
        file.extend_from_slice(&big);
        let all = find_jpegs(&file);
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].width, all[0].height), (1600, 1200));
        assert_eq!(&file[all[0].offset..all[0].offset + all[0].len], &big[..]);
        assert_eq!(best_jpeg(&file, 100).unwrap().width, 160);
        assert_eq!(best_jpeg(&file, 1000).unwrap().width, 1600);
        assert_eq!(best_jpeg(&file, 5000).unwrap().width, 1600);
    }
}
