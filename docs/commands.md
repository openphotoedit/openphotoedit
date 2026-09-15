# Engine commands

Every edit is `editor.exec({ op, ...params }, bytes?)`. Coordinates are document pixels. Colours are `{r,g,b,a?}` 0..255. `id` is a layer id; where it is optional, the active layer is used. Pixel payloads (`bytes`) are straight RGBA8 row-major unless stated. Every command returns `{ changed, label, dirty, data, revision }` and is one undo step unless noted.

Status: **✓ implemented** · **◻ contract** (a backend workstream implements it; the UI may call it and must handle an error until it lands).

## History (core ✓)

| op | params | notes |
|---|---|---|
| `edit.undo` / `edit.redo` | — | not recorded |
| `edit.history-go` | `index` | jump so `index` entries remain on the undo stack |
| `edit.seal` | — | close the current merge group (end of a slider drag) |

## Document and layers (core ✓)

| op | params | bytes |
|---|---|---|
| `doc.new` | `width, height, background?: Rgba8, resolution?` | |
| `doc.open-pixels` | `width, height, name?` | RGBA |
| `doc.set-resolution` | `resolution` | |
| `layer.add-pixel` | `name?, above?` → `data.id` | |
| `layer.import` | `width, height, x?, y?, name?, above?, provenance?` → `data.id` | RGBA |
| `layer.set-pixels` | `id?, x, y, width, height, respect_selection?, label?, provenance?` | RGBA |
| `layer.add-adjustment` | `adjustment, name?, above?` → `data.id` (mask from selection if any) | |
| `layer.set-adjustment` | `id, adjustment` (merges per layer) | |
| `layer.add-fill` / `layer.set-fill` | `fill` / `id, fill` | |
| `layer.add-text` / `layer.set-text` | `data: TextData, width, height, x, y` (+`id` for set) | RGBA of the rendered text |
| `layer.add-shape` / `layer.set-shape` | `data: ShapeData, name?` (+`id`) | |
| `layer.group` | `ids, name?` → `data.id` | |
| `layer.ungroup` | `id` | |
| `layer.delete` / `layer.duplicate` | `ids` | |
| `layer.move` | `id, parent?, index` | |
| `layer.reorder` | `id, direction: up\|down\|top\|bottom` | |
| `layer.props` | `id, name?, visible?, opacity?, fill_opacity?, blend?, clip?, locks?, color_label?, pass_through?, expanded?` | |
| `layer.offset` | `ids, dx, dy` (merges while the same ids move) | |
| `layer.set-active` | `id` (not recorded) | |
| `layer.merge-down` | `id` | |
| `layer.merge-visible` / `layer.flatten` / `layer.stamp-visible` | — | |
| `layer.rasterize` | `id` | |
| `layer.set-effects` | `id, effects: LayerEffects \| null` (layer style; see `crates/editor-core/src/effects.rs` for fields: `drop_shadow, inner_shadow, outer_glow, inner_glow, bevel, satin, color_overlay, gradient_overlay, stroke`, each optional with `enabled`; merges per layer) | |
| `layer.add-mask` | `id?, from: reveal-all\|hide-all\|selection\|hide-selection` | |
| `layer.delete-mask` | `id, apply?` | |
| `layer.mask-props` | `id, enabled?, linked?, density?` | |
| `layer.set-mask-pixels` | `id, x, y, width, height` | single channel |
| `layer.from-selection` | `id?, cut?` → `data.id` | |
| `layer.clear` | `id?` | |
| `layer.fill-selection` | `id?, color, opacity?` | |

Adjustment kinds (`adjustment.kind`): `brightness-contrast, levels, curves, exposure, vibrance, hue-saturation, color-balance, black-white, photo-filter, channel-mixer, invert, posterize, threshold, gradient-map, selective-color, develop, color-lookup`. Fields: see `crates/editor-core/src/adjust.rs` (serde names are the Rust field names, snake_case).

## Image (core ✓)

| op | params |
|---|---|
| `image.crop` | `x, y, width, height, delete_cropped? = true` |
| `image.resize` | `width, height, resample?: nearest\|bilinear\|bicubic\|lanczos` |
| `image.canvas-size` | `width, height, anchor?: top-left…bottom-right, center` |
| `image.rotate` | `turns` (clockwise quarter turns) |
| `image.flip` | `horizontal` |
| `image.rotate-arbitrary` | `degrees, expand?` (false = straighten, canvas keeps size) |
| `image.trim` / `image.reveal-all` | — |

