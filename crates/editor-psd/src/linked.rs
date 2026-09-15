//! Files embedded for smart objects (`lnk2` / `lnk3` / `lnkD` global
//! blocks) and the `SoLd` placement descriptor.

use editor_core::geom::Point;

use crate::descriptor::{Descriptor, Value};
use crate::error::Result;
use crate::io::{Reader, Writer};

#[derive(Clone, Debug)]
pub struct LinkedFile<'a> {
    pub id: String,
    pub name: String,
    pub file_type: [u8; 4],
    pub data: &'a [u8],
}

/// Parse every entry of a linked-layer block. Damaged entries end the list.
pub fn parse(block: &[u8]) -> Vec<LinkedFile<'_>> {
    let mut out = Vec::new();
    let mut r = Reader::new(block);
    while r.remaining() > 8 {
        let Ok(size) = r.u64() else { break };
        let start = r.pos();
        if size > r.remaining() as u64 {
            break;
        }
        let entry = (|| -> Result<Option<LinkedFile>> {
            let mut e = Reader::new(r.peek(size as usize).unwrap_or(&[]));
            let ty = e.sig()?;
            let version = e.i32()?;
            let id = e.pascal(1)?;
            let name = e.unicode()?;
            let file_type = e.sig()?;
            let _creator = e.sig()?;
            let data_size = e.u64()?;
            if e.u8()? != 0 {
                Descriptor::read_versioned(&mut e)?;
            }
            if &ty == b"liFE" {
                Descriptor::read_versioned(&mut e)?;
                if version > 3 {
                    e.skip(4 + 4 + 8, "linked file date")?;
                }
                return Ok(None);
            }
            if &ty != b"liFD" || data_size > e.remaining() as u64 {
                return Ok(None);
            }
            // The data slice borrows from the block, not the temporary reader.
            let offset = start + e.pos();
            Ok(Some(LinkedFile { id, name, file_type, data: &block[offset..offset + data_size as usize] }))
        })();
        if let Ok(Some(f)) = entry {
            out.push(f);
        }
        let next = start + size as usize;
        let next = next + (4 - size as usize % 4) % 4;
        if r.seek(next.min(block.len())).is_err() {
            break;
        }
    }
    out
}

/// One `liFD` entry holding `data`.
pub fn write_entry(id: &str, name: &str, file_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut e = Writer::new();
    e.bytes(b"liFD");
    e.i32(2);
    e.pascal(id, 1);
    e.unicode(name, true);
    e.bytes(file_type);
    e.bytes(b"8BIM");
    e.u64(data.len() as u64);
    e.u8(0);
    e.bytes(data);
    let mut w = Writer::new();
    w.u64(e.buf.len() as u64);
    w.bytes(&e.buf);
    w.pad_to(4);
    w.buf
}

pub struct Placement {
    pub id: String,
    pub quad: Option<[Point; 4]>,
    pub has_filters: bool,
    pub has_warp: bool,
}

fn quad_from(list: &[Value]) -> Option<[Point; 4]> {
    if list.len() != 8 {
        return None;
    }
    let v: Vec<f64> = list.iter().filter_map(Value::as_f64).collect();
    (v.len() == 8).then(|| [Point::new(v[0], v[1]), Point::new(v[2], v[3]), Point::new(v[4], v[5]), Point::new(v[6], v[7])])
}

/// `SoLd` / `SoLE`: "soLD", version, descriptor.
pub fn read_sold(data: &[u8]) -> Result<Placement> {
    let mut r = Reader::new(data);
    let _kind = r.sig()?;
    let _version = r.u32()?;
    let d = Descriptor::read_versioned(&mut r)?;
    let quad = d.list("nonAffineTransform").and_then(quad_from).or_else(|| d.list("Trnf").and_then(quad_from));
    let has_warp = d.obj("warp").is_some_and(|w| w.enum_value("warpStyle").is_some_and(|s| s != "warpNone")) || d.obj("quiltWarp").is_some();
    Ok(Placement { id: d.text("Idnt").unwrap_or("").to_string(), quad, has_filters: d.obj("filterFX").is_some(), has_warp })
}

