# OpenPhotoEdit — master feature matrix

Compiled 2026-09-14 from the four research files in `docs/research/`.

**Research inputs**

| File | What it covers | Rows |
|---|---|---|
| `01-photoshop-teardown.md` | Every Photoshop 27.10 and Camera Raw 18.6 tool, menu, filter and AI feature | 772 |
| `02-commercial-editors.md` | Features found in other commercial editors but not in Photoshop, plus Lite and AI UX patterns | 133 |
| `03-open-source-and-rust.md` | Open-source editors, Rust crates, licence map, algorithm notes | — |
| `04-local-ai.md` | Every incumbent AI feature mapped to an open model that runs locally, with licence and hardware limits | 62 |

This matrix consolidates those rows into one row per capability. It records whether Lite gets the capability, how much Pro needs it, when it is built, which model it needs (if any) and how hard it is.

Where to look for detail:
- Which Photoshop menu item a row maps to: 01.
- Model sizes and latencies: 04.

## Legend

**Found in** — where the capability exists today.

| Code | Product | Code | Product |
|---|---|---|---|
| PS | Photoshop | DT | darktable |
| ACR | Camera Raw | GIMP | GIMP |
| Lr | Lightroom | KR | Krita |
| C1 | Capture One | OC | **our OpenCapture editor** |
| AF | Affinity | GP | Google Photos |
| PM | Pixelmator Pro / Photomator | Photos | Apple Photos |
| PP | Photopea | R4M | Retouch4me |
| DxO | DxO PhotoLab | — | — |
| LN | Luminar Neo | — | — |

**Lite** — ● means the capability is exposed in the Lite profile. A blank means Pro only. Both profiles share one document engine, so anything done in Lite opens in Pro.

**Pro** — how much a professional needs it.

| Value | Meaning |
|---|---|
| **M** | Must: a switching pro misses it on day one |
| **E** | Edge: real but niche |
| **B** | Bloat: exists for pricing or lock-in |

**Phase** — when it is built. Each phase is defined in `PLAN.md`.

| Value | Phase |
|---|---|
| P0 | Engine |
| P1 | Lite 1.0 |
| P2 | Pro Core, the "open Photopea" |
| P3 | Pro Photographer |
| P4 | Pro Depth |
| P5 | Generative and agent |
| Later | After P5 |
| No | Deliberately not built |

**AI** — whether it needs a model.

| Value | Meaning |
|---|---|
| — | No model needed |
| cls | Classical algorithm |
| T1 | Small model in the browser by default |
| T2 | Optional browser download |
| T3 | Native local backend with a GPU |

**Size** — build effort.

| Value | Effort |
|---|---|
| S | Days |
| M | Weeks |
| L | Months |
| XL | Multi-person-months or research-grade |

---

