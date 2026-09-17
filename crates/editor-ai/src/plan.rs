//! "Describe an edit": a rule-based planner that turns a sentence into
//! visible, undoable steps.
//!
//! This is the no-model fallback research 04 §7 asks for, and the schema the
//! later LLM planner will emit into. A [`Plan`] is a list of [`Step`]s; each
//! step is either an ordinary engine command (`cmd`) or a named high-level
//! AI action (`action` + `params`) that `apps/web/src/lib/ai.ts` runs. The
//! LLM planner will produce exactly this JSON under grammar-constrained
//! decoding, so the executor and the chips UI never learn which planner
//! wrote it.
//!
//! Parsing: lower-case, split into clauses on "and", commas, "then", "also";
//! match each clause against a phrase table (longest phrase first) with
//! intensity words ("a bit", "slightly", "much", "a lot", "very") scaling
//! the amounts. All Develop tweaks in one sentence merge into one Develop
//! adjustment layer, so "brighter and warmer" is one layer, not two.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Set when nothing could be planned: a message for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<String>,
    /// Clauses that matched nothing, when others did.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ignored: Vec<String>,
}

/// What a phrase does.
enum Effect {
    /// Develop fields and amounts (scaled by intensity).
    Develop(&'static [(&'static str, f32)]),
    Cmd(fn(f32) -> Value),
    Action(&'static str, fn(&str) -> Value),
    /// A generative request with no local model: explain.
    Generative,
}

struct Rule {
    phrases: &'static [&'static str],
    label: &'static str,
    effect: Effect,
}

fn no_params(_: &str) -> Value {
    json!({})
}

fn upscale_params(clause: &str) -> Value {
    let factor = if clause.contains('4') || clause.contains("four") || clause.contains("400") { 4 } else { 2 };
    json!({ "factor": factor })
}

fn crop_params(clause: &str) -> Value {
    let ratio = |a: f32, b: f32| json!({ "aspect": a / b });
    if clause.contains("16:9") || clause.contains("16x9") || clause.contains("widescreen") {
        ratio(16.0, 9.0)
    } else if clause.contains("4:5") || clause.contains("instagram") {
        ratio(4.0, 5.0)
    } else if clause.contains("3:2") {
        ratio(3.0, 2.0)
    } else if clause.contains("4:3") {
        ratio(4.0, 3.0)
    } else if clause.contains("9:16") || clause.contains("story") || clause.contains("vertical") {
        ratio(9.0, 16.0)
    } else {
        ratio(1.0, 1.0)
    }
}

fn denoise_params(clause: &str) -> Value {
    let s = if clause.contains("bit") || clause.contains("slight") || clause.contains("little") {
        "low"
    } else if clause.contains("lot") || clause.contains("very") || clause.contains("much") || clause.contains("strong") {
        "high"
    } else {
        "medium"
    };
    json!({ "strength": s })
}

fn blur_params(clause: &str) -> Value {
    json!({ "amount": if clause.contains("bit") || clause.contains("slight") || clause.contains("little") { 6 } else if clause.contains("lot") || clause.contains("very") || clause.contains("much") { 24 } else { 12 } })
}

