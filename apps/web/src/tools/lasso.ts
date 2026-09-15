// Lasso (freehand) and polygonal lasso (click points; double-click, Enter or
// clicking the first point closes; Backspace removes the last; Escape
// cancels). Holding alt while dragging the freehand lasso lays straight
// segments, as in Photoshop.

import type { EditorStore } from "../lib/editor.svelte";
import type { Tool, ToolPointer } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, dist, docPolyPath, drawHandle, redraw, run, selectionMode, snap45, thin, trackHover, type Pt } from "./common";

type Mode = "replace" | "add" | "subtract" | "intersect";

let free: { points: Pt[]; mode: Mode } | null = null;

async function commit(ed: EditorStore, pts: Pt[], mode: Mode) {
  if (pts.length < 3) {
    if (mode === "replace") await run(ed, { op: "select.none" });
    return;
  }
  await run(ed, { op: "select.polygon", points: pts.map((p) => ({ x: p.x, y: p.y })), mode, feather: toolSettings.feather, anti_alias: toolSettings.antiAlias });
}

function modeAtDown(ed: EditorStore, p: ToolPointer): Mode {
  return ed.summary?.selection && (p.shift || p.alt) ? selectionMode(p, "replace") : (toolSettings.selectMode as Mode);
}

export const lasso: Tool = {
  id: "lasso",
  label: "Lasso",
  shortcut: "l",
  cursor: "crosshair",
  down(ed, p) {
    free = { points: [{ x: p.x, y: p.y }], mode: modeAtDown(ed, p) };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !free) return;
    const last = free.points[free.points.length - 1];
    // Sub-pixel jitter makes huge polygons for nothing; keep ~1 view px.
    if (dist(last, p) * ed.view.zoom >= 1) free.points.push({ x: p.x, y: p.y });
  },
  async up(ed) {
    const f = free;
    free = null;
    if (!f) return;
    await commit(ed, thin(f.points, 0.5 / Math.max(ed.view.zoom, 0.01)), f.mode);
    redraw(ed);
  },
  cancel(ed) {
    free = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (!free || free.points.length < 2) return;
    antsStroke(ctx, () => docPolyPath(ed, ctx, free!.points, false));
  },
};

// ---------------------------------------------------------------------------

let poly: { points: Pt[]; mode: Mode; hover: Pt | null; lastClick: number } | null = null;

function closePoly(ed: EditorStore) {
  const pl = poly;
  poly = null;
  redraw(ed);
  if (pl) void commit(ed, pl.points, pl.mode).then(() => redraw(ed));
}

export const polygonLasso: Tool = {
  id: "polygon-lasso",
  label: "Polygonal lasso",
  cursor: "crosshair",
  down(ed, p) {
    const now = performance.now();
    if (!poly) {
      poly = { points: [{ x: p.x, y: p.y }], mode: modeAtDown(ed, p), hover: null, lastClick: now };
      return;
    }
    const first = poly.points[0];
    const fv = ed.toView(first.x, first.y);
    const closeToFirst = poly.points.length >= 3 && Math.hypot(fv.x - p.vx, fv.y - p.vy) < (p.pointerType === "touch" ? 22 : 9);
    const dbl = now - poly.lastClick < 350;
    poly.lastClick = now;
    if (closeToFirst || dbl) {
      closePoly(ed);
      return;
    }
    const last = poly.points[poly.points.length - 1];
    poly.points.push(p.shift ? snap45(last, p) : { x: p.x, y: p.y });
  },
  move(ed, p) {
    trackHover(ed, p);
    if (!poly) return;
    const last = poly.points[poly.points.length - 1];
    poly.hover = p.shift ? snap45(last, p) : { x: p.x, y: p.y };
  },
  cancel(ed) {
    poly = null;
    redraw(ed);
  },
  deactivate(ed) {
    poly = null;
    redraw(ed);
  },
  key(ed, e) {
    if (!poly) return false;
    if (e.key === "Enter") {
      closePoly(ed);
      return true;
    }
    if (e.key === "Backspace" || e.key === "Delete") {
      poly.points.pop();
      if (!poly.points.length) poly = null;
      redraw(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    if (!poly) return;
    const pts = poly.hover ? [...poly.points, poly.hover] : poly.points;
    antsStroke(ctx, () => docPolyPath(ed, ctx, pts, false));
    const f = ed.toView(poly.points[0].x, poly.points[0].y);
    drawHandle(ctx, f.x, f.y, 7, true);
  },
};
