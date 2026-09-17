# Models

Every AI feature in OpenPhotoshop runs on the user's device. The weights
below are the only ones the app loads. Each is served from the app's own
origin (`models/<file>`, or the native server's `/models/<file>`), cached in
the browser's Cache Storage after first use, and pinned by size and sha256 —
the browser verifies the hash with WebCrypto before it keeps a file. Nothing
is fetched from a third party at run time, and that includes the ONNX
runtime's own `.wasm` files (`ort/`).

The rule for adding a model: its **weights** must permit commercial use and
redistribution (the app is AGPL-3.0-or-later, and forks may be commercial).
Check `docs/research/04-local-ai.md` §5 first. Non-commercial, research-only
and revenue-capped weights are excluded, and so is anything built on them.

## Getting the files

```sh
scripts/fetch-models.sh            # all models, then link them into apps/web/public/models
scripts/fetch-models.sh --tier1    # only the small default models (~120 MB)
scripts/fetch-models.sh --no-link  # fetch and verify only
```

For each file the script takes, in order: the copy already in `.vendor/models`
if its hash matches; a sibling checkout's `.vendor/models` (`../openpixels`,
`../openphotoid`) if its hash matches (an APFS clone, instant); otherwise a
download to `<file>.part`, verified, then renamed. A truncated or substituted
download never looks cached.

It then:

- creates **per-file symlinks** in `apps/web/public/models/` (a real,
  gitignored directory; a symlinked directory would show up in `git status`).
  Vite's dev server follows them, and `vite build` copies the real bytes into
  `dist/models/` — about 1.2 GB with every model linked. Use `--tier1` or
  remove links you do not need if build size matters.
- copies onnxruntime-web's runtime (`ort-wasm-simd-threaded{,.asyncify}.{mjs,wasm}`,
  ~41 MB) from `apps/web/node_modules/onnxruntime-web/dist` into
  `apps/web/public/ort/` (gitignored). The WebGPU build uses the asyncify
  runtime; the CPU build the plain one.

**Real-ESRGAN** publishes PyTorch weights only. The ONNX files are exported
by `openpixels/scripts/export-models.py` (needs torch), which checks each
conversion against the PyTorch forward pass; the export is deterministic, so
the pins below hold. Without a sibling copy, run that script and re-run
`fetch-models.sh`.

`apps/web/public/models.json` is the same catalogue in the native server's
manifest format (`crates/editor-server/src/models.rs`), one entry per file,
generated from `apps/web/src/lib/models.ts` (the command is in that file's
`serverManifest` comment). Real-ESRGAN entries carry the PyTorch source URL
and `"exported": true`; the server cannot download those itself.

## The catalogue

Tier 1 is small and fetched on first use of a feature. Tier 2 is a larger
optional download.

| id | File | Bytes | Tier | Licence | Used for |
|---|---|---:|---|---|---|
| `migan` | `migan_pipeline_v2.onnx` | 28,079,181 | 1 | MIT (code and weights) | Remove, small areas |
| `lama` | `lama_fp32.onnx` | 208,044,816 | 2 | Apache-2.0 | Remove, large areas ("best"; "auto" above 6% hole) |
| `modnet` | `modnet_photographic_portrait_matting.onnx` | 25,888,640 | 1 | Apache-2.0 | Subject in portraits; rough people matte |
| `u2netp` | `u2netp.onnx` | 4,574,861 | 1 | Apache-2.0 | Rough salient-object matte; scene analysis |
| `edgetam-encoder` | `edgetam/vision_encoder.onnx` + `.onnx_data` | 192,225 + 19,532,576 | 1 | Apache-2.0 | Click-to-select; subject refinement |
| `edgetam-decoder` | `edgetam/prompt_encoder_mask_decoder.onnx` + `.onnx_data` | 213,114 + 20,958,208 | 1 | Apache-2.0 | Click-to-select; subject refinement |
| `yunet` | `face_detection_yunet_2023mar.onnx` | 232,589 | 1 | MIT | Faces: detection, restore, red eye, scene |
| `realesr-x4v3` | `realesr_general_x4v3.onnx` | 4,866,422 | 1 | BSD-3-Clause | Upscale (fast); JPEG clean-up; light denoise |
| `realesr-x4v3-dn50` | `realesr_general_x4v3_dn50.onnx` | 4,866,422 | 1 | BSD-3-Clause | Denoise (medium) |
| `realesr-x4v3-wdn` | `realesr_general_wdn_x4v3.onnx` | 4,866,422 | 1 | BSD-3-Clause | Denoise (strong) |
| `realesrgan-x4plus` | `realesrgan_x4plus.onnx` | 67,051,644 | 2 | BSD-3-Clause | Upscale (best) |
| `gfpgan` | `gfpgan_1.4.onnx` | 340,299,087 | 2 | Apache-2.0 (grey: FFHQ) | Face restoration |
| `deoldify` | `deoldify_artistic.onnx` | 255,044,725 | 2 | MIT | Colorize |

