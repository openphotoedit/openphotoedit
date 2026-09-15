//! Action descriptors (`Objc`): Photoshop's typed key-value trees, used by
//! fill layers, several adjustments, text layers, smart objects and effects.

use crate::error::{corrupt, Result};
use crate::io::{Reader, Writer};

/// A descriptor key or class id: four characters, or a longer ASCII name.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Key(pub Vec<u8>);

impl Key {
    pub fn new(s: &str) -> Key {
        Key(s.as_bytes().to_vec())
    }
    pub fn is(&self, s: &str) -> bool {
        self.0 == s.as_bytes()
    }
    pub fn as_str(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Key {
        Key::new(s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RefItem {
    Property { name: String, class: Key, key: Key },
    Class { name: String, class: Key },
    EnumRef { name: String, class: Key, ty: Key, value: Key },
    Offset { name: String, class: Key, value: u32 },
    Identifier(i32),
    Index(i32),
    Name { name: String, class: Key, value: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Descriptor(Descriptor),
    GlobalObject(Descriptor),
    List(Vec<Value>),
    Double(f64),
    UnitFloat([u8; 4], f64),
    UnitFloats([u8; 4], Vec<f64>),
    Text(String),
    Enum(Key, Key),
    Integer(i32),
    LargeInteger(i64),
    Bool(bool),
    Class(String, Key),
    GlobalClass(String, Key),
    Alias(Vec<u8>),
    RawData(Vec<u8>),
    Path(Vec<u8>),
    Reference(Vec<RefItem>),
    ObjectArray(u32, Descriptor),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Descriptor {
    pub name: String,
    pub class: Key,
    pub items: Vec<(Key, Value)>,
}

impl Default for Key {
    fn default() -> Self {
        Key::new("null")
    }
}

const MAX_DEPTH: usize = 64;

fn read_key(r: &mut Reader) -> Result<Key> {
    let n = r.u32()? as usize;
    let n = if n == 0 { 4 } else { n };
    if n > r.remaining() {
        return Err(corrupt("descriptor key runs past the end"));
    }
    Ok(Key(r.bytes(n, "descriptor key")?.to_vec()))
}

fn write_key(w: &mut Writer, k: &Key) {
    let special = [b"warp".as_slice(), b"time", b"hold", b"list"];
    if k.0.len() == 4 && !special.contains(&k.0.as_slice()) {
        w.u32(0);
    } else {
        w.u32(k.0.len() as u32);
    }
    w.bytes(&k.0);
}

impl Descriptor {
    pub fn new(class: &str) -> Descriptor {
        Descriptor { name: String::new(), class: Key::new(class), items: Vec::new() }
    }

    /// A descriptor preceded by its u32 version (16).
    pub fn read_versioned(r: &mut Reader) -> Result<Descriptor> {
        let v = r.u32()?;
        if v != 16 {
            return Err(corrupt(format!("descriptor version {v}")));
        }
        Descriptor::read(r, 0)
    }

    pub fn write_versioned(&self, w: &mut Writer) {
        w.u32(16);
        self.write(w);
    }

    pub fn read(r: &mut Reader, depth: usize) -> Result<Descriptor> {
        if depth > MAX_DEPTH {
            return Err(corrupt("descriptor nesting is too deep"));
        }
        let name = r.unicode()?;
        let class = read_key(r)?;
        let count = r.u32()? as usize;
        // Each item takes at least 9 bytes; a count beyond that is damage.
        if count > r.remaining() / 9 + 1 {
            return Err(corrupt("descriptor item count is impossible"));
        }
        let mut items = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let key = read_key(r)?;
            let ty = r.sig()?;
            items.push((key, read_value(r, &ty, depth + 1)?));
        }
        Ok(Descriptor { name, class, items })
    }

    pub fn write(&self, w: &mut Writer) {
        w.unicode(&self.name, true);
        write_key(w, &self.class);
        w.u32(self.items.len() as u32);
        for (k, v) in &self.items {
            write_key(w, k);
            write_value(w, v);
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.items.iter().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    pub fn set(&mut self, key: &str, v: Value) -> &mut Self {
        if let Some(slot) = self.items.iter_mut().find(|(k, _)| k.is(key)) {
            slot.1 = v;
        } else {
            self.items.push((Key::new(key), v));
        }
        self
    }

    pub fn with(mut self, key: &str, v: Value) -> Self {
        self.set(key, v);
        self
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Value::as_f64)
    }

    pub fn int(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|v| match v {
            Value::Integer(i) => Some(*i),
            Value::LargeInteger(i) => Some(*i as i32),
            Value::Double(d) | Value::UnitFloat(_, d) => Some(d.round() as i32),
            _ => None,
        })
    }

    pub fn boolean(&self, key: &str) -> Option<bool> {
        match self.get(key) {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Value::Text(s)) => Some(s),
            _ => None,
        }
    }

    pub fn enum_value(&self, key: &str) -> Option<String> {
        match self.get(key) {
            Some(Value::Enum(_, v)) => Some(v.as_str()),
            _ => None,
        }
    }

    pub fn obj(&self, key: &str) -> Option<&Descriptor> {
        match self.get(key) {
            Some(Value::Descriptor(d)) | Some(Value::GlobalObject(d)) => Some(d),
            _ => None,
        }
    }

    pub fn list(&self, key: &str) -> Option<&[Value]> {
        match self.get(key) {
            Some(Value::List(l)) => Some(l),
            _ => None,
        }
    }

    pub fn raw(&self, key: &str) -> Option<&[u8]> {
        match self.get(key) {
            Some(Value::RawData(b)) => Some(b),
            _ => None,
        }
    }
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Double(d) | Value::UnitFloat(_, d) => Some(*d),
            Value::Integer(i) => Some(*i as f64),
            Value::LargeInteger(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn as_descriptor(&self) -> Option<&Descriptor> {
        match self {
            Value::Descriptor(d) | Value::GlobalObject(d) => Some(d),
            _ => None,
        }
    }
    pub fn unit(u: &str, v: f64) -> Value {
        let b = u.as_bytes();
        Value::UnitFloat([b[0], b[1], b[2], b[3]], v)
    }
    pub fn enumv(ty: &str, v: &str) -> Value {
        Value::Enum(Key::new(ty), Key::new(v))
    }
    pub fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }
}

fn read_value(r: &mut Reader, ty: &[u8; 4], depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(corrupt("descriptor nesting is too deep"));
    }
    Ok(match ty {
        b"Objc" => Value::Descriptor(Descriptor::read(r, depth)?),
        b"GlbO" => Value::GlobalObject(Descriptor::read(r, depth)?),
        b"VlLs" => {
            let n = r.u32()? as usize;
            if n > r.remaining() / 4 + 1 {
                return Err(corrupt("descriptor list count is impossible"));
            }
            let mut items = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let t = r.sig()?;
                items.push(read_value(r, &t, depth + 1)?);
            }
            Value::List(items)
        }
        b"doub" => Value::Double(r.f64()?),
        b"UntF" => Value::UnitFloat(r.sig()?, r.f64()?),
        b"UnFl" => {
            let unit = r.sig()?;
            let n = r.u32()? as usize;
            if n > r.remaining() / 8 {
                return Err(corrupt("unit float list is truncated"));
            }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(r.f64()?);
            }
            Value::UnitFloats(unit, v)
        }
        b"TEXT" => Value::Text(r.unicode()?),
        b"enum" => Value::Enum(read_key(r)?, read_key(r)?),
        b"long" => Value::Integer(r.i32()?),
        b"comp" => Value::LargeInteger(r.i64()?),
        b"bool" => Value::Bool(r.u8()? != 0),
        b"type" => Value::Class(r.unicode()?, read_key(r)?),
        b"GlbC" => Value::GlobalClass(r.unicode()?, read_key(r)?),
        b"alis" => Value::Alias(r.block_v(false, "alias")?.to_vec()),
        b"tdta" => Value::RawData(r.block_v(false, "raw descriptor data")?.to_vec()),
        b"Pth " => Value::Path(r.block_v(false, "path")?.to_vec()),
        b"ObAr" => {
            let n = r.u32()?;
            Value::ObjectArray(n, Descriptor::read(r, depth)?)
        }
        b"obj " => {
            let n = r.u32()? as usize;
            if n > r.remaining() / 4 + 1 {
                return Err(corrupt("reference count is impossible"));
            }
            let mut items = Vec::new();
            for _ in 0..n {
                let t = r.sig()?;
                items.push(match &t {
                    b"prop" => RefItem::Property { name: r.unicode()?, class: read_key(r)?, key: read_key(r)? },
                    b"Clss" => RefItem::Class { name: r.unicode()?, class: read_key(r)? },
                    b"Enmr" => RefItem::EnumRef { name: r.unicode()?, class: read_key(r)?, ty: read_key(r)?, value: read_key(r)? },
                    b"rele" => RefItem::Offset { name: r.unicode()?, class: read_key(r)?, value: r.u32()? },
                    b"Idnt" => RefItem::Identifier(r.i32()?),
                    b"indx" => RefItem::Index(r.i32()?),
                    b"name" => RefItem::Name { name: r.unicode()?, class: read_key(r)?, value: r.unicode()? },
                    other => return Err(corrupt(format!("unknown reference type {:?}", String::from_utf8_lossy(other)))),
                });
            }
            Value::Reference(items)
        }
        other => return Err(corrupt(format!("unknown descriptor value type {:?}", String::from_utf8_lossy(other)))),
    })
}

fn write_value(w: &mut Writer, v: &Value) {
    match v {
        Value::Descriptor(d) => {
            w.bytes(b"Objc");
            d.write(w);
        }
        Value::GlobalObject(d) => {
            w.bytes(b"GlbO");
            d.write(w);
        }
        Value::List(items) => {
            w.bytes(b"VlLs");
            w.u32(items.len() as u32);
            for i in items {
                write_value(w, i);
            }
        }
        Value::Double(d) => {
            w.bytes(b"doub");
            w.f64(*d);
        }
        Value::UnitFloat(u, d) => {
            w.bytes(b"UntF");
            w.bytes(u);
            w.f64(*d);
        }
        Value::UnitFloats(u, ds) => {
            w.bytes(b"UnFl");
            w.bytes(u);
            w.u32(ds.len() as u32);
            for d in ds {
                w.f64(*d);
            }
        }
        Value::Text(s) => {
            w.bytes(b"TEXT");
            w.unicode(s, true);
        }
        Value::Enum(t, e) => {
            w.bytes(b"enum");
            write_key(w, t);
            write_key(w, e);
        }
        Value::Integer(i) => {
            w.bytes(b"long");
            w.i32(*i);
        }
        Value::LargeInteger(i) => {
            w.bytes(b"comp");
            w.i64(*i);
        }
        Value::Bool(b) => {
            w.bytes(b"bool");
            w.u8(*b as u8);
        }
        Value::Class(n, k) | Value::GlobalClass(n, k) => {
            w.bytes(if matches!(v, Value::Class(..)) { b"type" } else { b"GlbC" });
            w.unicode(n, true);
            write_key(w, k);
        }
        Value::Alias(b) | Value::RawData(b) | Value::Path(b) => {
            w.bytes(match v {
                Value::Alias(_) => b"alis",
                Value::RawData(_) => b"tdta",
                _ => b"Pth ",
            });
            w.u32(b.len() as u32);
            w.bytes(b);
        }
        Value::ObjectArray(n, d) => {
            w.bytes(b"ObAr");
            w.u32(*n);
            d.write(w);
        }
        Value::Reference(items) => {
            w.bytes(b"obj ");
            w.u32(items.len() as u32);
            for it in items {
                match it {
                    RefItem::Property { name, class, key } => {
                        w.bytes(b"prop");
                        w.unicode(name, true);
                        write_key(w, class);
                        write_key(w, key);
                    }
                    RefItem::Class { name, class } => {
                        w.bytes(b"Clss");
                        w.unicode(name, true);
                        write_key(w, class);
                    }
                    RefItem::EnumRef { name, class, ty, value } => {
                        w.bytes(b"Enmr");
                        w.unicode(name, true);
                        write_key(w, class);
                        write_key(w, ty);
                        write_key(w, value);
                    }
                    RefItem::Offset { name, class, value } => {
                        w.bytes(b"rele");
                        w.unicode(name, true);
                        write_key(w, class);
                        w.u32(*value);
                    }
                    RefItem::Identifier(i) => {
                        w.bytes(b"Idnt");
                        w.i32(*i);
                    }
                    RefItem::Index(i) => {
                        w.bytes(b"indx");
                        w.i32(*i);
                    }
                    RefItem::Name { name, class, value } => {
                        w.bytes(b"name");
                        w.unicode(name, true);
                        write_key(w, class);
                        w.unicode(value, true);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let color = Descriptor::new("RGBC").with("Rd  ", Value::Double(12.0)).with("Grn ", Value::Double(34.0)).with("Bl  ", Value::Double(56.0));
        let d = Descriptor::new("null")
            .with("Clr ", Value::Descriptor(color))
            .with("Angl", Value::unit("#Ang", 90.0))
            .with("Type", Value::enumv("GrdT", "Lnr "))
            .with("Rvrs", Value::Bool(true))
            .with("Nm  ", Value::text("Grün"))
            .with("layerID", Value::Integer(-7))
            .with("list", Value::List(vec![Value::Integer(1), Value::Double(2.5)]))
            .with("EngineData", Value::RawData(vec![1, 2, 3]));
        let mut w = Writer::new();
        d.write_versioned(&mut w);
        let mut r = Reader::new(&w.buf);
        let back = Descriptor::read_versioned(&mut r).unwrap();
        assert!(r.at_end());
        assert_eq!(back, d);
        assert_eq!(back.obj("Clr ").unwrap().num("Grn "), Some(34.0));
        assert_eq!(back.enum_value("Type").as_deref(), Some("Lnr "));
    }

    #[test]
    fn damaged_descriptors_error() {
        let d = Descriptor::new("null").with("a", Value::List(vec![Value::Integer(1); 50]));
        let mut w = Writer::new();
        d.write_versioned(&mut w);
        for cut in 0..w.buf.len() {
            let mut r = Reader::new(&w.buf[..cut]);
            assert!(Descriptor::read_versioned(&mut r).is_err(), "cut at {cut}");
        }
    }
}
