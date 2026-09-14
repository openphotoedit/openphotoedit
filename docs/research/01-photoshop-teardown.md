# Adobe Photoshop teardown (desktop 27.x, web, iPad/mobile, Camera Raw 18.x)

Research for OpenPhotoshop. Compiled 2026-09-14. Current shipping versions at time of writing: **Photoshop 27.10** (released 2026-08-28) and **Camera Raw 18.6** (late August 2026).

## Summary

**What a switching pro needs on day one is almost entirely non-AI.** The things that decide a switch are:

- **Layer compositing:** layers, groups, pixel/vector/clipping masks, the 27 blend modes, Blend If, fill vs opacity, layer styles.
- **Non-destructive editing:** adjustment layers, smart objects with smart filters.
- **Selections:** marquee, lasso, wand, Quick/Object Selection, Select and Mask edge refinement.
- **Retouching:** Healing, Spot Healing, Clone, Patch, Content-Aware Fill, Dodge/Burn.
- **Warping:** Free Transform, Warp, Puppet Warp, Liquify.
- **Painting and vectors:** a brush engine with pen pressure, Pen tool and paths, a real type engine.
- **Pixel pipeline:** 16-bit, ICC colour management with soft proofing, and Camera Raw for raw files.
- **Files:** PSD/PSB round-trip fidelity, plus Actions and Batch processing.

**Adobe's AI features sit behind server calls, credits and an account.** These are OpenPhotoshop's differentiation seams:

- **Behind servers and credits:**
  - **Generative:** Generative Fill, Expand, Generate Image, Generate Background and Generate Similar.
  - **Newer compositing tools:** Generative Upscale, Harmonize, Rotate Object and Reflection Removal.
  - **Assistants:** the AI Assistant and AI Assisted Editor.
  - **Partner models:** Google Gemini/"Nano Banana" and Black Forest Labs FLUX.
- **Cost:** a single-app Photoshop plan includes 25 generative credits a month. Firefly Gen Fill costs 1 credit; partner models cost 10–40 credits per generation.
- **Remove tool:** it only got an on-device model option in 27.7 (May 2026).
- **Cloud-assisted selection:** Select Subject and Remove Background have a cloud-processing option (26.6).
- **Neural Filters:** they must be downloaded from Adobe's cloud, and some process in the cloud.

**Bloat / lock-in surfaces** exist mainly to sell storage, Stock and the ecosystem:

- Cloud documents (.psdc) and version history.
- Invite to Edit, Projects (26.11) and Comments.
- Creative Cloud Libraries and Adobe Stock (a browser was added in 27.10).
- Firefly Boards, Express templates, the Discover/Home screen, and Adobe Fonts activation.

**Deprecated or removed:**

- **3D:** discontinued from 22.5 (2021); the 3D menu and preferences were removed by 25.4.
- **Substance 3D Viewer bridge:** discontinued 2025-10-16.
- **Shake Reduction filter:** removed in 23.3.
- **Save for Web (Legacy):** still present but "modernized" in 27.7.
- **Also legacy:** slices/web export, CEP extension panels, and Neural Filters (a 2020–22 system largely superseded by Firefly features; update status unverified).

**Hardest engineering (XL):**

- PSD/PSB read-write fidelity: layer effects, smart objects, text engine data, adjustment layers.
- The type engine: OpenType shaping, paragraph composer, CJK/vertical text.
- The brush engine: dynamics, stylus input, mixer brush.
- Content-Aware Fill/Move quality: PatchMatch-class synthesis.
- The Camera Raw pipeline: demosaic, DCP profiles, lens database, Process Version 6 tone mapping, AI masks.
- Liquify and Puppet Warp at interactive speed.
- Select and Mask edge refinement (hair).
- Colour management with soft proofing and CMYK separations.
- AI features that need models (segmentation, inpainting, denoise, super-resolution, depth).

### Legend

| Column | Meaning |
|---|---|
| **Class** | `must` = a switching pro misses it on day one; `edge` = real but rare/niche; `bloat` = exists for pricing/ecosystem lock-in; `deprecated` = removed or on its way out |
| **Lite?** | `yes` = a casual user wanting simple edits would want it |
| **AI?** | `yes` = needs a neural model; `opt` = has a neural mode but a classical fallback exists |
| **Cloud seam** | `—` none; `cloud` = Adobe processes server-side; `credits` = consumes generative credits; `account` = needs Adobe ID / online licence |
| **Difficulty** | Rust/wgpu/wasm build effort. S ≈ days, M ≈ weeks, L ≈ months, XL ≈ multi-person-months or research-grade |

### Method and verification caveat

`helpx.adobe.com` returned HTTP 403 to the research fetcher for every page tried, including the desktop what's-new page, release notes, the Neural Filters list and the generative credits FAQ. That limited what could be checked directly:

- **Menus, tools and filters (the long-standing inventory):** enumerated from the Adobe user-guide structure as known through the Photoshop 2025 docs.
- **2023–2026 release chronology:** cross-checked against Adobe Community release announcements, Adobe newsroom, CG Channel's per-version coverage, DPReview, Julieanne Kost (Adobe) and Computer Darkroom.
- **Plan prices:** fetched from adobe.com/products/photoshop/plans.html on 2026-09-14.
- **Where marked:** anything not confirmed by a fetched source for the 2025–2026 state is marked **(unverified)**.

---

## 1. Tools (toolbar, including nested tools)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Move tool (V) | Move layers/selections; auto-select, show transform controls | must | yes | no | — | S | Auto-select needs per-pixel layer hit test |
| Align/distribute (Move options bar) | Align and distribute layers to selection/canvas | must | yes | no | — | S | |
| Artboard tool (V, nested) | Create and resize artboards | edge | no | no | — | M | Artboards are special groups with clip + auto-nesting |
| Rectangular Marquee (M) | Rectangle selection; feather, fixed ratio/size, add/subtract | must | yes | no | — | S | Selection = 8-bit mask, marching ants |
| Elliptical Marquee (M) | Ellipse selection with anti-alias | must | yes | no | — | S | |
| Single Row Marquee | 1-pixel-high row selection | edge | no | no | — | S | |
| Single Column Marquee | 1-pixel-wide column selection | edge | no | no | — | S | |
| Lasso (L) | Freehand selection | must | yes | no | — | S | |
| Polygonal Lasso (L) | Straight-edged polygon selection | must | no | no | — | S | |
| Magnetic Lasso (L) | Lasso that snaps to detected edges | edge | no | no | — | M | Live-wire edge cost path |
| Object Selection (W) | Box/lasso around object, auto-refines to it | must | yes | yes | cloud option for Select Subject only (26.6) | L | Segmentation model on device (SAM-class); wasm/WebGPU perf |
| Object Finder hover (Object Selection option) | Highlights detectable objects on hover; click to select | edge | yes | yes | — | L | Needs precomputed instance masks per document |
| Select Details / Select People (Object Selection) | Selects eyes, mouth, skin, hair, clothing individually (26.6) | edge | yes | yes | (unverified) | L | Face/body part parsing model |
| Quick Selection (W) | Brush that grows selection along edges | must | yes | opt | — | M | Graph-cut; "Enhance edge" option |
| Magic Wand (W) | Select contiguous/global similar colours by tolerance | must | yes | no | — | S | Flood fill, sample all layers |
| Selection Brush tool | Paint a selection directly as a brush (2024) | edge | yes | no | — | S | Added for generative workflows; toolbar group (unverified) |
| Crop (C) | Crop, straighten, ratio presets, overlays, delete/keep pixels | must | yes | no | — | M | Non-destructive crop, content-aware fill on rotate |
| Crop → Generative Expand | Enlarge canvas and AI-fill new area | edge | yes | yes | cloud + credits | XL | Outpainting model |
| Perspective Crop (C) | Crop quadrilateral and rectify perspective | edge | no | no | — | S | Homography resample |
| Slice (C) | Define web slices for legacy export | deprecated | no | no | — | S | Legacy web; tied to Save for Web |
| Slice Select (C) | Edit slice options/bounds | deprecated | no | no | — | S | |
| Frame tool (K) | Rectangle/ellipse placeholder frames that mask placed images | edge | no | no | — | M | New shape options + generative integration in 26.3 |
| Eyedropper (I) | Sample colour; sample size, current/all layers | must | yes | no | — | S | |
| Color Sampler (I) | Up to 10 persistent colour readouts in Info panel | must | no | no | — | S | Retouchers rely on it for neutral checks |
| Ruler (I) | Measure distance/angle; Straighten Layer | edge | no | no | — | S | |
| Note (I) | Attach text annotations to document | edge | no | no | — | S | Stored in PSD |
| Count (I) | Place numbered count markers | edge | no | no | — | S | Scientific/measurement use |
| 3D Material Eyedropper | Sampled 3D material | deprecated | no | no | — | — | Removed with 3D |
| Spot Healing Brush (J) | One-click blemish removal; content-aware/texture/proximity | must | yes | no | — | M | Texture synthesis + Poisson-style blending |
| Remove tool (J) | Brush over object; AI fills plausibly | must | yes | yes | cloud by default; on-device model option since 27.7 | L | Inpainting model; Remove Tool 3 gives 2K output (27.3) |
| Remove tool → Find Distractions: Wires and cables | Auto-detect and remove wires/cables (26.0) | edge | yes | yes | cloud (Firefly servers) | L | Thin-structure segmentation + inpaint |
| Remove tool → Find Distractions: People | Auto-detect and remove background people | edge | yes | yes | cloud | L | Person detection + large-area inpaint |
| Remove tool → General distractions | Auto-detect clutter category (27.6) | edge | yes | yes | cloud (unverified) | XL | Saliency judgement |
| Healing Brush (J) | Alt-sampled healing matching texture and lighting | must | no | no | — | M | Poisson/Laplacian blend; sample current & below |
| Patch tool (J) | Drag selection to patch from source; content-aware mode | must | no | no | — | M | Structure/colour adaptation sliders |
| Content-Aware Move (J) | Move or extend selected object, fill vacated area | edge | no | no | — | L | PatchMatch + blending |
| Red Eye tool (J) | Removes red-eye with pupil size/darken | edge | yes | no | — | S | |
| Brush (B) | Painting with presets, dynamics, smoothing, pressure | must | yes | no | — | XL | Brush engine: dab spacing, dynamics, tilt, smoothing, GPU compositing |
| Pencil (B) | Hard-edged aliased brush | edge | no | no | — | S | Pixel art |
| Color Replacement (B) | Paint new colour keeping luminance | edge | no | no | — | S | |
| Mixer Brush (B) | Wet-paint mixing with canvas colour | edge | no | no | — | L | Paint load/mix/wetness model |
| Adjustment Brush | Paint an adjustment; creates masked adjustment layer (25.9) | edge | yes | no | — | S | Wrapper around adjustment layer + mask |
| Clone Stamp (S) | Clone from sampled source; aligned, sample layers | must | yes | no | — | M | Clone Source panel: 5 sources, offset/scale/rotate, overlay |
| Pattern Stamp (S) | Paint with a pattern | edge | no | no | — | S | |
| History Brush (Y) | Paint back pixels from a history state/snapshot | edge | no | no | — | M | Requires addressable history states |
| Art History Brush (Y) | Stylised strokes sourced from history | deprecated | no | no | — | S | Legacy; rarely used |
| Eraser (E) | Erase to transparency/background; brush/pencil/block | must | yes | no | — | S | |
| Background Eraser (E) | Sampling, edge-aware erase to transparency | edge | no | no | — | S | |
| Magic Eraser (E) | Tolerance-based click erase | edge | yes | no | — | S | |
| Gradient tool (G) | On-canvas editable gradients; creates gradient fill layer | must | yes | no | — | M | Live gradient since 24.x; edit tool-made fills (27.6); methods: perceptual/linear/classic/smooth |
| Paint Bucket (G) | Tolerance flood fill with colour/pattern | must | yes | no | — | S | |
| 3D Material Drop (G) | Fill 3D material | deprecated | no | no | — | — | Removed with 3D |
| Blur tool | Paint local blur | edge | no | no | — | S | |
| Sharpen tool | Paint local sharpening; protect detail | edge | no | no | — | S | |
| Smudge tool | Finger-smear pixels | edge | no | no | — | S | |
| Dodge tool (O) | Paint to lighten; range + protect tones | must | no | no | — | S | Retouching staple |
| Burn tool (O) | Paint to darken; range + protect tones | must | no | no | — | S | |
| Sponge tool (O) | Paint saturation up/down; vibrance option | edge | no | no | — | S | |
| Pen tool (P) | Bezier path/shape drawing | must | no | no | — | M | Path booleans, rubber band |
| Freeform Pen (P) | Freehand path; magnetic option | edge | no | no | — | M | |
| Content-Aware Tracing tool (P) | Auto-trace edges into paths | deprecated | no | no | — | M | Technology Preview since 22.x; 2026 status (unverified) |
| Curvature Pen (P) | Click-to-curve path drawing | edge | no | no | — | S | |
| Add / Delete Anchor Point, Convert Point (P) | Edit path anchors and handles | must | no | no | — | S | |
| Horizontal Type (T) | Point/paragraph text layers | must | yes | no | — | XL | Text engine: OpenType shaping, composer, hyphenation |
| Vertical Type (T) | Vertical text (CJK) | edge | no | no | — | L | Vertical layout, tate-chu-yoko |
| Horizontal / Vertical Type Mask (T) | Type-shaped selections | edge | no | no | — | S | Reuses type engine |
| Path Selection (A) | Select whole paths/shapes | must | no | no | — | S | |
| Direct Selection (A) | Select/move individual anchors | must | no | no | — | S | |
| Rectangle (U) | Live shape with editable corner radii | must | yes | no | — | M | Vector shape layer rendering (AA, strokes, align) |
| Ellipse (U) | Live ellipse shape | must | yes | no | — | S | |
| Triangle (U) | Live triangle with corner radius | edge | no | no | — | S | |
| Polygon (U) | Polygon with sides, star ratio | edge | no | no | — | S | |
| Star tool | Dedicated regular-star shape tool (26.10) | edge | no | no | — | S | Per CG Channel 26.10 coverage |
| Line (U) | Line shapes with arrowheads | edge | yes | no | — | S | |
| Custom Shape (U) | Draw preset/custom vector shapes | edge | no | no | — | S | Needs shape library (.csh) |
| Hand (H) | Pan canvas; flick panning | must | yes | no | — | S | |
| Rotate View (R) | Rotate canvas view non-destructively | edge | no | no | — | S | |
| Zoom (Z) | Zoom; scrubby zoom, animated zoom | must | yes | no | — | S | Mip-mapped tile pyramid for big docs |
| Edit Toolbar | Customise toolbar groups/hide tools | edge | no | no | — | S | |
| Foreground/Background colour, swap, default | Current paint colours | must | yes | no | — | S | |
| Quick Mask mode (Q) | Edit selection as painted overlay | must | no | no | — | S | |
| Screen modes (F) | Standard / full screen with menus / full screen | edge | no | no | — | S | |

