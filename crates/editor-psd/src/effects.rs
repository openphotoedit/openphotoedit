//! Layer styles: the `lfx2` / `lmfx` object-based effects descriptor ⇄
//! `editor_core::effects::LayerEffects`.

use editor_core::blend::BlendMode;
use editor_core::color::Rgba8;
use editor_core::effects::*;
use editor_core::layer::GradientKind;

use crate::descriptor::{Descriptor, Value};
use crate::error::Result;
use crate::io::{Reader, Writer};
use crate::kinds::{color_descriptor, descriptor_color, gradient_descriptor, gradient_kind_enum, read_gradient_stops};
use crate::pattern::Patterns;

const BLEND_ENUMS: [(BlendMode, &str); 27] = [
    (BlendMode::Normal, "Nrml"),
    (BlendMode::Dissolve, "Dslv"),
    (BlendMode::Darken, "Drkn"),
    (BlendMode::Multiply, "Mltp"),
    (BlendMode::ColorBurn, "CBrn"),
    (BlendMode::LinearBurn, "linearBurn"),
    (BlendMode::DarkerColor, "darkerColor"),
    (BlendMode::Lighten, "Lghn"),
    (BlendMode::Screen, "Scrn"),
    (BlendMode::ColorDodge, "CDdg"),
    (BlendMode::LinearDodge, "linearDodge"),
    (BlendMode::LighterColor, "lighterColor"),
    (BlendMode::Overlay, "Ovrl"),
    (BlendMode::SoftLight, "SftL"),
    (BlendMode::HardLight, "HrdL"),
    (BlendMode::VividLight, "vividLight"),
    (BlendMode::LinearLight, "linearLight"),
    (BlendMode::PinLight, "pinLight"),
    (BlendMode::HardMix, "hardMix"),
    (BlendMode::Difference, "Dfrn"),
    (BlendMode::Exclusion, "Xclu"),
    (BlendMode::Subtract, "blendSubtraction"),
    (BlendMode::Divide, "blendDivide"),
    (BlendMode::Hue, "H   "),
    (BlendMode::Saturation, "Strt"),
    (BlendMode::Color, "Clr "),
    (BlendMode::Luminosity, "Lmns"),
];

pub fn blend_from_enum(s: &str) -> Option<BlendMode> {
    BLEND_ENUMS.iter().find(|(_, e)| *e == s).map(|(m, _)| *m).or(match s {
        "Sbtr" => Some(BlendMode::Subtract),
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        _ => None,
    })
}

pub fn blend_enum(m: BlendMode) -> Value {
    let s = BLEND_ENUMS.iter().find(|(b, _)| *b == m).map(|(_, e)| *e).unwrap_or("Nrml");
    Value::enumv("BlnM", s)
}

/// Document-wide light, from image resources 1037 (angle) and 1049
/// (altitude).
#[derive(Clone, Copy, Debug)]
pub struct GlobalLight {
    pub angle: f32,
    pub altitude: f32,
}

impl Default for GlobalLight {
    fn default() -> Self {
        GlobalLight { angle: 120.0, altitude: 30.0 }
    }
}

struct Obj<'a>(&'a Descriptor, GlobalLight);

