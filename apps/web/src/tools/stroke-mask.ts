// Painted-stroke helpers shared by the retouching tools: a translucent
// overlay while dragging and a single-channel mask of the stroke.

import type { EditorStore } from "../lib/editor.svelte";
import type { Rect } from "../engine/types";
import type { Pt } from "./common";

/** Draw the stroke as a tinted band in viewport space. */
export function drawStrokeTint(ed: EditorStore, ctx: CanvasRenderingContext2D, points: Pt[], size: number, color: string) {
  if (!points.length) return;
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.lineWidth = Math.max(1, size * ed.view.zoom);
  // One path so overlapping segments do not darken each other.
  const v = points.map((p) => ed.toView(p.x, p.y));
  if (v.length === 1) {
    ctx.beginPath();
    ctx.arc(v[0].x, v[0].y, ctx.lineWidth / 2, 0, Math.PI * 2);
    ctx.fill();
  } else {
    ctx.beginPath();
    ctx.moveTo(v[0].x, v[0].y);
    for (const q of v.slice(1)) ctx.lineTo(q.x, q.y);
    ctx.stroke();
  }
  ctx.restore();
}

/** Rasterise a round-capped stroke into a coverage mask clipped to the document. */
export function strokeMask(points: Pt[], size: number, docW: number, docH: number): { rect: Rect; bytes: Uint8Array } | null {
  if (!points.length) return null;
  const pad = size / 2 + 2;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of points) {
    x0 = Math.min(x0, p.x - pad);
    y0 = Math.min(y0, p.y - pad);
    x1 = Math.max(x1, p.x + pad);
    y1 = Math.max(y1, p.y + pad);
  }
  const rx = Math.max(0, Math.floor(x0));
  const ry = Math.max(0, Math.floor(y0));
  const rw = Math.min(docW, Math.ceil(x1)) - rx;
  const rh = Math.min(docH, Math.ceil(y1)) - ry;
  if (rw <= 0 || rh <= 0) return null;
  const c = new OffscreenCanvas(rw, rh);
  const g = c.getContext("2d", { willReadFrequently: true })!;
  g.translate(-rx, -ry);
  g.strokeStyle = "#fff";
  g.fillStyle = "#fff";
  g.lineCap = "round";
  g.lineJoin = "round";
  g.lineWidth = size;
  if (points.length === 1) {
    g.beginPath();
    g.arc(points[0].x, points[0].y, size / 2, 0, Math.PI * 2);
    g.fill();
  } else {
    g.beginPath();
    g.moveTo(points[0].x, points[0].y);
    for (const p of points.slice(1)) g.lineTo(p.x, p.y);
    g.stroke();
  }
  const d = g.getImageData(0, 0, rw, rh).data;
  const bytes = new Uint8Array(rw * rh);
  for (let i = 0; i < bytes.length; i++) bytes[i] = d[i * 4 + 3];
  return { rect: { x: rx, y: ry, w: rw, h: rh }, bytes };
}
