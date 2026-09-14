# 04 — Local AI: every incumbent AI feature, mapped to open models that run on the user's machine

Research for OpenPhotoshop: an open-source photo editor that runs entirely on the user's machine. The core is Rust. The browser build uses WebAssembly and WebGPU through onnxruntime-web. An optional native backend (`ort`, candle, llama.cpp) uses CUDA, Metal or CoreML for heavy models. Nothing is paid, nothing calls a cloud, and there are no API keys. There are two profiles: Lite and Pro.

Snapshot date: **2026-09-14**.

**How this was collected.**
- Licences come from Hugging Face `cardData.license` fields (via `huggingface.co/api/models/<repo>`), GitHub `LICENSE` files (via `gh api`), vendor model cards and dataset terms. Parameter counts and file sizes come from HF API file listings.
- The web-search budget ran out partway through, so the last third was checked by fetching pages and repos directly.
- Evidence tags used in the tables:
  - **[M]** measured by us in a sibling app (openpixels, openphotoid).
  - **[P]** published by a vendor, paper, repo or a secondary source.
  - **(approx)** is our own estimate, usually scaled from compute cost.
  - **(unverified)** is a claim we could not confirm first-hand.
  - **"unclear — verify"** means the licence could not be settled and the model must not ship until someone reads it.
- "Browser WebGPU feasible" assumes a mid laptop (M3 MacBook Air class) in Chrome, Edge or Safari 26, with an fp32 graph unless stated.

---

## 0. Executive summary

1. **Roughly two-thirds of the incumbents' AI features can be done locally with permissive models today.** Most of those work in the browser. The rest need a native GPU backend, or have no clean open model. The best-covered groups are:
   - **Selection:** SAM 2.1 / EdgeTAM, MediaPipe.
   - **Removal:** MI-GAN and LaMa.
   - **Upscale and denoise:** Real-ESRGAN compact, SPAN, RealPLKSR, NAFNet.
   - **Depth:** Depth Anything V2-Small, DA3-Small/Base/Mono-Large.
   - **Face and pose geometry:** MediaPipe.
   - **Organisation and OCR:** CLIP/SigLIP 2, SFace, PP-OCRv5, Florence-2.
   - **Provenance:** the `c2pa` crate.

   These are the honest gaps:
   - **Generative fill, expand and replace.** Tier 3 native only. FLUX.2 [klein] 4B is Apache-2.0 and fits in 12 GB. Quality is below Firefly, Nano Banana 2 and GPT Image 2.
   - **Wire removal and reflection removal.** No released permissive model exists.
   - **Sky segmentation.** No model with clean provenance.
   - **Learned auto-tone.** Every public checkpoint is trained on FiveK or PPR10K, and both datasets are research-only.
   - **Evoto-grade skin retouch.**
2. **The licence trap is bigger than the licence label.** Four patterns catch people:
   - **Non-commercial weights that everyone uses:** RMBG-2.0, CodeFormer, InsightFace, SegFormer, Depth Anything V2-Base/Large, Depth Pro, 4x-UltraSharp, FLUX.1 [dev]/Kontext/Fill, SD-Turbo/SD3.5 above $1M revenue, and MobileCLIP.
   - **Permissive weights trained on non-commercial datasets.** DIS5K sits under BiRefNet general/lite, BEN2 and IS-Net. CelebAMask-HQ sits under face parsing. FiveK and PPR10K sit under auto-tone. Adobe DIM sits under ViTMatte.
   - **Permissive models that load a restricted component.** OmniGen2 uses Qwen2.5-VL-3B (research licence). HiDream uses a Llama 3.1 encoder. Sana, Lumina and PRX use Gemma-family encoders.
   - **Our own runtime.** `ort-web` sends a telemetry beacon and loads from a CDN by default (§8).
3. **Commands should run through an LLM planner, not a pixel generator.** A small local LLM or VLM emits a typed, grammar-constrained plan of calls to deterministic, undoable engine tools. Generative models are one tool among many, and their output is always composited through a mask (§7). Browser: Qwen3-1.7B or Qwen3.5-0.8B. Native: Qwen3.5-4B or Gemma 4 E4B, both Apache-2.0.
4. **Download sizes by tier (§4):**
   - Tier 1 (default, in-browser): **~270 MB** in total, lazily fetched per feature.
   - Tier 2 (optional browser packs): **~2.5 GB** of vision packs, plus **~1.1 GB** for the command-bar LLM.
   - Tier 3 (native backend): **~16.5 GB** for the core generative pack, **~26 GB** with the SDXL compatibility pack, and **~44 GB** with the Qwen-Image-Edit workstation pack.
5. **About a dozen models ship already** in openpixels and openphotoid, along with their hardest-won lessons (§9):
   - Real-ESRGAN ×5, GFPGAN 1.4, DeOldify, U²-Net-p, YuNet and MODNet are already pinned and checksummed.
   - Fixed-shape tiling beats WebGPU buffer reuse.
   - fp16 graphs can be silently wrong on WebGPU.
   - The GPU path needs a pixel-parity canary, not a "session created" check.

---

## 1. Ground rules

- **Weights, not code, decide.** A model ships only if its *weights* permit commercial use and redistribution by anyone. Non-commercial, research-only, revenue-capped and territory-restricted licences are all excluded. We ship for free, but forks and users may be commercial.
- **Use-based licences** (OpenRAIL-M, OpenRAIL++, the Gemma Terms, the DINOv3 Licence) are allowed only with the licence shipped next to the weights and the use restrictions passed to users. They are flagged in every table.
- **CC-BY weights** (for example the OpenModelDB Nomos family) are allowed and need an attributions screen. **GPL-3.0 weights** (RawNIND) are a copyleft decision for the project licence. They are flagged and kept as an optional pack.
- **Dataset taint** means permissive weights trained on data whose terms forbid commercial use. We flag it and do not make the model a default until a legal call is made. Industry practice (rembg, imgly, HivisionIDPhotos, darktable) treats the weight licence as governing. We record the risk rather than pretend it is not there.
- **Every weight gets a manifest row:** URL, sha256, bytes, licence, attribution text and known-bad backends. Weights are served from our own origin, never a third-party CDN, as openpixels already does.

---

## 2. The incumbents: what each AI feature is and where it runs

| Product | AI features (2026) | Where it runs |
|---|---|---|
| **Photoshop 2026** | Generative Fill, Expand and Generate Background (Firefly Image 4, with partner models Gemini 3 / Nano Banana, FLUX.2 Pro and Flux Kontext Pro); Generative Remove; Generative Upscale (Topaz partner, up to 56 MP); Harmonize; Reference-image compositing; AI Assistant (agentic, web and mobile); Select Subject, Sky and Object Selection; Remove tool with distraction finder; Sky Replacement; Neural Filters (skin smoothing, colourise, depth blur, photo restoration, colour transfer); Face-Aware Liquify | Generative features are cloud and cost credits [P]. Select Subject offers a cloud or device choice. Remove tool, Liquify and Sky Replacement are device (unverified per feature) |
| **Lightroom / ACR / Lightroom Classic 15** | AI masks (subject, sky, background, objects, people and their facial parts); Denoise; Raw Details; Super Resolution; Lens Blur; Adaptive presets; Generative Remove; Distraction Removal (people, reflections, dust; wires and cables in ACR); Assisted Culling (eye focus, eyes open) | Denoise uses the ANE, on device [P]. Generative Remove is cloud. The others are mostly device (unverified per feature) |
| **Luminar Neo** | Sky AI, Relight AI, Light Depth, Enhance AI, Structure AI, Skin AI, Face AI, Body AI, Portrait Bokeh AI, Noiseless AI, Supersharp AI, Upscale AI, GenErase, GenExpand, GenSwap | Gen tools are cloud (unverified); the rest are device |
| **Pixelmator Pro** (Apple Creator Studio, Jan 2026) | ML Super Resolution, Deband, ML Denoise, ML Enhance, Auto Crop, Match Colors, Quick Selection, Remove Background, Repair | Apple silicon, on device [P] |
| **Google Photos / Pixel** | Magic Eraser, Magic Editor / Reimagine, Photo Unblur, Best Take, Auto Frame, "Help me edit" conversational editing (Gemini) | Magic Eraser and Unblur on device. Magic Editor and Help me edit are cloud [P] |
| **Apple Photos** | Clean Up, subject lift, Visual Look Up | On device [P] |
| **Samsung Galaxy AI** (S26) | Object eraser, Generative edit (move, erase, expand), Sketch to image, reflection and shadow eraser | S26 claims on-device generative edit ("EdgeFusion" with Nota AI). Creative Studio is cloud. Both from secondary sources (unverified) |
| **Evoto** | Skin and blemish retouch, flyaway hair, glasses glare, face and body reshape, makeup, backdrop clean-up, colour matching, culling | Most processing uploads to their cloud; one credit per export [P] |
| **Topaz Photo / Gigapixel** | Denoise, Sharpen / Super Focus, Recover Faces v3, Wonder 3.5 and Redefine (generative), Upscale, Remove, Dust & Scratch, lighting and colour | Local or cloud rendering [P] |
| **Photoroom** | Background removal, AI Backgrounds, AI Shadows, AI Relight, Instant Diffusion, batch | Cloud and proprietary models [P]. They did open-source PRX, a 1.3B Apache-2.0 text-to-image model, plus PRX Pixel (~7B) |
| **ChatGPT (GPT Image 2, Apr 2026), Gemini (Nano Banana 2 = Gemini 3.1 Flash Image, Feb 2026; Nano Banana Pro), BFL (FLUX.2 Pro, Flux Kontext Pro/Max)** | End-to-end instruction editing, multi-reference composition, text rendering | Cloud APIs [P] |

---

## 3. Feature catalogue (62 features)

Column legend:
- **Browser** is WebGPU feasibility (yes / borderline / no) with a typical M3-Air-class latency.
- **Native-only** means the feature needs the native backend.
- **Profile**: L = Lite, P = Pro, L+P = both.
- Unless noted, sizes are fp32 / fp16 / int8 / q4 ONNX files.