/// `PlLd`: "plcL", version, uuid, page, pages, anti-alias, type, 8 doubles.
pub fn read_plld(data: &[u8]) -> Result<Placement> {
    let mut r = Reader::new(data);
    let _kind = r.sig()?;
    let _version = r.u32()?;
    let id = r.pascal(1)?;
    r.skip(16, "placed layer")?;
    let v: Vec<f64> = (0..8).map(|_| r.f64()).collect::<Result<_>>()?;
    let quad = [Point::new(v[0], v[1]), Point::new(v[2], v[3]), Point::new(v[4], v[5]), Point::new(v[6], v[7])];
    Ok(Placement { id, quad: Some(quad), has_filters: false, has_warp: false })
}

/// A deterministic GUID-shaped id from a seed.
pub fn uuid_from(seed: u64) -> String {
    let a = crate::sidecar::fnv(&seed.to_le_bytes());
    let b = crate::sidecar::fnv(&a.to_le_bytes());
    let x = format!("{a:016x}{b:016x}");
    format!("{}-{}-4{}-a{}-{}", &x[0..8], &x[8..12], &x[13..16], &x[17..20], &x[20..32])
}

pub fn write_sold(id: &str, quad: &[Point; 4], width: u32, height: u32) -> Vec<u8> {
    let trnf = Value::List(quad.iter().flat_map(|p| [Value::Double(p.x), Value::Double(p.y)]).collect());
    let frac = |n: i32| Value::Descriptor(Descriptor::new("null").with("numerator", Value::Integer(n)).with("denominator", Value::Integer(600)));
    let px = |v: f64| Value::unit("#Pxl", v);
    let warp = Descriptor::new("warp")
        .with("warpStyle", Value::enumv("warpStyle", "warpNone"))
        .with("warpValue", Value::Double(0.0))
        .with("warpPerspective", Value::Double(0.0))
        .with("warpPerspectiveOther", Value::Double(0.0))
        .with("warpRotate", Value::enumv("Ornt", "Hrzn"))
        .with(
            "bounds",
            Value::Descriptor(
                Descriptor::new("Rctn").with("Top ", px(0.0)).with("Left", px(0.0)).with("Btom", px(height as f64)).with("Rght", px(width as f64)),
            ),
        )
        .with("uOrder", Value::Integer(4))
        .with("vOrder", Value::Integer(4));
    let d = Descriptor::new("null")
        .with("Idnt", Value::text(id))
        .with("placed", Value::text(id))
        .with("PgNm", Value::Integer(1))
        .with("totalPages", Value::Integer(1))
        .with("frameStep", frac(0))
        .with("duration", frac(0))
        .with("frameCount", Value::Integer(1))
        .with("Annt", Value::Integer(16))
        .with("Type", Value::Integer(2))
        .with("Trnf", trnf.clone())
        .with("nonAffineTransform", trnf)
        .with("warp", Value::Descriptor(warp))
        .with("Sz  ", Value::Descriptor(Descriptor::new("Pnt ").with("Wdth", Value::Double(width as f64)).with("Hght", Value::Double(height as f64))))
        .with("Rslt", Value::unit("#Rsl", 72.0));
    let mut w = Writer::new();
    w.bytes(b"soLD");
    w.u32(4);
    d.write_versioned(&mut w);
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let mut block = write_entry("a-b", "one.psb", b"8BPS", &[1, 2, 3, 4, 5]);
        block.extend(write_entry("c-d", "two.png", b"png ", &[9; 9]));
        let files = parse(&block);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].id, "a-b");
        assert_eq!(files[1].data, &[9; 9]);
        let q = [Point::new(1.0, 2.0), Point::new(3.0, 2.0), Point::new(3.0, 4.0), Point::new(1.0, 4.0)];
        let p = read_sold(&write_sold(&uuid_from(7), &q, 2, 2)).unwrap();
        assert_eq!(p.quad, Some(q));
        assert_eq!(p.id.len(), 36);
    }
}
