//! Type layers: `TySh` ⇄ `TextData`. The layer's own pixels come from the
//! channel data; this only carries the editable parameters.

use editor_core::color::Rgba8;
use editor_core::layer::{TextAlign, TextData};

use crate::descriptor::{Descriptor, Value};
use crate::engine_data::{self, Ev};
use crate::error::{corrupt, Result};
use crate::io::{Reader, Writer};

pub struct ParsedText {
    pub data: TextData,
    pub font_postscript: Option<String>,
}

/// Family, weight and italic from a PostScript font name
/// ("MyriadPro-BoldIt" → "Myriad Pro", 700, true).
pub fn font_from_postscript(ps: &str) -> (String, u16, bool) {
    let (base, style) = match ps.split_once('-') {
        Some((b, s)) => (b, s),
        None => (ps, ""),
    };
    let base = base.trim_end_matches("MT").trim_end_matches("PS");
    // Split CamelCase: "MyriadPro" → "Myriad Pro".
    let mut family = String::new();
    let chars: Vec<char> = base.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
            family.push(' ');
        }
        family.push(c);
    }
    let s = style.to_lowercase();
    let weight = if s.contains("thin") || s.contains("hairline") {
        100
    } else if s.contains("extralight") || s.contains("ultralight") {
        200
    } else if s.contains("semibold") || s.contains("demibold") {
        600
    } else if s.contains("extrabold") || s.contains("ultrabold") {
        800
    } else if s.contains("black") || s.contains("heavy") {
        900
    } else if s.contains("bold") {
        700
    } else if s.contains("medium") {
        500
    } else if s.contains("light") {
        300
    } else {
        400
    };
    let italic = s.contains("italic") || s.contains("oblique") || s.ends_with("it");
    (family, weight, italic)
}

fn ev_color(v: &Ev) -> Option<Rgba8> {
    let vals = v.get("Values")?.array()?;
    let n = |i: usize| vals.get(i).and_then(Ev::num).unwrap_or(0.0);
    let to = |f: f64| (f.clamp(0.0, 1.0) * 255.0).round() as u8;
    match v.get("Type").and_then(Ev::num).map(|t| t as i32) {
        Some(0) => Some(Rgba8::rgb(to(n(1)), to(n(1)), to(n(1)))),
        Some(2) => {
            let k = 1.0 - n(4);
            Some(Rgba8::rgb(to((1.0 - n(1)) * k), to((1.0 - n(2)) * k), to((1.0 - n(3)) * k)))
        }
        _ => Some(Rgba8 { r: to(n(1)), g: to(n(2)), b: to(n(3)), a: to(n(0)) }),
    }
}

