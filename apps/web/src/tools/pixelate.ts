// Pixelate: drag a box over something to hide; the pixels inside become a
// mosaic. It is destructive by design (the source detail is gone), so it
// works on a pixel layer: with anything else active, the visible image is
// stamped into a new layer first.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { clipRect, drawLabel, layersTopDown, rectFrom, redraw, run, setActive, trackHover, type Pt } from "./common";

let drag: { start: Pt; end: Pt; startV: Pt; moved: boolean } | null = null;
let working = false;

async function pixelateBox(ed: EditorStore, box: Rect) {
  const s = ed.summary;
  if (!s) return;
  const r = clipRect({ x: Math.round(box.x), y: Math.round(box.y), w: Math.round(box.w), h: Math.round(box.h) }, s.width, s.height);
  if (r.w < 2 || r.h < 2) return;
  working = true;
  try {
    if (ed.active?.kind !== "pixel") {
      // Markup layers stay live above the photo: pixelate the topmost pixel
      // layer instead, and stamp only when there is none.
      const target = layersTopDown(ed).find((l) => l.kind === "pixel" && l.visible && !l.locks.all && !l.locks.pixels);
      if (target) await setActive(ed, target.id);
      else if (!(await run(ed, { op: "layer.stamp-visible" }))) return;
    }
    // Computed here and written with one `layer.set-pixels`, so a pixelated
    // box is a single undo step (the filter route needs a borrowed
    // selection: three steps for one gesture).
    await pixelateInBrowser(ed, r, Math.max(2, Math.round(toolSettings.pixelateCell)));
  } finally {
    working = false;
    redraw(ed);
  }
}

/** Mosaic over `r` on the active layer, cells aligned to the document grid. */
async function pixelateInBrowser(ed: EditorStore, r: Rect, cell: number) {
  const id = ed.summary?.active;
  if (id == null) return;
  const px = await ed.engine.call<Uint8Array>("layer_region", id, r.x, r.y, r.w, r.h);
  const out = new Uint8ClampedArray(px.length);
  // Cells align to the document grid so neighbouring boxes match.
  const gx0 = Math.floor(r.x / cell) * cell;
  const gy0 = Math.floor(r.y / cell) * cell;
  for (let gy = gy0; gy < r.y + r.h; gy += cell) {
    for (let gx = gx0; gx < r.x + r.w; gx += cell) {
      const x0 = Math.max(r.x, gx) - r.x;
      const y0 = Math.max(r.y, gy) - r.y;
      const x1 = Math.min(r.x + r.w, gx + cell) - r.x;
      const y1 = Math.min(r.y + r.h, gy + cell) - r.y;
      let sr = 0;
      let sg = 0;
      let sb = 0;
      let sa = 0;
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const o = (y * r.w + x) * 4;
          const a = px[o + 3];
          sr += px[o] * a;
          sg += px[o + 1] * a;
          sb += px[o + 2] * a;
          sa += a;
        }
      }
      const n = (x1 - x0) * (y1 - y0);
      const cr = sa ? sr / sa : 0;
      const cg = sa ? sg / sa : 0;
      const cb = sa ? sb / sa : 0;
      const ca = n ? sa / n : 0;
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const o = (y * r.w + x) * 4;
          out[o] = cr;
          out[o + 1] = cg;
          out[o + 2] = cb;
          out[o + 3] = ca;
        }
      }
    }
  }
  await run(ed, { op: "layer.set-pixels", id, x: r.x, y: r.y, width: r.w, height: r.h, label: "Pixelate" }, { bytes: out });
}

export const pixelate: Tool = {
  id: "pixelate",
  label: "Pixelate",
  cursor: "crosshair",
  down(_ed, p) {
    if (working) return;
    drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, startV: { x: p.vx, y: p.vy }, moved: false };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) return;
    drag.end = { x: p.x, y: p.y };
    if (Math.hypot(p.vx - drag.startV.x, p.vy - drag.startV.y) > 3) drag.moved = true;
  },
  async up(ed) {
    const d = drag;
    drag = null;
    if (!d?.moved) {
      redraw(ed);
      return;
    }
    await pixelateBox(ed, rectFrom(d.start, d.end));
  },
  cancel(ed) {
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (!drag?.moved) return;
    const r = rectFrom(drag.start, drag.end);
    const a = ed.toView(r.x, r.y);
    const b = ed.toView(r.x + r.w, r.y + r.h);
    const cell = Math.max(4, toolSettings.pixelateCell * ed.view.zoom);
    ctx.save();
    ctx.beginPath();
    ctx.rect(a.x, a.y, b.x - a.x, b.y - a.y);
    ctx.clip();
    // A checker of the cell size previews the mosaic scale.
    for (let y = a.y, j = 0; y < b.y; y += cell, j++) {
      for (let x = a.x, i = 0; x < b.x; x += cell, i++) {
        ctx.fillStyle = (i + j) % 2 ? "rgba(255,255,255,0.28)" : "rgba(0,0,0,0.28)";
        ctx.fillRect(x, y, cell, cell);
      }
    }
    ctx.restore();
    ctx.save();
    ctx.strokeStyle = "#ffffff";
    ctx.setLineDash([5, 4]);
    ctx.strokeRect(a.x + 0.5, a.y + 0.5, b.x - a.x, b.y - a.y);
    ctx.restore();
    drawLabel(ctx, `${Math.round(r.w)} × ${Math.round(r.h)}`, b.x, b.y);
  },
};