---

## 2. Menus

### 2a. File

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| New (document presets) | New doc with size, resolution, mode, depth, profile presets | must | yes | no | account (Stock templates) | S | Presets panel pulls Stock/Express templates |
| Open / Open As / Open Recent | Open files, force format, recent list | must | yes | no | — | S | |
| Open as Smart Object | Open file wrapped as a smart object | edge | no | no | — | M | |
| Browse in Bridge | Launch Adobe Bridge browser | bloat | no | no | account | — | Separate app |
| Close / Close All / Close Others | Close documents | must | yes | no | — | S | |
| Save / Save As / Save a Copy | Save; format list customisable (27.10) | must | yes | no | — | S | Save As split into Save a Copy (22.4 era) |
| Save to cloud document | Save as .psdc cloud document | bloat | no | no | account + cloud storage | — | Lock-in format |
| Revert | Revert to last saved | must | yes | no | — | S | |
| Version History | Browse/restore cloud document versions | bloat | no | no | account + cloud | M | Only for cloud docs |
| Invite to Edit / Share | Share cloud document with collaborators | bloat | no | no | account + cloud | — | |
| Export → Quick Export as PNG | One-click export using preferences | must | yes | no | — | S | |
| Export → Export As | Export doc/layers/artboards to PNG/JPEG/GIF/SVG at scales | must | yes | no | — | M | SVG option state in 2026 (unverified) |
| Export → Save for Web (Legacy) | Optimised JPEG/PNG/GIF/WBMP with preview, slices, animation | edge | no | no | — | M | Modernised dialog in 27.7; still the animated-GIF route |
| Export → Artboards/Layer Comps/Layers to Files | Batch export parts of doc | must | no | no | — | S | |
| Export → Artboards/Layer Comps to PDF | Multi-page PDF from artboards/comps | edge | no | no | — | M | |
| Export → Color Lookup Tables | Export adjustments as .cube/.3dl/.look/.icc LUTs | edge | no | no | — | M | |
| Export → Data Sets as Files | Render variable data sets to files | edge | no | no | — | M | |
| Export → Paths to Illustrator | Export paths as .ai | edge | no | no | — | S | |
| Export → Render Video | Render timeline to H.264/HEVC or image sequence | edge | no | no | — | L | Media encoder integration |
| Export → Zoomify | Tiled zoomable web export | deprecated | no | no | — | S | Legacy |
| Export to cloud account / Firefly | Send image to Firefly video/Boards (27.0, 27.5, 27.7) | bloat | no | no | account + cloud | — | |
| Generate → Image Assets | Auto-export layers named with extensions (Generator) | edge | no | no | — | M | Node-based Generator plug-in |
| Search Adobe Stock | Search Stock from within Photoshop | bloat | no | no | account + purchase | — | Stock browser added 27.10 |
| Place Embedded | Place file as embedded smart object | must | yes | no | — | M | PDF/AI/SVG rasterise-on-render |
| Place Linked | Place file as linked smart object | must | no | no | — | M | File watching + relink |
| Package | Collect linked files into folder | edge | no | no | — | S | |
| Automate → Batch | Run an action over a folder of files | must | no | no | — | M | Needs Actions engine |
| Automate → PDF Presentation | Build multi-page PDF/slideshow from images | edge | no | no | — | S | |
| Automate → Create Droplet | Make a drag-and-drop action executable | edge | no | no | — | M | OS integration |
| Automate → Crop and Straighten Photos | Split scanned multi-photo pages | edge | yes | no | — | M | Rectangle detection |
| Automate → Contact Sheet II | Thumbnail grid sheet from folder | edge | no | no | — | S | |
| Automate → Conditional Mode Change | Change mode depending on source mode in actions | edge | no | no | — | S | |
| Automate → Fit Image | Resize to fit bounding box | edge | yes | no | — | S | |
| Automate → Lens Correction | Batch lens profile correction | edge | no | no | — | M | Needs lens profile DB |
| Automate → Merge to HDR Pro | Merge bracketed exposures; deghost; tone map | edge | no | no | — | L | Radiance recovery, alignment, deghosting |
| Automate → Photomerge | Panorama stitch: Auto/Perspective/Cylindrical/Spherical/Collage/Reposition | edge | no | no | — | XL | Feature matching, bundle adjust, seam blending, content-aware edges |
| Scripts → Image Processor | Batch convert/resize to JPEG/PSD/TIFF | must | no | no | — | S | |
| Scripts → Load Files into Stack | Load files as layers, optional auto-align | edge | no | no | — | S | |
| Scripts → Statistics (stack modes) | Median/mean etc. across stack | edge | no | no | — | M | |
| Scripts → Delete All Empty Layers / Flatten All Effects / Flatten All Masks | Housekeeping scripts | edge | no | no | — | S | |
| Scripts → Export Layers to Files | Legacy per-layer export script | edge | no | no | — | S | |
| Scripts → Load Multiple DICOM Files | Load DICOM series | edge | no | no | — | M | |
| Scripts → Browse / Script Events Manager | Run JSX; trigger scripts on events | edge | no | no | — | L | ExtendScript DOM compatibility is huge |
| Import → Variable Data Sets | Import CSV/TXT data sets | edge | no | no | — | S | |
| Import → Video Frames to Layers | Video frames to layers | edge | no | no | — | M | Needs video decode |
| Import → Notes / WIA Support / Import from iPhone | Import annotations; scanners/cameras | edge | no | no | — | M | OS APIs |
| File Info | Edit XMP/IPTC/EXIF metadata | must | no | no | — | M | XMP round-trip |
| Print / Print One Copy | Print with colour management, soft proof | must | no | no | — | L | Print dialog §11 |
| Exit / Quit | Quit | must | yes | no | — | S | |

### 2b. Edit

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Undo / Redo / Toggle Last State | Multi-level undo (default) | must | yes | no | — | M | Tile-based copy-on-write undo for huge docs |
| Fade | Fade last filter/paint by opacity + blend mode | edge | no | no | — | S | |
| Cut / Copy / Copy Merged / Paste | Clipboard incl. merged copy | must | yes | no | — | S | OS clipboard formats |
| Paste Special → In Place / Into / Outside / Without Formatting | Paste variants creating masks | must | no | no | — | S | |
| Clear | Delete selected pixels | must | yes | no | — | S | |
| Search (Ctrl/Cmd+F) | Search UI, help, Stock, docs | edge | no | no | account (Stock/Learn) | S | |
| Check Spelling / Find and Replace Text | Text layer proofing | edge | no | no | — | M | Dictionaries per language |
| Fill (content-aware, colour, pattern, history) | Fill selection; script patterns | must | yes | no | — | M | Scripted patterns (brick, cross weave, spiral...) |
| Stroke | Stroke selection with colour/width/location | must | yes | no | — | S | |
| Content-Aware Fill (workspace) | Fill with sampling-area control, preview, output to layer | must | yes | no | — | XL | PatchMatch quality, colour/rotation/scale adaptation, mirror |
| Generative Fill | Prompted AI fill of selection; variations; generative layer | edge | yes | yes | cloud + credits (1 Firefly; 10–40 partner) | XL | Model choice dropdown since 27.0 |
| Content-Aware Scale | Resize protecting subjects/skin tones | edge | no | no | — | L | Seam carving + protect mask |
| Puppet Warp | Pin-based mesh deformation | edge | no | no | — | L | ARAP mesh deformation |
| Perspective Warp | Define planes and re-project perspective | edge | no | no | — | L | Quad-mesh warping |
| Free Transform | Scale/rotate/skew with reference point, interpolation options | must | yes | no | — | M | Smart object re-render from source |
| Transform → Scale/Rotate/Skew/Distort/Perspective | Individual transform modes | must | yes | no | — | S | |
| Transform → Warp (presets, custom grid, split) | Mesh warp with grid splits and presets | must | no | no | — | L | Bezier-patch warp, editable on smart objects |
| Transform → Rotate 180/90 CW/CCW, Flip H/V, Again | Quick transforms | must | yes | no | — | S | |
| Auto-Align Layers | Align layers: Auto/Perspective/Collage/Cylindrical/Spherical/Reposition | edge | no | no | — | L | Feature matching + lens correction |
| Auto-Blend Layers | Panorama blend or focus stack | edge | no | no | — | L | Focus stacking with seamless tones |
| Sky Replacement | Replace sky with preset/own image; relight foreground | edge | yes | yes | — (device) | L | Sky segmentation + colour harmonise; outputs layer group |
| Define Brush Preset / Pattern / Custom Shape | Create presets from selection/path | must | no | no | — | S | |
| Purge (Clipboard/Histories/All/Video cache) | Free memory | edge | no | no | — | S | |
| Adobe PDF Presets | Manage PDF export presets | edge | no | no | — | S | |
| Presets → Preset Manager / Export-Import / Migrate | Manage brushes, swatches, gradients, styles, patterns... | must | no | no | — | M | ABR/GRD/PAT/ASL parsers |
| Remote Connections | Allow external apps to drive Photoshop | edge | no | no | — | M | |
| Color Settings | Working spaces, policies, conversion options | must | no | no | — | M | |
| Assign Profile / Convert to Profile | Tag or convert document ICC profile | must | no | no | — | M | LCMS2-class CMM |
| Keyboard Shortcuts | Customise shortcuts for menus, panels, tools, taskspaces | must | no | no | — | S | |
| Menus (visibility/colour) | Hide/colour menu items | edge | no | no | — | S | |
| Toolbar | Customise toolbar | edge | no | no | — | S | |
| Preferences | App preferences (see §11) | must | no | no | — | M | |

### 2c. Image

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Mode | Colour mode + bit depth (see §6) | must | no | no | — | L | |
| Adjustments submenu | Destructive adjustments (see §3) | must | yes | no | — | M | |
| Auto Tone / Auto Contrast / Auto Color | One-click tonal/colour correction | must | yes | no | — | S | Algorithms selectable in Levels/Curves options |
| Image Size | Resample: Preserve Details 2.0, Bicubic variants, Nearest | must | yes | opt | — | M | Preserve Details 2.0 is ML-based upscaling (on device) |
| Canvas Size | Change canvas with anchor and extension colour | must | yes | no | — | S | |
| Image Rotation | Rotate/flip whole canvas, arbitrary angle | must | yes | no | — | S | |
| Crop / Trim / Reveal All | Crop to selection; trim transparent/colour; reveal off-canvas | must | yes | no | — | S | |
| Duplicate | Duplicate document | must | yes | no | — | S | |
| Apply Image | Blend a channel/layer into target with mode/mask | edge | no | no | — | M | Frequency separation, luminosity masks |
| Calculations | Combine channels into new channel/selection/doc | edge | no | no | — | M | |
| Variables → Define / Data Sets | Data-driven text/visibility/pixel replacement | edge | no | no | — | M | |
| Apply Data Set | Apply a data set to template | edge | no | no | — | S | |
| Trap | CMYK trapping for print | edge | no | no | — | S | |
| Analysis → Measurement Scale / Record Measurements / Data Points / Scale Marker | Scientific measurement of selections | edge | no | no | — | M | Measurement Log panel |
| Generative Upscale (entry point) | AI upscale via Firefly/Topaz models (27.0) | edge | yes | yes | cloud + credits | XL | Menu location (unverified); see §10 |

