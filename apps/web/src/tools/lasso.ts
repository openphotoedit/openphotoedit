// Lasso (freehand) and polygonal lasso (click points; double-click, Enter or
// clicking the first point closes; Backspace removes the last; Escape
// cancels). Holding Alt while dragging the freehand lasso lays straight
// segments, as in Photoshop: the segment follows the pointer from the last
// point, releasing Alt goes back to freehand from there, and releasing the
// button with Alt still held keeps the outline open so each click adds a
// corner; letting go of Alt then closes it.

import type { EditorStore } from "../lib/editor.svelte";
import type { Tool, ToolPointer } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, dist, docPolyPath, drawHandle, redraw, run, selectionMode, snap45, thin, trackHover, type Pt } from "./common";

type Mode = "replace" | "add" | "subtract" | "intersect";

interface Free {
  points: Pt[];
  mode: Mode;
  /** Alt held: the straight segment's moving end (not yet a point). */
  straight: Pt | null;
  /** Button released with Alt held: clicks add corners until Alt is let go. */
  open: boolean;
  /** Alt was held at the press to choose the mode; straight segments start once it is pressed again. */
  altFree: boolean;
}

let free: Free | null = null;

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

async function closeFree(ed: EditorStore) {
  const f = free;
  free = null;
  redraw(ed);
  if (!f) return;
  const pts = f.straight ? [...f.points, f.straight] : f.points;
  await commit(ed, thin(pts, 0.5 / Math.max(ed.view.zoom, 0.01)), f.mode);
  redraw(ed);
}

export const lasso: Tool & { keyup(ed: EditorStore, e: KeyboardEvent): boolean } = {
  id: "lasso",
  label: "Lasso",
  shortcut: "l",
  cursor: "crosshair",
  down(ed, p) {
    if (free?.open) {
      // A corner of the open outline; dragging from here without Alt goes freehand again.
      free.points.push({ x: p.x, y: p.y });
      free.straight = null;
      if (!p.alt) free.open = false;
      redraw(ed);
      return;
    }
    const keysMode = !!ed.summary?.selection && p.alt;
    free = { points: [{ x: p.x, y: p.y }], mode: modeAtDown(ed, p), straight: null, open: false, altFree: !keysMode };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    const f = free;
    if (!f) return;
    if (!p.alt) f.altFree = true;
    if (f.open) {
      f.straight = { x: p.x, y: p.y };
      redraw(ed);
      return;
    }
    if (!pressed) return;
    if (p.alt && f.altFree) {
      f.straight = { x: p.x, y: p.y };
      return;
    }
    if (f.straight) {
      // Alt let go: the straight segment ends here, freehand resumes.
      f.points.push({ x: p.x, y: p.y });
      f.straight = null;
      return;
    }
    const last = f.points[f.points.length - 1];
    // Sub-pixel jitter makes huge polygons for nothing; keep ~1 view px.
    if (dist(last, p) * ed.view.zoom >= 1) f.points.push({ x: p.x, y: p.y });
  },
  async up(ed, p) {
    const f = free;
    if (!f) return;
    if (p.alt && f.altFree && f.points.length >= 1) {
      // Keep going with clicks while Alt stays down.
      if (f.straight) f.points.push(f.straight);
      else if (!f.open) f.points.push({ x: p.x, y: p.y });
      f.straight = null;
      f.open = true;
      redraw(ed);
      return;
    }
    if (f.straight) f.points.push({ x: p.x, y: p.y });
    f.straight = null;
    await closeFree(ed);
  },
  keyup(ed, e) {
    if (e.key === "Alt" && free?.open) {
      free.straight = null;
      void closeFree(ed);
      return true;
    }
    return false;
  },
  key(ed, e) {
    if (e.key === "Enter" && free?.open) {
      free.straight = null;
      void closeFree(ed);
      return true;
    }
    return false;
  },
  cancel(ed) {
    free = null;
    redraw(ed);
  },
  deactivate(ed) {
    free = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (!free) return;
    const pts = free.straight ? [...free.points, free.straight] : free.points;
    if (pts.length < 2) return;
    antsStroke(ctx, () => docPolyPath(ed, ctx, pts, false));
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
