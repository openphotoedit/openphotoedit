# Architecture and working agreement

Read this before touching the code. It is short on purpose.

## Shape

```
apps/web (Svelte 5 + TS, Vite)             one UI, two profiles: src/lite, src/pro
  src/engine/client.ts  ──postMessage──▶  src/engine/engine.worker.ts
                                            ├── wasm Engine (crates/editor-wasm)
                                            └── jobs/ (AI and other long work)
crates/
  editor-core       document, tiles, compositor, adjustments, history, core commands (doc/layer/image/select), pixels helpers
  editor-filters    filter.* and analyze.*   (registered domain)
  editor-paint      paint.*                  (registered domain)
  editor-select     select.* extensions      (registered domain; core handles rect/ellipse/polygon/mask/invert/feather)
  editor-transform  transform.*              (registered domain)
  editor-psd        PSD/PSB read + write
  editor-project    native project format
  editor-ai         AI pre/post-processing, tiling, planner rules (pure Rust)
  editor-wasm       wasm-bindgen surface; lib.rs registers the domains
  editor-server     native local server binary `openphotoshop`
```

- **Pixels**: 8-bit straight RGBA in 256² copy-on-write tiles (`editor_core::plane::Plane`). Absent tile = fill value.
- **Layers**: `editor_core::layer` — pixel (`Raster` = plane + x/y offset), adjustment, fill, group, text (browser rasterises), shape (tiny-skia rasterises). Bottom-to-top order.
- **Every edit is a JSON command** with an `op` like `layer.add-adjustment`. `Editor::exec` records one undo snapshot per command (snapshots share tiles, so they are cheap). Commands with the same `merge_key` in a row collapse into one undo step (slider drags, brush segments).
- **Rendering**: `editor_core::render` is the single CPU compositor. It renders any doc rectangle at any scale. The viewport, brush patches, export and PSD composite all use it.
- **Destructive pixel edits** go through `editor_core::pixels::edit_layer`, which handles selection coverage, transparency lock, pixel lock and raster growth. Do not re-implement those rules.

## Domains and file ownership

Several workstreams build in parallel **in the same checkout**. Only edit files you own. If you need something from a file you do not own, add it in your own crate/file if at all possible; if a core change is unavoidable, keep it small and additive (a new `pub fn`), keep `cargo test -p editor-core` green, and say so in your final report.

| Workstream | Owns |
|---|---|
| core (lead) | `crates/editor-core/**` (except additive helpers noted by others), `crates/editor-wasm/src/lib.rs`, `apps/web/src/engine/{client,protocol,types,engine.worker}.ts`, `apps/web/src/lib/{editor.svelte,io,brand}.ts`, `apps/web/src/canvas/**`, `apps/web/src/App.svelte`, `docs/architecture.md` |
| filters | `crates/editor-filters/**` |
| paint + select | `crates/editor-paint/**`, `crates/editor-select/**` |
| transform | `crates/editor-transform/**` |
| psd + project | `crates/editor-psd/**`, `crates/editor-project/**`, `crates/editor-wasm/src/{psd,project}.rs`, `apps/web/src/lib/psd-io.ts`, `testdata/psd/**` |
| ai | `crates/editor-ai/**`, `crates/editor-wasm/src/ai.rs`, `apps/web/src/engine/jobs/**`, `apps/web/src/lib/ai.ts`, `apps/web/src/lib/models.ts`, `scripts/fetch-models.sh`, `MODELS.md` |
| server | `crates/editor-server/**` |
| tools | `apps/web/src/tools/**` (all canvas tools, tool settings store, tool option components), `apps/web/src/lib/text.ts` |
| pro ui | `apps/web/src/pro/**`, `apps/web/src/ui/**` (shared primitives: add, do not break existing exports) |
| lite ui | `apps/web/src/lite/**` |

`docs/commands.md` is the contract between backend crates and the UI. Backend workstreams implement exactly those ops (adding optional params is fine; renaming is not). UI workstreams call them.

## Rules that apply to everyone

- **Nobody commits.** The lead commits. Do not run `git add`, `git commit`, `git stash`, `git checkout -- …` or anything that touches other people's uncommitted files.
- **Cargo**: use your own target dir to avoid lock contention: `CARGO_TARGET_DIR=target/<workstream> cargo test -p <crate>`. If a build fails in a file you do not own, wait a minute and retry; do not "fix" it.
- **Wasm**: rebuild with `scripts/build-wasm.sh --dev` (serialised by a lock; safe to run concurrently).
- **Dev server ports** (avoid collisions with sibling products): core 5203, filters 5211, paint 5212, transform 5213, psd 5214, ai 5215, server 5216, tools 5217, pro 5218, lite 5219. Start with `npx vite --port <p> --strictPort` from `apps/web`.
- **No third-party requests at run time.** Models, the ONNX runtime `.wasm` files and fonts are served from our own origin. No CDNs, no analytics.
- **Licences**: the app is AGPL-3.0-or-later. Model weights must allow commercial use and redistribution — check `docs/research/04-local-ai.md` §5 before adding any model.
- **Design**: use the vendored tokens (`apps/web/src/vendor/tokens`) through semantic variables only (`--text-muted`, `--surface-card`, …), the `oa-` component classes in `src/styles/components.css`, and Lucide icons (`@lucide/svelte/icons/<name>`). No emoji as icons. Pro renders in `oa-dark`; Lite follows the OS. Control heights come from `--control-h-*`.
- **Copy**: sentence case, plain words, active voice. A button says what happens. Errors say what went wrong and what to do.
- **Tests**: Rust unit tests next to the code; real photos in `testdata/` over synthetic fixtures where the behaviour depends on content. UI flows get a Playwright check under `apps/web/e2e/`.

## Test commands

```sh
cargo test -p editor-core                  # engine
scripts/build-wasm.sh --dev                # wasm + glue into apps/web/src/wasm-gen
cd apps/web && npx vite build              # type-free production build
cd apps/web && npx svelte-check            # types
node apps/web/e2e/smoke.mjs <photo> <out.png>   # needs a preview server on 5203
```
