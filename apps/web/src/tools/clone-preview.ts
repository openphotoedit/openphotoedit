// Clone stamp preview: the source pixels drawn inside the brush circle
// before painting, shaped by the brush tip and at the brush opacity, so you
// can line up the clone. Compositor does the same (BrushCursorOverlay.swift,
// MIT). Pixels come from the engine's unrecorded region reads and are cached
// per source rectangle and document revision.

import type { EditorStore } from "../lib/editor.svelte";
import type { Pt } from "./common";
import { redraw } from "./common";

/** Larger brushes skip the preview: reading that many pixels per move is not free. */
const MAX_SIDE = 600;

let cache: { key: string; img: OffscreenCanvas } | null = null;
let pending = "";

function keyFor(ed: EditorStore, x: number, y: number, side: number, all: boolean) {
  return `${x},${y},${side},${all ? "m" : ed.summary?.active},${ed.summary?.revision}`;
}

async function fetchRegion(ed: EditorStore, key: string, x: number, y: number, side: number, all: boolean) {
  pending = key;
  try {
    const id = ed.summary?.active;
    const bytes = all || id == null ? await ed.engine.call<Uint8Array>("region", x, y, side, side) : await ed.engine.call<Uint8Array>("layer_region", id, x, y, side, side);
    if (pending !== key) return;
    const img = new OffscreenCanvas(side, side);
    img.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(bytes.subarray(0, side * side * 4)), side, side), 0, 0);
    cache = { key, img };
    redraw(ed);
  } catch {
    /* no preview */
  } finally {
    if (pending === key) pending = "";
  }
}

/**
 * Draw the pixels around `src` inside the brush at `dst` (document points).
 * `hardness` shapes the edge like the brush tip; `opacity` is the brush's.
 */
export function drawClonePreview(ed: EditorStore, ctx: CanvasRenderingContext2D, src: Pt, dst: Pt, size: number, hardness: number, opacity: number, sampleAll: boolean) {
  const side = Math.ceil(size) + 2;
  if (side > MAX_SIDE || size * ed.view.zoom < 8) return;
  const x = Math.round(src.x - side / 2);
  const y = Math.round(src.y - side / 2);
  const key = keyFor(ed, x, y, side, sampleAll);
  if (cache?.key !== key) {
    if (pending !== key) void fetchRegion(ed, key, x, y, side, sampleAll);
    if (!cache || !cache.key.endsWith(`,${side},${sampleAll ? "m" : ed.summary?.active},${ed.summary?.revision}`)) return;
  }
  const img = cache.img;
  // Where the fetched square lands at the destination (it may be the
  // previous square while the new one loads).
  const [cx, cy] = cache.key.split(",").map(Number);
  const dx = dst.x - src.x;
  const dy = dst.y - src.y;
  const z = ed.view.zoom;
  const tl = ed.toView(cx + dx, cy + dy);
  const c = ed.toView(dst.x, dst.y);
  const r = (size / 2) * z;
  ctx.save();
  // Tip shape: solid to the hardness radius, fading to the edge.
  const layer = new OffscreenCanvas(Math.ceil(2 * r) + 2, Math.ceil(2 * r) + 2);
  const g = layer.getContext("2d")!;
  const ox = c.x - r - 1;
  const oy = c.y - r - 1;
  g.imageSmoothingEnabled = z < 1;
  g.drawImage(img, tl.x - ox, tl.y - oy, side * z, side * z);
  g.globalCompositeOperation = "destination-in";
  const grad = g.createRadialGradient(r + 1, r + 1, Math.max(0, Math.min(0.999, hardness)) * r, r + 1, r + 1, r);
  grad.addColorStop(0, "rgba(0,0,0,1)");
  grad.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = grad;
  g.fillRect(0, 0, layer.width, layer.height);
  ctx.globalAlpha = Math.max(0.05, Math.min(1, opacity)) * 0.85;
  ctx.drawImage(layer, ox, oy);
  ctx.restore();
}
