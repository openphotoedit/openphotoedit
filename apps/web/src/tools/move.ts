// Move: drag the selected layers (or the selected pixels), auto-select the
// layer under the pointer, alt-drag to duplicate (the selected pixels when
// there is a selection, else the layers), arrow keys to nudge. Edges and
// centres snap to the canvas, other layers and guides (Control disables).

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { ancestorIds, antsStroke, clipRect, docRectPath, drawLabel, findLayer, intRect, isUnknownOp, layerAt, redraw, run, seal, selectedLayerIds, trackHover, exec, reportError, setActive, unionBounds, unionRect } from "./common";
import { drawSnapGuides, snapActive, snapBox, snapTargets, snapTolerance, type Targets } from "./snap";
import { allLayers } from "../engine/types";
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
  /** Alt-drag with a selection: copy the selected pixels, leave the source. */
  duplicatePixels: boolean;
  targets: Targets | null;
  guides: Targets | null;
  /** Unsnapped drag, so snapping can be recomputed as the view pans. */
  raw: { dx: number; dy: number };
  /** An `edit.begin` is open (Alt-drag duplicate: copy and move are one step). */
  txn: boolean;
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
  const ids = selectedLayerIds(ed);
  if (!ids.length) return;
  await run(ed, { op: "layer.offset", ids, dx, dy });
}

/** The selected pixels of the active pixel layer can be moved. */
function pixelSelection(ed: EditorStore): Rect | null {
  const sel = ed.summary?.selection?.bounds ?? null;
  return sel && sel.w > 0 && sel.h > 0 && ed.active?.kind === "pixel" ? sel : null;
}

/**
 * Cmd/Ctrl+arrow in any tool, and plain arrows with the Move tool while a
 * selection exists: move the selected pixels (Shift = 10 px).
 */