impl Obj<'_> {
    fn enabled(&self) -> bool {
        self.0.boolean("enab").unwrap_or(true)
    }
    fn blend(&self, key: &str, def: BlendMode) -> BlendMode {
        self.0.enum_value(key).and_then(|s| blend_from_enum(&s)).unwrap_or(def)
    }
    fn color(&self, key: &str, def: Rgba8) -> Rgba8 {
        self.0.obj(key).and_then(descriptor_color).unwrap_or(def)
    }
    fn pct(&self, key: &str, def: f32) -> f32 {
        self.0.num(key).map(|v| v as f32 / 100.0).unwrap_or(def)
    }
    fn f(&self, key: &str, def: f32) -> f32 {
        self.0.num(key).map(|v| v as f32).unwrap_or(def)
    }
    fn angle(&self) -> f32 {
        if self.0.boolean("uglg").unwrap_or(false) {
            self.1.angle
        } else {
            self.f("lagl", self.1.angle)
        }
    }
    fn shadow(&self, def: &Shadow) -> Shadow {
        Shadow {
            enabled: self.enabled(),
            blend: self.blend("Md  ", def.blend),
            color: self.color("Clr ", def.color),
            opacity: self.pct("Opct", def.opacity),
            angle: self.angle(),
            distance: self.f("Dstn", def.distance),
            spread: self.f("Ckmt", def.spread),
            size: self.f("blur", def.size),
        }
    }
    fn glow(&self, def: &Glow) -> Glow {
        Glow {
            enabled: self.enabled(),
            blend: self.blend("Md  ", def.blend),
            color: self.color("Clr ", def.color),
            opacity: self.pct("Opct", def.opacity),
            spread: self.f("Ckmt", def.spread),
            size: self.f("blur", def.size),
            source: if self.0.enum_value("glwS").as_deref() == Some("SrcC") { GlowSource::Center } else { GlowSource::Edge },
            range: self.pct("Inpr", def.range),
            // A gradient glow carries `Grad` where a solid one carries `Clr `.
            stops: self.0.obj("Grad").filter(|g| g.enum_value("GrdF").as_deref() != Some("ClNs")).map(read_gradient_stops).unwrap_or_default(),
        }
    }
}

/// The first descriptor of a single key or of its `…Multi` list.
fn first<'a>(d: &'a Descriptor, single: &str, multi: &str) -> Option<&'a Descriptor> {
    d.obj(single).or_else(|| d.list(multi).and_then(|l| l.iter().find_map(Value::as_descriptor)))
}

/// The gradient keys shared by the gradient overlay and a gradient stroke:
/// `Grad`, `Type`, `Angl`, `Scl `, `Rvrs` and `Ofst`. Blend, opacity and the
/// enabled flag belong to the effect, not to the gradient, so they keep
/// `def`'s values here.
fn gradient(o: &Descriptor, def: &GradientOverlay) -> GradientOverlay {
    GradientOverlay {
        enabled: def.enabled,
        blend: def.blend,
        opacity: def.opacity,
        stops: o.obj("Grad").map(read_gradient_stops).unwrap_or_else(|| def.stops.clone()),
        gradient: match o.enum_value("Type").as_deref() {
            Some("Rdl ") => GradientKind::Radial,
            Some("Angl") => GradientKind::Angle,
            Some("Rflc") => GradientKind::Reflected,
            Some("Dmnd") => GradientKind::Diamond,
            _ => GradientKind::Linear,
        },
        angle: o.num("Angl").map(|v| v as f32).unwrap_or(def.angle),
        scale: o.num("Scl ").map(|v| v as f32).unwrap_or(def.scale),
        reverse: o.boolean("Rvrs").unwrap_or(false),
        offset: o.obj("Ofst").map(|q| (q.num("Hrzn").unwrap_or(0.0) as f32, q.num("Vrtc").unwrap_or(0.0) as f32)).unwrap_or(def.offset),
    }
}

/// A `Ptrn` sub-descriptor (`Nm  ` and `Idnt`) plus the tiling keys beside
/// it, resolved against the file's pattern library.
fn pattern_overlay(o: &Descriptor, patterns: &Patterns, def: &PatternOverlay) -> (PatternOverlay, Option<String>) {
    let p = o.obj("Ptrn");
    let id = p.and_then(|p| p.text("Idnt")).unwrap_or_default().trim_end_matches('\0').to_string();
    let name = p.and_then(|p| p.text("Nm  ")).unwrap_or_default().trim_end_matches('\0').to_string();
    let image = patterns.get(&id).cloned();
    let missing = if image.is_none() && !id.is_empty() { Some(id.clone()) } else { None };
    let phase = o.obj("phase").map(|q| (q.num("Hrzn").unwrap_or(0.0) as f32, q.num("Vrtc").unwrap_or(0.0) as f32)).unwrap_or((0.0, 0.0));
    (
        PatternOverlay {
            enabled: o.boolean("enab").unwrap_or(true),
            blend: o.enum_value("Md  ").and_then(|s| blend_from_enum(&s)).unwrap_or(def.blend),
            opacity: o.num("Opct").map(|v| v as f32 / 100.0).unwrap_or(def.opacity),
            scale: o.num("Scl ").map(|v| v as f32).unwrap_or(def.scale),
            angle: o.num("Angl").map(|v| v as f32).unwrap_or(def.angle),
            phase,
            // A pattern overlay says `Algn`; a pattern stroke says `Lnkd`.
            linked: o.boolean("Algn").or_else(|| o.boolean("Lnkd")).unwrap_or(def.linked),
            name,
            id,
            pattern: image,
        },
        missing,
    )
}

