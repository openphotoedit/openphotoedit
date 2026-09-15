//! Channel compression: raw, RLE (PackBits per row), ZIP and ZIP with
//! prediction.

use crate::error::{corrupt, PsdError, Result};
use crate::io::{Reader, Writer};

pub const RAW: u16 = 0;
pub const RLE: u16 = 1;
pub const ZIP: u16 = 2;
pub const ZIP_PREDICT: u16 = 3;

/// Bytes of one decoded row.
pub fn row_bytes(width: usize, depth: u16) -> usize {
    (width * depth as usize).div_ceil(8)
}

/// Largest expansion ratios, used to reject channels whose claimed size
/// cannot possibly come from the bytes present (a tiny damaged file must
/// not make us allocate gigabytes).
const RLE_MAX_RATIO: usize = 128;
const ZIP_MAX_RATIO: usize = 1100;

/// Decode one channel (or the merged image, with `height` = rows of all
/// channels together) into `width * height * bytes_per_sample` bytes.
pub fn decode(data: &[u8], compression: u16, width: usize, height: usize, depth: u16, psb: bool) -> Result<Vec<u8>> {
    if !matches!(depth, 1 | 8 | 16 | 32) {
        return Err(PsdError::Unsupported(format!("{depth}-bit channels")));
    }
    let row = row_bytes(width, depth);
    let total = row.checked_mul(height).ok_or_else(|| corrupt("channel size overflows"))?;
    if total == 0 {
        return Ok(Vec::new());
    }
    match compression {
        RAW => {
            // Short raw data is padded with zeros; nothing is invented beyond
            // what the file could hold.
            if data.len() < total / 2 {
                return Err(corrupt("raw channel data is shorter than its size"));
            }
            let mut out = vec![0u8; total];
            let n = data.len().min(total);
            out[..n].copy_from_slice(&data[..n]);
            Ok(out)
        }
        RLE => decode_rle(data, row, height, psb),
        ZIP | ZIP_PREDICT => {
            if total / ZIP_MAX_RATIO > data.len() {
                return Err(corrupt("ZIP channel claims more data than it could hold"));
            }
            let mut out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, total)
                .map_err(|e| corrupt(format!("ZIP data does not inflate ({:?})", e.status)))?;
            if out.len() < total {
                out.resize(total, 0);
            }
            if compression == ZIP_PREDICT {
                unpredict(&mut out, width, height, depth)?;
            }
            Ok(out)
        }
        other => Err(corrupt(format!("unknown compression method {other}"))),
    }
}

fn decode_rle(data: &[u8], row: usize, height: usize, psb: bool) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let count_bytes = if psb { 4 } else { 2 };
    if data.len() < height.saturating_mul(count_bytes) {
        return Err(corrupt("RLE row table is truncated"));
    }
    let mut counts = Vec::with_capacity(height);
    for _ in 0..height {
        counts.push(if psb { r.u32()? as usize } else { r.u16()? as usize });
    }
    let body = data.len() - r.pos();
    if (row * height) / RLE_MAX_RATIO > body + height {
        return Err(corrupt("RLE channel claims more data than it could hold"));
    }
    let mut out = vec![0u8; row * height];
    for (y, &n) in counts.iter().enumerate() {
        // Some writers get the row table wrong; decode what is there.
        let n = n.min(r.remaining());
        let src = r.bytes(n, "RLE row")?;
        packbits_decode(src, &mut out[y * row..(y + 1) * row]);
    }
    Ok(out)
}

/// PackBits into a fixed-size row. Overlong runs are clipped; short rows
/// leave zeros.
pub fn packbits_decode(src: &[u8], out: &mut [u8]) {
    let (mut i, mut o) = (0usize, 0usize);
    while i < src.len() && o < out.len() {
        let h = src[i] as i8;
        i += 1;
        if h >= 0 {
            let n = h as usize + 1;
            let n = n.min(src.len() - i).min(out.len() - o);
            out[o..o + n].copy_from_slice(&src[i..i + n]);
            i += h as usize + 1;
            o += n;
        } else if h != -128 {
            let n = (1 - h as isize) as usize;
            if i >= src.len() {
                break;
            }
            let v = src[i];
            i += 1;
            let n = n.min(out.len() - o);
            out[o..o + n].fill(v);
            o += n;
        }
    }
}