pub fn read(data: &[u8], layer_left: i32, layer_top: i32) -> Result<ParsedText> {
    let mut r = Reader::new(data);
    let version = r.u16()?;
    if version != 1 {
        return Err(corrupt(format!("type layer version {version}")));
    }
    let t: Vec<f64> = (0..6).map(|_| r.f64()).collect::<Result<_>>()?;
    let _text_version = r.u16()?;
    let d = Descriptor::read_versioned(&mut r)?;
    let text = d.text("Txt ").unwrap_or("").replace('\r', "\n");
    let text = text.trim_end_matches('\n').to_string();
    let scale = (t[2] * t[2] + t[3] * t[3]).sqrt().max(1e-6);
    let rotation = t[1].atan2(t[0]).to_degrees() as f32;

    let mut out = TextData { text, rotation, ..Default::default() };
    let mut font_ps = None;
    if let Some(ed) = d.raw("EngineData").and_then(|b| engine_data::parse(b).ok()) {
        let sheet_default = ed.path(&["ResourceDict", "StyleSheetSet"]).and_then(|s| s.idx(0)).and_then(|s| s.get("StyleSheetData"));
        let run = ed.path(&["EngineDict", "StyleRun", "RunArray"]).and_then(|a| a.idx(0)).and_then(|r| r.path(&["StyleSheet", "StyleSheetData"]));
        let get = |k: &str| run.and_then(|r| r.get(k)).or_else(|| sheet_default.and_then(|s| s.get(k)));
        if let Some(size) = get("FontSize").and_then(Ev::num) {
            out.font_size = (size * scale) as f32;
        }
        if let Some(c) = get("FillColor").and_then(ev_color) {
            out.color = c;
        }
        if let Some(font) = get("Font").and_then(Ev::num) {
            let name = ed.path(&["ResourceDict", "FontSet"]).and_then(|f| f.idx(font as usize)).and_then(|f| f.get("Name")).and_then(Ev::str);
            if let Some(name) = name {
                let (family, weight, italic) = font_from_postscript(name);
                out.font_family = format!("{family}, sans-serif");
                out.font_weight = weight;
                out.italic = italic;
                font_ps = Some(name.to_string());
            }
        }
        if get("FauxBold") == Some(&Ev::Bool(true)) {
            out.font_weight = out.font_weight.max(700);
        }
        if get("FauxItalic") == Some(&Ev::Bool(true)) {
            out.italic = true;
        }
        if let Some(tr) = get("Tracking").and_then(Ev::num) {
            out.letter_spacing = (tr / 1000.0) as f32 * out.font_size;
        }
        let auto = get("AutoLeading") != Some(&Ev::Bool(false));
        if !auto {
            if let Some(l) = get("Leading").and_then(Ev::num) {
                if out.font_size > 0.0 {
                    out.line_height = (l * scale) as f32 / out.font_size;
                }
            }
        }
        let para = ed.path(&["EngineDict", "ParagraphRun", "RunArray"]).and_then(|a| a.idx(0)).and_then(|p| p.path(&["ParagraphSheet", "Properties"]));
        out.align = match para.and_then(|p| p.get("Justification")).and_then(Ev::num).map(|v| v as i32) {
            Some(1) => TextAlign::Right,
            Some(2) => TextAlign::Center,
            _ => TextAlign::Left,
        };
        let shape = ed.path(&["EngineDict", "Rendered", "Shapes", "Children"]).and_then(|c| c.idx(0)).and_then(|c| c.path(&["Cookie", "Photoshop"]));
        let box_bounds = shape.and_then(|s| s.get("BoxBounds")).and_then(Ev::array);
        if shape.and_then(|s| s.get("ShapeType")).and_then(Ev::num) == Some(1.0) {
            if let Some(bb) = box_bounds {
                let v = |i: usize| bb.get(i).and_then(Ev::num).unwrap_or(0.0);
                out.box_width = Some(((v(2) - v(0)) * scale) as f32);
                out.x = (t[4] + v(0) * scale) as f32;
                out.y = (t[5] + v(1) * scale) as f32;
            }
        }
    }
    if out.box_width.is_none() {
        // Point text: the transform is the first baseline's origin.
        out.x = match out.align {
            TextAlign::Left => t[4] as f32,
            _ => layer_left as f32,
        };
        out.y = (t[5] as f32 - out.font_size * 0.8).max(layer_top as f32 - out.font_size);
    }
    Ok(ParsedText { data: out, font_postscript: font_ps })
}

fn color_ev(c: Rgba8) -> Ev {
    Ev::dict(vec![
        ("Type", Ev::int(1)),
        ("Values", Ev::Array(vec![Ev::float(c.a as f64 / 255.0), Ev::float(c.r as f64 / 255.0), Ev::float(c.g as f64 / 255.0), Ev::float(c.b as f64 / 255.0)])),
    ])
}

fn postscript_name(t: &TextData) -> String {
    let family: String = t.font_family.split(',').next().unwrap_or("Arial").trim().trim_matches('"').chars().filter(|c| !c.is_whitespace()).collect();
    let family = if family.is_empty() || family.eq_ignore_ascii_case("system-ui") || family.eq_ignore_ascii_case("sans-serif") { "ArialMT".to_string() } else { family };
    let style = match (t.font_weight >= 600, t.italic) {
        (true, true) => "-BoldItalic",
        (true, false) => "-Bold",
        (false, true) => "-Italic",
        _ => "",
    };
    format!("{family}{style}")
}