/// Read `lfx2` / `lmfx` data (object effects version, then a descriptor).
pub fn read(data: &[u8], light: GlobalLight, patterns: &Patterns) -> Result<(LayerEffects, Vec<String>)> {
    let mut r = Reader::new(data);
    let _version = r.u32()?;
    let d = Descriptor::read_versioned(&mut r)?;
    // `Scl ` is the document's resolution against 72 dpi (150 dpi files say
    // 208.33), not a factor on the effect sizes: every size here is already
    // in pixels, and Photoshop renders them at face value.
    let mut fx = LayerEffects { enabled: d.boolean("masterFXSwitch").unwrap_or(true), scale: 1.0, ..Default::default() };
    let mut dropped = Vec::new();
    if let Some(o) = first(&d, "DrSh", "dropShadowMulti") {
        fx.drop_shadow = Some(Obj(o, light).shadow(&Shadow::default()));
    }
    if let Some(o) = first(&d, "IrSh", "innerShadowMulti") {
        let def = Shadow { blend: BlendMode::Multiply, opacity: 0.35, ..Shadow::default() };
        fx.inner_shadow = Some(Obj(o, light).shadow(&def));
    }
    if let Some(o) = d.obj("OrGl") {
        fx.outer_glow = Some(Obj(o, light).glow(&Glow::default()));
    }
    if let Some(o) = d.obj("IrGl") {
        fx.inner_glow = Some(Obj(o, light).glow(&Glow::default()));
    }
    if let Some(o) = d.obj("ebbl") {
        let ob = Obj(o, light);
        let def = Bevel::default();
        fx.bevel = Some(Bevel {
            enabled: ob.enabled(),
            style: match o.enum_value("bvlS").as_deref() {
                Some("OtrB") => BevelStyle::OuterBevel,
                Some("Embs") => BevelStyle::Emboss,
                Some("PlEb") => BevelStyle::PillowEmboss,
                _ => BevelStyle::InnerBevel,
            },
            technique: match o.enum_value("bvlT").as_deref() {
                Some("PrBL") | Some("Slmt") => BevelTechnique::ChiselHard,
                _ => BevelTechnique::Smooth,
            },
            depth: ob.f("srgR", def.depth),
            down: o.enum_value("bvlD").as_deref() == Some("Out "),
            size: ob.f("blur", def.size),
            soften: ob.f("Sftn", def.soften),
            angle: ob.angle(),
            altitude: if o.boolean("uglg").unwrap_or(false) { light.altitude } else { ob.f("Lald", light.altitude) },
            highlight_blend: ob.blend("hglM", def.highlight_blend),
            highlight_color: ob.color("hglC", def.highlight_color),
            highlight_opacity: ob.pct("hglO", def.highlight_opacity),
            shadow_blend: ob.blend("sdwM", def.shadow_blend),
            shadow_color: ob.color("sdwC", def.shadow_color),
            shadow_opacity: ob.pct("sdwO", def.shadow_opacity),
        });
        if o.boolean("useTexture").unwrap_or(false) {
            dropped.push("bevel texture".to_string());
        }
    }
    if let Some(o) = d.obj("ChFX") {
        let ob = Obj(o, light);
        let def = Satin::default();
        fx.satin = Some(Satin {
            enabled: ob.enabled(),
            blend: ob.blend("Md  ", def.blend),
            color: ob.color("Clr ", def.color),
            opacity: ob.pct("Opct", def.opacity),
            angle: ob.f("lagl", def.angle),
            distance: ob.f("Dstn", def.distance),
            size: ob.f("blur", def.size),
            invert: o.boolean("Invr").unwrap_or(def.invert),
        });
    }
    if let Some(o) = first(&d, "SoFi", "solidFillMulti") {
        let ob = Obj(o, light);
        let def = ColorOverlay::default();
        fx.color_overlay = Some(ColorOverlay { enabled: ob.enabled(), blend: ob.blend("Md  ", def.blend), color: ob.color("Clr ", def.color), opacity: ob.pct("Opct", def.opacity) });
    }
    if let Some(o) = first(&d, "GrFl", "gradientFillMulti") {
        let ob = Obj(o, light);
        let def = GradientOverlay::default();
        fx.gradient_overlay = Some(GradientOverlay { enabled: ob.enabled(), blend: ob.blend("Md  ", def.blend), opacity: ob.pct("Opct", def.opacity), ..gradient(o, &def) });
    }
    // Photoshop can stack several strokes; every one of them draws.
    let frames: Vec<&Descriptor> = match d.obj("FrFX") {
        Some(o) => vec![o],
        None => d.list("frameFXMulti").map(|l| l.iter().filter_map(Value::as_descriptor).collect()).unwrap_or_default(),
    };
    for o in frames {
        let ob = Obj(o, light);
        let def = StrokeEffect::default();
        // A stroke is painted with a colour, a gradient or a pattern; the
        // gradient shares its geometry with the gradient overlay.
        let paint = o.enum_value("PntT").unwrap_or_else(|| "SClr".into());
        let mut stroke = StrokeEffect {
            enabled: ob.enabled(),
            size: ob.f("Sz  ", def.size),
            position: match o.enum_value("Styl").as_deref() {
                Some("InsF") => StrokePosition::Inside,
                Some("CtrF") => StrokePosition::Center,
                _ => StrokePosition::Outside,
            },
            blend: ob.blend("Md  ", def.blend),
            opacity: ob.pct("Opct", def.opacity),
            color: ob.color("Clr ", def.color),
            gradient: None,
            pattern: None,
        };
        match paint.as_str() {
            // A noise gradient is generated from a seed, not from stops;
            // there is nothing to interpolate, so keep the colour stroke.
            "GrFl" if o.obj("Grad").and_then(|g| g.enum_value("GrdF")).as_deref() == Some("ClNs") => {
                dropped.push("a noise-gradient stroke (drawn as a colour stroke)".to_string());
            }
            "GrFl" => stroke.gradient = Some(gradient(o, &GradientOverlay::default())),
            "Ptrn" => {
                let (p, missing) = pattern_overlay(o, patterns, &PatternOverlay::default());
                if stroke.enabled && missing.is_some() {
                    dropped.push("a pattern stroke whose pattern is not in this file (drawn as a colour stroke)".to_string());
                }
                if p.pattern.is_some() {
                    stroke.pattern = Some(p);
                }
            }
            _ => {}
        }
        if fx.stroke.is_none() {
            fx.stroke = Some(stroke);
        } else {
            fx.extra_strokes.push(stroke);
        }
    }
    if let Some(o) = first(&d, "patternFill", "patternFillMulti") {
        let (p, missing) = pattern_overlay(o, patterns, &PatternOverlay::default());
        if p.enabled && missing.is_some() {
            dropped.push("a pattern overlay whose pattern is not in this file".to_string());
        }
        // Photoshop leaves a disabled patternFill on every layer it touches;
        // only carry one that has pixels to draw or was actually switched on.
        if p.enabled || p.pattern.is_some() {
            fx.pattern_overlay = Some(if p.enabled { p } else { PatternOverlay { pattern: None, ..p } });
        }
    }
    for multi in ["dropShadowMulti", "innerShadowMulti", "solidFillMulti", "gradientFillMulti"] {
        if d.list(multi).is_some_and(|l| l.iter().filter(|v| v.as_descriptor().is_some_and(|o| o.boolean("enab").unwrap_or(true))).count() > 1) {
            dropped.push(format!("extra {} instances", multi.trim_end_matches("Multi")));
        }
    }
    Ok((fx, dropped))
}

