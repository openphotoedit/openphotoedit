# The MCP App view

`viewer.html` is the whole app: one self-contained file, served by the Rust
server as the resource `ui://openphotoedit/viewer` with mimeType
`text/html;profile=mcp-app`, rendered by the host in a sandboxed iframe.

It is built for the MCP Apps default CSP —

```
default-src 'none'; script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline'; img-src 'self' data:;
media-src 'self' data:; connect-src 'none';
```

— so there is no external font, no CDN, no `fetch`/`XHR`/`WebSocket`, no
storage of any kind. Pixels only ever arrive as `data:` URIs inside tool
results. The harness enforces all of this.

## Protocol

MCP Apps extension, spec 2026-01-26 (folded into MCP 2026-07-28). JSON-RPC
2.0 over `postMessage` with `window.parent`, target origin `*` because the
sandbox gives the frame an opaque origin.

| Direction | Method | When |
|---|---|---|
| view → host | `ui/initialize` | on load, id 1 |
| view → host | `ui/notifications/initialized` | once the handshake resolves |
| view → host | `tools/call` | undo, redo, render_preview, save_document |
| view → host | `ui/update-model-context` | after undo / redo / save, so the chat knows |
| view → host | `ui/request-display-mode` | the expand button, only if the host offers `fullscreen` |
| view → host | `notifications/message` | ready line, and any failure |
| view → host | `ui/notifications/size-changed` | on resize |
| host → view | `ui/notifications/tool-input` | shows "Rendering…" in the empty state |
| host → view | `ui/notifications/tool-result` | the payload below |
| host → view | `ui/notifications/tool-cancelled` | clears busy, shows the notice |
| host → view | `ui/notifications/host-context-changed` | theme and display mode |
| host → view | `ui/resource-teardown`, `ping` | answered with `{}` |

## The payload it reads

The flat snake_case object `apps/mcp/src/server.rs` already builds —
`Open::state(id)` merged with `document_data(ed, id)` plus the preview
fields `render_preview` inserts. **Every field is optional**, and each
payload only carries part of it; the view merges what is there and keeps
what is not, so a `save_document` receipt does not blank the canvas.

```jsonc
{
  "session_id": "ph_7f3c91",          // string | null
  "path":       "/…/harbour-dusk.psd",// string — also the title
  "format":     "psd",                // string — the tag beside the title
  "family":     "layered",            // string
  "width":  3600, "height": 2400,     // number (also read from document.{width,height})
  "resolution": 300,                  // number, ppi
  "saved":   false,                   // bool — false paints the unsaved dot
  "revision": 4,                      // number — a change here makes a "before"
  "color_mode": "Rgba8",              // string, optional — status line
  "file_bytes": 48233472,             // number, optional — status line
  "layers": [ /* summary::layer, bottom→top, groups carry children[] */ ],
  "history": { "undo": ["Open","Curves"], "redo": [] },   // label arrays
  "warnings": [],
  "document": { "width":3600, "height":2400, "resolution":300, "layer_count":8 },
  "histogram": { "r":[256], "g":[256], "b":[256], "l":[256] },
  "preview_data_uri": "data:image/jpeg;base64,…",
  "preview": { "width":768, "height":512, "bytes":48122, "scale":4.69,
               "mime_type":"image/jpeg" }
}
```

A layer is read for `id`, `name`, `kind`, `visible`, `opacity` (0..1),
`blend` (kebab-case), `clip`, `mask` (an object means "has a mask") and
`children`. The tree is walked backwards at every level, so the list reads
top layer first with a group's header above its own children, the way every
editor shows it.

A save receipt — `{output_path, width, height, format, bytes, metadata,
session_id, closed}` — is recognised by `output_path` with no `layers`: it
retitles the document, clears the unsaved dot and takes `bytes` as the file
size, and touches nothing else.

`camelCase` equivalents (`sessionId`, `preview.uri`, `histogram.luma`,
`colorMode`, `hasMask`, `clipped`) are accepted as synonyms, so an
alternative server shape still renders.

### Two fields worth adding

`color_mode` and `file_bytes` already exist on `image_info`'s payload but
not on `Open::state`. Adding them there would fill the last two slots of the
status line for an open session; without them the view falls back to
`resolution` and hides the size.

### before / after

The server sends no "before" image and does not need to. The view keeps the
preview it was last showing and promotes it to *before* when a payload
arrives at a **different `revision`**. A re-render at another size (same
revision) replaces the preview without disturbing the comparison. An
explicit `before_data_uri` wins if one is ever sent.

## Tools it calls

Names and argument names are the server's, verbatim:

| Tool | Arguments |
|---|---|
| `undo` / `redo` | `{ session_id }` |
| `render_preview` | `{ session_or_path, max_side }` — clamped to `engine::MAX_PREVIEW_SIDE` (768) |
| `save_document` | `{ session_id, output_path, overwrite }` |

Save is the one destructive action, so it always asks first: **Overwrite**
sends the document's own `path` with `overwrite: true`; **Save a copy** sends
`<name>-edited.<ext>` with `overwrite: false`, which the server refuses if
that file already exists. Nothing is written without that confirmation.

The layer list is read-only. The view never toggles visibility or opacity,
because no tool exists for it and a control that silently does nothing is
worse than no control.

## Style

OpenApps design system. The token files cannot be linked (single file, no
network), so the variables used are inlined at the top of `viewer.html`,
verbatim from `apps/web/src/vendor/tokens/css/*` and
`apps/web/src/styles/app.css`, with the dark block character-identical to
`colors.css`'s. OpenPhotoEdit holds **accent slot 24, iris**, used only for
the 7px identity dot and the 16% tint on the active compare toggle. The
buttons are ports of `apps/web/src/styles/components.css`.

Dark mode follows `prefers-color-scheme` and is overridden by
`hostContext.theme` via `data-theme` on `<html>`.

## Verifying

```
node apps/mcp/ui/host-harness.mjs
```

Stands up a fake host in Playwright: the viewer in a sandboxed iframe with
the spec's default CSP applied as a `<meta>` policy and a violation reporter,
answering the handshake, pushing tool results whose pixels are drawn in-page
with OffscreenCanvas, and logging every `tools/call`. It drives the empty
state, a first preview, an edit, the split drag, zoom, undo, a JSON-RPC
failure, an `isError` result, both save paths and the save receipt, at
380×700 and 1000×800 in light and dark, asserting no CSP violation, no
network request outside `data:`, no horizontal overflow and no overlapping
panels. Screenshots land in `shots/`.