```
6f1f3530a1a2324b19752018ce756088b07973cda8d7d890034ace5c8a48c40b  migan_pipeline_v2.onnx
1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6  lama_fp32.onnx
07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9  modnet_photographic_portrait_matting.onnx
309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8  u2netp.onnx
ed068218eba96760fe02d04ce899c449660ac813a088d80ed7f42c8bb01e7cec  edgetam/vision_encoder.onnx
21e75dba7077dfcb53e8c9a6e99977156f2240ff3f1f9cddc43d66aa1ecb528e  edgetam/vision_encoder.onnx_data
d3668299ec3edf70fbb139ec642b54bf3d4be453fd1b688a6b5938e0856fe546  edgetam/prompt_encoder_mask_decoder.onnx
dfa2125e30d08d388732f20c18fb63ab0f0f590cd270eb307f0c2b65919d1be8  edgetam/prompt_encoder_mask_decoder.onnx_data
8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4  face_detection_yunet_2023mar.onnx
0e121299bf9b23a764d3f9e2120d0d0727d5ac5032e03a46cb78d28bcc7c6872  realesr_general_x4v3.onnx
b345bc59c38dbb78d871b5adf9519c475fe243c198f70cc9d0b97a464715d50b  realesr_general_x4v3_dn50.onnx
be536211515f088de05f1aa799a8079e92f49978b47aad6cec007f9fca62bb68  realesr_general_wdn_x4v3.onnx
800a80063abcc8db53f6579407347ac2d53a9f5697dcc839cff38fbd806faf37  realesrgan_x4plus.onnx
accc4757b26bdb89b32b4d3500d4f79c9dff97c1dd7c7104bf9dcb95e3311385  gfpgan_1.4.onnx
9ac296cf05fecbdb604f50211f632b722402bc1f9a96ee5a8987b01c0c3c688f  deoldify_artistic.onnx
```

## Provenance and attribution

- **MI-GAN** — Sargsyan, Navasardyan, Xu, Shi, *MI-GAN: A Simple Baseline for
  Image Inpainting on Mobile Devices*, ICCV 2023, Picsart AI Research. The
  pre-converted "MI-GAN-512-Places2 ONNX pipeline" linked from the official
  repository's README (github.com/Picsart-AI-Research/MI-GAN), hosted by the
  first author at huggingface.co/andraniksargsyan/migan. Code MIT; weights
  under a separate `LICENSE-WEIGHTS`, also MIT (read 14 Sep 2026). The graph
  takes uint8 RGB and a uint8 mask (255 = keep) and does its own crop and
  blend.
- **LaMa (big-lama)** — Suvorov et al., *Resolution-robust Large Mask
  Inpainting with Fourier Convolutions*, WACV 2022 (github.com/advimman/lama,
  Apache-2.0). ONNX port `lama_fp32.onnx` by Carve
  (huggingface.co/Carve/LaMa-ONNX, Apache-2.0), fixed 512², opset 17.
  *Grey zone*: trained on Places2, whose image terms are unverified.
- **MODNet** — Ke et al., *MODNet: Real-Time Trimap-Free Portrait Matting via
  Objective Decomposition*, AAAI 2022 (github.com/ZHKKKe/MODNet, Apache-2.0).
  Photographic weights as redistributed by HivisionIDPhotos (release
  `pretrained-model`), the same file OpenPhotoId ships.
- **U²-Net-p** — Qin et al., *U²-Net*, Pattern Recognition 2020 (Apache-2.0),
  via the rembg `v0.0.0` release, the same file OpenPixels ships.