export async function nudgeSelectedPixels(ed: EditorStore, dx: number, dy: number): Promise<boolean> {
  const sel = pixelSelection(ed);
  const id = ed.summary?.active;
  if (!sel || id == null) return false;
  await moveSelectedPixels(ed, id, sel, dx, dy);
  return true;
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
      duplicatePixels: false,
      targets: null,
      guides: null,
      raw: { dx: 0, dy: 0 },
      txn: false,
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
      let ids = selectedLayerIds(ed);
      const locked = ids.map((id) => findLayer(ed, id)).find((l) => l && (l.locks.all || l.locks.position));
      if (locked) {
        ed.toast(t("{name} is locked. Unlock its position to move it.", { name: locked.name }), "error");
        drag = null;
        return;
      }
      const sel = pixelSelection(ed);
      if (sel) {
        // Photoshop: with a selection the Move tool moves (Alt: copies) the
        // selected pixels of the active layer, never the whole layer.
        d.selection = sel;
        d.duplicatePixels = p.alt;
        ids = [active.id];
      } else if (p.alt) {
        const before = new Set(allLayers(ed.summary?.layers ?? []).map((l) => l.id));
        d.txn = !!(await run(ed, { op: "edit.begin", label: "Duplicate Layer" }, { quiet: true }));
        const r = await run(ed, { op: "layer.duplicate", ids });
        if (r) {
          const fresh = allLayers(ed.summary?.layers ?? []).filter((l) => !before.has(l.id)).map((l) => l.id);
          // Only the top-level copies move; their children follow.
          ids = fresh.filter((id) => !ancestorIds(ed, id).some((a) => fresh.includes(a)));
        }
      }
      d.ids = ids;
      d.bounds = unionBounds(ed, ids);
      d.targets = snapTargets(ed, ids);
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
    drag.raw = { dx, dy };
    drag.guides = null;
    const box = drag.selection ?? drag.bounds;
    if (box && drag.targets && snapActive()) {
      const sn = snapBox({ x: box.x + dx, y: box.y + dy, w: box.w, h: box.h }, drag.targets, snapTolerance(ed));
      // Shift keeps its axis: never snap the locked one.
      if (!(p.shift && dx === 0)) dx += sn.dx;
      else sn.guides.xs = [];
      if (!(p.shift && dy === 0)) dy += sn.dy;
      else sn.guides.ys = [];
      drag.guides = sn.guides;
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
      if (d.duplicatePixels) await duplicateSelectedPixels(ed, d.ids[0], d.selection, d.dx, d.dy);
      else await moveSelectedPixels(ed, d.ids[0], d.selection, d.dx, d.dy);
    } else {
      flush(ed);
      await d.chain;
    }
    if (d.txn) await run(ed, { op: "edit.end" }, { quiet: true });
    drag = null;
    await seal(ed);
    redraw(ed);
  },
  cancel(ed) {
    if (drag?.raf) cancelAnimationFrame(drag.raf);
    const d = drag;
    if (d?.txn) void d.chain.then(() => run(ed, { op: "edit.end" }, { quiet: true }));
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
    if (pixelSelection(ed)) void nudgeSelectedPixels(ed, m[0], m[1]);
    else void nudge(ed, m[0], m[1]);
    return true;
  },
  overlay(ed, ctx) {
    const d = drag;
    const r = d ? (d.selection ?? d.bounds) : null;
    if (!d || !r || !d.moved) return;
    drawSnapGuides(ed, ctx, d.guides);
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
    drawLabel(ctx, `${d.duplicatePixels ? "+ " : ""}Δx ${d.dx}  Δy ${d.dy}`, q.x, q.y);
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

/**
 * Alt-drag with a selection: copy the selected pixels to the new place on the
 * same layer and move the selection with them; the source stays.
 * The engine's `transform.selection-pixels` has no duplicate flag, so this
 * composites in the browser; the copy and the selection move are one
 * "Duplicate Pixels" undo step (an `edit.begin`/`edit.end` transaction).
 */
async function duplicateSelectedPixels(ed: EditorStore, id: number, sel: Rect, dx: number, dy: number) {
  const s = ed.summary;
  if (!s) return;
  const area = intRect(clipRect(unionRect(sel, { ...sel, x: sel.x + dx, y: sel.y + dy }), s.width, s.height));
  if (area.w <= 0 || area.h <= 0) return;
  try {
    const base = await ed.engine.call<Uint8Array>("layer_region", id, area.x, area.y, area.w, area.h);
    const patch = await ed.engine.call<Uint8Array>("layer_region", id, sel.x, sel.y, sel.w, sel.h);
    const cov = await ed.engine.call<Uint8Array>("selection_region", sel.x, sel.y, sel.w, sel.h);
    const out = new Uint8Array(base);
    for (let y = 0; y < sel.h; y++) {
      const ty = sel.y + y + dy - area.y;
      if (ty < 0 || ty >= area.h) continue;
      for (let x = 0; x < sel.w; x++) {
        const tx = sel.x + x + dx - area.x;
        if (tx < 0 || tx >= area.w) continue;
        const k = y * sel.w + x;
        const sa = (patch[k * 4 + 3] * cov[k]) / 255 / 255;
        if (sa <= 0) continue;
        const o = (ty * area.w + tx) * 4;
        const da = out[o + 3] / 255;
        const oa = sa + da * (1 - sa);
        for (let c = 0; c < 3; c++) out[o + c] = Math.round((patch[k * 4 + c] * sa + out[o + c] * da * (1 - sa)) / oa);
        out[o + 3] = Math.round(oa * 255);
      }
    }
    // One undo step: the copy and the selection that follows it.
    await ed.engine.transaction(
      "Duplicate Pixels",
      async () => {
        await exec(ed, { op: "layer.set-pixels", id, x: area.x, y: area.y, width: area.w, height: area.h, label: "Duplicate Pixels" }, out);
        await exec(ed, { op: "select.transform", matrix: { a: 1, b: 0, c: 0, d: 1, e: dx, f: dy } });
      },
      { cancelOnError: true },
    );
  } catch (e) {
    reportError(ed, e);
  }
}
