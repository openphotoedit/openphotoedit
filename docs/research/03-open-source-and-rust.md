# 03 — Open-source landscape and the Rust ecosystem

Research for OpenPhotoshop: a locally-run, open-source photo editor with a Rust core (WebAssembly in the browser, native via Tauri or a local server), Lite and Pro profiles, aiming at Photoshop's feature set over time.

Snapshot date: **2026-09-14**.

**How the numbers were collected.**
- GitHub stars, licences, push dates, releases and tags come from the GitHub REST API (`gh api`), pulled today.
- Crate downloads, versions and licences come from the crates.io API, pulled today. "DL" means all-time downloads and "90d" means recent downloads.
- GIMP and Krita are developed on GitLab (gitlab.gnome.org and invent.kde.org), so their GitHub star counts are mirror counts and understate interest.
- Complaint signals come mostly from the most-reacted issues on each project's tracker. Reddit could not be fetched reliably from this environment, so a few Reddit-style claims rest on press or forum articles instead.
- Anything I could not confirm first-hand is marked **(unverified)**.

---

## Executive summary

1. **Nobody offers "open-source Photoshop in the browser" yet.** The best free web Photoshop is Photopea. It is closed source and paid for by ads. Its most-reacted GitHub issue ever is *"Make Photopea Self Hosted?"*, with 138 reactions ([#4870](https://github.com/photopea/photopea/issues/4870)). The best open-source layered editor is GIMP 3.2. It is desktop-only (C/GTK), cannot import PSD layer styles or adjustment layers (work items [12606](https://gitlab.gnome.org/GNOME/gimp/-/work_items/12606) and [15505](https://gitlab.gnome.org/GNOME/gimp/-/work_items/15505) are open), and ships no built-in AI.
2. **Graphite (27.2k stars, Rust, wasm + wgpu + Vello) is the closest thing to our architecture, and it is a real threat.** In the last three months it merged 203 PRs, including Photoshop-matched Curves, Color Balance, Selective Color and Levels nodes, plus a GPU brush rewrite. But it is node- and vector-first. Raster selection tools and masking are still roadmap items ("Beta 1"), RAW is "Beta 2", and PSD import is an "LTS" item. Its crates are **not published** (`publish = false`), apart from `rawkit`, which only decodes Sony ARW. The code is **MIT OR Apache-2.0**, so we can vendor it legally, but it is not packaged as a library.
3. **New Rust and web entrants appeared in 2025–26.**
   - **RapidRAW** (10.0k stars, AGPL-3.0, Rust + wgpu + Tauri): Lightroom-class RAW editing with AI masks, but no layers.
   - **Darkly** (320 stars, AGPL-3.0, Rust + Svelte + wasm + WebGPU): a painter-first "Photoshop alternative", in beta.
   - **PaintFE** (377 stars, MIT, Rust + egui + wgpu): a Paint.NET-class editor.
   - **Schist** (20 stars, MIT, Rust + GPUI): claims PSD support and "all nine layer effects".

   None of them combines PSD fidelity, RAW, layers and local AI in the browser.
4. **The Rust stack is now good enough for the core.**
   - `image` and `zune-*` for codecs; `moxcms` (ICC, now a hard dependency of `image`) for colour management.
   - `wgpu` 30 and `vello` 0.10 for GPU work; `parley`, `swash` and `harfrust` for type; `kurbo` for paths.
   - `jxl-rs` (the JPEG XL decoder Chromium and Firefox use), `exr`, `resvg` and `hayro` for formats.
   - `ort` with `ort-web` (ONNX Runtime Web) for AI.

   The real gaps are:
   - a **PSD writer** with layer styles, 16-bit and PSB. `ag-psd` (Rust) is a fresh AI-written port with 6 stars; the upstream JS library lacks 16-bit and PSB; **PhotoshopAPI** (C++, BSD-3) is the best reference.
   - **selection and retouch algorithms** (GrabCut, PatchMatch healing, guided filter, local Laplacian), which have no maintained Rust crates.
   - **RAW in wasm**: `rawler` covers 857 camera modes but is LGPL-2.1 and untested on wasm.
5. **The wasm memory verdict: design for tiles, not Memory64.** Memory64 shipped in Chrome 133 and Firefox 134 but is **not supported in Safari**, including 26.6 and 27 TP. SpiderMonkey measured it at 10% to over 100% slower because bounds checks come back. The 16 GB JS API cap would not rescue a 20-layer 16-bit document anyway. Use 32-bit wasm (4 GB), a tiled, compressed layer store spilling to OPFS, and GPU textures for only the visible level of detail.
6. **Licensing traps** (details in §C.2):
   - GPL: GIMP, GEGL filters, Resynthesizer, darktable, RawTherapee/ART, Krita, ComfyUI, krita-ai-diffusion, chaiNNer, Fooocus.
   - LGPL: `rawler`, `rawloader`, `imagepipe`, LibRaw, lensfun, libheif, `seamcarving`.
   - AGPL or commercial: Imazen `zen*`, `heic`, `jxl-encoder`, `darkly`, RapidRAW, Upscayl.
   - Other: CC-BY-SA-3.0 on the lensfun database; GPL-3 on the `jpegxl-rs` and `rexiv2` wrappers; **software patents** on PatchMatch/Content-Aware Fill (Adobe) and HEVC (HEIC decode).
   - An **AGPL-3.0** app can absorb all of the GPL, LGPL and AGPL code. A **permissive** app has to clean-room it.

---

# Part A — Open-source editors

## A.0 Scoreboard

| Project | Stars (2026-09-14) | Last release | Licence | Stack | Platform | Activity |
|---|---|---|---|---|---|---|
| GIMP | 6.4k GitHub mirror; 299 on GitLab | **3.2.6**, 2026-09-10 | GPL-3.0+ (libgimp LGPL) | C, GTK3, GEGL/babl | Desktop | Very active |
| Krita | 10.4k mirror; 93 on invent | **5.3.3** stable, 2026-07-29; **6.0.x** Qt6 (tag v6.0.4) | GPL-3.0 | C++, Qt5/Qt6 | Desktop, Android | Very active |
| darktable | 13.1k | **5.6.1**, 2026-08-27 | GPL-3.0 | C, GTK, OpenCL | Desktop | Very active |
| RawTherapee | 4.2k | **5.13**, 2026-07-26 | GPL-3.0 | C++, GTK | Desktop | Active |
| ART | 470 | **1.26.8**, 2026-08-23 | GPL-3.0 (RawTherapee fork) | C++, GTK | Desktop | Active, single lead |
| **Graphite** | **27.2k** | No versioned releases; rolling alpha at editor.graphite.art; desktop app at RC stage | **MIT OR Apache-2.0** (workspace); GitHub shows Apache-2.0 | Rust → wasm, Svelte/TS, wgpu, Vello; desktop uses CEF + winit | Web + desktop RC | Extremely active (203 merged PRs since 2026-06-01) |
| **RapidRAW** | **10.0k** | **v1.6.3**, 2026-09-03 | AGPL-3.0 | Rust (1.2 MB), TS/React, WGSL, Tauri 2 | Win/mac/Linux/Android | Extremely active |
| Pinta | 4.0k | **3.1.2**, 2026-04-02 | MIT | C#, .NET, GTK4 | Desktop | Active |
| PixiEditor | 8.0k | **2.1.2.4**, 2026-09-04 | LGPL-3.0 | C#, Avalonia | Desktop | Very active |
| Photoflare | 468 | v1.7.4, 2026-08-03 | GPL-3.0 | C++, Qt | Desktop | Low but alive |
| LazPaint | 549 | v7.3, 2025-05-24 | GPL-3.0 | Pascal (Lazarus) | Desktop | Low |
| Paint.NET | n/a (closed) | 5.1.12 stable, 2026-03-08; 5.2 alpha (build 9719, Aug 2026) | **Freeware, not OSS** | C#, Direct2D | Windows | Active |
| digiKam | 804 on the archived mirror (moved to invent.kde.org) | **9.1.0**, 2026-06-06 | GPL-2.0+ | C++, Qt | Desktop DAM | Active |
| Shotwell | 166 mirror | tag 33.0, 2026-09-13 | LGPL-2.1 | Vala, GTK | Linux | Maintenance |
| PhotoDemon | 2.3k | (unverified) | BSD-style | VB6 | Windows | Active |
| miniPaint | 3.4k | v4.14.3, 2026-04-20 (previous release 2024-03) | MIT | Vanilla JS, canvas | Web | Sporadic |
| Filerobot Image Editor | 1.9k | v4.9.1, 2024-12-30; npm 5.0.0-beta.159, 2026-06-02 | MIT | React, Konva | Web SDK | Vendor-maintained (Scaleflex) |
| TOAST UI Image Editor | 7.7k | v3.15.3, 2022-04-25 | MIT | fabric.js | Web SDK | **Archived** |
| Photopea | 8.4k (issues-only repo) | Continuous | **Proprietary**, ad-funded + premium | JS/WebGL | Web | Very active, solo developer |
| vue-fabric-editor | 8.0k | (unverified) | MIT | Vue + fabric.js | Web (template/poster designer) | Active |
| bitmappery | 342 | n/a | MIT | TS | Web | Solo |
| **Darkly** | 320 (created 2026-05) | Beta, crate 0.8.0 | AGPL-3.0 | Rust + Svelte + wasm + WebGPU | Web | Very active |
| **PaintFE** | 377 (created 2026-02) | v1.3.11, 2026-08-10 | MIT | Rust, egui, wgpu, Rhai | Win/Linux (wasm target present) | Very active |
| Schist | 20 | Alpha | MIT | Rust, GPUI | Desktop | Active |
| IOPaint (formerly lama-cleaner) | 23.3k | iopaint-1.5.3, 2024-11-23 | Apache-2.0 | Python, React | Local web UI | **Archived 2025-08-13** |
| InvokeAI | 28.2k | **v6.14.1**, 2026-09-06 | Apache-2.0 | Python, React | Local web UI | Active; community-stewarded since the company wound down on 2025-10-31 |
| ComfyUI | 133k | **v0.35.0**, 2026-09-09 | GPL-3.0 | Python | Local web UI | Extremely active |
| Fooocus | 53.1k | v2.5.5, 2024-08-12 | GPL-3.0 | Python, Gradio | Local | Frozen (LTS / bug fixes only) |
| krita-ai-diffusion | 10.6k | **v1.53.0**, 2026-08-22 | GPL-3.0 | Python plugin + ComfyUI | Krita | Very active |
| Upscayl | 49.2k | v2.15.0, 2024-12-25 (tag v2.15.1 later) | AGPL-3.0 | Electron, TS, Real-ESRGAN-ncnn | Desktop | Pushes continue, releases stalled |
| rembg | 24.7k | v2.0.84, 2026-09-08 | MIT | Python, ONNX | CLI/lib | Active |
| chaiNNer | 6.0k | v0.25.1, 2025-10-23 | GPL-3.0 | Python, Electron | Desktop node graph | Slow |
| OpenVINO AI plugins for GIMP | 800 | n/a | Apache-2.0 | Python | GIMP 3 | Active (Intel) |
| GIMP-ML | 1.6k | n/a | MIT | Python | GIMP | Stale since 2024-10 |
| Resynthesizer | 1.9k | n/a | GPL-3.0 | C | GIMP plugin | Maintained |

---

## A.1 GIMP 3.x

- **Repo:** https://gitlab.gnome.org/GNOME/gimp (GitHub mirror: https://github.com/GNOME/gimp)
- **Version:** 3.0 shipped in March 2025. **3.2.0 shipped on 2026-03-14.** Bug-fix releases followed: 3.2.2 (2026-03-28), 3.2.4 (2026-04-19) and **3.2.6 (2026-09-10)**. The 3.2 branch is now fixes-only, and new features target 3.4.
- **Licence:** GPL-3.0-or-later for the app. libgimp and GEGL/babl are LGPL-3.0.
- **Stack:** C, GTK3, GEGL (a node-based, floating-point, tiled image-processing graph) and babl (pixel-format conversion).
- **Headline features in 3.x:**
  - **Non-destructive (NDE) filters** on layers, groups, channels, and the new link, text and vector layers. Groups support pass-through, which gives an adjustment-layer-like workflow. 3.2 adds a GEGL filter browser.
  - **Link layers** (like linked smart objects) and **vector layers**, both new in 3.2.
  - High-bit-depth, linear-light processing through GEGL. MyPaint brushes v2.
  - 3.2 also adds PSB import, `.acv`/`.alv` Curves and Levels preset import, APNG, and JPEG 2000 export.
- **CMYK status:** Colour management works through babl, with CMYK soft-proofing and a CMYK colour selector; 3.2 adds a Total Ink Coverage readout. **Editing stays RGB-internal**: there is no native CMYK document mode like Photoshop's (unverified whether a CMYK export path beyond soft-proof and CMYK JPEG/TIFF exists).
- **Better than Photoshop:** free and scriptable (Python 3, Script-Fu), GEGL's float-first pipeline, a huge format list, Linux-native, extensible via G'MIC.
- **Missing versus Photoshop:**
  - No true adjustment layers or smart objects with embedded filters (NDE filters come close).
  - No layer styles, no content-aware fill built in (the GPL Resynthesizer plugin fills that role), no generative AI, and a weak selection toolset (Quick Selection is its most-upvoted open request, [#2912](https://gitlab.gnome.org/GNOME/gimp/-/work_items/2912)).
  - No artboards, no Camera Raw equivalent (it hands off to darktable or RawTherapee), and no web version.
- **PSD fidelity:** medium-low.
  - Import: layers, groups, masks, blend modes, 8 and 16-bit, and PSB since 3.2.
  - **Layer styles are not imported** ([12606](https://gitlab.gnome.org/GNOME/gimp/-/work_items/12606), [16339](https://gitlab.gnome.org/GNOME/gimp/-/work_items/16339)), and neither are **adjustment layers** ([15505](https://gitlab.gnome.org/GNOME/gimp/-/work_items/15505)). Text arrives rasterised (unverified for 3.2).
  - Legacy blend-mode mismatches are tracked in [10505](https://gitlab.gnome.org/GNOME/gimp/-/work_items/10505).
- **Non-destructive:** partial but real (NDE filters, link and vector layers).
- **AI:** none built in. Third-party options are Intel's OpenVINO plugins for GIMP 3 (super-resolution, semantic segmentation, SD 1.5/SDXL inpainting; Apache-2.0) and GIMP-ML (stale).
- **Recurring complaints:**
  1. It is not Photoshop-compatible in workflow or PSD styles, which is why the PhotoGIMP patch has **17.9k stars** ([Diolinux/PhotoGIMP](https://github.com/Diolinux/PhotoGIMP)).
  2. It lacks a modern quick or AI selection ([#2912](https://gitlab.gnome.org/GNOME/gimp/-/work_items/2912)), and the foreground select tool has bugs ([#5492](https://gitlab.gnome.org/GNOME/gimp/-/work_items/5492)).

## A.2 Krita

- **Repo:** https://invent.kde.org/graphics/krita (mirror: https://github.com/KDE/krita)
- **Version:** Krita 5.3.0 and 6.0.0 shipped together on 2026-03-24. 5.3 is built on Qt5; 6.0 is the Qt6 port with native Wayland, Wayland colour management, HDR and 10-bit output. **Stable is 5.3.3 (2026-07-29).** 6.0.x (6.0.2 in May, tag v6.0.4 now) is labelled experimental, with the Qt6 line expected to become the default by the end of the year.
- **Licence:** GPL-3.0.
- **Stack:** C++, Qt, tiled layer engine with swap, OpenGL canvas, Python plugins.
- **Headline features:** a best-in-class brush engine; filter layers, filter masks, transform masks, fill layers and file layers (a real non-destructive stack); OCIO/LUT support; HDR/scene-linear painting; animation. 5.3/6.0 add on-canvas text editing.
- **Better than Photoshop:** painting, the brush engine, HDR painting, wrap-around mode, animation. Free.
- **Missing versus Photoshop:** photo retouch tools (healing is basic, there is no content-aware fill or camera-raw stage), a weaker photo-selection toolset, weaker type and vector tools, and no web build. The 2026 roadmap focuses on mobile QML UI, SQLite file storage, HDR and region-based updates for big images. It says **nothing about photo editing, PSD or AI**.
- **PSD fidelity:** medium. Layers, groups, masks, many blend modes and 16-bit import well. Krita has PS-compatible layer styles and can load `.asl` style libraries. Text and adjustment layers are lossy (unverified in detail).
- **Non-destructive:** strong (filter, transform and fill layers and masks).
- **AI:** none in core. **krita-ai-diffusion** (Acly, 10.6k stars, GPL-3.0, v1.53.0 2026-08-22) is the best OSS "AI inside a layered editor". It offers selection-based inpaint and outpaint, live painting, tiled upscaling, Flux 2, Z-Image, SD 1.5/XL, edit models, ControlNet, IP-Adapter and regional prompts, with a local ComfyUI backend or a paid cloud (interstice.cloud).
- **Recurring complaints:**
  1. The Qt6 build is labelled experimental, so users must pick between 5.3 and 6.0 ([Linuxiac](https://linuxiac.com/krita-6-0-2-released-as-qt-6-build-5-3-2-remains-production-choice/)).
  2. Performance on large images is a known pain point, which is why "region based updates" is on the [2026 roadmap](https://krita.org/en/posts/2026/roadmap-2026/).

## A.3 RAW developers: darktable, RawTherapee, ART (and RapidRAW)

| | darktable | RawTherapee | ART | RapidRAW |
|---|---|---|---|---|
| Version | 5.6.1 (2026-08-27) | 5.13 (2026-07-26) | 1.26.8 (2026-08-23) | v1.6.3 (2026-09-03) |
| Licence | GPL-3.0 | GPL-3.0 | GPL-3.0 | **AGPL-3.0** |
| Pipeline | Scene-referred by default: exposure → colour calibration (CAT) → **filmic rgb / sigmoid / AgX** display transform. OpenCL. | Display-referred heritage plus a scene-oriented "Abstract profile", CIECAM Colour Appearance & Lighting, gamut compression; new Michaelis-Menten tone mapper in 5.13 | RT fork simplified for usability; its own local-editing masks, CTL/LUT scripting | WGSL shaders on wgpu; scene-referred V-Log path for film emulation (Spektrafilm) |
| Tone mappers | filmic, sigmoid, **AgX** (5.6 retuned AgX defaults toward sigmoid and added sigmoid-like presets) | Michaelis-Menten, tone equaliser, log encoding | Log tone mapping (unverified details) | (unverified) |
| AI | **5.6 added optional AI** ([blog](https://www.darktable.org/2026/06/meet-darktable-5.6-ai-tools/)): SAM 2.1 / SegNext object masks (click to vector mask), NIND denoise, RealPLKSR/BSRGAN upscale, all local ONNX, models curated for provenance in [darktable-ai](https://github.com/darktable-org/darktable-ai), compile-out with `USE_AI` | None | None | AI masks (subject, sky, depth), AI lens blur, generative inpaint crop workflow, edge-aware mask refinement |
| Layers | No (it is a pixel pipeline with drawn and parametric masks) | No | No | No |
| RAW decoding | rawspeed + LibRaw | Its own dcraw-derived code + LibRaw | Same as RT, + LibRaw | **`rawler`** (dnglab fork) |

- **Better than Photoshop / ACR:** the scene-referred maths is more rigorous (darktable) and more transparent (open maths, open colour science). Everything is fully local, with non-destructive sidecars. darktable's AI model provenance policy is stronger than Adobe's.
- **Missing versus Photoshop:** no compositing, layers or type. The UI is complex; darktable's UI-redesign issue [#8497](https://github.com/darktable-org/darktable/issues/8497) and GTK4 migration [#15920](https://github.com/darktable-org/darktable/issues/15920) are among its most-reacted. CR3 support was the most-wanted issue for years ([#2170](https://github.com/darktable-org/darktable/issues/2170)).
- **PSD fidelity:** none (TIFF/EXR hand-off).
- **RapidRAW** is the notable 2025–26 story. It is Rust, wgpu and Tauri, under 20 MB, now on Android. Recent changes include liquify, a skin retouch tool, focus stacking, tethering, lensfun lens correction, guided perspective, JXL export (using `jxl-oxide` plus Imazen's `jxl-encoder`), `ort` for AI, and `little_exif`/`kamadak-exif`. Its README says it is "not yet as polished as mature tools like Darktable, RawTherapee". Its top historical requests were lens corrections ([#112](https://github.com/CyberTimon/RapidRAW/issues/112)), X-Trans RAF support ([#25](https://github.com/CyberTimon/RapidRAW/issues/25)) and Lightroom import ([#533](https://github.com/CyberTimon/RapidRAW/issues/533)).
- **Complaint signals:** RawTherapee users most want JPEG XL and EXR output ([#6273](https://github.com/RawTherapee/RawTherapee/issues/6273), [#1895](https://github.com/RawTherapee/RawTherapee/issues/1895)). Recurring darktable issues are about macOS slowness ([#7463](https://github.com/darktable-org/darktable/issues/7463), [#6068](https://github.com/darktable-org/darktable/issues/6068)).

## A.4 Graphite — deep study

**Repo:** https://github.com/GraphiteEditor/Graphite · **Web app:** https://editor.graphite.art · **Site:** https://graphite.art (formerly graphite.rs)

### Status in September 2026
- **27.2k stars.** Very active: 203 merged PRs from 2026-06-01 to 2026-09-14, with daily commits (for example "New node: Color Balance", #4527, 2026-09-13).
- **No versioned GitHub releases.** It ships a rolling alpha. The roadmap puts the project around Alpha 4/5: "performance, animation, desktop RC1", then "desktop app release candidates RC2-6" and a "1.0 launch".
- **Desktop app:** the tracking issue [#2535](https://github.com/GraphiteEditor/Graphite/issues/2535) is its most-reacted issue. The design renders the editor UI with **CEF** (Chromium Embedded Framework, `cef = "151"`) to an offscreen texture composited over a native wgpu viewport, using `winit`. Web and desktop are both first-class.
- **LWN review (2025-12-26):** "raster tools remain experimental… full non-destructive raster editing hasn't been implemented yet". Exports are limited to PNG, JPEG and SVG, and it cannot import GIMP, Scribus or Inkscape native files ([LWN](https://lwn.net/Articles/1051242/)).

### Architecture (from the workspace `Cargo.toml` and source tree)
- **Frontend:** Svelte and TypeScript, kept deliberately thin. All editor logic is Rust compiled to wasm (`wasm-bindgen` pinned at 0.2.121) through `frontend/wrapper`.
- **`editor` crate:** a message-passing architecture. The tool FSMs, document message handlers and a "portfolio" of documents all emit messages. The document *is* a node graph: layers are a user-facing view of a graph.
- **Graphene** is the node-graph language and runtime.
  - `node-graph/graph-craft` compiles a document network into a proto-network: it resolves types, flattens nested networks and deduplicates identical subgraphs by hash (`graphene-hash`).
  - `interpreted-executor` runs the graph with dynamic dispatch through `dyn-any`, a type-erased `Any` that works without `'static`.
  - `node-macro` provides `#[node_macro::node]`, which turns a plain Rust function into a node with typed parameters. `preprocessor` handles graph-level rewriting.
  - Nodes are grouped in crates: `gcore`, `gstd`, `raster`, `blending`, `brush`, `vector`, `path-bool`, `text`, `transform`, `repeat`, `math`, `graphic`.
  - Type crates: `raster-types`, `vector-types`, `graphic-types`, `brush-types`, `no-std-types` (the colour and blend-mode maths are `no_std` so they can compile to GPU shaders).
- **Rendering:**
  - `node-graph/libraries/rendering` converts graph output to **Vello** scenes (`vello` 0.10, `to_peniko.rs`) or SVG (`usvg`/`resvg` 0.47).
  - `wgpu-executor` runs on **wgpu 29** with the `spirv` feature.
  - Raster GPU nodes are written in **Rust-GPU** (`spirv-std` 0.10-alpha, `cargo-gpu`; `node-graph/nodes/raster/shaders`), so the same Rust function runs on CPU and GPU.
  - Graphite also carries `wgpu-sync` and uses the `fragile-send-sync-non-atomic-wasm` feature because wasm is single-threaded here.
- **Text:** `parley` 0.9 plus `skrifa`. **Geometry:** `kurbo` 0.13, `lyon_geom`, and its own `path-bool` crate for boolean operations.
- **RAW:** `libraries/rawkit` (crate `rawkit` 0.1.0, 2024-11) handles **only Sony `.arw`**.
- **Document format:** a new `document/format`, `graph-storage` and `container` split (unverified details of on-disk format stability).

### What raster features exist today (from `node-graph/nodes/raster/src`)
- **Adjustments (nodes):** `levels`, `curves` (with a transfer-curve widget, added Sept 2026), `brightness_contrast` and `brightness_contrast_classic`, `hue_saturation`, `vibrance`, `exposure`, `channel_mixer`, `selective_color`, `color_balance`, `black_and_white`, `gradient_map`, `posterize`, `threshold`, `invert`, `gamma_correction`, `luminance`, `extract_channel`, `make_opaque`.
  - These deliberately reproduce **Photoshop's integer arithmetic**. Test names include `combined_tones_use_integer_arithmetic`, `ties_follow_the_unrounded_fixed_point_luma` and `preserve_luminosity_makes_sliders_relative`. That makes them a useful clean reference because the licence is permissive.
- **Filters:** `blur`, `median_filter`, `dehaze`. **Blending:** `mix`, `color_overlay`; the blend-mode formulas live in `no-std-types/src/blending.rs`. A recent PR fixed the Overlay, whole-colour and alpha-only modes.
- **Other:** `mask`, `combine_channels`, `sample_image`, `noise_pattern`, `extend_image_to_bounds`.
- **Brush:** summer 2026 PRs replaced the legacy brush with a "Basic Brush" node, added "caching infrastructure for the GPU brush nodes" and new brush-stroke data types, and added tablet input on web.
- **Not yet built:** selection tools and marquee masking, clone/heal/history brush (visible in the toolbar but disabled), RAW (beyond ARW), PSD/TIFF import, layer styles, 16/32-bit document workflows (unverified), tiled huge-image handling (unverified), and AI. The "neural nodes/tools like Magic Wand" item sits on the LTS roadmap (issue [#1694](https://github.com/GraphiteEditor/Graphite/issues/1694)).

### Roadmap (from https://graphite.art/features/)
- Alpha 5: GPU-accelerated raster rendering, improved brush with pressure and tilt, basic PDF export.
- **Beta 1:** raster adjustments, filters and effects; **selection tools and marquee masking**; broader SVG filters.
- **Beta 2:** **raw photo processing**, history brush, clone stamp.
- **LTS:** imports of **PDF, EPS, AI, DXF, PSD, TIFF**; neural tools; rotoscoping; live video compositing.
- No dates are given. Keyframe animation is slated for "late 2026".

### Are Graphite's crates reusable?
- **Legally, yes.** The workspace declares `license = "MIT OR Apache-2.0"`, with `LICENSE-APACHE` and `LICENSE-MIT` at the root, so it is compatible with both AGPL and permissive apps. Keep attribution; Graphite runs `cargo-about` for third-party notices.
- **Practically, only in pieces.**
  - Every crate has `publish = false` and `version = "0.0.0"`, so we would vendor via git and track a fast-moving, unversioned API.
  - The coupling is tight: `dyn-any`, the node macro and the graph compiler.
  - Most extractable: `no-std-types` blending and colour maths, the `raster` adjustment nodes (as reference implementations), `path-bool`, `math-parser`, `rawkit` (weak), and the Vello/usvg bridge in `rendering`.
  - Least extractable: the editor and message system, which is the whole product.
- **Strategic reading:** Graphite is chasing *Blender for 2D* (procedural first, then raster). We are chasing a *Photoshop-grade layered photo editor* (PSD, selection, retouch, RAW, AI). Their Beta 1 and Beta 2 overlap with our first 12 months. **Assume Graphite ships basic raster selection and RAW within about 12–24 months (unverified forecast).** The differentiation has to be PSD round-trip, Photoshop muscle memory (layers panel, adjustment layers, layer styles, selection refine), photo retouching and local AI, not "Rust in the browser".
- **Complaints:** users most want the desktop app ([#2535](https://github.com/GraphiteEditor/Graphite/issues/2535)), Linux packaging ([#584](https://github.com/GraphiteEditor/Graphite/issues/584)) and tablet support ([#1545](https://github.com/GraphiteEditor/Graphite/issues/1545)). A desktop RC2 Flatpak was reported as a "system HOG" ([#3686](https://github.com/GraphiteEditor/Graphite/issues/3686)). Critics note non-destructive claims do not yet hold for raster (via the XDA and itsfoss reviews).

## A.5 Simple desktop editors

- **Pinta** (MIT, C#/.NET/GTK4, 3.1.2)
  - A Paint.NET 3.0 clone: layers, blend modes, effects, add-ins.
  - No adjustment layers, no PSD (no PSD code found in the repo), no AI.
  - Complaints: selection-resize behaviour ([#585](https://github.com/PintaProject/Pinta/issues/585)), no zoom-to-cursor ([#1027](https://github.com/PintaProject/Pinta/issues/1027)), HiDPI UI ([#1365](https://github.com/PintaProject/Pinta/issues/1365)).
  - Worth noting: the Paint.NET 3.36 effects code is MIT and reusable.
- **Photoflare** (GPL-3, Qt): a quick editor with batch processing and minimal layers (unverified). Low activity.
- **LazPaint** (GPL-3, Lazarus/Pascal, v7.3): raster plus vector layers, Paint.NET-like; its file-extension code references PSD (import depth unverified).
- **Paint.NET** (freeware, **closed source** since 3.36 in 2009):
  - 5.1.12 is stable; 5.2 is in alpha, preparing for "6.0".
  - It has a strong GPU effect system and a large plugin ecosystem. Not reusable, but a UX reference for Lite.
- **PixiEditor** (LGPL-3, 8.0k, C#/Avalonia, 2.1.x): a "universal 2D editor" with a node graph, vector, pixel art and animation. Not a photo editor.
- **PhotoDemon** (BSD-style, VB6, Windows): surprisingly deep photo toolset; a reference only.
- **digiKam 9.1** (GPL-2, DAM) and **Shotwell** (LGPL-2.1): organisers with light editing. digiKam has AI face recognition and auto-tagging (unverified for 9.x). Relevant only if Pro adds a library.

## A.6 Web editors

- **miniPaint** (MIT, 3.4k)
  - Vanilla JS canvas editor: layers, effects, basic tools.
  - 8-bit canvas only, no PSD (no PSD code found), no non-destructive editing.
  - Its release gap (2024-03 to 2026-04) signals near-dormancy. Top request: better selection ([#312](https://github.com/viliusle/miniPaint/issues/312)).
- **Filerobot Image Editor** (MIT, 1.9k): an embeddable React SDK from Scaleflex for crop, finetune, filters, annotate and resize. It is a single-image annotator, not a layered compositor. v5 has been in beta for a long time (npm 5.0.0-beta.159). About 154k npm downloads a month.
- **TOAST UI Image Editor** (MIT, 7.7k): fabric.js-based, **archived**, last release 2022. The issue "Is this library still maintained?" ([#927](https://github.com/nhn/tui.image-editor/issues/927)) says it all. Still about 138k npm downloads a month from legacy use.
- **Photopea** (proprietary; issues repo 8.4k):
  - Solo developer Ivan Kutskir, launched 2013. About $3M a year in 2024, roughly 90% from ads, and 17M+ monthly visitors (per Wikipedia and interview sources).
  - **Best-in-class web PSD fidelity**, plus AI, XD, Sketch, Figma, XCF, KRI, CLIP, SAI, AFPHOTO and PDN import, RAW (DNG, NEF, CR2, CR3, ARW, RW2, RAF, ORF…), JXL, HEIC, AVIF and EXR.
  - Premium removes ads, adds 5 GB PeaDrive storage, doubles history length and gives more AI credits.
  - **Not open source.** The top asks are self-hosting ([#4870](https://github.com/photopea/photopea/issues/4870), 138 reactions) and open-sourcing ([#5328](https://github.com/photopea/photopea/issues/5328)), plus autosave ([#760](https://github.com/photopea/photopea/issues/760)). *This is the clearest proof of demand for our wedge.*
- **Pintura** (commercial SDK): a polished crop, filter and annotate component. Not relevant beyond UX reference.
- **vue-fabric-editor** (MIT, 8.0k): a Canva-like poster and template designer, not photo editing.
- **bitmappery** (MIT, 342): a small but serious non-destructive web editor with layers, masks and a timeline. A worthwhile code read.
- **Darkly** (AGPL-3, 320, created 2026-05, beta)
  - Rust + Svelte + wasm + WebGPU. A "Photoshop alternative where painters are first-class": node-based brush engine, layers, blend modes, masks, selection, and "veil" shader layers.
  - Painter-first, with no PSD or RAW focus (unverified). **It proves the Rust+WebGPU browser editor is buildable now**, and it is the nearest AGPL peer.
- **GitHub topic search:**
  - `photoshop-clone` contains only zero-star repos.
  - `photoshop-alternative` holds a handful of 0–3-star repos from 2026, which suggests an AI-assisted wave of clones with no traction.
  - `image-editor` and `photo-editor` are dominated by Graphite, ImageToolbox (Android), jspaint (MS Paint clone, 7.9k), TOAST UI and Pinta.
  - **No open-source web Photoshop clone has more than 1k stars with layered PSD editing.**

## A.7 AI-oriented OSS image tools — which editing workflows they enable

| Tool | Licence | What it enables | Relevance to us |
|---|---|---|---|
| **IOPaint** (lama-cleaner) | Apache-2.0, **archived 2025-08** | Object removal with LaMa/MAT/MI-GAN/SD inpaint, plugins (RemBG, RealESRGAN, GFPGAN, SAM interactive seg), local web UI | The model list and its UX (brush over object → erase) are the "Remove tool" template. LaMa (Apache-2.0) and MI-GAN (MIT, unverified) are the key light models. Archival means an opening. |
| **InvokeAI** 6.14 | Apache-2.0 | **Unified Canvas**: infinite canvas, generations as layers, inpaint and outpaint masks, regional prompts, pressure canvas; FLUX.2 Klein edit models, Wan 2.2 video | The best reference for "generative layers" UX. Permissive licence, so UI patterns are free to copy. |
| **ComfyUI** 0.35 | GPL-3.0 | A node-graph backend for any diffusion or edit workflow; LayerStyle and LayerForge nodes (MIT) imitate PS layers inside ComfyUI | Pro could *talk to* a user-run ComfyUI over HTTP. Process separation avoids GPL linkage (get counsel for the permissive case). |
| **Fooocus** | GPL-3.0, frozen | One-click inpaint/outpaint with its own inpaint head | Historical. Its UX simplicity is the lesson. |
| **krita-ai-diffusion** | GPL-3.0 | Selection-bounded generative fill, live painting, regions, tiled upscale, via ComfyUI | The **gold-standard UX for AI inside a layered editor**. |
| **GIMP AI plugins** (OpenVINO) | Apache-2.0 | Super-resolution, segmentation, SD inpainting on Intel hardware | Shows GIMP's AI story is all plugins. |
| **Upscayl** | AGPL-3.0 | Real-ESRGAN-family upscaling via ncnn/Vulkan, desktop | Upscale models. Releases stalled since 2024-12. |
| **rembg** | MIT | Background removal with U²-Net, IS-Net, BiRefNet (unverified list), SAM; ONNX | Its ONNX model zoo maps directly onto `ort-web`. Model licences vary (BiRefNet MIT; BRIA RMBG is **non-commercial**, a trap). |
| **chaiNNer** | GPL-3.0 | Node-graph batch image processing (upscale, convert, ONNX/PyTorch/NCNN) | Batch and actions reference. |
| **darktable 5.6 AI** | GPL-3.0 (models listed separately) | SAM 2.1 click-to-mask, NIND denoise, upscale; ONNX with statically-shaped, tiled manifests | **A strong template for shipping local ONNX models responsibly**: provenance repo, optional download, tile sizes in a manifest. |

---

# Part B — Rust crate ecosystem

"wasm" means it compiles to `wasm32-unknown-unknown` for the browser.
- ✅: pure Rust, known to work.
- ⚠️: possible with effort (C dependencies via `cc`/emscripten, threads, or filesystem assumptions).
- ❌: native-only.

## B.1 Core imaging

| Crate | DL (90d) | Latest | Licence | wasm | Gives us | Gaps |
|---|---|---|---|---|---|---|
| `image` | 188M (49M) | 0.25.10 (2026-03) | MIT/Apache | ✅ | Codec hub; `ImageBuffer<P>` for 8/16-bit and `Rgba32F`; **`moxcms` is now a mandatory dependency** (ICC-aware conversion) | Not a document model; no tiling; slow generic pixel loops; limited metadata |
| `imageproc` | 13M (2.8M) | 0.27.0 (2026-06) | MIT | ✅ | Filters (Gaussian, bilateral, median), morphology, distance transform, edges, contours, geometric transforms, connected components, **flood fill** (unverified that it is exposed for RGBA with tolerance), Hough, template matching | Mostly `u8`, CPU-only, no GrabCut, guided filter, Poisson or PatchMatch |
| `zune-image` family | zune-jpeg 109M; zune-image 97k | 0.5.x | MIT/Apache/Zlib | ✅ | Fastest pure-Rust JPEG decoder (used by `image`); PNG, PSD (simple composite), JPEG XL (lossless encoder), HDR, QOI; SIMD | `zune-image` as a framework is low-adoption; `zune-psd` reads only the flattened composite |
| `fast_image_resize` | 18.9M | 6.1.0 (2026-07) | MIT/Apache | ✅ (wasm SIMD128) | SIMD Lanczos3, CatmullRom and more for u8/u16/f32, alpha-aware | Resize only |
| `photon-rs` | 88k | 0.3.3 (2025-05) | Apache-2.0 | ✅ (designed for wasm) | 90+ filters and effects, a canvas bridge | 8-bit, toy-grade maths, slow release cadence |
| `pix` | 10.8M | 1.0.1 | MIT/Apache | ✅ | Typed pixel formats, compositing ops | Small |
| `pixels` | 658k | 0.17.2 | MIT | ✅ | Framebuffer to wgpu surface | For emulators, not editors |
| `kornia` / `kornia-image` / `kornia-imgproc` | 27k / 76k / 46k | 0.1.x | Apache-2.0 | ⚠️ | CV primitives (warp, colour, filters, features), typed `Image<T, C>` | Robotics focus; young API |
| `ndarray` + `rayon` | 120M / 543M | 0.17 / 1.12 | MIT/Apache | ✅ (rayon needs `wasm-bindgen-rayon` plus COOP/COEP for SharedArrayBuffer) | Tensors and parallelism | — |
| `palette` | 20.4M | 0.7.7 (2026-08) | MIT/Apache | ✅ | Typed colour spaces (sRGB/linear, Lab, LCh, Oklab, HSV, XYZ), blend, white points | No ICC |
| `color` (linebender) | 5.6M | 0.3.3 | Apache/MIT | ✅ | CSS Color 4 spaces, used by Vello and Graphite | No ICC |
| `moxcms` | **77M** (38M) | 0.9.0 (2026-07) | BSD-3/Apache | ✅ | **Pure-Rust ICC CMS**: matrix/TRC and LUT (A2B/B2A) profiles, CMYK, Lab, rendering intents, SIMD. Depended on by `image`. | GitHub repo has only 51 stars (bus-factor 1, awxkee); CMYK proofing and black-point compensation depth unverified |
| `lcms2` | 6.4M | 6.2.0 (2026-08) | MIT (Little CMS is MIT) | ⚠️ (C via `cc`; should compile to wasm32 with clang, unverified) | The reference CMS: every intent, BPC, device links, CMYK, named colours | C dependency |
| `qcms` | 3.1M | 0.3.0 (2024-01) | MIT | ✅ | Firefox's CMS: fast RGB/CMYK-to-RGB | Display-oriented, no proofing |
| `tintbox` | 1.2k | 0.5.0 (2026-06) | MIT | ✅ | Claims a full-parity pure-Rust lcms2 reimplementation | 5 stars, brand new; **unverified quality** |
| `kolor`, `empfindung` | small | stale | MIT | ✅ | Colour-space and ΔE maths | Stale |
| `libblur` | 99k | 0.24 | Apache/BSD-3 | ✅ | Fast Gaussian, box and stack blurs for u8/u16/f32 (same author as moxcms) | Blur only |
| `libvips` | 1.0M | 2.3.0 | MIT bindings; **libvips LGPL-2.1** | ❌ (wasm-vips exists separately) | Tiled, demand-driven huge-image processing, a great reference architecture | C, LGPL |
| `opencv` | 4.2M | 0.100 | MIT bindings; OpenCV Apache-2.0 | ❌ in practice | GrabCut, inpaint (Telea/NS), seamlessClone (Poisson), stitching, HDR, guided filter (contrib) | Heavy C++; opencv.js is a separate build |

## B.2 GPU

| Crate | DL | Latest | Licence | wasm | Notes |
|---|---|---|---|---|---|
| `wgpu` | 35M | **30.0.1** (2026-08-22) | MIT/Apache | ✅ WebGPU and WebGL2 backends | The foundation. Browser WebGPU (per the gpuweb implementation-status wiki and web.dev): Chrome/Edge on desktop and Android; **Safari 26** on macOS, iOS and iPadOS; Firefox 141 on Windows and 145 on macOS ARM, with Linux and Android promised for 2026. Spec default limits must be designed for: `maxTextureDimension2D` 8192, `maxBufferSize` 256 MiB, `maxStorageBufferBindingSize` 128 MiB. Graphite and RapidRAW pin wgpu 29 (RapidRAW downgraded "to prevent P3 color shifts on Apple devices"). |
| `naga` | 38.7M | 30.0.1 | MIT/Apache | ✅ | WGSL ↔ SPIR-V/MSL/HLSL/GLSL translation. `naga_oil` (Bevy) adds shader imports and defines. |
| `vello` | 855k | **0.10.0** (2026-08) | Apache/MIT | ✅ (needs WebGPU compute) | GPU compute 2D vector renderer; used by Graphite. |
| `vello_cpu` / `vello_hybrid` | 3.8M / 38k | 0.2.0 (2026-08) | Apache/MIT | ✅ | "Sparse strips" renderers. Hybrid (CPU geometry, GPU fill) was "roughly beta" in Linebender's Q1 2026 update and targets WebGL2-class GPUs. Good fallback for vector and shape layers and text. |
| `rust-gpu` (`spirv-std`, `cargo-gpu`) | 444k | 0.9/0.10-alpha | MIT/Apache | build-time | Write shaders in Rust and share code between CPU and GPU. Graphite uses it for raster nodes. Nightly toolchain and alpha stability. |
| `tiny-skia` | 47M | 0.12.0 | BSD-3 | ✅ | CPU Skia subset: paths, gradients, AA, masks. Used by resvg. Great for CPU fallback and thumbnails. |
| Dedicated GPU image-filter crates | — | — | — | — | **No maintained general "GPU image filter" crate exists.** `ff-render`, `oximedia-*` and `zenfilters` are tiny or AGPL. Expect to write our own WGSL kernels. Graphite's Rust-GPU raster nodes and RapidRAW's 93 KB of WGSL (AGPL) are the best references. |

## B.3 Formats

| Format | Crate(s) | DL | Licence | wasm | Status and gaps |
|---|---|---|---|---|---|
| **PSD read** | `psd` (chinedufn) | 163k | MIT/Apache | ✅ (has a browser demo) | Layers, groups, blend mode, opacity, RGB/Gray, 8-bit composite. Last release 2024-01. No layer styles, text, adjustments or smart objects, and **no write**. |
| PSD read (composite) | `zune-psd` | 61k | MIT/Apache/Zlib | ✅ | Flattened image only. |
| PSD raw | `rawpsd` | 3k | CC0 | ✅ | Minimal raw reader (used by warpainter). |
| **PSD read/write** | **`ag-psd` (Rust)** — [Vasyanator/ag-psd-rs](https://github.com/Vasyanator/ag-psd-rs) | 719 | MIT | ✅ (pure Rust) | Created 2026-06-28, 6 stars. The README says it is "vibe-coded… written by Claude… **not hand-audited**", yet used in production for comic typesetting. Claims PSD/PSB read and write, layer effects, text EngineData, vector data, adjustment layers, smart-object metadata, ABR/CSH/ASE, and PS 2026 long-form enums. **Upstream JS ag-psd lists: no 16-bit, no PSB, RGB-only write, no composite redraw**, so the Rust port's PSB claim is **unverified**. Treat it as a scaffold and fuzz it heavily. |
| PSD reference: JS | `ag-psd` (Agamnentzar) | npm 782k/month | MIT | — | The most battle-tested open **writer** of layer effects, text and vector data in 8-bit RGB. v31.0.2 (2026-07). |
| PSD reference: Python | `psd-tools` | — | MIT | — | Low-level read/write of the full structure; composites basic layers, fill effects and some adjustments. Useful as a **test oracle**. |
| PSD reference: C++ | **PhotoshopAPI** (EmilDohne) | 373 stars | **BSD-3** | — | "Read/write parser of PSD **and PSB** with fully fledged 8/16/32-bit support", smart objects, masks, Python bindings, v0.9.1 (2026-05). **The best permissive reference for a Rust PSB and high-bit writer.** |
| PSD reference: Ruby/JS/C++ | psd.rb (MIT, stale 2021), psd.js (npm now says UNLICENSED, a trap), @webtoon/psd (MIT, read-only, 2023), psd_sdk (BSD-2, read-only) | — | — | — | Read-side references. |
| OpenRaster (.ora) | No dedicated crate (`openraster` does not exist; `ora` is unrelated) | — | — | — | A ZIP of PNGs plus stack.xml. Trivial to implement with `zip`. |
| TIFF | `tiff` | 117M | MIT | ✅ | 8/16/32f, LZW/Deflate/PackBits; limited multi-page, CMYK and layered-TIFF (Photoshop layers in tag 37724) support. |
| PNG / APNG | `png` | 224M | MIT/Apache | ✅ | Mature. |
| JPEG decode | `zune-jpeg` (used by image), `jpeg-decoder` (legacy) | 109M / 88M | MIT/Apache(/Zlib) | ✅ | Mature. |
| JPEG encode | `jpeg-encoder` (Apache/MIT **and IJG**), `mozjpeg` / `mozjpeg-sys` (IJG + BSD, C), `jpegli-rs` (new) | 7.6M / 4.3M | — | ✅ / ⚠️ | jpeg-encoder is pure Rust. mozjpeg is best quality per byte but C. IJG licence needs an acknowledgement clause in docs. |
| WebP | `image-webp` (pure Rust decode plus lossless encode), `webp` (libwebp bindings, lossy encode) | 73M / 4.5M | MIT/Apache | ✅ / ⚠️ | Lossy encode needs libwebp (BSD) compiled to wasm. |
| AVIF | `ravif` (BSD-3) + `rav1e` (BSD-2) encode; `dav1d` (MIT bindings, C decoder BSD-2) or `rav1d` (BSD-2, Rust port) decode | 46M / 45M / 1.3M / 42k | BSD/MIT | ✅ encode (slow, single-threaded without threads); ⚠️ decode | AV1 is royalty-free (AOM). |
| HEIC/HEIF | `libheif-rs` + `libheif-sys` (MIT wrappers; **libheif LGPL-3**; libde265 LGPL-3; x265 **GPL-2**) | 752k | — | ⚠️ | **HEVC patent pools** apply to decode as well as encode; that is a legal risk distinct from copyright. Alternatives: `heic` (imazen, **AGPL-3 or commercial**, 13k DL), `heif-oxide` (MIT/Apache, 1 star, unverified quality). In-browser alternative: let Safari/Chrome decode HEIC via `ImageDecoder` where the OS supports it (unverified per platform). |
| **JPEG XL** | **`jxl`** (libjxl/jxl-rs, **BSD-3**, 276k DL): the decoder vendored in Chromium (Chrome 145, flag-gated) and Firefox; `jxl-oxide` (MIT/Apache, 2.2M DL, mature, used by rawler and RapidRAW) | — | — | ✅ | Encode: `zune-jpegxl` (lossless only, permissive); `jxl-encoder` (imazen, **AGPL or commercial**); `jpegxl-rs` (**GPL-3** wrapper around BSD libjxl, a trap); or build libjxl (BSD-3) via `cc`. |
| OpenEXR | `exr` | 71M | BSD-3 | ✅ | Pure Rust, multi-layer, deep-data read, tiles and mip levels. Mature. |
| GIF | `gif` | 132M | MIT/Apache | ✅ | Mature. |
| QOI | `qoi` | 64M | MIT/Apache | ✅ | Stable (last release 2022, format frozen). |
| SVG | `usvg` / `resvg` | 28M / 25M | Apache/MIT | ✅ | The best SVG renderer in any language for static SVG. Place-as-smart-object and SVG import. |
| PDF | `hayro` (Apache/MIT, pure-Rust interpreter and renderer, 2.2M DL); `pdfium-render` (MIT/Apache wrapper, needs PDFium binary or wasm build, BSD-3); `lopdf` (MIT, object-level read/write) | — | — | ✅ / ⚠️ / ✅ | Import PDF pages with hayro. Write PDF with lopdf, or `krilla` (Typst's PDF writer; not checked on crates.io, **unverified**). |
| EXIF/XMP/IPTC | `kamadak-exif` (BSD-2, read-only, 13.6M DL); `little_exif` (MIT/Apache, **read and write**, 365k); `nom-exif` (MIT repo; crates.io shows "non-standard", check); `rexiv2` (**GPL-3**, bindings to exiv2 GPL-2+, native) | — | — | ✅ / ✅ / ✅ / ❌ | No pure-Rust crate does full XMP round-trip. We would need an XMP packet preserver (treat XMP as opaque XML, patch known keys). |
| DNG write | `dng` (apertus, **AGPL-3.0**); `gamut-dng` (MIT/Apache, new, 464 DL, unverified) | — | — | ✅ | dnglab itself writes DNG (LGPL-2.1). |

## B.4 RAW

| Crate | DL | Latest | Licence | wasm | Assessment |
|---|---|---|---|---|---|
| **`rawler`** (dnglab) | 125k | 0.8.0 (2026-08-30) | **LGPL-2.1** | ⚠️ (depends on `memmap2`, `rayon`, `ureq`; likely feature-gatable, **unverified on wasm32**) | The **broadest pure-Rust decoder**: SUPPORTED_CAMERAS.md lists **857 supported model/mode rows** (Canon CR3 including R5 II, Nikon Z8/Zf, Fuji X-T5/X-T50, ARRI…). Includes basic demosaic and colour matrices. RapidRAW ships a fork. |
| `rawloader` + `imagepipe` | 318k / 118k | 0.37.2 / 0.5.1 (2026-08) | LGPL-2.1 / **LGPL-3.0** | ✅ likely (pure Rust, unverified) | The original pure-Rust pair (pedrocr). imagepipe is a full basic pipeline (demosaic → WB → matrix → gamma). Smaller camera list than rawler; CR3 support unverified. PaintFE uses both. |
| `rawkit` (Graphite) | 1.2k | 0.1.0 (2024-11) | MIT/Apache | ✅ | **Sony ARW only.** Permissive but tiny. |
| `quickraw` | 23k | 0.1.6 (2023) | LGPL-2.1 | ✅ | Stale. |
| `libraw-rs` / `rsraw` | 51k / 5k | 2021 / 2026-03 | MIT/Apache bindings; **LibRaw is LGPL-2.1 or CDDL-1.0** | ⚠️ (C++; emscripten builds of LibRaw exist, unverified) | Widest coverage overall, the industry default. Its AHD/DCB/AAHD demosaics are good; the best ones (AMaZE, RCD, LMMSE) live in RawTherapee/darktable under **GPL**. |
| `demosaic` | 1k | 0.3.0 (2026-02) | MIT/Apache | ✅ | Bayer and X-Trans algorithms (which ones is unverified), 3 stars. Promising, unproven. |
| `zenraw` | 484 | 0.2.0 | **AGPL or commercial** | — | Imazen. |

**Demosaic quality note.**
- Ranked roughly: bilinear < AHD ≈ VNG < DCB < **RCD** (Luis Sanz Rodríguez; its original C code is **GPL-3**) ≈ **AMaZE** (Emil Martinec; GPL-3 in RT) < ML demosaic.
- For X-Trans: Markesteijn 1/3-pass (dcraw heritage, public; darktable's copy is GPL).
- For a **permissive** licence, implement RCD and Markesteijn from the papers and descriptions rather than copying code. For AGPL, RT/darktable code can be adapted with attribution.

**wasm feasibility:** RAW decoding is CPU-bound, benefits from SIMD and threads, and a 50 MP Bayer frame is about 200 MB as `f32` RGB. It is feasible in 32-bit wasm with `wasm-bindgen-rayon`. The bigger issue is **colour science** (DCP/DNG profiles, camera matrices, lens correction), not decoding.

## B.5 Text (type tool)

| Crate | DL | Latest | Licence | wasm | Role |
|---|---|---|---|---|---|
| `harfrust` | 8.1M | 0.13.3 (2026-08) | MIT | ✅ | The HarfBuzz org's official Rust port. **Successor to `rustybuzz`**, whose last release was 0.20.1 in 2024-11. |
| `skrifa` (fontations) | 25.9M | 0.47 | MIT/Apache | ✅ | Google's font parsing, variable fonts, hinting; used by Chrome's Skia path. |
| `swash` | 12.8M | 0.2.10 | Apache/MIT | ✅ | Shaping, scaling and rendering glyphs, colour emoji. |
| `parley` | 3.0M | 0.11.1 (2026-08) | Apache/MIT | ✅ | Rich text layout: bidi, line breaking, styles, font fallback. Bevy switched to it in 2026; Graphite uses it. **Recommended.** |
| `cosmic-text` | 8.4M | 0.19.0 | MIT/Apache | ✅ | Complete editing buffer (cursor, selection, shaping via harfrust/swash). Great for an on-canvas editable text box. |
| `fontdue` | 8.8M | 0.9.4 | MIT/Apache/Zlib | ✅ | Fast rasteriser, no shaping. |
| `ab_glyph` | 42M | 0.2.32 | Apache-2.0 | ✅ | Simple rasterisation (PaintFE uses it). |

**Gaps:**
- No crate provides **Photoshop text-engine semantics**: paragraph and character styles, point versus area text, warp text, faux bold, tracking/kerning in 1/1000 em, and **EngineData** serialisation for PSD.
- In the browser, **system font enumeration** requires the Local Font Access API, which only Chromium has. Plan for user-uploaded fonts plus Google Fonts.

## B.6 Vector and paths

| Crate | DL | Licence | Role |
|---|---|---|---|
| `kurbo` | 44M | Apache/MIT | Béziers, arc length, offsets, hit tests. **The base** for pen tool, shape layers and vector masks. |
| `lyon` | 6.2M | MIT/Apache | Tessellation to triangles for GPU; path building. |
| `tiny-skia` | 47M | BSD-3 | CPU raster of paths and masks; stroke expansion. |
| `zeno` | 11.7M | Apache/MIT | Small path rasteriser (swash's). |
| `flo_curves` | 285k | Apache-2.0 | Bézier boolean operations and fitting. Graphite has its own `path-bool`, MIT/Apache, vendorable. |

## B.7 ML inference

| Crate | DL | Latest | Licence | Browser path | Assessment |
|---|---|---|---|---|---|
| **`ort`** (+ **`ort-web`**) | 18.3M (ort-web 5.6k) | 2.0.0-rc.13 (2026-07); ort-web 0.3.1+1.27 | MIT/Apache (ONNX Runtime is MIT) | `ort-web` binds **onnxruntime-web** (15.8M npm downloads a month; WASM CPU, WebGPU and WebNN EPs) through wasm-bindgen | **Recommended.** It is the same API native and web: CUDA, CoreML, DirectML and OpenVINO natively, WebGPU in the browser. darktable, rembg and RapidRAW all standardise on ONNX. Caveat (from the "ONNX-in-browser gotchas" and "onnxruntime-web on iOS 17" memories): ORT-web 1.29 fails on iOS 17 Safari; WebGPU buffer reuse bugs. |
| `ort-tract`, `ort-candle` | 125k / 8k | 0.4.1 | MIT/Apache | pure-Rust alternative backends for `ort` | A fallback when shipping the ORT wasm binary is unwanted; operator coverage is limited. |
| `candle` | 8.0M | 0.11.0 (2026-06) | MIT/Apache | wasm CPU + SIMD; **no official WebGPU backend** (community fork FerrisMind/candle adds wgpu/Vulkan) | Good for diffusion and transformers natively (CUDA/Metal). In the browser, CPU only. |
| `burn` | 1.4M | 0.21.0 (2026-08) | MIT/Apache | **wgpu backend → WebGPU in wasm**; `burn-import` converts ONNX to Rust code | The most "pure Rust + WebGPU" option. Import coverage for SAM/LaMa-class graphs is unverified. |
| `tract` (`tract-onnx`) | 2.9M | 0.23.7 (2026-09) | MIT/Apache | wasm CPU | Solid CPU ONNX, used by `ort-tract`. |
| `rten` | 1.5M | 0.26.0 (2026-08) | MIT/Apache | wasm CPU + SIMD | Robert Knight's ONNX engine, well optimised for wasm CPU (OCR). |
| `wonnx` | 19k | 0.5.1 (2023) | MIT/Apache | WebGPU | **Archived.** Do not use. |
| `segment-anything-rs` (kalosm) | 17k | 0.4.0 (2025-02) | MIT/Apache | candle | SAM via candle, stale. |

**Models worth planning around** (licences need a per-model check):
- SAM 2.1 (Apache-2.0), EfficientSAM, MobileSAM, SlimSAM (Apache/MIT, unverified).
- LaMa (Apache-2.0) and MI-GAN (MIT, unverified) for removal.
- BiRefNet (MIT) or IS-Net for background removal. **Avoid BRIA RMBG** (CC BY-NC).
- Real-ESRGAN (BSD-3) and RealPLKSR for upscaling; NIND denoise (darktable-ai lists its licence).
- For generative fill: SD 1.5/SDXL inpainting (CreativeML OpenRAIL-M, use restrictions); FLUX.1-schnell (Apache-2.0); FLUX.2 Klein (licence unverified). All are too heavy for browser Lite.

## B.8 Selection, retouch and algorithms

| Algorithm | Rust status | Best reference (licence) | Notes |
|---|---|---|---|
| **Flood fill / magic wand** | `imageproc` has flood fill (tolerance semantics unverified); trivial to write | Pinta/Paint.NET 3.36 (MIT) | Use scanline fill with a tolerance in Lab or luma plus anti-aliasing. Contiguous and global modes. |
| **Graph cut / GrabCut** | **No maintained crate.** `scirs2-vision` mentions segmentation (quality unverified); `ruvector-mincut` is unrelated | OpenCV `grabCut` (Apache-2.0); Boykov-Kolmogorov maxflow paper; GIMP foreground select uses SIOX / matting-levin (GPL) | For Quick Selection, ML models (SAM) now beat GrabCut. Keep GrabCut as a CPU fallback. |
| **Guided filter** (He et al.) | No crate; about 60 lines with box filters (`libblur`) | OpenCV ximgproc (Apache-2.0); paper | Needed for **Select and Mask / Refine Edge** (edge-aware matte refinement), dehaze and detail. Graphite has `dehaze` (MIT). |
| **Matting** (Refine Hair) | None | Closed-form matting (Levin, paper); ViTMatte / MODNet (Apache/MIT, unverified) via ONNX | Ship ML matting. |
| **Poisson blending / healing brush** | `russell_pde` (generic PDE solver); no image crate | OpenCV `seamlessClone` (Apache-2.0); Pérez et al. 2003 paper | Healing = Poisson solve with a source texture guide; on GPU use Jacobi or multigrid in WGSL. |
| **PatchMatch / content-aware fill** | **No crate.** `texture-synthesis` (Embark, MIT/Apache, **archived 2023**) does multi-resolution stochastic synthesis **with inpaint and guide maps**, a good permissive base. `inpaint` (EUPL-1.2, Telea-type). | **Resynthesizer** (GIMP, **GPL-3**, learn-only if permissive); Barnes et al. 2009 paper; G'MIC `inpaint` (CeCILL, GPL-compatible) | **Patent warning:** Adobe holds PatchMatch correspondence patents (e.g. [US8811749B1](https://patents.google.com/patent/US8811749), [US8861869B2](https://patents.google.com/patent/US8861869B2/en)) plus content-aware fill patents (US8818135B1 "Low memory content aware fill", US9697595B2). Filing dates around 2009–2012 suggest expiry around 2029–2032 (**unverified; get patent counsel**). Low-risk path: **ML inpainting (LaMa/MI-GAN)** for Remove, and plain texture synthesis (Efros-Leung / Harrison's resynthesizer ideas from 2001–2005) from papers. |
| **Seam carving** (content-aware scale) | `seamcarving` (**LGPL-3**, stale 2020); `content-aware-resizing` (tiny) | Avidan & Shamir 2007 paper (the patent was assigned to MERL/Adobe, status unverified) | Easy to implement (energy map plus DP). A protection mask is required. |
| **Liquify / mesh warp** | None standalone | PaintFE (MIT, WGSL liquify + Catmull-Rom mesh warp); RapidRAW liquify (AGPL) | A displacement field texture plus a GPU sampler. Tools: forward warp, bloat, pucker, reconstruct. |
| **Lens correction** | **`lensfun`** crate (pure-Rust port, 878 DL; **crates.io says LGPL-3.0-or-later but the GitHub repo is tagged GPL-3.0**, clarify). Database is **CC-BY-SA-3.0**. | lensfun C++ (LGPL-3 library; GPL-3 apps) | Using the database as data is fine; modified DB files must stay CC-BY-SA with attribution. |
| **Panorama stitching** | None (`stitchy` just concatenates) | Hugin/libpano13 (GPL-2), enblend/enfuse (GPL-2), OpenCV stitching (Apache-2.0) | Pipeline: features (ORB/AKAZE via `kornia` or ML such as LightGlue, Apache-2.0) → RANSAC homography → spherical/cylindrical warp → seam finding (graph cut) → **multi-band Laplacian blending** (Burt-Adelson, paper). |
| **HDR merge** | `image-hdr` (Apache-2.0; Poisson photon-noise radiance estimation) | Debevec-Malik; Mertens exposure fusion (paper; enfuse GPL) | Exposure fusion is a Laplacian pyramid blend, easy on GPU. |
| **Frequency separation** | Trivial (Gaussian plus subtract/add in linear or gamma space; the PS "Linear Light" 50% offset trick) | — | Ship as an action or macro. |
| **Bilateral** | `imageproc::filter::bilateral_filter` | — | Slow on CPU; use a bilateral grid on GPU. |
| **Local Laplacian** (Paris et al. 2011; Aubry et al. 2014 fast version) | None | darktable `local laplacian` (GPL-3); Halide examples (MIT) | This is the Clarity/Texture engine, so implement it from the papers. |
| **Focus stacking** | None | RapidRAW (AGPL) | — |

## B.9 Undo and document model

| Crate | DL | Licence | Use |
|---|---|---|---|
| `undo` | 182k | MIT/Apache | Classic command pattern with merge, checkpoints and branching history; a good fit for a history panel. |
| `undoredo` | 4.8k | MIT/Apache | Deltas, snapshots or commands. |
| `loro` | 665k | MIT | CRDT with **version DAG, time travel and checkout**; rich movable-tree support fits a layer tree. Future collaboration. |
| `yrs` (y-crdt) | 3.0M | MIT | Yjs port, interoperable with JS Yjs. |
| `automerge` | 587k | MIT | CRDT, JSON-like. |

**Recommendation:**
- A custom **command pattern** with **copy-on-write tile snapshots**, as in Krita and Photoshop.
  - Pixel operations record dirty tiles (old tile handles are refcounted, not copied).
  - Structural operations (layer tree, properties) are small commands.
  - Serialise the structural part so a future `loro` layer can sync it.
- CRDTs do not handle pixel payloads well. Keep pixels content-addressed (`blake3`) and outside the CRDT.

---

# Part C — Synthesis

## C.1 Gap statement

**Is "open-source Photoshop in the browser" already served?** No. Each existing option covers only part of it:

| Need | Photopea | GIMP 3.2 | Krita 5.3/6 | Graphite | darktable / RapidRAW | Darkly | krita-ai-diffusion / InvokeAI |
|---|---|---|---|---|---|---|---|
| Open source | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Runs in browser, no install | ✅ | ❌ | ❌ | ✅ | ❌ | ✅ | ❌ (local server) |
| Photoshop-like layer UX | ✅ | ~ | ~ | ❌ (node/vector-first) | ❌ | ~ (painter) | ~ |
| PSD round-trip incl. styles, text, adjustments | ✅ (best) | ❌ | ~ | ❌ (LTS roadmap) | ❌ | ❌ (unverified) | ❌ |
| Adjustment layers / non-destructive | ✅ | ~ (NDE filters) | ✅ | ✅ (graph) | ✅ (pipeline) | ~ | ❌ |
| 16/32-bit linear pipeline | ~ (unverified) | ✅ | ✅ | ? | ✅ | ? | ❌ |
| RAW develop (ACR-class) | ~ (basic) | ❌ (hand-off) | ❌ | ❌ (Beta 2) | ✅ | ❌ | ❌ |
| Selection refine / AI select | ✅ (AI credits) | ❌ | ❌ | ❌ | ✅ (SAM masks) | ? | ~ |
| Healing / content-aware / remove | ✅ | plugin (GPL) | basic | ❌ | RapidRAW ~ | ? | ✅ (diffusion) |
| Local AI, no account | ❌ (cloud credits) | plugins | plugin + ComfyUI | ❌ | ✅ | ❌ | ✅ (GPU required) |
| No ads, no login | ❌ (ads) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

**What no open-source project does well in 2026:** a **Photoshop-grade layered photo compositor** that combines four things:
1. **PSD round-trip at high fidelity**, including layer styles, adjustment layers, text as text, and 16-bit/PSB;
2. a **photographer's pipeline**, meaning RAW develop feeding into layers;
3. **local AI** tools for select subject, remove, background removal and upscale, running on WebGPU with no account;
4. all of it running **in a browser tab**, with an optional native shell for bigger files.

**Photopea proves demand** (17M+ monthly visits; self-host and open-source requests are its top issues) but it is closed and ad-funded. **GIMP** has the depth but no web version, poor PSD styles and no AI. **Graphite** has our architecture but is heading toward "Blender for 2D", with raster, RAW and PSD 6–24+ months out. **Darkly** is closest in stack and licence (AGPL, Rust+WebGPU) but is painter-first.

**Realistic wedge (ordered):**
1. **"Open Photopea": PSD-faithful, private, ad-free, in the browser.**
   - Open a PSD, edit layers, masks, text and adjustments, save a PSD that reopens cleanly in Photoshop.
   - This is the single most defensible technical moat. It is tedious, test-corpus-driven work that neither GIMP nor Graphite prioritises.
   - Lite = 8-bit RGB PSD, layers, masks, basic adjustments, export. Pro = 16-bit, PSB, layer styles, smart objects, text.
2. **Local AI retouch without credits:** select subject (SAM), remove object (LaMa/MI-GAN), background removal (BiRefNet), upscale (Real-ESRGAN), all on WebGPU through `ort-web`. This is the feature Photopea meters with credits and GIMP lacks.
3. **RAW-into-layers** (Pro): a `rawler`-based develop stage with scene-referred defaults (AgX/sigmoid-style), feeding a 16-bit or float document. This is the Camera Raw hand-off that GIMP and Krita outsource.
4. **Not a wedge:** "it's Rust", "it's local" (see the "Local and private is not a wedge" memory), painting (Krita and Darkly win), vector/procedural (Graphite wins) and generative diffusion (ComfyUI and InvokeAI win; offer a bridge instead).

**Honest risks:**
- Graphite could add PSD import and selection faster than expected. Its licence is permissive, so it could also absorb *our* permissive code if we go MIT.
- Photopea could open-source or self-host (low probability).
- PSD fidelity is a long-tail problem: Photoshop 2026 changed descriptor enums to long form, per the ag-psd-rs notes.

## C.2 Reuse map

Licence columns:
- **AGPL app:** can we link or copy it into an AGPL-3.0 app?
- **Permissive app:** can we do the same in an MIT/Apache app?

| Subsystem | Recommendation | Licence | AGPL app | Permissive app | Maturity | Risk |
|---|---|---|---|---|---|---|
| App shell (web) | Rust core → wasm (`wasm-bindgen`), TS/Svelte or Solid UI (thin, as in Graphite) | MIT/Apache | ✅ | ✅ | High | Low |
| App shell (native) | **Tauri 2.11** (Apache/MIT, 111k stars) with the same wasm UI plus native Rust core over IPC; or Graphite's CEF + winit approach | MIT/Apache | ✅ | ✅ | High | Medium (two builds) |
| GPU | `wgpu` 30 + WGSL (optionally Rust-GPU later) | MIT/Apache | ✅ | ✅ | High | Medium (Firefox Linux/Android WebGPU still pending; WebGL2 fallback) |
| Vector, shape and text rendering | `vello` (GPU) / `vello_hybrid` / `tiny-skia` fallback; `kurbo`; Graphite `path-bool` (vendored) | Apache/MIT, BSD-3 | ✅ | ✅ | Medium-high | Medium (Vello API churn) |
| Codecs | `image` + `zune-jpeg` + `png` + `tiff` + `image-webp` + `exr` + `gif` + `qoi` + `ravif` | MIT/Apache/BSD | ✅ | ✅ | High | Low |
| JPEG XL | `jxl` (jxl-rs) decode; `zune-jpegxl` lossless encode; libjxl via `cc` for lossy | BSD-3 | ✅ | ✅ | High (decode) | Low. **Avoid `jpegxl-rs` (GPL-3) and imazen `jxl-encoder` (AGPL/commercial) if permissive.** |
| HEIC | Native: `libheif-rs` (LGPL libheif). Web: browser decoder where available | LGPL-3 + HEVC patents | ✅ | ⚠️ dynamic linking only; patents apply either way | High | **High (patents)** |
| AVIF | `ravif`/`rav1e` encode; `rav1d` decode | BSD | ✅ | ✅ | High | Low |
| SVG / PDF import | `resvg`/`usvg`; `hayro` | Apache/MIT | ✅ | ✅ | High / medium | Low |
| **PSD read/write** | Fork **`ag-psd` (Rust)** as scaffold, cross-check against **PhotoshopAPI** (BSD-3) for PSB and 16/32-bit, oracle-test with **psd-tools** (MIT) and real Photoshop files | MIT / BSD-3 | ✅ | ✅ | **Low** (6-star AI port) | **High** (fidelity long tail; needs a corpus of real PS 2024–2026 files) |
| Metadata | `little_exif` (read/write) + `kamadak-exif` (read) + an opaque XMP packet preserver | MIT/Apache, BSD-2 | ✅ | ✅ | Medium | Medium. **Avoid `rexiv2` (GPL-3).** |
| Colour management | **`moxcms`** (pure Rust, wasm); `lcms2` for native proofing and CMYK validation | BSD-3/Apache; MIT | ✅ | ✅ | Medium-high | Medium (moxcms bus factor) |
| Colour maths | `palette`, `color` | MIT/Apache | ✅ | ✅ | High | Low |
| Resize | `fast_image_resize` | MIT/Apache | ✅ | ✅ | High | Low |
| Filters (CPU) | `imageproc`, `libblur`; own f32 kernels | MIT; Apache/BSD | ✅ | ✅ | Medium | Low |
| Adjustments (PS-exact) | Port Graphite `raster/adjustments.rs` + `no-std-types/blending.rs` | MIT/Apache | ✅ | ✅ | Medium | Low |
| Text | `parley` + `harfrust`/`skrifa` + `swash`; `cosmic-text` for editing | MIT/Apache | ✅ | ✅ | Medium-high | Medium (PS text-engine semantics are ours to build) |
| RAW decode | **`rawler`** (broadest); `rawloader`/`imagepipe` as alternatives | **LGPL-2.1 / LGPL-3** | ✅ | ⚠️ LGPL static linking into a single wasm binary triggers relink obligations, so ship object files or a separate module, or get legal advice | Medium-high | Medium |
| Demosaic | Implement RCD, AMaZE-style and Markesteijn from papers; or adapt RT/darktable code (AGPL only) | GPL-3 source | ✅ (adapt with attribution) | ❌ copy; clean-room only | — | Medium |
| Lens correction | `lensfun` crate + lensfun DB | LGPL-3 or GPL-3 (clarify) + **CC-BY-SA-3.0** DB | ✅ | ⚠️ | Medium | Medium |
| RAW pipeline and tone mapping | Learn from darktable (sigmoid, AgX, filmic) and RapidRAW | **GPL-3 / AGPL-3** | ✅ (AGPL can include GPL-3 code) | ❌ learn-only | — | Medium |
| AI runtime | **`ort` + `ort-web`** (WebGPU/WASM EPs); `burn` (wgpu) as pure-Rust fallback | MIT/Apache | ✅ | ✅ | High (ORT) | Medium (iOS Safari quirks, model sizes) |
| AI models | SAM 2.1, LaMa, BiRefNet, Real-ESRGAN | Apache/MIT/BSD (per model) | ✅ | ✅ | High | Medium: **avoid BRIA RMBG (non-commercial)** and check OpenRAIL use clauses |
| Healing / content-aware | ML inpaint (LaMa) for Remove; Poisson healing from papers; `texture-synthesis` (MIT/Apache) for Content-Aware Fill v1 | MIT/Apache | ✅ | ✅ | Medium | **Patents (Adobe PatchMatch)**. Resynthesizer and G'MIC are GPL/CeCILL: learn-only if permissive. |
| Selection | SAM (ML) + own flood fill + own guided filter; GrabCut after OpenCV (Apache) | — | ✅ | ✅ | — | Medium (build effort) |
| Liquify / mesh warp | Learn from PaintFE (MIT) | MIT | ✅ | ✅ | Medium | Low |
| Panorama / HDR | OpenCV (Apache) algorithms re-implemented; Hugin and enblend (GPL-2) as references | Apache; GPL-2 | ✅ (GPL-2-*only* code is **not** AGPL-3-compatible; check "or later") | ❌ learn-only | — | Medium |
| History | Custom command pattern + COW tiles; `undo` crate; `loro` later | MIT/Apache | ✅ | ✅ | High | Low |
| Generative fill | Optional bridge to a user-run **ComfyUI** (GPL-3) or InvokeAI (Apache) over HTTP | — | ✅ | ✅ (separate process) | High | Low-medium |
| Node graph (optional Pro) | Graphite `graph-craft` / `interpreted-executor` (vendored) | MIT/Apache | ✅ | ✅ | Medium (unversioned) | High coupling |

**Learn-only code if we go permissive:**
- GIMP (GPL-3), GEGL operations (LGPL-3; static-link issues in wasm).
- **Resynthesizer (GPL-3)**, G'MIC (CeCILL-2.1).
- **darktable, RawTherapee, ART (GPL-3)**; Krita (GPL-3); Hugin/enblend (GPL-2).
- ComfyUI, Fooocus, chaiNNer, krita-ai-diffusion (GPL-3).
- RapidRAW, Darkly, Upscayl (AGPL-3); Imazen `zen*`, `heic`, `jxl-encoder` (AGPL or commercial); `dng` crate (AGPL-3).
- `jpegxl-rs` and `rexiv2` (GPL-3 wrappers).

**Licence decision implication:**
- **AGPL-3.0 unlocks** the RawTherapee/darktable demosaic and tone code, RapidRAW's WGSL pipeline, Darkly, and LGPL linking without worry. That is a big head start on Pro RAW.
- **MIT/Apache** keeps Graphite-style ecosystem goodwill and lets others embed us, but forces clean-room work on RAW maths, demosaic, retouch and lens correction.
- Given the openapps pattern and that the wedge is fidelity (not a library), **AGPL-3.0 is the pragmatic choice for the app**. Publish the PSD crate and the tile engine as **MIT/Apache** separately to attract contributors.

## C.3 Algorithm notes for the hard parts

### C.3.1 PSD write fidelity
- **Structure:** header (`8BPS`, version 1 = PSD, 2 = PSB) → colour-mode data → image resources (`8BIM` blocks: resolution 1005, ICC 1039, XMP 1060, slices, guides 1032, layer comps 1065, and so on) → layer and mask info → merged composite image.
- **PSB** is needed when a dimension exceeds 30,000 px or the file exceeds 2 GB. Many length fields widen to 8 bytes and RLE row counts widen to 4 bytes.
- **Always write a correct flattened composite** (Maximize Compatibility). Many readers, including Lightroom, macOS Preview and older Photoshop paths, show only it. That means **our renderer must composite exactly like Photoshop** or the file looks different outside Photoshop.
- **Layer records:**
  - Channel IDs: -1 transparency, -2 user mask, -3 real user mask (vector plus pixel).
  - Blend-mode keys: `norm`, `mul `, `scrn`, `over`, `sLit`, `hLit`, `vLit`, `lLit`, `pLit`, `hMix`, `diff`, `smud`, `div `, `idiv`, `lbrn`, `lddg`, `dkCl`, `lgCl`, `hue `, `sat `, `colr`, `lum `, `diss`, `pass` for groups.
  - Clipping, flags, opacity and fill opacity (`iOpa`).
  - Groups use `lsct` section dividers; the group *end* marker comes first in file order.
- **Additional layer info keys to support**, in rough value order:
  1. `luni` (Unicode name), `lyid`, `lsct`, `iOpa`, `lclr`, `shmd`
  2. `lfx2` / `lmfx` (**object-based effects descriptor**; legacy `lrFX` is optional)
  3. Adjustment layers: `levl`, `curv`, `hue2`, `CgEd` (brightness/contrast), `blnc`, `blwh`, `mixr`, `selc`, `vibA`, `expA`, `phfl`, `grdm`, `post`, `thrs`, `nvrt`, `clrL` (colour lookup)
  4. Fill layers: `SoCo`, `GdFl`, `PtFl`; vector: `vmsk`/`vsms`, `vogk` (live shapes), `vscg`
  5. Text: `TySh` (a descriptor plus **EngineData**, a PostScript-like dictionary text blob)
  6. Smart objects: `SoLd`/`SoLE` + `PlLd` with the placed transform; embedded file data in `lnk2`/`lnk3`/`lnkD`; smart filters in the `filterFX` descriptor
  7. Artboards (`artb`/`artd`), `FMsk`, `cust`, `CAI ` (content credentials, unverified key name)
- **Text layers:** Photoshop re-renders text from EngineData, but other readers use our raster. Write both, and set the flag so Photoshop does not prompt "update text layers?". ag-psd documents that prompt as unavoidable without a matching render (unverified whether a flag avoids it).
- **Descriptors (`Objc`):** typed key-value trees with 4-character or long keys. Photoshop 2026 writes enum values in long form, so accept both and write the classic 4-character form for compatibility (unverified which form older PS versions require).
- **Compression:** raw, RLE (PackBits per row), ZIP, and ZIP with prediction. Use ZIP with prediction for 16-bit: delta per row, big-endian. 32-bit uses float delta prediction.
- **Testing strategy (the moat):**
  - Build a corpus of real PS 2020–2026 files, each paired with a PNG composite exported from Photoshop.
  - Round-trip read → write → read with a structural diff, plus a pixel diff of our composite against the Photoshop PNG, with ΔE thresholds.
  - Use psd-tools as a second parser.
  - Fuzz with `cargo-fuzz`; ag-psd-rs already enforces decode byte budgets.
  - The "Only real files find parser bugs" memory applies directly.

### C.3.2 Layer styles
- **Effects:** drop shadow, inner shadow, outer glow, inner glow, bevel and emboss (with contour and texture), satin, colour/gradient/pattern overlay, stroke. PS CC allows up to 10 instances of some effects (multiple strokes and shadows).
- **Rendering order, as Photoshop draws it** (bottom to top): drop shadow → outer glow → [layer content with fill opacity] → pattern overlay → gradient overlay → colour overlay → satin → inner glow → inner shadow → bevel → stroke (stroke position inside, centre or outside changes the order). Exact order is **unverified** and must be confirmed against the corpus. All effects then composite with layer opacity, **except** that "Fill" opacity affects only content and interior effects.
- **Core primitive:** the **alpha distance transform.**
  - Spread and choke = threshold of the dilated or eroded alpha.
  - Size = blur, where "Softer" is Gaussian-like and "Precise" uses a Euclidean distance field (EDT, Felzenszwalb-Huttenlocher, O(n)).
  - Contour = 1D LUT on the normalised distance. Noise = dither. Jitter = gradient-map jitter.
- **Bevel and emboss:** height map = contour(distance field); normals from the gradient; lighting with global light altitude and azimuth; highlight and shadow composited with their own blend modes (default Screen and Multiply). Chisel Hard uses an unblurred EDT.
- **GPU:** jump flooding algorithm (JFA) for the distance field in WGSL, O(log n) passes. Satin = offset-and-mask-difference blurred.
- **Fidelity trap:** effects render at document resolution. "Scale Effects" and layer bounds expansion are needed because an outer glow extends beyond the layer bounds.

### C.3.3 Blend modes: linear versus gamma
- **Photoshop blends in the document's encoded space** for 8/16-bit documents (sRGB-ish gamma), unless *Color Settings → Blend RGB Colors Using Gamma 1.0* is on. 32-bit documents blend in **linear**. Text anti-aliasing and layer opacity also have a "Blend Text Colors Using Gamma" option.
- **For PSD fidelity we must replicate gamma-space blending by default**, even though linear is physically correct.
  - Keep a per-document `blend_space: Encoded | Linear` flag, matching PS and read from the PSD's colour-settings-related resources where present (unverified which resource stores it).
  - Offer linear as the "photographic" mode in the RAW or 32-bit pipeline.
- **Formulas:** the W3C Compositing & Blending Level 1 separable and non-separable modes match Photoshop for the standard set.
  - Photoshop-specific modes: Linear Burn, Linear Dodge (Add), Vivid Light, Linear Light, Pin Light, Hard Mix, Subtract, Divide, Darker Color, Lighter Color, Dissolve.
  - Graphite has recently fixed its Overlay and whole-colour formulas (MIT), so it is a reference.
  - **Hue, Saturation, Color and Luminosity use Photoshop's luma coefficients** (0.3, 0.59, 0.11) and the `SetLum`/`ClipColor` procedure in the W3C spec, not Lab.
  - **Dissolve** uses a deterministic per-pixel noise seed. Match it only approximately.
- **Premultiplied alpha** for GPU compositing; unpremultiply before non-separable modes. Watch 8-bit quantisation drift; composite in 16-bit or f32 internally even for 8-bit documents, then round with Photoshop's integer rounding (see Graphite's "integer arithmetic" tests).
- **Group "Pass Through" versus "Normal"** (isolated) changes results. Clipping masks form a base-layer group, and Photoshop's "Blend Clipped Layers as Group" and "Transparency Shapes Layer" flags matter.

### C.3.4 16-bit and 32-bit float pipeline
- **Internal format:** store tiles as `u8`, `u16` or `f32` per document depth. **Always process in f32** on GPU (`rgba16float` textures are universally supported; `rgba32float` is filterable only with the `float32-filterable` feature, so use `rgba16float` for display pyramids and `rgba32float` storage for compute).
- **Photoshop 16-bit is 15-bit plus 1**: values 0–32768, not 0–65535. PSD stores 0–65535 on disk (unverified whether PS scales on read/write; check in the corpus). Keep this in mind for exact histograms.
- **32-bit documents** are linear, scene-referred, and unbounded (>1.0). Adjustments must be HDR-safe (curves on log-encoded input). Displaying them needs an exposure/tonemap view transform, which in Photoshop is not destructive.
- **Colour:** a working-space ICC per document (sRGB, Display P3, Adobe RGB, ProPhoto) via moxcms. A display transform to the canvas (browser canvases are sRGB or `display-p3`; WebGPU canvases support `display-p3` and extended range, while `rgba16float` HDR canvas availability varies by browser, unverified). CMYK documents: store 4 channels, soft-proof via moxcms or lcms2 A2B/B2A LUTs, and composite in CMYK the way Photoshop does in CMYK mode (Pro, late).

### C.3.5 Tiled / virtual memory in wasm's 4 GB heap
- **Budget maths:** a 50 MP 16-bit RGBA layer is 400 MB and a float layer is 800 MB. 20 layers plus history easily exceeds 8 GB, so **full in-memory is impossible in any browser**, and even Memory64 caps at 16 GB.
- **Memory64 status (2026-09):**
  - Chrome/Edge **133+** ✅; Firefox **134+** ✅; **Safari ❌** (not in 26.5, 26.6 or 27 TP, per caniuse).
  - It is 10% to over 100% slower because guard-page bounds-check elision is lost (SpiderMonkey blog, 2025-01).
  - **Verdict: do not depend on Memory64.** Build for wasm32 (4 GB max; practical limits on iOS are lower, unverified figures) and optionally ship a wasm64 build for Chromium Pro users later.
- **Architecture, modelled on Krita, GEGL and libvips:**
  1. **Tiles** of 256×256 (512 for 8-bit) per layer per mip level. Sparse storage (transparent or uniform tiles are implicit). Copy-on-write with a refcount.
  2. **Three storage tiers:**
     - (a) hot tiles decoded in wasm linear memory, under an LRU budget of about 1–1.5 GB;
     - (b) warm tiles **LZ4-compressed** (`lz4_flex`, MIT) in wasm memory or in JS `ArrayBuffer`s outside the wasm heap (they do not count against its 4 GB; total tab memory still bounded);
     - (c) cold tiles in the **Origin Private File System** through `FileSystemSyncAccessHandle` in a dedicated worker (synchronous, fast; available in all engines, unverified for Safari worker sync handles on iOS).
  3. **Undo** = retain old tile handles (compressed, spill to OPFS).
  4. **Render** only the visible viewport at the matching mip level. The GPU holds a tile atlas (`rgba16float`, for example 4096² pages) plus per-layer tile caches. Composite on GPU per tile, bounded by `maxTextureDimension2D` 8192 and `maxBufferSize` 256 MiB. Never allocate a full-document texture.
  5. **Filters** run tile-by-tile with apron or padding (blur radius). Global operations (histogram, PatchMatch, AI) run on downsampled proxies first, then refine per tile. AI models run on tiled crops with overlap-blend, as darktable declares tile sizes in its model manifest.
  6. **Threads:** `wasm-bindgen-rayon` needs SharedArrayBuffer, which needs COOP/COEP headers. That conflicts with some third-party embeds, so plan for a single-threaded fallback. OPFS I/O and ORT-web run in separate workers.
- **Native (Tauri) build:** same tile engine, OS mmap swap file, no 4 GB limit. This is the "Pro for huge files" story.

### C.3.6 GPU compositing
- **Render graph:** a layer tree compiles to a DAG: groups become isolated render targets (pass-through groups inline); adjustment layers become full-screen fragment or compute passes over the running composite; masks multiply alpha; clipping groups become base-alpha-masked targets.
- **Per-tile dirty-rect invalidation:** cache group outputs per tile keyed by content hash, as Graphite does with graph dedup hashing.
- **WebGPU constraints:**
  - Blend modes beyond fixed-function need a shader that reads the destination. WebGPU cannot read the framebuffer, so **ping-pong textures**: composite each layer in a compute or fragment pass that samples both the backdrop and the layer.
  - Group mip pyramids for zoomed-out views. Use `rgba16float` throughout, then a final display transform (ICC via 3D LUT texture, 33³ from moxcms) to an sRGB or P3 canvas.
- **Precision:** keep the CPU reference compositor (f32, `rayon`) identical to the GPU WGSL and test both against the PSD composite corpus. Export and PSD composite use CPU or GPU readback deterministically. **Float determinism across GPUs is not guaranteed**, so exact-match tests run on the CPU path.
- **Vector and text layers:** rasterise via Vello (GPU) or `vello_cpu`/`tiny-skia` into layer tiles at the current scale; re-rasterise on zoom (vector crispness).
- **Fallback:** if WebGPU is unavailable (older Firefox on Linux/Android), use a CPU compositor with only the viewport composited, plus WebGL2 via wgpu for display.

---

## Sources

### Editors and projects
- GIMP 3.2 released: https://www.gimp.org/news/2026/03/14/gimp-3-2-released/
- GIMP 3.2 release notes: https://www.gimp.org/release-notes/gimp-3.2.html
- GIMP 3.2.2: https://www.gimp.org/news/2026/03/28/gimp-3-2-2-released/ · 3.2.4: https://www.gimp.org/news/2026/04/19/gimp-3-2-4-released/ · 3.2.6: https://www.gimp.org/news/2026/09/10/gimp-3-2-6-released/
- GIMP work items: https://gitlab.gnome.org/GNOME/gimp/-/work_items/12606 · /15505 · /16339 · /10505 · /2912 · /5492
- PhotoGIMP: https://github.com/Diolinux/PhotoGIMP
- Krita 6.0 (Phoronix): https://www.phoronix.com/news/Krita-6.0-Released · Krita 5.3/6.0 release notes: https://krita.org/en/release-notes/krita-5-3-release-notes/ · Krita 2026 roadmap: https://krita.org/en/posts/2026/roadmap-2026/ · Krita download: https://krita.org/en/download/ · Linuxiac on 6.0.2: https://linuxiac.com/krita-6-0-2-released-as-qt-6-build-5-3-2-remains-production-choice/
- krita-ai-diffusion: https://github.com/Acly/krita-ai-diffusion
- darktable 5.6.0: https://www.darktable.org/2026/06/darktable-5.6.0-released/ · 5.6.1: https://www.darktable.org/2026/08/darktable-5.6.1-released/ · AI tools: https://www.darktable.org/2026/06/meet-darktable-5.6-ai-tools/ · darktable-ai: https://github.com/darktable-org/darktable-ai
- darktable issues: https://github.com/darktable-org/darktable/issues/8497 · /15920 · /2170 · /7463 · /6068
- RawTherapee 5.13: https://rawtherapee.com/2026/07/rawtherapee-5.13-released/ · issues /6273 · /1895
- ART: https://github.com/artraweditor/ART
- RapidRAW: https://github.com/CyberTimon/RapidRAW · issues /112 · /25 · /533
- Graphite repo: https://github.com/GraphiteEditor/Graphite · roadmap: https://graphite.art/features/ · LWN review: https://lwn.net/Articles/1051242/ · desktop issue: https://github.com/GraphiteEditor/Graphite/issues/2535 · /584 · /1545 · /1694 · /3686 · rawkit README: https://github.com/GraphiteEditor/Graphite/tree/master/libraries/rawkit · XDA review: https://www.xda-developers.com/free-open-source-web-based-graphics-editor-replaced-gimp/
- Pinta: https://github.com/PintaProject/Pinta · issues /585 · /1027 · /1365
- PixiEditor: https://github.com/PixiEditor/PixiEditor
- Photoflare: https://github.com/PhotoFlare/photoflare · LazPaint: https://github.com/bgrabitmap/lazpaint
- Paint.NET 5.2 alpha: https://blog.paint.net/2026/08/10/paint-net-5-2-alpha-build-9719/ · Wikipedia: https://en.wikipedia.org/wiki/Paint.NET
- digiKam: https://invent.kde.org/graphics/digikam · Shotwell: https://gitlab.gnome.org/GNOME/shotwell
- miniPaint: https://github.com/viliusle/miniPaint (issue /312)
- Filerobot: https://github.com/scaleflex/filerobot-image-editor
- TOAST UI Image Editor: https://github.com/nhn/tui.image-editor (issue /927)
- Photopea: https://en.wikipedia.org/wiki/Photopea · https://github.com/photopea/photopea · issues /4870 · /5328 · /760 · revenue: https://www.builtplain.com/photopea-free-app-ad-revenue/
- Darkly: https://github.com/darkly-art/darkly · PaintFE: https://github.com/kylejckson/PaintFE · Schist: https://github.com/Infrawrench/schist · bitmappery: https://github.com/igorski/bitmappery
- IOPaint (archived): https://github.com/Sanster/IOPaint
- InvokeAI: https://github.com/invoke-ai/InvokeAI · releases: https://invoke.ai/releases/ · company wind-down: https://softuts.com/invokeai-commercial-platform-shuts-down-open-source-project-continues/
- ComfyUI: https://github.com/Comfy-Org/ComfyUI · ComfyUI_LayerStyle: https://github.com/chflame163/ComfyUI_LayerStyle
- Fooocus: https://github.com/lllyasviel/Fooocus · Upscayl: https://github.com/upscayl/upscayl · rembg: https://github.com/danielgatis/rembg · chaiNNer: https://github.com/chaiNNer-org/chaiNNer
- OpenVINO AI plugins for GIMP: https://github.com/intel/openvino-ai-plugins-gimp · GIMP-ML: https://github.com/kritiksoman/GIMP-ML · Resynthesizer: https://github.com/bootchk/resynthesizer

### PSD
- ag-psd (JS): https://github.com/Agamnentzar/ag-psd · ag-psd (Rust): https://github.com/Vasyanator/ag-psd-rs
- psd-tools: https://github.com/psd-tools/psd-tools · PhotoshopAPI: https://github.com/EmilDohne/PhotoshopAPI
- psd (Rust): https://github.com/chinedufn/psd · psd.rb: https://github.com/layervault/psd.rb · @webtoon/psd: https://github.com/webtoon/psd · psd_sdk: https://github.com/MolecularMatters/psd_sdk
- Adobe PSD file format spec: https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/

### Rust crates (crates.io API, 2026-09-14)
- https://crates.io/crates/image · /imageproc · /zune-image · /zune-jpeg · /fast_image_resize · /photon-rs · /pix · /pixels · /kornia · /ndarray · /rayon · /palette · /color · /moxcms · /lcms2 · /qcms · /tintbox · /libblur · /libvips · /opencv
- /wgpu · /naga · /vello · /vello_cpu · /vello_hybrid · /spirv-std · /tiny-skia
- /psd · /zune-psd · /rawpsd · /ag-psd · /tiff · /png · /jpeg-decoder · /jpeg-encoder · /mozjpeg · /image-webp · /webp · /ravif · /rav1e · /dav1d · /rav1d · /libheif-rs · /heic · /heif-oxide · /jxl · /jxl-oxide · /jpegxl-rs · /zune-jpegxl · /jxl-encoder · /exr · /gif · /qoi · /resvg · /usvg · /pdfium-render · /lopdf · /hayro · /kamadak-exif · /little_exif · /rexiv2 · /nom-exif · /dng · /gamut-dng
- /rawler · /rawloader · /imagepipe · /rawkit · /quickraw · /libraw-rs · /rsraw · /demosaic · /zenraw
- /cosmic-text · /swash · /rustybuzz · /harfrust · /fontdue · /parley · /skrifa · /ab_glyph · /kurbo · /lyon · /zeno · /flo_curves
- /ort · /ort-web · /ort-tract · /ort-candle · /candle-core · /burn · /tract-onnx · /wonnx · /rten · /segment-anything-rs
- /texture-synthesis · /inpaint · /seamcarving · /image-hdr · /lensfun · /undo · /undoredo · /loro · /yrs · /automerge · /tauri · /wasm-bindgen-rayon · /zenblend · /darkly
- dnglab supported cameras: https://github.com/dnglab/dnglab/blob/main/SUPPORTED_CAMERAS.md
- lensfun licence: https://github.com/lensfun/lensfun · https://lensfun.github.io/manual/v0.3.2/license.html · lensfun-rs: https://github.com/vdavid/lensfun-rs
- texture-synthesis (archived): https://github.com/EmbarkStudios/texture-synthesis
- jxl-rs: https://github.com/libjxl/jxl-rs · JPEG XL in Firefox via Rust: https://hwbusters.com/news/jpeg-xl-in-firefox-157-mozilla-made-google-rewrite-the-decoder-in-rust-first/ · Chrome JXL: https://www.januschka.com/chromium-jxl-resurrection.html
- Linebender Q1 2026: https://linebender.org/blog/tmil-25/ · Vello: https://github.com/linebender/vello
- Burn wgpu backend: https://burn.dev/blog/cross-platform-gpu-backend/ · Candle WebGPU issue: https://github.com/huggingface/candle/issues/344
- ort: https://github.com/pykeio/ort · onnxruntime-web on npm: https://www.npmjs.com/package/onnxruntime-web

### Platform
- Memory64 on caniuse: https://caniuse.com/wf-wasm-memory64 · SpiderMonkey "Is Memory64 actually worth using?": https://spidermonkey.dev/blog/2025/01/15/is-memory64-actually-worth-using.html · Chrome status: https://chromestatus.com/feature/5070065734516736
- WebGPU implementation status: https://github.com/gpuweb/gpuweb/wiki/Implementation-Status · web.dev: https://web.dev/blog/webgpu-supported-major-browsers

### Patents and algorithms
- PatchMatch paper: https://gfx.cs.princeton.edu/pubs/Barnes_2009_PAR/
- Adobe patents: https://patents.google.com/patent/US8811749 · https://patents.google.com/patent/US8861869B2/en · https://patents.google.com/patent/US8818135 · https://patents.google.com/patent/US9697595B2