- **EdgeTAM** — Zhou et al., *EdgeTAM: On-Device Track Anything Model*, CVPR
  2025, Meta (github.com/facebookresearch/EdgeTAM, Apache-2.0). ONNX export by
  onnx-community (huggingface.co/onnx-community/EdgeTAM-ONNX, Apache-2.0),
  fp32 with external data. Chosen over SlimSAM-77 (also Apache) for SAM 2
  quality at a similar size. **EdgeSAM is S-Lab non-commercial and is not
  this model.**
- **YuNet** — Wu et al., OpenCV Zoo `face_detection_yunet_2023mar` (MIT).
- **Real-ESRGAN** — Wang et al., *Real-ESRGAN*, ICCVW 2021
  (github.com/xinntao/Real-ESRGAN, BSD-3-Clause). `realesr-general-x4v3` and
  `-wdn-x4v3` from release v0.2.5.0, `RealESRGAN_x4plus` from v0.1.0, exported
  by OpenPixels; `dn50` is the 50/50 interpolation of the general and wdn
  weights (Real-ESRGAN's own `--denoise_strength 0.5`) baked into a file.
- **GFPGAN v1.4** — Wang et al., *Towards Real-World Blind Face Restoration
  with Generative Facial Prior*, CVPR 2021, Tencent ARC
  (github.com/TencentARC/GFPGAN). ONNX as published in
  huggingface.co/facefusion/models-3.0.0. Licence Apache-2.0 "except
  third-party components"; *grey zone*: trained on FFHQ (images under
  non-commercial terms). **Full precision only**: the fp16 file returns the
  same wash for any face on WebGPU (OpenPixels, measured).
- **DeOldify (artistic)** — Jason Antic (github.com/jantic/DeOldify, MIT),
  ONNX as published in huggingface.co/facefusion/models-3.0.0. Full precision;
  run at 256², lightness kept from the photo.

## Considered and not shipped

- **BiRefNet / BiRefNet_lite** (MIT weights) — trained on DIS5K, whose terms
  are non-commercial; research 04 puts it in the grey zone pending a legal
  call. Not shipped. U²-Net-p locates objects and EdgeTAM draws them instead.
- **1x-DeJPG-OmniSR** (CC-BY-4.0) — licence-clean, but published as PyTorch
  only; converting its OmniSR architecture needs a torch toolchain this
  workstream did not have. JPEG clean-up uses Real-ESRGAN general x4v3 at 1×
  (4× then area-averaged back), which removes blocking and ringing well but
  smooths fine texture. Revisit with the export script.
- **NAFNet-SIDD** (MIT) denoise — not exported yet; the Real-ESRGAN denoise
  variants cover it at small size.
- **RMBG-1.4/2.0, CodeFormer, InsightFace, SegFormer, 4x-UltraSharp, MAT,
  EdgeSAM** — non-commercial (research 04 §5).
- **Carve `lama.onnx`** (dynamo export) — slower than `lama_fp32.onnx` per its
  own card.

## Backends and the pixel-parity canary

`apps/web/src/engine/jobs/ort.ts` picks WebGPU only when an adapter exists
(headless Chromium exposes `navigator.gpu` with none), otherwise the CPU
build with up to 8 threads when cross-origin isolated. Sessions use
`enableMemPattern: false`, `freeDimensionOverrides` for tiled models and
`logSeverityLevel: 3`.

On the first session per (model, backend) on a device, the worker runs a
golden input — a 96² PNG crop of `testdata/photos/portrait.jpg`
(`jobs/canary-portrait.png`), resized to each model's input — reduces the
output to a 4×4-per-channel grid of means, and compares it with the signature
recorded on the CPU backend (`canary` in `lib/models.ts`). More than 10% of
the reference's spread on any cell, or a flat or non-finite output, marks that
backend bad for that model on this device (remembered in Cache Storage), and
the job falls back to the CPU. `badBackends` lists files already measured
wrong: MODNet on WebGPU (OpenPhotoId, holes through hair).

Re-record the signatures after changing a model file or onnxruntime-web:
`node apps/web/e2e/ai.mjs canary` writes `target/ai/out/canary-wasm.json`.
