// Browser-side pixel transforms, used when the transform backend has not
// landed (or for layer kinds it cannot take). Each ends in one ordinary
// `layer.set-pixels` command, so it is still a single undo step.

import type { EditorStore } from "../lib/editor.svelte";
import type { Point, Rect } from "../engine/types";
import { run } from "./common";

export interface Matrix {
  a: number;
  b: number;
  c: number;
  d: number;
  e: number;
  f: number;
}

export const IDENTITY: Matrix = { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 };

export function applyM(m: Matrix, p: Point): Point {
  return { x: m.a * p.x + m.c * p.y + m.e, y: m.b * p.x + m.d * p.y + m.f };
}

/** `m` applied after `n`. */
export function mul(m: Matrix, n: Matrix): Matrix {
  return {
    a: m.a * n.a + m.c * n.b,
    b: m.b * n.a + m.d * n.b,
    c: m.a * n.c + m.c * n.d,
    d: m.b * n.c + m.d * n.d,
    e: m.a * n.e + m.c * n.f + m.e,
    f: m.b * n.e + m.d * n.f + m.f,
  };
}

export function invert(m: Matrix): Matrix | null {
  const det = m.a * m.d - m.b * m.c;
  if (Math.abs(det) < 1e-12) return null;
  return {
    a: m.d / det,
    b: -m.b / det,
    c: -m.c / det,
    d: m.a / det,
    e: (m.c * m.f - m.d * m.e) / det,
    f: (m.b * m.e - m.a * m.f) / det,
  };
}

export function isIdentity(m: Matrix, eps = 1e-6) {
  return Math.abs(m.a - 1) < eps && Math.abs(m.b) < eps && Math.abs(m.c) < eps && Math.abs(m.d - 1) < eps && Math.abs(m.e) < eps && Math.abs(m.f) < eps;
}

/** Affine map taking triangle (s0,s1,s2) onto (d0,d1,d2). */
export function triangleMatrix(s: Point[], d: Point[]): Matrix | null {
  const src: Matrix = { a: s[1].x - s[0].x, b: s[1].y - s[0].y, c: s[2].x - s[0].x, d: s[2].y - s[0].y, e: s[0].x, f: s[0].y };
  const dst: Matrix = { a: d[1].x - d[0].x, b: d[1].y - d[0].y, c: d[2].x - d[0].x, d: d[2].y - d[0].y, e: d[0].x, f: d[0].y };
  const inv = invert(src);
  return inv ? mul(dst, inv) : null;
}

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/**
 * Draw `img` (covering `src` in its own coordinates) so that its corners
 * land on `quad` (TL, TR, BR, BL). A true perspective needs a mesh; two
 * affine triangles per cell of an n×n grid is close enough for previews and
 * for a fallback.
 */
export function drawQuad(ctx: Ctx, img: CanvasImageSource, iw: number, ih: number, quad: Point[], n = 8) {
  const bil = (u: number, v: number): Point => {
    const top = { x: quad[0].x + (quad[1].x - quad[0].x) * u, y: quad[0].y + (quad[1].y - quad[0].y) * u };
    const bot = { x: quad[3].x + (quad[2].x - quad[3].x) * u, y: quad[3].y + (quad[2].y - quad[3].y) * u };
    return { x: top.x + (bot.x - top.x) * v, y: top.y + (bot.y - top.y) * v };
  };
  for (let j = 0; j < n; j++) {
    for (let i = 0; i < n; i++) {
      const u0 = i / n;
      const u1 = (i + 1) / n;
      const v0 = j / n;
      const v1 = (j + 1) / n;
      const s = [
        { x: u0 * iw, y: v0 * ih },
        { x: u1 * iw, y: v0 * ih },
        { x: u1 * iw, y: v1 * ih },
        { x: u0 * iw, y: v1 * ih },
      ];
      const d = [bil(u0, v0), bil(u1, v0), bil(u1, v1), bil(u0, v1)];
      for (const tri of [[0, 1, 2], [0, 2, 3]]) {
        const m = triangleMatrix(tri.map((k) => s[k]), tri.map((k) => d[k]));
        if (!m) continue;
        ctx.save();
        // Clip to the destination triangle, grown a hair to hide seams.
        const c = { x: (d[tri[0]].x + d[tri[1]].x + d[tri[2]].x) / 3, y: (d[tri[0]].y + d[tri[1]].y + d[tri[2]].y) / 3 };
        ctx.beginPath();
        tri.forEach((k, idx) => {
          const q = d[k];
          const len = Math.hypot(q.x - c.x, q.y - c.y) || 1;
          const px = q.x + ((q.x - c.x) / len) * 0.6;
          const py = q.y + ((q.y - c.y) / len) * 0.6;
          if (idx === 0) ctx.moveTo(px, py);
          else ctx.lineTo(px, py);
        });
        ctx.closePath();
        ctx.clip();
        ctx.transform(m.a, m.b, m.c, m.d, m.e, m.f);
        ctx.drawImage(img, 0, 0);
        ctx.restore();
      }
    }
  }
}

export function quadBounds(q: Point[]): Rect {
  const xs = q.map((p) => p.x);
  const ys = q.map((p) => p.y);
  const x = Math.floor(Math.min(...xs));
  const y = Math.floor(Math.min(...ys));
  return { x, y, w: Math.ceil(Math.max(...xs)) - x, h: Math.ceil(Math.max(...ys)) - y };
}

