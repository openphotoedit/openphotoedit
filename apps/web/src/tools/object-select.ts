// Object selection: click an object, or drag a box around it. The model
// runs as an AI job; until it lands the tool says so.

import * as ai from "../lib/ai";
import type { EditorStore } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, docRectPath, isNotReady, notReady, rectFrom, redraw, reportError, selectionMode, trackHover, type Pt } from "./common";

let drag: { start: Pt; end: Pt; startV: Pt; moved: boolean; mode: "replace" | "add" | "subtract"; positive: boolean } | null = null;
let running = false;

async function select(ed: EditorStore, points: { x: number; y: number; positive: boolean }[], mode: "replace" | "add" | "subtract") {
  if (running) return;
  running = true;
  try {
    await ai.selectObjectAt(points, mode);
  } catch (e) {
    if (isNotReady(e)) notReady(ed, t("Object selection"));
    else reportError(ed, e);
  } finally {
    running = false;
    redraw(ed);
  }
}

export const objectSelect: Tool = {
  id: "object-select",
  label: "Object selection",
  cursor: "crosshair",
  down(ed, p) {
    const m = ed.summary?.selection && (p.shift || p.alt) ? selectionMode(p, "replace") : toolSettings.selectMode;
    const mode = m === "intersect" ? "add" : m;
    drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, startV: { x: p.vx, y: p.vy }, moved: false, mode, positive: mode !== "subtract" };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) return;
    drag.end = { x: p.x, y: p.y };
    if (Math.hypot(p.vx - drag.startV.x, p.vy - drag.startV.y) > 4) drag.moved = true;
  },
  async up(ed) {
    const d = drag;
    drag = null;
    redraw(ed);
    if (!d) return;
    if (!d.moved) {
      await select(ed, [{ x: d.start.x, y: d.start.y, positive: true }], d.mode);
      return;
    }
    // A box is sent as its two corners, both positive.
    const r = rectFrom(d.start, d.end);
    await select(
      ed,
      [
        { x: r.x, y: r.y, positive: true },
        { x: r.x + r.w, y: r.y + r.h, positive: true },
      ],
      d.mode,
    );
  },
  cancel(ed) {
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (!drag?.moved) return;
    const r = rectFrom(drag.start, drag.end);
    antsStroke(ctx, () => docRectPath(ed, ctx, r));
  },
};