const RULES: &[Rule] = &[
    // ---- AI actions (checked before tone words: "blur the background" is not "blur")
    Rule { phrases: &["remove the background", "remove background", "delete the background", "cut out the subject", "cut out", "cutout", "transparent background", "isolate the subject", "background removal"], label: "Remove background", effect: Effect::Action("removeBackground", no_params) },
    Rule { phrases: &["blur the background", "blur background", "blurry background", "background blur", "bokeh", "portrait mode", "shallow depth of field", "defocus the background"], label: "Blur background", effect: Effect::Action("blurBackground", blur_params) },
    Rule { phrases: &["select the subject", "select subject", "select the person", "select person", "select the main subject"], label: "Select subject", effect: Effect::Action("selectSubject", no_params) },
    Rule { phrases: &["select the background", "select background"], label: "Select background", effect: Effect::Action("selectBackground", no_params) },
    Rule { phrases: &["remove the selection", "remove selected", "remove the selected", "erase the selection", "remove what is selected", "remove this", "remove that", "remove the object", "remove object", "erase object", "delete object", "content aware fill", "content-aware fill"], label: "Remove selected", effect: Effect::Action("removeSelected", no_params) },
    Rule { phrases: &["upscale", "enlarge", "super resolution", "increase resolution", "higher resolution", "make it bigger", "increase the resolution"], label: "Upscale", effect: Effect::Action("upscale", upscale_params) },
    Rule { phrases: &["restore faces", "restore the face", "restore face", "fix faces", "fix the face", "enhance faces", "enhance the face", "face restoration", "sharpen the face"], label: "Restore faces", effect: Effect::Action("restoreFaces", no_params) },
    Rule { phrases: &["colorize", "colourise", "colorise", "colourize", "add color", "add colour", "make it color", "make it colour"], label: "Colorize", effect: Effect::Action("colorize", no_params) },
    Rule { phrases: &["jpeg artifacts", "jpeg artefacts", "compression artifacts", "compression artefacts", "blocky", "jpeg noise", "deblock"], label: "Remove JPEG artifacts", effect: Effect::Action("removeJpegArtifacts", no_params) },
    Rule { phrases: &["reduce noise", "denoise", "de-noise", "less noise", "less noisy", "remove noise", "noise reduction", "clean up noise", "grainy"], label: "Reduce noise", effect: Effect::Action("denoise", denoise_params) },
    Rule { phrases: &["red eye", "red-eye", "redeye"], label: "Remove red eye", effect: Effect::Action("removeRedEye", no_params) },
    Rule { phrases: &["straighten", "level the horizon", "fix the horizon", "horizon straight", "fix the tilt", "crooked", "tilted"], label: "Straighten", effect: Effect::Action("straighten", no_params) },
    Rule { phrases: &["crop square", "square crop", "crop to square", "crop it square", "make it square", "crop 1:1", "crop to 16:9", "crop 16:9", "crop to 4:5", "crop 4:5", "crop for instagram", "crop 3:2", "crop 4:3", "crop to 9:16", "crop for a story", "crop vertical"], label: "Crop", effect: Effect::Action("crop", crop_params) },
    Rule { phrases: &["auto enhance", "auto-enhance", "autofix", "auto fix", "fix it", "make it better", "enhance it", "improve it", "auto tone", "auto"], label: "Auto enhance", effect: Effect::Action("auto", no_params) },
    Rule { phrases: &["add clouds", "replace the sky", "replace sky", "change the sky", "sky replacement", "add a ", "add an ", "generate", "put a ", "insert a ", "make him", "make her", "change his", "change her", "turn it into", "turn him into", "turn her into", "extend the image", "expand the image", "outpaint", "change the background to", "replace the background"], label: "", effect: Effect::Generative },
    // ---- Commands
    Rule { phrases: &["rotate left", "rotate counterclockwise", "rotate anticlockwise", "rotate -90"], label: "Rotate left", effect: Effect::Cmd(|_| json!({ "op": "image.rotate", "turns": -1 })) },
    Rule { phrases: &["rotate right", "rotate clockwise", "rotate 90", "rotate it"], label: "Rotate right", effect: Effect::Cmd(|_| json!({ "op": "image.rotate", "turns": 1 })) },
    Rule { phrases: &["rotate 180", "upside down"], label: "Rotate 180°", effect: Effect::Cmd(|_| json!({ "op": "image.rotate", "turns": 2 })) },
    Rule { phrases: &["flip vertically", "flip vertical", "flip upside"], label: "Flip vertical", effect: Effect::Cmd(|_| json!({ "op": "image.flip", "horizontal": false })) },
    Rule { phrases: &["flip horizontally", "flip horizontal", "flip it", "mirror", "flip"], label: "Flip horizontal", effect: Effect::Cmd(|_| json!({ "op": "image.flip", "horizontal": true })) },
    Rule { phrases: &["sharpen", "sharper", "crisper", "crisp", "more detail", "less blurry", "unblur"], label: "Sharpen", effect: Effect::Cmd(|k| json!({ "op": "filter.unsharp-mask", "amount": (80.0 * k).round(), "radius": 1.5, "threshold": 2 })) },
    Rule { phrases: &["blur it", "blur the image", "blur the photo", "make it blurry", "gaussian blur", "blur", "soften the image"], label: "Blur", effect: Effect::Cmd(|k| json!({ "op": "filter.gaussian-blur", "radius": (4.0 * k).round().max(1.0) })) },
    Rule { phrases: &["invert", "negative"], label: "Invert", effect: Effect::Cmd(|_| json!({ "op": "layer.add-adjustment", "adjustment": { "kind": "invert" }, "name": "Invert" })) },
    Rule { phrases: &["sepia"], label: "Sepia", effect: Effect::Cmd(|_| json!({ "op": "layer.add-adjustment", "adjustment": { "kind": "black-white", "tint": true, "tint_color": { "r": 172, "g": 122, "b": 51, "a": 255 } }, "name": "Sepia" })) },
    Rule { phrases: &["undo that", "undo", "go back"], label: "Undo", effect: Effect::Cmd(|_| json!({ "op": "edit.undo" })) },
    // ---- Looks (Develop presets)
    Rule { phrases: &["make it pop", "pop", "punchy", "punchier", "more impact"], label: "Make it pop", effect: Effect::Develop(&[("contrast", 20.0), ("vibrance", 25.0), ("clarity", 15.0)]) },
    Rule { phrases: &["vintage", "retro", "film look", "old film", "analog look", "analogue look", "faded film"], label: "Vintage look", effect: Effect::Develop(&[("contrast", -10.0), ("saturation", -25.0), ("temperature", 18.0), ("blacks", 20.0), ("vignette", -25.0), ("grain", 20.0)]) },
    Rule { phrases: &["dramatic", "moody", "dark and moody", "gritty"], label: "Dramatic look", effect: Effect::Develop(&[("exposure", -0.2), ("contrast", 35.0), ("clarity", 30.0), ("vibrance", -10.0), ("vignette", -30.0)]) },
    Rule { phrases: &["cinematic", "movie look", "film still"], label: "Cinematic look", effect: Effect::Develop(&[("contrast", 18.0), ("saturation", -12.0), ("temperature", -6.0), ("tint", 4.0), ("blacks", 10.0), ("vignette", -20.0)]) },
    Rule { phrases: &["dreamy", "soft focus", "ethereal", "soft look", "softer look"], label: "Dreamy look", effect: Effect::Develop(&[("clarity", -35.0), ("contrast", -10.0), ("dehaze", -12.0), ("exposure", 0.1)]) },
    Rule { phrases: &["matte look", "matte", "faded"], label: "Matte look", effect: Effect::Develop(&[("blacks", 30.0), ("contrast", -15.0), ("saturation", -8.0)]) },
    Rule { phrases: &["golden hour", "sunset look", "sunset glow", "warm glow"], label: "Golden hour", effect: Effect::Develop(&[("temperature", 40.0), ("tint", 8.0), ("vibrance", 15.0), ("exposure", 0.1)]) },
    Rule { phrases: &["black and white", "black & white", "b&w", "b & w", "monochrome", "grayscale", "greyscale", "no color", "no colour", "desaturate completely"], label: "Black and white", effect: Effect::Develop(&[("black_white", 100.0)]) },
    // ---- Develop tweaks
    Rule { phrases: &["remove the haze", "remove haze", "dehaze", "less hazy", "less haze", "cut through the haze", "clear up the haze", "foggy", "hazy"], label: "Dehaze", effect: Effect::Develop(&[("dehaze", 30.0)]) },
    Rule { phrases: &["add haze", "more haze", "hazier"], label: "Add haze", effect: Effect::Develop(&[("dehaze", -25.0)]) },
    Rule { phrases: &["remove vignette", "no vignette", "less vignette", "brighten the corners", "brighten corners"], label: "Lighten edges", effect: Effect::Develop(&[("vignette", 25.0)]) },
    Rule { phrases: &["vignette", "darken the edges", "darken edges", "darker edges", "darken the corners", "darker corners"], label: "Vignette", effect: Effect::Develop(&[("vignette", -30.0)]) },
    Rule { phrases: &["film grain", "add grain", "grain", "add noise"], label: "Grain", effect: Effect::Develop(&[("grain", 25.0)]) },
    Rule { phrases: &["less texture", "smooth", "smoother", "soften"], label: "Less clarity", effect: Effect::Develop(&[("clarity", -25.0)]) },
    Rule { phrases: &["more texture", "clarity", "more clarity", "texture", "structure"], label: "More clarity", effect: Effect::Develop(&[("clarity", 30.0)]) },
    Rule { phrases: &["recover highlights", "tone down the highlights", "tone down highlights", "less highlights", "bring down the highlights", "bring down highlights", "darker highlights", "blown out", "overexposed sky", "too bright sky"], label: "Recover highlights", effect: Effect::Develop(&[("highlights", -45.0), ("whites", -15.0)]) },
    Rule { phrases: &["brighter highlights", "brighten highlights", "boost highlights"], label: "Brighter highlights", effect: Effect::Develop(&[("highlights", 30.0)]) },
    Rule { phrases: &["lift the shadows", "lift shadows", "brighten the shadows", "brighten shadows", "open up the shadows", "open up shadows", "more shadow detail", "shadows brighter", "brighter shadows", "too dark in the shadows"], label: "Lift shadows", effect: Effect::Develop(&[("shadows", 45.0)]) },
    Rule { phrases: &["deepen the shadows", "deeper shadows", "darker shadows", "darken shadows", "darken the shadows", "crush the blacks", "crush blacks", "richer blacks", "deeper blacks"], label: "Deeper shadows", effect: Effect::Develop(&[("shadows", -30.0), ("blacks", -20.0)]) },
    Rule { phrases: &["less contrast", "lower contrast", "reduce contrast", "reduce the contrast", "flatter", "too contrasty", "less contrasty"], label: "Less contrast", effect: Effect::Develop(&[("contrast", -25.0)]) },
    Rule { phrases: &["more contrast", "add contrast", "increase contrast", "increase the contrast", "boost contrast", "higher contrast", "contrasty", "contrast"], label: "More contrast", effect: Effect::Develop(&[("contrast", 25.0)]) },
    Rule { phrases: &["less saturated", "less saturation", "desaturate", "muted", "less colorful", "less colourful", "tone down the colors", "tone down the colours", "washed out", "too colorful", "too colourful", "less vibrant"], label: "Muted colors", effect: Effect::Develop(&[("saturation", -30.0)]) },
    Rule { phrases: &["more vibrant", "vibrant", "vibrance", "more vibrance", "more colorful", "more colourful", "colorful", "colourful", "boost the colors", "boost the colours", "boost colors", "boost colours", "more saturated", "more saturation", "saturate", "saturated", "vivid", "richer colors", "richer colours"], label: "More vibrant", effect: Effect::Develop(&[("vibrance", 30.0), ("saturation", 8.0)]) },
    Rule { phrases: &["warmer", "warm it up", "warm up", "warm", "more yellow", "more orange", "less blue", "too cold", "too blue"], label: "Warmer", effect: Effect::Develop(&[("temperature", 25.0)]) },
    Rule { phrases: &["cooler", "colder", "cool it down", "cool down", "cool", "more blue", "less yellow", "less orange", "too warm", "too yellow", "too orange"], label: "Cooler", effect: Effect::Develop(&[("temperature", -25.0)]) },
    Rule { phrases: &["more magenta", "less green", "too green"], label: "More magenta", effect: Effect::Develop(&[("tint", 20.0)]) },
    Rule { phrases: &["more green", "less magenta", "too magenta", "too pink", "less pink"], label: "More green", effect: Effect::Develop(&[("tint", -20.0)]) },
    Rule { phrases: &["underexpose", "darker", "darken", "dimmer", "dim", "less bright", "too bright", "less exposure", "reduce exposure", "lower exposure", "overexposed"], label: "Darker", effect: Effect::Develop(&[("exposure", -0.35)]) },
    Rule { phrases: &["brighter", "brighten", "lighter", "lighten", "more light", "more exposure", "increase exposure", "increase the exposure", "raise exposure", "too dark", "underexposed", "bright", "light it up", "expose"], label: "Brighter", effect: Effect::Develop(&[("exposure", 0.35)]) },
];