fn common(d: Descriptor, enabled: bool, blend: BlendMode, opacity: f32) -> Descriptor {
    d.with("enab", Value::Bool(enabled))
        .with("present", Value::Bool(true))
        .with("showInDialog", Value::Bool(true))
        .with("Md  ", blend_enum(blend))
        .with("Opct", Value::unit("#Prc", (opacity * 100.0) as f64))
}

fn contour() -> Value {
    let pt = |x: f64, y: f64| Value::Descriptor(Descriptor::new("CrPt").with("Hrzn", Value::Double(x)).with("Vrtc", Value::Double(y)));
    Value::Descriptor(Descriptor::new("ShpC").with("Nm  ", Value::text("Linear")).with("Crv ", Value::List(vec![pt(0.0, 0.0), pt(255.0, 255.0)])))
}

fn px(v: f32) -> Value {
    Value::unit("#Pxl", v as f64)
}

/// The gradient keys, on a descriptor that already carries the effect's own
/// enabled flag, blend and opacity.
fn gradient_keys(o: Descriptor, g: &GradientOverlay) -> Descriptor {
    o.with("Grad", Value::Descriptor(gradient_descriptor(&g.stops)))
        .with("Angl", Value::unit("#Ang", g.angle as f64))
        .with("Type", Value::enumv("GrdT", gradient_kind_enum(g.gradient)))
        .with("Rvrs", Value::Bool(g.reverse))
        .with("Dthr", Value::Bool(false))
        .with("Algn", Value::Bool(true))
        .with("Scl ", Value::unit("#Prc", g.scale as f64))
        .with("Ofst", Value::Descriptor(Descriptor::new("Pnt ").with("Hrzn", Value::unit("#Prc", g.offset.0 as f64)).with("Vrtc", Value::unit("#Prc", g.offset.1 as f64))))
}