## Selection (core ✓ unless marked)

`mode`: `replace | add | subtract | intersect`.

| op | params | status |
|---|---|---|
| `select.all` / `select.none` / `select.invert` | — | ✓ |
| `select.rect` / `select.ellipse` | `x, y, width, height, mode?, feather?` (+`anti_alias?` ellipse) | ✓ |
| `select.polygon` | `points, mode?, feather?, anti_alias?` | ✓ |
| `select.mask` | `x, y, width, height, mode?, feather?, label?` + single-channel bytes | ✓ |
| `select.layer-alpha` | `id, mode?` | ✓ |
| `select.feather` | `radius` | ✓ |
| `select.magic-wand` | `x, y, tolerance (0..255), contiguous = true, sample_all = false, anti_alias = true, mode?` | ✓ |
| `select.color-range` | `color?: Rgba8, preset?: highlights\|midtones\|shadows\|skin, fuzziness (0..200), mode?` | ✓ |
| `select.similar` | `tolerance` | ✓ |
| `select.grow` / `select.shrink` | `by` (px) | ✓ |
| `select.border` | `width` | ✓ |
| `select.smooth` | `radius` | ✓ |
| `select.quick` | `points: Point[], radius, mode: add\|subtract` (edge-aware brush; incremental, merge key per `stroke_id?`) | ✓ |
| `select.refine` | `radius, smooth, feather, contrast, shift_edge (-100..100), decontaminate?` (edge-aware against the merged image) | ✓ |
| `select.transform` | `matrix: {a,b,c,d,e,f}` | ✓ |

## Filters (◻ filters)

All take `id?` (active layer), act on the selection (feathered) or the whole layer, and respect locks via `pixels::edit_layer`.

| op | params |
|---|---|
| `filter.gaussian-blur` | `radius` |
| `filter.box-blur` | `radius` |
| `filter.motion-blur` | `angle (deg), distance` |
| `filter.radial-blur` | `amount, mode: spin\|zoom, cx?, cy?` |
| `filter.surface-blur` | `radius, threshold` |
| `filter.lens-blur` | `radius, depth?` (depth: single-channel bytes, doc-sized; absent = uniform) |
| `filter.tilt-shift` | `center_y, band, feather, radius` |
| `filter.unsharp-mask` | `amount (%), radius, threshold` |
| `filter.smart-sharpen` | `amount, radius, reduce_noise` |
| `filter.high-pass` | `radius` |
| `filter.add-noise` | `amount (%), gaussian?, monochromatic?` |
| `filter.reduce-noise` | `strength (0..10), preserve_details (%), reduce_color_noise (%)` |
| `filter.median` | `radius` |
| `filter.dust-and-scratches` | `radius, threshold` |
| `filter.minimum` / `filter.maximum` | `radius` |
| `filter.pixelate` | `cell` (irreversible mosaic: source pixels are gone) |
| `filter.emboss` | `angle, height, amount` |
| `filter.find-edges` / `filter.solarize` / `filter.invert` / `filter.desaturate` | — |
| `filter.twirl` | `angle` |
| `filter.pinch` | `amount (-100..100)` |
| `filter.spherize` | `amount` |
| `filter.ripple` | `amount, size: small\|medium\|large` |
| `filter.polar-coordinates` | `to_polar` |
| `filter.offset` | `dx, dy, wrap` |
| `filter.clouds` | `seed?` (primary/secondary: `fg, bg`) |
| `filter.custom` | `kernel: number[] (odd square), scale, offset` |
| `filter.apply-adjustment` | `adjustment` (destructive Image › Adjustments) |
| `filter.content-aware-fill` | `sample?: "auto"` (fills the selection from surrounding texture) |
| `filter.spot-heal` | `x, y, radius` |
| `filter.red-eye` | `x, y, width, height, pupil_size?, darken?` |
| `filter.frequency-separation` | `radius` → creates "Low frequency" and "High frequency" layers above; `data.ids` |
| `filter.vignette` | `amount (-100..100), midpoint, feather` |

Analysis (not recorded, `changed: false`):