fn intensity(clause: &str) -> f32 {
    let has = |w: &[&str]| w.iter().any(|p| clause.contains(p));
    if has(&["a tiny bit", "a touch", "tiny bit", "a hair", "very slightly"]) {
        0.3
    } else if has(&["a bit", "slightly", "a little", "somewhat", "a tad", "subtle", "gently", "just a"]) {
        0.5
    } else if has(&["way ", "extremely", "much more", "a lot", "lots", "very ", "really ", "much ", "strongly", "heavily", "significantly", "super "]) {
        2.0
    } else {
        1.0
    }
}

fn range(field: &str) -> (f32, f32) {
    match field {
        "exposure" => (-5.0, 5.0),
        "grain" | "black_white" => (0.0, 100.0),
        _ => (-100.0, 100.0),
    }
}

fn normalise(text: &str) -> String {
    let lower = text.to_lowercase().replace(['’', '\''], "");
    let spaced: String = lower.chars().map(|c| if c.is_alphanumeric() || " &:-.|".contains(c) { c } else { ' ' }).collect();
    let mut out = String::new();
    for word in spaced.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word.trim_end_matches('.'));
    }
    out
}

fn clauses(text: &str) -> Vec<String> {
    let mut s = format!(" {} ", normalise(&text.replace([',', ';', '\n'], " | ")));
    // Phrases that contain a clause separator survive the split.
    for (from, to) in [(" black and white ", " b&w "), (" dark and moody ", " moody "), (" black & white ", " b&w "), (" black-and-white ", " b&w ")] {
        s = s.replace(from, to);
    }
    for sep in [" and then ", " then ", " and also ", " also ", " and ", " plus ", " as well as ", " but ", " while ", " with "] {
        s = s.replace(sep, " | ");
    }
    s.split('|').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

/// Whole-word-boundary containment, so "cool" does not match "school".
fn contains_phrase(clause: &str, phrase: &str) -> Option<usize> {
    let hay = format!(" {clause} ");
    let needle = phrase.trim();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let start = from + pos;
        let end = start + needle.len();
        let before_ok = !hay[..start].chars().last().is_some_and(|c| c.is_alphanumeric());
        // A phrase ending in a space ("add a ") must match that space.
        let after_ok = phrase.ends_with(' ') || !hay[end..].chars().next().is_some_and(|c| c.is_alphanumeric());
        if before_ok && after_ok {
            return Some(needle.len());
        }
        from = start + 1;
    }
    None
}