### 2d. Layer (commands; layer model detailed in §5)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| New → Layer / Background from Layer / Group / Group from Layers | Create layers and groups | must | yes | no | — | S | |
| New → Artboard / Artboard from Group/Layers | Create artboards | edge | no | no | — | M | |
| New → Frame from Layers | Convert layer to frame | edge | no | no | — | S | |
| New → Layer via Copy / Layer via Cut | Selection to new layer | must | yes | no | — | S | |
| Copy CSS / Copy SVG | Copy layer as CSS/SVG code | edge | no | no | — | M | |
| Duplicate / Delete / Rename Layer | Basic layer ops | must | yes | no | — | S | |
| Quick Export / Export As (layer) | Export selected layers | must | yes | no | — | S | |
| Layer Style submenu | Blending options, effects, copy/paste/clear, scale effects | must | no | no | — | XL | §5 |
| Smart Filter | Enable/disable/clear smart filter masks | must | no | no | — | M | |
| New Fill Layer → Solid Color / Gradient / Pattern | Editable fill layers | must | yes | no | — | S | |
| New Adjustment Layer | Non-destructive adjustments | must | yes | no | — | M | |
| Layer Content Options | Re-open fill/adjustment settings | must | no | no | — | S | |
| Layer Mask → Reveal/Hide All/Selection, From Transparency, Apply, Delete, Disable, Link | Pixel mask commands | must | yes | no | — | S | |
| Vector Mask commands | Create/delete/rasterise vector masks | must | no | no | — | M | |
| Create/Release Clipping Mask | Clip layer to layer below | must | yes | no | — | S | |
| Mask All Objects | Auto-mask every detected object in layer | edge | no | yes | — | L | Instance segmentation; one masked group per object |
| Smart Objects → Convert / New via Copy / Edit Contents / Replace / Export / Relink / Embed / Convert to Linked / Convert to Layers / Update Modified | Smart object management | must | no | no | — | XL | Nested PSD rendering, linked-file sync |
| Smart Objects → Stack Mode | Median, mean, max, entropy etc. over stacked layers | edge | no | no | — | M | Tourist removal/noise averaging |
| Smart Objects → Rasterize | Rasterise smart object | must | no | no | — | S | |
| Video Layers → New from File / Blank / Insert-Duplicate-Delete Frame / Replace Footage / Interpret / Restore | Video layer management | edge | no | no | — | L | Video decode pipeline |
| Rasterize (Type/Shape/Fill/Vector Mask/Smart Object/Video/Layer Style/All) | Convert to pixels | must | yes | no | — | S | |
| New Layer Based Slice | Slice from layer bounds | deprecated | no | no | — | S | |
| Group / Ungroup / Hide Layers | Organise | must | yes | no | — | S | |
| Arrange (front/forward/backward/back/reverse) | Change stacking order | must | yes | no | — | S | |
| Combine Shapes | Boolean shape combine | edge | no | no | — | M | Path booleans |
| Align / Distribute / Align to Selection | Align layers | must | yes | no | — | S | |
| Lock Layers | Lock pixels/transparency/position/artboard nesting/all | must | no | no | — | S | |
| Link / Select Linked Layers | Link layers to move together | edge | no | no | — | S | |
| Merge Down / Merge Visible / Flatten Image / Stamp Visible (shortcut) | Merge layers | must | yes | no | — | S | |
| Matting → Color Decontaminate / Defringe / Remove Black/White Matte | Clean fringes on cut-outs | edge | no | no | — | S | |
| Layer clean-up (auto delete empty, auto rename) | Remove empty layers, name unnamed layers (27.6) | edge | yes | opt | (unverified) | M | Content-based naming may use a vision model |

### 2e. Type

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| More from Adobe Fonts | Browse/activate Adobe Fonts | bloat | no | no | account | — | Fonts deactivate if subscription lapses |
| Font browser | In-app browsing of installed/Adobe fonts, class/tag filters (25.x/26.2) | edge | yes | no | account for Adobe Fonts | M | |
| Save and restore font lists | Save/restore font lists (27.9) | edge | no | no | — | S | Per CG Channel 27.9.1 coverage |
| Panels → Character / Paragraph / Glyphs / Character & Paragraph Styles | Open type panels | must | no | no | — | M | |
| Anti-Alias (None/Sharp/Crisp/Strong/Smooth/Platform LCD/Mac) | Text rasterisation mode | must | no | no | — | M | |
| Vector-based text rendering option | Vector text display, better CJK (26.11) | edge | no | no | — | M | |
| Orientation (Horizontal/Vertical) | Change text direction | edge | no | no | — | L | |
| OpenType features (ligatures, alternates, fractions, stylistic sets...) | OpenType substitution | must | no | no | — | L | rustybuzz/HarfBuzz shaping |
| Create Work Path / Convert to Shape | Text outlines to paths/shapes | edge | no | no | — | M | |
| Rasterize Type Layer | Convert text to pixels | must | yes | no | — | S | |
| Convert to Point / Paragraph Text | Switch text box type | must | no | no | — | S | |
| Dynamic Text | Auto-fit text to box; flow along circles/paths/shapes (26.8→27.4→27.10) | edge | yes | no | — | L | Reflow + fit-to-path solver |
| Warp Text | Arc/Flag/Wave etc. text warps | must | yes | no | — | M | |
| Match Font | Identify font from image, suggest similar | edge | yes | yes | account for Adobe Fonts matches | L | OCR + font classifier |
| Font Preview Size | Preview size in font menu | edge | no | no | — | S | |
| Language Options (World-Ready/East Asian/Middle Eastern & South Asian) | Script-specific composers | edge | no | no | — | XL | RTL/Indic shaping, justification |
| Update All Text Layers / Manage Missing Fonts | Re-render text; substitute missing fonts | must | no | no | — | M | Missing-font substitution matters for PSD import |
| Paste Lorem Ipsum | Placeholder text | edge | no | no | — | S | |
| Load / Save Default Type Styles | Default type style management | edge | no | no | — | S | |
| Extrude to 3D | 3D type | deprecated | no | no | — | — | Removed with 3D |

### 2f. Select

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| All / Deselect / Reselect / Inverse | Basic selection commands | must | yes | no | — | S | |
| All Layers / Deselect Layers / Find Layers / Isolate Layers | Layer selection helpers | must | no | no | — | S | |
| Color Range | Select by sampled colours, skin tones, highlights/midtones/shadows, faces | must | no | opt | — | M | Detect Faces option uses face detector |
| Focus Area | Select in-focus regions | edge | no | no | — | M | Local sharpness estimation |
| Subject | One-click main subject selection; device or cloud | must | yes | yes | cloud option (26.6) | L | Portrait-aware hair refinement |
| Sky | One-click sky selection | edge | yes | yes | — | L | |
| Select People (options bar / Select Details) | Select people and their parts (26.6) | edge | yes | yes | (unverified) | L | |
| Remove Background (quick action) | Subject selection → layer mask | must | yes | yes | cloud option (26.6) | L | Properties/Contextual Task Bar |
| Select and Mask (workspace) | Refine edges: smart radius, refine hair, decontaminate, output | must | no | opt | — | XL | Matting quality on hair; Object Aware refine mode |
| Modify → Border / Smooth / Expand / Contract / Feather | Morphological selection ops | must | no | no | — | S | |
| Grow / Similar | Expand selection by tolerance | edge | no | no | — | S | |
| Transform Selection | Transform marching-ants selection | must | no | no | — | S | |
| Edit in Quick Mask Mode | Paint selection as overlay | must | no | no | — | S | |
| Load Selection / Save Selection | Alpha channel round-trip | must | no | no | — | S | |
| New 3D Extrusion | 3D extrusion from selection | deprecated | no | no | — | — | Removed |

### 2g. Filter, 3D, View, Plugins, Window, Help

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Filter menu (all entries) | See §4 for every filter | must | yes | opt | mixed | XL | |
| 3D menu (entire) | 3D layers, meshes, materials, rendering | deprecated | no | no | — | — | Discontinued from 22.5; menu/prefs removed by 25.4; no longer present in 2026 |
| View → Proof Setup / Proof Colors / Gamut Warning | Soft-proof display | must | no | no | — | M | §6 |
| View → Pixel Aspect Ratio / Correction | Non-square pixel display | edge | no | no | — | S | Video legacy |
| View → 32-bit Preview Options | Tone-map display of 32-bit docs | edge | no | no | — | M | |
| View → Zoom In/Out, Fit on Screen, 100%, 200%, Print Size | Zoom commands | must | yes | no | — | S | |
| View → Flip Horizontal | Flip view (not pixels) | edge | no | no | — | S | |
| View → Pattern Preview | Live tile preview for pattern design | edge | no | no | — | S | |
| View → Screen Mode | Screen modes | edge | no | no | — | S | |
| View → Extras / Show (Layer Edges, Selection Edges, Target Path, Grid, Guides, Smart Guides, Slices, Notes, Pixel Grid, Mesh, Brush Preview...) | Toggle overlays | must | no | no | — | S | |
| View → Rulers / Snap / Snap To | Rulers and snapping targets | must | no | no | — | S | |
| View → Guides (New Guide, New Guide Layout, from Shape, Lock, Clear) | Guide management | must | no | no | — | S | |
| View → Lock Slices / Clear Slices | Slice management | deprecated | no | no | — | S | |
| Plugins → Plugins Panel / Browse Plugins / Manage Plugins | UXP plugin marketplace | edge | no | no | account (Creative Cloud marketplace) | L | UXP JS API surface |
| Plugins → installed plug-ins (8BF/UXP) | Third-party filters/panels | edge | no | no | — | XL | 8BF C API compatibility would be huge |
| Window → Arrange (tabs, tile, float, match zoom/location) | Document window layout | must | no | no | — | M | |
| Window → Workspace (Essentials, Photography, Painting, Graphic & Web, Motion, 3D gone; New/Reset/Delete) | Workspaces | must | no | no | — | M | §11 |
| Window → Extensions (legacy) | CEP extension panels | deprecated | no | no | — | — | CEP being phased out for UXP |
| Window → panel toggles | Show/hide each panel (§7) | must | no | no | — | S | |
| Help → Photoshop Help / Tutorials / Discover / Updates / System Info / GPU Compatibility / Manage Extensions / Content Credentials info | Help, learning, diagnostics | edge | no | no | account (Discover) | S | |
| Unified Account Menu | Account/plan/credits menu in Edit workspace (27.7) | bloat | no | no | account | — | |

---

## 3. Adjustments (Image → Adjustments and adjustment layers)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Brightness/Contrast | Simple brightness and contrast; legacy mode | must | yes | no | — | S | |
| Levels | Input/output black, white, gamma per channel; auto options | must | yes | no | — | S | Eyedroppers set black/grey/white points |
| Curves | Point curves per channel; targeted adjustment tool; presets | must | yes | no | — | M | Spline matching Adobe's curve interpolation for PSD fidelity |
| Exposure | Exposure, offset, gamma (32-bit friendly) | must | no | no | — | S | Linear-light maths |
| Vibrance → Color and Vibrance | Vibrance, saturation, plus Temperature/Tint (27.0 redesign) | must | yes | no | — | S | |
| Hue/Saturation | Hue/sat/lightness per colour range; colorize; on-image tool | must | yes | no | — | M | Range falloff sliders |
| Color Balance | Shift shadows/midtones/highlights colour; preserve luminosity | must | yes | no | — | S | |
| Black & White | Channel-weighted greyscale conversion with tint | must | yes | no | — | S | |
| Photo Filter | Warming/cooling/colour filter with density | edge | yes | no | — | S | |
| Channel Mixer | Mix source channels into outputs; monochrome | edge | no | no | — | S | |
| Color Lookup | Apply 3D LUT (.cube/.3dl/.look), abstract/device link profiles | edge | yes | no | — | M | 3D LUT trilinear/tetrahedral on GPU |
| Invert | Invert colours | must | yes | no | — | S | |
| Posterize | Reduce tonal levels | edge | no | no | — | S | |
| Threshold | Two-tone by threshold level | edge | no | no | — | S | |
| Gradient Map | Map luminance to gradient; dither; method | edge | yes | no | — | S | |
| Selective Color | CMYK-style per-colour-family adjustment (relative/absolute) | must | no | no | — | M | Exact Adobe maths needed for PSD fidelity |
| Clarity and Dehaze (adjustment layer) | Midtone contrast and haze removal (27.3) | must | yes | no | — | M | Local contrast + dark-channel-style dehaze, matching ACR |
| Grain (adjustment layer) | Film grain: amount, size, roughness (27.3) | edge | yes | no | — | S | Deterministic seeded grain |
| Light (adjustment layer) | Exposure, contrast, highlights, shadows, whites, blacks (27.10) | must | yes | no | — | M | ACR-style local tone mapping on layers |
| Shadows/Highlights (destructive/smart filter) | Local shadow/highlight recovery with radius | must | yes | no | — | M | Not an adjustment layer; smart filter only |
| HDR Toning (destructive) | Local-adapt tone mapping; flattens document | edge | no | no | — | M | Flattens; not on layers |
| Desaturate | Remove colour | edge | yes | no | — | S | |
| Match Color | Match colour statistics between images/layers | edge | no | no | — | M | |
| Replace Color | Select colour range and shift HSL | edge | no | no | — | S | |
| Equalize | Histogram equalisation | edge | no | no | — | S | |
| Auto Tone / Auto Contrast / Auto Color | Automatic corrections (Levels/Curves algorithms) | must | yes | no | — | S | |
| Adjust Colors (on-canvas) | HSL on 6 auto-detected dominant colours via task bar (26.6) | edge | yes | no | — | M | Colour clustering; not a model per sources |
| Adjustment Presets | One-click preset looks as adjustment layer groups (25.0; redesigned 26.5) | edge | yes | no | — | S | Tabbed groups, drag to organise |
| Adjustment presets (Curves/Levels/Hue-Sat/B&W etc. .acv/.alv/.ahu files) | Save/load per-adjustment preset files | must | no | no | — | S | File-format parsers |
| Solid Color / Gradient / Pattern fill layers | Editable fill layers (see §5) | must | yes | no | — | S | |

---

## 4. Filters