function union(a: Rect, b: Rect): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
}

function toCanvas(rgba: Uint8Array | Uint8ClampedArray, w: number, h: number) {
  const c = new OffscreenCanvas(w, h);
  c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), w, h), 0, 0);
  return c;
}

const MAX_FALLBACK_PIXELS = 64_000_000;

/**
 * Transform a pixel layer's content (the part inside `src`) by `matrix` or
 * onto `quad`, writing the result with one `layer.set-pixels`. When
 * `selection` is given only selected pixels move; the rest stay put.
 */
export async function transformPixelsInBrowser(
  ed: EditorStore,
  id: number,
  src: Rect,
  target: { matrix?: Matrix; quad?: Point[] },
  opts: { selectionOnly?: boolean; label?: string } = {},
): Promise<boolean> {
  const quad = target.quad ?? [
    applyM(target.matrix!, { x: src.x, y: src.y }),
    applyM(target.matrix!, { x: src.x + src.w, y: src.y }),
    applyM(target.matrix!, { x: src.x + src.w, y: src.y + src.h }),
    applyM(target.matrix!, { x: src.x, y: src.y + src.h }),
  ];
  const dst = quadBounds(quad);
  const area = union(src, dst);
  if (area.w * area.h > MAX_FALLBACK_PIXELS || src.w <= 0 || src.h <= 0) {
    ed.toast("This transform is too large to preview in the browser yet.", "error");
    return false;
  }
  const layerPx = await ed.engine.call<Uint8Array>("layer_region", id, area.x, area.y, area.w, area.h);
  const out = new OffscreenCanvas(area.w, area.h);
  const g = out.getContext("2d")!;
  // Source patch: the layer inside `src`, masked by the selection if asked.
  const patchPx = await ed.engine.call<Uint8Array>("layer_region", id, src.x, src.y, src.w, src.h);
  let sel: Uint8Array | null = null;
  if (opts.selectionOnly) {
    sel = await ed.engine.call<Uint8Array>("selection_region", src.x, src.y, src.w, src.h);
    for (let i = 0; i < sel.length; i++) patchPx[i * 4 + 3] = Math.round((patchPx[i * 4 + 3] * sel[i]) / 255);
  }
  // Base: everything that stays behind.
  if (sel) {
    const base = new Uint8ClampedArray(layerPx);
    for (let y = 0; y < src.h; y++) {
      for (let x = 0; x < src.w; x++) {
        const o = ((y + src.y - area.y) * area.w + (x + src.x - area.x)) * 4 + 3;
        base[o] = Math.round((base[o] * (255 - sel[y * src.w + x])) / 255);
      }
    }
    g.putImageData(new ImageData(base, area.w, area.h), 0, 0);
  }
  const patch = toCanvas(patchPx, src.w, src.h);
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  g.save();
  g.translate(-area.x, -area.y);
  if (target.matrix) {
    const m = target.matrix;
    g.transform(m.a, m.b, m.c, m.d, m.e, m.f);
    g.drawImage(patch, src.x, src.y);
  } else {
    drawQuad(g, patch, src.w, src.h, quad, 12);
  }
  g.restore();
  const data = g.getImageData(0, 0, area.w, area.h).data;
  const r = await run(ed, { op: "layer.set-pixels", id, x: area.x, y: area.y, width: area.w, height: area.h, label: opts.label ?? "Free Transform" }, { bytes: data });
  return !!r;
}

/** Selection coverage moved by `matrix`, as a `select.mask` command. */
export async function transformSelectionInBrowser(ed: EditorStore, target: { matrix?: Matrix; quad?: Point[] }) {
  const s = ed.summary?.selection;
  if (!s) return;
  const src = s.bounds;
  const cov = await ed.engine.call<Uint8Array>("selection_region", src.x, src.y, src.w, src.h);
  const rgba = new Uint8ClampedArray(src.w * src.h * 4);
  for (let i = 0; i < cov.length; i++) {
    rgba[i * 4 + 3] = cov[i];
  }
  const quad = target.quad ?? [
    applyM(target.matrix!, { x: src.x, y: src.y }),
    applyM(target.matrix!, { x: src.x + src.w, y: src.y }),
    applyM(target.matrix!, { x: src.x + src.w, y: src.y + src.h }),
    applyM(target.matrix!, { x: src.x, y: src.y + src.h }),
  ];
  const dst = quadBounds(quad);
  if (dst.w <= 0 || dst.h <= 0) return;
  const c = new OffscreenCanvas(dst.w, dst.h);
  const g = c.getContext("2d")!;
  g.translate(-dst.x, -dst.y);
  const img = toCanvas(rgba, src.w, src.h);
  if (target.matrix) {
    const m = target.matrix;
    g.transform(m.a, m.b, m.c, m.d, m.e, m.f);
    g.drawImage(img, src.x, src.y);
  } else {
    drawQuad(g, img, src.w, src.h, quad, 12);
  }
  const px = g.getImageData(0, 0, dst.w, dst.h).data;
  const mask = new Uint8Array(dst.w * dst.h);
  for (let i = 0; i < mask.length; i++) mask[i] = px[i * 4 + 3];
  await run(ed, { op: "select.mask", x: dst.x, y: dst.y, width: dst.w, height: dst.h, mode: "replace", label: "Transform Selection" }, { bytes: mask });
}