pub fn plan(text: &str) -> Plan {
    let mut develop: Map<String, Value> = Map::new();
    let mut develop_labels: Vec<String> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    let mut ignored = Vec::new();
    let mut generative = false;

    for clause in clauses(text) {
        let k = intensity(&clause);
        let negated = clause.starts_with("dont ") || clause.starts_with("do not ") || clause.starts_with("no need");
        if negated {
            continue;
        }
        let before = steps.len() + develop_labels.len();
        let mut matched = false;
        let mut clause_generative = false;
        let mut remaining = clause.clone();
        // Several rules can match one clause ("brighter warmer"): keep
        // taking the longest phrase and removing it.
        loop {
            let mut best: Option<(usize, &Rule, &str)> = None;
            for rule in RULES {
                for p in rule.phrases {
                    if let Some(len) = contains_phrase(&remaining, p) {
                        if best.is_none_or(|b| len > b.0) {
                            best = Some((len, rule, p));
                        }
                    }
                }
            }
            let Some((_, rule, phrase)) = best else { break };
            matched = true;
            remaining = remaining.replacen(phrase.trim(), " ", 1);
            match &rule.effect {
                Effect::Generative => clause_generative = true,
                Effect::Develop(fields) => {
                    for (field, amount) in fields.iter() {
                        let cur = develop.get(*field).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                        let (lo, hi) = range(field);
                        let v = if *field == "black_white" { *amount } else { cur + amount * k };
                        let v = (v.clamp(lo, hi) as f64 * 100.0).round() / 100.0;
                        develop.insert(field.to_string(), json!(v));
                    }
                    let label = match k {
                        k if k < 0.4 => format!("{} (a touch)", rule.label),
                        k if k < 0.9 => format!("{} (a bit)", rule.label),
                        k if k > 1.5 => format!("{} (a lot)", rule.label),
                        _ => rule.label.to_string(),
                    };
                    if !develop_labels.contains(&label) {
                        develop_labels.push(label);
                    }
                }
                Effect::Cmd(build) => steps.push(Step { label: rule.label.into(), cmd: Some(build(k)), ..Default::default() }),
                Effect::Action(name, params) => {
                    if !steps.iter().any(|s| s.action.as_deref() == Some(name)) {
                        steps.push(Step { label: rule.label.into(), action: Some(name.to_string()), params: Some(params(&clause)), ..Default::default() });
                    }
                }
            }
            if remaining.split_whitespace().count() == 0 {
                break;
            }
        }
        // "add a vignette" is a vignette, not a generative request: the
        // generative phrases only count when nothing concrete matched.
        let concrete = matched && (steps.len() + develop_labels.len()) > before;
        if clause_generative && !concrete {
            generative = true;
        }
        let leftover: Vec<&str> = remaining.split_whitespace().filter(|w| !FILLER.contains(w)).collect();
        if !matched && !leftover.is_empty() {
            ignored.push(clause.clone());
        }
    }

    if !develop.is_empty() {
        let mut adj = develop.clone();
        adj.insert("kind".into(), json!("develop"));
        let label = develop_labels.join(", ");
        // Develop runs first: tone before geometry and AI actions, so an
        // upscale or cutout sees the look the user asked for.
        steps.insert(
            0,
            Step { label: label.clone(), cmd: Some(json!({ "op": "layer.add-adjustment", "adjustment": Value::Object(adj), "name": label })), ..Default::default() },
        );
    }

    if steps.is_empty() {
        let unsupported = if generative {
            "That needs a generative model, which does not run in the browser. Try removing, selecting or adjusting instead.".to_string()
        } else if text.trim().is_empty() {
            "Describe an edit, like \u{201c}brighter and a bit warmer\u{201d} or \u{201c}remove the background\u{201d}.".to_string()
        } else {
            format!(
                "I could not turn \u{201c}{}\u{201d} into an edit. Try phrases like \u{201c}brighter\u{201d}, \u{201c}more contrast\u{201d}, \u{201c}black and white\u{201d} or \u{201c}blur the background\u{201d}.",
                text.trim()
            )
        };
        return Plan { steps, unsupported: Some(unsupported), ignored };
    }
    if generative {
        ignored.push("generative request (needs a generative model)".into());
    }
    Plan { steps, unsupported: None, ignored }
}