pub fn packbits_encode(row: &[u8], out: &mut Vec<u8>) {
    let n = row.len();
    let mut i = 0;
    while i < n {
        // Run of equal bytes?
        let mut run = 1;
        while i + run < n && run < 128 && row[i + run] == row[i] {
            run += 1;
        }
        if run >= 3 || (run == 2 && i + run >= n) {
            out.push((1i16 - run as i16) as i8 as u8);
            out.push(row[i]);
            i += run;
            continue;
        }
        // Literal: until a run of 3 starts or 128 bytes.
        let start = i;
        let mut len = 0;
        while i < n && len < 128 {
            if i + 2 < n && row[i] == row[i + 1] && row[i] == row[i + 2] {
                break;
            }
            i += 1;
            len += 1;
        }
        out.push((len - 1) as u8);
        out.extend_from_slice(&row[start..start + len]);
    }
}

/// RLE-encode `height` rows of `row` bytes each: row table then data.
pub fn encode_rle(data: &[u8], row: usize, height: usize, psb: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(data.len() / 2);
    let mut counts = Vec::with_capacity(height);
    for y in 0..height {
        let before = body.len();
        if row > 0 {
            packbits_encode(&data[y * row..(y + 1) * row], &mut body);
        }
        counts.push(body.len() - before);
    }
    let mut w = Writer::new();
    for c in counts {
        if psb {
            w.u32(c as u32);
        } else {
            w.u16(c as u16);
        }
    }
    w.bytes(&body);
    w.buf
}

fn unpredict(buf: &mut [u8], width: usize, height: usize, depth: u16) -> Result<()> {
    match depth {
        8 => {
            for y in 0..height {
                let row = &mut buf[y * width..(y + 1) * width];
                for x in 1..width {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
            }
        }
        16 => {
            for y in 0..height {
                let row = &mut buf[y * width * 2..(y + 1) * width * 2];
                let mut prev = 0u16;
                for x in 0..width {
                    let v = u16::from_be_bytes([row[2 * x], row[2 * x + 1]]).wrapping_add(if x == 0 { 0 } else { prev });
                    row[2 * x..2 * x + 2].copy_from_slice(&v.to_be_bytes());
                    prev = v;
                }
            }
        }
        32 => {
            // Bytes are delta-coded across the row of 4·w bytes, with the
            // samples' bytes grouped by significance (all first bytes, then
            // all second bytes, …).
            let rw = width * 4;
            let mut tmp = vec![0u8; rw];
            for y in 0..height {
                let row = &mut buf[y * rw..(y + 1) * rw];
                for x in 1..rw {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
                for x in 0..width {
                    for b in 0..4 {
                        tmp[x * 4 + b] = row[b * width + x];
                    }
                }
                row.copy_from_slice(&tmp);
            }
        }
        _ => return Err(PsdError::Unsupported("prediction at this bit depth".into())),
    }
    Ok(())
}

pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_round_trip() {
        let rows: Vec<Vec<u8>> = vec![
            vec![],
            vec![7],
            vec![1, 1],
            vec![1, 2, 3, 3, 3, 3, 4, 5, 5],
            (0..600).map(|i| (i / 7) as u8).collect(),
            vec![9; 1000],
            (0..300).map(|i| (i * 31 % 251) as u8).collect(),
        ];
        for row in rows {
            let mut enc = Vec::new();
            packbits_encode(&row, &mut enc);
            let mut out = vec![0u8; row.len()];
            packbits_decode(&enc, &mut out);
            assert_eq!(out, row);
        }
    }

    #[test]
    fn rle_channel_round_trip() {
        let (w, h) = (37, 11);
        let data: Vec<u8> = (0..w * h).map(|i| ((i % 13) * 20) as u8).collect();
        for psb in [false, true] {
            let enc = encode_rle(&data, w, h, psb);
            assert_eq!(decode(&enc, RLE, w, h, 8, psb).unwrap(), data);
        }
    }

    #[test]
    fn zip_prediction_16bit() {
        let (w, h) = (5, 3);
        let vals: Vec<u16> = (0..w * h).map(|i| (i as u16) * 4000).collect();
        // Encode with prediction by hand.
        let mut pred = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = vals[y * w + x];
                let p = if x == 0 { v } else { v.wrapping_sub(vals[y * w + x - 1]) };
                pred.extend_from_slice(&p.to_be_bytes());
            }
        }
        let z = zlib_compress(&pred);
        let out = decode(&z, ZIP_PREDICT, w, h, 16, false).unwrap();
        let back: Vec<u16> = out.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        assert_eq!(back, vals);
    }

    #[test]
    fn absurd_sizes_are_rejected() {
        assert!(decode(&[0, 1, 2], RLE, 30000, 30000, 8, false).is_err());
        assert!(decode(&[0x78, 0x9c, 1, 2], ZIP, 30000, 30000, 8, false).is_err());
        assert!(decode(&[1, 2], RAW, 30000, 30000, 8, false).is_err());
    }
}
