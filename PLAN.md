# OpenPhotoEdit — plan

Status: **research complete, no code yet** (2026-09-14).

| Document | What it holds |
|---|---|
| `docs/why.md` | Why this is worth building |
| `docs/feature-matrix.md` | Every capability, with its phase (361 rows) |
| `docs/research/01–04` | Evidence behind the matrix |

---

## 1. Product shape

**One engine, two profiles.**

| | **Lite** | **Pro** |
|---|---|---|
| Who | Anyone with a photo to fix, crop, annotate or clean up | Designers, retouchers, photographers switching from Photoshop, Lightroom or Photopea |
| Mental model | "Tap the thing, pick the fix." A step stack, not layers | Photoshop: layers, masks, adjustment layers, selections, PSD |
| Surface | One canvas, a bottom or side tool strip, prompt bar and chips | Toolbar, options bar, dockable panels, PS-compatible shortcuts |
| AI | Unlimited local remove, select, cutout, blur background, upscale, denoise, auto-enhance | The same, plus text-select, culling, batch masks, retouch; generative fill with the native pack |
| Built from | OpenCapture's crop/annotate/redact interaction and OpenPixels' AI engine | Graphite-style adjustments, a forked PSD crate, the shared tile engine |

**The profiles are views of one document.**

- Every Lite step is a real adjustment layer, mask or pixel layer underneath.
- **Open in Pro** shows those layers exactly; nothing is flattened or converted.
- Pro documents open in Lite read-only, with a note: "This file has 14 layers — open in Pro to edit them."
- The profile is a setting (first run asks "Simple or Professional?"), switchable at any time. It changes the tool registry, panel layout and defaults, never the file.

**No paid features.** There are no accounts, credits or watermarks. Support features may come later but never gate a function. OpenCapture's paid watermark is replaced by a free one.

---

## 2. Architecture

### 2.1 Three ways to run the same code

```
                ┌──────────────── apps/web (Svelte 5 UI, Lite + Pro shells) ───────────────┐
                │                    packages/engine: Engine interface                     │
                │           ┌─────────────────────────┴───────────────────────────┐        │
                │     WasmEngine (worker)                                  NativeEngine    │
                │     crates → wasm32, WebGPU                         WebSocket, binary tiles
                └───────────┬─────────────────────────────────────────────────────┬────────┘
                            │                                                     │
 1. Hosted web / PWA   ─────┘        2. Local Rust server (editor-server)  ────────┘
    zero install, 4 GB wasm heap,       one binary, serves the same UI on 127.0.0.1
    AI tiers T1–T2 in browser           (rust-embed, the OpenDownloader pattern),
                                        native wgpu + ort (CUDA/Metal/DirectML),
                                        llama.cpp, diffusion → AI tier T3,
                                        mmap tile swap, no memory cap
                                     3. Later: Tauri desktop + iPad/Android store apps,
                                        same crates (the suite's end state)
```

**What "Rust backend, locally run" means here.** The editing engine is Rust everywhere.
- **In the browser** it is compiled to wasm and runs in a worker.
- **As the local backend** it is one native binary: `openphotoedit serve` opens the browser at `http://127.0.0.1:<port>`.
  - The UI and engine share an origin, which avoids the mixed-content, CORS and Private Network Access problems OpenDownloader documented.
  - The UI detects the native engine and unlocks tier-3 AI and unlimited document size.

### 2.2 Crates

Crate names use a neutral `editor-` prefix so they survive a product rename.

