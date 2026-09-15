//! EngineData: the PostScript-like dictionary Photoshop's text engine keeps
//! inside a type layer's descriptor.

use crate::error::{corrupt, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    Dict(Vec<(String, Ev)>),
    Array(Vec<Ev>),
    /// Value and whether it was written with a decimal point.
    Num(f64, bool),
    Str(String),
    Bool(bool),
    Name(String),
    Null,
}

impl Ev {
    pub fn get(&self, key: &str) -> Option<&Ev> {
        match self {
            Ev::Dict(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn path(&self, keys: &[&str]) -> Option<&Ev> {
        let mut cur = self;
        for k in keys {
            cur = cur.get(k)?;
        }
        Some(cur)
    }
    pub fn idx(&self, i: usize) -> Option<&Ev> {
        match self {
            Ev::Array(a) => a.get(i),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Ev::Num(v, _) => Some(*v),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Ev::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn array(&self) -> Option<&[Ev]> {
        match self {
            Ev::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn int(v: i64) -> Ev {
        Ev::Num(v as f64, false)
    }
    pub fn float(v: f64) -> Ev {
        Ev::Num(v, true)
    }
    pub fn dict(items: Vec<(&str, Ev)>) -> Ev {
        Ev::Dict(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

const MAX_DEPTH: usize = 256;

pub fn parse(data: &[u8]) -> Result<Ev> {
    let mut end = data.len();
    while end > 0 && data[end - 1] == 0 {
        end -= 1;
    }
    let d = &data[..end];
    let mut i = 0usize;
    // A stack of open containers; a pending dictionary key is kept beside
    // its dictionary.
    enum Frame {
        Dict(Vec<(String, Ev)>, Option<String>),
        Array(Vec<Ev>),
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Ev> = None;

    fn push_value(stack: &mut [Frame], root: &mut Option<Ev>, v: Ev) -> Result<()> {
        match stack.last_mut() {
            None => {
                *root = Some(v);
                Ok(())
            }
            Some(Frame::Array(a)) => {
                a.push(v);
                Ok(())
            }
            Some(Frame::Dict(items, key)) => match key.take() {
                Some(k) => {
                    items.push((k, v));
                    Ok(())
                }
                None => Err(corrupt("EngineData value without a key")),
            },
        }
    }

    let ws = |c: u8| matches!(c, b' ' | b'\n' | b'\r' | b'\t');
    while i < d.len() {
        let c = d[i];
        if ws(c) {
            i += 1;
            continue;
        }
        if stack.len() > MAX_DEPTH {
            return Err(corrupt("EngineData nesting is too deep"));
        }
        if c == b'<' && d.get(i + 1) == Some(&b'<') {
            i += 2;
            stack.push(Frame::Dict(Vec::new(), None));
        } else if c == b'>' && d.get(i + 1) == Some(&b'>') {
            i += 2;
            match stack.pop() {
                Some(Frame::Dict(items, _)) => push_value(&mut stack, &mut root, Ev::Dict(items))?,
                _ => return Err(corrupt("EngineData has an unbalanced >>")),
            }
        } else if c == b'[' {
            i += 1;
            stack.push(Frame::Array(Vec::new()));
        } else if c == b']' {
            i += 1;
            match stack.pop() {
                Some(Frame::Array(items)) => push_value(&mut stack, &mut root, Ev::Array(items))?,
                _ => return Err(corrupt("EngineData has an unbalanced ]")),
            }
        } else if c == b'/' {
            i += 1;
            let start = i;
            while i < d.len() && !ws(d[i]) && !matches!(d[i], b'/' | b'[' | b']' | b'(' | b'<' | b'>') {
                i += 1;
            }
            let name = String::from_utf8_lossy(&d[start..i]).into_owned();
            match stack.last_mut() {
                Some(Frame::Dict(_, key @ None)) => *key = Some(name),
                None => {
                    // Top-level keys without an enclosing <<: condensed form.
                    stack.push(Frame::Dict(Vec::new(), Some(name)));
                }
                _ => push_value(&mut stack, &mut root, Ev::Name(name))?,
            }
        } else if c == b'(' {
            i += 1;
            let mut bytes = Vec::new();
            while i < d.len() && d[i] != b')' {
                if d[i] == b'\\' && i + 1 < d.len() {
                    i += 1;
                }
                bytes.push(d[i]);
                i += 1;
            }
            i += 1;
            let s = if bytes.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&units)
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            push_value(&mut stack, &mut root, Ev::Str(s))?;
        } else if d[i..].starts_with(b"true") {
            i += 4;
            push_value(&mut stack, &mut root, Ev::Bool(true))?;
        } else if d[i..].starts_with(b"false") {
            i += 5;
            push_value(&mut stack, &mut root, Ev::Bool(false))?;
        } else if d[i..].starts_with(b"null") {
            i += 4;
            push_value(&mut stack, &mut root, Ev::Null)?;
        } else if c.is_ascii_digit() || c == b'.' || c == b'-' {
            let start = i;
            while i < d.len() && (d[i].is_ascii_digit() || d[i] == b'.' || d[i] == b'-' || d[i] == b'e' || d[i] == b'E') {
                i += 1;
            }
            let s = std::str::from_utf8(&d[start..i]).unwrap_or("0");
            let v: f64 = s.parse().unwrap_or(0.0);
            push_value(&mut stack, &mut root, Ev::Num(v, s.contains('.')))?;
        } else {
            i += 1;
        }
    }
    // Close a condensed top-level dictionary.
    while let Some(f) = stack.pop() {
        let v = match f {
            Frame::Dict(items, _) => Ev::Dict(items),
            Frame::Array(a) => Ev::Array(a),
        };
        push_value(&mut stack, &mut root, v)?;
    }
    root.ok_or_else(|| corrupt("EngineData is empty"))
}

const FLOAT_KEYS: &[&str] = &[
    "Axis", "XY", "Zone", "WordSpacing", "FirstLineIndent", "GlyphSpacing", "StartIndent", "EndIndent", "SpaceBefore", "SpaceAfter",
    "LetterSpacing", "Values", "GridSize", "GridLeading", "PointBase", "BoxBounds", "TransformPoint0", "TransformPoint1",
    "TransformPoint2", "FontSize", "Leading", "HorizontalScale", "VerticalScale", "BaselineShift", "Tsume", "OutlineWidth",
    "AutoLeading",
];

fn fmt_float(v: f64) -> String {
    let mut s = format!("{v:.5}");
    while s.ends_with('0') && !s.ends_with(".0") {
        s.pop();
    }
    if let Some(rest) = s.strip_prefix("0.") {
        if !rest.starts_with('0') || rest.len() > 1 {
            s = format!(".{rest}");
        }
    } else if let Some(rest) = s.strip_prefix("-0.") {
        s = format!("-.{rest}");
    }
    s
}

pub fn serialize(v: &Ev) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\n\n");
    write_value(&mut out, v, None, false, 0);
    out
}

fn indent(out: &mut Vec<u8>, n: usize) {
    out.extend(std::iter::repeat_n(b'\t', n));
}

fn write_value(out: &mut Vec<u8>, v: &Ev, key: Option<&str>, in_prop: bool, depth: usize) {
    let prefix = |out: &mut Vec<u8>| {
        if in_prop {
            out.push(b' ');
        } else {
            indent(out, depth);
        }
    };
    match v {
        Ev::Null => {
            prefix(out);
            out.extend_from_slice(b"null");
        }
        Ev::Bool(b) => {
            prefix(out);
            out.extend_from_slice(if *b { b"true" } else { b"false" });
        }
        Ev::Num(n, float) => {
            prefix(out);
            let is_float = *float || key.is_some_and(|k| FLOAT_KEYS.contains(&k)) || n.fract() != 0.0;
            if is_float {
                out.extend_from_slice(fmt_float(*n).as_bytes());
            } else {
                out.extend_from_slice((*n as i64).to_string().as_bytes());
            }
        }
        Ev::Name(s) => {
            prefix(out);
            out.push(b'/');
            out.extend_from_slice(s.as_bytes());
        }
        Ev::Str(s) => {
            prefix(out);
            out.push(b'(');
            out.extend_from_slice(&[0xFE, 0xFF]);
            for u in s.encode_utf16() {
                for b in u.to_be_bytes() {
                    if matches!(b, b'(' | b')' | b'\\') {
                        out.push(b'\\');
                    }
                    out.push(b);
                }
            }
            out.push(b')');
        }
        Ev::Array(items) => {
            prefix(out);
            if items.iter().all(|x| matches!(x, Ev::Num(..))) {
                out.push(b'[');
                let ints = key == Some("RunLengthArray");
                for x in items {
                    out.push(b' ');
                    let n = x.num().unwrap_or(0.0);
                    if ints {
                        out.extend_from_slice((n as i64).to_string().as_bytes());
                    } else {
                        out.extend_from_slice(fmt_float(n).as_bytes());
                    }
                }
                out.extend_from_slice(b" ]");
            } else {
                out.extend_from_slice(b"[\n");
                for x in items {
                    write_value(out, x, key, false, depth);
                    out.push(b'\n');
                }
                indent(out, depth);
                out.push(b']');
            }
        }
        Ev::Dict(items) => {
            if in_prop {
                out.push(b'\n');
            }
            indent(out, depth);
            out.extend_from_slice(b"<<\n");
            for (k, val) in items {
                indent(out, depth + 1);
                out.push(b'/');
                out.extend_from_slice(k.as_bytes());
                write_value(out, val, Some(k), true, depth + 1);
                out.push(b'\n');
            }
            indent(out, depth);
            out.extend_from_slice(b">>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_serialize() {
        let v = Ev::dict(vec![
            ("EngineDict", Ev::dict(vec![("Editor", Ev::dict(vec![("Text", Ev::Str("Hi (there)\r".into()))]))])),
            ("Sizes", Ev::Array(vec![Ev::float(12.5), Ev::float(0.25)])),
            ("RunLengthArray", Ev::Array(vec![Ev::int(3)])),
            ("Flag", Ev::Bool(true)),
            ("Fonts", Ev::Array(vec![Ev::dict(vec![("Name", Ev::Str("Arial".into())), ("Script", Ev::int(0))])])),
        ]);
        let bytes = serialize(&v);
        let back = parse(&bytes).unwrap();
        assert_eq!(back.path(&["EngineDict", "Editor", "Text"]).and_then(Ev::str), Some("Hi (there)\r"));
        assert_eq!(back.get("Sizes").and_then(|a| a.idx(1)).and_then(Ev::num), Some(0.25));
        assert_eq!(back.path(&["Fonts"]).and_then(|a| a.idx(0)).and_then(|f| f.get("Name")).and_then(Ev::str), Some("Arial"));
    }

    #[test]
    fn garbage_does_not_panic() {
        for s in [&b"<<"[..], b">>", b"]", b"<< /A [ 1 2 >>", b"(\xfe\xff\x00", b"/A /B /C", b""] {
            let _ = parse(s);
        }
    }
}
