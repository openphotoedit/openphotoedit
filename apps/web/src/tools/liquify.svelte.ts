// Liquify (Filter › Liquify, Shift+Cmd/Ctrl+X): displacement brushes on the
// active pixel layer. Pointer samples go to `transform.liquify` once per
// animation frame with a shared stroke_id, so the canvas shows the warp live
// and the whole stroke undoes as one step. The engine keeps one displacement
// field per layer and always resamples from the pixels the session started
// with, so strokes never accumulate blur and Reconstruct can undo a warp
// locally. The session is released when the tool is put down.

import { t } from "../lib/i18n";
import { registerTools } from "./registry";
import type { Tool, ToolPointer } from "./types";
import { digitOpacity, drawBrushCircle, exec, FrameBatcher, hover, isUnknownOp, newStrokeId, notReady, redraw, reportError, requirePixels, stepSize, trackHover } from "./common";

export type LiquifyMode = "forward" | "reconstruct" | "twirl-cw" | "twirl-ccw" | "pucker" | "bloat" | "push-left";

export const LIQUIFY_MODES: { value: LiquifyMode; label: string }[] = [
  { value: "forward", label: "Forward warp" },
  { value: "reconstruct", label: "Reconstruct" },
  { value: "twirl-cw", label: "Twirl clockwise" },
  { value: "twirl-ccw", label: "Twirl counterclockwise" },
  { value: "pucker", label: "Pucker" },
  { value: "bloat", label: "Bloat" },
  { value: "push-left", label: "Push left" },
];

interface LiquifySettings {
  mode: LiquifyMode;
  /** Brush diameter, document pixels. */
  size: number;
  /** 0..1: how far each dab moves pixels. */
  pressure: number;
  /** 0..1: how far toward the edge the brush keeps full strength. */
  density: number;
}

const KEY = "ops.liquify";
const DEFAULTS: LiquifySettings = { mode: "forward", size: 120, pressure: 0.5, density: 0.5 };

function load(): LiquifySettings {
  const out = { ...DEFAULTS };
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (v && typeof v === "object") {
      if (LIQUIFY_MODES.some((m) => m.value === v.mode)) out.mode = v.mode;
      for (const k of ["size", "pressure", "density"] as const) if (typeof v[k] === "number" && Number.isFinite(v[k])) out[k] = v[k];
    }
  } catch {
    /* private mode: defaults */
  }
  return out;
}

/** Liquify options; bind to the fields directly. */
export const liquifySettings: LiquifySettings = $state(load());

$effect.root(() => {
  $effect(() => {
    const json = JSON.stringify(liquifySettings);
    try {
      localStorage.setItem(KEY, json);
    } catch {
      /* settings last for this session */
    }
  });
});

export function setLiquifySize(v: number) {
  liquifySettings.size = Math.max(1, Math.min(5000, Math.round(v)));
}

interface Pt3 {
  x: number;
  y: number;
  p: number;
}

interface LiquifyStroke {
  id: string;
  layer: number | null;
  last: Pt3 | null;
  batch: FrameBatcher<Pt3>;
  failed: boolean;
}

let stroke: LiquifyStroke | null = null;
/** Layer ids with a live engine session to release on deactivate. */
const sessions = new Set<number>();

const sp = (p: ToolPointer): Pt3 => ({ x: p.x, y: p.y, p: p.pointerType === "pen" ? p.pressure : 1 });

async function end(ed: Parameters<NonNullable<Tool["down"]>>[0]) {
  const st = stroke;
  stroke = null;
  if (!st) return;
  await st.batch.done();
  redraw(ed);
}

export const liquify: Tool = {
  id: "liquify",
  label: "Liquify",
  cursor: "none",
  down(ed, p) {
    if (!requirePixels(ed)) return;
    const layer = ed.summary?.active ?? null;
    const st: LiquifyStroke = { id: newStrokeId("liquify"), layer, last: null, failed: false, batch: null as unknown as FrameBatcher<Pt3> };
    st.batch = new FrameBatcher<Pt3>(async (pts) => {
      if (st.failed) return;
      // Repeat the previous sample so dab spacing runs on across segments.
      const points = st.last ? [st.last, ...pts] : pts;
      st.last = pts[pts.length - 1];
      const s = liquifySettings;
      try {
        await exec(ed, {
          op: "transform.liquify",
          tool: s.mode,
          size: s.size,
          pressure: Math.round(s.pressure * 100),
          density: Math.round(s.density * 100),
          points,
          stroke_id: st.id,
        });
        if (layer != null) sessions.add(layer);
      } catch (e) {
        st.failed = true;
        if (isUnknownOp(e)) notReady(ed, t("Liquify"));
        else reportError(ed, e);
      }
    });
    stroke = st;
    st.batch.push(sp(p));
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (pressed && stroke) stroke.batch.push(sp(p));
  },
  up(ed) {
    return end(ed);
  },
  cancel(ed) {
    if (stroke) void end(ed);
    redraw(ed);
  },
  deactivate(ed) {
    if (stroke) void end(ed);
    // Free the engine's displacement fields; the pixels stay as they are.
    for (const id of sessions) void exec(ed, { op: "transform.liquify-end", id }).catch(() => null);
    sessions.clear();
  },
  key(ed, e) {
    if (e.code === "BracketLeft" || e.code === "BracketRight") {
      setLiquifySize(stepSize(liquifySettings.size, e.code === "BracketRight" ? 1 : -1));
      redraw(ed);
      return true;
    }
    const o = digitOpacity(e);
    if (o != null) {
      liquifySettings.pressure = o;
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    if (!hover.inside) return;
    const r = (liquifySettings.size / 2) * ed.view.zoom;
    drawBrushCircle(ctx, hover.vx, hover.vy, r, r * 2 < 6);
    // The inner ring shows where the brush keeps full strength.
    const inner = r * liquifySettings.density * 0.75;
    if (inner > 3) {
      ctx.save();
      ctx.setLineDash([3, 3]);
      ctx.strokeStyle = "rgba(255,255,255,0.7)";
      ctx.beginPath();
      ctx.arc(hover.vx, hover.vy, inner, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }
  },
};

registerTools(liquify);
