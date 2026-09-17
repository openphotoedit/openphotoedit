**Release candidate. Not a public launch** — the app is under construction and
this build is for testing on real files.

OpenPhotoEdit is a layered photo editor with two profiles: **Lite** for a
photo you want fixed, **Pro** for layers, masks and PSD files. The engine is
Rust, it runs on your own machine, and the AI features are local and
unmetered — no account, no credits, nothing uploaded.

## Which file do I want?

| File | For |
|---|---|
| `OpenPhotoEdit-macOS-arm64-*.zip` | **Apple Silicon Mac.** Unzip and double-click `OpenPhotoEdit.app`. It opens the editor in your browser from `127.0.0.1` and runs the engine natively — no 4 GB browser memory cap, and AI runs on the CPU/GPU directly. |
| `openphotoedit-web-*.zip` | **Self-hosting the web app.** Static files: serve the folder over HTTPS (or `python3 -m http.server` locally) and open it. |

**The Mac app is not signed or notarised yet**, so macOS will refuse the first
launch. Right-click the app → Open → Open, or:

```sh
xattr -d com.apple.quarantine /Applications/OpenPhotoEdit.app
```

**AI models are not in either archive** (they are ~1 GB). The app downloads
what a feature needs the first time you use it, or fetch them all up front
with `OpenPhotoEdit.app/Contents/MacOS/OpenPhotoEdit models fetch --all`.

## What works

- **PSD read and write**, tested against a 269-file corpus of real Photoshop
  files: layers, groups, masks, blend modes, adjustment and fill layers,
  clipping, smart objects, 16/32-bit and PSB.
- **Camera RAW** (Canon CR2/CR3, Nikon NEF, Sony ARW, Fujifilm RAF, Olympus
  ORF, DNG) developed into re-editable layers.
- **Local AI**: remove an object, select a subject, cut out a background, blur
  the background, upscale, denoise, restore faces, colourise.
- **Lite**: auto-enhance, adjust, looks, crop, markup, export to a size limit,
  and a prompt bar that turns "make it warmer and brighter" into sliders you
  can still edit.
- 344 Rust tests and 66 browser checks pass on this build.

## Known problems in this build

- PSD **layer styles** render imperfectly: outer glow can smudge dark, bevel
  is flat, pattern overlay is not drawn at all.
- 81 of 244 corpus files differ from Photoshop's own composite by more than
  the tolerance (structure matches on all 269).
- Object removal in **fast mode** can leave a ghost; use best mode.
- RAW develops flatter than the camera's own JPEG.
- **Surface blur** (~8 s) and **CPU denoise** (~16 s) are slow on large images.

## What this is not

No account, no telemetry, no store listing, no paid tier. The web app is not
deployed anywhere yet — `openphotoedit.com` is registered but not serving.