### 4a. Top-level and special filters

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Last Filter / Fade | Re-run last filter; fade it | edge | no | no | — | S | |
| Convert for Smart Filters | Turn layer into smart object for live filters | must | no | no | — | M | |
| Neural Filters (workspace) | Model-driven filters, downloaded on demand | edge | yes | yes | account; models download from cloud; some cloud processing | XL | Largely superseded by Firefly features; update cadence (unverified) |
| Filter Gallery | Stack Artistic/Brush/Sketch/Texture effects with preview | edge | yes | no | — | L | 47 legacy effects; exact look is hard to match |
| Adaptive Wide Angle | Straighten curved lines from fisheye/wide lenses | edge | no | no | — | L | Needs lens model + constraint solve |
| Camera Raw Filter | Apply ACR controls to any layer (see §9) | must | yes | opt | cloud for generative parts | XL | |
| Lens Correction | Profile-based distortion/CA/vignette; custom corrections | must | no | no | — | L | Lens profile DB (LCP) |
| Liquify | Forward warp, reconstruct, smooth, twirl, pucker, bloat, push, freeze/thaw | must | yes | no | — | L | GPU displacement mesh, brush pressure, masks |
| Liquify → Face-Aware Liquify | Sliders for eyes, nose, mouth, face shape | edge | yes | yes | — | L | Face landmark model |
| Vanishing Point | Clone/paint/paste in perspective planes | edge | no | no | — | L | Plane grid + perspective clone |
| Blur Gallery (workspace) | On-canvas pins for blur types, bokeh, noise restore | edge | yes | no | — | L | |
| Video → De-Interlace | Remove interlace lines | edge | no | no | — | S | |
| Video → NTSC Colors | Clamp to broadcast-safe colours | deprecated | no | no | — | S | |
| 3D → Generate Bump/Normal Map | Texture maps from image | deprecated | no | no | — | — | Removed with 3D (unverified whether any remnant stays) |
| Sharpen → Shake Reduction | Deconvolve camera-shake blur | deprecated | no | no | — | — | Removed in Photoshop 23.3 |

### 4b. Neural Filters (status per third-party listing; helpx list not fetchable → treat statuses as unverified)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Skin Smoothing (featured) | Removes blemishes/acne, blur/smoothness sliders | edge | yes | yes | model download | L | |
| Smart Portrait (beta) | GAN edits: expression, age, gaze, hair, head/light direction | edge | yes | yes | cloud processing for some operations | XL | Face GAN; ethically fraught |
| Makeup Transfer (beta) | Transfer makeup from reference face | edge | yes | yes | model download | XL | |
| Landscape Mixer (beta) | Blend landscape with reference/season/time sliders | edge | yes | yes | cloud (unverified) | XL | |
| Style Transfer (featured) | Apply artistic style from preset/reference | edge | yes | yes | model download | L | |
| Harmonization (beta) | Match colour/tone of layer to another | edge | yes | yes | model download | M | Superseded by Harmonize (27.0) |
| Color Transfer (beta) | Transfer palette from reference image | edge | yes | yes | model download | M | |
| Colorize (featured) | Auto-colourise greyscale photos | edge | yes | yes | cloud (unverified) | L | |
| Super Zoom (featured) | Zoom/crop with ML upscaling, JPEG/noise removal | edge | yes | yes | cloud (unverified) | L | |
| Depth Blur (beta) | Depth-map-based background blur and haze | edge | yes | yes | model download | L | Monocular depth model |
| JPEG Artifacts Removal (featured) | Remove compression artefacts | edge | yes | yes | model download | M | |
| Photo Restoration (beta) | Repair scratches/fade, enhance faces in old photos | edge | yes | yes | model download | L | |
| Wait-list filters (e.g. Photo to Sketch, Water Color) | Proposed filters users vote on | deprecated | no | yes | — | — | Names/status (unverified) |

### 4c. Blur Gallery

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Field Blur | Multi-pin blur amount gradient | edge | yes | no | — | M | |
| Iris Blur | Elliptical focus falloff blur | edge | yes | no | — | M | |
| Tilt-Shift | Linear focus band, distortion | edge | yes | no | — | M | |
| Path Blur | Motion blur along drawn paths; strobe | edge | no | no | — | L | |
| Spin Blur | Rotational blur (wheels) | edge | no | no | — | M | |
| Blur Gallery Effects/Noise panels | Light bokeh, colour, noise restoration | edge | no | no | — | M | |

### 4d. Blur

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Average | Fill with average colour | edge | no | no | — | S | |
| Blur / Blur More | Fixed small blur | edge | no | no | — | S | |
| Box Blur | Box-kernel blur | edge | no | no | — | S | |
| Gaussian Blur | Gaussian blur with radius | must | yes | no | — | S | Separable GPU pass |
| Lens Blur (filter) | Depth-map bokeh with iris shape, specular highlights | edge | no | no | — | M | |
| Motion Blur | Directional blur | must | yes | no | — | S | |
| Radial Blur | Spin/zoom blur | edge | no | no | — | S | |
| Shape Blur | Blur with custom shape kernel | edge | no | no | — | S | |
| Smart Blur | Edge-preserving blur; edge-only modes | edge | no | no | — | S | |
| Surface Blur | Bilateral-style edge-preserving blur | edge | no | no | — | S | |

### 4e. Distort

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Displace | Displace pixels by a PSD displacement map | edge | no | no | — | S | |
| Pinch | Squeeze inward/outward | edge | no | no | — | S | |
| Polar Coordinates | Rectangular ↔ polar mapping | edge | no | no | — | S | |
| Ripple | Ripple distortion | edge | no | no | — | S | |
| Shear | Bend along a curve | edge | no | no | — | S | |
| Spherize | Spherical bulge | edge | no | no | — | S | |
| Twirl | Twirl around centre | edge | no | no | — | S | |
| Wave | Configurable wave generators | edge | no | no | — | S | |
| ZigZag | Radial ripple | edge | no | no | — | S | |

### 4f. Noise

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Add Noise | Uniform/Gaussian noise, monochromatic | must | no | no | — | S | |
| Despeckle | Mild edge-preserving blur | edge | no | no | — | S | |
| Dust & Scratches | Median-based defect removal with threshold | must | no | no | — | S | |
| Median | Median filter | edge | no | no | — | S | |
| Reduce Noise | Luminance/colour noise and JPEG artefacts | edge | yes | no | — | M | Weaker than ACR Denoise |

### 4g. Pixelate

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Color Halftone | Per-channel halftone dots | edge | no | no | — | S | |
| Crystallize | Voronoi cell clustering | edge | no | no | — | S | |
| Facet | Painted solid clusters | edge | no | no | — | S | |
| Fragment | Offset copies | edge | no | no | — | S | |
| Mezzotint | Random dot/line/stroke patterns | edge | no | no | — | S | |
| Mosaic | Square pixel blocks | edge | yes | no | — | S | Redaction use |
| Pointillize | Random dots on background | edge | no | no | — | S | |

### 4h. Render

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Flame | Procedural flames along a path | edge | no | no | — | M | |
| Picture Frame | Procedural decorative frames | edge | no | no | — | M | |
| Tree | Procedural trees | edge | no | no | — | M | |
| Clouds | Perlin-style clouds from fg/bg colours | edge | no | no | — | S | |
| Difference Clouds | Clouds blended by difference | edge | no | no | — | S | |
| Fibers | Fibre texture | edge | no | no | — | S | |
| Lens Flare | Lens flare types with brightness | edge | yes | no | — | S | |
| Lighting Effects (workspace) | Spot/point/infinite lights, texture channel bump | edge | no | no | — | M | GPU lit relief |

### 4i. Sharpen

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Sharpen / Sharpen More / Sharpen Edges | Fixed sharpening | edge | no | no | — | S | |
| Smart Sharpen | Gaussian/lens/motion blur removal with shadow/highlight fade | must | yes | no | — | M | |
| Unsharp Mask | Amount/radius/threshold USM | must | yes | no | — | S | |

### 4j. Stylize

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Diffuse | Shuffle pixels for soft focus | edge | no | no | — | S | |
| Emboss | Relief from edges | edge | no | no | — | S | |
| Extrude | 3D blocks/pyramids | edge | no | no | — | S | |
| Find Edges | Edge outline | edge | no | no | — | S | |
| Oil Paint | Painterly GPU filter with lighting | edge | yes | no | — | M | |
| Solarize | Blend negative/positive | edge | no | no | — | S | |
| Tiles | Break into tiles | edge | no | no | — | S | |
| Trace Contour | Contour lines at level | edge | no | no | — | S | |
| Wind | Wind streaks | edge | no | no | — | S | |

### 4k. Other

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Custom | User 5×5 convolution kernel | edge | no | no | — | S | |
| High Pass | High-pass detail extraction | must | no | no | — | S | Frequency separation/sharpening staple |
| HSB/HSL | Convert channels to HSB/HSL encoding | edge | no | no | — | S | |
| Maximum | Morphological dilate (square/round) | edge | no | no | — | S | |
| Minimum | Morphological erode | edge | no | no | — | S | |
| Offset | Wrap-around offset for tiling | edge | no | no | — | S | |

### 4l. Filter Gallery effects (8-bit RGB only; all `edge`, Lite `no` unless noted, no AI, no cloud, difficulty S–M; hard part is matching Adobe's exact look)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Artistic → Colored Pencil | Pencil-drawn look | edge | no | no | — | S | |
| Artistic → Cutout | Cut paper posterised shapes | edge | no | no | — | S | |
| Artistic → Dry Brush | Dry brush edges | edge | no | no | — | S | |
| Artistic → Film Grain | Grain with highlight area | edge | no | no | — | S | |
| Artistic → Fresco | Coarse dabbed paint | edge | no | no | — | S | |
| Artistic → Neon Glow | Colour glow | edge | no | no | — | S | |
| Artistic → Paint Daubs | Brush daubs | edge | no | no | — | S | |
| Artistic → Palette Knife | Knife-painted look | edge | no | no | — | S | |
| Artistic → Plastic Wrap | Shiny plastic coating | edge | no | no | — | S | |
| Artistic → Poster Edges | Posterise + black edges | edge | no | no | — | S | |
| Artistic → Rough Pastels | Pastel on texture | edge | no | no | — | M | |
| Artistic → Smudge Stick | Smudged darks | edge | no | no | — | S | |
| Artistic → Sponge | Sponge texture | edge | no | no | — | S | |
| Artistic → Underpainting | Painted-under texture | edge | no | no | — | M | |
| Artistic → Watercolor | Watercolour wash | edge | yes | no | — | S | |
| Brush Strokes → Accented Edges | Accentuated edges | edge | no | no | — | S | |
| Brush Strokes → Angled Strokes | Diagonal strokes | edge | no | no | — | S | |
| Brush Strokes → Crosshatch | Crosshatch strokes | edge | no | no | — | S | |
| Brush Strokes → Dark Strokes | Dark/light strokes | edge | no | no | — | S | |
| Brush Strokes → Ink Outlines | Ink outline drawing | edge | no | no | — | S | |
| Brush Strokes → Spatter | Airbrush spatter | edge | no | no | — | S | |
| Brush Strokes → Sprayed Strokes | Angled sprayed strokes | edge | no | no | — | S | |
| Brush Strokes → Sumi-e | Japanese ink brush | edge | no | no | — | S | |
| Distort → Diffuse Glow | Soft diffused glow | edge | no | no | — | S | |
| Distort → Glass | Glass texture distortion | edge | no | no | — | S | |
| Distort → Ocean Ripple | Underwater ripples | edge | no | no | — | S | |
| Sketch → Bas Relief | Low relief carving | edge | no | no | — | S | |
| Sketch → Chalk & Charcoal | Chalk/charcoal drawing | edge | no | no | — | S | |
| Sketch → Charcoal | Charcoal drawing | edge | no | no | — | S | |
| Sketch → Chrome | Polished chrome | edge | no | no | — | S | |
| Sketch → Conté Crayon | Conté crayon texture | edge | no | no | — | S | |
| Sketch → Graphic Pen | Ink pen strokes | edge | no | no | — | S | |
| Sketch → Halftone Pattern | Halftone screen | edge | no | no | — | S | |
| Sketch → Note Paper | Handmade paper | edge | no | no | — | S | |
| Sketch → Photocopy | Photocopy look | edge | no | no | — | S | |
| Sketch → Plaster | Moulded plaster | edge | no | no | — | S | |
| Sketch → Reticulation | Film emulsion reticulation | edge | no | no | — | S | |
| Sketch → Stamp | Rubber stamp | edge | no | no | — | S | |
| Sketch → Torn Edges | Torn paper | edge | no | no | — | S | |
| Sketch → Water Paper | Blotted paper | edge | no | no | — | S | |
| Stylize → Glowing Edges | Neon edges | edge | no | no | — | S | |
| Texture → Craquelure | Cracked plaster | edge | no | no | — | S | |
| Texture → Grain | Grain types | edge | no | no | — | S | |
| Texture → Mosaic Tiles | Mosaic tiles with grout | edge | no | no | — | S | |
| Texture → Patchwork | Square patches | edge | no | no | — | S | |
| Texture → Stained Glass | Stained-glass cells | edge | no | no | — | S | |
| Texture → Texturizer | Apply brick/burlap/canvas/sandstone texture | edge | no | no | — | S | |

---

## 5. Layers system

### 5a. Layer types

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Pixel layer | Raster layer with transparency | must | yes | no | — | M | Sparse tiled storage, unbounded (off-canvas) pixels |
| Background layer | Locked opaque bottom layer | must | yes | no | — | S | JPEG can import as normal layer (27.7 option) |
| Adjustment layer | Non-destructive adjustment with built-in mask | must | yes | no | — | M | PSD stores descriptors per adjustment |
| Fill layer (solid/gradient/pattern) | Editable fill with mask | must | yes | no | — | S | |
| Smart object – embedded | Nested document; non-destructive transforms/filters | must | no | no | — | XL | Nested PSB render + transform from source |
| Smart object – linked (file) | References external file, updates on change | edge | no | no | — | L | |
| Smart object – linked (Library) | References CC Library asset | bloat | no | no | account + cloud | — | |
| Type layer | Live editable text | must | yes | no | — | XL | PSD EngineData parse/re-render |
| Shape layer | Live vector shape with fill/stroke | must | yes | no | — | M | |
| Video layer | Video footage as layer on timeline | edge | no | no | — | L | |
| Frame layer | Placeholder frame masking content | edge | no | no | — | M | |
| Artboard | Canvas-like container with own bounds/background | edge | no | no | — | M | |
| Group (layer set) | Folder; pass-through or isolated blending; group masks | must | yes | no | — | M | Pass Through vs Normal semantics |
| Generative layer | Layer holding AI variations + prompt (Gen Fill/Expand) | edge | yes | yes | cloud + credits | M | UI for variation switching; generation itself XL |
| 3D layer | 3D model layer | deprecated | no | no | — | — | Removed |

