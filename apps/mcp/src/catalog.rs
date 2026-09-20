//! The command catalogue: every `{op, params}` the engine accepts.
//!
//! This is what makes `run_operations` usable. An agent that can list the
//! operations does not have to guess at names, and the guessing is the part
//! that fails silently — `filter.gaussian_blur` is not a typo the engine can
//! correct, it is an unknown op.
//!
//! The list is written out here rather than derived at run time because the
//! domain crates parse their commands with `#[serde(tag = "op")]` enums,
//! which produce no name list to read. What keeps it honest is
//! [`tests::the_catalogue_lists_every_op_the_engine_registers`]: it reads the
//! op names straight out of the engine's own sources and fails when one is
//! missing here. A new filter lands with a failing test, not with a
//! catalogue that has quietly gone stale.
//!
//! `params` is a signature, not a schema: the authority is `docs/commands.md`
//! and the Rust types it points at.

pub struct OpDoc {
    pub op: &'static str,
    pub params: &'static str,
    pub summary: &'static str,
}

impl OpDoc {
    pub fn domain(&self) -> &'static str {
        match self.op.split_once('.') {
            Some((d, _)) => d,
            None => self.op,
        }
    }
}

/// Every domain, in the order `list_operations` reports them.
pub const DOMAINS: &[(&str, &str)] = &[
    ("edit", "undo, redo and transactions"),
    ("doc", "creating and opening documents"),
    ("layer", "the layer tree: pixels, adjustments, masks, groups, effects, smart objects"),
    ("image", "whole-canvas geometry: crop, resize, rotate, flip"),
    ("select", "selections: shapes, magic wand, colour range, refine edge"),
    ("filter", "destructive pixel filters: blur, sharpen, noise, distort, retouch"),
    ("analyze", "read-only analysis: histogram, auto settings, image facts, colour pick"),
    ("paint", "the brush engine, bucket, gradient"),
    ("transform", "free transform, warp, liquify, lens correction, content-aware scale"),
];

