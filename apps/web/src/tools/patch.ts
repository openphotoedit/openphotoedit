// Patch: lasso the area to fix, then drag the selection onto clean texture.
// The texture is copied into the selection and its colour is blended into
// the surroundings (the difference along the selection edge is spread
// smoothly across the patch by push-pull interpolation), then written with
// one `layer.set-pixels` that respects the selection's soft edge.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, dist, docPolyPath, docRectPath, inRect, redraw, run, trackHover, type Pt } from "./common";

let lasso: Pt[] | null = null;
let drag: { start: Pt; end: Pt; sel: Rect } | null = null;
let working = false;

export const patch: Tool = {
  id: "patch",
  label: "Patch",
  cursor: "crosshair",
  down(ed, p) {
    if (working) return;
    const sel = ed.summary?.selection?.bounds;
    if (sel && inRect(sel, p) && !p.shift && !p.alt) {
      drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, sel };
      return;
    }
    lasso = [{ x: p.x, y: p.y }];
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed) return;
    if (drag) drag.end = { x: p.x, y: p.y };
    else if (lasso && dist(lasso[lasso.length - 1], p) * ed.view.zoom >= 1) lasso.push({ x: p.x, y: p.y });
  },
  async up(ed) {
    if (lasso) {
      const pts = lasso;
      lasso = null;
      if (pts.length >= 3) await run(ed, { op: "select.polygon", points: pts, mode: "replace", feather: 1, anti_alias: true });
      else await run(ed, { op: "select.none" });
      redraw(ed);
      return;
    }
    const d = drag;
    drag = null;
    if (!d) return;
    const dx = Math.round(d.end.x - d.start.x);
    const dy = Math.round(d.end.y - d.start.y);
    if (!dx && !dy) {
      redraw(ed);
      return;
    }
    working = true;
    try {
      await applyPatch(ed, d.sel, dx, dy);
    } finally {
      working = false;
      redraw(ed);
    }
  },
  cancel(ed) {
    lasso = null;
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (lasso && lasso.length > 1) antsStroke(ctx, () => docPolyPath(ed, ctx, lasso!, false));
    if (drag) {
      const r = { ...drag.sel, x: drag.sel.x + drag.end.x - drag.start.x, y: drag.sel.y + drag.end.y - drag.start.y };
      antsStroke(ctx, () => docRectPath(ed, ctx, r));
    }
  },
};

async function applyPatch(ed: EditorStore, sel: Rect, dx: number, dy: number) {
  const active = ed.active;
  if (!active || active.kind !== "pixel") {
    ed.toast(t("Patch works on a pixel layer. Select one first."), "error");
    return;
  }
  // One pixel of margin to measure the colour difference just outside.
  const r = { x: sel.x - 1, y: sel.y - 1, w: sel.w + 2, h: sel.h + 2 };
  const [dst, src, cov] = await Promise.all([
    ed.engine.call<Uint8Array>("layer_region", active.id, r.x, r.y, r.w, r.h),
    ed.engine.call<Uint8Array>("layer_region", active.id, r.x + dx, r.y + dy, r.w, r.h),
    ed.engine.call<Uint8Array>("selection_region", r.x, r.y, r.w, r.h),
  ]);
  const { w, h } = r;
  const n = w * h;
  // Boundary: unselected pixels next to selected ones.
  const diff = new Float32Array(n * 3);
  const weight = new Float32Array(n);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (cov[i] > 16) continue;
      let near = false;
      for (const [ox, oy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const nx = x + ox;
        const ny = y + oy;
        if (nx >= 0 && ny >= 0 && nx < w && ny < h && cov[ny * w + nx] > 16) near = true;
      }
      if (!near) continue;
      for (let c = 0; c < 3; c++) diff[i * 3 + c] = dst[i * 4 + c] - src[i * 4 + c];
      weight[i] = 1;
    }
  }
  const offset = pushPull(diff, weight, w, h);
  const k = 0.25 + 0.75 * toolSettings.patchBlend;
  const out = new Uint8ClampedArray(n * 4);
  for (let i = 0; i < n; i++) {
    for (let c = 0; c < 3; c++) out[i * 4 + c] = src[i * 4 + c] + offset[i * 3 + c] * k;
    out[i * 4 + 3] = src[i * 4 + 3];
  }
  await run(ed, { op: "layer.set-pixels", id: active.id, x: r.x, y: r.y, width: w, height: h, respect_selection: true, label: "Patch" }, { bytes: out });
}

/** Smoothly fill `values` (3 channels) everywhere from the weighted samples. */
function pushPull(values: Float32Array, weights: Float32Array, w: number, h: number): Float32Array {
  if (w <= 1 && h <= 1) return values;
  const w2 = Math.max(1, Math.ceil(w / 2));
  const h2 = Math.max(1, Math.ceil(h / 2));
  const v2 = new Float32Array(w2 * h2 * 3);
  const wt2 = new Float32Array(w2 * h2);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const wt = weights[i];
      if (!wt) continue;
      const j = (y >> 1) * w2 + (x >> 1);
      wt2[j] += wt;
      for (let c = 0; c < 3; c++) v2[j * 3 + c] += values[i * 3 + c] * wt;
    }
  }
  for (let j = 0; j < w2 * h2; j++) {
    if (wt2[j] > 0) for (let c = 0; c < 3; c++) v2[j * 3 + c] /= wt2[j];
    wt2[j] = Math.min(1, wt2[j]);
  }
  const coarse = w2 === w && h2 === h ? v2 : pushPull(v2, wt2, w2, h2);
  const out = new Float32Array(w * h * 3);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const wt = Math.min(1, weights[i]);
      // Bilinear sample of the coarse level.
      const fx = Math.min(w2 - 1, Math.max(0, (x - 0.5) / 2));
      const fy = Math.min(h2 - 1, Math.max(0, (y - 0.5) / 2));
      const x0 = Math.floor(fx);
      const y0 = Math.floor(fy);
      const x1 = Math.min(w2 - 1, x0 + 1);
      const y1 = Math.min(h2 - 1, y0 + 1);
      const ax = fx - x0;
      const ay = fy - y0;
      for (let c = 0; c < 3; c++) {
        const up =
          coarse[(y0 * w2 + x0) * 3 + c] * (1 - ax) * (1 - ay) +
          coarse[(y0 * w2 + x1) * 3 + c] * ax * (1 - ay) +
          coarse[(y1 * w2 + x0) * 3 + c] * (1 - ax) * ay +
          coarse[(y1 * w2 + x1) * 3 + c] * ax * ay;
        out[i * 3 + c] = values[i * 3 + c] * wt + up * (1 - wt);
      }
    }
  }
  return out;
}