const FILLER: &[&str] = &[
    "please", "can", "you", "could", "would", "make", "it", "the", "a", "an", "this", "photo", "picture", "image", "pic", "me", "my", "just", "bit", "little", "slightly", "more", "much", "very", "really", "lot", "of", "to", "and", "some", "i", "want", "like", "look", "looks", "bit", "tad", "touch", "somewhat", "way", "overall", "too", "so", "that", "is", "be", "want", "need", "it.", "in", "on", "for", "up", "down",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn develop_of(p: &Plan) -> Map<String, Value> {
        p.steps[0].cmd.as_ref().unwrap()["adjustment"].as_object().unwrap().clone()
    }

    /// The phrase table: (sentence, expected develop fields with sign, expected actions/ops).
    #[test]
    fn phrase_table() {
        type Case = (&'static str, &'static [(&'static str, f32)], &'static [&'static str]);
        let cases: &[Case] = &[
            ("brighter", &[("exposure", 0.35)], &[]),
            ("make it a bit brighter", &[("exposure", 0.175)], &[]),
            ("much brighter", &[("exposure", 0.7)], &[]),
            ("darker", &[("exposure", -0.35)], &[]),
            ("a bit warmer", &[("temperature", 12.5)], &[]),
            ("cooler please", &[("temperature", -25.0)], &[]),
            ("more contrast", &[("contrast", 25.0)], &[]),
            ("less contrast", &[("contrast", -25.0)], &[]),
            ("black and white", &[("black_white", 100.0)], &[]),
            ("make it monochrome", &[("black_white", 100.0)], &[]),
            ("more vibrant", &[("vibrance", 30.0)], &[]),
            ("less saturated", &[("saturation", -30.0)], &[]),
            ("lift the shadows", &[("shadows", 45.0)], &[]),
            ("recover highlights", &[("highlights", -45.0)], &[]),
            ("remove the haze", &[("dehaze", 30.0)], &[]),
            ("add a vignette", &[("vignette", -30.0)], &[]),
            ("add film grain", &[("grain", 25.0)], &[]),
            ("make it pop", &[("contrast", 20.0), ("vibrance", 25.0)], &[]),
            ("vintage look", &[("saturation", -25.0), ("grain", 20.0)], &[]),
            ("dramatic", &[("contrast", 35.0)], &[]),
            ("golden hour", &[("temperature", 40.0)], &[]),
            ("brighter and warmer", &[("exposure", 0.35), ("temperature", 25.0)], &[]),
            ("brighter, a bit warmer and more contrast", &[("exposure", 0.35), ("temperature", 12.5), ("contrast", 25.0)], &[]),
            ("remove the background", &[], &["removeBackground"]),
            ("blur the background", &[], &["blurBackground"]),
            ("portrait mode", &[], &["blurBackground"]),
            ("select the subject", &[], &["selectSubject"]),
            ("select background", &[], &["selectBackground"]),
            ("remove the selected object", &[], &["removeSelected"]),
            ("upscale 2x", &[], &["upscale"]),
            ("upscale it 4x", &[], &["upscale"]),
            ("straighten", &[], &["straighten"]),
            ("level the horizon", &[], &["straighten"]),
            ("crop square", &[], &["crop"]),
            ("crop to 16:9", &[], &["crop"]),
            ("sharpen", &[], &["filter.unsharp-mask"]),
            ("remove red eye", &[], &["removeRedEye"]),
            ("reduce noise", &[], &["denoise"]),
            ("fix the jpeg artifacts", &[], &["removeJpegArtifacts"]),
            ("restore faces", &[], &["restoreFaces"]),
            ("colorize this old photo", &[], &["colorize"]),
            ("rotate left", &[], &["image.rotate"]),
            ("flip horizontally", &[], &["image.flip"]),
            ("auto enhance", &[], &["auto"]),
            ("sepia", &[], &["layer.add-adjustment"]),
            ("invert", &[], &["layer.add-adjustment"]),
            ("warmer and blur the background", &[("temperature", 25.0)], &["blurBackground"]),
            ("remove the background then upscale 2x", &[], &["removeBackground", "upscale"]),
            ("too dark", &[("exposure", 0.35)], &[]),
            ("too blue", &[("temperature", 25.0)], &[]),
            ("less green", &[("tint", 20.0)], &[]),
            ("deeper blacks", &[("blacks", -20.0)], &[]),
            ("dreamy", &[("clarity", -35.0)], &[]),
            ("cinematic", &[("contrast", 18.0)], &[]),
            ("matte look", &[("blacks", 30.0)], &[]),
            ("more clarity", &[("clarity", 30.0)], &[]),
            ("a bit brighter highlights", &[("highlights", 15.0)], &[]),
            ("brighten the corners", &[("vignette", 25.0)], &[]),
            ("very slightly cooler", &[("temperature", -7.5)], &[]),
            ("enlarge", &[], &["upscale"]),
        ];
        for (text, fields, ops) in cases {
            let p = plan(text);
            assert!(p.unsupported.is_none(), "{text}: unsupported {:?}", p.unsupported);
            let mut idx = 0;
            if !fields.is_empty() {
                let d = develop_of(&p);
                for (f, v) in fields.iter() {
                    let got = d.get(*f).and_then(Value::as_f64).unwrap_or(f64::NAN) as f32;
                    assert!((got - v).abs() < 0.02, "{text}: {f} = {got}, want {v} ({d:?})");
                }
                idx = 1;
            }
            let names: Vec<String> = p.steps[idx..]
                .iter()
                .map(|s| s.action.clone().unwrap_or_else(|| s.cmd.as_ref().unwrap()["op"].as_str().unwrap().to_string()))
                .collect();
            assert_eq!(names, ops.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "{text}: {p:?}");
        }
        assert!(cases.len() >= 60);
    }

    #[test]
    fn word_boundaries_hold() {
        // "cool" inside "school" and "warm" inside "swarm" must not match.
        let p = plan("school swarm");
        assert!(p.unsupported.is_some(), "{p:?}");
    }

    #[test]
    fn unknown_and_generative_text_is_unsupported_with_a_message() {
        let p = plan("make the dog talk");
        assert!(p.steps.is_empty());
        assert!(p.unsupported.as_ref().unwrap().contains("Try phrases"));
        let g = plan("replace the sky with a sunset");
        assert!(g.unsupported.as_ref().unwrap().contains("generative"), "{g:?}");
        let partial = plan("brighter and make the dog talk");
        assert_eq!(partial.steps.len(), 1);
        assert_eq!(partial.ignored, vec!["make the dog talk".to_string()]);
    }

    /// Every adjustment the planner emits must deserialise as the engine's
    /// own `Adjustment` type, so a plan never fails at execution.
    #[test]
    fn adjustments_deserialise_as_engine_types() {
        for text in ["brighter warmer more contrast vignette grain black and white", "sepia", "invert", "vintage", "dramatic"] {
            for s in plan(text).steps {
                if let Some(cmd) = s.cmd {
                    if let Some(adj) = cmd.get("adjustment") {
                        serde_json::from_value::<editor_core::adjust::Adjustment>(adj.clone()).unwrap_or_else(|e| panic!("{text}: {e} in {adj}"));
                    }
                }
            }
        }
    }
}