#[rustfmt::skip]
pub const CATALOG: &[OpDoc] = &[
    // --- edit ------------------------------------------------------------
    OpDoc { op: "edit.undo", params: "—", summary: "Step back one history entry." },
    OpDoc { op: "edit.redo", params: "—", summary: "Step forward one history entry." },
    OpDoc { op: "edit.history-go", params: "index", summary: "Jump so that `index` entries remain on the undo stack." },
    OpDoc { op: "edit.seal", params: "—", summary: "Close the current merge group (the end of a slider drag)." },
    OpDoc { op: "edit.begin", params: "label?", summary: "Open a transaction: everything until edit.end becomes one undo step." },
    OpDoc { op: "edit.end", params: "—", summary: "Close the transaction opened by edit.begin and record it as one step." },
    OpDoc { op: "edit.cancel", params: "—", summary: "Abandon the open transaction and restore the document to how it was." },
    OpDoc { op: "edit.clear-history", params: "—", summary: "Drop the undo and redo stacks, keeping the document." },

    // --- doc -------------------------------------------------------------
    OpDoc { op: "doc.new", params: "width, height, background?: Rgba8, resolution?", summary: "Replace the document with an empty one of this size." },
    OpDoc { op: "doc.open-pixels", params: "width, height, name? (+ RGBA bytes)", summary: "Replace the document with one holding these pixels. How a file becomes a document." },
    OpDoc { op: "doc.set-resolution", params: "resolution", summary: "Set pixels per inch; pixel data is untouched." },

    // --- layer -----------------------------------------------------------
    OpDoc { op: "layer.add-pixel", params: "name?, above? → data.id", summary: "Add an empty pixel layer." },
    OpDoc { op: "layer.import", params: "width, height, x?, y?, name?, above?, provenance? (+ RGBA bytes) → data.id", summary: "Add a pixel layer from supplied pixels." },
    OpDoc { op: "layer.set-pixels", params: "id?, x, y, width, height, respect_selection?, label?, provenance? (+ RGBA bytes)", summary: "Write a rectangle of pixels into a layer." },
    OpDoc { op: "layer.add-adjustment", params: "adjustment, name?, above? → data.id", summary: "Add a non-destructive adjustment layer (masked by the selection, if any)." },
    OpDoc { op: "layer.set-adjustment", params: "id, adjustment", summary: "Change an adjustment layer's settings; fields merge." },
    OpDoc { op: "layer.add-fill", params: "fill → data.id", summary: "Add a solid, gradient or pattern fill layer." },
    OpDoc { op: "layer.set-fill", params: "id, fill", summary: "Change a fill layer." },
    OpDoc { op: "layer.add-text", params: "data: TextData, width, height, x, y (+ RGBA of the rendered text)", summary: "Add a text layer. The caller rasterises the glyphs." },
    OpDoc { op: "layer.set-text", params: "id, data: TextData, width, height, x, y (+ RGBA)", summary: "Replace a text layer's content and raster." },
    OpDoc { op: "layer.add-shape", params: "data: ShapeData, name? → data.id", summary: "Add a vector shape layer (rasterised by the engine)." },
    OpDoc { op: "layer.set-shape", params: "id, data: ShapeData", summary: "Change a shape layer." },
    OpDoc { op: "layer.group", params: "ids, name? → data.id", summary: "Put layers into a new group." },
    OpDoc { op: "layer.ungroup", params: "id", summary: "Dissolve a group, keeping its children in place." },
    OpDoc { op: "layer.delete", params: "ids", summary: "Delete layers." },
    OpDoc { op: "layer.duplicate", params: "ids", summary: "Duplicate layers above themselves." },
    OpDoc { op: "layer.move", params: "id, parent?, index", summary: "Move a layer to a position in the tree." },
    OpDoc { op: "layer.reorder", params: "id, direction: up|down|top|bottom", summary: "Move a layer within its parent." },
    OpDoc { op: "layer.props", params: "id, name?, visible?, opacity?, fill_opacity?, blend?, clip?, locks?, color_label?, pass_through?, expanded?", summary: "Set layer properties." },
    OpDoc { op: "layer.offset", params: "ids, dx, dy", summary: "Move layers on the canvas (merges while the same ids move)." },
    OpDoc { op: "layer.set-active", params: "id", summary: "Choose the layer that id-less commands act on. Not recorded." },
    OpDoc { op: "layer.merge-down", params: "id", summary: "Merge a layer into the one below." },
    OpDoc { op: "layer.merge-visible", params: "—", summary: "Merge all visible layers into one." },
    OpDoc { op: "layer.flatten", params: "—", summary: "Flatten the document to a single layer." },
    OpDoc { op: "layer.stamp-visible", params: "—", summary: "Add a new layer holding the composite of the visible ones." },
    OpDoc { op: "layer.rasterize", params: "id", summary: "Turn a text, shape, smart or adjustment layer into pixels." },
    OpDoc { op: "layer.set-effects", params: "id, effects: LayerEffects | null", summary: "Set the layer style (shadows, glows, bevel, overlays, stroke); fields merge." },
    OpDoc { op: "layer.add-mask", params: "id?, from: reveal-all|hide-all|selection|hide-selection", summary: "Add a layer mask." },
    OpDoc { op: "layer.delete-mask", params: "id, apply?", summary: "Remove a mask, optionally applying it to the pixels first." },
    OpDoc { op: "layer.mask-props", params: "id, enabled?, linked?, density?", summary: "Set mask properties." },
    OpDoc { op: "layer.set-mask-pixels", params: "id, x, y, width, height (+ single-channel bytes)", summary: "Write a rectangle into a layer mask." },
    OpDoc { op: "layer.place-smart", params: "width, height, name?, above? (+ RGBA bytes) → data.id", summary: "Place pixels as a smart object." },
    OpDoc { op: "layer.convert-to-smart", params: "ids", summary: "Wrap layers in a smart object." },
    OpDoc { op: "layer.smart-transform", params: "id, quad, resample?", summary: "Re-place a smart object's corners, resampling from the original." },
    OpDoc { op: "layer.smart-filter-add", params: "id, filter", summary: "Add a smart (re-editable) filter to a smart object." },
    OpDoc { op: "layer.smart-filter-set", params: "id, index, filter?, enabled?, opacity?, blend?", summary: "Change a smart filter." },
    OpDoc { op: "layer.smart-filter-remove", params: "id, index", summary: "Remove a smart filter." },
    OpDoc { op: "layer.smart-filter-move", params: "id, from, to", summary: "Reorder a smart object's filters." },
    OpDoc { op: "layer.smart-replace", params: "id, width, height (+ RGBA bytes)", summary: "Replace a smart object's source contents." },
    OpDoc { op: "layer.smart-unpack", params: "id", summary: "Unpack a smart object whose source is a document into ordinary layers." },
    OpDoc { op: "layer.from-selection", params: "id?, cut? → data.id", summary: "Copy (or cut) the selected pixels to a new layer." },
    OpDoc { op: "layer.clear", params: "id?", summary: "Clear the selected pixels, or the whole layer." },
    OpDoc { op: "layer.fill-selection", params: "id?, color, opacity?", summary: "Fill the selection with a colour." },

    // --- image -----------------------------------------------------------
    OpDoc { op: "image.crop", params: "x, y, width, height, delete_cropped? = true", summary: "Crop the document." },
    OpDoc { op: "image.resize", params: "width, height, resample?: nearest|bilinear|bicubic|lanczos", summary: "Resample the whole document to a new pixel size." },
    OpDoc { op: "image.canvas-size", params: "width, height, anchor?: top-left…bottom-right, center", summary: "Change the canvas without scaling the pixels." },
    OpDoc { op: "image.rotate", params: "turns", summary: "Rotate the document by whole clockwise quarter turns." },
    OpDoc { op: "image.flip", params: "horizontal", summary: "Mirror the whole document." },
    OpDoc { op: "image.rotate-arbitrary", params: "degrees, expand?", summary: "Rotate by any angle; expand: false straightens within the same canvas." },
    OpDoc { op: "image.trim", params: "—", summary: "Crop away uniform or transparent borders." },
    OpDoc { op: "image.reveal-all", params: "—", summary: "Grow the canvas until every layer fits." },

    // --- select ----------------------------------------------------------
    OpDoc { op: "select.all", params: "—", summary: "Select the whole canvas." },
    OpDoc { op: "select.none", params: "—", summary: "Deselect." },
    OpDoc { op: "select.invert", params: "—", summary: "Invert the selection." },
    OpDoc { op: "select.rect", params: "x, y, width, height, mode?, feather?", summary: "Rectangular marquee." },
    OpDoc { op: "select.ellipse", params: "x, y, width, height, mode?, feather?, anti_alias?", summary: "Elliptical marquee." },
    OpDoc { op: "select.polygon", params: "points, mode?, feather?, anti_alias?", summary: "Polygonal lasso." },
    OpDoc { op: "select.mask", params: "x, y, width, height, mode?, feather?, label? (+ single-channel bytes)", summary: "Set the selection from a supplied coverage mask." },
    OpDoc { op: "select.layer-alpha", params: "id, mode?", summary: "Select a layer's opaque pixels." },
    OpDoc { op: "select.feather", params: "radius", summary: "Soften the selection edge." },
    OpDoc { op: "select.magic-wand", params: "x, y, tolerance, contiguous?, sample_all?, anti_alias?, mode?", summary: "Select pixels similar to the one clicked." },
    OpDoc { op: "select.color-range", params: "color?: Rgba8, preset?: highlights|midtones|shadows|skin, fuzziness, mode?", summary: "Select by colour across the image." },
    OpDoc { op: "select.similar", params: "tolerance", summary: "Extend the selection to similar colours anywhere in the image." },
    OpDoc { op: "select.grow", params: "by", summary: "Grow the selection by a number of pixels." },
    OpDoc { op: "select.shrink", params: "by", summary: "Shrink the selection by a number of pixels." },
    OpDoc { op: "select.border", params: "width", summary: "Replace the selection with a band along its edge." },
    OpDoc { op: "select.smooth", params: "radius", summary: "Round off the corners of the selection." },
    OpDoc { op: "select.quick", params: "points, radius, mode: add|subtract", summary: "Edge-aware quick-selection brush." },
    OpDoc { op: "select.refine", params: "radius, smooth, feather, contrast, shift_edge, decontaminate?", summary: "Refine Edge: re-cut the selection against the image (hair, fur)." },
    OpDoc { op: "select.transform", params: "matrix: {a,b,c,d,e,f}", summary: "Transform the selection itself, not the pixels." },

    // --- filter ----------------------------------------------------------
    OpDoc { op: "filter.gaussian-blur", params: "id?, radius", summary: "Gaussian blur." },
    OpDoc { op: "filter.box-blur", params: "id?, radius", summary: "Box blur." },
    OpDoc { op: "filter.motion-blur", params: "id?, angle, distance", summary: "Directional motion blur." },
    OpDoc { op: "filter.radial-blur", params: "id?, amount, mode: spin|zoom, cx?, cy?", summary: "Spin or zoom blur about a centre." },
    OpDoc { op: "filter.surface-blur", params: "id?, radius, threshold", summary: "Blur flat areas while keeping edges." },
    OpDoc { op: "filter.lens-blur", params: "id?, radius, focal? (+ optional depth map)", summary: "Aperture-shaped defocus, optionally driven by a depth map." },
    OpDoc { op: "filter.tilt-shift", params: "id?, center_y, band, feather, radius", summary: "Blur everything outside a horizontal band." },
    OpDoc { op: "filter.unsharp-mask", params: "id?, amount, radius, threshold", summary: "Unsharp mask." },
    OpDoc { op: "filter.smart-sharpen", params: "id?, amount, radius, reduce_noise", summary: "Sharpen with noise held back." },
    OpDoc { op: "filter.high-pass", params: "id?, radius", summary: "Keep only detail finer than the radius." },
    OpDoc { op: "filter.add-noise", params: "id?, amount, gaussian?, monochromatic?, seed?", summary: "Add noise." },
    OpDoc { op: "filter.reduce-noise", params: "id?, strength, preserve_details, reduce_color_noise", summary: "Reduce luminance and colour noise." },
    OpDoc { op: "filter.median", params: "id?, radius", summary: "Median filter." },
    OpDoc { op: "filter.dust-and-scratches", params: "id?, radius, threshold", summary: "Remove specks without softening everything." },
    OpDoc { op: "filter.minimum", params: "id?, radius", summary: "Erode (minimum) filter." },
    OpDoc { op: "filter.maximum", params: "id?, radius", summary: "Dilate (maximum) filter." },
    OpDoc { op: "filter.pixelate", params: "id?, cell", summary: "Mosaic. Irreversible: the source pixels are gone." },
    OpDoc { op: "filter.emboss", params: "id?, angle, height, amount", summary: "Emboss." },
    OpDoc { op: "filter.find-edges", params: "id?", summary: "Trace edges." },
    OpDoc { op: "filter.solarize", params: "id?", summary: "Solarise." },
    OpDoc { op: "filter.invert", params: "id?", summary: "Invert the pixels destructively." },
    OpDoc { op: "filter.desaturate", params: "id?", summary: "Desaturate the pixels destructively." },
    OpDoc { op: "filter.twirl", params: "id?, angle", summary: "Twirl distortion." },
    OpDoc { op: "filter.pinch", params: "id?, amount", summary: "Pinch or bulge." },
    OpDoc { op: "filter.spherize", params: "id?, amount", summary: "Wrap the image onto a sphere." },
    OpDoc { op: "filter.ripple", params: "id?, amount, size: small|medium|large", summary: "Ripple distortion." },
    OpDoc { op: "filter.polar-coordinates", params: "id?, to_polar", summary: "Convert between rectangular and polar coordinates." },
    OpDoc { op: "filter.offset", params: "id?, dx, dy, wrap", summary: "Shift the pixels, optionally wrapping." },
    OpDoc { op: "filter.clouds", params: "id?, seed?, fg?, bg?", summary: "Render clouds from the two colours." },
    OpDoc { op: "filter.custom", params: "id?, kernel: number[], scale, offset", summary: "Arbitrary odd-square convolution kernel." },
    OpDoc { op: "filter.apply-adjustment", params: "id?, adjustment", summary: "Apply an adjustment destructively (Image › Adjustments)." },
    OpDoc { op: "filter.vignette", params: "id?, amount, midpoint, feather", summary: "Darken or lighten the corners." },
    OpDoc { op: "filter.content-aware-fill", params: "id?, sample?", summary: "Fill the selection from surrounding texture." },
    OpDoc { op: "filter.spot-heal", params: "id?, x, y, radius | x, y, width, height (+ mask bytes)", summary: "Resynthesise a blemish from its surroundings." },
    OpDoc { op: "filter.red-eye", params: "id?, x, y, width, height, pupil_size?, darken?", summary: "Remove red eye inside a rectangle." },
    OpDoc { op: "filter.frequency-separation", params: "id?, radius → data.ids", summary: "Split a layer into low- and high-frequency layers for retouching." },

    // --- analyze ---------------------------------------------------------
    OpDoc { op: "analyze.histogram", params: "id?, merged?, rect? → {r,g,b,l: number[256]}", summary: "Histogram of the composite or one layer. Not recorded." },
    OpDoc { op: "analyze.auto", params: "style: auto|vivid|natural|bw → {develop}", summary: "Suggest develop settings for the image. Not recorded." },
    OpDoc { op: "analyze.auto-levels", params: "mode: tone|contrast|color → {levels}", summary: "Suggest levels for the image. Not recorded." },
    OpDoc { op: "analyze.facts", params: "→ {exposure, clipping_low, clipping_high, cast, noise_sigma, sharpness, tilt_degrees}", summary: "Measure the image: exposure, clipping, colour cast, noise, sharpness, tilt." },
    OpDoc { op: "analyze.pick", params: "x, y, size: 1|3|5, merged? → {color}", summary: "Sample a colour. Not recorded." },

    // --- paint -----------------------------------------------------------
    OpDoc { op: "paint.stroke", params: "id?, target: pixels|mask, tool: brush|pencil|eraser|dodge|burn|sponge|blur|sharpen|smudge|clone|heal, brush, points, stroke_id, smooth?", summary: "Paint a stroke. Segments sharing a stroke_id are one undo step." },
    OpDoc { op: "paint.stroke-end", params: "stroke_id", summary: "Release stroke state; healing finalises its blend here. Not recorded." },
    OpDoc { op: "paint.fill", params: "id?, x, y, tolerance, contiguous?, sample_all?, anti_alias?, color, opacity?", summary: "Paint bucket." },
    OpDoc { op: "paint.magic-erase", params: "id?, x, y, tolerance, contiguous?", summary: "Erase a contiguous colour region to transparency." },
    OpDoc { op: "paint.gradient", params: "id?, target: pixels|mask, from, to, gradient: linear|radial|angle|reflected|diamond, stops, opacity?, blend?, reverse?", summary: "Draw a gradient." },

    // --- transform -------------------------------------------------------
    OpDoc { op: "transform.layer", params: "ids, matrix?: {a,b,c,d,e,f}, quad?: [Point;4], resample?", summary: "Free transform: scale, rotate, skew or distort layers." },
    OpDoc { op: "transform.flip", params: "ids?, horizontal", summary: "Mirror layers about the centre of their combined bounds." },
    OpDoc { op: "transform.selection-pixels", params: "id?, matrix?, quad?, resample?", summary: "Lift the selected pixels, transform them and drop them back." },
    OpDoc { op: "transform.warp", params: "id, grid: Point[16], resample?", summary: "4×4 Bézier patch warp over the layer bounds." },
    OpDoc { op: "transform.liquify", params: "id, tool: forward|reconstruct|twirl-cw|twirl-ccw|pucker|bloat|push-left, size, pressure, density, points, stroke_id", summary: "Liquify displacement brushes." },
    OpDoc { op: "transform.liquify-end", params: "stroke_id", summary: "Free liquify state. Not recorded." },
    OpDoc { op: "transform.perspective-crop", params: "quad: [Point;4], width, height, resample?", summary: "Straighten a photographed rectangle and crop to it." },
    OpDoc { op: "transform.lens-correct", params: "id?, distortion, chromatic_rc, chromatic_by, vignette, vignette_midpoint, scale, auto_scale?", summary: "Correct barrel distortion, fringing and vignetting." },
    OpDoc { op: "transform.content-aware-scale", params: "width, height, protect_skin?", summary: "Seam-carve the document to a new size." },
];

