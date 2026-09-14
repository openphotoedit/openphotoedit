# Why OpenPhotoshop

> Working name. "Photoshop" is a registered Adobe trademark; see *Open decisions* in `PLAN.md` before anything public carries it.

## The problem

Photoshop users are frustrated in the same few ways:

- **They pay for AI that is capped and metered.** The single-app plan costs $22.99 a month and includes 25 generative credits, about two premium generations. One user wrote: *"Generative Fill has dropped so significantly. Nothing it generates is even worth keeping, yet credits are deducted"* ([Adobe Community, Oct 2025](https://community.adobe.com/questions-712/ai-and-generative-credits-just-keep-getting-worse-1182326)).
- **Cancelling is hard.** Another wrote: *"Their subscription model is terrible...not clear on when you can't cancel without a fee"* ([Trustpilot, Aug 2026](https://www.trustpilot.com/review/photoshop.com)). Adobe paid a $150M settlement over that fee in March 2026.

The free alternatives each trade something away:

- **Photopea** carries ads, is closed source, and charges credits for AI.
- **Affinity** needs a Canva login, and all of its AI is Canva Premium. One user: *"As long as I don't need to login to a Canva account to use Affinity, I'll be fine… If they start subscriptions… I'm out"* ([Threads](https://www.threads.com/@douglaspmarx/post/DQe4c-tEfEB/as-long-as-i-dont-need-to-login-to-a-canva-account-to-use-affinity-ill-be-fine-i)).
- **GIMP** has no web version, drops PSD layer styles and adjustment layers, and ships no AI.

## Who has it

1. **People who need a Photoshop occasionally.** A designer opening a client's PSD, a marketer cutting out a product, a student. Today they use Photopea with an ad blocker, or pay Adobe for one month. Photopea serves about 1M daily users and 17M+ monthly visits.
2. **Casual photo editors.** They want to remove a person, blur a background, fix a dark photo or get a file under 500 KB. Today they use phone editors, Canva or ad-funded web tools. Many of those upload the photo, cap free use or add a watermark.
3. **Working photographers** (weddings, portraits, events). They pay Lightroom or Capture One for develop and culling, and Evoto or Imagen per image for retouching. Capture One has raised prices 6% two years running, Lightroom went from $11.99 to $14.99, and Topaz went from $99 once to $199 a year.

## What exists today

The full enumeration is in `docs/feature-matrix.md` (361 consolidated capabilities) and `docs/research/01–04`. The rows that decide a switch:

| Function | Photoshop | Photopea | Affinity | GIMP 3.2 | Graphite | Us | Class | Note |
|---|---|---|---|---|---|---|---|---|
| Layers, masks, 27 blend modes | ✓ | ✓ | ✓ | ✓ | node-based | ✓ P2 | must | |
| Adjustment layers | ✓ | ✓ | ✓ | NDE filters | ✓ nodes | ✓ P2 | must | |
| PSD round-trip incl. styles, text, smart objects | ✓ | ✓ best | partial | ✗ styles/adjustments | ✗ roadmap "LTS" | ✓ P2–P4 | must | **the moat** |
| Runs in a browser tab, no install | web (sub) | ✓ | ✗ | ✗ | ✓ | ✓ | must | |
| No account, no ads | ✗ | ✗ ads | ✗ Canva login | ✓ | ✓ | ✓ | must | |
| Open source | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | edge | house style, not the wedge |
| Select subject / click-select | cloud option | credits | Premium | ✗ | ✗ | ✓ local T1 | must | EdgeTAM, MODNet |
| Remove object | cloud → device (27.7) | credits | Premium | GPL plugin | ✗ | ✓ local T1 | must | MI-GAN, LaMa |
| Background removal | ✓ | credits | Premium | ✗ | ✗ | ✓ local T1 | must | |
| Faithful upscale / AI denoise | credits / ACR local | credits | Premium | ✗ | ✗ | ✓ local T1–T2 | must | Real-ESRGAN, NAFNet |
| Generative fill / expand | credits | credits | Premium | ✗ | ✗ | ✓ native T3 | edge | needs 12 GB+ GPU |
| Prompt edits as visible sliders | AI Assistant (credits) | ✗ | ✗ | ✗ | ✗ | ✓ P1 rules, P3 LLM | edge→must | Lightroom meters this |
| RAW develop into layers | ACR (separate) | basic | Develop persona | hand-off | ✗ roadmap | ✓ P3 | must (photographers) | |
| AI culling | Lightroom credits | ✗ | ✗ | ✗ | ✗ | ✓ P3 local | must (photographers) | |
| Batch face retouch | ✗ (Evoto per-export) | ✗ | ✗ | ✗ | ✗ | ✓ P4 local | must (portrait) | |
| Cloud docs, Libraries, Stock, Invite to Edit | ✓ | — | — | — | — | ✗ | bloat | |
| 3D, video timeline | removed / legacy | — | — | — | — | ✗ | bloat | |

## The gap

**No open-source project offers a Photoshop-grade layered editor with faithful PSD round-trip and credit-free AI in a browser tab.**

- **Photopea** proves the demand, but it is one developer, closed source, and funded 90% by ads. Its most-reacted GitHub issue is "Make Photopea Self Hosted?" ([#4870](https://github.com/photopea/photopea/issues/4870)).
- **Affinity** is free, but its business model is selling Canva's cloud AI. It structurally cannot give AI away or drop the login.
- **Adobe** sells credits and cannot make generation free or offline.
- **GIMP** is C/GTK on the desktop and has deliberately not prioritised PSD styles or AI.
- **Graphite** shares our stack (Rust, wasm, wgpu) but is heading towards node-based vector work. Raster selection, RAW and PSD are 6–24+ months out on its roadmap.

**The structural weakness is the meter.** Every incumbent that computes AI in its cloud has to charge for it. Removal, masking, cutout, upscale, denoise, culling and retouch all run on the user's own GPU at no cost to us, so they can be unlimited forever. That is Photopea's premium tier, Affinity's Premium tier, Lightroom's culling credits and Evoto's per-export fee, all given away.

## Demand evidence

| Signal | Number | Source |
|---|---|---|
| Free browser Photoshop usage | ~1M DAU, 17M+ monthly visits, ~$3M/yr revenue | research 02 §1.5, 03 §A |
| Self-host / open request | "Make Photopea Self Hosted?", Photopea's most-reacted issue (138) | GitHub #4870 |
| Free pro-editor appetite | Affinity: 1M+ sign-ups in its first four days as a free app | research 02 §1.3 |
| Paid market | Photoshop $22.99–$69.99/mo; Topaz $199/yr; Capture One ~$18–48/mo; Evoto per export | research 01, 02 |
| Complaint density | Credits, quality regressions, cancellation fees, lag; Trustpilot 1.9/5 for photoshop.com | research 01 |
| Open alternatives' pull | Upscayl 49k stars, IOPaint 23k, rembg 25k, RapidRAW 10k in under two years | research 03 |

## Why us

Most of the work is not the part a weekend project can reach:

- **PSD fidelity is corpus-driven drudgery.** A reader and writer must survive real Photoshop 2020–2026 files, including a flattened composite our renderer draws to match Photoshop (research 03 §C.3).
- **A tiled, GPU-composited engine** edits a 20-layer 16-bit document inside the browser's 4 GB memory limit. A native Rust server lifts the limit for the same UI.
- **The suite has already paid for the AI integration lessons.**
  - OpenPixels ships tiled Real-ESRGAN, GFPGAN and DeOldify on WebGPU, with the fixes for three onnxruntime-web failures.
  - OpenPhotoId ships MODNet and YuNet with native `ort` and a diagnostics page that found the iOS 17 failure.
  - OpenCapture ships the crop, annotate and redact interaction that Lite starts from.

"Local and private" and "written in Rust" are not the reason; the memory *Local and private is not a wedge* applies. The reason is **PSD round-trip plus unmetered AI in a tab**.

**Lite on its own would fail the supply gate.** Pixlr, Canva and a dozen SEO tools already offer simple web editing. Lite earns its place three ways:
- as the on-ramp to the same engine;
- through unlimited local remove, cutout and upscale, which those tools cap;
- by opening in Pro without losing a single step.

## What we will not build

These are the bloat rows in the matrix, left out on purpose:

- **Adobe ecosystem:** cloud documents and version-history sync, Creative Cloud Libraries, Adobe Stock, font activation, Invite to Edit and comments.
- **Discontinued or niche Photoshop features:** 3D (Adobe removed it), the video timeline, and `.8bf` plugin hosting.
- **Generative novelties:** body reshaping, makeup, spatial reframing / rotate object, photo-to-video, custom model training, template and brand-kit libraries.
- **Lightroom extras:** map view, books, slideshows, web galleries.
- **Monetisation:** accounts, credits or any paid gate on a function. Support features may come later, but never as a lock on a capability.

## Scorecard

| Gate | Result |
|---|---|
| Does not duplicate a suite territory | **Pass, with a boundary.** OpenCapture stays the capture tool and OpenPixels the one-job enhancer. OpenPhotoshop is the layered editor, and Lite reuses both rather than competing with them |
| No server, no account, no API key | **Pass.** The browser build is fully local; the optional native Rust server runs on the user's own machine |
| Core is a pure function over a file | **Pass.** A document is operations applied to tiles; codecs, PSD, compositor and AI pre- and post-processing are I/O-free crates |
| Payoff in under 30 seconds | **Pass.** Drop a photo, tap the person, press Erase. Or drop a PSD and see its layers |
| One-screenshot demo | **Pass.** A PSD's layer panel beside a removed object, with "unlimited, no credits" |
| Structural incumbent weakness | **Pass.** The cloud-AI meter (Adobe, Canva/Affinity, Photopea premium, Evoto) and a closed, ad-funded, one-person Photopea |

| Dimension | Score | Weight | Points |
|---|---|---|---|
| Demand | 5 | ×2 | 10 |
| Gap | 3 | ×2 | 6 |
| Fit (Rust/wasm local core) | 5 | ×1 | 5 |
| Demo | 4 | ×1 | 4 |
| **Total** | | | **25 / 30** |

The gap scores 3, not 5: free Photopea and free Affinity exist, and Graphite could close in.

## Verdict

**Build it — but as a sequence, not a clone.** The order is:

1. The engine.
2. Lite, a shippable, unlimited local AI photo editor built on OpenCapture's interaction.
3. Pro Core, an "open Photopea" that round-trips PSD.
4. Photographer, Depth and Generative phases, in the order `PLAN.md` gives.

"All of Photoshop" is a multi-year roadmap of 361 capabilities. It is not a first release.