### 5b. Masks and clipping

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Layer (pixel) mask | Greyscale mask hiding layer areas | must | yes | no | — | S | |
| Vector mask | Path-based resolution-independent mask | must | no | no | — | M | |
| Clipping mask | Layer visible only through base layer's alpha | must | yes | no | — | S | |
| Mask Density / Feather (Properties) | Non-destructive mask density and feather | must | no | no | — | S | |
| Mask → Select and Mask / Color Range / Invert | Refine mask edges from Properties | must | no | opt | — | L | |
| Link/unlink mask to layer | Move mask independently | must | no | no | — | S | |
| Filter mask (smart filters) | Mask for smart filter stack | must | no | no | — | S | |
| Quick Mask | Temporary selection mask | must | no | no | — | S | |

### 5c. Blend modes (27 layer modes + extras)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Normal, Dissolve | Standard and random-dither compositing | must | yes | no | — | S | Dissolve needs Adobe's dither pattern for fidelity |
| Darken group: Darken, Multiply, Color Burn, Linear Burn, Darker Color | Darkening modes | must | yes | no | — | S | |
| Lighten group: Lighten, Screen, Color Dodge, Linear Dodge (Add), Lighter Color | Lightening modes | must | yes | no | — | S | |
| Contrast group: Overlay, Soft Light, Hard Light, Vivid Light, Linear Light, Pin Light, Hard Mix | Contrast modes | must | yes | no | — | S | Soft Light formula must match Adobe's, not W3C |
| Inversion group: Difference, Exclusion, Subtract, Divide | Comparative modes | must | no | no | — | S | |
| Component group: Hue, Saturation, Color, Luminosity | HSY component modes | must | yes | no | — | S | Adobe's non-separable luma maths |
| 32-bit mode subset | Only some modes available in 32-bit | edge | no | no | — | S | Linear-light compositing |
| Pass Through (groups) | Group contents blend with layers below | must | no | no | — | M | |
| Behind / Clear (painting only) | Paint behind existing pixels / to transparency | edge | no | no | — | S | |
| Blend Mode preview on hover | Live preview while scrolling modes | edge | yes | no | — | S | Needs fast GPU recomposite |
| Gamma for blending (Color Settings: blend RGB using gamma 1.0; text gamma) | Linear vs gamma-space blending option | edge | no | no | — | S | |

### 5d. Layer styles (Layer Style dialog)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Drop Shadow (multiple instances) | Offset blurred shadow; spread, contour, noise, knockout | must | yes | no | — | L | Precise Adobe spread/contour/noise reproduction |
| Inner Shadow (multiple) | Shadow inside edges; choke | must | no | no | — | L | |
| Outer Glow | Glow outside; softer/precise technique, range, jitter | edge | no | no | — | L | Precise technique = distance transform |
| Inner Glow | Glow inside from edge/centre | edge | no | no | — | L | |
| Bevel & Emboss (+Contour, +Texture) | Inner/outer bevel, emboss, pillow, stroke emboss; gloss contour | edge | no | no | — | XL | Chisel hard/soft; hardest effect to match |
| Satin | Interior satin shading | edge | no | no | — | M | |
| Color Overlay (multiple) | Solid colour fill with mode/opacity | must | yes | no | — | S | |
| Gradient Overlay (multiple) | Gradient fill with styles, reverse, dither, method | must | no | no | — | M | |
| Pattern Overlay | Pattern fill with scale/link | edge | no | no | — | S | |
| Stroke (multiple) | Outside/inside/centre stroke with colour/gradient/pattern | must | yes | no | — | M | Distance field for accurate stroke |
| Global Light | Shared light angle/altitude across styles | edge | no | no | — | S | |
| Scale Effects / Copy-Paste-Clear Style / Create Layers from style | Manage styles | edge | no | no | — | S | Create Layers = rasterise effects to layers |
| Styles panel presets (.asl) | Save/apply style presets | edge | yes | no | — | M | ASL descriptor parser |
| Contours (preset curves) | Contour curves for glows/shadows/bevels | edge | no | no | — | S | |

### 5e. Blending options and layer management

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Opacity vs Fill | Fill fades pixels not effects; special modes behave differently | must | no | no | — | M | "Special eight" modes differ under Fill |
| Knockout (none/shallow/deep) | Punch through to group/background | edge | no | no | — | M | |
| Channels R/G/B checkboxes | Restrict blending to channels | edge | no | no | — | S | |
| Blend Interior Effects as Group | Inner effects blend with layer mode | edge | no | no | — | M | |
| Blend Clipped Layers as Group | Clipped layers use base mode | edge | no | no | — | M | |
| Transparency Shapes Layer / Layer Mask Hides Effects / Vector Mask Hides Effects | Advanced effect-mask interaction | edge | no | no | — | M | |
| Blend If (This Layer / Underlying, split sliders, per channel) | Luminosity/channel-based blending ranges | must | no | no | — | M | Photographers use for sky blending |
| Layer Comps (panel) | Save visibility/position/appearance/style states | edge | no | no | — | M | |
| Smart filters (stack, per-filter blending options, re-edit) | Non-destructive filter stack on smart objects | must | no | no | — | L | Every filter must re-render deterministically |
| Lock transparent pixels | Paint only on existing pixels | must | no | no | — | S | |
| Lock image pixels | Prevent painting | must | no | no | — | S | |
| Lock position | Prevent moving | must | no | no | — | S | |
| Prevent auto-nesting in/out of artboards | Artboard lock | edge | no | no | — | S | |
| Lock all | Fully lock layer | must | no | no | — | S | |
| Link layers | Move/transform together | edge | no | no | — | S | |
| Layer filtering (Kind, Name, Effect, Mode, Attribute, Color, Smart Object, Selected, Artboard) | Filter the Layers panel | edge | no | no | — | S | |
| Colour labels, thumbnails size, layer search, Isolate Layers | Panel organisation | edge | no | no | — | S | |
| Stamp Visible (Ctrl/Cmd+Alt/Opt+Shift+E) | Merge visible to new layer | must | no | no | — | S | |
| Auto-select / right-click layer pick | Select layer under cursor | must | yes | no | — | S | |

---

## 6. Channels, paths, colour modes, bit depth, colour management

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Channels panel (composite + colour channels) | View/edit individual colour channels | must | no | no | — | S | |
| Alpha channels | Store selections as greyscale channels | must | no | no | — | S | |
| Spot channels | Spot ink plates with solidity, custom inks | edge | no | no | — | M | Print separations; PSD/DCS/TIFF/PDF output |
| Split Channels / Merge Channels | Separate channels into docs and back | edge | no | no | — | S | |
| Channel-based luminosity selections (Ctrl-click) | Load channel luminance as selection | must | no | no | — | S | |
| Paths panel (Work Path, saved paths) | Store/manage Bezier paths | must | no | no | — | S | |
| Fill / Stroke Path (incl. simulate pressure) | Render paths with brush tools | edge | no | no | — | M | Stroking with brush engine |
| Make Selection from path / Make Work Path from selection | Path ↔ selection conversion | must | no | no | — | M | Vectorising selection edges |
| Clipping Path (export) | Mark path as clipping path for layout apps | edge | no | no | — | S | Stored in TIFF/EPS/PSD |
| Bitmap mode | 1-bit: 50% threshold, pattern, diffusion, halftone | edge | no | no | — | S | |
| Grayscale mode | Single-channel greyscale with dot gain/gamma profile | must | no | no | — | S | |
| Duotone mode (mono/duo/tri/quadtone) | Ink curves per plate | edge | no | no | — | M | Curves + preset inks |
| Indexed Color mode | Palette-based colour with dither options and Color Table | edge | no | no | — | M | Quantisation algorithms (perceptual/selective/adaptive) |
| RGB mode | Standard RGB working space | must | yes | no | — | S | |
| CMYK mode | Four-plate print colour | must | no | no | — | L | CMM conversions, GCR/UCR, total ink limit |
| Lab mode | Device-independent L*a*b* editing | edge | no | no | — | M | |
| Multichannel mode | Channels as independent spot plates | edge | no | no | — | S | |
| 8 bits/channel | Standard depth | must | yes | no | — | S | |
| 16 bits/channel | High-precision editing (15-bit internally) | must | no | no | — | M | Every filter/blend in 16-bit; Adobe uses 0–32768 |
| 32 bits/channel (float HDR) | Linear float HDR editing with limited tools | edge | no | no | — | L | Better 32-bit support added 26.0 |
| HDR/EDR display of 32-bit and HDR docs | Show HDR on capable displays | edge | no | no | — | L | Platform HDR swapchain |
| Color Settings (working spaces, policies, gray/spot dot gain) | Global colour management config | must | no | no | — | M | |
| ICC profile embed/assign/convert | Tagging and converting via CMM | must | no | no | — | M | |
| Rendering intents + Black Point Compensation | Perceptual/relative/saturation/absolute conversions | must | no | no | — | M | |
| Profile mismatch / missing profile dialogs | Handle untagged/mismatched files on open/paste | must | no | no | — | S | |
| Proof Setup (Working CMYK, custom device, colour-blindness proofs) | Configure soft proof | must | no | no | — | M | |
| Proof Colors (soft proof toggle) | Preview output device appearance | must | no | no | — | M | |
| Gamut Warning | Grey overlay on out-of-gamut colours | edge | no | no | — | S | |
| OpenColorIO (OCIO) colour management | OCIO configs for VFX pipelines (26.0; OCIO 2.5 + ACES 2.0 in 27.8) | edge | no | no | — | L | OCIO has C++ only; would need bindings or port |
| Color Picker (+HUD picker, only web colours, Lab/CMYK/HSB fields) | Pick colours in any model | must | yes | no | — | S | |
| Color Libraries (Pantone etc.) | Named spot colour books | edge | no | no | account (Pantone Connect paid since 2022) | S | Licensing seam, not code |
| Color Table (indexed) | Edit indexed palette | edge | no | no | — | S | |
| Dither on 8-bit gradients/conversions | Reduce banding | edge | no | no | — | S | |

---

## 7. Panels

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Layers | Layer stack, modes, opacity/fill, locks, filters | must | yes | no | — | M | |
| Properties | Context settings for layers, adjustments, masks, type, doc; Quick Actions | must | yes | no | — | M | Quick Actions: Remove Background, Select Subject |
| Adjustments | Adjustment layer buttons + presets | must | yes | no | — | S | |
| Contextual Task Bar | Floating next-step actions (select subject, gen fill, etc.) | edge | yes | opt | many actions are cloud/credits | M | 25.0 (Sept 2023); "Prompt to Edit" added 27.10 |
| History | Undo states list, snapshots, non-linear history option | must | yes | no | — | M | |
| Actions | Record/play/edit action sets; button mode | must | no | no | — | L | Recording every command as descriptors; reimagined beta 26.6, streamlined 27.6 |
| Brushes | Brush presets, groups, recent | must | yes | no | — | S | ABR import |
| Brush Settings | Tip shape, dynamics, scattering, texture, dual brush, colour dynamics, transfer, pose | must | no | no | — | XL | Full brush engine parameter space |
| Tool Presets | Saved tool configurations | edge | no | no | — | S | |
| Clone Source | 5 clone sources with offset/scale/rotation/overlay | edge | no | no | — | S | |
| Swatches | Colour swatch groups | must | yes | no | — | S | ACO/ASE |
| Color | Colour sliders/wheel/cube | must | yes | no | — | S | |
| Gradients | Gradient presets and groups | must | yes | no | — | S | GRD parser |
| Patterns | Pattern presets | edge | no | no | — | S | PAT parser |
| Shapes | Custom shape presets | edge | no | no | — | S | |
| Styles | Layer style presets | edge | no | no | — | S | |
| Character | Font, size, leading, tracking, kerning, scale, baseline, OpenType toggles | must | yes | no | — | L | |
| Paragraph | Alignment, indents, spacing, hyphenation, composer | must | no | no | — | L | |
| Character Styles / Paragraph Styles | Reusable text styles | edge | no | no | — | M | |
| Glyphs | Browse and insert glyphs/alternates | edge | no | no | — | M | |
| Info | Cursor colour values, sampler readouts, doc info | must | no | no | — | S | |
| Histogram | Live histogram per channel with stats | must | no | no | — | S | |
| Navigator | Thumbnail navigation + zoom | edge | yes | no | — | S | |
| Channels | Channel list | must | no | no | — | S | |
| Paths | Path list | must | no | no | — | S | |
| Layer Comps | Comp states | edge | no | no | — | S | |
| Timeline | Video timeline or frame animation | edge | no | no | — | L | |
| Measurement Log | Recorded measurements table | edge | no | no | — | S | |
| Notes | Document notes | edge | no | no | — | S | |
| Libraries | Creative Cloud Libraries assets | bloat | no | no | account + cloud | — | |
| Comments | Cloud document comments | bloat | no | no | account + cloud | — | |
| Version History | Cloud document versions | bloat | no | no | account + cloud | — | |
| Discover / Learn | In-app search, tutorials, quick actions | bloat | yes | no | account + online | — | |
| Plugins panel | Installed UXP plugins | edge | no | no | account (marketplace) | M | |
| Content Credentials panel | Configure C2PA attribution on export | edge | no | no | account (Adobe signing) | M | c2pa-rs exists (Rust) |
| Adobe Stock browser | Browse/import Stock inside Photoshop (27.10) | bloat | no | no | account + purchase | — | |
| Firefly Boards / Projects surfaces | Moodboards and shared cloud projects (26.11, 27.5) | bloat | no | yes | account + cloud | — | |
| Generative credits indicator | Shows remaining credits (27.6) | bloat | no | no | account | — | |
| AI Assistant panel (beta) | Conversational multi-step editing agent | edge | yes | yes | account + cloud + credits | XL | Desktop public beta via Photoshop Beta June 2026 |
| Home screen | Recent files, Firefly generation history, templates | bloat | yes | no | account | S | |
| 3D panel | 3D scene | deprecated | no | no | — | — | Removed |

