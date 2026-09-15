// Rectangular and elliptical marquees. Modifiers held at pointer-down pick
// the combine mode (shift add, alt subtract, both intersect); modifiers
// pressed during the drag constrain (shift square) and centre (alt).
// Dragging inside an existing selection with no modifier moves its outline.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, docEllipsePath, docRectPath, drawLabel, inRect, redraw, run, selectionMode, trackHover, type Pt } from "./common";

interface MarqueeDrag {
  start: Pt;
  end: Pt;
  mode: "replace" | "add" | "subtract" | "intersect";
  /** Modifiers at pointer-down were consumed by the mode. */
  modeFromKeys: boolean;
  square: boolean;
  centre: boolean;
  moved: boolean;
  /** Dragging the existing selection outline. */
  moveSel: Rect | null;
  shiftFree: boolean;
  altFree: boolean;
}

let drag: MarqueeDrag | null = null;

export function marqueeBox(d: { start: Pt; end: Pt; square: boolean; centre: boolean }): Rect {
  let dx = d.end.x - d.start.x;
  let dy = d.end.y - d.start.y;
  if (d.square) {
    const m = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * m;
    dy = Math.sign(dy || 1) * m;
  }
  if (d.centre) return { x: d.start.x - Math.abs(dx), y: d.start.y - Math.abs(dy), w: Math.abs(dx) * 2, h: Math.abs(dy) * 2 };
  return { x: Math.min(d.start.x, d.start.x + dx), y: Math.min(d.start.y, d.start.y + dy), w: Math.abs(dx), h: Math.abs(dy) };
}

/** Move the whole selection outline by whole pixels. */
export async function offsetSelection(ed: EditorStore, dx: number, dy: number) {
  const b = ed.summary?.selection?.bounds;
  if (!b || (!dx && !dy)) return;
  const bytes = await ed.engine.call<Uint8Array>("selection_region", b.x, b.y, b.w, b.h);
  await run(ed, { op: "select.mask", x: b.x + dx, y: b.y + dy, width: b.w, height: b.h, mode: "replace", label: "Move Selection" }, { bytes });
}

function makeMarquee(id: "marquee-rect" | "marquee-ellipse", label: string, shortcut: string | undefined): Tool {
  const ellipse = id === "marquee-ellipse";
  return {
    id,
    label,
    shortcut,
    cursor: "crosshair",
    down(ed, p) {
      const hasSel = !!ed.summary?.selection;
      const keysMode = hasSel && (p.shift || p.alt);
      const mode = keysMode ? selectionMode(p, "replace") : (toolSettings.selectMode as MarqueeDrag["mode"]);
      const sel = ed.summary?.selection?.bounds ?? null;
      const moveSel = sel && !p.shift && !p.alt && mode === "replace" && inRect(sel, p) ? sel : null;
      drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, mode, modeFromKeys: keysMode, square: false, centre: false, moved: false, moveSel, shiftFree: !keysMode || !p.shift, altFree: !keysMode || !p.alt };
    },
    move(ed, p, pressed) {
      trackHover(ed, p);
      if (!pressed || !drag) return;
      drag.end = { x: p.x, y: p.y };
      const sv = ed.toView(drag.start.x, drag.start.y);
      if (Math.hypot(p.vx - sv.x, p.vy - sv.y) > 3) drag.moved = true;
      // Keys held to choose the mode constrain only once released and
      // pressed again, as in Photoshop.
      if (!p.shift) drag.shiftFree = true;
      if (!p.alt) drag.altFree = true;
      drag.square = p.shift && drag.shiftFree;
      drag.centre = p.alt && drag.altFree;
    },
    async up(ed, p) {
      const d = drag;
      drag = null;
      if (!d) return;
      d.end = { x: p.x, y: p.y };
      if (d.moveSel && d.moved) {
        await offsetSelection(ed, Math.round(d.end.x - d.start.x), Math.round(d.end.y - d.start.y));
        return;
      }
      const box = marqueeBox(d);
      if (!d.moved || box.w < 1 || box.h < 1) {
        if (d.mode === "replace") await run(ed, { op: "select.none" });
        redraw(ed);
        return;
      }
      await run(ed, {
        op: ellipse ? "select.ellipse" : "select.rect",
        x: box.x,
        y: box.y,
        width: box.w,
        height: box.h,
        mode: d.mode,
        feather: toolSettings.feather,
        ...(ellipse ? { anti_alias: toolSettings.antiAlias } : {}),
      });
      redraw(ed);
    },
    cancel(ed) {
      drag = null;
      redraw(ed);
    },
    key(ed, e) {
      if (e.key === "Escape") return false;
      // Arrow keys move the selection outline.
      const step = e.shiftKey ? 10 : 1;
      const map: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
      const m = map[e.key];
      if (!m || !ed.summary?.selection || e.metaKey || e.ctrlKey || e.altKey) return false;
      void offsetSelection(ed, m[0], m[1]);
      return true;
    },
    overlay(ed, ctx) {
      const d = drag;
      if (!d || !d.moved) return;
      if (d.moveSel) {
        const r = { ...d.moveSel, x: d.moveSel.x + Math.round(d.end.x - d.start.x), y: d.moveSel.y + Math.round(d.end.y - d.start.y) };
        antsStroke(ctx, () => docRectPath(ed, ctx, r));
        return;
      }
      const box = marqueeBox(d);
      antsStroke(ctx, () => (ellipse ? docEllipsePath(ed, ctx, box) : docRectPath(ed, ctx, box)));
      const q = ed.toView(box.x + box.w, box.y + box.h);
      const sign = d.mode === "add" ? "+ " : d.mode === "subtract" ? "− " : d.mode === "intersect" ? "× " : "";
      drawLabel(ctx, `${sign}W ${Math.round(box.w)}  H ${Math.round(box.h)}`, q.x, q.y);
    },
  };
}

export const marqueeRect = makeMarquee("marquee-rect", "Rectangular marquee", "m");
export const marqueeEllipse = makeMarquee("marquee-ellipse", "Elliptical marquee", undefined);