| Crate | Owns | Starts from |
|---|---|---|
| `editor-doc` | Document tree (layers, groups, masks, adjustment params), typed commands, history, project format; `serde` + `schemars` | new |
| `editor-tiles` | 256² copy-on-write tiles, sparse, LZ4 warm tier, spill trait (OPFS worker / mmap), mip pyramid | Krita / libvips design (research 03 §C.3.5) |
| `editor-color` | Working spaces, ICC via `moxcms`, blend-space flag, display LUT | `moxcms` |
| `editor-composite` | **CPU reference compositor** in f32: 27 blend modes, adjustments math, PS rounding | Graphite `adjustments.rs` / `blending.rs` (MIT) |
| `editor-gpu` | wgpu/WGSL compositor per tile, filters, jump-flood distance fields | `wgpu` 30 |
| `editor-filters` | Blur, sharpen, noise, local Laplacian, guided filter, Poisson heal, frequency separation, seam carving | `imageproc`, `libblur`, own kernels |
| `editor-select` | Flood fill, graph cut, mask algebra, matting band, colour/luminance range | new |
| `editor-codec` | JPEG/PNG/WebP/GIF/AVIF/JXL/TIFF/EXR, HEIC (native), EXIF/XMP | `image`, `zune-*`, `jxl`, `ravif`, `little_exif`; OpenCapture `encode.rs` |
| `editor-psd` | PSD/PSB read/write, descriptors, layer records, composite; **published separately MIT/Apache** | fork `ag-psd` (Rust) cross-checked with PhotoshopAPI (BSD-3) |
| `editor-raw` | RAW decode, demosaic, scene-referred tone, lens profiles | `rawler` (LGPL), darktable/RawTherapee maths (AGPL-compatible) |
| `editor-text` | Shaping, layout, text layers | `parley`, `swash`, `harfrust` |
| `editor-vector` | Paths, shapes, booleans, rasterisation | `kurbo`, `vello` / `tiny-skia` |
| `editor-ai` | Model manifest, download/verify/cache, backend canary, tiling, pre/post, inpaint orchestration, face alignment | **lift** `openpixels/pixels-core` (tile, face, matte, metrics) and `openphotoid/frame-engine`, `frame-matting`, `frame-retouch` |
| `editor-plan` | Instruction tool schema, rule parser (8 languages), plan validator, GBNF generator, image-facts extractor | new (research 04 §7) |
| `editor-c2pa` | Content Credentials per history step | `c2pa` |
| `editor-wasm` | wasm-bindgen surface | — |
| `editor-server` | axum loopback server, rust-embed UI, native AI runtimes | OpenDownloader `dl-app` |

Front end:
- `packages/engine`: worker, client, `Engine` interface.
- `packages/ui`: components; claims accent **slot 24**, which must be minted because 1–23 are taken.
- `apps/web`: Svelte 5, the choice of OpenPixels, OpenPhotoId and Graphite, because a panel-heavy Pro UI needs a component framework.

Other layout:
- `e2e/` holds Playwright on a product-specific port (5201+; see the memory *E2E ports collide between products*).
- `docs/research/` holds the research files.

### 2.3 Non-negotiable engineering rules

These come from what the suite has already learned:

1. **CPU reference compositor = oracle.**
   - The GPU output must match it within tolerance.
   - Both are tested against Photoshop-exported composites of a real PSD corpus.
   - Exact-match tests run on CPU only, because GPU float results differ across hardware.
2. **Real files, not fixtures** (memories *Only real files find parser bugs* and *Only real pages find extraction bugs*).
   - A PSD corpus from Photoshop 2020–2026, RAW files from ≥30 camera bodies, and iPhone HEICs.
   - Every file carries the publishable-fixture marker.
3. **GPU canary per model × backend × browser version**, as OpenPhotoId learned from WebGPU mattes that were wrong without any error. Look at the GPU path's pixels in a headed browser.
4. **No third-party requests at run time.**
   - Self-host the ONNX runtime and every model.
   - Disable `ort-web` telemetry.
   - Models carry sha256, licence and attribution in one manifest.
