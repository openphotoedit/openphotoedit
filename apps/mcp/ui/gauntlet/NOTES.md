# The Photoshop gauntlet

`index.html` is the whole front end: one self-contained file, served by the
Rust server at `/`, that watches a run of the PSD composite test and shows it
as a scoreboard someone can stand in front of.

269 real Photoshop files are opened by our engine and flattened. Photoshop
wrote its own flattened composite into each of those files when it saved; the
run compares ours against that, pixel for pixel. The page shows all three —
ours, Adobe's, and the amplified difference — file by file, with the tally,
the error histogram, the worst offenders and the throughput building around
it, and ends on a summary card.

Constraints, the same ones `viewer.html` is built to: one file, no CDN, no
external font, no storage, nothing but `data:` URIs for pixels, so it survives
a strict CSP (`default-src 'none'; script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'`).
Every colour is a brand hex from `apps/web/src/vendor/tokens/css/colors.css`,
copied verbatim out of `apps/mcp/ui/viewer.html`; OpenPhotoEdit holds accent
slot 24, iris.

## The feed

An AG-UI event stream (https://docs.ag-ui.com) over SSE, same origin.

Event names are matched case- and separator-insensitively, so `RUN_STARTED`,
`RunStarted` and `run_started` all land in the same place.

| Event | What the page does with it |
|---|---|
| `RUN_STARTED` | clears the board, remembers `runId` |
| `TEXT_MESSAGE_*` | the opening sentence becomes the line under the rail; the closing one is quoted on the summary card |
| `STATE_SNAPSHOT` | replaces the state; `total` builds the rail |
| `STATE_DELTA` | RFC 6902 JSON Patch (`add`/`replace`/`remove`/`copy`/`move`/`test`); a malformed op is dropped rather than allowed to stop the run |
| `STEP_STARTED` / `STEP_FINISHED` | `stepName` is the file the server is on — it goes in the status line. No phase is inferred from it; the phase comes from the state |
| `RUN_FINISHED` | the summary card. `result.megapixels` and `result.files`, when present, are trusted over anything the page counted |
| `RUN_ERROR` | the error card, with `code` and `message`, over whatever was measured first |

The stream URL is found by trying, in order, `/gauntlet/stream`,
`/agui/gauntlet`, `/gauntlet/events`, `/agui/stream`, `/events`, moving on
only while nothing has ever arrived. `?stream=<path>` overrides it. The page
passes its own `?pace=` and `?filter=` through to the server.

## The state it reads

```jsonc
{ "total": 269, "comparable": 244, "index": 137, "phase": "compare",
  "current": { "file": "psd-tools/layer_effects.psd", "width": 800, "height": 600,
               "layers": 12, "mae": 0.42, "p95": 2.0, "verdict": "within1",
               "ms": 38, "note": null,
               "ours": "data:image/jpeg;base64,…", "adobe": "data:…", "diff": "data:…" },
  "tally": { "exact": 91, "within1": 64, "within3": 12, "within10": 23,
             "over": 54, "structureOnly": 25, "failed": 0 },
  "histogram": [/* 32 counts */],
  "histogramEdges": [0, 0.5, 1, … 16],
  "worst": [ { "file": "…", "mae": 159.36, "ours": "data:…", "adobe": "data:…", "diff": "data:…" } ],
  "throughput": { "filesPerSec": 66.9, "megapixelsPerSec": 11.6, "elapsedMs": 4020 } }
```

Everything is optional and everything has a fallback:

- `histogramEdges` drives the axis labels and the colour of each bar. Without
  it the page assumes 0.5 levels per bucket, which is what the server sends.
  The last bucket is open-ended, and the axis says so (`15.5+`).
- `worst[]` entries need only `file` and `mae`; a missing `verdict` is derived
  from the MAE, and missing dimensions are taken from the decoded image.
- `index` may be 0- or 1-based; the rail position is the number of distinct
  files that have landed, not the server's index.
- A file described by several deltas is **one** file. The ledger is keyed by
  path, so the megapixel total and the median are not double counted.

## Pacing, pause and the queue

`?pace=<ms>` is a dwell **per file**, not per event — the server sends four or
five deltas for each file it finishes, and pacing those individually would put
the board three files behind the stream. Events are drained up to and
including the next file that lands, then the board waits.

**Pause** holds the events rather than dropping them: the server keeps
sending, the queue grows (the count is in the status line and on the Resume
button) and the board catches up on resume. If the queue ever passes 400
events the board fast-forwards to 120 rather than holding megabytes of base64.

The filter applies on Enter or on Start — never on a keystroke, or a presenter
typing four letters would restart the run four times.

## The triptych

Ours, Adobe's and the difference are one picture of one file, so all three are
decoded together and revealed together, and the caption is painted at the same
moment. A swap the stream has already overtaken is dropped rather than
half-applied — at full speed a file lands every few milliseconds, and a
staggered reveal would put one panel a file behind the filename above it.

The plates take the file's own aspect ratio from `current.width/height`, so a
2480 × 3508 poster is a tall plate and not a landscape box with the picture
letterboxed inside it. An empty plate says why it is empty ("no Adobe
composite in this file", "the file would not open", or `current.note`).

The `1× / 4× / 10×` control on the difference panel is a **view** gain on top
of whatever the server already amplified — a projector in a bright room needs
it. It is hidden below 780px.

## Showing the failures

The point is a credible claim, not a green wall.

- Seven verdicts, one hue each, the same in both themes: exact and within-1
  green, within-3 blue, within-10 orange, over-10 red, structure-only violet,
  failed a dark red that cannot be mistaken for "fine".
- The rail is one tick per file in its verdict colour, so the misses are
  scattered visibly through the run rather than averaged away.
- Histogram bar heights are √count, so a single 159-level miss is still a
  visible bar next to a hundred exact matches, and no bucket with anything in
  it is shorter than 4px.
- The summary card names the number over ten levels and the worst file, and
  the stacked bar is drawn from the honest tally with percentages.

## Verifying it

`replay-harness.mjs` — Playwright, no server needed.

```
node apps/mcp/ui/gauntlet/replay-harness.mjs                       # replay
node apps/mcp/ui/gauntlet/replay-harness.mjs --live http://127.0.0.1:5261
```

The replay serves `index.html` from a synthetic origin under the CSP above,
replaces `window.EventSource` before the page's script runs, and generates
every pixel it "receives" inside the page with `OffscreenCanvas` — ours,
Adobe's, and a gamma-boosted difference — so it depends on no file in the repo
but `index.html`. It replays 40 files (two ugly outliers, one with no Adobe
composite, one outright failure), drives mid-run, the outlier landing, a
worst-offender click, pause and resume, the summary, a transient blip, a
dropped connection, an empty filter and a failed run, and screenshots each
wide and narrow, light and dark, plus a 1920 × 1080 projector pass and a
reduced-motion pass, into `shots/`. It asserts zero network requests outside
the document and `data:` URIs, no CSP violation, no uncaught error and no
sideways overflow.

`--live` runs the same page against the real server and checks the numbers:
269 files, every one tallied exactly once, 47 MP (not double), the worst
offender on the board, and the pill reading *Finished* — not *Reconnecting* —
after the server hangs up on `RUN_FINISHED`.

Screenshots are in `shots/`. `live-*.png` are from the real corpus.
