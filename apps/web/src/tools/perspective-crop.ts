// Perspective crop: drag a box, then drag its four corners onto the edges
// of something photographed at an angle. Enter flattens it.

import type { EditorStore } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolState } from "./state.svelte";
import { dist, drawHandle, drawLabel, handleRadius, isCoarse, isUnknownOp, notReady, rectFrom, redraw, setCursor, trackHover, type Pt, exec, reportError } from "./common";

let quad: Pt[] | null = null;
let drag: { kind: "draw" | "corner" | "move"; index: number; start: Pt; orig: Pt[] } | null = null;

function inside(q: Pt[], p: Pt) {
  let c = false;
  for (let i = 0, j = q.length - 1; i < q.length; j = i++) {
    if (q[i].y > p.y !== q[j].y > p.y && p.x < ((q[j].x - q[i].x) * (p.y - q[i].y)) / (q[j].y - q[i].y) + q[i].x) c = !c;
  }
  return c;
}

function outputSize(q: Pt[]) {
  const w = (dist(q[0], q[1]) + dist(q[3], q[2])) / 2;
  const h = (dist(q[0], q[3]) + dist(q[1], q[2])) / 2;
  return { w: Math.max(1, Math.round(w)), h: Math.max(1, Math.round(h)) };
}

function sync() {
  toolState.perspectivePending = !!quad;
}

export async function commitPerspectiveCrop(ed: EditorStore) {
  if (!quad) return;
  const q = quad;
  const { w, h } = outputSize(q);
  try {
    await exec(ed, { op: "transform.perspective-crop", quad: q.map((p) => ({ x: p.x, y: p.y })), width: w, height: h });
  } catch (e) {
    if (isUnknownOp(e)) notReady(ed, t("Perspective crop"));
    else reportError(ed, e);
    return;
  }
  quad = null;
  sync();
  ed.fit();
  redraw(ed);
}

export function cancelPerspectiveCrop(ed: EditorStore) {
  quad = null;
  drag = null;
  sync();
  redraw(ed);
}

export const perspectiveCrop: Tool = {
  id: "perspective-crop",
  label: "Perspective crop",
  cursor: "crosshair",
  deactivate(ed) {
    cancelPerspectiveCrop(ed);
  },
  down(ed, p) {
    const pt = { x: p.x, y: p.y };
    if (quad) {
      const rad = handleRadius(p.pointerType) + 2;
      const idx = quad.findIndex((c) => {
        const v = ed.toView(c.x, c.y);
        return Math.hypot(v.x - p.vx, v.y - p.vy) <= rad;
      });
      if (idx >= 0) {
        drag = { kind: "corner", index: idx, start: pt, orig: quad.map((c) => ({ ...c })) };
        return;
      }
      if (inside(quad, pt)) {
        drag = { kind: "move", index: -1, start: pt, orig: quad.map((c) => ({ ...c })) };
        return;
      }
    }
    drag = { kind: "draw", index: -1, start: pt, orig: [] };
    quad = null;
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) {
      if (quad) {
        const rad = handleRadius(p.pointerType) + 2;
        const onCorner = quad.some((c) => {
          const v = ed.toView(c.x, c.y);
          return Math.hypot(v.x - p.vx, v.y - p.vy) <= rad;
        });
        setCursor(onCorner ? "pointer" : inside(quad, p) ? "move" : "crosshair");
      }
      return;
    }
    const d = drag;
    if (d.kind === "draw") {
      const r = rectFrom(d.start, p);
      if (r.w * ed.view.zoom < 3 && r.h * ed.view.zoom < 3) return;
      quad = [
        { x: r.x, y: r.y },
        { x: r.x + r.w, y: r.y },
        { x: r.x + r.w, y: r.y + r.h },
        { x: r.x, y: r.y + r.h },
      ];
    } else if (d.kind === "corner" && quad) {
      quad[d.index] = { x: p.x, y: p.y };
    } else if (d.kind === "move" && quad) {
      const dx = p.x - d.start.x;
      const dy = p.y - d.start.y;
      quad = d.orig.map((c) => ({ x: c.x + dx, y: c.y + dy }));
    }
    sync();
  },
  up(ed) {
    drag = null;
    if (quad) {
      const { w, h } = outputSize(quad);
      if (w < 2 || h < 2) quad = null;
    }
    sync();
    redraw(ed);
  },
  cancel(ed) {
    cancelPerspectiveCrop(ed);
  },
  key(ed, e) {
    if (e.key === "Enter" && quad) {
      void commitPerspectiveCrop(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    if (!quad) return;
    const v = quad.map((c) => ed.toView(c.x, c.y));
    ctx.save();
    ctx.fillStyle = "rgba(0,0,0,0.5)";
    ctx.beginPath();
    ctx.rect(0, 0, ed.viewport.width, ed.viewport.height);
    ctx.moveTo(v[0].x, v[0].y);
    for (let i = 3; i >= 0; i--) ctx.lineTo(v[i].x, v[i].y);
    ctx.closePath();
    ctx.fill("evenodd");
    // Perspective grid (bilinear), so the user can line it up with edges.
    ctx.strokeStyle = "rgba(255,255,255,0.6)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    const lerp = (a: Pt, b: Pt, k: number) => ({ x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k });
    for (let i = 1; i < 4; i++) {
      const k = i / 4;
      const a = lerp(v[0], v[1], k);
      const b = lerp(v[3], v[2], k);
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      const c = lerp(v[0], v[3], k);
      const d = lerp(v[1], v[2], k);
      ctx.moveTo(c.x, c.y);
      ctx.lineTo(d.x, d.y);
    }
    ctx.stroke();
    ctx.strokeStyle = "#ffffff";
    ctx.beginPath();
    ctx.moveTo(v[0].x, v[0].y);
    for (const q of v.slice(1)) ctx.lineTo(q.x, q.y);
    ctx.closePath();
    ctx.stroke();
    for (const q of v) drawHandle(ctx, q.x, q.y, isCoarse() ? 16 : 10, true);
    const { w, h } = outputSize(quad);
    drawLabel(ctx, `${w} × ${h} px`, v[2].x, v[2].y);
    ctx.restore();
  },
};