## A. Files and formats

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| A1 | Open JPEG, PNG, WebP, GIF, BMP | all | ● | M | P1 | — | S | `image`, `zune-jpeg` |
| A2 | Open HEIC/HEIF (iPhone photos) | PS, Photos, AF | ● | M | P1 | — | M | Browser decoder where present; native `libheif` (LGPL, HEVC patents) |
| A3 | Open and save AVIF | PS, AF | ● | M | P1 | — | S | `ravif` / `rav1d` |
| A4 | JPEG XL read and write | PS 27, ACR | | E | P3 | — | S | `jxl-rs` decode (the decoder Chromium and Firefox use); libjxl for lossy encode |
| A5 | TIFF 8/16/32-bit, layered TIFF | PS | | M | P3 | — | M | A layered TIFF carries a PSD layer block |
| A6 | PSD read: layers, groups, masks, blend modes, opacity | PS, PP, AF, GIMP | ● (flattened) | M | P2 | — | L | Fork `ag-psd` (Rust); use psd-tools as an oracle; test against a corpus of real Photoshop files |
| A7 | PSD write with a correct flattened composite | PS, PP | | M | P2 | — | L | Other readers show only the composite, so our compositor must match Photoshop's |
| A8 | PSD adjustment and fill layers round-trip | PS, PP | | M | P2 | — | L | Keys `levl`, `curv`, `hue2`, `selc`, `SoCo`… |
| A9 | PSD text layers round-trip (EngineData) | PS, PP | | M | P4 | — | XL | Write both the text descriptor and our raster |
| A10 | PSD layer styles round-trip (`lfx2`) | PS, PP | | M | P4 | — | L | |
| A11 | PSD smart objects, embedded and linked | PS | | M | P4 | — | XL | `SoLd` / `PlLd` / `lnk2` |
| A12 | PSD 16-bit and 32-bit | PS | | M | P3 | — | M | ZIP with prediction |
| A13 | PSB (large document format) | PS | | E | P4 | — | M | Needed above 30,000 px or 2 GB |
| A14 | Camera RAW open (CR3, NEF, ARW, RAF, DNG…) | ACR, Lr, C1, DT | ● (auto-develop) | M | P3 | — | L | `rawler` (LGPL-2.1, 857 camera modes) |
| A15 | Render to DNG | Lr 15.5, DxO | | E | Later | — | M | |
| A16 | OpenEXR and Radiance HDR | PS, AF | | E | P4 | — | S | `exr` |
| A17 | Place or rasterise SVG | PS, AF | | E | P2 | — | S | `resvg` |
| A18 | Export shapes and paths as SVG | PS | | E | P4 | — | M | |
| A19 | PDF export; open a PDF page as an image | PS, OC | ● (export) | E | P1 | — | S | OpenCapture's `pdf.rs`; `hayro` for import |
| A20 | Animated GIF and frame animation | PS | | E | Later | — | M | |
| A21 | OpenRaster / Krita `.kra` interchange | KR, GIMP | | E | Later | — | M | |
| A22 | GIMP `.xcf` import | GIMP | | E | Later | — | M | |
| A23 | Affinity `.afphoto` import | AF | | B | No | — | XL | Proprietary and undocumented |
| A24 | Native project format: lossless, parametric, versioned | all | ● (autosave) | M | P0 | — | M | Zip of tiles plus a JSON document; carries AI provenance |
| A25 | Autosave and crash recovery | PS, AF | ● | M | P1 | — | M | Stored in OPFS (the browser's private file storage) |
| A26 | Export As / Quick Export: format, size, quality, preview | PS | ● | M | P1 | — | S | |
| A27 | Export recipes: several outputs in one click (Web / Instagram / Print) | C1, Lr, PM | ● | M | P1 | — | S | |
| A28 | Export to a target file size ("under 500 KB") | PS Save for Web | ● | E | P1 | — | S | OpenPhotoId's `encode_jpeg_within` |
| A29 | Metadata keep or strip (EXIF, GPS, XMP) | PS, Lr | ● | M | P1 | — | S | `little_exif`; strip GPS by default |
| A30 | File Info (IPTC/XMP editor) | PS, Lr | | E | P3 | — | S | |
| A31 | Paste from clipboard, drag and drop | PS, PP, OC | ● | M | P1 | — | S | |
| A32 | Recent files; open a folder | Lr, ON1, DxO | ● | M | P1 | — | S | File System Access API |
| A33 | Place embedded or linked file | PS | | M | P2 | — | M | |

## B. Canvas, view and workspace

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| B1 | Zoom and pan, fit, 100%, pinch, scrubby zoom | all, OC | ● | M | P1 | — | S | Tile pyramid |
| B2 | Rotate view | PS, KR | | E | P2 | — | S | |
| B3 | Navigator panel | PS | | E | P2 | — | S | |
| B4 | Rulers, guides, smart guides, snapping | PS | | M | P2 | — | M | |
| B5 | Grid and pixel grid | PS | | E | P2 | — | S | |
| B6 | Before/after toggle and split view | Lr, PM, Photos | ● | M | P1 | — | S | |
| B7 | Info panel, colour samplers | PS | | M | P2 | — | S | Retouchers use them for neutral checks |
| B8 | Live histogram with clipping warnings | PS, Lr | ● | M | P1 | — | S | |
| B9 | History with snapshots | PS | ● (step stack) | M | P1 | — | M | Lite shows every step, and each one can be reopened |
| B10 | Unlimited undo on copy-on-write tiles | all | ● | M | P0 | — | M | Replaces OpenCapture's 20 whole-canvas snapshots |
| B11 | Dockable panels, saved workspaces | PS | | E | P2 | — | M | |
| B12 | Photoshop-compatible shortcuts, customisable | PS, PP | ● (basic) | M | P2 | — | S | Muscle memory is the switching cost |
| B13 | Command palette / tool search | GP, PS Discover | ● | M | P1 | — | S | Shared by Lite and Pro |
| B14 | Contextual task bar | PS 25 | ● | M | P2 | — | S | |
| B15 | Tabs for multiple documents | PS | | M | P2 | — | S | |
| B16 | Switch Lite ↔ Pro without losing anything | ours | ● | M | P1 | — | M | One document model |
| B17 | Light and dark UI in 8 languages | suite | ● | M | P1 | — | S | Per the `openapps-i18n` skill |
| B18 | Touch and pen: pressure, tilt | PS iPad, KR | ● (touch) | M | P2 | — | M | Pointer Events |
| B19 | Full-screen and screen modes | PS | ● | E | P1 | — | S | |
| B20 | Artboards | PS | | E | P4 | — | M | |
| B21 | Note, count and ruler tools | PS | | E | Later | — | S | |
| B22 | Huge documents: tiles, mip levels, spill to disk | PS, KR | | M | P0 | — | XL | The browser build is capped at 4 GB of memory; the native build is not |

## C. Layers and compositing

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| C1 | Pixel layers and groups: reorder, rename, show/hide, lock | PS, all | ● (implicit) | M | P2 | — | M | Lite's step stack *is* a layer stack, kept out of sight |
| C2 | Opacity and fill opacity | PS | | M | P2 | — | S | |
| C3 | All 27 blend modes, matching Photoshop | PS | | M | P2 | — | M | Blend in gamma space by default, as Photoshop does; linear in 32-bit |
| C4 | Pass-through vs isolated groups | PS | | M | P2 | — | M | |
| C5 | Pixel layer masks | PS | | M | P2 | — | M | |
| C6 | Vector masks | PS | | M | P4 | — | M | |
| C7 | Clipping masks | PS | | M | P2 | — | S | |
| C8 | Blend If (split sliders) | PS | | M | P3 | — | M | |
| C9 | Adjustment layers | PS, AF | ● (as steps) | M | P2 | — | M | |
| C10 | Fill layers: solid, gradient, pattern | PS | | M | P2 | — | S | |
| C11 | Smart objects (embedded) | PS | | M | P4 | — | XL | |
| C12 | Linked smart objects, Edit Contents | PS | | E | P4 | — | L | |
| C13 | Smart filters / live filter layers with masks | PS, AF | | M | P4 | — | L | Native in a DAG (graph-based) core |
| C14 | Layer styles: shadows, glows, bevel, satin, overlays, stroke | PS, PP | ● (shadow and stroke on text) | M | P4 | — | L | Distance fields computed by jump flooding |
| C15 | Several instances of one effect | PS | | E | P4 | — | S | |
| C16 | Layer comps | PS | | E | Later | — | M | |
| C17 | Merge down / visible, flatten, stamp visible | PS | | M | P2 | — | S | |
| C18 | Auto-select layer on canvas; align and distribute | PS | | M | P2 | — | S | |
| C19 | Filter the layer list by kind or name | PS | | E | P2 | — | S | |
| C20 | Knockout, transparency shapes layer, blend clipped as group | PS | | E | P4 | — | M | |
| C21 | Layer colour labels | PS | | E | P2 | — | S | |
| C22 | Auto-align layers | PS | | E | P4 | cls | L | Feature matching |
| C23 | Auto-blend layers | PS | | E | P4 | cls | L | |
| C24 | Stack modes such as median (removes tourists) | PS | ● | E | Later | cls | M | |
| C25 | Frame tool / placeholders | PS | | E | Later | — | M | |
| C26 | Optional node-graph view of the document | Graphite, AF | | E | Later | — | L | Exposes the same DAG |
| C27 | CPU reference compositor matched to the GPU | ours | | M | P0 | — | L | The test oracle against Photoshop composites |

## D. Selection and masking

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| D1 | Marquee, rectangle and ellipse: add/subtract, feather, fixed ratio | PS | ● | M | P2 | — | S | |
| D2 | Lasso, polygonal lasso | PS | ● (circle to select) | M | P2 | — | S | |
| D3 | Magnetic lasso | PS | | E | Later | cls | M | |
| D4 | Magic wand: tolerance, contiguous, sample all layers | PS | | M | P2 | — | S | |
| D5 | Quick Selection brush | PS | | M | P3 | cls+T1 | M | Graph cut, assisted by EdgeTAM |
| D6 | Click to select an object | PS, Lr, AF | ● | M | P1 | T1/T2 | M | EdgeTAM (41 MB) → SAM 2.1-small (184 MB) |
| D7 | Highlight objects on hover | PS | ● | E | P3 | T1 | M | Needs masks computed in advance |
| D8 | Select subject | PS, Lr, PM | ● | M | P1 | T1 | M | MODNet for portraits, plus SAM |
| D9 | Select sky | PS, Lr | ● | M | P3 | T2 | M | No clean sky model: Grounding DINO "sky" + SAM, with a classical fallback |
| D10 | Select background (inverse of subject) | Lr, PM | ● | M | P1 | T1 | S | |
| D11 | Select people, face parts, skin, hair, clothes | Lr, PS 26.6, C1 | ● | M | P3 | T1 | M | MediaPipe Multiclass + Face Landmarker (Apache). Not BiSeNet or SegFormer (non-commercial) |
| D12 | Select by text ("the red car") | ours | ● | E | P3 | T2 | M | Grounding DINO tiny + SAM |
| D13 | Depth-range mask | ACR, DxO PL10, ON1 | | M | P3 | T1 | M | Depth Anything V2-Small (Apache). Base and Large are non-commercial |
| D14 | Colour range, luminance range, focus area | PS, ACR | | M | P3 | cls | M | |
| D15 | Select and Mask: refine hair edge, decontaminate colours | PS | ● (auto-refine) | M | P3 | cls | L | Guided filter, plus closed-form matting in the uncertain band |
| D16 | Quick Mask mode | PS | | M | P2 | — | S | |
| D17 | Save selections as alpha channels | PS | | M | P2 | — | S | |
| D18 | Grow, shrink, border, smooth, feather | PS | | M | P2 | — | S | |
| D19 | Transform selection | PS | | M | P2 | — | S | |
| D20 | Path ↔ selection | PS | | M | P4 | — | M | |
| D21 | Paste AI masks across a batch, re-detected on each photo | Lr, C1, Evoto | | M | P3 | T1 | M | Masks are stored as "subject", not as bitmaps |
| D22 | Combine and intersect masks | Lr, C1 16.7 | | M | P3 | — | S | |
| D23 | One-click background removal and cutout export | PP, Canva, PM | ● | M | P1 | T1/T2 | M | MODNet → BiRefNet_lite after a legal check on its training data. **Not RMBG (non-commercial)** |
| D24 | Edge preview overlays | PS | ● | M | P2 | — | S | |
| D25 | Channels panel, spot channels | PS | | E | P4 | — | M | |

## E. Crop, transform and warp

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| E1 | Crop: ratio presets, exact output size, straighten, overlays | PS, all, OC | ● | M | P1 | — | S | OpenCapture's marquee and exact W×H |
| E2 | Non-destructive crop | PS | ● | M | P1 | — | S | |
| E3 | Auto-straighten, suggested crops | PM, GP Auto Frame | ● | E | P1 | cls | M | Horizon from a Hough transform; saliency |
| E4 | Rotate 90°, flip, rotate freely | all | ● | M | P1 | — | S | |
| E5 | Image size (resampling methods), canvas size | PS | ● | M | P1 | — | S | `fast_image_resize` |
| E6 | Free Transform: scale, rotate, skew, distort, perspective | PS | ● (scale/rotate) | M | P2 | — | M | |
| E7 | Warp (grid, split) | PS, PM | | M | P4 | — | L | Pixelmator puts Warp behind its subscription |
| E8 | Puppet Warp | PS | | E | P4 | — | L | |
| E9 | Perspective Warp | PS | | E | P4 | — | L | |
| E10 | Liquify: forward warp, bloat, pucker, freeze mask | PS, AF | | M | P4 | — | L | |
| E11 | Face-aware Liquify | PS, LN | ● (subtle) | E | P4 | T1 | M | MediaPipe landmarks |
| E12 | Perspective crop | PS | | E | P2 | — | S | OpenDocScan's `warp_rgba_to_quad` |
| E13 | Content-aware scale (seam carving) | PS | | E | Later | cls | M | |
| E14 | Manual lens correction: distortion, chromatic aberration, vignette | PS, ACR | | M | P3 | — | M | |
| E15 | Lens-profile correction | ACR, DT | | M | P3 | — | M | The lensfun database is CC-BY-SA |
| E16 | Upright / auto keystone | ACR, Lr | ● ("fix angle") | M | P3 | cls | M | |
| E17 | Adaptive Wide Angle | PS | | E | Later | — | L | |
| E18 | Generative Expand (outpaint) | PS, Lr, iOS 27 | ● (native only) | E | P5 | T3 | XL | FLUX.2 [klein] 4B (Apache) |
| E19 | Wide-angle face un-stretch | DxO ViewPoint | | E | Later | cls | M | |
| E20 | Resize for social formats (1:1, 4:5, 9:16) | Canva | ● | E | P1 | cls | S | Crops around saliency |

## F. Retouch and repair

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| F1 | Spot Healing brush | PS, Lr | ● | M | P2 | cls+T1 | M | Poisson blend; MI-GAN for larger spots |
| F2 | Healing brush (sampled source) | PS | | M | P2 | cls | M | |
| F3 | Clone Stamp: aligned, sample layers, source overlay | PS | | M | P2 | — | M | |
| F4 | Patch tool | PS | | M | P3 | cls | M | |
| F5 | Content-Aware Move / Extend | PS | | E | P4 | cls+T2 | L | |
| F6 | Remove tool: brush or click an object away | PS, Photos, Canva, GP | ● | M | P1 | T1/T2/T3 | M | MI-GAN (28 MB) → big-lama (208 MB) → klein as "Best quality" |
| F7 | Auto-highlight distractions (people, clutter) | ACR, iOS Clean Up | ● | E | P3 | T2 | L | RF-DETR / Grounding DINO + saliency |
| F8 | Wire and cable removal | PS, ACR | ● | E | Later | cls+T2 | L | No permissive model: ridge tracing + LaMa |
| F9 | Reflection removal | PS 27, ACR | | E | Later | — | XL | No clean model exists |
| F10 | Content-Aware Fill with a sampling area | PS | | M | P3 | cls | L | `texture-synthesis` (MIT); check the PatchMatch patent |
| F11 | Red-eye | PS | ● | E | P1 | T1 | S | YuNet + classical |
| F12 | Dust-spot detection and removal | ACR, DxO PL10 | | M | P3 | cls | M | |
| F13 | Blemish removal that keeps pores | Evoto, R4M, C1 | ● | M | P3 | cls+T1 | M | Frequency separation + MI-GAN |
| F14 | Dodge, burn, sponge | PS | | M | P2 | — | S | |
| F15 | Blur, sharpen and smudge tools | PS | | E | P2 | — | S | |
| F16 | One-step frequency separation | AF, R4M | | E | P3 | cls | S | Photoshop needs a manual recipe |
| F17 | Old-photo restoration: scratches, fading | LN, PS Neural Filters | ● | E | P3 | T2 | L | LaMa + classical scratch mask |
| F18 | Glasses glare removal | Evoto | | E | Later | T2 | L | |
| F19 | Deband / remove moiré | PM, ACR | ● | E | P3 | cls | M | |

## G. Painting and drawing

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| G1 | Brush: size, hardness, opacity, flow, spacing, smoothing | PS, KR | ● (pen, highlighter) | M | P2 | — | L | |
| G2 | Brush dynamics: pressure, tilt, jitter, scatter, texture, dual brush | PS, KR | | M | P4 | — | XL | |
| G3 | Import `.abr` brushes | PS, KR, PP | | E | P4 | — | M | |
| G4 | Pencil (aliased) | PS | | E | P2 | — | S | |
| G5 | Mixer brush | PS | | E | Later | — | L | |
| G6 | Eraser, background eraser, magic eraser | PS | ● (eraser) | M | P2 | — | S | |
| G7 | Live, editable gradient tool | PS | | M | P2 | — | M | |
| G8 | Paint bucket | PS | | M | P2 | — | S | |
| G9 | Pattern stamp, history brush | PS | | E | Later | — | M | |
| G10 | Colour picker, swatches, eyedropper | PS | ● | M | P1 | — | S | OpenCapture had no colour choice |
| G11 | Colour replacement brush | PS | | E | Later | — | S | |
| G12 | Symmetry painting | PS, KR | | E | Later | — | M | |
| G13 | Live shapes: rectangle with corner radii, ellipse, polygon, line with arrowheads, custom | PS, OC | ● (arrow, box, ellipse, line) | M | P2 | — | M | |
| G14 | Adjustment brush (paint an adjustment) | PS 25.9, Lr | ● | M | P2 | — | S | |
| G15 | AI brush / generative paint | Krita AI, InvokeAI | | E | P5 | T3 | XL | |

## H. Adjustments and colour

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| H1 | Auto-enhance offering 3–4 alternatives | GP, PM | ● | E | P1 | cls | M | Learned auto-tone is blocked: every checkpoint is trained on FiveK/PPR10K (research-only) |
| H2 | Master Light / Color / B&W sliders that expand | Photos | ● | B | P1 | — | S | The core Lite control |
| H3 | Brightness / contrast | PS | ● | M | P1 | — | S | |
| H4 | Levels: per channel, auto, eyedroppers | PS | | M | P2 | — | S | |
| H5 | Curves: per channel, on-image adjust, eyedroppers | PS | | M | P2 | — | M | |
| H6 | Exposure / offset / gamma | PS | ● | M | P1 | — | S | |
| H7 | Vibrance / saturation | PS | ● | M | P1 | — | S | |
| H8 | Hue/Saturation: ranges, colorize | PS | | M | P2 | — | S | |
| H9 | Color Balance | PS | | M | P2 | — | S | |
| H10 | Black & White with channel weights | PS | ● | M | P1 | — | S | |
| H11 | Photo Filter | PS | | E | P2 | — | S | |
| H12 | Channel Mixer | PS | | M | P2 | — | S | |
| H13 | Color Lookup (3D LUT `.cube`) | PS | ● (looks) | M | P2 | — | S | |
| H14 | Invert, posterize, threshold, equalize, desaturate | PS | | M | P2 | — | S | |
| H15 | Gradient Map | PS | | M | P2 | — | S | |
| H16 | Selective Color | PS | | M | P2 | — | S | Graphite has a Photoshop-matched version (MIT) |
| H17 | Shadows / highlights | PS | ● | M | P1 | cls | M | Local tone mapping |
| H18 | HDR Toning (legacy) | PS | | B | No | — | — | Replaced by tone mapping (L2) |
| H19 | Match the look of a reference photo | PS, C1, PM | ● | E | P3 | cls | M | LUT fitting (MKL, sliced OT) |
| H20 | Replace Color | PS | | E | P2 | — | S | |
| H21 | White balance: temperature, tint, picker, auto | ACR, all | ● | M | P1 | cls | S | |
| H22 | Clarity, texture, dehaze | ACR | ● | M | P1 | cls | M | Local Laplacian / guided filter |
| H23 | Looks with a strength slider | Snapseed, Photos | ● | E | P1 | — | S | |
| H24 | 2D style pad | Apple Styles | ● | B | P1 | — | S | |
| H25 | Film emulation with grain | DxO FilmPack, Snapseed | ● | E | P3 | — | M | |
| H26 | Colour-grading wheels | ACR, DxO PL10 | | M | P3 | — | S | |
| H27 | Point Color / colour editor with uniformity | ACR, C1 | | M | P3 | — | M | |
| H28 | Built-in and user presets | PS, Lr | ● | M | P2 | — | S | |
| H29 | Even out exposure and WB across a set | C1 | | M | P3 | cls | M | Wedding and event consistency |
| H30 | U-Point control points | DxO | | E | Later | cls | M | |
| H31 | Bloom / glow / Orton | AF, LN | ● | E | P2 | — | S | |
| H32 | Post-crop vignette | ACR | ● | M | P1 | — | S | |
| H33 | Colourise a black-and-white photo | PS Neural, AF | ● | E | P3 | T2 | M | DeOldify (MIT, already in OpenPixels) |
| H34 | Low-light enhancement | ours | ● | E | P3 | T1 | S | Retinexformer (MIT) |

## I. Filters and effects

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| I1 | Gaussian, box, surface blur | PS | | M | P2 | — | S | |
| I2 | Motion, radial, path and spin blur | PS | | E | P4 | — | M | |
| I3 | Field, iris and tilt-shift blur | PS | ● (tilt-shift) | E | P3 | — | M | |
| I4 | Blur the background / lens blur with bokeh | ACR, Photos | ● | M | P1 → P3 | T1 | M | P1 blurs outside the subject mask; P3 uses depth-aware bokeh |
| I5 | Unsharp mask, smart sharpen, high pass | PS | ● (sharpen) | M | P1 | — | S | `pixels-core::unsharp` |
| I6 | Output sharpening by medium | Lr | | E | P3 | — | S | |
| I7 | Add noise / grain | PS | ● | M | P2 | — | S | |
| I8 | Median, dust & scratches, classical noise reduction | PS | ● | M | P1 | cls | S | |
| I9 | AI denoise for JPEG and phone photos | ACR, PM, Topaz | ● | M | P1 | T1/T2 | M | realesr wdn/dn50 (OpenPixels) → NAFNet |
| I10 | AI raw denoise | ACR, DxO DeepPRIME | | M | P3 | T2/T3 | L | RawNIND is GPL-3 weights: fine under AGPL |
| I11 | Deblur / motion shake | Topaz, GP Unblur | ● | E | P3 | T2 | M | NAFNet-GoPro |
| I12 | Remove JPEG artefacts | PM, Topaz | ● | E | P1 | T1 | S | 1x-DeJPG-OmniSR (CC-BY, credit needed) |
| I13 | Faithful 2×/4× upscale | ACR, PM, Topaz | ● | M | P1 | T1/T2 | M | realesr x4v3 / SPAN → RealPLKSR. **Not 4x-UltraSharp (non-commercial)** |
| I14 | Generative upscale with a creativity slider | PS/Topaz, Magnific | | B | P5 | T3 | XL | AdcSR. Photographers prefer faithful modes |
| I15 | Face restoration | Topaz, LN, OpenPixels | ● | E | P3 | T2 | M | GFPGAN fp32 only. **Not CodeFormer (non-commercial)** |
| I16 | Pixelate / mosaic that permanently destroys the source pixels | PS, OC | ● | M | P1 | — | S | OpenCapture's `pixelateRegion` |
| I17 | Distort: pinch, polar coordinates, ripple, spherize, twirl, wave, displace | PS | | E | P4 | — | M | |
| I18 | Stylize: emboss, find edges, oil paint, solarize, wind | PS | | E | P4 | — | S | |
| I19 | Render: clouds, lens flare, lighting effects, flame, tree | PS | | E | Later | — | M | |
| I20 | Filter Gallery (artistic and sketch effects) | PS | | B | Later | — | M | |
| I21 | High pass, minimum/maximum, offset, custom kernel | PS | | M | P2 | — | S | |
| I22 | Vanishing Point | PS | | E | Later | — | L | |
| I23 | Write-your-own expression filter | AF | | E | Later | — | M | Generates WGSL shader code |
| I24 | Relighting with lights placed on the photo | LN Light Depth, ON1 | ● (portrait light) | E | P4 | T2/T3 | L | Depth + MoGe-2 normals; IC-Light v1 natively (licence to verify) |
| I25 | Depth-aware fog and haze | LN | ● | E | Later | T1 | M | |
| I26 | Sky replacement with foreground colour match | PS, LN | ● | E | P4 | T2 | L | Sky mask via Grounding DINO + SAM |
| I27 | Multi-band sharpening | AF | | E | Later | — | S | |
| I28 | Harmonize a pasted layer to the scene | PS 27 | | E | P4 | T2 | M | PCT-Net (weights licence to verify) |
| I29 | Neural style transfer | PS Neural | | B | Later | T2 | M | |
| I30 | Focus peaking overlay | C1, AF | | M | P3 | cls | S | |

## J. Type and vector

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| J1 | Point and paragraph text: font, size, colour | PS, OC | ● | M | P1 (basic) | — | M | `parley` + `swash` |
| J2 | OpenType features, kerning, tracking, leading | PS | | M | P4 | — | L | |
| J3 | Character and paragraph styles | PS | | E | P4 | — | M | |
| J4 | Text on a path, warped text | PS | | E | P4 | — | M | |
| J5 | Vertical and CJK text | PS | | E | P4 | — | XL | We ship zh, ja and ko UIs |
| J6 | Use locally installed fonts | PS | ● | M | P2 | — | S | Local Font Access API; native build has full access |
| J7 | Identify a font from an image | PS Match Font | | E | Later | T2 | M | |
| J8 | Turn text in a photo into an editable text layer | Canva, PS | | E | P4 | T1 | L | PP-OCRv5 + inpaint + font match |
| J9 | Pen, curvature pen, anchor editing | PS | | M | P4 | — | M | `kurbo` |
| J10 | Paths panel, path booleans, stroke/fill path | PS | | M | P4 | — | M | |
| J11 | Live shape properties | PS | ● | M | P2 | — | M | |
| J12 | Trace an image into vectors | PS, AF | | E | Later | cls | M | `vtracer` (MIT) |
| J13 | Stickers and emoji | phones, Canva | ● | B | Later | — | S | |

## K. RAW develop (Camera Raw class)

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| K1 | Demosaic Bayer and X-Trans; black and white levels | ACR, DT | ● (auto) | M | P3 | — | L | Can learn from RawTherapee/darktable under AGPL |
| K2 | Camera profiles and looks | ACR, C1 | | M | P3 | — | L | |
| K3 | Scene-referred tone mapping with highlight recovery | DT, ACR | ● (auto) | M | P3 | — | L | Sigmoid / AgX-style |
| K4 | Basic panel: exposure, contrast, highlights, shadows, whites, blacks | ACR | ● | M | P3 | — | M | |
| K5 | Tone curve: parametric and point | ACR | | M | P3 | — | S | |
| K6 | HSL / colour mixer | ACR | | M | P3 | — | S | |
| K7 | Detail: sharpening, noise reduction | ACR | | M | P3 | — | M | |
| K8 | Optics: lens profile, CA, defringe, vignette | ACR | | M | P3 | — | M | |
| K9 | Geometry: Upright, manual transforms | ACR | | M | P3 | cls | M | |
| K10 | Effects: grain, post-crop vignette | ACR | ● | M | P3 | — | S | |
| K11 | Calibration | ACR | | E | P3 | — | S | |
| K12 | Local masks: brush, linear, radial, range | ACR | | M | P3 | — | M | |
| K13 | AI masks: subject, sky, background, objects, people and their parts | ACR | ● | M | P3 | T1/T2 | M | See D6–D13 |
| K14 | Raw Details / Super Resolution | ACR | | E | Later | T2 | L | |
| K15 | HDR editing and HDR output | ACR 15+ | | E | Later | — | L | Browser HDR canvases vary |
| K16 | Snapshots, virtual copies | ACR, Lr | ● | M | P3 | — | S | |
| K17 | Copy, paste and sync settings across photos | Lr, Photos | ● | M | P3 | — | S | |
| K18 | Recommended / adaptive presets | ACR, Lr | ● | E | P3 | T2 | M | |
| K19 | Instant preview from the embedded JPEG | Lr | | M | P3 | — | S | |
| K20 | **RAW layer inside a layered document, re-editable at any time** | PS (ACR smart object), AF | | M | P3 | — | M | Part of our wedge: raw development lives inside the layered document instead of a separate app |
| K21 | White-balance picker; clipping warnings | ACR | ● | M | P3 | — | S | |
| K22 | Flat-field correction | C1 | | E | Later | — | S | |
| K23 | Film-negative inversion | Negative Lab Pro | | E | Later | cls | M | |
| K24 | Import Lightroom/ACR XMP settings (best effort) | ACR | | E | Later | — | L | |

## L. Merges and stacking

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| L1 | Panorama | PS, ACR, Lr | | E | P4 | cls | L | Reimplement OpenCV's algorithms (Apache) |
| L2 | HDR merge, 32-bit tone mapping | PS, ACR, AF | | E | P4 | cls | L | |
| L3 | Focus stacking | PS, AF, ON1 | | E | P4 | cls | L | |
| L4 | Astro stacking with calibration frames | AF | | E | Later | cls | L | |
| L5 | Best Take (best face from a burst) | Pixel | ● | E | Later | T1 | L | |

## M. Library, culling, batch and automation

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| M1 | Browse a folder without importing | ON1, DxO | ● | M | P3 | — | M | |
| M2 | Ratings, flags, colour labels, keywords | Lr, C1 | | M | P3 | — | L | SQLite natively; IndexedDB in the browser |
| M3 | Collections and smart collections | Lr, C1 | | E | Later | — | M | |
| M4 | Compare and Survey views | Lr, C1 | ● | M | P3 | — | S | |
| M5 | Keyboard culling with auto-advance | Lr, C1 | | M | P3 | — | S | |
| M6 | AI culling: eye focus, closed eyes, exposure, blur | Lr 15, C1 16.8, Aftershoot | ● (best of burst) | M | P3 | T1 | M | YuNet + Face Landmarker + Laplacian. **Lightroom charges credits for this** |
| M7 | Group similar shots and pick the best frame | Aftershoot, ON1 2027 | ● | M | P3 | T2 | M | CLIP q8 + quality score |
| M8 | Duplicate detection | Lr | ● | E | P3 | cls | S | Perceptual hash |
| M9 | Local natural-language photo search | Lr (cloud only), GP | ● | E | Later | T2 | L | CLIP / SigLIP 2. **Not MobileCLIP (research-only)** |
| M10 | Group photos by face | Lr, Photos | | E | Later | T2 | L | SFace. **Not InsightFace (non-commercial)** |
| M11 | Auto keywords, captions, alt text | Lr | | E | Later | T2 | M | Florence-2 (MIT) |
| M12 | Batch recipe: preset, resize, rename, watermark, export | PS Image Processor, PM | ● (batch resize/export) | M | P3 | — | M | |
| M13 | Actions: record, play back, batch, droplets | PS | | M | P3 | — | L | Easy because every operation is already a typed command |
| M14 | Scripting and plugin API | PS UXP, PP | | E | P4 | — | L | WASM components, sandboxed |
| M15 | Variables and data sets | PS | | B | Later | — | M | |
| M16 | Batch rename with tokens | Lr, Bridge | | M | P3 | — | S | |
| M17 | Watch folders | ON1, Lr | | E | Later | — | M | Native only |
| M18 | Card import with backup copy | Lr, C1 | | E | Later | — | M | Native only |
| M19 | Tethered capture | Lr, C1 | | E | Later | — | XL | WebUSB PTP, or libgphoto2 natively |
| M20 | Watermark on export: text or logo, tiled, opacity | Lr, C1, OC | ● | M | P1 | — | S | **Paid in OpenCapture; free here.** Port `openpdfedit-watermark` (MIT/Apache) |
| M21 | Contact sheets, print packages | Lr, C1 | | E | Later | — | M | |
| M22 | Map view, books, slideshows, web galleries | Lr | | B | No | — | — | |

## N. Portrait retouching

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| N1 | Skin smoothing that keeps texture | PS Neural, Evoto, OpenPhotoId | ● | M | P3 | cls+T1 | M | OpenPhotoId `frame-retouch/skin.rs` |
| N2 | Even out skin tone | C1, R4M | ● | E | P3 | cls | M | |
| N3 | Brighten eyes, whiten teeth | C1, LN | ● | E | P3 | T1 | S | Landmarks |
| N4 | Shine reduction | R4M | ● | E | Later | cls | M | |
| N5 | AI dodge and burn | R4M | | E | Later | T2 | L | |
| N6 | **Apply one retouch profile to every face in a shoot** | Evoto (cloud, per export), R4M | | M (portrait) | P4 | T1 | L | Wedge against per-image pricing |
| N7 | Face-shape sliders | LN, Evoto, PS Face-Aware Liquify | ● (subtle) | E | P4 | T1 | M | Off by default |
| N8 | Body reshaping | LN, Evoto | | B | No | — | — | Ethically contested |
| N9 | Flyaway hair cleanup | Evoto | | E | Later | T2 | XL | |
| N10 | Makeup | Evoto | | B | No | — | — | |
| N11 | Backdrop cleanup | R4M | | E | Later | cls | M | |

## O. Generative AI (native backend)

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| O1 | Generative Fill from a prompt inside a selection | PS, AF, PP | ● (if the native pack is installed) | E | P5 | T3 | XL | FLUX.2 [klein] 4B, Apache (~8.4 GB). **Not FLUX dev/Fill/Kontext (non-commercial)** |
| O2 | Generate a background behind the subject | PS, Photoroom | ● | E | P5 | T3 | XL | |
| O3 | Generate an image from text as a new layer | PS, AF | | E | P5 | T3 | L | |
| O4 | Variations / generate similar | PS | | E | P5 | T3 | M | |
| O5 | Condition on a reference image | PS 27 | | E | P5 | T3 | L | |
| O6 | Edit the whole image by instruction, then check the result | GP, ChatGPT, Nano Banana | | E | P5 | T3 | XL | Qwen-Image-Edit-2511 workstation pack (≥24 GB); changes outside the mask are rejected |
| O7 | Tap an object to move it, with the hole filled and a shadow added | Canva Magic Grab, Samsung | ● | E | P4 | T1/T2 | L | SAM + LaMa + drop shadow |
| O8 | Bridge to the user's own ComfyUI or InvokeAI | krita-ai-diffusion | | E | P5 | — | M | A separate process, so no licence entanglement |
| O9 | Rotate an object / new viewpoint | PS 27, iOS 27 | | B | No | — | — | Reviewers report distorted faces |
| O10 | Photo-to-video animation | Lr mobile | | B | No | — | — | |
| O11 | Train a custom model | PS (500 credits) | | B | No | — | — | |

## P. AI-native experience

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| P1 | **Prompt bar that turns requests into visible slider changes** | Lr "Edit using Describe" (credits, Android) | ● | M | P1 rules → P3 LLM | cls → T2 | M | A rule parser covering ~50 phrasings in 8 languages, then Qwen3-1.7B |
| P2 | Suggestion chips ("make it better", "blur background") | GP, Lr | ● | E | P1 | cls | S | |
| P3 | Quick actions that appear only when a subject, sky or face is detected | Lr mobile | ● | E | P1 | T1 | M | |
| P4 | Object first: tap or circle → Erase / Move / Adjust / Blur | GP, Canva | ● | M | P1 | T1 | M | |
| P5 | Autopilot: suggest fixes for noise, blur, tilt and faces without applying them | Topaz | ● | E | P3 | cls | M | Uses the Rust image-facts extractor |
| P6 | Every AI step stays in the history and can be re-edited | Samsung, Lr | ● | M | P1 | — | M | Lightroom's "Flatten AI edits" exists to save credits; we have no credits to save |
| P7 | Fast / Best quality choice on heavy AI | iOS 27 | ● | E | P1 | — | S | Chooses MI-GAN vs LaMa vs klein |
| P8 | Faithful vs creative toggle | Magnific, Topaz | | E | P5 | — | S | Faithful is the default |
| P9 | Multi-step agent: plan, preview, confirm, execute | PS AI Assistant, Canva AI 2.0 | | E | P5 | T3 | L | Qwen3.5-4B, constrained to valid tool calls |
| P10 | Describe a repetitive job in words and save it as an Action | AF 3.2 (Claude connector) | | E | P5 | T2/T3 | M | |
| P11 | Models page: size, licence, download, forget, attributions | OpenPixels | ● | M | P1 | — | S | |
| P12 | Diagnostics and GPU pixel-parity canary | OpenPhotoId | | M | P0 | — | S | Catches silently wrong WebGPU output |
| P13 | Background AI job queue with progress and cancel | Lr, C1 | ● | M | P1 | — | M | |
| P14 | "What to do next" coach | LN | ● | B | Later | T2 | M | |
| P15 | Content Credentials (C2PA) on export, with AI steps labelled | PS, Lr | ● (toggle) | M | P3 | — | M | The `c2pa` crate builds for wasm |

## Q. Colour management and output

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| Q1 | Per-document working space: sRGB, P3, Adobe RGB, ProPhoto | PS | (auto) | M | P2 | — | M | `moxcms` |
| Q2 | Handle embedded profiles; assign or convert | PS | (auto) | M | P2 | — | S | |
| Q3 | 16-bit editing | PS | | M | P3 | — | M | Photoshop's 16-bit is 0–32768 |
| Q4 | 32-bit float / HDR documents | PS | | E | P4 | — | L | |
| Q5 | Soft proofing, gamut warning | PS, Lr | | M | P4 | — | M | |
| Q6 | CMYK mode and separations | PS, AF | | E (M for print) | P4 | — | XL | `lcms2` natively |
| Q7 | Lab, grayscale, duotone, indexed, bitmap, multichannel | PS | | E | Later | — | M | |
| Q8 | Colour-managed print: sizing, marks | PS, Lr | ● (simple) | E | P4 | — | M | |
| Q9 | Wide-gamut (P3) canvas | PS | (auto) | E | P3 | — | M | |
| Q10 | Copy to clipboard with transparency | OC, PS | ● | M | P1 | — | S | |

## R. Annotation (from OpenCapture)

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| R1 | Arrow with chosen colour and width | OC | ● | E | P1 | — | S | OpenCapture's is fixed red, width 3 |
| R2 | Rectangle, ellipse, line | OC | ● | E | P1 | — | S | |
| R3 | Text box with a background pill | OC | ● | E | P1 | — | S | |
| R4 | Highlighter, freehand pen | (missing in OC) | ● | E | P1 | — | S | |
| R5 | Numbered step markers | (missing in OC) | ● | E | Later | — | S | |
| R6 | Solid redaction box + irreversible pixelate | OC | ● | M | P1 | — | S | |
| R7 | Shapes stay editable after they are placed | OC (until commit) | ● | M | P1 | — | S | Kept as vector layers, not baked into pixels |
| R8 | Frames and borders (device, polaroid, caption) | OC, Snapseed | ● | E | P1 | — | S | |
| R9 | Collage | Canva, phones | ● | E | Later | — | M | |

## S. Platform and extensibility

| ID | Feature | Found in | Lite | Pro | Phase | AI | Size | Notes |
|---|---|---|---|---|---|---|---|---|
| S1 | Zero-install web app that works offline | PP, ours | ● | M | P1 | — | M | |
| S2 | **Local Rust server binary**: same UI, native AI, no 4 GB memory limit | OpenDownloader pattern | | M | P2 | — | M | `axum` + `rust-embed`, one loopback port |
| S3 | Desktop shell (Tauri) and store apps (iPad, Android) | PS iPad, AF, PM | ● | M | Later | — | L | The suite's end state |
| S4 | Browser extension: right-click any image → edit | OpenPixels | ● | E | Later | — | S | |
| S5 | Sandboxed WASM plugin system | PS UXP | | E | P4 | — | L | |
| S6 | Host Photoshop `.8bf` plugins | PS | | E | No | — | — | Windows-native only |
| S7 | Embeddable editor API (iframe + postMessage) | PP API | | E | Later | — | M | Photopea has validated this |
| S8 | Self-hostable static bundle / Docker image | PP issue #4870 | | M | P2 | — | S | Photopea's most-requested feature |
| S9 | Accessibility: keyboard navigation, screen-reader panels | — | ● | M | P1 | — | M | |
| S10 | **No third-party requests at run time** | ours | ● | M | P0 | — | S | Disable `ort-web` telemetry; self-host the ONNX runtime and models |
| S11 | Cloud documents and version-history sync | PS | | B | No | — | — | Could come later through OpenSync |
| S12 | Libraries, Stock, font activation | PS | | B | No | — | — | |
| S13 | Invite to Edit, comments | PS | | B | No | — | — | |
| S14 | Accounts, credits, paid tiers | PS, AF, PP | | — | No | — | — | Optional support features later; never gating a function |
| S15 | Video timeline | PS | | B | No | — | — | |
| S16 | 3D | PS (removed) | | — | No | — | — | |

---

## Counts

Computed from the tables above by `scripts/matrix-counts.mjs`.

| Group | Rows | In Lite | Pro must | Needs a model (T1–T3) |
|---|---|---|---|---|
| A Files and formats | 33 | 14 | 20 | 0 |
| B Canvas, view and workspace | 22 | 12 | 15 | 0 |
| C Layers and compositing | 27 | 4 | 16 | 0 |
| D Selection and masking | 25 | 12 | 21 | 11 |
| E Crop, transform and warp | 20 | 10 | 10 | 2 |
| F Retouch and repair | 19 | 8 | 9 | 9 |
| G Painting and drawing | 15 | 5 | 8 | 1 |
| H Adjustments and colour | 34 | 19 | 21 | 2 |
| I Filters and effects | 30 | 14 | 11 | 13 |
| J Type and vector | 13 | 4 | 6 | 2 |
| K RAW develop | 24 | 9 | 17 | 3 |
| L Merges and stacking | 5 | 1 | 0 | 1 |
| M Library, culling, batch | 22 | 8 | 10 | 5 |
| N Portrait retouching | 11 | 5 | 2 | 6 |
| O Generative AI | 11 | 3 | 0 | 7 |
| P AI-native experience | 15 | 11 | 7 | 6 |
| Q Colour management | 10 | 2 | 5 | 0 |
| R Annotation | 9 | 9 | 2 | 0 |
| S Platform | 16 | 5 | 6 | 0 |
| **Total** | **361** | **155** | **186** | **68** |

**Rows by phase:**

| Phase | Rows |
|---|---|
| P0 | 6 |
| P1 | 69 |
| P2 | 71 |
| P3 | 76 |
| P4 | 51 |
| P5 | 13 |
| Later | 58 |
| No | 15 |
| Split (P1 → P3) | 2 |

Photoshop parity is not a release; it is the sum of P2–P4 plus the "Later" rows.