### 3A. Selection and matting

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Click-to-select object | PS Object Selection (device); LrC Select Objects (device); Pixelmator Quick Selection (device); Apple subject lift (device) | **EdgeTAM**; **SAM 2.1 tiny / small / base+**; SlimSAM-77; MobileSAM | EdgeTAM ~14M (approx): enc 19.5 / 9.7 / 4.9 MB, dec 21 / 10.5 MB [P]. SAM 2.1-t 38.9M: enc 134 / 67 / 52.6 / 43.8 MB. SAM 2.1-s 46M: enc 162.5 / 81.2 / 59.7 / 48.3 MB. base+ 80.8M: enc 306 / 153 MB. SlimSAM-77 enc 23.3 / 12.2 MB. MobileSAM 9.66M | All Apache-2.0 (code and weights) | **Yes.** Encoder once per image: EdgeTAM 60–150 ms, SAM 2.1-s 0.4–1 s. Decoder per click 10–30 ms (approx) | No | SAM 2.1 matches or beats PS Object Selection on well-bounded objects. Masks come out at 256², so edges are soft until refined (#8); weaker on hair and thin structures | L+P | EdgeTAM is 22× faster than SAM 2 (Meta). Pad to a fixed 1024², as openpixels learned. **EdgeSAM is S-Lab non-commercial.** SAM 2.1 large (898 MB) is too big for the browser |
| 2 | Select subject (salient object) | PS / LrC Select Subject (device or cloud); Photoroom (cloud) | **MODNet** (portraits); **BiRefNet-portrait**; BiRefNet_lite / BiRefNet; U²-Net-p | MODNet 24.7–25.9 MB (fp16 13 MB). BiRefNet_lite 224 / 115 MB. BiRefNet and -portrait 973 / 490 MB | MODNet Apache-2.0. BiRefNet MIT, but **general and lite are trained on DIS5K (non-commercial terms)**. BiRefNet-portrait is trained on P3M-10k (MIT) plus "humans" (unverified) | MODNet **yes**, 50–150 ms (approx). BiRefNet_lite **borderline**, 1–3 s at 1024² (approx; openphotoid OOM'd on a VM and never benchmarked it). BiRefNet full **borderline/no** | BiRefNet full is better native | BiRefNet is Photoroom-class on objects [P, community]. MODNet is portrait-only with coarse hair; openphotoid's WebGPU MODNet matte was **silently wrong** [M] | L+P | **RMBG-1.4 and RMBG-2.0 (Bria) are non-commercial.** BEN2 (MIT) is DIS5K-trained too |
| 3 | Remove background (cutout) | Photoroom, PS Quick Action, Pixelmator, Apple (device or cloud) | #2 plus Rust refinement: guided-filter upsample, closed-form matting in the uncertainty band, foreground-colour estimation | as #2 | as #2 | Yes (MODNet) / borderline (BiRefNet) | No | With refinement, near Photoroom on people. Below on glass and translucent objects | L+P | Reuse openphotoid `frame-matting`: guided filter at a constant radius of 2 [M]. withoutbg open weights (454.5 MB) use a **DINOv3 Licence** backbone |
| 4 | Select sky | LrC / PS Select Sky (device); Luminar Sky AI (device) | **Grounding DINO tiny ("sky.") + SAM 2.1**; skyseg.onnx (U²-Net); DA-V2-Small far-depth plus colour heuristics (untested idea) | GDINO-t 719 / 360 / 204 / 151 MB (q4f16). skyseg 176 MB (u2netp demo 2 MB) | GDINO Apache-2.0. skyseg is MIT on HF, but its **training-data provenance is unclear — verify** | **Borderline**, 1–3 s (approx) | No | **Below LrC** on trees, fences and hazy horizons. Needs guided-filter or matting refinement | L+P | **SegFormer ADE20K is NVIDIA non-commercial**, and so is every fine-tune. SkyAR is CC BY-NC-SA. ADE20K image terms are unverified. Gap: a permissive sky model trained on clean data |
| 5 | Select people (instances) | LrC Select People (device); PS | **RF-DETR-Seg Nano–Large**; SAM 2.1 from person boxes; MediaPipe Selfie Multiclass (single person) | RF-DETR nano 30.5M: 108 / 54 / 29 / 25 MB [P] | RF-DETR N–L (detection and all seg variants) Apache-2.0; **XL and 2XL detection are PML 1.0** | Yes, 100–300 ms (approx) | No | Solid instances. LrC also names people through face grouping (#53) | L+P | **Ultralytics YOLO-seg is AGPL-3.0. YOLOE is AGPL. YOLO-World is GPL-3.0.** RF-DETR is COCO closed-vocabulary |
| 6 | Face and body parts (skin, hair, clothes, brows, eyes, lips, teeth) | LrC People masks (device); Evoto (cloud) | **MediaPipe Selfie Multiclass** (background, hair, body skin, face skin, clothes, others); **MediaPipe Face Landmarker** 478-point polygons for eyes, iris, lips, teeth and brows | Multiclass 256² 16.4 MB (mIoU 77.2; 512² variant 81.1). Face Landmarker ~3.7 MB (approx). Hair segmenter 0.8 MB | Apache-2.0 per the Google model-card PDFs. A "CC BY 4.0" reading traces to the docs-site footer; **treat the card as governing, verify** | Yes, <50 ms (approx) | No | Near LrC for skin, hair and clothes at the model's resolution. Iris, sclera and teeth from landmark polygons are less exact than LrC | L+P | **BiSeNet face-parsing and jonathandinu/face-parsing are non-commercial** (CelebAMask-HQ; SegFormer). Sapiens v1 is CC BY-NC. **Sapiens2** bans "biometric processing" |
| 7 | Text-prompt selection ("select the red car") | PS AI Assistant (cloud); Google Help me edit (cloud) | **Grounding DINO tiny → SAM 2.1**; **Florence-2 base/large** (phrase grounding, referring-expression polygon); OWLv2; SAM 3 / 3.1 concept prompts | GDINO-t 0.2B (above). Florence-2-base 0.23B, q4f16 224 MB total, fp16 544 MB. OWLv2-base 614 / 307 / 163 MB. **SAM 3: 0.9B, 3.44 GB; tracker ONNX enc 1.87 GB / 935 MB / 296 MB q4f16** | GDINO and OWLv2 Apache-2.0; Florence-2 MIT. **SAM 3 and 3.1 use the custom "SAM License", manually gated; not read in this pass, unclear — verify.** EfficientSAM3 is labelled Apache but distilled from SAM 3 (unclear — verify) | GDINO+SAM **borderline**, 1–3 s per query (approx). **SAM 3 no**: int4 took ~28 s on an RTX 3050 laptop via Python ORT [P] | SAM 3 native only, after licence review | Good on plain nouns. **Weak on relations** ("the person on the left"), so resolve relations in the planner from boxes (§7). Grounding DINO 1.5/1.6 are API-only | P (L via command bar) | onnx-community's SAM 3 export is **tracker-only (no text)**; community text exports exist. Prompts for GDINO must be lowercase and end with "." |
| 8 | Refine hair edge / matting | PS Select and Mask "Refine Hair" (device) | **Rust closed-form matting + foreground estimation** (pymatting port, MIT); BiRefNet-matting / HR-matting; ViTMatte-small (trimap) | ViTMatte-s 25.8M, 104 MB. BiRefNet-matting 885 MB | pymatting MIT. BiRefNet-matting MIT (datasets unverified). **ViTMatte weights are trained on Composition-1k (Adobe DIM, research-only)** | Classical yes. ViTMatte yes, 0.3–1 s (approx) | BiRefNet-HR-matting native | Below PS Refine Hair on backlit wisps. Classical band matting is good on solid backgrounds | P | **FBA Matting (Adobe DIM, non-commercial) and MatAnyone / MatAnyone 2 (S-Lab) are excluded** |

### 3B. Removal, fill and compositing

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 9 | Remove object (brush) | PS Remove (device) / Generative Remove (cloud); LrC Remove (device); Apple Clean Up, Google Magic Eraser, Samsung Object Eraser (device); Luminar GenErase (cloud, unverified) | **MI-GAN**; **big-lama**; ZITS | MI-GAN ~6M (approx), `migan_pipeline_v2.onnx` 28.1 MB. LaMa ~51M (approx), 208 MB fp32, fixed 512² (Carve); OpenCV `inpainting_lama_2025jan.onnx` 92.6 MB | MI-GAN: MIT code **and** a separate MIT weights licence. LaMa Apache-2.0. ZITS Apache code, weights not stated (verify) | **Yes.** MI-GAN 30–80 ms at 512² (approx). LaMa ≤200 ms at 512² on M1 WebGPU [P, secondary] | No | **At parity with Magic Eraser and Clean Up** on small and medium distractions over texture. **Clearly behind Generative Remove** on large objects over structured backgrounds such as buildings and faces | L+P | Crop to the mask bbox plus context, run at a fixed 512², paste back with feathering. LaMa's FFT ops are the fp16/WebGPU risk. **MAT (CC BY-NC) and FcF (NVIDIA non-commercial StyleGAN2-ADA code) are excluded.** Places2 dataset terms are unverified |
| 10 | Auto distraction / people removal | LrC Distraction Removal: People (device, unverified); Google suggestions | RF-DETR person → SAM 2.1 → saliency (MODNet/BiRefNet) to protect the subject → MI-GAN/LaMa | as #1, #5, #9 | Apache / MIT | Borderline, 1–3 s (approx) | No | Detection is easy. Deciding what "distracting" means is heuristic (small, far, not salient, cut by the frame), and LrC will look smarter at first | L+P | Show the proposals as removable chips; never auto-apply |
| 11 | Dust spot removal | LrC Distraction Removal: Dust | Classical sensor-dust detection in Rust (spots at fixed positions across frames, local-contrast blobs on smooth areas) + MI-GAN | MI-GAN only | MIT | Yes | No | Near LrC for sensor dust | P | Batch-aware: the same dust position across a shoot |
| 12 | Wire / cable removal | ACR / LrC "wires and cables" (device, unverified); PS Remove "find distractions" | **No released open wire-segmentation model.** WireSegHR (Adobe, CVPR'23) released test data only; WireSeg-32K (arXiv 2609.03102, 2026-09-02) release is unclear. Workable now: stroke-snapping ridge detector (Rust: Hessian/Frangi + Hough) → LaMa, which is good on thin masks | LaMa | LaMa Apache; WireSegHR data licence unclear | Yes (classical + LaMa) | No | **Behind Adobe on automatic detection.** Close on user-stroked wires against sky | P | Track WireSeg-32K. It fine-tunes SAM 3, so the licence would inherit (verify) |
| 13 | Reflection removal (through glass) | LrC Reflections (device); Samsung reflection eraser | DSRNet (Apache code, **weights unclear — verify**); RDNet and DSIT (**no licence**) | — | See models | Borderline (DSRNet-class nets, approx) | — | **Research gap.** Nothing permissive and strong. Clearly behind LrC | P (defer) | Do not ship until a cleared model exists |
| 14 | Shadow removal | Samsung shadow eraser; Google | ShadowFormer (MIT; ISTD/SRD data unverified) | small (unverified) | MIT | Yes (approx) | No | Behind on real photos; trained on narrow datasets | P (defer) | ShadowDiffusion has no licence |
| 15 | Glasses glare removal | Evoto (cloud) | Landmark eye-region mask + specular threshold → MI-GAN; Tier 3: FLUX.2 [klein] edit | as above | MIT / Apache | Yes (classical path) | Generative path | **Behind Evoto** on heavy glare | P | |
| 16 | Generative fill with a prompt | PS Generative Fill (cloud: Firefly, Gemini 3, FLUX.2 Pro); Luminar GenSwap (cloud); Google Magic Editor (cloud); Samsung Generative edit | **FLUX.2 [klein] 4B**; **SDXL-inpainting 0.1 + SDXL-Lightning**; Qwen-Image-Edit-2511; LongCat-Image-Edit; SD 1.5-inpainting + LCM-LoRA (browser experiment) | **klein 4B:** 3.88B DiT + Qwen3-4B encoder; DiT bf16 7.75 GB / fp8 4.07 / nvfp4 2.46 / GGUF Q8 4.30 / Q4_K_M 2.60 GB; encoder bf16 8.05 GB; "~13 GB VRAM" bf16 [P]. **SDXL inpaint:** ~2.6B UNet, ~6.9 GB fp16 set (approx). **Qwen-Image-Edit-2511:** 20.43B + Qwen2.5-VL-7B; Q4_K_M 13.2 GB, Q3_K_M 9.9 GB; nunchaku int4 11.5 GB. **SD 1.5-inpaint:** ~0.86B UNet, ~2 GB fp16 | klein 4B **Apache-2.0** (ungated, 2026-01-15). SDXL inpaint and Lightning **OpenRAIL++**. SD 1.5 **OpenRAIL-M**. Qwen-Image-Edit Apache-2.0. LongCat Apache-2.0 | **No** for klein, SDXL and Qwen. SD 1.5-inpaint + LCM is **borderline**: 10–30 s at 512² on M3 Air, ~2 GB download (approx) | **Yes** | Klein 4B and Qwen-Image-Edit: usable for simple fills, but **below Firefly on photoreal texture matching** and **well below Nano Banana 2 and GPT Image 2** on complex prompts, text and identity (assessment; no controlled comparison found) | P (L as an optional pack on capable machines) | **FLUX.1 Fill [dev], FLUX.1 Kontext [dev], FLUX.2 [dev] and FLUX.2 [klein] 9B are BFL non-commercial**, as are ICEdit and UniWorld (inherited). SD-Turbo, SDXL-Turbo and SD3.5 terminate above **$1M revenue**. Always composite through the mask |
| 17 | Generative expand / outpaint | PS Generative Expand (cloud); Luminar GenExpand; Samsung | FLUX.2 [klein] 4B; SDXL-inpaint + **xinsir ControlNet Union ProMax**; LaMa for small, textured extensions | ControlNet ProMax 1.26B, ~2.5 GB fp16 (approx) | ControlNet Union Apache-2.0 on an OpenRAIL++ base | LaMa extension yes; generative no | Yes (generative) | LaMa is fine for ~10% sky, grass or wall. The generative path trails Firefly on scene coherence | L+P | Expand in several steps; keep the original pixels untouched |
| 18 | Generate background (product or portrait) | Photoroom AI Backgrounds (cloud); PS Generate Background (cloud) | Cutout (#3) → SDXL-inpaint + ControlNet ProMax, or klein 4B edit; contact shadows via GPSDiffusion or a classical matte-plus-depth drop shadow | GPSDiffusion is SD-based | GPSDiffusion MIT (SD base licence applies); DESOBAv2 Apache | No | Yes | Behind Photoroom on lighting consistency and shadows | P | PRX (Photoroom, 1.3B, Apache-2.0) is text-to-image only; its T5-Gemma encoder brings **Gemma Terms** |
| 19 | Harmonize a pasted layer | PS Harmonize (cloud); Photoroom Relight (cloud) | **PCT-Net**; INR-Harmonization; classical Lab statistics matching under the mask | PCT-Net "lightweight" (size unverified) | PCT-Net **MPL-2.0** (weights inherit, verify); INR-Harmonization Apache-2.0; iHarmony4 terms unverified | Yes, 100–300 ms (approx) | No | Colour and tone matching is fine. **Behind PS Harmonize**, which also relights and adds shadows | L+P | **Harmonizer is CC BY-NC-SA.** DucoNet has no licence. libcom bundles LBM and FLUX Kontext (non-commercial), so pick its components individually |
| 20 | Replace or swap an object | Luminar GenSwap; PS Generative Fill | FLUX.2 [klein] 4B edit (multi-reference); Qwen-Image-Edit-2511 | as #16 | Apache-2.0 | No | Yes | Behind Firefly and Nano Banana on perspective and lighting fit | P | |
| 21 | End-to-end instruction edit ("make it golden hour") | GPT Image 2, Nano Banana 2 / Pro, Flux Kontext Pro (cloud); Google Help me edit; PS AI Assistant | **FLUX.2 [klein] 4B**; **Qwen-Image-Edit-2511**; OmniGen2; Step1X-Edit v1.2; GLM-Image; Ovis-U1-3B | OmniGen2 3.97B + Qwen2.5-VL-3B, ~17 GB native. Step1X-Edit 12B + Qwen2.5-VL-7B: FP8 + offload 18 GB, 51 s. GLM-Image 9B AR + 7B DiT, ~23 GB with offload | klein and Qwen Apache-2.0. **OmniGen2 is Apache but embeds Qwen2.5-VL-3B (qwen-research, non-commercial), so inheritance is unclear — verify.** Step1X-Edit Apache (FLUX-derived weights unclear — verify). GLM-Image MIT | No | Yes | **Clearly behind** the cloud leaders on identity preservation, text and multi-step intent, with global drift in untouched areas | P | Used only as a tool inside the plan (§7), never as the whole edit. **HunyuanImage 2.1/3.0 exclude the EU, UK and South Korea**; Bria FIBO is CC BY-NC; JarvisArt is non-commercial |

### 3C. Enhancement and restoration

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 22 | Super resolution (faithful) | LrC / ACR Super Resolution (device); Pixelmator ML Super Resolution (device); Topaz Gigapixel (device or cloud) | **realesr-general-x4v3**; **4x-Nomos8k-span-otf-medium** (SPAN); **4xNomosWebPhoto_RealPLKSR** / RealPLKSR; Real-ESRGAN x4plus; 4xRealWebPhoto-v4-dat2; Swin2SR | x4v3 ~1.2M (approx), 4.9 MB [M]. SPAN ~0.4M, 0.9 MB. RealPLKSR ~7M, 29.7 MB. x4plus 16.7M, 67 MB [M] (69.5 / 36.1 MB facefusion). DAT2 48.8 MB. Swin2SR ×2 54.4 / 32.4 / 19 MB | x4v3 and x4plus BSD-3. SPAN, Nomos and RealWebPhoto **CC-BY-4.0** (attribution). RealPLKSR pretrain MIT (darktable) / CC0. Swin2SR Apache-2.0 | **Yes.** 3 MP→12 MP: x4v3 or SPAN 4–10 s; x4plus 25–60 s (approx). DAT2 borderline | No | Matches LrC Super Resolution for 2×. GAN hallucination on foliage and text. **Behind Topaz on faces and text** | L+P | **4x-UltraSharp and UltraSharpV2 are CC-BY-NC-SA.** Across 671 OpenModelDB models, 280 are NC or NC-SA. openpixels caps output at ~40 MP [M] |
| 23 | Generative upscale | Topaz Wonder / Redefine; PS Generative Upscale (cloud) | **AdcSR**; PiSA-SR; OSEDiff (one-step, SD 2.1 base) | SD 2.1 base ~0.87B UNet, ~2.6 GB fp16 set (approx) + LoRA | Code Apache-2.0; SD 2.1 base OpenRAIL++ | No | Yes: ~0.3–1 s per 512² tile on a 4070 (approx) | Probably behind Topaz Redefine (unverified). One-step models hallucinate less than multi-step ones | P | **SUPIR, HYPIR, StableSR and InvSR are non-commercial; FluxSR is FLUX dev-based** |
| 24 | Denoise (RGB, JPEG, phone) | Pixelmator ML Denoise; Topaz; Luminar Noiseless | **NAFNet-SIDD w32**; SCUNet; realesr-general-wdn (shipped) | NAFNet w32 ~29M (approx), ~116 MB (unverified); SCUNet ~18M (unverified) | NAFNet MIT (darktable ships it); SCUNet Apache-2.0 | **Yes.** 512² tile 0.3–1 s → 12 MP in 20–60 s (approx) | No | Good on sensor-noise JPEGs. **Behind LrC**, which denoises the raw mosaic | L+P | |
| 25 | RAW denoise / joint demosaic | LrC Denoise (raw); DxO DeepPRIME | **RawNIND UtNet2** (darktable 5.6); vkdt `jddcnn`; PMRID | UtNet2 fixed 512², Bayer [1,4,512,512] → [1,3,1024,1024], opset 20 fp32 [P]. vkdt: 45 ms for a 100 MP frame on an RTX 4080S [P] | RawNIND **GPL-3.0 weights and code** (RawNIND data CC BY / CC0). vkdt BSD-2 code, **weights unclear — verify**. PMRID Apache-2.0 (MegEngine format) | **Borderline.** fp32 only: darktable hit fp16 overflow on CoreML, and WebGPU fp16 carries the same risk. 24 MP raw in 1–3 min (approx) | Better native (darktable: "a few seconds" on a GPU) | Credible open raw denoise; comparison with LrC unverified | P | Copyleft weights: an optional pack only if the project licence allows (legal call). The only permissive path is PMRID (needs conversion) |
| 26 | Deblur (motion / defocus) | Google Photo Unblur (device); Topaz Sharpen / Super Focus; Luminar Supersharp | **NAFNet-GoPro** (32.87 / 33.71 dB); FFTformer; Restormer motion/defocus | NAFNet ~70–120 MB (unverified); Restormer ~26M (approx) | NAFNet, FFTformer and Restormer MIT; DeblurGANv2 BSD-style | NAFNet borderline; FFTformer no | FFTformer / Restormer native | **Behind Topaz and Unblur.** GoPro-trained models generalise poorly to real handheld blur | P | **MPRNet is Academic Public Licence; Stripformer is MIT edited to non-commercial** |
| 27 | JPEG artefact / deblock | Pixelmator Deband / ML Enhance; Topaz | **1x-DeJPG-OmniSR**; 1x-DeJPG-realplksr-otf; FBCNN (quality-factor adjustable) | OmniSR 6.5 MB; realplksr 29.6 MB; FBCNN (size unverified) | DeJPG models **CC-BY-4.0**; FBCNN Apache-2.0 | Yes | No | Near Pixelmator Deband on web JPEGs (assessment) | L+P | **1x-JPG-00-20…80-100 and Kim2091-DeJpeg are NC** |
| 28 | Low-light enhance | Luminar; LrC Auto; phone night modes (capture-time) | **Retinexformer**; HVI-CIDNet | Retinexformer 1.61M (~6.5 MB, approx); CIDNet ~1.9M | Both MIT (LOL dataset terms unverified) | Yes, <200 ms at 1–2 MP (approx) | No | Tends to over-brighten and flatten; treat it as a starting point with a strength slider | L | **Zero-DCE / Zero-DCE++ (CC BY-NC) and LLFlow (CC BY-NC-SA) are excluded.** SCI has no licence |
| 29 | Face restoration | Topaz Recover Faces v3; Remini (cloud); PS Neural Filters | **GFPGAN 1.4** (shipped); RestoreFormer++; PMRF (native) | GFPGAN 340 MB fp32 [M]; RestoreFormer++ 294 MB | GFPGAN Apache-2.0 "except third-party components" (FFHQ NC-SA training data; DFDNet-derived code) → **grey**. RestoreFormer++ Apache (FFHQ). PMRF MIT | **Yes: 0.27 s per face on WebGPU vs 10.7 s on CPU [M]** | PMRF native | **Behind CodeFormer and Topaz v3** on severe degradation; identity drift, so blend strength matters | L+P | **CodeFormer (S-Lab), GPEN (no licence; models withdrawn "due to commercial issues"), DifFace and OSDFace are excluded.** **fp16 GFPGAN is silently broken on WebGPU [M]** |
| 30 | Old photo restoration | PS Neural Filter Photo Restoration; MyHeritage (cloud); Remini | Pipeline: scratch detection (Microsoft BOPBTL) → LaMa → GFPGAN → colourise (#32) | BOPBTL sizes unverified | BOPBTL README says "MIT", but also "academic research use only" → **conflict, verify** | Borderline (whole pipeline) | No | Behind MyHeritage on heavy damage | L+P | openpixels "Old-photo preset" is the starting point [M] |
| 31 | Scratch and dust removal on scans | Topaz Dust & Scratch; LrC Dust | Classical morphological scratch detection, or BOPBTL detector → MI-GAN | as above | as above | Yes | No | Near-parity on film dust; weaker on emulsion damage | P | |
| 32 | Colourisation | PS Neural Filter Colorize; MyHeritage (cloud) | **DeOldify artistic** (shipped); **DDColor-tiny**; DDColor large | DeOldify 255 MB fp32 [M] (stable 873 MB). DDColor-tiny .pth 220 MB; large .pth 912 MB, ONNX 980 MB | DeOldify MIT (weights stated MIT). DDColor Apache-2.0 on the repo and every HF weight repo (ImageNet terms unverified) | **Yes** (DeOldify at 256², openpixels [M]); DDColor-tiny borderline | DDColor large native | Both behind Nano Banana and MyHeritage on plausibility. DDColor is less brown than DeOldify (assessment) | L+P | **CT2 is CC BY-NC.** Run at 256², keep L from the original, take ab from the model [M] |

### 3D. Tone, colour, depth, relighting and portrait

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 33 | Auto enhance / auto tone | LrC Auto; Pixelmator ML Enhance; Luminar Enhance AI; Google auto-enhance (all device) | Classical Rust auto: percentile levels, exposure, local contrast. **Image-Adaptive-3DLUT / AdaInt / SepLUT architectures, retrained on licensed data** | 3D-LUT <600K params; 4K in <2 ms on a Titan RTX [P] | Code Apache-2.0. **All public checkpoints are trained on MIT-Adobe FiveK (research only) or PPR10K (non-commercial, "derived data" named)** | Yes, trivially | No | Classical auto is **below LrC Auto**. A retrained LUT net is plausibly comparable, since LrC Auto is also a global move (unverified) | L | The project is the training data, not the model. CLUT-Net and CSRNet have no licence. JarvisArt is non-commercial |
| 34 | Auto white balance | LrC Auto WB; Pixelmator ML WB | Classical grey-world, grey-edge and white-patch; **C5** (cross-camera constancy); FC4 | Tiny | C5 Apache-2.0; FC4 MIT; training sets (NUS, Gehler-Shi) unclear — verify | Yes | No | Behind LrC on mixed light | L | **Deep White-Balance Editing and Mixed-illuminant WB (Afifi) are non-commercial** |
| 35 | Adaptive / subject-aware presets | LrC Adaptive presets (subject, sky, portrait) | No new model: recipes over masks from #2, #4 and #6 | — | — | Yes | No | **Parity is achievable**, because the feature is masks plus recipes | L+P | Ship presets as data |
| 36 | Match a look from a reference image | Pixelmator Match Colors; PS Neural Filter Colour Transfer | **Classical Reinhard / MKL / sliced optimal transport fitted to a 3D LUT in Rust**; WCT2 (optional) | WCT2 VGG encoder and decoders, hundreds of MB (approx) | Classical: no weights; WCT2 MIT | Yes | No | Classical LUT fit ≈ Match Colors in kind (assessment); WCT2 adds texture-aware transfer | L+P | **Neural Preset (official), FastPhotoStyle / PhotoWCT, Deep Preset and SA-LUT are all non-commercial** |
| 37 | Sky replacement | PS Sky Replacement; Luminar Sky AI | Sky mask (#4) + classical blend, foreground colour and exposure match, optional reflection mirror. No diffusion needed | as #4 | as #4 | Borderline (mask cost) | No | Behind Luminar until the sky mask is solved on trees | L+P | |
| 38 | Depth map for lens blur | LrC Lens Blur (device); PS Neural Depth Blur; Luminar Portrait Bokeh; Apple Portrait | **Depth Anything V2-Small**; **DA3-Small / DA3-Base / DA3MONO-LARGE**; MoGe-2; Distill-Any-Depth-S; MiDaS / DPT | DA-V2-S 24.8M: **99.1 / 49.6 / 27.3 / 19.1 MB (q4f16)**. DA3-Small 0.08B (~105 MB). DA3-Base 0.12B. DA3MONO-LARGE 0.35B (~1.4 GB fp32, approx) | DA-V2-Small **Apache-2.0**. DA3 Small, Base, Mono-Large and Metric-Large **Apache-2.0**. MoGe / MoGe-2 MIT. MiDaS MIT | DA-V2-S **yes**, 100–300 ms at 518 px (Xenova claims "<200 ms") [P]. DA3-Mono-L borderline | DA3 large variants native | Depth is good. **Hair and edge depth is the gap vs LrC Lens Blur**, so fuse with the matte | L+P | **DA-V2 Base/Large/Giant, DA3 Large/Giant/Nested and Video-DA Base/Large are CC BY-NC.** onnx-community `depth-anything-v3-large` is tagged Apache but its base checkpoint is unstated, so verify. **Depth Pro weights are `apple-amlr` (research only). UniDepth, DepthCrafter and Sapiens are non-commercial** |
| 39 | Bokeh rendering | as #38 | Classical occlusion-aware layered gather in WGSL/Rust; BokehMe (optional neural correction) | small | BokehMe Apache-2.0 | Yes | No | Near LrC with a good matte | L+P | Rust owns this, not a model |
| 40 | Relighting | Luminar Relight AI / Light Depth (device); Photoroom AI Relight (cloud) | **Depth (#38) + MoGe-2 ViT-S normals → Lambertian / point-light shading**; Tier 3: **IC-Light v1** | MoGe-2 ViT-S ~35M (~140 MB, approx). IC-Light fc/fbc/fcon 1.72 GB each (SD 1.5 UNet) | MoGe-2 MIT. **IC-Light code Apache-2.0; weights show `creativeml-openrail-m` on one reading of the card and no licence field via the HF API → verify** | Depth-shading path yes | IC-Light native | Depth-shading ≈ Luminar Relight AI in kind. **Far behind** generative relighting (IC-Light v2, Photoroom) on faces | L+P | **IC-Light v2 (FLUX) is non-commercial and unreleased; LBM relighting is CC BY-NC; DSINE normals are non-commercial.** IC-Light's demo uses RMBG-1.4, so swap in BiRefNet or MODNet |
| 41 | Skin smoothing | Evoto (cloud); PS Neural Filter Skin Smoothing; Luminar Skin AI; PortraitPro | **Frequency separation / surface blur inside the Multiclass skin mask** (openphotoid `skin.rs`); **ABPN** (ModelScope `damo/cv_unet_skin-retouching`) | ABPN size unverified | Classical: none. ABPN **Apache-2.0** per the ModelScope API (bundled face detector licence unverified) | Yes | No | Classical ≈ Luminar Skin AI. **Behind Evoto's texture-preserving retouch** | L+P | RetouchFormer has no licence (FFHQR data) |
| 42 | Blemish removal | Evoto; PS Spot Healing (device); Luminar | DoG blob detection on the skin mask (Rust) → MI-GAN on small patches; ABPN | as above | MIT / Apache | Yes | No | Fine on isolated spots; **behind Evoto on acne clusters** | L+P | |
| 43 | Eye and teeth enhancement | Luminar Face AI; Evoto | Face Landmarker polygons + HSL, clarity and whitening | ~3.7 MB | Apache-2.0 (card) | Yes | No | Parity in kind | L | |
| 44 | Face-aware liquify | PS Face-Aware Liquify (device); Evoto face reshape | **MediaPipe Face Landmarker (478 points)** + moving-least-squares / mesh warp in Rust | ~3.7 MB | Apache-2.0 | Yes, <50 ms landmarks (approx) | No | Parity with PS in kind | P | **dlib `shape_predictor_68` cannot be used commercially** (author's note, iBUG 300-W). **InsightFace 2d106 is non-commercial.** PIPNet and SPIGA weights are tainted by 300W, WFLW and CelebA |
| 45 | Body reshaping | Luminar Body AI; Evoto | **MediaPipe Pose Landmarker** (33 points); RTMPose / DWPose / ViTPose; person mask + constrained warp + MI-GAN for exposed background | Pose Lite 3 MB / Full 6 MB / Heavy 26 MB. RTMPose-t/s/m/l 3.5M / 5.7M / 13.9M / 28.1M | MediaPipe Pose Apache-2.0 (card). RTMPose, DWPose and ViTPose code Apache (training-mix licences unclear) | Yes | No | Behind: background bending near the body needs fill | P | **SMPL / SMPL-X are non-commercial; Alibaba FlowBasedBodyReshaping is academic only; Sapiens is non-commercial** |
| 46 | Flyaway hair clean-up | Evoto | Matte boundary band + MI-GAN; Tier 3: klein edit in a masked band | as above | as above | Yes (classical path) | Generative path | **Clearly behind Evoto** | P | |

### 3E. Semantic, organisation and culling

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 47 | Semantic search ("beach at sunset") | Lightroom cloud search (cloud); Apple and Google Photos | **CLIP ViT-B/32**; **SigLIP 2 base p16-224**; Meta PE-Core B16 | CLIP B/32 ~151M total; vision tower ~88 MB q8 (approx). SigLIP 2 base 0.4B. PE-Core B16 0.09B vision + 0.31B text | CLIP MIT; SigLIP 2 Apache-2.0; PE-Core Apache-2.0 | Yes, 30–100 ms per image (approx) | No | Comparable to Lightroom's cloud search for common concepts (unverified) | L+P | **MobileCLIP / MobileCLIP2 are `apple-amlr` (research only); jina-clip-v2 is CC BY-NC.** Prefer OpenCLIP DataComp checkpoints to LAION ones |
| 48 | Auto-tagging and keywords | Adobe Sensei (cloud); Excire (device) | Zero-shot CLIP/SigLIP against a keyword vocabulary; Florence-2 caption → keywords | as #47, #51 | as above | Yes | No | Adequate; below Excire's trained taxonomy (assessment) | P | |
| 49 | Culling: eyes open, focus, duplicates, aesthetics | LrC Assisted Culling (eye focus, eyes open); Aftershoot, Imagen | **Blink:** Face Landmarker blendshapes. **Focus:** Laplacian variance on the eye region (openpixels metrics). **Duplicates:** pHash + CLIP cosine (or DINOv2-S). **Aesthetics:** LAION aesthetic predictor linear head on CLIP | Heads <1 MB; DINOv2-S 21M (~88 MB) | LAION aesthetic MIT; improved-aesthetic-predictor Apache; DINOv2 Apache-2.0 | Yes | No | Eyes open and focus at parity with LrC. **"Aesthetic" is weak** vs Aftershoot's per-photographer models | P | **pyiqa is PolyForm Noncommercial; CLIP-IQA and Q-Align code are S-Lab.** Reimplementing the CLIP-IQA idea is fine. DINOv3 has a custom licence |
| 50 | Face recognition and grouping | LrC People (device); Apple and Google Photos | **YuNet → SFace**; EdgeFace | SFace (MobileFaceNet) fp32 ~37 MB (approx), int8 available; accuracy 0.9940 [P]. EdgeFace 1.24–3.65M | SFace Apache-2.0 (training data unstated, verify). EdgeFace BSD-3 code (WebFace-derived, unverified) | Yes, 5–20 ms per face (approx) | No | **Below ArcFace-R100** (InsightFace) on hard cases such as age gaps and profiles | L+P | **InsightFace buffalo_l, antelopev2 and SCRFD weights are non-commercial.** Immich's use rests on an emailed permission that does not extend to third parties. AdaFace and facenet-pytorch weights sit on retracted or research datasets (grey) |
| 51 | Captioning and alt text | Adobe Express / PS alt text (cloud); Apple | **Florence-2-base**; **SmolVLM-256M**; Qwen3.5-0.8B | Florence-2-base q4f16 **224 MB**. SmolVLM-256M q4f16 **189 MB**. Qwen3.5-0.8B q4f16 **646 MB** | Florence-2 MIT; SmolVLM Apache-2.0; Qwen3.5 Apache-2.0 | Yes, 1–4 s (approx) | No | Florence-2 is literal and terse; below cloud VLMs on nuance | L+P | **Qwen2.5-VL-3B is qwen-research (non-commercial); Moondream 3 is BSL 1.1; LFM2-VL is capped at $10M revenue** |
| 52 | OCR (text into editable layers) | Apple Live Text (device); Google Lens | **PP-OCRv5 mobile det + rec**; Tesseract (tesseract.js); Florence-2 `<OCR>` | PP-OCRv5 mobile a few MB each (~20 MB set, approx) | PP-OCR Apache-2.0; Tesseract Apache-2.0 | Yes, 0.2–1 s (approx) | PaddleOCR-VL (~1B) and DeepSeek-OCR (3B, MIT) native | Near Live Text on Latin and CJK signage (unverified) | L+P | **Surya weights are RAIL-M with a $5M funding/revenue cap** |

### 3F. Text and fonts

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 53 | Font identification | PS Match Font (device, unverified); WhatTheFont (cloud) | **storia/font-classify-onnx** (ResNet-50, ~3,000 Google Fonts); gaborcselle/font-identifier (ResNet-18) | ~95 MB / ~45 MB fp32 (approx) | HF tag MIT for both (storia's GitHub licence not stated, verify) | Yes, 50–200 ms (approx) | No | Below PS Match Font on breadth, but it maps to **freely licensed Google Fonts**, which are the only fonts we can ship | P | |
| 54 | Edit text in a photo | Nano Banana 2 / GPT Image 2 (cloud); Google Lens translate | **Deterministic:** PP-OCRv5 detect → erase with MI-GAN/LaMa (ViTEraser code MIT, weights unverified) → font-id → re-render as a real text layer with perspective, colour and noise matching. **Generative:** Qwen-Image-Edit-2511; AnyText2 | AnyText2 on an SD 1.5 base (~2 GB) | AnyText2 Apache-2.0 + OpenRAIL-M base; Qwen-Image-Edit Apache-2.0 | Deterministic path yes | Generative path native | Deterministic path is clean on flat signage, screenshots and posters. **Behind GPT Image 2** on curved or textured text | P | **RepText and FLUX-Text are FLUX.1 [dev]-based (non-commercial).** TextDiffuser-2 carries an "academic only" README note (verify) |

### 3G. Command bar and agent

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 55 | Natural-language command bar ("warmer, and remove the person on the left") | PS AI Assistant (cloud); Google Help me edit (cloud) | **Browser:** Qwen3-1.7B, Qwen3-4B-Instruct-2507, Qwen3.5-0.8B/2B (vision). **Native:** Qwen3.5-4B, Gemma 4 E4B, Phi-4-mini, SmolLM3-3B, Granite-4.0-micro | WebLLM VRAM (q4f16): Qwen3-0.6B 1,403 MB; **Qwen3-1.7B 2,037 MB**; Qwen3-4B 3,432 MB; Qwen3.5-0.8B 1,629 MB; Qwen3.5-2B 2,245 MB; Qwen3.5-4B 3,868 MB; Phi-4-mini 3,438 MB [P]. Downloads roughly 0.5–2.5 GB (approx) | Qwen3 and Qwen3.5 Apache-2.0; Gemma 4 **Apache-2.0** (2026-04-02); Phi-4-mini MIT; SmolLM3 and Granite 4 Apache-2.0 | **Borderline.** transformers.js Qwen3-4B q4f16 on an M2 Pro: **22.3 tok/s, TTFT 0.8 s**; Qwen3.5-4B: 14.6 tok/s, **TTFT 16 s** (open issue) [P]. A 150-token plan takes ~7–12 s on an M3 Air (approx) | 4B VLMs are comfortable native | Good at compound adjustments and single-object references with constrained decoding. **Well below cloud agents** on vague aesthetic intent. BFCL V4 overall: Qwen3-8B 42.6%, Qwen3-4B-2507 35.7%, Qwen3-1.7B 28.4%, Llama-3.2-3B 22.0% [P] | L+P (Lite gets a rule-based parser fallback with no model) | **xLAM-2 and Hammer2.1 function-calling fine-tunes are CC BY-NC or qwen-research; Llama 3.2 11B Vision excludes EU-domiciled developers; LFM2 is capped at $10M.** Gemma 3 / 3n use Gemma Terms (a prohibited-use policy passes downstream) |
| 56 | "What should I fix?" suggestions | Google Help me edit suggestions; Luminar | Qwen3.5-4B or Gemma 4 E4B (vision) native; Qwen3.5-0.8B browser; plus deterministic image stats (clipping, cast, noise, blur, tilt) | as above; Gemma 4 E2B ONNX q4f16 ≈ 3.4 GB (approx) | Apache-2.0 | Borderline (0.8B) | 4B native | Deterministic stats do most of the work; the VLM ranks and explains | L+P | Feed the VLM measured stats, not only pixels |

### 3H. Creative

| # | Feature | Incumbents (cloud?) | Candidate open models | Params / size | Licence | Browser | Native-only | Quality vs incumbent (honest) | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 57 | Vectorise / image trace | Illustrator Image Trace (device); Vectorizer.ai (cloud) | **vtracer** (Rust, no model); StarVector-1B im2svg | StarVector 1B (~2 GB fp16, approx) | vtracer MIT; StarVector Apache-2.0 | vtracer yes | StarVector native | vtracer ≈ Illustrator Image Trace; **behind Vectorizer.ai** on logos | P | **potrace and autotrace are GPL.** OmniSVG sits on Qwen2.5-VL-3B (research licence) |
| 58 | Seamless pattern / texture | Illustrator / Firefly Text to Pattern (cloud) | Classical: offset + blend, Wang tiles, patch-based synthesis (Rust). Generative: Z-Image-Turbo or SDXL with circular conv padding | Z-Image-Turbo 6.15B + Qwen3-4B encoder; GGUF Q4_K 3.86 GB, Q8 6.58 GB | Z-Image-Turbo Apache-2.0 | Classical yes | Generative native | Classical is good for tiling a photo. The generative path trails Firefly (unverified) | P | Z-Image-Edit is **not yet released** (Sept 2026) |
| 59 | Generate image from text | PS Generate Image (cloud) | **Z-Image-Turbo**; FLUX.2 [klein] 4B; FLUX.1 [schnell] (12B); Sana-Sprint 0.6B/1.6B; PRX 1.3B | as above; Sana-Sprint "0.3 s per 1024² on a 4090" [P] | Z-Image, klein 4B and schnell Apache-2.0. **Sana: Apache + Gemma-2 encoder under Gemma Terms. PRX: Apache + T5-Gemma (Gemma Terms)** | Intel shows Z-Image-Turbo running in-browser on AI PCs (unverified) | Yes | Below the Firefly, GPT Image and Nano Banana top tier (assessment) | P | Not core to a photo editor; it falls out of the Tier 3 pack |
| 60 | AI brush (edge-aware and generative) | PS brush-based Generative Fill; Luminar GenSwap brush | **Edge-aware:** EdgeTAM decoder per stroke (encoder cached). **Generative:** #16 models driven by a stroke mask | as #1 / #16 | as #1 / #16 | Edge-aware yes | Generative native | Edge-aware brush ≈ LrC and PS auto-mask brushes | L+P | |
| 61 | Style / artistic filter | PS Neural Filters Style Transfer | AdaIN (port MIT, unverified); WCT2 MIT | small–medium | as stated | Yes | No | Dated look; low priority | P (defer) | |

### 3I. Provenance

| # | Feature | Incumbents | Candidate | Size | Licence | Browser | Native-only | Quality | Profile | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| 62 | Content Credentials for AI edits | Adobe PS/LR (Content Authenticity); Pixel 10 (every camera JPEG); Google Photos on save; Galaxy S25 (AI edits only) | **`c2pa` crate 0.90.22** (2026-09-10) | no model | **MIT OR Apache-2.0** | **Yes.** `wasm32-unknown-unknown` is Tier 1A with `rust_native_crypto`, no OpenSSL [P] | No | Manifest content can match Adobe's. **Verifiers show an "unknown source" unless the signing certificate is on the C2PA Trust List** (Interim Trust List frozen 2026-01-01) | L+P | Record one `c2pa.actions` entry per tool call. Use `digitalSourceType` `compositeWithTrainedAlgorithmicMedia` for fills (check the exact IPTC spelling), `algorithmicallyEnhanced` for SR and denoise, and `trainedAlgorithmicMedia` for fully generated images. Start with `c2pa.opened` plus an ingredient |

---

## 4. Tiering recommendation

**How each tier is delivered:**
- **Tier 1** is served from our own origin, fetched on the first use of a feature, cached in Cache Storage or OPFS, and pinned by sha256.
- **Tier 2** is an explicit download from a Models page with size and licence shown (the openpixels pattern).
- **Tier 3** needs the native backend (Tauri or a local server) and a GPU. Tier 3 also runs every Tier 1 and 2 model natively and faster.

**The fp16 rule.** Ship fp32 by default for anything under ~150 MB. Enable fp16 only after the pixel-parity canary passes on WebGPU for that exact file (§8). openpixels [M] shows fp16 buys nothing on CPU and can be silently wrong on the GPU.

### Tier 1: in the browser by default (small, permissive, proven or near-proven)

| Feature group | Exact model (file) | Size | Licence |
|---|---|---|---|
| Click-select, edge-aware brush | EdgeTAM encoder + decoder (onnx-community, fp32) | 40.5 MB | Apache-2.0 |
| Portrait subject, cutout | MODNet photographic (openphotoid file) | 24.7 MB | Apache-2.0 |
| Remove object, dust, blemish, glare | MI-GAN `migan_pipeline_v2.onnx` | 28.1 MB | MIT (weights MIT) |
| Skin, hair, clothes masks | MediaPipe Selfie Multiclass 256² | 16.4 MB | Apache-2.0 (card) |
| Face parts, liquify, blink | MediaPipe Face Landmarker | ~3.7 MB (approx) | Apache-2.0 (card; verify the CC BY reading) |
| Body reshape | MediaPipe Pose Landmarker Full | 6 MB | Apache-2.0 |
| Face detection | YuNet 2023mar | 0.23 MB | MIT |
| Upscale, denoise (fast) | realesr-general-x4v3 + wdn + dn50 (openpixels) | 14.6 MB | BSD-3 |
| Upscale (sharper, tiny) | 4x-Nomos8k-span-otf-medium (SPAN) | 0.9 MB | CC-BY-4.0 (attribution) |
| JPEG artefacts | 1x-DeJPG-OmniSR | 6.5 MB | CC-BY-4.0 |
| Low-light | Retinexformer | ~6.5 MB (approx) | MIT |
| Depth, lens blur, relight base | Depth Anything V2-Small (fp32; fp16 49.6 MB after the canary) | 99.1 MB | Apache-2.0 |
| OCR, text layers | PP-OCRv5 mobile det + rec | ~20 MB (approx) | Apache-2.0 |
| No model needed | vtracer, LUT look transfer, classical auto tone/WB, frequency-separation skin, guided filter and band matting, bokeh renderer, `c2pa` | — | MIT / Apache |
| **Tier 1 total** | | **~267 MB (approx)** if everything is fetched; ~218 MB with fp16 depth | |

### Tier 2: optional browser downloads (100–800 MB each, WebGPU)

| Feature group | Exact model | Size | Licence / gate |
|---|---|---|---|
| High-quality click-select | SAM 2.1 hiera-small encoder + decoder (fp32) | 183.5 MB | Apache-2.0 |
| Text-prompt select | Grounding DINO tiny (fp16 after the canary; int8 204 MB fallback) | 360 MB | Apache-2.0 |
| Object cutout (HQ) | BiRefNet_lite (fp32) | 224 MB | MIT, **legal gate on DIS5K training data**. Clean fallback: BiRefNet-portrait for people (490 MB fp16) plus SAM 2.1 for objects |
| Remove (large masks) | big-lama, Carve fixed 512² (fp32) | 208 MB | Apache-2.0 |
| Upscale (quality) | 4xNomosWebPhoto_RealPLKSR + Real-ESRGAN x4plus (openpixels) | 29.7 + 67 MB | CC-BY-4.0 / BSD-3 |
| Denoise (quality) | NAFNet-SIDD width32 | ~116 MB (unverified) | MIT |
| Deblur | NAFNet-GoPro width32 | ~100 MB (unverified) | MIT |
| JPEG (quality) | 1x-DeJPG-realplksr-otf | 29.6 MB | CC-BY-4.0 |
| Face restore | GFPGAN 1.4 fp32 (openpixels) | 340 MB | Apache-2.0 (FFHQ grey, noted) |
| Colourise | DeOldify artistic fp32 (openpixels); evaluate DDColor-tiny (~220 MB, Apache) as a replacement | 255 MB | MIT |
| Relight normals | MoGe-2 ViT-S normal | ~140 MB (approx) | MIT |
| Harmonize | PCT-Net | <50 MB (approx, unverified) | MPL-2.0 (verify weights) |
| Search, aesthetics, duplicates | CLIP ViT-B/32 vision (q8) + LAION aesthetic head | ~88 MB (approx) | MIT |
| Face grouping | SFace (OpenCV Zoo) | ~37 MB (approx) | Apache-2.0 |
| Captions, alt text | Florence-2-base-ft q4f16 | 224 MB | MIT |
| Font id | storia font-classify-onnx | ~95 MB (approx) | MIT (HF tag; verify repo) |
| **Vision packs subtotal** | | **~2.5 GB (approx)** | |
| Command-bar planner (above the band) | Qwen3-1.7B q4f16 (WebLLM); vision-aware alternative Qwen3.5-0.8B (646 MB) | ~1.1 GB (approx) | Apache-2.0 |
| **Tier 2 total** | | **~3.6 GB (approx)**, each pack independent | |

### Tier 3: native backend only (multi-GB, needs a GPU)

| Feature group | Exact model | Size | Licence |
|---|---|---|---|
| Generative fill, expand, replace, instruction edit, text-to-image | **FLUX.2 [klein] 4B**: DiT fp8 (4.07 GB) + Qwen3-4B encoder (fp8/Q8 ~4 GB, approx) + VAE (~0.3 GB, approx). GGUF Q4 path ≈ 5.4 GB | **~8.4 GB** | Apache-2.0 |
| Relight (generative) | IC-Light v1 `fc` + SD 1.5 text encoder and VAE | ~2.1 GB | Apache code; **weights licence verify** (OpenRAIL-M expected) |
| Generative upscale | AdcSR (SD 2.1 base fp16 + weights) | ~2.7 GB (approx) | Apache-2.0 + OpenRAIL++ |
| Depth (HQ) | DA3MONO-LARGE fp16 | ~0.7 GB (approx) | Apache-2.0 |
| Planner / VLM | Qwen3.5-4B Q4_K_M (llama.cpp, GBNF); alternative Gemma 4 E4B | ~2.7 GB (approx) | Apache-2.0 |
| **Core generative pack total** | | **~16.5 GB (approx)** | |
| Compatibility pack (8 GB GPUs, faster on Macs) | SDXL-inpainting 0.1 fp16 + SDXL-Lightning LoRA + xinsir ControlNet Union ProMax | ~9.8 GB (approx) | OpenRAIL++ / Apache-2.0 |
| Workstation pack (≥24 GB VRAM) | Qwen-Image-Edit-2511 Q4_K_M + Qwen2.5-VL-7B encoder (Q4) + VAE | ~18 GB (approx) | Apache-2.0 |
| Optional copyleft pack | RawNIND UtNet2 raw denoise (darktable) | size unverified | **GPL-3.0**: legal call |
| **All Tier 3 packs** | | **~44 GB (approx)** | |

**Possible saving (unverified).** FLUX.2 [klein] 4B and Z-Image both condition on a Qwen3-4B-class text encoder (klein's config is `Qwen3ForCausalLM`, hidden size 2560). If those weights are the stock Qwen3-4B, one copy could serve as both the image encoder and the command-bar LLM (Qwen3-4B-Instruct-2507 is a different checkpoint; verify before relying on it). That would save ~4 GB.

---

## 5. Licence red flags: competitors use these, we must not ship them

| Do **not** ship | Why (licence as found) | Permissive substitute |
|---|---|---|
| **RMBG-1.4 / RMBG-2.0** (Bria) | Bria custom licence / **CC BY-NC 4.0**, gated; commercial use needs an agreement | BiRefNet-portrait / BiRefNet_lite (MIT, see the DIS5K note), MODNet |
| **CodeFormer**, DifFace, StableSR, InvSR, EdgeSAM, MatAnyone / MatAnyone 2, SA-LUT, APISR ("academic only") | NTU **S-Lab Licence 1.0** (non-commercial) | GFPGAN 1.4, RestoreFormer++, PMRF (MIT); EdgeTAM / SAM 2.1; classical band matting |
| **GPEN** | No licence file; best models withdrawn "due to commercial issues" | GFPGAN / RestoreFormer++ |
| **InsightFace** (buffalo_l, antelopev2, SCRFD, 2d106) | Weights "for non-commercial research only" (Immich has a non-transferable emailed permission) | YuNet + SFace (Apache), EdgeFace (BSD-3 code) |
| **dlib shape_predictor_68** | Trained on iBUG 300-W; the author notes it "can't be used in a commercial product" | MediaPipe Face Landmarker (Apache) |
| **SegFormer** (all NVIDIA checkpoints and fine-tunes, incl. jonathandinu/face-parsing), **FastPhotoStyle**, **FcF** (StyleGAN2-ADA code) | NVIDIA Source Code Licence (non-commercial) | MediaPipe Multiclass; GDINO + SAM for sky; WCT2; MI-GAN / LaMa |
| **BiSeNet face-parsing weights** | Trained on CelebAMask-HQ (non-commercial) | MediaPipe Multiclass + Face Landmarker |
| **Depth Anything V2 Base/Large/Giant, DA3 Large/Giant/Nested, Video-DA Base/Large** | CC BY-NC 4.0 | DA-V2-Small, DA3 Small/Base/Mono-Large/Metric-Large (Apache), MoGe-2 (MIT) |
| **Depth Pro** (Apple) | HF weights `apple-amlr`: research only (GitHub code licence differs) | DA3MONO-LARGE, MoGe-2 |
| **UniDepth, DepthCrafter, Sapiens v1** (seg, depth, normals) | CC BY-NC / academic only | DA3, MoGe-2, MediaPipe |
| **Sapiens2** | Commercial allowed, but bans "biometric processing" and deceptive content; Meta audit rights | MediaPipe, RF-DETR |
| **MobileCLIP / MobileCLIP2** | `apple-amlr` research only | CLIP (MIT), SigLIP 2, PE-Core (Apache) |
| **jina-clip-v2** | CC BY-NC 4.0 | SigLIP 2 |
| **4x-UltraSharp / UltraSharpV2** and ~280 NC OpenModelDB models | CC-BY-NC(-SA) | Nomos / RealWebPhoto / LSDIR (CC-BY), SPAN, RealPLKSR, Real-ESRGAN |
| **SUPIR, HYPIR** | "strictly non-commercial" | AdcSR, PiSA-SR, OSEDiff (Apache + SD 2.1) |
| **MPRNet, Stripformer** | Academic Public Licence / MIT edited to non-commercial | NAFNet, FFTformer, Restormer (MIT) |
| **Zero-DCE / Zero-DCE++, LLFlow** | CC BY-NC / CC BY-NC-SA | Retinexformer, HVI-CIDNet (MIT) |
| **CT2** colourisation | CC BY-NC 4.0 | DDColor (Apache), DeOldify (MIT) |
| **MAT** inpainting | CC BY-NC 4.0, "research purposes only" | MI-GAN (MIT), LaMa (Apache) |
| **FLUX.1 [dev], FLUX.1 Fill [dev], FLUX.1 Kontext [dev], FLUX.2 [dev], FLUX.2 [klein] 9B**; everything built on them (ICEdit, UniWorld-V1, RepText, FLUX-Text, FluxSR, FLUX dev ControlNets, Hyper-SD FLUX LoRAs) | BFL Non-Commercial Licence (outputs are usable commercially, but the *model* may not be used commercially) | **FLUX.2 [klein] 4B (Apache)**, Qwen-Image-Edit-2511, Z-Image-Turbo, FLUX.1 [schnell] |
| **SD-Turbo, SDXL-Turbo, SD3 / SD3.5**, TSD-SR (SD3 base) | Stability AI Community Licence: terminates above **$1M annual revenue**, registration required | SD 1.5 / SDXL (OpenRAIL), klein 4B, Z-Image-Turbo |
| **HunyuanImage 2.1 / 3.0** | Tencent Hunyuan Community: **excludes the EU, UK and South Korea**; licence needed above 100M MAU | Qwen-Image-Edit-2511 |
| **Bria FIBO** | CC BY-NC 4.0 | klein 4B |
| **IC-Light v2** (FLUX), **LBM relighting** (Jasper), **Harmonizer**, **DSINE** | Non-commercial / CC BY-NC(-SA) / Imperial non-commercial | IC-Light v1 (verify), PCT-Net / INR-Harmonization, MoGe-2 / StableNormal normals |
| **SkyAR, Neural Preset, Deep Preset, Deep WB, Mixed-illuminant WB** | CC BY-NC-SA / non-commercial | Classical sky blend; LUT transfer; C5 / classical WB |
| **SMPL / SMPL-X, Alibaba FlowBasedBodyReshaping, AGGN** | Max Planck non-commercial / academic only / no licence | MediaPipe Pose + classical warp |
| **pyiqa (IQA-PyTorch), CLIP-IQA, Q-Align code** | PolyForm Noncommercial / S-Lab | LAION aesthetic head (MIT), classical metrics, a reimplementation of the CLIP-IQA idea |
| **xLAM-2, Hammer2.1** (tool-calling fine-tunes), **JarvisArt** | CC BY-NC / qwen-research / JarvisArt Non-Commercial | Base Qwen3 / Qwen3.5 with grammar-constrained decoding |
| **Qwen2.5-VL-3B**, and derivatives that bundle it (OmniSVG; OmniGen2, verify) | Qwen Research Licence (non-commercial) | Qwen3.5-2B/4B, Qwen3-VL-2B, Qwen2.5-VL-7B (Apache) |
| **Moondream 3** | BSL 1.1 (use grant excludes competing hosted services) | Moondream 2 (Apache), Florence-2, SmolVLM |
| **LFM2 / LFM2-VL** | LFM Open Licence: commercial rights end above **$10M revenue** | Qwen3.5, SmolVLM |
| **Llama 3.2 11B/90B Vision** | Llama 3.2 Community: multimodal rights not granted to EU-domiciled developers; 700M MAU clause | Qwen3.5, Gemma 4 |
| **Surya OCR** weights | Modified RAIL-M: free only under a $5M funding/revenue cap | PP-OCRv5, Tesseract |
| **Ultralytics YOLO / YOLOE** (AGPL-3.0), **YOLO-World** (GPL-3.0) | Copyleft (network AGPL) | RF-DETR N–L (Apache), Grounding DINO, OWLv2 |
| **potrace, autotrace; imgly background-removal-js (AGPL); inpaint-web (GPL); Fooocus (GPL)** | Copyleft code | vtracer (MIT); write our own pipeline around the permissive weights |

### Grey zone: allowed only after a recorded decision

- **Permissive weights trained on non-commercial datasets.**
  - DIS5K: BiRefNet general/lite/HR/dynamic, BEN2, IS-Net, possibly HQ-SAM.
  - FFHQ: GFPGAN, RestoreFormer++.
  - Adobe Composition-1k: ViTMatte.
  - FiveK and PPR10K: all auto-tone checkpoints.
  - Places2 and iHarmony4: LaMa, PCT-Net.
- **Use-based and attribution licences.** Ship the licence and pass the restrictions on.
  - OpenRAIL-M and OpenRAIL++: SD 1.5, SDXL, SD 2.1, IC-Light v1.
  - Gemma Terms: carried by Sana, Lumina-Image 2.0 and PRX through their text encoders.
  - DINOv3 Licence: withoutbg open weights, EoMT-DINOv3.
  - CC-BY: needs an attribution screen.
- **Licence conflicts to resolve by reading.**
  - IC-Light v1 weights.
  - MediaPipe Apache card vs the CC BY docs footer.
  - Bringing Old Photos Back to Life ("MIT" vs "academic use only").
  - skyseg provenance.
  - onnx-community DA3-large base checkpoint.
  - EfficientSAM3 (distilled from SAM 3).
  - SAM 3 / 3.1 "SAM License".
- **GPL-3.0 weights:** RawNIND. This is a project-licence decision.
- **Runtime, not a model: `ort-web` (pyke) phones home by default.**
  - It sends a `navigator.sendBeacon` to `signal.pyke.io` on session init. Use `EnvironmentBuilder::with_telemetry(false)`.
  - It fetches the ONNX Runtime build from `cdn.pyke.io`. Use a self-hosted `Dist`.
  - Both defaults contradict "no cloud calls" (read from `backends/web` in pykeio/ort, v2.0.0-rc.13).

---

## 6. Hardware reality

Everything below is **(approx)** unless tagged [M] or [P]. Estimates scale published or measured anchors by compute:
- LaMa at 512² is ≤200 ms on M1 WebGPU [P].
- GFPGAN is 0.27 s per 512² face on WebGPU [M].
- WebLLM keeps ~71–80% of native decode speed [P].
- transformers.js Qwen3-4B runs at 22.3 tok/s on an M2 Pro [P].
- Real-ESRGAN x4plus costs ~18M multiply-adds per input pixel; x4v3 costs ~1.2M.

Reference machines:
- **M3 MacBook Air, 16 GB.** 10-core GPU, fanless, so sustained loads throttle.
- **Windows laptop with an iGPU.** Iris Xe baseline. Lunar Lake Arc 140V or Radeon 780M are about 1.5–2× faster.
- **Desktop RTX 4070, 12 GB.**

Latency by workload (browser = WebGPU, native = the local backend):

| Workload | M3 Air: browser | M3 Air: native (CoreML/Metal) | iGPU laptop: browser | iGPU laptop: native (DirectML/OpenVINO) | RTX 4070: browser (D3D12) | RTX 4070: native (CUDA fp16) |
|---|---|---|---|---|---|---|
| **SAM encoder**, EdgeTAM, 1024² | 60–150 ms | 20–50 ms | 150–400 ms | 80–200 ms | 20–50 ms | 5–15 ms |
| **SAM encoder**, SAM 2.1-small, 1024² | 0.4–1.0 s | 0.15–0.3 s | 1–2.5 s | 0.5–1.2 s | 80–200 ms | 20–40 ms |
| SAM decoder, per click | 10–30 ms | <10 ms | 20–60 ms | 10–30 ms | <10 ms | <5 ms |
| **LaMa**, 512² | 150–300 ms | 80–150 ms | 0.4–1.0 s (wasm CPU 3–8 s) | 0.3–0.6 s | 40–80 ms | 15–30 ms |
| MI-GAN, 512² | 30–80 ms | 15–40 ms | 80–200 ms | 50–120 ms | <20 ms | <10 ms |
| **Real-ESRGAN x4v3**, 3 MP → 12 MP | 4–10 s | 2–5 s | 10–25 s | 6–15 s | 1–3 s | <1–2 s |
| **Real-ESRGAN x4plus**, 3 MP → 12 MP | 25–60 s | 12–30 s | 1.5–4 min | 1–2.5 min | 8–20 s | 4–8 s |
| **Real-ESRGAN x4plus**, 12 MP → 192 MP | **Not feasible** (tab memory; openpixels caps output at ~40 MP [M]) | 1–2.5 min, streamed to disk | Not feasible | 6–15 min | Not feasible | 15–35 s |
| **SDXL-class inpaint** 1024², 30 steps | Not feasible (UNet + encoders ≈ 7 GB) | 45–90 s | Not feasible | Arc 140V 1.5–4 min; Iris Xe impractical | Not feasible | 5–8 s |
| SDXL inpaint + Lightning, 8 steps | Not feasible | 15–30 s | Not feasible | 30–90 s (Arc 140V) | Not feasible | 1.5–3 s |
| FLUX.2 [klein] 4B, 4 steps, 1024² | Not feasible | 20–60 s, GGUF Q8 (unverified) | Not feasible | Impractical | Not feasible | 2–6 s, fp8 (unverified) |
| Qwen-Image-Edit-2511 | Not feasible | Does not fit 16 GB usefully | Not feasible | Not feasible | Not feasible | 0.5–2 min with int4, CPU offload and Lightning (low confidence) |
| **3–4B VLM q4**, decode | 12–20 tok/s | 25–40 tok/s | 6–12 tok/s | 10–20 tok/s | 40–70 tok/s | 90–130 tok/s |
| 3–4B VLM, image prefill (~1k tokens) | 3–15 s (Qwen3.5 TTFT issue: 16 s on an M2 Pro [P]) | 1.5–4 s | 8–30 s | 4–10 s | 1–3 s | <1 s |

What each machine can realistically run:
- **M3 Air 16 GB.**
  - Browser: all of Tier 1, all Tier 2 vision packs, and a 1.7B–4B planner.
  - Native: klein 4B, SDXL + Lightning, IC-Light, AdcSR, a 4B VLM.
  - Not Qwen-Image-Edit.
  - Watch unified-memory pressure when a diffusion model and the VLM are loaded together: unload one.
- **Windows iGPU laptop.**
  - Browser: Tier 1 comfortably; Tier 2 packs with patience (text-select and BiRefNet take seconds); the 1.7B planner.
  - Native Tier 3: Lunar Lake / 780M class only, and only for SDXL-Lightning-class models. Iris Xe machines should not be offered Tier 3.
- **RTX 4070 desktop.**
  - Everything in Tiers 1–3 except the workstation pack, which needs CPU offload and is slow.
  - Browser performance is fine too, but the native backend is 2–4× faster.

**Browser-specific ceilings.**
- WebGPU buffer limits and tab memory make weight sets above ~2 GB unreliable. wllama caps single files at 2 GB.
- WebGPU ships in Chrome/Edge, Safari 26, and Firefox 141 (Windows) / 145 (Apple silicon macOS). **Firefox on Linux does not have it yet.**
- **iOS 17 Safari cannot create onnxruntime-web sessions at all** [M]. iOS 26 works, but on the CPU path in testing [M].

---

## 7. Instruction editing: planner over tools, not a pixel generator

**Recommendation (three sentences).** A small local LLM or VLM acts as a **planner** that turns the user's sentence plus measured image facts into a **typed, schema-validated list of tool calls** against our deterministic engine. Each step is an ordinary undoable history entry and a C2PA action. Pixel-generating editors (FLUX.2 [klein], Qwen-Image-Edit) are exposed as **one tool among many** (`generative_fill(region, prompt)`), always confined to a mask and composited, never allowed to repaint the whole photo.

### Why a planner beats end-to-end editing here

- **Reversibility and precision.** "Warmer by a bit" becomes `white_balance{temp:+400K}`, a slider the user can see and adjust. An end-to-end model returns new pixels with no handle and silent global drift, including in untouched regions and faces.
- **Hardware.** A 1.7B–4B planner runs in the browser (§6). No commercially clean end-to-end editor does: klein 4B needs ~5–8 GB native, Qwen-Image-Edit ~13–18 GB.
- **Quality ceiling.** Local end-to-end editors trail GPT Image 2 and Nano Banana 2 badly. Deterministic tools *are* the quality ceiling for tone and colour, and they match Lightroom because they are the same kind of operation.
- **Licences and provenance.** Most steps are non-generative (`algorithmicallyEnhanced`), and only the masked generative step is labelled `compositeWithTrainedAlgorithmicMedia`.
- **Evidence it works.** JarvisArt (NeurIPS 2025, non-commercial weights) and MonetGPT show that MLLM agents orchestrating 200+ Lightroom-style operations are a viable research line. We copy the architecture, not the weights.

### Shape

```
user text ─┐
image stats (Rust: exposure histogram, clipping, WB cast, noise σ, blur, tilt, faces, detected objects+boxes) ─┐
current selection / layers ─┐
                            ▼
      planner LLM (grammar-constrained JSON; schema generated from Rust types)
                            ▼
  plan: [ {tool:"select", query:"person", pick:"leftmost"},
          {tool:"remove", mask:"$1", engine:"auto"},        // MI-GAN / LaMa / klein by mask area + tier
          {tool:"white_balance", temp_k:+400, tint:0},
          {tool:"vibrance", amount:+10} ]
                            ▼
  Rust validator (types, ranges, references) → preview → user confirms/edits the chips → executor
                            ▼
  each step = history entry (undo) + c2pa.actions entry
```

### Rules

- **Grounding stays deterministic.** The LLM never draws masks. It emits a query (`"person"`) plus a relational selector (`leftmost`, `largest`, `nearest to centre`, `above the horizon`). Rust resolves the selector over Grounding DINO / RF-DETR boxes, then SAM produces the mask. Small LLMs are weak at spatial relations, and this moves them out of the model.
- **Constrained decoding.**
  - Browser: WebLLM JSON mode (XGrammar-based) or wllama GBNF.
  - Native: llama.cpp GBNF / JSON-schema.
  - Generate the grammar from the same Rust structs (`serde` + `schemars`) that the executor deserialises, so there is one source of truth.
  - WebLLM's native `tools` support is "preliminary", so don't depend on it.
- **Tool granularity.** Plan in 20–40 coarse, well-named tools with bounded numeric ranges. Expose Lightroom-style parameters rather than 200 fine ones: the 1.7B model's BFCL score (28%) says a small, sharp schema matters more than the model.
- **Lite fallback with no model.**
  - A rule-based parser covers the top ~50 phrasings ("brighter", "warmer", "remove background", "straighten") in the eight UI languages.
  - The LLM loads only on first use of free text.
- **Escalation.** When a step needs content that doesn't exist ("add clouds", "replace the sky with sunset") and Tier 3 is absent, the planner must return an explicit `unsupported_here{needs:"generative pack"}` instead of approximating.
- **Our own evaluation set.** Build 300 commands × 50 photos with expected plans and score exact-match on tools, ±tolerance on values, and correct target object. Don't trust BFCL for our domain.
- **End-to-end editors, where allowed:** Pro-only, Tier 3, as `generative_edit(region|whole, prompt)`. The output is diffed against the input, and changes outside the mask are rejected or feathered back.

---

## 8. What the Rust side should own

**Where the seam falls.**
- In the browser, JavaScript (or `ort-web` from Rust, with telemetry disabled and a self-hosted `Dist`) owns the onnxruntime-web session.
- Natively, `ort` with CUDA, CoreML or DirectML EPs owns ONNX. llama.cpp or candle own LLMs. A diffusion runtime owns Tier 3 (stable-diffusion.cpp or a candle port; klein support is unverified).
- **Rust owns everything around the forward pass, so there is one implementation tested by `cargo test`.** This is the openpixels architecture, proven [M].

1. **Model registry and manifest.**
   - Fields per model:
     - id and version
     - URL(s) on our origin
     - sha256 and byte size
     - licence SPDX or custom id, attribution text, grey-zone flag
     - input spec (shape, fixed or dynamic, normalisation constants, colour space) and output spec
     - precision
     - allowed backends and known-bad backends (e.g. `gfpgan_fp16: webgpu=broken`)
     - minimum tier and RAM/VRAM estimate
   - One manifest drives the Models page, the attribution screen and the downloader.
2. **Download, cache and verification.**
   - Ranged, resumable downloads; streaming sha256 while writing.
   - Write to `.part` and rename atomically only after the hash passes *and* the session loads. openpixels learned that a killed conversion leaves a plausible broken file [M].
   - Storage: OPFS / Cache Storage in the browser, the app data directory natively.
   - Report the storage quota; eviction goes through a user-visible Models page.
   - No third-party origins at run time.
3. **Backend selection, fallbacks and the pixel-parity canary.**
   - Order: WebGPU → WASM-SIMD with threads (needs COOP/COEP) → WASM single-thread. Natively: CUDA/CoreML/DirectML → CPU.
   - **On first session per (model, backend, browser version), run a bundled golden input and compare with a stored output (tolerance on mean and max error, plus a "responds to input" check such as image vs negative).** On failure, mark the backend bad for that model and fall back.
   - This catches the two silent WebGPU failures already hit: GFPGAN fp16 returning a constant, and the MODNet matte with holes [M].
   - Record the outcome in a `#/diagnostics/run` page, the openphotoid pattern [M].
4. **Tiling and shapes.**
   - Plan tiles, then pad **every tile to one constant square** with edge replication. WebGPU buffer reuse breaks on shape changes; also set `enableMemPattern:false` and `freeDimensionOverrides` [M].
   - Merge the overlap band with linear seam ramps.
   - Accumulate in 16-bit fixed point to halve peak memory [M].
   - Cap working size against memory, and report the cap to the UI.
   - Pull the last row and column back to full tiles.
   - Stream very large outputs to disk natively.
5. **Pre- and post-processing.**
   - Decode with EXIF orientation; sRGB ↔ linear; ICC via `moxcms` (see research 03).
   - Letterbox / resize / normalise per the manifest.
   - LAB conversions and the gamut-preserving chroma merge for colourisation [M].
   - Face alignment (FFHQ 5-point) and feathered paste-back [M].
   - SAM prompt coordinate transforms.
   - Landmark decode, and YuNet / RF-DETR / GDINO box decoding and NMS.
6. **Mask operations.**
   - Boolean ops, grow / shrink / feather, connected components, fill holes.
   - Low-resolution mask upsampling with a guided filter (constant radius 2 [M]).
   - Closed-form matting restricted to the uncertainty band, plus foreground-colour estimation (pymatting port).
   - Polygon rasterisation from landmarks.
   - Sky and people mask fusion, and depth-edge alignment to the matte.
7. **Inpainting orchestration.**
   - Crop the mask bbox with a context margin and choose an engine by mask area and tier (MI-GAN < LaMa < diffusion).
   - Resize to the model's fixed size, then paste back with a feathered or Poisson blend and grain matching.
   - Multi-pass for large masks.
   - Choose the outpaint canvas.
8. **Classical algorithms that replace models.**
   - Auto levels / exposure / white balance; LUT fitting (Reinhard, MKL, sliced OT) and application.
   - Frequency-separation skin smoothing; DoG blemish detection; sensor-dust detection.
   - Ridge / Hough wire tracing; MLS / mesh warps for liquify and body.
   - Depth-aware bokeh gather; Lambertian relight from depth and normals.
   - pHash; Laplacian blur; noise estimation (Immerkær) [M]; PSNR/SSIM "faithfulness" metrics [M].
   - vtracer integration.
9. **Planner infrastructure.**
   - Tool schema types; the JSON Schema → GBNF generator.
   - Plan validator (ranges, references) and executor with transactional undo.
   - The image-facts extractor fed to the LLM.
   - Relational selector resolution over boxes.
   - The rule-based Lite parser.
10. **Provenance.** `c2pa` (`rust_native_crypto` on wasm32) builds an `actions` assertion per history step, with `digitalSourceType`, software agent and model id+hash, and signs the export. Signing uses a locally generated CA plus leaf certificate (shown as an unknown source by verifiers until a trust-list certificate exists).
11. **Job control.** One worker, sessions reused, cooperative cancellation between tiles, progress events. Unload models under memory pressure (never keep a diffusion model and the VLM resident together on 16 GB).

---

## 9. Reusable as-is from openpixels and openphotoid

| Asset | Where | Licence | Status | Use in OpenPhotoshop |
|---|---|---|---|---|
| `realesr_general_x4v3.onnx`, `_wdn_`, `_dn50` (4.9 MB each) | openpixels `MODELS.md` | BSD-3 | Shipped, sha256-pinned, WebGPU-proven [M] | Tier 1 upscale and denoise |
| `realesrgan_x4plus.onnx` (67 MB), `realesrgan_x4plus_anime_6b.onnx` (18 MB) | openpixels | BSD-3 | Shipped [M] | Tier 2 upscale; anime / illustration |
| `gfpgan_1.4.onnx` (340 MB, **fp32 only**) | openpixels | Apache-2.0 (FFHQ grey) | Shipped. 0.27 s per face on WebGPU; the runtime guard catches degenerate output [M] | Tier 2 face restore |
| `deoldify_artistic.onnx` (255 MB, fp32) | openpixels | MIT | Shipped; runs at 256² [M] | Tier 2 colourise |
| `u2netp.onnx` (4.6 MB) | openpixels | Apache-2.0 | Shipped [M] | Fallback cutout; superseded by MODNet / BiRefNet |
| `face_detection_yunet_2023mar.onnx` (0.23 MB) | Both | MIT | Shipped; 61 ms detect [M] | Tier 1 face detection |
| `modnet_photographic_portrait_matting.onnx` (24.7 MB) + tract variant | openphotoid | Apache-2.0 | Shipped. CPU 231–316 ms at 512² on a VM [M]; **WebGPU matte wrong, so GPU is opt-in** [M]; the tract variant runs in pure Rust on mobile | Tier 1 portrait subject |
| `birefnet_lite.onnx` (224 MB) + Rust wrapper | openphotoid `frame-matting` | MIT (**DIS5K grey**) | Code shipped; **inference never verified** (VM OOM) [M] | Tier 2 after a real-hardware benchmark and the legal call |
| `pixels-core`: tile plan and merge, metrics, unsharp, LAB, matte compositing, face paste-back | openpixels crate | MIT / Apache | 37 tests [M] | Base of §8 items 4–5 |
| `frame-matting` guided filter (radius 2), foreground estimation; `frame-face`; `frame-retouch` (`skin.rs` surface blur, capped strength; `enhance.rs` non-generative sharpening) | openphotoid crates | project licence | Validated on real photos [M] | §8 item 6; Tier 1 skin smoothing |
| `frame-engine` (ort + `tract-backend`) | openphotoid | — | Native and mobile runtime seam [M] | Native backend skeleton |
| `fetch-models.sh`, `export-models.py` (PyTorch → ONNX with a parity check), `MODELS.md` format | openpixels | — | [M] | Model pipeline and manifest |
| `ort.js` session options: `enableMemPattern:false`, `freeDimensionOverrides`, `logSeverityLevel:3`; constant-square tiling | openpixels `apps/web/src/lib/engine` | — | Fixes three WebGPU and load-time bugs [M] | Copy into the browser engine |
| `#/diagnostics/run` route pattern | openphotoid web app | — | Found the iOS 17 failure [M] | Canary and diagnostics page |

Sibling `package.json` files still declare `onnxruntime-web ^1.20.1`, while later work was measured on 1.26–1.29 [M]. Pin one version for OpenPhotoshop and re-run the canaries on upgrade.

---

## 10. Verification backlog before any of this ships

1. Read the full **SAM License** (SAM 3 / 3.1) and the **IC-Light v1 weights** licence. Settle the **MediaPipe Apache vs CC BY** question.
2. Make the legal call on **DIS5K-trained weights** (BiRefNet general/lite, BEN2) and **FFHQ-trained** GFPGAN. Until then, BiRefNet-portrait is the clean HQ cutout.
3. Benchmark on the three reference machines: EdgeTAM, SAM 2.1-small, MI-GAN, LaMa, NAFNet, BiRefNet_lite, Grounding DINO tiny and DA-V2-Small. Run each in browser fp32 / fp16, **with pixel-parity canaries**. Every browser latency in this doc except LaMa, GFPGAN and the LLM figures is an estimate.
4. Check FLUX.2 [klein] 4B on stable-diffusion.cpp / candle / ComfyUI: masked inpainting quality, 4070 and M3 latency, and whether its encoder equals stock Qwen3-4B.
5. Confirm OmniGen2's Qwen2.5-VL-3B licence inheritance and Step1X-Edit's FLUX lineage.
6. Build the 300-command planner eval (§7) and pick between Qwen3-1.7B, Qwen3.5-2B and Qwen3-4B-2507 on it. Re-check the Qwen3.5 TTFT issue in transformers.js.
7. Track WireSeg-32K (wires), Z-Image-Edit (Apache, unreleased) and any permissive sky or reflection model.
8. Decide whether retraining a 3D-LUT auto-tone model on licensed data is worth it. The data is the cost.

---

## Sources

**Sibling projects (read-only, measured):**
- `openpixels/MODELS.md`, `PLAN.md`, `docs/architecture.md`, `docs/testing.md`
- `openphotoid/MODELS.md`, `docs/research/02-models-licenses.md`, `docs/research/05-m0-results.md`, `crates/frame-retouch/src/skin.rs`, `crates/frame-engine/Cargo.toml`

**Incumbents:**
- Photoshop: https://helpx.adobe.com/photoshop/desktop/generative-ai/generative-ai-features-overview.html · https://www.photoshopnews.com/2026/04/04/whats-new-photoshop-2026-ai-assistant-generative-updates · https://weandthecolor.com/ai-features-in-adobe-photoshop-that-actually-changed-how-i-work-a-designers-field-report/208072 · https://fstoppers.com/photoshop/black-cloud-hanging-over-photoshops-new-features-716251
- Lightroom: https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/whats-new.html · https://petapixel.com/2025/11/03/lightrooms-new-features-ai-culling-auto-dust-removal-color-variance-slider-and-more/
- Google: https://blog.google/products-and-platforms/products/photos/android-conversational-editing-google-photos/ · https://9to5google.com/2025/09/23/google-photos-help-me-edit/
- Samsung: https://www.techbuzz.ai/articles/samsung-galaxy-s26-ultra-adds-ai-photo-editing-features · https://www.promptquorum.com/local-llms/galaxy-s26-local-ai-on-device-2026
- Apple / Pixelmator: https://www.apple.com/newsroom/2026/01/introducing-apple-creator-studio-an-inspiring-collection-of-creative-apps/ · https://www.apple.com/pixelmator-pro/
- Luminar: https://skylum.com/luminar/skin-ai · https://skylum.com/luminar/generase
- Topaz: https://docs.topazlabs.com/topaz-photo/release-summary · https://www.topazlabs.com/topaz-photo
- Evoto: https://support.evoto.ai/understanding-evoto-credits-and-pricing/ · https://www.evoto.ai/
- Photoroom: https://www.photoroom.com/inside-photoroom/new-in-photoroom-h1-2026-product-recap · https://www.photoroom.com/blog/prx-pixel-photoroom-s-open-source-pixel-space-image-model · https://huggingface.co/Photoroom/prx-1024-t2i-beta
- Cloud image models: https://fal.ai/learn/tools/gpt-image-2-vs-nano-banana-2 · https://www.cometapi.com/gpt-image-2-vs-nano-banana-2/

**Selection and matting:**
- SAM family: https://huggingface.co/facebook/sam3 · https://github.com/facebookresearch/sam3/blob/main/LICENSE · https://huggingface.co/facebook/sam3.1 · https://github.com/facebookresearch/sam2 · https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX · https://huggingface.co/onnx-community/sam2.1-hiera-base-plus-ONNX · https://huggingface.co/onnx-community/sam3-tracker-ONNX · https://huggingface.co/danilobukvic/sam3-text-onnx · https://github.com/facebookresearch/EdgeTAM · https://huggingface.co/onnx-community/EdgeTAM-ONNX · https://github.com/SimonZeng7108/efficientsam3 · https://github.com/ChaoningZhang/MobileSAM · https://github.com/chongzhou96/EdgeSAM/blob/master/LICENSE · https://github.com/SysCV/sam-hq · https://huggingface.co/Xenova/slimsam-77-uniform · https://github.com/mit-han-lab/efficientvit
- Browser demos: https://github.com/lucasgelfond/webgpu-sam2 · https://huggingface.co/spaces/webml-community/segment-anything-webgpu
- Open-vocabulary detection: https://huggingface.co/onnx-community/grounding-dino-tiny-ONNX · https://huggingface.co/onnx-community/Florence-2-base-ft · https://huggingface.co/onnx-community/owlv2-base-patch16-ensemble-ONNX · https://github.com/roboflow/rf-detr · https://github.com/AILab-CVC/YOLO-World · https://github.com/THU-MIG/yoloe
- Cutout and matting: https://huggingface.co/ZhengPeng7/BiRefNet · https://huggingface.co/onnx-community/BiRefNet_lite-ONNX · https://raw.githubusercontent.com/xuebinqin/DIS/main/DIS5K-Dataset-Terms-of-Use.pdf · https://jizhizili.github.io/files/p3m_dataset_agreement/P3M-10k_Dataset_Release_Agreement.pdf · https://huggingface.co/briaai/RMBG-2.0 · https://huggingface.co/PramaLLC/BEN2 · https://github.com/ZHKKKe/MODNet · https://huggingface.co/Xenova/modnet · https://huggingface.co/hustvl/vitmatte-small-composition-1k · https://github.com/pq-yang/MatAnyone2 · https://github.com/withoutbg/withoutbg · https://github.com/facebookresearch/dinov3/blob/main/LICENSE.md
- Sky and parsing: https://github.com/NVlabs/SegFormer/blob/master/LICENSE · https://huggingface.co/JianyuanWang/skyseg · https://github.com/jiupinjia/SkyAR · https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf · https://huggingface.co/jonathandinu/face-parsing · https://github.com/switchablenorms/CelebAMask-HQ · https://github.com/facebookresearch/sapiens · https://github.com/facebookresearch/sapiens2 · https://github.com/ultralytics/ultralytics
- transformers.js: https://raw.githubusercontent.com/huggingface/transformers.js/main/README.md

**Removal, generative and compositing:**
- Inpainting: https://github.com/advimman/lama/blob/main/LICENSE · https://huggingface.co/Carve/LaMa-ONNX · https://huggingface.co/opencv/inpainting_lama · https://github.com/lexluthor0304/NegativeConverter/issues/163 · https://github.com/Picsart-AI-Research/MI-GAN · https://raw.githubusercontent.com/Picsart-AI-Research/MI-GAN/main/LICENSE-WEIGHTS · https://github.com/fenglinglwb/MAT · https://raw.githubusercontent.com/SHI-Labs/FcF-Inpainting/main/LICENSE · https://github.com/DQiaole/ZITS_inpainting · https://www.iopaint.com/models · https://github.com/lxfater/inpaint-web
- Wires, reflections, shadows: https://github.com/adobe-research/auto-wire-removal · https://arxiv.org/abs/2609.03102 · https://github.com/mingcv/DSRNet · https://github.com/lime-j/RDNet · https://github.com/GuoLanqing/ShadowFormer
- Stability: https://huggingface.co/stabilityai/sd-turbo/blob/main/LICENSE.md · https://huggingface.co/stabilityai/sdxl-turbo/blob/main/LICENSE.md · https://stability.ai/license
- Black Forest Labs: https://huggingface.co/black-forest-labs/FLUX.1-Kontext-dev/blob/main/LICENSE.md · https://huggingface.co/black-forest-labs/FLUX.2-klein-4B · https://huggingface.co/black-forest-labs/FLUX.2-klein-9B/resolve/main/LICENSE.md · https://bfl.ai/blog/flux2-klein-towards-interactive-visual-intelligence · https://huggingface.co/black-forest-labs/FLUX.2-dev
- Other generators and editors: https://github.com/QwenLM/Qwen-Image · https://huggingface.co/unsloth/Qwen-Image-Edit-2511-GGUF · https://huggingface.co/nunchaku-ai/nunchaku-qwen-image-edit-2509 · https://huggingface.co/Tongyi-MAI/Z-Image-Turbo · https://github.com/Tongyi-MAI/Z-Image/issues/169 · https://huggingface.co/meituan-longcat/LongCat-Image-Edit · https://github.com/VectorSpaceLab/OmniGen2 · https://github.com/stepfun-ai/Step1X-Edit · https://huggingface.co/HiDream-ai/HiDream-E1-1 · https://huggingface.co/zai-org/GLM-Image · https://huggingface.co/briaai/FIBO · https://huggingface.co/tencent/HunyuanImage-3.0/raw/main/LICENSE · https://github.com/River-Zhang/ICEdit · https://github.com/PKU-YuanGroup/UniWorld-V1 · https://github.com/NVlabs/Sana
- Local runtimes and browser diffusion: https://github.com/nunchaku-tech/nunchaku · https://github.com/leejet/stable-diffusion.cpp · https://opensource.microsoft.com/blog/2024/02/29/onnx-runtime-web-unleashes-generative-ai-in-the-browser-using-webgpu/ · https://github.com/microsoft/onnxruntime-inference-examples/tree/main/js/sd-turbo · https://github.com/mlc-ai/web-stable-diffusion
- Relight and harmonize: https://github.com/lllyasviel/IC-Light · https://github.com/lllyasviel/IC-Light/discussions/98 · https://github.com/gojasper/LBM · https://github.com/ZHKKKe/Harmonizer · https://github.com/rakutentech/PCT-Net-Image-Harmonization · https://github.com/WindVChen/INR-Harmonization · https://github.com/bcmi/Libcom · https://huggingface.co/xinsir/controlnet-union-sdxl-1.0

**Enhancement and restoration:**
- Upscaling: https://github.com/OpenModelDB/open-model-database · https://openmodeldb.info/models/4x-UltraSharp · https://openmodeldb.info/models/4x-NomosWebPhoto-RealPLKSR · https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE · https://github.com/hongyuanyu/SPAN · https://github.com/dslisleedh/PLKSR · https://github.com/zhengchen1999/DAT · https://github.com/XPixelGroup/HAT · https://huggingface.co/Xenova/swin2SR-classical-sr-x2-64 · https://github.com/Fanghua-Yu/SUPIR · https://github.com/IceClear/StableSR · https://github.com/zsyOAOA/InvSR · https://github.com/Guaishou74851/AdcSR · https://github.com/csslc/PiSA-SR · https://github.com/cswry/OSEDiff
- darktable AI: https://github.com/darktable-org/darktable-ai · https://www.darktable.org/2026/06/meet-darktable-5.6-ai-tools/
- Denoise and deblur: https://github.com/megvii-research/NAFNet · https://github.com/cszn/SCUNet · https://github.com/swz30/Restormer · https://github.com/swz30/MPRNet · https://github.com/kkkls/FFTformer · https://github.com/MegEngine/PMRID · https://discuss.pixls.us/t/neural-demosaicing-denoising-for-intel-and-amd/58586
- JPEG and low-light: https://github.com/jiaxi-jiang/FBCNN · https://github.com/caiyuanhao1998/Retinexformer · https://github.com/Fediory/HVI-CIDNet · https://github.com/Li-Chongyi/Zero-DCE
- Faces: https://github.com/TencentARC/GFPGAN · https://github.com/sczhou/CodeFormer · https://github.com/wzhouxiff/RestoreFormerPlusPlus · https://github.com/yangxy/GPEN · https://github.com/ohayonguy/PMRF
- Old photos and colourisation: https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life · https://github.com/piddnad/DDColor · https://huggingface.co/piddnad/DDColor-models · https://github.com/jantic/DeOldify · https://github.com/shuchenweng/CT2 · https://huggingface.co/facefusion/models-3.0.0

**Tone, depth and portrait:**
- Tone and white-balance datasets and models: https://data.csail.mit.edu/graphics/fivek/legal/LicenseAdobe.txt · https://github.com/csjliang/PPR10K · https://github.com/HuiZeng/Image-Adaptive-3DLUT · https://github.com/ImCharlesY/AdaInt · https://github.com/mv-lab/nilut · https://github.com/mahmoudnafifi/C5 · https://github.com/mahmoudnafifi/Deep_White_Balance · https://github.com/LYL1015/JarvisArt
- Look transfer: https://github.com/ZHKKKe/NeuralPreset · https://github.com/clovaai/WCT2 · https://github.com/Ry3nG/SA-LUT
- Depth and bokeh: https://github.com/DepthAnything/Depth-Anything-V2 · https://github.com/ByteDance-Seed/Depth-Anything-3 · https://huggingface.co/onnx-community/depth-anything-v2-small · https://huggingface.co/onnx-community/depth-anything-v3-small · https://huggingface.co/apple/DepthPro · https://huggingface.co/Ruicheng/moge-2-vitl-normal · https://github.com/lpiccinelli-eth/UniDepth · https://github.com/Tencent/DepthCrafter · https://github.com/JuewenPeng/BokehMe
- Normals and retouching: https://github.com/baegwangbin/DSINE · https://modelscope.cn/models/damo/cv_unet_skin-retouching
- Landmarks and pose: https://storage.googleapis.com/mediapipe-assets/Model%20Card%20MediaPipe%20Face%20Mesh%20V2.pdf · https://storage.googleapis.com/mediapipe-assets/Model%20Card%20BlazePose%20GHUM%203D.pdf · https://github.com/davisking/dlib-models · https://github.com/open-mmlab/mmpose/tree/main/projects/rtmpose · https://github.com/JianqiangRen/FlowBasedBodyReshaping · https://smpl.is.tue.mpg.de/modellicense.html

**Semantic, LLM, text and provenance:**
- Embeddings: https://huggingface.co/apple/MobileCLIP2-S0 · https://huggingface.co/jinaai/jina-clip-v2 · https://huggingface.co/google/siglip2-base-patch16-224 · https://huggingface.co/facebook/PE-Core-B16-224 · https://huggingface.co/Xenova/clip-vit-base-patch32
- Quality scoring: https://github.com/LAION-AI/aesthetic-predictor · https://github.com/chaofengc/IQA-PyTorch/blob/main/LICENSE · https://github.com/IceClear/CLIP-IQA
- Faces: https://github.com/deepinsight/insightface/tree/master/python-package · https://huggingface.co/immich-app/buffalo_l · https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface · https://github.com/otroshi/edgeface
- OCR: https://huggingface.co/PaddlePaddle/PaddleOCR-VL · https://github.com/gutenye/ocr · https://github.com/datalab-to/surya
- VLMs: https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct · https://huggingface.co/Qwen/Qwen2.5-VL-3B-Instruct/blob/main/LICENSE · https://huggingface.co/Qwen/Qwen3.5-2B · https://huggingface.co/onnx-community/Qwen3.5-0.8B-ONNX · https://unsloth.ai/docs/models/qwen3.5 · https://huggingface.co/google/gemma-4-E2B-it · https://huggingface.co/onnx-community/gemma-4-E2B-it-ONNX · https://ai.google.dev/gemma/terms · https://huggingface.co/moondream/moondream3-preview · https://www.liquid.ai/lfm-license · https://www.llama.com/llama3_2/use-policy/
- LLM runtimes and tool calling: https://gorilla.cs.berkeley.edu/leaderboard.html · https://huggingface.co/Salesforce/xLAM-2-1b-fc-r · https://raw.githubusercontent.com/mlc-ai/web-llm/main/src/config.ts · https://github.com/ngxson/wllama · https://arxiv.org/html/2412.15803v2 · https://github.com/huggingface/transformers.js/issues/1599
- Fonts and text: https://github.com/Storia-AI/font-classify · https://huggingface.co/gaborcselle/font-identifier · https://github.com/tyxsspa/AnyText2 · https://huggingface.co/Shakker-Labs/RepText · https://huggingface.co/GD-ML/FLUX-Text
- Vectorising: https://github.com/visioncortex/vtracer · https://huggingface.co/starvector/starvector-1b-im2svg · https://huggingface.co/OmniSVG/OmniSVG
- Content Credentials: https://crates.io/crates/c2pa · https://github.com/contentauth/c2pa-rs · https://github.com/contentauth/c2pa-rs/blob/main/docs/support-tiers.md · https://opensource.contentauthenticity.org/docs/conformance/trust-lists/ · https://spec.c2pa.org/specifications/specifications/2.4/guidance/Guidance.html · https://blog.google/security/pixel-android-trusted-images-c2pa-content-credentials/

**Runtimes and platforms:**
- ort: https://github.com/pykeio/ort (`backends/web`: `_telemetry.js`, `_loader.js`, `env.rs`, `api.rs`; `docs/content/backends/web.mdx`; releases v2.0.0-rc.11 to rc.13)
- WebGPU: https://github.com/gpuweb/gpuweb/wiki/Implementation-Status · https://web.dev/blog/webgpu-supported-major-browsers
- Mac benchmarks: https://www.heyuan110.com/posts/ai/2026-02-15-draw-things-ultimate-guide/ · https://rentamac.io/stable-diffusion-mac/
- NVIDIA benchmarks: https://localaimaster.com/gpu/best-ai-models-rtx-4070
