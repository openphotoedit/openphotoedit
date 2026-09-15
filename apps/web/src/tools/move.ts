// Move: drag the active layer (or the selected pixels), auto-select the
// layer under the pointer, alt-drag to duplicate, arrow keys to nudge.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, docRectPath, drawLabel, isUnknownOp, layerAt, redraw, run, seal, trackHover, exec, reportError, setActive } from "./common";
import { transformPixelsInBrowser, transformSelectionInBrowser } from "./raster";

interface Drag {
  sx: number;
  sy: number;
  dx: number;
  dy: number;
  sentDx: number;
  sentDy: number;
  ids: number[];
  ready: boolean;
  /** Moving selected pixels rather than the whole layer. */
  selection: Rect | null;
  bounds: Rect | null;
  chain: Promise<unknown>;
  raf: number;
  moved: boolean;
}

let drag: Drag | null = null;

function flush(ed: EditorStore) {
  const d = drag;
  if (!d || !d.ready || d.selection) return;
  d.raf = 0;
  const ddx = d.dx - d.sentDx;
  const ddy = d.dy - d.sentDy;
  if (!ddx && !ddy) return;
  d.sentDx = d.dx;
  d.sentDy = d.dy;
  d.chain = d.chain.then(() => run(ed, { op: "layer.offset", ids: d.ids, dx: ddx, dy: ddy }));
}

function schedule(ed: EditorStore) {
  if (drag && !drag.raf) drag.raf = requestAnimationFrame(() => flush(ed));
}

async function nudge(ed: EditorStore, dx: number, dy: number) {
  const id = ed.summary?.active;
  if (id == null) return;
  await run(ed, { op: "layer.offset", ids: [id], dx, dy });
}

export const move: Tool = {
  id: "move",
  label: "Move",
  shortcut: "v",
  cursor: "move",
  down(ed, p) {
    const s = ed.summary;
    if (!s) return;
    const d: Drag = {
      sx: p.x,
      sy: p.y,
      dx: 0,
      dy: 0,
      sentDx: 0,
      sentDy: 0,
      ids: s.active != null ? [s.active] : [],
      ready: false,
      selection: null,
      bounds: ed.active?.bounds ?? null,
      chain: Promise.resolve(),
      raf: 0,
      moved: false,
    };
    drag = d;
    void (async () => {
      // Cmd/Ctrl inverts auto-select for this click, as in Photoshop.
      const auto = toolSettings.autoSelect !== p.mod;
      if (auto) {
        const hit = await layerAt(ed, p.x, p.y, (l) => l.kind !== "adjustment" && l.kind !== "group" && !l.locks.all);
        if (hit && hit.id !== ed.summary?.active) {
          await setActive(ed, hit.id);
        }
      }
      const active = ed.active;
      if (!active || drag !== d) {
        if (drag === d) drag = null;
        return;
      }
      if (active.locks.all || active.locks.position) {
        ed.toast(t("{name} is locked. Unlock its position to move it.", { name: active.name }), "error");
        drag = null;
        return;
      }
      if (p.alt) {
        await run(ed, { op: "layer.duplicate", ids: [active.id] });
      }
      const target = ed.active!;
      d.ids = [target.id];
      d.bounds = target.bounds ?? null;
      const sel = ed.summary?.selection?.bounds ?? null;
      if (sel && !p.alt && target.kind === "pixel") d.selection = sel;
      d.ready = true;
      schedule(ed);
      redraw(ed);
    })();
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) return;
    let dx = p.x - drag.sx;
    let dy = p.y - drag.sy;
    if (p.shift) {
      // Constrain to horizontal, vertical or 45°.
      const ax = Math.abs(dx);
      const ay = Math.abs(dy);
      if (ax > ay * 2) dy = 0;
      else if (ay > ax * 2) dx = 0;
      else {
        const m = Math.max(ax, ay);
        dx = Math.sign(dx) * m;
        dy = Math.sign(dy) * m;
      }
    }
    drag.dx = Math.round(dx);
    drag.dy = Math.round(dy);
    if (drag.dx || drag.dy) drag.moved = true;
    schedule(ed);
  },
  async up(ed) {
    const d = drag;
    if (!d) return;
    // Wait for the auto-select/duplicate step to settle.
    for (let i = 0; i < 50 && !d.ready && drag === d; i++) await new Promise((r) => setTimeout(r, 10));
    if (d.raf) cancelAnimationFrame(d.raf);
    if (d.selection && d.ready && (d.dx || d.dy)) {
      await moveSelectedPixels(ed, d.ids[0], d.selection, d.dx, d.dy);
    } else {
      flush(ed);
      await d.chain;
    }
    drag = null;
    await seal(ed);
    redraw(ed);
  },
  cancel(ed) {
    if (drag?.raf) cancelAnimationFrame(drag.raf);
    drag = null;
    redraw(ed);
  },
  key(ed, e) {
    const step = e.shiftKey ? 10 : 1;
    const map: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const m = map[e.key];
    if (!m || e.metaKey || e.ctrlKey || e.altKey) return false;
    void nudge(ed, m[0], m[1]);
    return true;
  },
  overlay(ed, ctx) {
    const d = drag;
    const r = d ? (d.selection ?? d.bounds) : null;
    if (!d || !r || !d.moved) return;
    // Where the content is going: the engine catches up a frame later.
    const shown = { x: r.x + d.dx, y: r.y + d.dy, w: r.w, h: r.h };
    ctx.save();
    ctx.strokeStyle = "#1473e6";
    ctx.lineWidth = 1;
    ctx.beginPath();
    docRectPath(ed, ctx, shown);
    ctx.stroke();
    ctx.restore();
    if (d.selection) antsStroke(ctx, () => docRectPath(ed, ctx, shown));
    const q = ed.toView(shown.x + shown.w, shown.y + shown.h);
    drawLabel(ctx, `Δx ${d.dx}  Δy ${d.dy}`, q.x, q.y);
  },
};

async function moveSelectedPixels(ed: EditorStore, id: number, sel: Rect, dx: number, dy: number) {
  const matrix = { a: 1, b: 0, c: 0, d: 1, e: dx, f: dy };
  try {
    await exec(ed, { op: "transform.selection-pixels", id, matrix });
  } catch (e) {
    if (!isUnknownOp(e)) {
      reportError(ed, e);
      return;
    }
    const ok = await transformPixelsInBrowser(ed, id, sel, { matrix }, { selectionOnly: true, label: "Move" });
    if (ok) await transformSelectionInBrowser(ed, { matrix });
  }
}