| op | params → data |
|---|---|
| `analyze.histogram` | `id?, merged?: true, rect?` → `{ r, g, b, l: number[256] }` |
| `analyze.auto` | `style: auto\|vivid\|natural\|bw` → `{ develop: Develop }` (Lite "Auto" variations) |
| `analyze.auto-levels` | `mode: tone\|contrast\|color` → `{ levels: Levels }` |
| `analyze.facts` | → `{ exposure, clipping_low, clipping_high, cast: {temperature, tint}, noise_sigma, sharpness, tilt_degrees }` (planner input) |
| `analyze.pick` | `x, y, size: 1\|3\|5, merged?: true` → `{ color: Rgba8 }` |

## Paint (✓ paint)

`brush`: `{ size, hardness (0..1), opacity (0..1), flow (0..1), spacing (fraction of size, default 0.1), color: Rgba8, blend?: BlendMode, pressure_size?: bool, pressure_opacity?: bool, angle?, roundness? }`. `points`: `[{x, y, p?}]` (p = pressure 0..1).

| op | params |
|---|---|
| `paint.stroke` | `id?, target: pixels\|mask, tool: brush\|pencil\|eraser\|dodge\|burn\|sponge\|blur\|sharpen\|smudge\|clone\|heal, brush, points, stroke_id` — send points in segments while dragging with the same `stroke_id` (one undo step; the engine keeps per-stroke coverage so opacity does not build up within a stroke). Tool extras: `range: shadows\|midtones\|highlights, exposure (0..1)` (dodge/burn), `sponge: saturate\|desaturate` , `strength` (blur/sharpen/smudge), `source: {dx, dy, sample_all?}` (clone/heal: source offset from destination). Returns `dirty`. |
| `paint.stroke-end` | `stroke_id` (not recorded; releases stroke state; heal finalises its blend here) |
| `paint.fill` | `id?, x, y, tolerance, contiguous = true, sample_all = false, anti_alias = true, color, opacity?` (bucket) |
| `paint.magic-erase` | `id?, x, y, tolerance, contiguous = true` |
| `paint.gradient` | `id?, target: pixels\|mask, from, to, gradient: linear\|radial\|angle\|reflected\|diamond, stops, opacity?, blend?, reverse?` |

## Transform (✓ transform)

| op | params |
|---|---|
| `transform.layer` | `ids, matrix?: {a,b,c,d,e,f}, quad?: [Point;4] (perspective/distort: where the layer's bounding box corners go, TL,TR,BR,BL), resample?` |
| `transform.selection-pixels` | `id?, matrix?, quad?` (lifts the selected pixels, transforms, drops them; selection follows) |
| `transform.warp` | `id, grid: Point[16]` (4×4 Bézier patch control points over the layer bounds) |
| `transform.liquify` | `id, tool: forward\|reconstruct\|twirl-cw\|twirl-ccw\|pucker\|bloat\|push-left, size, pressure, density, points, stroke_id` (segments merge like paint strokes) |
| `transform.perspective-crop` | `quad: [Point;4], width, height` (whole document) |
| `transform.lens-correct` | `id?, distortion (-100..100), chromatic_rc (-100..100), chromatic_by, vignette (-100..100), vignette_midpoint, scale (%)` |
| `transform.content-aware-scale` | `width, height, protect_skin?` (whole document) |
| `transform.liquify-end` | `stroke_id` (not recorded; frees liquify state) |

Optional `resample` on layer/selection-pixels/warp/perspective-crop; `auto_scale` on lens-correct. Smart objects: `transform.layer` maps their quad (lossless); warp and content-aware scale rasterize them.

## PSD and project (◻ psd, wasm methods rather than commands)

| method on `Engine` | returns |
|---|---|
| `import_psd(bytes)` | replaces the document; summary JSON; warnings in `data.warnings` |
| `export_psd(options_json)` | PSD bytes (`{"psb": false, "max_compat": true}`) |
| `save_project()` | project bytes |
| `load_project(bytes)` | replaces the document |

## AI (◻ ai, jobs in the worker; see `apps/web/src/lib/ai.ts`)

AI features end in ordinary commands (`select.mask`, `layer.set-pixels`, `layer.import`, `layer.add-adjustment`) with `provenance: "ai:<model-id>"`, so every AI step is visible, undoable and labelled.