---

## 8. File formats (R = open/read, W = save/export)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| PSD (R/W) | Native layered format up to 30,000 px | must | yes | no | — | XL | Layer effects, smart objects, text EngineData, adjustments, Maximize Compatibility composite |
| PSB Large Document (R/W) | Up to 300,000 px, >2 GB | must | no | no | — | L | 64-bit length fields; tiled memory |
| Cloud document .psdc (R/W) | Adobe cloud format | bloat | no | no | account + cloud | — | Not openly documented |
| TIFF (R/W) | Layers, 8/16/32-bit, LZW/ZIP/JPEG, pyramid, alpha | must | no | no | — | M | Photoshop layer data in TIFF tag |
| PNG (R/W) | 8/16-bit, alpha, interlace, compression speed | must | yes | no | — | S | |
| JPEG (R/W) | Quality 0–12, baseline/progressive | must | yes | no | — | S | |
| JPEG XL (R/W) | Open, edit, save JXL incl. HDR (26.8) | edge | yes | no | — | M | jxl-oxide (R) / libjxl (W) |
| AVIF (R/W) | Open, edit, save AVIF incl. HDR (26.8) | edge | yes | no | — | M | rav1e encode speed |
| HEIF/HEIC (R; W unverified) | Open iPhone HEIC; HDR via Camera Raw | must | yes | no | — | M | HEVC patent/licensing; OS codecs on Win |
| WebP (R/W) | Native WebP open/save (since 23.2) | must | yes | no | — | S | Camera Raw opens WebP too (18.2) |
| GIF (R/W) | Indexed GIF; animated via Timeline/Save for Web | must | yes | no | — | S | |
| BMP (R/W) | Windows bitmap | edge | no | no | — | S | |
| Photoshop PDF (R/W) | PDF with preserved Photoshop editing capabilities | must | no | no | — | L | PDF writer + embedded PSD data |
| Generic PDF (R via Import PDF dialog) | Rasterise pages/images at chosen resolution | must | no | no | — | L | Needs PDF renderer (pdfium/hayro) |
| Photoshop EPS (R/W) | Rasterise EPS on open; save EPS with preview | edge | no | no | — | L | Needs PostScript interpreter to read |
| Photoshop DCS 2.0 (W) | Pre-separated EPS plates | deprecated | no | no | — | M | |
| SVG (R as smart object/place; W via Copy SVG/Export As) | Vector import rasterised; SVG export of shapes | edge | yes | no | — | M | Exact 2026 open/export support (unverified) |
| AI/Illustrator (Place) | Place .ai as smart object | edge | no | no | — | L | PDF-compatible part only |
| Camera raw (CR2/CR3/NEF/ARW/RAF/ORF/RW2/etc.) (R via ACR) | Open raw files through Camera Raw | must | yes | no | — | XL | rawler/rawloader coverage; CR3, compressed RAF, X-Trans |
| DNG (R via ACR; W from ACR / Render to DNG 18.5) | Open/save Digital Negative | must | no | no | — | L | DNG SDK semantics, linear DNG output |
| OpenEXR (R/W) | 16/32-bit float, alpha | edge | no | no | — | M | `exr` crate |
| Radiance HDR (R/W) | 32-bit .hdr | edge | no | no | — | S | |
| Portable Bit Map PBM/PGM/PPM/PNM/PFM (R/W) | Netpbm formats | edge | no | no | — | S | |
| DICOM (R/W) | Medical imaging frames, anonymise | edge | no | no | — | M | |
| Targa TGA (R/W) | Truevision with alpha | edge | no | no | — | S | |
| IFF (R/W) | Amiga/Maya IFF | edge | no | no | — | S | |
| PCX (R/W) | Legacy PC Paintbrush | deprecated | no | no | — | S | |
| Pixar (R/W) | Pixar image format | deprecated | no | no | — | S | |
| Photoshop Raw .raw (R/W) | Headerless raw bytes | edge | no | no | — | S | |
| Scitex CT (R/W) | Legacy prepress | deprecated | no | no | — | S | (unverified still present) |
| MPO (R) | Multi-picture stereo JPEG | edge | no | no | — | S | |
| JPEG 2000 (R/W) | Wavelet JPEG | edge | no | no | — | M | Optional/legacy status (unverified) |
| Cineon/DPX (R) | Film scan formats | edge | no | no | — | S | (unverified in 2026) |
| Video files (MOV/MP4/AVI etc.) (R as video layer) | Import video footage | edge | no | no | — | L | |
| Render Video (W H.264/HEVC/image sequence) | Export timeline | edge | no | no | — | L | |
| Brushes .abr (R/W) | Brush presets | must | no | no | — | M | Sampled + computed tips, dynamics descriptors |
| Gradients .grd / Patterns .pat / Shapes .csh / Styles .asl / Swatches .aco/.ase / Contours .shc / Tool presets .tpl | Preset formats | must | no | no | — | M | Descriptor format parsers |
| Actions .atn (R/W) | Action sets | must | no | no | — | L | Descriptor-level command mapping |
| LUTs .cube/.3dl/.look/.csp (R; W via Export) | Colour lookups | edge | no | no | — | S | |
| ICC profiles .icc/.icm (R/W) | Colour profiles | must | no | no | — | M | |
| XMP presets / ACR presets .xmp (R/W) | Camera Raw presets and profiles | must | no | no | — | M | |
| 3D formats (OBJ/3DS/DAE/KMZ/U3D/glTF) | 3D import/export | deprecated | no | no | — | — | Removed with 3D |

---

## 9. Adobe Camera Raw 18.x (also Camera Raw Filter)

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Raw decode + demosaic | Decode vendor raw and demosaic Bayer/X-Trans | must | yes | no | — | XL | Camera coverage treadmill; Adobe ships new camera support monthly |
| Process Version (PV6) | Versioned rendering maths for reproducible edits | must | no | no | — | L | Must version our own pipeline |
| Camera/lens support database | Colour matrices, DCP profiles, lens profiles per model | must | no | no | — | XL | Data problem more than code; LCP/lensfun |
| Profile browser: Adobe Color/Landscape/Portrait/Vivid/Neutral/Standard/Monochrome | Base rendering profiles | must | yes | no | — | L | DCP with look tables |
| Camera Matching profiles | Emulate in-camera picture styles | must | no | no | — | L | Needs per-camera data |
| Creative profiles (Artistic, B&W, Modern, Vintage) + amount | Stylised LUT-based profiles | edge | yes | no | — | M | |
| Film-inspired profiles and presets | 12 film-look profiles (18.3) | edge | yes | no | — | M | |
| Adobe Adaptive Color / Adaptive B&W profile | Image-adaptive AI tone/colour starting point (SDR/HDR) | edge | yes | yes | — (on device) | L | |
| White Balance (As Shot/Auto/presets, Temp/Tint, picker) | Set white balance | must | yes | no | — | M | Raw WB in camera space vs rendered |
| Tone: Exposure, Contrast, Highlights, Shadows, Whites, Blacks, Auto | Global tone controls | must | yes | opt | — | L | Local-adaptive highlights/shadows; Auto uses ML (Sensei) |
| Presence: Texture, Clarity, Dehaze | Local contrast and haze controls | must | yes | no | — | M | |
| Presence: Vibrance, Saturation | Colour intensity | must | yes | no | — | S | |
| Tone Curve: parametric | Highlights/lights/darks/shadows region curve | must | no | no | — | S | |
| Tone Curve: point + per-channel R/G/B, targeted adjustment | Point curves per channel | must | no | no | — | S | |
| Color Mixer: HSL / Color / B&W Mix | Per-hue hue/sat/lum and greyscale mix | must | yes | no | — | M | |
| Point Color (global & in masks) | Pick colour, shift hue/sat/lum with range controls | edge | no | no | — | M | Released 2024 |
| Point Color → Variance | Compress/expand colour variance for skin tones (18.0) | edge | no | no | — | M | |
| Color Grading | Shadows/midtones/highlights/global wheels, blending, balance | must | yes | no | — | S | |
| Detail: Sharpening (amount, radius, detail, masking) | Capture sharpening with edge mask | must | no | no | — | M | |
| Detail: Manual Noise Reduction (luminance/colour + detail/contrast/smoothness) | Classical noise reduction | must | no | no | — | M | |
| Denoise (AI) | Joint demosaic + denoise with amount, non-destructive (17.4) | must | yes | yes | — (on device; ANE on Apple) | XL | Model on raw mosaic; GPU speed |
| Raw Details (AI) | Improved demosaic detail | edge | no | yes | — | L | |
| Super Resolution (AI) | 2× linear upscale | edge | yes | yes | — | L | |
| Optics: Remove Chromatic Aberration, Use Profile Corrections, Defringe | Lens corrections | must | no | no | — | M | Lens profiles data |
| Optics: Distortion & Vignette manual | Manual lens corrections | must | no | no | — | S | |
| Lens Blur (AI) | Depth-based synthetic bokeh, focus range, bokeh shapes incl. anamorphic | edge | yes | yes | — (on device) | XL | Monocular depth + bokeh render |
| Geometry: Upright (Auto/Level/Vertical/Full/Guided) | Automatic perspective straightening | must | yes | no | — | L | Line detection + vanishing points |
| Geometry: Manual Transforms (vertical, horizontal, rotate, aspect, scale, X/Y offset) | Manual perspective | must | no | no | — | M | |
| Geometry: Projection | Reshape wide-angle edges for group photos (18.3) | edge | no | no | — | M | |
| Geometry: Anamorphic Desqueeze | 1.33×/1.6×/2× desqueeze (18.3) | edge | no | no | — | S | |
| Effects: Post-Crop Vignetting | Vignette with style/midpoint/roundness/feather/highlights | must | yes | no | — | S | |
| Effects: Grain | Amount, size, roughness | edge | yes | no | — | S | |
| Effects: Glow (Diffusion, Bloom, Halation) | Optical glow looks, global or masked (18.6) | edge | yes | no | — | M | |
| Calibration | Shadow tint; R/G/B primary hue/sat; process version | edge | no | no | — | S | |
| Crop & Rotate (straighten, aspect, angle) | Crop raw | must | yes | no | — | S | |
| Trim | Make outside-crop transparent; generate or keep (18.6) | edge | no | opt | cloud + credits if Generate used | M | |
| Generative Expand (ACR) | AI-extend canvas beyond crop | edge | yes | yes | cloud + credits | XL | Toggle in Preferences (18.5); introduction version (unverified) |
| Masking: Select Subject | AI subject mask | must | yes | yes | — (on device) | L | |
| Masking: Select Sky | AI sky mask | must | yes | yes | — | L | |
| Masking: Select Background | AI inverse-subject mask | must | yes | yes | — | L | |
| Masking: Objects (brush/rectangle) | AI object mask from rough stroke | edge | yes | yes | — | L | Improved detection in 18.0 |
| Masking: People (face skin, body skin, eyebrows, sclera, iris & pupil, lips, teeth, hair, clothes) | Per-person facial/body part masks | edge | yes | yes | — | XL | Human parsing model |
| Masking: Landscape (sky, water, vegetation, mountains, architecture, natural/artificial ground, snow) | Scene-element masks (17.3; snow 18.0) | edge | yes | yes | — | XL | Semantic segmentation |
| Masking: Brush (size, feather, flow, density, auto mask) | Paint local mask | must | yes | no | — | S | |
| Masking: Linear Gradient (incl. bidirectional 18.4) | Graduated local adjustment | must | yes | no | — | S | |
| Masking: Radial Gradient | Elliptical local adjustment | must | yes | no | — | S | |
| Masking: Range – Color / Luminance | Refine mask by colour/luma range | must | no | no | — | S | |
| Masking: Range – Depth | Mask by depth map for any photo (18.3) | edge | no | yes | — | L | Depth estimation |
| Mask ops: add/subtract/intersect/invert, rename, overlay, edge controls (18.5) | Combine mask components | must | no | no | — | M | |
| Local adjustment sliders (temp, tint, exposure… sharpness, noise, moiré, defringe, colour, hue) | Everything adjustable per mask | must | yes | no | — | M | |
| Remove tool (Heal / Clone / Content-aware / Generative AI toggle) | Spot removal incl. generative mode | must | yes | opt | Generative AI mode = cloud + credits | L | |
| Visualize Spots | Edge view to find dust | edge | no | no | — | S | |
| Distraction Removal: Reflections | Remove glass reflections (AI) | edge | yes | yes | (unverified cloud vs device) | XL | Improved 18.0 |
| Distraction Removal: People | Detect and remove people (Technology Preview) | edge | yes | yes | cloud + credits (unverified) | XL | Toggle in Preferences (18.5) |
| Distraction Removal: Dust | Auto-detect sensor dust spots (17.5 → 18.0) | edge | yes | yes | — | M | |
| Remove Blemishes | Auto-detect skin blemishes; Amount/Fade (18.5) | edge | yes | yes | (unverified) | L | |
| Red Eye / Pet Eye | Remove red/pet eye | edge | yes | no | — | S | |
| Presets (incl. Premium and Adaptive presets: portrait, sky, subject) | One-click looks; AI-mask-aware presets with amount slider | edge | yes | opt | — | M | |
| Snapshots | Save named edit states | must | no | no | — | S | |
| Auto settings / Auto per mask | ML auto tone | edge | yes | yes | — | M | |
| Histogram + clipping warnings | RGB histogram with clip overlays | must | no | no | — | S | |
| Vectorscope | Hue/sat scope; red-at-3 o'clock, Lab readouts (18.4/18.5) | edge | no | no | — | S | |
| Metadata panel | EXIF/IPTC display, GPS link (18.6) | edge | no | no | — | S | |
| Before/After views, zoom, filmstrip | Compare and navigate | must | yes | no | — | S | |
| Multi-image editing + sync settings | Edit and sync many raws (via Bridge/open) | must | no | no | — | M | |
| HDR editing and output | Edit HDR, visualise HDR range, SDR preview, HDR export | edge | no | no | — | L | Gain-map JPEG/AVIF/JXL output |
| Save Image (DNG/JPEG/TIFF/PSD/PSB/PNG/JXL/AVIF) | Export from ACR with resize/sharpen | must | yes | no | — | M | Exact format list (unverified) |
| Open as Smart Object (raw) | Re-editable raw inside PSD | must | no | no | — | L | Embed raw + XMP |
| XMP sidecar / DNG embedded settings | Store edits | must | no | no | — | M | |
| Content Credentials (C2PA) on export | Attach provenance | edge | no | no | account (Adobe signing) | M | |
| Generative AI preferences toggle | Enable/disable Generative Remove/Expand/People (18.5) | edge | no | no | — | S | |
| WebP support | Open WebP in ACR (18.2) | edge | yes | no | — | S | |
| Assisted Culling | AI focus/eyes/subject scoring for culling | edge | yes | yes | — | L | Lightroom-family feature; ACR availability (unverified) |