/// The pattern keys. The pixels stay in the file's `Patt` resource, which
/// import keeps and export writes back; here we only name the pattern again.
/// `link_key` is `Algn` for an overlay and `Lnkd` for a stroke.
fn pattern_keys(o: Descriptor, p: &PatternOverlay, link_key: &str) -> Descriptor {
    let mut o = o
        .with("Scl ", Value::unit("#Prc", p.scale as f64))
        .with(link_key, Value::Bool(p.linked))
        .with("phase", Value::Descriptor(Descriptor::new("Pnt ").with("Hrzn", Value::Double(p.phase.0 as f64)).with("Vrtc", Value::Double(p.phase.1 as f64))));
    if !p.id.is_empty() {
        o.set("Ptrn", Value::Descriptor(Descriptor::new("Ptrn").with("Nm  ", Value::text(&p.name)).with("Idnt", Value::text(&p.id))));
    }
    if p.angle != 0.0 {
        o.set("Angl", Value::unit("#Ang", p.angle as f64));
    }
    o
}

/// Write `lfx2` data for these effects.
pub fn write(fx: &LayerEffects) -> Vec<u8> {
    let mut d = Descriptor::new("null").with("Scl ", Value::unit("#Prc", (fx.scale * 100.0) as f64)).with("masterFXSwitch", Value::Bool(fx.enabled));
    let shadow = |s: &Shadow, class: &str| {
        common(Descriptor::new(class), s.enabled, s.blend, s.opacity)
            .with("Clr ", color_descriptor(s.color))
            .with("uglg", Value::Bool(false))
            .with("lagl", Value::unit("#Ang", s.angle as f64))
            .with("Dstn", px(s.distance))
            .with("Ckmt", px(s.spread))
            .with("blur", px(s.size))
            .with("Nose", Value::unit("#Prc", 0.0))
            .with("AntA", Value::Bool(false))
            .with("TrnS", contour())
            .with("layerConceals", Value::Bool(true))
    };
    let glow = |g: &Glow, class: &str, inner: bool| {
        let mut o = common(Descriptor::new(class), g.enabled, g.blend, g.opacity);
        if g.stops.is_empty() {
            o.set("Clr ", color_descriptor(g.color));
        } else {
            o.set("Grad", Value::Descriptor(gradient_descriptor(&g.stops)));
        }
        let mut o = o
            .with("GlwT", Value::enumv("BETE", "SfBL"))
            .with("Ckmt", px(g.spread))
            .with("blur", px(g.size))
            .with("Nose", Value::unit("#Prc", 0.0))
            .with("ShdN", Value::unit("#Prc", 0.0))
            .with("AntA", Value::Bool(false))
            .with("TrnS", contour())
            .with("Inpr", Value::unit("#Prc", (g.range * 100.0) as f64));
        if inner {
            o.set("glwS", Value::enumv("IGSr", if g.source == GlowSource::Center { "SrcC" } else { "SrcE" }));
        }
        o
    };
    if let Some(s) = &fx.drop_shadow {
        d.set("DrSh", Value::Descriptor(shadow(s, "DrSh")));
    }
    if let Some(s) = &fx.inner_shadow {
        d.set("IrSh", Value::Descriptor(shadow(s, "IrSh")));
    }
    if let Some(g) = &fx.outer_glow {
        d.set("OrGl", Value::Descriptor(glow(g, "OrGl", false)));
    }
    if let Some(g) = &fx.inner_glow {
        d.set("IrGl", Value::Descriptor(glow(g, "IrGl", true)));
    }
    if let Some(b) = &fx.bevel {
        let o = Descriptor::new("ebbl")
            .with("enab", Value::Bool(b.enabled))
            .with("present", Value::Bool(true))
            .with("showInDialog", Value::Bool(true))
            .with("hglM", blend_enum(b.highlight_blend))
            .with("hglC", color_descriptor(b.highlight_color))
            .with("hglO", Value::unit("#Prc", (b.highlight_opacity * 100.0) as f64))
            .with("sdwM", blend_enum(b.shadow_blend))
            .with("sdwC", color_descriptor(b.shadow_color))
            .with("sdwO", Value::unit("#Prc", (b.shadow_opacity * 100.0) as f64))
            .with("bvlT", Value::enumv("bvlT", if b.technique == BevelTechnique::ChiselHard { "PrBL" } else { "SfBL" }))
            .with(
                "bvlS",
                Value::enumv(
                    "BESl",
                    match b.style {
                        BevelStyle::InnerBevel => "InrB",
                        BevelStyle::OuterBevel => "OtrB",
                        BevelStyle::Emboss => "Embs",
                        BevelStyle::PillowEmboss => "PlEb",
                    },
                ),
            )
            .with("uglg", Value::Bool(false))
            .with("lagl", Value::unit("#Ang", b.angle as f64))
            .with("Lald", Value::unit("#Ang", b.altitude as f64))
            .with("srgR", Value::unit("#Prc", b.depth as f64))
            .with("blur", px(b.size))
            .with("bvlD", Value::enumv("BESs", if b.down { "Out " } else { "In  " }))
            .with("TrnS", contour())
            .with("antialiasGloss", Value::Bool(false))
            .with("Sftn", px(b.soften))
            .with("useShape", Value::Bool(false))
            .with("useTexture", Value::Bool(false));
        d.set("ebbl", Value::Descriptor(o));
    }
    if let Some(c) = &fx.color_overlay {
        d.set("SoFi", Value::Descriptor(common(Descriptor::new("SoFi"), c.enabled, c.blend, c.opacity).with("Clr ", color_descriptor(c.color))));
    }
    if let Some(g) = &fx.gradient_overlay {
        let o = gradient_keys(common(Descriptor::new("GrFl"), g.enabled, g.blend, g.opacity), g);
        d.set("GrFl", Value::Descriptor(o));
    }
    if let Some(s) = &fx.satin {
        let o = common(Descriptor::new("ChFX"), s.enabled, s.blend, s.opacity)
            .with("Clr ", color_descriptor(s.color))
            .with("AntA", Value::Bool(true))
            .with("Invr", Value::Bool(s.invert))
            .with("lagl", Value::unit("#Ang", s.angle as f64))
            .with("Dstn", px(s.distance))
            .with("blur", px(s.size))
            .with("MpgS", contour());
        d.set("ChFX", Value::Descriptor(o));
    }
    if let Some(p) = &fx.pattern_overlay {
        let o = pattern_keys(common(Descriptor::new("patternFill"), p.enabled, p.blend, p.opacity), p, "Algn");
        d.set("patternFill", Value::Descriptor(o));
    }
    for (n, s) in fx.stroke.iter().chain(fx.extra_strokes.iter()).enumerate() {
        let o = Descriptor::new("FrFX")
            .with("enab", Value::Bool(s.enabled))
            .with("present", Value::Bool(true))
            .with("showInDialog", Value::Bool(true))
            .with(
                "Styl",
                Value::enumv(
                    "FStl",
                    match s.position {
                        StrokePosition::Outside => "OutF",
                        StrokePosition::Inside => "InsF",
                        StrokePosition::Center => "CtrF",
                    },
                ),
            )
            .with("PntT", Value::enumv("FrFl", if s.gradient.is_some() { "GrFl" } else if s.pattern.is_some() { "Ptrn" } else { "SClr" }))
            .with("Md  ", blend_enum(s.blend))
            .with("Opct", Value::unit("#Prc", (s.opacity * 100.0) as f64))
            .with("Sz  ", px(s.size))
            .with("Clr ", color_descriptor(s.color));
        let o = match (&s.gradient, &s.pattern) {
            (Some(g), _) => gradient_keys(o, g),
            (None, Some(p)) => pattern_keys(o, p, "Lnkd"),
            _ => o,
        };
        // One stroke goes in `FrFX`; a stack goes in `frameFXMulti`, which
        // is where Photoshop itself writes more than one.
        if fx.extra_strokes.is_empty() {
            d.set("FrFX", Value::Descriptor(o));
        } else {
            match d.get("frameFXMulti") {
                Some(Value::List(l)) => {
                    let mut l = l.clone();
                    l.push(Value::Descriptor(o));
                    d.set("frameFXMulti", Value::List(l));
                }
                _ => {
                    debug_assert_eq!(n, 0);
                    d.set("frameFXMulti", Value::List(vec![Value::Descriptor(o)]));
                }
            }
        }
    }
    let mut w = Writer::new();
    w.u32(0);
    d.write_versioned(&mut w);
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_round_trip() {
        let fx = LayerEffects {
            drop_shadow: Some(Shadow { distance: 7.0, size: 9.0, spread: 10.0, angle: 90.0, opacity: 0.5, ..Default::default() }),
            stroke: Some(StrokeEffect { size: 4.0, position: StrokePosition::Inside, color: Rgba8::rgb(255, 0, 0), ..Default::default() }),
            color_overlay: Some(ColorOverlay::default()),
            outer_glow: Some(Glow::default()),
            inner_glow: Some(Glow { source: GlowSource::Center, ..Default::default() }),
            bevel: Some(Bevel { style: BevelStyle::Emboss, down: true, ..Default::default() }),
            satin: Some(Satin::default()),
            gradient_overlay: Some(GradientOverlay { stops: GradientOverlay::default().stops, ..Default::default() }),
            inner_shadow: Some(Shadow { blend: BlendMode::Multiply, ..Default::default() }),
            pattern_overlay: Some(PatternOverlay {
                enabled: true,
                blend: BlendMode::Multiply,
                opacity: 0.5,
                scale: 81.0,
                angle: 30.0,
                phase: (3.0, -4.0),
                linked: false,
                name: "Tie Dye".into(),
                id: "1b29876b-58b7-11d4-b895-a898787104c1".into(),
                pattern: Some(PatternImage { width: 1, height: 1, rgba: vec![1, 2, 3, 4] }),
            }),
            ..Default::default()
        };
        let bytes = write(&fx);
        let mut pats = Patterns::new();
        pats.insert("1b29876b-58b7-11d4-b895-a898787104c1".into(), PatternImage { width: 1, height: 1, rgba: vec![1, 2, 3, 4] });
        let (back, dropped) = read(&bytes, GlobalLight::default(), &pats).unwrap();
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(back, fx);
    }

    #[test]
    fn a_pattern_overlay_naming_a_pattern_the_file_does_not_have_is_reported() {
        let fx = LayerEffects {
            pattern_overlay: Some(PatternOverlay { id: "missing".into(), ..Default::default() }),
            ..Default::default()
        };
        let (back, dropped) = read(&write(&fx), GlobalLight::default(), &Patterns::new()).unwrap();
        assert_eq!(dropped.len(), 1);
        assert!(back.pattern_overlay.is_some_and(|p| p.pattern.is_none()));
    }
}