5. **Fixed-shape tiling for AI; fp32 by default.** Use fp16 only after its canary passes (see *ONNX-in-browser gotchas*).
6. **Never allocate a full-document buffer.** Everything works tile by tile, including export, which is streamed.
7. **Model licences: weights decide.** Nothing from the red-flag list in research 04 §5 ships, and grey-zone models (for example BiRefNet's DIS5K training data) need a recorded decision.

---

## 3. Phases

Sizes are relative (S days, M weeks, L months, XL multi-person-months). This is a long roadmap, and each phase ships something usable.

### P0 — Engine (L)

Nothing user-visible; everything after this depends on it.

- `editor-doc`, `editor-tiles`, `editor-composite` (normal, multiply, screen, overlay to start), `editor-gpu` viewport compositor, history.
- `editor-ai` lifted from OpenPixels and OpenPhotoId, with manifest, canary and diagnostics route.
- The PSD test corpus and composite oracle harness. The moat needs its test bench from day one.
- A walking skeleton of `editor-server` serving the UI.
- **Exit:** open a 50 MP JPEG, pan and zoom at 60 fps, and add three adjustment layers with unlimited undo. The same build works in a Chrome tab and through `editor-server`. Memory stays under 1.5 GB in the tab.

### P1 — Lite 1.0 (L) — first public release

OpenCapture's editor tools, moved onto the new engine:
- crop with ratio lock and exact output size, straighten, rotate and flip;
- arrow, box, ellipse and line **with colour and width** (OpenCapture had neither);
- text with background, highlighter and pen;
- solid redaction plus irreversible pixelate;
- **free** watermark;
- frames;
- zoom;
- copy, PNG and PDF export.

Photo editing:
- Auto with 3–4 alternatives;
- Light / Color / B&W master sliders that expand;
- white balance, vignette, clarity;
- looks with a strength slider;
- sharpen and denoise;
- before/after;
- a step stack where every step can be reopened.

AI, unlimited and local:
- tap-to-select (EdgeTAM);
- **Erase** (MI-GAN, with LaMa as "Best");
- remove background (MODNet);
- blur background;
- upscale 2×/4× (Real-ESRGAN x4v3 / SPAN);
- JPEG clean-up;
- red-eye;
- content-aware quick actions;
- **prompt bar with chips**, backed by a rule parser in 8 languages that writes visible slider changes.

Files and output:
- JPEG, PNG, WebP, HEIC and AVIF in;
- export recipes (Web / Instagram / Print / target size);
- strip GPS by default;
- autosave;
- PWA offline;
- Models page with sizes and licences.

**Exit:** a first-time user removes a person from a photo in under 30 seconds with no download prompt beyond ~70 MB. Then:
- the `openapps-ship` test guide with real screenshots;
- deployed to the product domain (memory *Deploy web apps after every edit*);
- the Chrome/Edge extension to right-click → edit follows.

### P2 — Pro Core: the "open Photopea" (XL)

- **Layers:** pixel layers and groups, opacity/fill, **all 27 blend modes matching Photoshop**, pass-through groups, pixel masks, clipping masks, the full set of adjustment layers, fill layers.
- **Selections:** marquee, lasso, wand, click and subject selection, Quick Mask, alpha channels, modify, transform.
- **Tools:** Free Transform, spot heal, healing brush, clone stamp, dodge/burn, brush (non-dynamic), eraser, gradient, bucket, live shapes, basic text with local fonts.
- **Files and workspace:**
  - **PSD read and write, 8-bit**, with adjustment and fill layers and a correct composite;
  - SVG place;
  - rulers, guides and snapping;
  - dockable panels, tabs;
  - Photoshop shortcuts;
  - contextual task bar.
- **Platform:** `editor-server` release binaries (macOS, Windows, Linux), plus a self-hostable static bundle (answering Photopea #4870).
- **Exit:** 95% of the PSD corpus (8-bit, no text or smart objects) round-trips. Photoshop reopens it with ΔE < 1 on the composite and a structural diff with no lost layers. A retoucher can do a standard skin-and-background job end to end.

### P3 — Pro Photographer (XL)

- **RAW:** decode through `rawler`; scene-referred develop (Basic, curve, HSL, grading, detail, optics, geometry, local and AI masks); **the RAW layer stays re-editable inside the document**.
- **Depth:** 16-bit, JPEG XL, TIFF, PSD 16/32-bit.
- **Selection and masks:** sky, people parts, depth, text-prompt selection, Select and Mask edge refinement, Blend If.
- **Retouch:** AI denoise (NAFNet; RawNIND natively), deblur, face restore, colourise, content-aware fill, patch, dust spots, frequency separation, skin smoothing.
- **Library and batch:** folder browsing, ratings and flags, compare, **AI culling with no credits** (eyes, focus, exposure, similar groups), copy/paste masks re-detected per photo, batch recipes, Actions.
- **AI assistance:** LLM prompt bar (Qwen3-1.7B, optional 1.1 GB), autopilot suggestions, C2PA.
- **Exit:** a wedding photographer culls and develops 800 raws and exports two recipes without leaving the app and without an account.

### P4 — Pro Depth (XL)

- **Layer model:** layer styles (jump-flood distance fields), smart objects and smart filters, vector masks.
- **Warping:** Warp, Puppet and Perspective Warp, Liquify with face-aware mode.
- **Type and vector:** full type engine (OpenType, styles, text on path, CJK vertical), pen and paths.
- **PSD:** text, styles, smart objects, PSB.
- **Merges and colour:** panorama, HDR, focus stack; 32-bit; soft proofing, CMYK.
- **AI features:** batch per-face retouch profiles, relighting, sky replacement, harmonize, tap-to-move objects.
- **Extensibility:** brush dynamics, `.abr` import, WASM plugin API.

### P5 — Generative and agent (L, native backend)

- **Native generative pack:** Generative Fill, Expand, Background and Variations via FLUX.2 [klein] 4B (Apache, ~8.4 GB). Optional SDXL + Lightning pack for 8 GB GPUs; Qwen-Image-Edit workstation pack for ≥24 GB.
- **Agent planner:** Qwen3.5-4B, constrained to typed tool calls, with preview and confirm; every step is an undo entry and a C2PA action.
- **Plain language to Actions:** describe a repetitive job and save it as an Action.
- **Faithful vs creative toggle:** faithful is the default.
- **ComfyUI / InvokeAI bridge:** for users who already run one.

### Later

The "Later" rows in the matrix, 58 of them, include:
- tethering, catalog collections, natural-language search, face grouping;
- store apps: Tauri desktop, iPad, Android;
- embed API;
- mixer brush, Vanishing Point, animation.

---

## 4. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| **Trademark: "Photoshop" in the name** | High if published | Rename before any public repo, domain or store listing (see §5) |
| Scope: 361 capabilities is years of work | Certain | Ship per phase; P1 is valuable on its own; the matrix decides what is left out |
| PSD fidelity long tail (Photoshop 2026 changed descriptor enums) | High | Corpus-first testing from P0; round-trip gate on every PR; keep the unknown `8BIM` blocks and layer info byte-exact |
| Graphite adds raster, selection and PSD faster than expected | Medium | Our wedge is PSD + Photoshop UX + local AI; Graphite is node/vector-first. Its code is MIT/Apache and we can reuse it |
| WebGPU outputs silently wrong on some GPU/browser | High (already hit) | Per-model canary, CPU fallback, headed-browser pixel checks |
| Safari: no Memory64, iOS 17 cannot run onnxruntime-web | Certain | Tile engine within 4 GB; diagnostics page; the native server covers big files |
| Model licence taint (DIS5K, FFHQ, CelebA) | Medium | Red-flag list; recorded grey-zone decisions; clean fallbacks (MODNet, MediaPipe) |
| Patents: HEVC (HEIC decode), PatchMatch (Content-Aware Fill) | Medium | Browser/OS HEIC decoders where present; texture synthesis and ML inpainting instead of PatchMatch; legal check before P3 |
| LGPL `rawler` statically linked into wasm | Medium | AGPL-3.0 app licence resolves it; otherwise ship RAW as a separate wasm module |
| The suite's accent ring is full (slots 1–23) | Certain | Mint slot 24 by lightness/chroma per `tokens/ACCENT-SLOTS.md` |

---

## 5. Open decisions (need Darius)

1. **Name — settled 17 September 2026: OpenPhotoEdit.** The earlier
   working name, "OpenPhotoshop", used Adobe's registered mark and invited a
   GitHub DMCA/trademark takedown, a store rejection and a UDRP challenge; the
   new name carries none of that. The folder, the binary, crate metadata, the
   `.opproj` format string (the old one is still read) and the accent-slot
   registry row all moved with it. The internal `ops-` CSS prefix and the
   `ops.*` storage keys stayed: they are invisible, and renaming 470 call
   sites buys nothing. Still to do: the domain and the GitHub account.
2. **Licence.**
   - Recommendation: **AGPL-3.0-or-later** for the app, as OpenCapture uses. It lets us use `rawler`, `libheif` and darktable/RawTherapee maths.
   - Publish `editor-psd` and `editor-tiles` as **MIT/Apache** to attract contributors.
   - Permissive for everything would force clean-room RAW, demosaic and retouch code.
3. **Which Pro comes first after Lite.**
   - Recommendation: **P2 "open Photopea"** (PSD compositor) before **P3 Photographer**. PSD is the moat and shares all of Lite's engine; the photographer pipeline adds `rawler` and library work on top.
   - The reverse order is defensible if the first Pro users are photographers.
4. **Whether P1 includes the optional LLM prompt bar**, or ships rules-only.
   - Recommendation: rules-only in P1. Add the 1.1 GB LLM in P3 after the 300-command evaluation.
5. **Grey-zone models:** BiRefNet_lite (DIS5K), GFPGAN (FFHQ). Ship them as optional downloads with a note, or hold them back.