/// A `TySh` block Photoshop can re-render. `index` counts text layers.
pub fn write(t: &TextData, index: i32, height: f32) -> Vec<u8> {
    let text = format!("{}\r", t.text.replace("\r\n", "\r").replace('\n', "\r"));
    let len = text.encode_utf16().count() as i64;
    let size = t.font_size.max(1.0) as f64;
    let font = postscript_name(t);
    let fonts = Ev::Array(vec![
        Ev::dict(vec![("Name", Ev::Str(font.clone())), ("Script", Ev::int(0)), ("FontType", Ev::int(1)), ("Synthetic", Ev::int(0))]),
        Ev::dict(vec![("Name", Ev::Str("AdobeInvisFont".into())), ("Script", Ev::int(0)), ("FontType", Ev::int(0)), ("Synthetic", Ev::int(0))]),
    ]);
    let justification = match t.align {
        TextAlign::Left => 0,
        TextAlign::Right => 1,
        TextAlign::Center => 2,
    };
    let para_props = || {
        Ev::dict(vec![
            ("Justification", Ev::int(justification)),
            ("FirstLineIndent", Ev::float(0.0)),
            ("StartIndent", Ev::float(0.0)),
            ("EndIndent", Ev::float(0.0)),
            ("SpaceBefore", Ev::float(0.0)),
            ("SpaceAfter", Ev::float(0.0)),
            ("AutoHyphenate", Ev::Bool(true)),
            ("HyphenatedWordSize", Ev::int(6)),
            ("PreHyphen", Ev::int(2)),
            ("PostHyphen", Ev::int(2)),
            ("ConsecutiveHyphens", Ev::int(8)),
            ("Zone", Ev::float(36.0)),
            ("WordSpacing", Ev::Array(vec![Ev::float(0.8), Ev::float(1.0), Ev::float(1.33)])),
            ("LetterSpacing", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0), Ev::float(0.0)])),
            ("GlyphSpacing", Ev::Array(vec![Ev::float(1.0), Ev::float(1.0), Ev::float(1.0)])),
            ("AutoLeading", Ev::float(1.2)),
            ("LeadingType", Ev::int(0)),
            ("Hanging", Ev::Bool(false)),
            ("Burasagari", Ev::Bool(false)),
            ("KinsokuOrder", Ev::int(0)),
            ("EveryLineComposer", Ev::Bool(false)),
        ])
    };
    let style = |with_font: bool| {
        let mut items = vec![];
        if with_font {
            items.push(("Font", Ev::int(0)));
        }
        items.extend([
            ("FontSize", Ev::float(size)),
            ("FauxBold", Ev::Bool(false)),
            ("FauxItalic", Ev::Bool(false)),
            ("AutoLeading", Ev::Bool((t.line_height - 1.2).abs() < 0.01)),
            ("Leading", Ev::float(size * t.line_height as f64)),
            ("HorizontalScale", Ev::float(1.0)),
            ("VerticalScale", Ev::float(1.0)),
            ("Tracking", Ev::int((t.letter_spacing as f64 / size * 1000.0).round() as i64)),
            ("AutoKerning", Ev::Bool(true)),
            ("Kerning", Ev::int(0)),
            ("BaselineShift", Ev::float(0.0)),
            ("FontCaps", Ev::int(0)),
            ("FontBaseline", Ev::int(0)),
            ("Underline", Ev::Bool(false)),
            ("Strikethrough", Ev::Bool(false)),
            ("Ligatures", Ev::Bool(true)),
            ("DLigatures", Ev::Bool(false)),
            ("BaselineDirection", Ev::int(2)),
            ("Tsume", Ev::float(0.0)),
            ("StyleRunAlignment", Ev::int(2)),
            ("Language", Ev::int(0)),
            ("NoBreak", Ev::Bool(false)),
            ("FillColor", color_ev(t.color)),
            ("StrokeColor", color_ev(Rgba8::BLACK)),
            ("FillFlag", Ev::Bool(true)),
            ("StrokeFlag", Ev::Bool(false)),
            ("FillFirst", Ev::Bool(true)),
            ("YUnderline", Ev::int(1)),
            ("OutlineWidth", Ev::float(1.0)),
            ("CharacterDirection", Ev::int(0)),
            ("HindiNumbers", Ev::Bool(false)),
            ("Kashida", Ev::int(1)),
            ("DiacriticPos", Ev::int(2)),
        ]);
        Ev::dict(items)
    };
    let shape_type = if t.box_width.is_some() { 1 } else { 0 };
    let mut photoshop = vec![("ShapeType", Ev::int(shape_type))];
    if let Some(bw) = t.box_width {
        photoshop.push(("BoxBounds", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0), Ev::float(bw as f64), Ev::float(height.max(size as f32) as f64)])));
    } else {
        photoshop.push(("PointBase", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0)])));
    }
    photoshop.push((
        "Base",
        Ev::dict(vec![
            ("ShapeType", Ev::int(shape_type)),
            ("TransformPoint0", Ev::Array(vec![Ev::float(1.0), Ev::float(0.0)])),
            ("TransformPoint1", Ev::Array(vec![Ev::float(0.0), Ev::float(1.0)])),
            ("TransformPoint2", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0)])),
        ]),
    ));
    let resources = || {
        Ev::dict(vec![
            ("TheNormalStyleSheet", Ev::int(0)),
            ("TheNormalParagraphSheet", Ev::int(0)),
            (
                "ParagraphSheetSet",
                Ev::Array(vec![Ev::dict(vec![("Name", Ev::Str("Normal RGB".into())), ("DefaultStyleSheet", Ev::int(0)), ("Properties", para_props())])]),
            ),
            ("StyleSheetSet", Ev::Array(vec![Ev::dict(vec![("Name", Ev::Str("Normal RGB".into())), ("StyleSheetData", style(true))])])),
            ("FontSet", fonts.clone()),
            ("SuperscriptSize", Ev::float(0.583)),
            ("SuperscriptPosition", Ev::float(0.333)),
            ("SubscriptSize", Ev::float(0.583)),
            ("SubscriptPosition", Ev::float(0.333)),
            ("SmallCapSize", Ev::float(0.7)),
        ])
    };
    let ed = Ev::dict(vec![
        (
            "EngineDict",
            Ev::dict(vec![
                ("Editor", Ev::dict(vec![("Text", Ev::Str(text.clone()))])),
                (
                    "ParagraphRun",
                    Ev::dict(vec![
                        (
                            "DefaultRunData",
                            Ev::dict(vec![
                                ("ParagraphSheet", Ev::dict(vec![("DefaultStyleSheet", Ev::int(0)), ("Properties", Ev::Dict(vec![]))])),
                                ("Adjustments", Ev::dict(vec![("Axis", Ev::Array(vec![Ev::float(1.0), Ev::float(0.0), Ev::float(1.0)])), ("XY", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0)]))])),
                            ]),
                        ),
                        (
                            "RunArray",
                            Ev::Array(vec![Ev::dict(vec![
                                ("ParagraphSheet", Ev::dict(vec![("DefaultStyleSheet", Ev::int(0)), ("Properties", para_props())])),
                                ("Adjustments", Ev::dict(vec![("Axis", Ev::Array(vec![Ev::float(1.0), Ev::float(0.0), Ev::float(1.0)])), ("XY", Ev::Array(vec![Ev::float(0.0), Ev::float(0.0)]))])),
                            ])]),
                        ),
                        ("RunLengthArray", Ev::Array(vec![Ev::int(len)])),
                        ("IsJoinable", Ev::int(1)),
                    ]),
                ),
                (
                    "StyleRun",
                    Ev::dict(vec![
                        ("DefaultRunData", Ev::dict(vec![("StyleSheet", Ev::dict(vec![("StyleSheetData", Ev::Dict(vec![]))]))])),
                        ("RunArray", Ev::Array(vec![Ev::dict(vec![("StyleSheet", Ev::dict(vec![("StyleSheetData", style(true))]))])])),
                        ("RunLengthArray", Ev::Array(vec![Ev::int(len)])),
                        ("IsJoinable", Ev::int(2)),
                    ]),
                ),
                (
                    "GridInfo",
                    Ev::dict(vec![
                        ("GridIsOn", Ev::Bool(false)),
                        ("ShowGrid", Ev::Bool(false)),
                        ("GridSize", Ev::float(18.0)),
                        ("GridLeading", Ev::float(22.0)),
                        ("GridColor", color_ev(Rgba8::rgb(0, 0, 255))),
                        ("GridLeadingFillColor", color_ev(Rgba8::rgb(0, 0, 255))),
                        ("AlignLineHeightToGridFlags", Ev::Bool(false)),
                    ]),
                ),
                ("AntiAlias", Ev::int(4)),
                ("UseFractionalGlyphWidths", Ev::Bool(true)),
                (
                    "Rendered",
                    Ev::dict(vec![
                        ("Version", Ev::int(1)),
                        (
                            "Shapes",
                            Ev::dict(vec![
                                ("WritingDirection", Ev::int(0)),
                                (
                                    "Children",
                                    Ev::Array(vec![Ev::dict(vec![
                                        ("ShapeType", Ev::int(shape_type)),
                                        ("Procession", Ev::int(0)),
                                        ("Lines", Ev::dict(vec![("WritingDirection", Ev::int(0)), ("Children", Ev::Array(vec![]))])),
                                        ("Cookie", Ev::dict(vec![("Photoshop", Ev::Dict(photoshop.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))])),
                                    ])]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        ("ResourceDict", resources()),
        ("DocumentResources", resources()),
    ]);
    let engine = engine_data::serialize(&ed);

    let a = (t.rotation as f64).to_radians();
    let (tx, ty) = if t.box_width.is_some() { (t.x as f64, t.y as f64) } else { (t.x as f64, t.y as f64 + size * 0.8) };
    let mut w = Writer::new();
    w.u16(1);
    for v in [a.cos(), a.sin(), -a.sin(), a.cos(), tx, ty] {
        w.f64(v);
    }
    w.u16(50);
    Descriptor::new("TxLr")
        .with("Txt ", Value::Text(text))
        .with("textGridding", Value::enumv("textGridding", "None"))
        .with("Ornt", Value::enumv("Ornt", "Hrzn"))
        .with("AntA", Value::enumv("Annt", "antiAliasSharp"))
        .with("TextIndex", Value::Integer(index))
        .with("EngineData", Value::RawData(engine))
        .write_versioned(&mut w);
    w.u16(1);
    Descriptor::new("warp")
        .with("warpStyle", Value::enumv("warpStyle", "warpNone"))
        .with("warpValue", Value::Double(0.0))
        .with("warpPerspective", Value::Double(0.0))
        .with("warpPerspectiveOther", Value::Double(0.0))
        .with("warpRotate", Value::enumv("Ornt", "Hrzn"))
        .write_versioned(&mut w);
    w.zeros(16);
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postscript_names() {
        assert_eq!(font_from_postscript("MyriadPro-BoldIt"), ("Myriad Pro".to_string(), 700, true));
        assert_eq!(font_from_postscript("ArialMT"), ("Arial".to_string(), 400, false));
    }

    #[test]
    fn text_round_trip() {
        let t = TextData {
            text: "Hello\nworld".into(),
            font_family: "Helvetica, sans-serif".into(),
            font_size: 36.0,
            color: Rgba8::rgb(10, 200, 30),
            align: TextAlign::Center,
            box_width: Some(300.0),
            x: 40.0,
            y: 60.0,
            ..Default::default()
        };
        let bytes = write(&t, 0, 100.0);
        let back = read(&bytes, 40, 60).unwrap().data;
        assert_eq!(back.text, t.text);
        assert!((back.font_size - 36.0).abs() < 1e-3);
        assert_eq!(back.color, t.color);
        assert_eq!(back.align, TextAlign::Center);
        assert_eq!(back.box_width, Some(300.0));
        assert_eq!((back.x, back.y), (40.0, 60.0));
        assert!(back.font_family.starts_with("Helvetica"));
    }
}
