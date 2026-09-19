// Smart guides. While a layer, a selection, a transform box or a crop box is
// dragged, its edges and centre snap to the canvas edges and centre, the
// edges and centres of other visible layers, and ruler guides, within a few
// screen pixels. Control held disables it for that moment, as in Photoshop.
// The snapped lines are drawn in magenta. (Behaviour after Compositor's
// TransformSnap / CropSnap, MIT; our targets use each layer's summary bounds.)

import type { EditorStore } from "../lib/editor.svelte";
import type { LayerInfo, Rect } from "../engine/types";
import { ancestorIds, mods } from "./common";

/** Snap distance in screen (CSS) pixels. */
export const SNAP_PX = 8;

const KEY = "ops.snap";

function load(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export interface GuideLine {
  axis: "x" | "y";
  pos: number;
}

export const snapConfig: {
  /** View › Snap. */
  enabled: boolean;
  /** Ruler guides, registered by the shell that owns them. */
  guides: (() => GuideLine[]) | null;
} = { enabled: load(), guides: null };

export function setSnapEnabled(on: boolean) {
  snapConfig.enabled = on;
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    /* private mode: lasts for the session */
  }
}

/** Snapping applies now: switched on and Control not held. */
export function snapActive(): boolean {
  return snapConfig.enabled && !mods.ctrl;
}

/** Snap distance in document pixels at the current zoom. */
export function snapTolerance(ed: EditorStore): number {
  return SNAP_PX / Math.max(1e-6, ed.view.zoom);
}

export interface Targets {
  xs: number[];
  ys: number[];
}

/**
 * Canvas edges (and centre), every other visible layer's edges (and
 * centre), and ruler guides. `exclude` are the moving layers: their own
 * children and enclosing groups are left out too.
 */
export function snapTargets(ed: EditorStore, exclude: number[], opts: { centres?: boolean } = {}): Targets {
  const s = ed.summary;
  if (!s) return { xs: [], ys: [] };
  const centres = opts.centres ?? true;
  const xs = [0, s.width];
  const ys = [0, s.height];
  if (centres) {
    xs.push(s.width / 2);
    ys.push(s.height / 2);
  }
  const skip = new Set(exclude);
  for (const id of exclude) for (const a of ancestorIds(ed, id)) skip.add(a);
  const walk = (ls: LayerInfo[]) => {
    for (const l of ls) {
      if (!l.visible || (skip.has(l.id) && !l.children) || exclude.includes(l.id)) continue;
      if (l.children) {
        walk(l.children);
        continue;
      }
      const b = l.bounds;
      if (!b || b.w <= 0 || b.h <= 0 || l.kind === "adjustment") continue;
      xs.push(b.x, b.x + b.w);
      ys.push(b.y, b.y + b.h);
      if (centres) {
        xs.push(b.x + b.w / 2);
        ys.push(b.y + b.h / 2);
      }
    }
  };
  walk(s.layers);
  for (const g of snapConfig.guides?.() ?? []) (g.axis === "x" ? xs : ys).push(g.pos);
  return { xs, ys };
}

/** Nearest target to any of `values` within `tol`: the offset to apply, or 0. */
export function snapAxis(values: number[], targets: number[], tol: number): { d: number; hit: boolean } {
  let best = Infinity;
  for (const v of values) {
    for (const t of targets) {
      const d = t - v;
      if (Math.abs(d) <= tol && Math.abs(d) < Math.abs(best)) best = d;
    }
  }
  return Number.isFinite(best) ? { d: best, hit: true } : { d: 0, hit: false };
}

export interface BoxSnap {
  dx: number;
  dy: number;
  guides: Targets;
}

/**
 * Snap a moving box: each axis independently, whichever of its left,
 * centre or right (top, middle, bottom) is nearest a target.
 */
export function snapBox(box: Rect, t: Targets, tol: number, opts: { centres?: boolean } = {}): BoxSnap {
  const centres = opts.centres ?? true;
  const xv = centres ? [box.x, box.x + box.w / 2, box.x + box.w] : [box.x, box.x + box.w];
  const yv = centres ? [box.y, box.y + box.h / 2, box.y + box.h] : [box.y, box.y + box.h];
  const sx = snapAxis(xv, t.xs, tol);
  const sy = snapAxis(yv, t.ys, tol);
  return {
    dx: sx.d,
    dy: sy.d,
    guides: { xs: sx.hit ? hits(xv.map((v) => v + sx.d), t.xs) : [], ys: sy.hit ? hits(yv.map((v) => v + sy.d), t.ys) : [] },
  };
}

/** Targets that coincide with any of `values` after snapping (the lines to draw). */
export function hits(values: number[], targets: number[]): number[] {
  const out = new Set<number>();
  for (const v of values) for (const t of targets) if (Math.abs(t - v) < 0.51) out.add(t);
  return [...out];
}

// ---------------------------------------------------------------------------
// Drawing

/** Photoshop's smart-guide magenta. */
export const GUIDE_COLOR = "#ff2bd6";

/** Lines across the viewport at the snapped document positions. */
export function drawSnapGuides(ed: EditorStore, ctx: CanvasRenderingContext2D, g: Targets | null) {
  if (!g || (!g.xs.length && !g.ys.length)) return;
  const W = ed.viewport.width;
  const H = ed.viewport.height;
  ctx.save();
  ctx.strokeStyle = GUIDE_COLOR;
  ctx.lineWidth = 1;
  ctx.setLineDash([]);
  ctx.beginPath();
  for (const x of g.xs) {
    const px = Math.round(ed.toView(x, 0).x) + 0.5;
    ctx.moveTo(px, 0);
    ctx.lineTo(px, H);
  }
  for (const y of g.ys) {
    const py = Math.round(ed.toView(0, y).y) + 0.5;
    ctx.moveTo(0, py);
    ctx.lineTo(W, py);
  }
  ctx.stroke();
  ctx.restore();
}