pub fn lookup(op: &str) -> Option<&'static OpDoc> {
    CATALOG.iter().find(|d| d.op == op)
}

/// The catalogue, optionally narrowed to one domain.
pub fn filtered(domain: Option<&str>) -> Vec<&'static OpDoc> {
    match domain {
        None => CATALOG.iter().collect(),
        Some(d) => {
            let d = d.trim().trim_end_matches('.').to_ascii_lowercase();
            CATALOG.iter().filter(|o| o.domain() == d).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Files the engine declares its op names in, and how to read them.
    ///
    /// Every op name in the engine appears either as a serde rename on a
    /// command enum variant or as a match arm on the op string, so those two
    /// shapes are what this looks for. Anything from `#[cfg(test)]` onwards
    /// is ignored: the crates' own tests mention ops that do not exist
    /// (`select.bogus` is there to check the fall-through).
    const SOURCES: &[&str] = &[
        "crates/editor-core/src/editor.rs",
        "crates/editor-core/src/ops/layer.rs",
        "crates/editor-core/src/ops/image.rs",
        "crates/editor-core/src/ops/select.rs",
        "crates/editor-filters/src/lib.rs",
        "crates/editor-filters/src/analyze.rs",
        "crates/editor-paint/src/lib.rs",
        "crates/editor-select/src/lib.rs",
        "crates/editor-transform/src/lib.rs",
    ];

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the crate sits at apps/mcp in the repository")
    }

    /// Pull `"domain.op"` literals out of one source file.
    fn ops_declared_in(text: &str) -> BTreeSet<String> {
        let known: BTreeSet<&str> = DOMAINS.iter().map(|(d, _)| *d).collect();
        let mut found = BTreeSet::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("#[cfg(test)]") {
                break;
            }
            let is_rename = trimmed.starts_with("#[serde(rename = \"");
            let is_arm = trimmed.starts_with('"') && trimmed.contains("\" =>");
            if !is_rename && !is_arm {
                continue;
            }
            let Some(start) = line.find('"') else { continue };
            let rest = &line[start + 1..];
            let Some(end) = rest.find('"') else { continue };
            let literal = &rest[..end];
            let Some((domain, name)) = literal.split_once('.') else {
                continue;
            };
            if known.contains(domain) && !name.is_empty() {
                found.insert(literal.to_string());
            }
        }
        found
    }

    #[test]
    fn the_catalogue_lists_every_op_the_engine_registers() {
        let root = repo_root();
        let listed: BTreeSet<&str> = CATALOG.iter().map(|o| o.op).collect();
        let mut registered = BTreeSet::new();
        for rel in SOURCES {
            let path = root.join(rel);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            registered.extend(ops_declared_in(&text));
        }
        assert!(
            registered.len() > 100,
            "only found {} ops in the engine sources — the scan is broken, not the catalogue",
            registered.len()
        );
        let missing: Vec<&String> = registered
            .iter()
            .filter(|op| !listed.contains(op.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "the engine registers ops the catalogue does not list: {missing:?}\n\
             Add them to apps/mcp/src/catalog.rs (and docs/commands.md)."
        );
    }

    #[test]
    fn the_catalogue_invents_nothing() {
        // The other direction: an op listed here that the engine has never
        // heard of would send an agent down a hole. Running each one with no
        // parameters mostly fails — that is fine and expected — but it must
        // never fail with "unknown".
        let mut ed = crate::engine::new_editor();
        ed.exec(
            serde_json::json!({ "op": "doc.new", "width": 8, "height": 8 }),
            &[],
        )
        .unwrap();
        for entry in CATALOG {
            let out = ed.exec(serde_json::json!({ "op": entry.op }), &[]);
            if let Err(e) = out {
                let msg = e.to_string();
                assert!(
                    !msg.contains("unknown operation") && !msg.contains("unknown variant"),
                    "the catalogue lists {} but the engine does not know it: {msg}",
                    entry.op
                );
            }
        }
    }

    #[test]
    fn every_op_has_a_domain_the_list_knows() {
        let known: BTreeSet<&str> = DOMAINS.iter().map(|(d, _)| *d).collect();
        for entry in CATALOG {
            assert!(
                known.contains(entry.domain()),
                "{} is in no listed domain",
                entry.op
            );
            assert!(!entry.summary.is_empty(), "{} has no summary", entry.op);
        }
    }

    #[test]
    fn filtering_narrows_to_one_domain() {
        let filters = filtered(Some("filter"));
        assert!(filters.len() > 30);
        assert!(filters.iter().all(|o| o.domain() == "filter"));
        assert_eq!(filtered(Some("nonsense")).len(), 0);
        assert_eq!(filtered(None).len(), CATALOG.len());
    }

    #[test]
    fn no_op_is_listed_twice() {
        let unique: BTreeSet<&str> = CATALOG.iter().map(|o| o.op).collect();
        assert_eq!(unique.len(), CATALOG.len());
    }
}