---

## 10. AI / Firefly-era features and when they shipped

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Sky Replacement | Replace sky, relight foreground (22.0, Oct 2020) | edge | yes | yes | — (device) | L | Pre-Firefly |
| Neural Filters | Model-based filters (22.0, Oct 2020) | edge | yes | yes | model download; some cloud | XL | See §4b |
| Select Subject / Object Selection | Device AI selection (Select Subject 2018; Object Selection 21.0) | must | yes | yes | cloud option added 26.6 | L | |
| Generative Fill | Prompted inpainting (beta May 2023 → GA 25.0 Sept 2023) | edge | yes | yes | cloud + credits | XL | Firefly Image 3 by 26.0; Firefly Fill & Expand model 27.3 |
| Generative Expand | Outpaint via Crop (beta mid-2023 → GA 25.0) | edge | yes | yes | cloud + credits | XL | |
| Remove tool | AI object removal brush (beta 2023 → GA 25.0) | must | yes | yes | cloud; on-device option 27.7 | L | Remove Tool 3 2K output 27.3 |
| Contextual Task Bar | Suggested next actions (GA 25.0) | edge | yes | opt | — | M | |
| Adjustment Presets | Preset looks (25.0) | edge | yes | no | — | S | |
| Generate Image | Text-to-image into document (beta Apr 2024; GA by 26.0) (exact GA version unverified) | edge | yes | yes | cloud + credits | XL | Partner models + custom models in 27.8 |
| Reference Image | Style reference for Gen Fill/Generate Image (beta 2024) | edge | yes | yes | cloud + credits | XL | Multiple references: up to 6 Gemini / 3 FLUX (27.6) |
| Generate Background | Replace background from prompt (beta 2024) | edge | yes | yes | cloud + credits | XL | |
| Generate Similar | More variations like a chosen one (26.0; new model 27.4) | edge | yes | yes | cloud + credits | XL | |
| Enhance Detail | Sharpen generative variations (beta 2024) | edge | no | yes | cloud + credits (unverified) | L | GA status (unverified) |
| Adjustment Brush | Paint adjustments (25.9, May 2024) | edge | yes | no | — | S | Not AI; bundled with AI wave |
| Distraction Removal (Wires & cables, People) | Auto-find and remove (26.0, Oct 2024) | edge | yes | yes | cloud | L | |
| General distractions | Auto-find clutter (27.6, Apr 2026) | edge | yes | yes | cloud (unverified) | XL | |
| Generative Workspace (beta) | Dedicated generation workspace (26.0 beta) | bloat | yes | yes | cloud + credits | — | Status in 2026 (unverified); likely folded into AI Assisted Editor |
| Improved Object Selection / hover | New AI object detection (26.0) | must | yes | yes | — | L | |
| Select Details / Select People | Facial parts, hair, clothing (26.6, Apr 2025) | edge | yes | yes | (unverified) | L | |
| Composition Reference (Generate Image) | Match composition of reference (26.6) | edge | yes | yes | cloud + credits | XL | |
| Actions panel reimagined (beta → streamlined) | Suggested edits + natural-language action search (26.6 beta; 27.6) | edge | yes | yes | (unverified) | L | |
| Harmonize | Match light/colour/shadows of composited layer (beta 2025 → GA 27.0, Oct 2025) | edge | yes | yes | cloud + credits | XL | Relighting + shadow synthesis |
| Generative Upscale | Upscale via Firefly (to 6,144², 5–10 cr), Topaz Gigapixel (10–20 cr), Topaz Bloom (35 cr) (27.0) | edge | yes | yes | cloud + credits | XL | On-device alternatives exist (Real-ESRGAN-class) |
| Partner models in Generative Fill | Gemini 2.5 Flash Image ("Nano Banana"), FLUX.1 Kontext (27.0) — ~10 credits | edge | yes | yes | cloud + credits | — | Third-party APIs |
| Nano Banana Pro (Gemini 3) and FLUX.2 pro in Gen Fill | Newer partner models (27.2, Dec 2025) — 40 / 20 credits | edge | yes | yes | cloud + credits | — | |
| Firefly Image 5 and Gemini 3.1 "Nano Banana 2" in Gen Fill | (27.6, Apr 2026) — FI5 10 cr; NB2 20–40 cr | edge | yes | yes | cloud + credits | — | |
| Instruct Edit | Instruction-based generative edits (27.6) | edge | yes | yes | cloud + credits | XL | Details (unverified) |
| Rotate Object | Rotate 2D cut-out as if 3D (beta → GA 27.6) | edge | yes | yes | cloud + credits (~20 in beta) | XL | Novel-view synthesis |
| Reflection Removal (Photoshop) | Remove glass reflections to separate layer (27.6) | edge | yes | yes | (unverified) | XL | |
| Layer clean-up / auto-rename | Rename unnamed layers from content (27.6) | edge | yes | opt | (unverified) | M | |
| On-device Remove model | Choose local vs cloud for Remove (27.7, May 2026) | must | yes | yes | none when local | L | Adobe's own admission that local matters |
| Generate Image partner models + Custom Models (beta) | Gemini/FLUX.2 Pro in Generate Image; train style on 10–30 images (27.8, Jun 2026) | edge | no | yes | cloud + credits (train 500, use 20) | XL | |
| AI Assistant (agentic) | Plain-language multi-step edits (announced MAX Oct 2025; web/mobile public beta 2026-03-10; desktop public beta June 2026) | edge | yes | yes | account + cloud + credits | XL | Tool-calling agent over command API |
| AI Assisted Editor (beta workspace) | Simplified AI editing mode (27.10, Aug 2026) | edge | yes | yes | account + cloud + credits | XL | Models incl. Firefly, Nano Banana 2, Gemini w/ Google Search; 4K output |
| Prompt to Edit | Natural-language edit from Contextual Task Bar (27.10) | edge | yes | yes | cloud + credits | XL | |
| Markup (visual annotations) | Draw arrows/circles/labels to guide AI (27.10; Gemini models) | edge | yes | yes | cloud + credits | L | |
| Mask/selection-constrained generation | Restrict generative edits to mask (27.10; Firefly Image 5) | edge | yes | yes | cloud + credits | M | |
| Photoshop in ChatGPT | Photoshop edits inside ChatGPT, free (2025-12-10) | bloat | yes | yes | account (ChatGPT) + cloud | — | Distribution play |
| Firefly Boards integration | Open PS cloud docs in Boards, round-trip (27.5, 27.7) | bloat | no | yes | account + cloud | — | |
| Content Credentials auto-attached to generative output | C2PA provenance on Firefly results | edge | no | no | account | M | |
| Generative credit limits | 25/mo single-app; partner/premium features cost more | bloat | — | — | credits | — | See Pricing |

---

## 11. Workflow and miscellaneous

| Feature | What it does (≤12 words) | Class | Lite? | AI? | Cloud seam | Difficulty | Note |
|---|---|---|---|---|---|---|---|
| Artboards workflow | Multiple screens/sizes in one doc, export per artboard | edge | no | no | — | M | |
| Creative Cloud Libraries | Shared colours, styles, graphics across apps | bloat | no | no | account + cloud | — | |
| Cloud documents + offline cache | Save to Adobe cloud, sync across devices | bloat | no | no | account + cloud storage | — | |
| Invite to Edit / Comments | Collaboration on cloud docs | bloat | no | no | account + cloud | — | |
| Share for Review | Review links with comments (beta 2023) | bloat | no | no | account + cloud | — | 2026 status (unverified); Projects added 26.11 |
| Projects | Shared cloud project folders for PSD/AI/Express (26.11) | bloat | no | no | account + cloud | — | |
| Adobe Express templates / Stock | Template and asset marketplace access | bloat | yes | no | account + purchase | — | |
| UXP plugins + Creative Cloud marketplace | JS/HTML plugin platform with Photoshop DOM/batchPlay | edge | no | no | account for marketplace | L | Studios depend on plugins (retouch panels) |
| ExtendScript (JSX) / JavaScript scripting | Script automation via DOM and Action Manager | must | no | no | — | XL | Pipelines/studios; API compatibility enormous |
| CEP extensions | Legacy HTML panels | deprecated | no | no | — | — | Being phased out for UXP |
| Generator (Image Assets) | Layer-name-driven asset export | edge | no | no | — | M | |
| Actions / Batch / Droplets | Macro recording and batch processing | must | no | no | — | L | |
| Keyboard shortcut customisation (sets, export) | Custom shortcut sets | must | no | no | — | S | Ship a Photoshop-compatible default set |
| Workspaces (built-in + custom, auto-restore) | Saved panel layouts | must | no | no | — | M | Docking UI in web tech |
| Preferences → Performance (RAM, history states, cache levels/tile size) | Tune memory and history | must | no | no | — | M | |
| Preferences → GPU (Use Graphics Processor, OpenCL, advanced) | GPU acceleration | must | no | no | — | L | wgpu backend on Metal/DX12/Vulkan/WebGPU |
| Scratch disks | Disk-backed virtual memory for huge docs (updated 27.1) | must | no | no | — | L | Tile paging to disk; in-browser via OPFS |
| Auto-recovery + background save | Crash recovery | must | yes | no | — | M | |
| Preferences → File Handling (Maximize PSD compatibility, recent files, legacy options) | File save behaviour | must | no | no | — | S | |
| Preferences → Units & Rulers, Guides/Grid/Slices, Cursors, Transparency & Gamut | Environment preferences | must | no | no | — | S | |
| Preferences → Technology Previews | Toggle experimental features | edge | no | no | — | S | |
| Quiet Mode | Suppress pop-ups/notifications (27.0) | edge | yes | no | — | S | Users asked for less nagging |
| Modernised UI + accessibility (keyboard nav, contrast) | 27.0 redesign of controls/dialogs | must | yes | no | — | M | |
| Performance work (brushes, transforms, large files) | 27.0 responsiveness; AMD Zen 4/5 optimisation (27.10) | must | yes | no | — | L | |
| History panel: states, snapshots, non-linear history | Undo browsing and snapshot branching | must | yes | no | — | M | |
| Non-destructive editing (smart objects + adjustments + masks) | Full re-editability | must | yes | no | — | XL | Core architecture decision |
| Guides, Smart Guides, Grid, Pixel Grid, snapping | Precise layout | must | no | no | — | S | |
| Rulers + origin | Measurement rulers | must | no | no | — | S | |
| Video timeline (layers, keyframes, transitions, audio) | Basic video editing | edge | no | no | — | XL | Decoding + audio + keyframes |
| Frame animation (GIF) | Frame-by-frame animation with tweening | edge | yes | no | — | M | Animated GIF/WebP output |
| 3D (layers, rendering, printing) | Former 3D modelling/painting | deprecated | no | no | — | — | Discontinued 22.5 (Aug 2021); menu removed by 25.4; Substance 3D Viewer beta workflow discontinued 2025-10-16 |
| Print dialog | Colour-managed print, printer vs Photoshop manages, proofing, marks, bleed, 16-bit | must | no | no | — | L | OS print APIs; not feasible in browser beyond PDF |
| Content Credentials (C2PA) | Attach creator/edit provenance on export | edge | no | no | account (Adobe cert) | M | c2pa-rs; our own signing needs a cert |
| Photoshop on the web | Browser Photoshop subset + generative tools | bloat | yes | opt | account (always online) | — | Included with desktop plans; Mobile & Web plan $7.99/mo |
| Photoshop on iPad | Touch Photoshop sharing core with mobile app | edge | yes | opt | account | — | Feature subset (unverified detail) |
| Photoshop mobile (iPhone Feb 2025; Android beta mid-2025) | Free core tools, premium via plan | edge | yes | opt | account | — | Tap Select, Gen Fill |
| Adobe Fonts sync | Activate fonts from subscription | bloat | yes | no | account + subscription | — | |
| Bridge / Lightroom handoff | Edit in Photoshop from Lightroom; Bridge browsing | edge | no | no | account | M | |
| Licence check-in | Apps must go online periodically to validate subscription | bloat | no | no | account + online | — | Monthly ~30 days / annual ~99 days per Adobe FAQ (unverified this session) |
| Generative AI content guidelines filter | Blocks prompts/outputs deemed violating | bloat | no | yes | cloud | — | Frequent false-positive complaints |
| OS requirements | Windows 10+, macOS 14+ (27.x) | must | — | — | — | — | Native Apple Silicon |
| Search / Discover learning content | In-app tutorials and search | bloat | yes | no | account + online | — | |

