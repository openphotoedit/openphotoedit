// Liquify (Filter › Liquify, Shift+Cmd/Ctrl+X): displacement brushes on the
// active pixel layer. Pointer samples go to `transform.liquify` once per
// animation frame with a shared stroke_id, so the canvas shows the warp live
// and each stroke undoes as one step. The engine keeps one displacement
// field per layer and always resamples from the pixels the session started
// with, so strokes never accumulate blur and Reconstruct can undo a warp
// locally.
//
// Like a filter dialog, Liquify is a modal session: the strokes are a
// preview until OK or Enter keeps them as ONE history step, and Cancel or
// Escape puts the layer back exactly (`edit.squash`). Switching to another
// tool keeps them, as it does for Free Transform. OK and Cancel return to
// the tool that was in use before.

import type { EditorStore } from "../lib/editor.svelte";
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
/** Layer ids with a live engine session to release when Liquify ends. */
const sessions = new Set<number>();

/** The open Liquify session: the undo depth and document it began on. */
let session: { base: number; tab: number } | null = null;
/** The tool to go back to after OK or Cancel. */
let returnTo = "move";

/** For the options bar: whether a session is open, and whether it is closing. */
export const liquifySession = $state({ open: false, closing: false });

/** Filter › Liquify: remember the current tool, then pick up Liquify. */
export function openLiquify(ed: EditorStore) {
  if (ed.tool !== "liquify") returnTo = ed.tool;
  ed.tool = "liquify";
}

function begin(ed: EditorStore) {
  session = { base: ed.summary?.history.undo.length ?? 0, tab: ed.currentTab };
  liquifySession.open = true;
}

/**
 * End the session: keep its strokes as one step, or take them all back.
 * Steps that are not Liquify strokes (something else ran meanwhile) are
 * never folded in or thrown away; the strokes then stay as they are.
 */
async function finish(ed: EditorStore, keep: boolean) {
  const s = session;
  session = null;
  liquifySession.open = false;
  if (stroke) await end(ed);
  for (const id of sessions) await exec(ed, { op: "transform.liquify-end", id }).catch(() => null);
  sessions.clear();
  if (!s || s.tab !== ed.currentTab) return;
  const ours = ed.summary?.history.undo.slice(s.base) ?? [];
  if (!ours.length || !ours.every((l) => l === "Liquify")) return;
  try {
    await ed.engine.exec({ op: "edit.squash", index: s.base, label: "Liquify", discard: !keep });
    if (keep) ed.dirty = true;
  } catch (e) {
    reportError(ed, e, t("Liquify"));
  }
  redraw(ed);
}

/** OK / Enter (`keep`) or Cancel / Escape: close Liquify and go back to the previous tool. */
export async function closeLiquify(ed: EditorStore, keep: boolean) {
  if (!session || liquifySession.closing) return;
  liquifySession.closing = true;
  try {
    await finish(ed, keep);
  } finally {
    liquifySession.closing = false;
  }
  if (ed.tool === "liquify") ed.tool = returnTo === "liquify" ? "move" : returnTo;
}

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
  activate(ed) {
    begin(ed);
  },
  down(ed, p) {
    if (liquifySession.closing || !requirePixels(ed)) return;
    if (!session) begin(ed);
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
    // Another tool picked up mid-session keeps the strokes, as one step.
    if (session) void finish(ed, true);
  },
  key(ed, e) {
    if (e.key === "Enter" || e.key === "Escape") {
      void closeLiquify(ed, e.key === "Enter");
      return true;
    }
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
