// Eyedropper: sample the merged image (or the active layer) into the
// foreground colour; alt samples into the background. A loupe ring shows
// the colour under the pointer while pressed.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rgba8 } from "../engine/types";
import type { Tool, ToolPointer } from "./types";
import { toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import { css, drawCrosshair, hover, redraw, trackHover, exec } from "./common";

let pressed = false;
let target: "primary" | "secondary" = "primary";
let current: Rgba8 | null = null;
let busy = false;
let queued: ToolPointer | null = null;
let analyzeMissing = false;

/** Average colour of a size×size box at (x, y). */
export async function sampleColor(ed: EditorStore, x: number, y: number, size = toolSettings.sampleSize, merged = toolSettings.sampleMerged): Promise<Rgba8 | null> {
  const s = ed.summary;
  if (!s) return null;
  const px = Math.floor(x);
  const py = Math.floor(y);
  if (px < 0 || py < 0 || px >= s.width || py >= s.height) return null;
  if (!analyzeMissing) {
    try {
      const r = await exec(ed, { op: "analyze.pick", x: px, y: py, size, merged });
      const c = (r.data as { color?: Rgba8 } | null)?.color;
      if (c) return c;
    } catch (e) {
      if (/unknown operation/i.test(String(e))) analyzeMissing = true;
    }
  }
  const half = Math.floor(size / 2);
  const x0 = Math.max(0, px - half);
  const y0 = Math.max(0, py - half);
  const w = Math.min(s.width, px + half + 1) - x0;
  const h = Math.min(s.height, py + half + 1) - y0;
  const id = s.active;
  const bytes = merged || id == null ? await ed.engine.call<Uint8Array>("region", x0, y0, w, h) : await ed.engine.call<Uint8Array>("layer_region", id, x0, y0, w, h);
  let r = 0;
  let g = 0;
  let b = 0;
  let a = 0;
  const n = w * h;
  for (let i = 0; i < n; i++) {
    const al = bytes[i * 4 + 3];
    r += bytes[i * 4] * al;
    g += bytes[i * 4 + 1] * al;
    b += bytes[i * 4 + 2] * al;
    a += al;
  }
  if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
  return { r: Math.round(r / a), g: Math.round(g / a), b: Math.round(b / a), a: Math.round(a / n) };
}

async function sample(ed: EditorStore, p: ToolPointer) {
  if (busy) {
    queued = p;
    return;
  }
  busy = true;
  try {
    const c = await sampleColor(ed, p.x, p.y);
    if (c) {
      current = c;
      if (pressed) {
        const opaque = { r: c.r, g: c.g, b: c.b, a: 255 };
        if (target === "primary") ed.primary = opaque;
        else ed.secondary = opaque;
        toolState.sampled = opaque;
      }
    }
  } finally {
    busy = false;
    redraw(ed);
    const q = queued;
    queued = null;
    if (q) void sample(ed, q);
  }
}

export const eyedropper: Tool = {
  id: "eyedropper",
  label: "Eyedropper",
  shortcut: "i",
  cursor: "crosshair",
  down(ed, p) {
    pressed = true;
    target = p.alt ? "secondary" : "primary";
    void sample(ed, p);
  },
  move(ed, p, isPressed) {
    trackHover(ed, p);
    if (isPressed) void sample(ed, p);
  },
  up(ed, p) {
    void sample(ed, p).then(() => {
      pressed = false;
      current = null;
      redraw(ed);
    });
  },
  cancel(ed) {
    pressed = false;
    current = null;
    redraw(ed);
  },
  overlay(_ed, ctx) {
    if (!pressed || !current || !hover.inside) return;
    const x = hover.vx;
    const y = hover.vy;
    const prev = target === "primary" ? _ed.primary : _ed.secondary;
    ctx.save();
    // Loupe ring: new colour on the top half, previous on the bottom.
    const R = 46;
    const w = 16;
    ctx.lineWidth = w;
    ctx.strokeStyle = css({ ...current, a: 255 });
    ctx.beginPath();
    ctx.arc(x, y, R, Math.PI, Math.PI * 2);
    ctx.stroke();
    ctx.strokeStyle = css({ ...prev, a: 255 });
    ctx.beginPath();
    ctx.arc(x, y, R, 0, Math.PI);
    ctx.stroke();
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(128,128,128,0.9)";
    ctx.beginPath();
    ctx.arc(x, y, R + w / 2, 0, Math.PI * 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(x, y, R - w / 2, 0, Math.PI * 2);
    ctx.stroke();
    ctx.restore();
    drawCrosshair(ctx, x, y, 6);
  },
};