---

## Pricing (2026)

Prices are USD. Plans are subscription-only; there is no perpetual licence.

### Individual plans

From adobe.com/products/photoshop/plans.html, fetched 2026-09-14:

| Plan | Price | Generative credits / month | Includes |
|---|---|---|---|
| Photoshop (single app) | $22.99/mo annual billed monthly; $263.88/yr prepaid | 25 | Photoshop desktop + web + mobile, 100 GB, Adobe Express Premium |
| Photography (1 TB) | $19.99/mo annual billed monthly | 1,000 (per plans page; earlier forum reports said 500 for 1 TB and 250 for 20 GB — allowances have shifted, treat as unverified) | Photoshop + Lightroom, 1 TB |
| Creative Cloud Pro | $69.99/mo (promo $34.99 for first 3 months); $779.88/yr prepaid | 4,000 (plus unlimited "standard" generations per third-party reporting) | 20+ apps |
| Creative Cloud Pro, students/teachers | $19.99/mo first year | 4,000 | |
| Firefly Pro | $19.99/mo | 4,000 | Photoshop web + mobile only |
| Photoshop Mobile & Web | $7.99/mo or $69.99/yr (Feb 2025 launch pricing) | (unverified) | No desktop |

### Business plans

| Plan | Price per licence | Generative credits / month | Storage |
|---|---|---|---|
| Photoshop for Teams | $37.99/mo | 25 | 1 TB |
| Creative Cloud Pro for Teams | $99.99/mo | 4,000 | 1 TB |

### Other price points and credit rules

- **Month-to-month prices:** CG Channel's 27.x articles quote the single-app plan at **$34.49/mo** and the Photography plan at **$29.99/mo or $239.88/yr**. These appear to be the no-commitment monthly prices (unverified mapping).
- **Early termination fee:** cancelling an "annual, billed monthly" plan in year one costs 50% of the remaining payments.
  - Adobe settled the DOJ/FTC case over hiding this fee for **$150M** on 2026-03-13: a $75M civil penalty plus $75M in free services.
  - It must now disclose the fee up front.
- **Credit costs per generation:**

  | Feature | Credits |
  |---|---|
  | Firefly Generative Fill/Expand | 1 |
  | Gemini 2.5 / FLUX.1 Kontext | ~10 |
  | FLUX.2 pro | 20 |
  | Nano Banana Pro | 40 |
  | Nano Banana 2 | 20–40 |
  | Firefly Image 5 | 10 |
  | Generative Upscale, Firefly | 5–10 |
  | Generative Upscale, Topaz Gigapixel | 10–20 |
  | Generative Upscale, Topaz Bloom | 35 |
  | Rotate Object (beta) | ~20 |
  | Custom model: train / use | 500 / 20 |

- **What 25 credits buys:** on the single-app plan, about 2 Nano Banana Pro generations or 1 Firefly Image 4 Ultra image a month (per CG Channel).

---

## Top recurring user complaints

**Reddit caveat:** reddit.com is blocked for this research agent's fetcher, so r/photoshop threads could not be quoted directly. The quotes below come from the Adobe Community forums and Trustpilot.

| # | Complaint theme | Quote | Source |
|---|---|---|---|
| 1 | Generative Fill quality regressed after model swaps | "The quality has dropped to an unusable level. Textures that were rendered beautifully in previous versions now sometimes appear smudged" | https://community.adobe.com/t5/photoshop-ecosystem-bugs/p-the-generative-fill-in-photoshop-26-0-0-is-definitely-worse-than-in-previous-versions/idi-p/14931200 |
| 2 | Paying credits for unusable output | "Generative Fill has dropped so significantly. Nothing it generates is even worth keeping, yet credits are deducted" (Oct 24, 2025) | https://community.adobe.com/questions-712/ai-and-generative-credits-just-keep-getting-worse-1182326 |
| 3 | Confusing, shifting credit allowances | "I should be getting 500 credits/month. But when I check my account, it looks like I'm only getting 250 credits/month." (Oct 8, 2025) | https://community.adobe.com/questions-712/photoshop-since-way-back-why-250-not-500-generative-credits-1182846 |
| 4 | Performance: lag and freezes | "Unusable laggy and freezes ALL THE TIME" (Sept 2026) | https://www.trustpilot.com/review/photoshop.com |
| 5 | Subscription and cancellation fees | "Their subscription model is terrible...not clear on when you can't cancel without a fee" (Aug 1, 2026) | https://www.trustpilot.com/review/photoshop.com |
| 6 | Over-eager content filter / access revoked | Thread titled "You no longer have access due to a violation of our terms of use" after benign Generative Fill use (Sept 2023) | https://community.adobe.com/t5/photoshop-beta-discussions/you-no-longer-have-access-due-to-a-violation-of-our-terms-of-use/td-p/14097711/highlight/true/page/3 |
| 7 | Censorship of legitimate prompts | "censorship in general keeping us from creating things like graphics for signs stating that guns are not allowed" | https://community.adobe.com/questions-712/ai-and-generative-credits-just-keep-getting-worse-1182326 |

Other patterns:

- **Ratings:** Trustpilot rates photoshop.com 1.9/5 (26 reviews).
- **Bad updates:** 27.9 was pulled for washing out images, then fixed in 27.9.1 (August 2026).
- **Removed features:** 3D and Shake Reduction still draw "bring it back" threads.
- **Cloud dependence:** features stop working offline; Remove only got a local model in 27.7.

---

## Sources

**Adobe (official)**
- Adobe plans page (fetched): https://www.adobe.com/products/photoshop/plans.html
- Photoshop desktop what's new (403 to fetcher): https://helpx.adobe.com/photoshop/desktop/whats-new/whats-new-in-adobe-photoshop-on-desktop.html
- Photoshop desktop release notes (403 to fetcher): https://helpx.adobe.com/photoshop/desktop/whats-new/photoshop-on-desktop-release-notes.html
- Remove wires/people/distractions (helpx, via search snippet): https://helpx.adobe.com/photoshop/desktop/repair-retouch/remove-objects-fill-space/remove-wires-people-distractions.html
- Selection Brush tool (helpx, via search snippet): https://helpx.adobe.com/photoshop/desktop/make-selections/freehand-selections/create-quick-selections-with-selection-brush-tool.html
- Neural Filters list and FAQ (403 to fetcher): https://helpx.adobe.com/photoshop/using/neural-filters-list-and-faq.html
- Generative credits FAQ (403 to fetcher): https://helpx.adobe.com/creative-cloud/apps/generative-ai/generative-credits-faq.html
- Photoshop 3D discontinued FAQ: https://helpx.adobe.com/my_en/photoshop/kb/3d-faq.html
- Substance 3D Viewer decommission: https://helpx.adobe.com/substance-3d-viewer/get-started/substance-3d-viewer-decomission.html
- Camera Raw release notes: https://helpx.adobe.com/camera-raw/desktop/whats-new/release-notes.html

**Adobe Community announcements**
- Photoshop 27.10: https://community.adobe.com/announcements-710/photoshop-27-10-new-ai-editing-tools-light-adjustment-layer-and-more-1639062
- Photoshop 27.7: https://community.adobe.com/announcements-710/photoshop-27-7-is-live-faster-performance-smarter-ai-and-cleaner-workflows-1623899
- Photoshop 27.6: https://community.adobe.com/announcements-710/photoshop-v27-6-is-live-rotate-in-3d-clean-up-faster-and-create-with-smarter-ai-1557783
- Camera Raw 18.5: https://community.adobe.com/announcements-561/camera-raw-v18-5-now-available-smarter-ai-retouching-enhanced-vectorscope-render-to-dng-and-more-1635111
- Camera Raw for MAX 2025: https://community.adobe.com/announcements-561/what-s-new-in-adobe-camera-raw-for-max-2025-182206
- Shake Reduction removed in 23.3: https://community.adobe.com/feature-requests-713/p-bring-back-shake-reduction-filter-removed-in-photoshop-v-23-3-0-653780

**Adobe newsroom and Adobe staff**
- Adobe newsroom, MAX 2025 Creative Cloud: https://news.adobe.com/news/2025/10/adobe-max-2025-creative-cloud
- Adobe newsroom, Photoshop mobile & web (Feb 2025): https://news.adobe.com/news/2025/02/photoshop-mobile-web
- Adobe newsroom, Photoshop/Express/Acrobat in ChatGPT: https://news.adobe.com/news/2025/12/adobe-photoshop-express-acrobat-chatgpt
- Julieanne Kost (Adobe), 27.6 catch-up: https://jkost.com/blog/2026/04/a-quick-catch-up-on-photoshops-latest-features-v27-6.html
- Julieanne Kost, Camera Raw 18: https://jkost.com/blog/2025/10/5-new-updates-for-adobe-camera-raw-v18.html

**Third-party release coverage**
- CG Channel, per release:
  - 26.0: https://www.cgchannel.com/2024/10/adobe-releases-photoshop-26-0-and-updates-photoshop-on-the-web/
  - 26.3: https://www.cgchannel.com/2025/01/adobe-releases-photoshop-26-3/
  - 26.5: https://www.cgchannel.com/2025/03/adobe-releases-photoshop-26-5/
  - 26.6: https://www.cgchannel.com/2025/04/adobe-releases-photoshop-26-6/
  - 26.8: https://www.cgchannel.com/2025/06/adobe-releases-photoshop-26-8/
  - 26.10: https://www.cgchannel.com/2025/08/adobe-releases-photoshop-26-10/
  - 26.11: https://www.cgchannel.com/2025/09/adobe-releases-photoshop-26-11/
  - 27.0: https://www.cgchannel.com/2025/10/adobe-releases-photoshop-27-0/
  - 27.2: https://www.cgchannel.com/2025/12/adobe-releases-photoshop-27-2/
  - 27.3 + 27.4 beta: https://www.cgchannel.com/2026/01/adobe-releases-photoshop-27-3-and-photoshop-27-4-beta/
  - 27.6: https://www.cgchannel.com/2026/04/adobe-releases-photoshop-27-6/
  - 27.7: https://www.cgchannel.com/2026/05/adobe-releases-photoshop-27-7/
  - 27.8: https://www.cgchannel.com/2026/06/adobe-releases-photoshop-27-8/
  - 27.9.1: https://www.cgchannel.com/2026/08/adobe-releases-photoshop-27-9-1/
  - 27.10: https://www.cgchannel.com/2026/08/adobe-releases-photoshop-27-10/
- CG Channel, Photoshop on iPhone/Android: https://www.cgchannel.com/2025/06/adobe-launches-photoshop-on-iphone/
- DPReview, AI Assistant desktop beta: https://www.dpreview.com/news/1952544048/adobe-photoshop-ai-assistant-desktop-beta/
- DPReview, Camera Raw Glow: https://www.dpreview.com/news/adobe-adds-glow-to-camera-raw/
- WinBuzzer, AI Assistant public beta (web/mobile): https://winbuzzer.com/2026/03/11/adobe-photoshop-ai-assistant-public-beta-web-mobile-xcxwbn/
- PetaPixel, Clarity/Dehaze and Grain adjustment layers: https://petapixel.com/2026/01/27/photoshop-update-adds-popular-camera-raw-tools-as-adjustment-layers/
- PhotoshopCAFE, 2026 overview: https://photoshopcafe.com/whats-new-in-photoshop-2026-full-release-overview/
- PhotoshopCAFE, August 2026 update: https://photoshopcafe.com/5-big-new-features-in-photoshop-2026-august-update/
- Computer Darkroom, Camera Raw:
  - Late August 2026 (18.6): https://www.computer-darkroom.com/blog/2026/08/25/camera-raw-late-august-2026/
  - June 2026 (18.4): https://www.computer-darkroom.com/blog/2026/06/16/camera-raw-lightroom-classic-desktop-june-2026/
  - April 2026 (18.3): https://www.computer-darkroom.com/blog/2026/04/16/camera-raw-lightroom-classic-desktop-april-2026/
  - February 2026 (18.2): https://www.computer-darkroom.com/blog/2026/02/20/camera-raw-lightroom-classic-desktop-february-2026/
- Neural Filters category listing (third-party): https://fixthephoto.com/photoshop-neural-filters.html

**Legal and complaints**
- US DOJ, $150M settlement: https://www.justice.gov/opa/pr/adobe-agrees-150-million-settlement-and-injunction-resolve-alleged-violations-restore-online
- FTC complaint (June 2024): https://www.ftc.gov/news-events/news/press-releases/2024/06/ftc-takes-action-against-adobe-executives-hiding-fees-preventing-consumers-easily-cancelling
- Trustpilot, photoshop.com: https://www.trustpilot.com/review/photoshop.com
