// Shared plumbing for the canvas tools: running commands whose backend may
// not have landed yet, overlay drawing in viewport pixels, hit-testing,
// and the small geometry every tool needs.

import type { EditorStore } from "../lib/editor.svelte";
import { allLayers, type ExecResult, type LayerInfo, type Point, type Rect, type Rgba8 } from "../engine/types";
import { t } from "../lib/i18n";
import type { ToolPointer } from "./types";

// ---------------------------------------------------------------------------
// Commands

const warned = new Set<string>();

export function isUnknownOp(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return /unknown operation/i.test(msg);
}

export function isNotReady(e: unknown): boolean {
  return e instanceof Error && (e.name === "NotReady" || /not available yet/i.test(e.message));
}

/** Tell the user once per feature that it has not arrived yet. */
export function notReady(ed: EditorStore, feature: string) {
  if (warned.has(feature)) return;
  warned.add(feature);
  ed.toast(t("{feature} is not available yet.", { feature }), "info");
  setTimeout(() => warned.delete(feature), 8000);
}

/**
 * Run a command and throw on failure. Like `editor.exec` it marks the
 * document dirty, tells `editor.onExec` listeners (Actions recording) and
 * surfaces engine warnings, but it leaves the error to the caller so tools
 * can tell "not implemented yet" from a real failure.
 */
export async function exec(ed: EditorStore, cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray): Promise<ExecResult> {
  const hadBytes = !!bytes && bytes.byteLength > 0;
  // Parameters often come from reactive state (summary, options), whose
  // proxies cannot cross postMessage: send plain data.
  cmd = JSON.parse(JSON.stringify(cmd));
  const r = await ed.engine.exec(cmd, bytes);
  if (r.changed) ed.dirty = true;
  const listeners = (ed as unknown as { onExec?: Set<(c: Record<string, unknown>, r: ExecResult, b: boolean) => void> }).onExec;
  if (listeners) {
    for (const fn of listeners) {
      try {
        fn(cmd, r, hadBytes);
      } catch (e) {
        console.error("onExec listener failed", e);
      }
    }
  }
  const warnings = (r as { warnings?: unknown }).warnings;
  if (Array.isArray(warnings)) for (const w of warnings) ed.toast(String(w), "info");
  return r;
}

/**
 * Report a command failure. Editing pixels of a text, shape or smart object
 * layer gets a toast with a Rasterize action instead of a bare error.
 */
export function reportError(ed: EditorStore, e: unknown, feature?: string) {
  if (isUnknownOp(e)) {
    notReady(ed, feature ?? t("This tool"));
    return;
  }
  const msg = e instanceof Error ? e.message : String(e);
  const layer = ed.active;
  if (layer && /rasteri[sz]e it|no pixels to edit/i.test(msg) && (layer.kind === "text" || layer.kind === "shape" || (layer.kind as string) === "smart")) {
    const kind = (layer.kind as string) === "smart" ? t("a smart object") : layer.kind === "text" ? t("a text layer") : t("a shape layer");
    const id = layer.id;
    ed.toast(t("{name} is {kind}. Rasterize it to edit its pixels.", { name: layer.name, kind }), "error", {
      label: t("Rasterize"),
      run: () => void run(ed, { op: "layer.rasterize", id }),
    });
    return;
  }
  ed.error(e);
}

/**
 * True when the active layer has pixels a destructive tool can edit. For a
 * text, shape or smart object layer it shows the Rasterize toast instead.
 */
export function requirePixels(ed: EditorStore): boolean {
  const l = ed.active;
  if (!l) {
    ed.toast(t("Select a layer first."), "error");
    return false;
  }
  if (l.kind === "text" || l.kind === "shape" || (l.kind as string) === "smart") {
    reportError(ed, new Error("rasterize it before editing its pixels"));
    return false;
  }
  return true;
}

/**
 * Run a command. An operation the engine does not know yet becomes a single
 * "not available yet" notice for `feature`; any other failure is an error
 * toast. Resolves to null on failure.
 */
export async function run(
  ed: EditorStore,
  cmd: Record<string, unknown>,
  opts: { bytes?: Uint8Array | Uint8ClampedArray; feature?: string; quiet?: boolean; throwUnknown?: boolean } = {},
): Promise<ExecResult | null> {
  try {
    return await exec(ed, cmd, opts.bytes);
  } catch (e) {
    if (isUnknownOp(e)) {
      if (opts.throwUnknown) throw e;
      notReady(ed, opts.feature ?? String(cmd.op));
    } else if (!opts.quiet) {
      reportError(ed, e, opts.feature);
    }
    return null;
  }
}

/** Close the current merge group so the next drag is its own undo step. */
export function seal(ed: EditorStore) {
  return ed.engine.exec({ op: "edit.seal" }).catch(() => undefined);
}

/** Make a layer active (not recorded in history). */
export function setActive(ed: EditorStore, id: number) {
  return exec(ed, { op: "layer.set-active", id }).catch(() => null);
}

let strokeCounter = Date.now() % 100000;
export function newStrokeId(prefix = "s"): string {
  return `${prefix}${++strokeCounter}`;
}

export function redraw(ed: EditorStore) {
  ed.overlayTick++;
}

// ---------------------------------------------------------------------------
// Canvas element access (the canvas component is not ours; these reach it
// through its stable class names).

export function canvasHost(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[data-testid="canvas"]');
}

export function overlayCanvas(): HTMLCanvasElement | null {
  return document.querySelector<HTMLCanvasElement>(".ops-canvas__overlay");
}

/** Change the pointer cursor while a tool is active (e.g. over a handle). */
export function setCursor(cursor: string) {
  const el = overlayCanvas();
  if (el && el.style.cursor !== cursor) el.style.cursor = cursor;
}

/** Where the pointer hovers, for brush outlines; null once it leaves the canvas. */
export const hover: { x: number; y: number; vx: number; vy: number; inside: boolean; pointerType: string } = {
  x: 0,
  y: 0,
  vx: 0,
  vy: 0,
  inside: false,
  pointerType: "mouse",
};

let leaveHooked: HTMLElement | null = null;
export function trackHover(ed: EditorStore, p: ToolPointer) {
  hover.x = p.x;
  hover.y = p.y;
  hover.vx = p.vx;
  hover.vy = p.vy;
  hover.inside = true;
  hover.pointerType = p.pointerType;
  const el = overlayCanvas();
  if (el && leaveHooked !== el) {
    leaveHooked = el;
    el.addEventListener("pointerleave", () => {
      hover.inside = false;
      ed.overlayTick++;
    });
  }
}

export function isCoarse(pointerType?: string) {
  if (pointerType) return pointerType === "touch";
  return typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;
}

/** Handle hit radius in CSS pixels. */
export function handleRadius(pointerType?: string) {
  return isCoarse(pointerType) ? 22 : 8;
}

export function isMac() {
  return typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform);
}

// ---------------------------------------------------------------------------
// Geometry

export type Pt = Point;

export function dist(a: Pt, b: Pt) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

export function clamp(v: number, lo: number, hi: number) {
  return Math.min(hi, Math.max(lo, v));
}

export function rectFrom(a: Pt, b: Pt): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

export function inRect(r: Rect, p: Pt, pad = 0) {
  return p.x >= r.x - pad && p.y >= r.y - pad && p.x <= r.x + r.w + pad && p.y <= r.y + r.h + pad;
}

export function distToSegment(p: Pt, a: Pt, b: Pt) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  const k = len2 === 0 ? 0 : clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / len2, 0, 1);
  return Math.hypot(p.x - (a.x + k * dx), p.y - (a.y + k * dy));
}

export function rotatePt(p: Pt, c: Pt, rad: number): Pt {
  const s = Math.sin(rad);
  const k = Math.cos(rad);
  const dx = p.x - c.x;
  const dy = p.y - c.y;
  return { x: c.x + dx * k - dy * s, y: c.y + dx * s + dy * k };
}

/** Snap the vector a→b to 45° steps (shift-constrained lines). */
export function snap45(a: Pt, b: Pt): Pt {
  const ang = Math.atan2(b.y - a.y, b.x - a.x);
  const step = Math.PI / 4;
  const s = Math.round(ang / step) * step;
  const len = dist(a, b);
  return { x: a.x + Math.cos(s) * len, y: a.y + Math.sin(s) * len };
}

/**
 * Marquee-style rectangle from a drag: `square` constrains to equal sides,
 * `centre` grows from the start point. `ratio` (w/h) locks the aspect.
 */
export function dragBox(a: Pt, b: Pt, square: boolean, centre: boolean, ratio: number | null = null): Rect {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  const r = square ? 1 : ratio;
  if (r) {
    const w = Math.max(Math.abs(dx), Math.abs(dy) * r);
    const h = w / r;
    dx = Math.sign(dx || 1) * w;
    dy = Math.sign(dy || 1) * h;
  }
  if (centre) return { x: a.x - Math.abs(dx), y: a.y - Math.abs(dy), w: Math.abs(dx) * 2, h: Math.abs(dy) * 2 };
  return rectFrom(a, { x: a.x + dx, y: a.y + dy });
}

export function polylineBounds(pts: Pt[], pad = 0): Rect {
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of pts) {
    x0 = Math.min(x0, p.x);
    y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x);
    y1 = Math.max(y1, p.y);
  }
  if (!pts.length) return { x: 0, y: 0, w: 0, h: 0 };
  return { x: x0 - pad, y: y0 - pad, w: x1 - x0 + 2 * pad, h: y1 - y0 + 2 * pad };
}

export function intRect(r: Rect): Rect {
  const x = Math.floor(r.x);
  const y = Math.floor(r.y);
  return { x, y, w: Math.ceil(r.x + r.w) - x, h: Math.ceil(r.y + r.h) - y };
}

export function clipRect(r: Rect, w: number, h: number): Rect {
  const x0 = clamp(r.x, 0, w);
  const y0 = clamp(r.y, 0, h);
  const x1 = clamp(r.x + r.w, 0, w);
  const y1 = clamp(r.y + r.h, 0, h);
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** Moving-average smoothing for freehand input; `amount` 0..1. */
export function smooth(points: Pt[], amount: number): Pt[] {
  if (amount <= 0 || points.length < 3) return points;
  const radius = Math.max(1, Math.round(amount * 6));
  const out: Pt[] = [];
  for (let i = 0; i < points.length; i++) {
    let sx = 0;
    let sy = 0;
    let n = 0;
    for (let j = Math.max(0, i - radius); j <= Math.min(points.length - 1, i + radius); j++) {
      sx += points[j].x;
      sy += points[j].y;
      n++;
    }
    out.push(i === 0 || i === points.length - 1 ? points[i] : { x: sx / n, y: sy / n });
  }
  return out;
}

/** Drop points closer than `min` to their predecessor. */
export function thin(points: Pt[], min: number): Pt[] {
  const out: Pt[] = [];
  for (const p of points) if (!out.length || dist(out[out.length - 1], p) >= min) out.push(p);
  const last = points[points.length - 1];
  if (last && out[out.length - 1] !== last) out.push(last);
  return out;
}

// ---------------------------------------------------------------------------
// Colour

export function css(c: Rgba8, alphaMul = 1) {
  return `rgba(${c.r},${c.g},${c.b},${((c.a ?? 255) / 255) * alphaMul})`;
}

export function toHex(c: Rgba8) {
  const h = (v: number) => Math.round(v).toString(16).padStart(2, "0");
  return `#${h(c.r)}${h(c.g)}${h(c.b)}`;
}

export function fromHex(hex: string, a = 255): Rgba8 {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  if (!m) return { r: 0, g: 0, b: 0, a };
  return { r: parseInt(m[1], 16), g: parseInt(m[2], 16), b: parseInt(m[3], 16), a };
}

export function rgba(c: Rgba8): Required<Rgba8> {
  return { r: c.r, g: c.g, b: c.b, a: c.a ?? 255 };
}

// ---------------------------------------------------------------------------
// Overlay drawing (viewport CSS pixels)

export function v(ed: EditorStore, p: Pt) {
  return ed.toView(p.x, p.y);
}

/** Black-and-white dashed path, readable on any image. */
export function antsStroke(ctx: CanvasRenderingContext2D, path: () => void, phase = 0) {
  ctx.save();
  ctx.lineWidth = 1;
  ctx.setLineDash([]);
  ctx.strokeStyle = "rgba(255,255,255,0.95)";
  ctx.beginPath();
  path();
  ctx.stroke();
  ctx.setLineDash([4, 4]);
  ctx.lineDashOffset = phase;
  ctx.strokeStyle = "rgba(0,0,0,0.95)";
  ctx.beginPath();
  path();
  ctx.stroke();
  ctx.restore();
}

export function docRectPath(ed: EditorStore, ctx: CanvasRenderingContext2D, r: Rect) {
  const a = ed.toView(r.x, r.y);
  const b = ed.toView(r.x + r.w, r.y + r.h);
  ctx.rect(Math.round(a.x) + 0.5, Math.round(a.y) + 0.5, Math.round(b.x - a.x), Math.round(b.y - a.y));
}

export function docEllipsePath(ed: EditorStore, ctx: CanvasRenderingContext2D, r: Rect) {
  const a = ed.toView(r.x, r.y);
  const b = ed.toView(r.x + r.w, r.y + r.h);
  ctx.ellipse((a.x + b.x) / 2, (a.y + b.y) / 2, Math.abs(b.x - a.x) / 2, Math.abs(b.y - a.y) / 2, 0, 0, Math.PI * 2);
}

export function docPolyPath(ed: EditorStore, ctx: CanvasRenderingContext2D, pts: Pt[], close = false) {
  pts.forEach((p, i) => {
    const q = ed.toView(p.x, p.y);
    if (i === 0) ctx.moveTo(q.x, q.y);
    else ctx.lineTo(q.x, q.y);
  });
  if (close) ctx.closePath();
}

/** A square handle centred on a viewport point. */
export function drawHandle(ctx: CanvasRenderingContext2D, x: number, y: number, size = 8, round = false) {
  ctx.save();
  ctx.fillStyle = "#ffffff";
  ctx.strokeStyle = "#1473e6";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  if (round) ctx.arc(x, y, size / 2, 0, Math.PI * 2);
  else ctx.rect(Math.round(x - size / 2) + 0.5, Math.round(y - size / 2) + 0.5, size, size);
  ctx.fill();
  ctx.stroke();
  ctx.restore();
}

/** Circle outline for brush cursors, visible on light and dark. */
export function drawBrushCircle(ctx: CanvasRenderingContext2D, x: number, y: number, r: number, crosshair = false) {
  ctx.save();
  ctx.lineWidth = 1;
  ctx.strokeStyle = "rgba(0,0,0,0.8)";
  ctx.beginPath();
  ctx.arc(x, y, Math.max(1, r), 0, Math.PI * 2);
  ctx.stroke();
  ctx.strokeStyle = "rgba(255,255,255,0.9)";
  ctx.beginPath();
  ctx.arc(x, y, Math.max(1, r + 1), 0, Math.PI * 2);
  ctx.stroke();
  if (crosshair || r < 4) {
    drawCrosshair(ctx, x, y, 5);
  }
  ctx.restore();
}

export function drawCrosshair(ctx: CanvasRenderingContext2D, x: number, y: number, s = 7) {
  ctx.save();
  for (const [col, w] of [["rgba(255,255,255,0.9)", 3], ["rgba(0,0,0,0.9)", 1]] as const) {
    ctx.strokeStyle = col;
    ctx.lineWidth = w;
    ctx.beginPath();
    ctx.moveTo(x - s, y);
    ctx.lineTo(x + s, y);
    ctx.moveTo(x, y - s);
    ctx.lineTo(x, y + s);
    ctx.stroke();
  }
  ctx.restore();
}

/** A small label pill near a viewport point (sizes, angles). */
export function drawLabel(ctx: CanvasRenderingContext2D, text: string, x: number, y: number) {
  ctx.save();
  ctx.font = "500 11px Geist, system-ui, sans-serif";
  const w = ctx.measureText(text).width + 12;
  const h = 20;
  const lx = Math.round(x + 14);
  const ly = Math.round(y + 14);
  ctx.fillStyle = "rgba(20,20,20,0.85)";
  ctx.beginPath();
  ctx.roundRect(lx, ly, w, h, 4);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.textBaseline = "middle";
  ctx.fillText(text, lx + 6, ly + h / 2 + 0.5);
  ctx.restore();
}

// ---------------------------------------------------------------------------
// Handles on a box (8 of them)

export type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export const HANDLE_IDS: HandleId[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function boxHandles(r: Rect): Record<HandleId, Pt> {
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  return {
    nw: { x: r.x, y: r.y },
    n: { x: cx, y: r.y },
    ne: { x: r.x + r.w, y: r.y },
    e: { x: r.x + r.w, y: cy },
    se: { x: r.x + r.w, y: r.y + r.h },
    s: { x: cx, y: r.y + r.h },
    sw: { x: r.x, y: r.y + r.h },
    w: { x: r.x, y: cy },
  };
}

export const HANDLE_CURSOR: Record<HandleId, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  n: "ns-resize",
  s: "ns-resize",
  e: "ew-resize",
  w: "ew-resize",
};

/** Hit-test handles in viewport space. */
export function hitHandle(ed: EditorStore, handles: Record<string, Pt>, p: ToolPointer): string | null {
  const rad = handleRadius(p.pointerType);
  let best: string | null = null;
  let bestD = Infinity;
  for (const [id, h] of Object.entries(handles)) {
    const q = ed.toView(h.x, h.y);
    const d = Math.hypot(q.x - p.vx, q.y - p.vy);
    if (d <= rad && d < bestD) {
      best = id;
      bestD = d;
    }
  }
  return best;
}

/** Resize a rectangle by dragging one handle to `p`. */
export function resizeBox(orig: Rect, handle: HandleId, p: Pt, opts: { ratio?: number | null; centre?: boolean } = {}): Rect {
  let x0 = orig.x;
  let y0 = orig.y;
  let x1 = orig.x + orig.w;
  let y1 = orig.y + orig.h;
  const cx = orig.x + orig.w / 2;
  const cy = orig.y + orig.h / 2;
  if (handle.includes("w")) x0 = p.x;
  if (handle.includes("e")) x1 = p.x;
  if (handle.includes("n")) y0 = p.y;
  if (handle.includes("s")) y1 = p.y;
  if (opts.centre) {
    if (handle.includes("w")) x1 = 2 * cx - x0;
    if (handle.includes("e")) x0 = 2 * cx - x1;
    if (handle.includes("n")) y1 = 2 * cy - y0;
    if (handle.includes("s")) y0 = 2 * cy - y1;
  }
  let r = { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
  const ratio = opts.ratio;
  if (ratio && r.w > 0 && r.h > 0) {
    const vertical = handle === "n" || handle === "s";
    const horizontal = handle === "e" || handle === "w";
    let w = r.w;
    let h = r.h;
    if (vertical) w = h * ratio;
    else if (horizontal) h = w / ratio;
    else if (w / h > ratio) h = w / ratio;
    else w = h * ratio;
    // Anchor the edge opposite the handle (or the centre).
    let nx = handle.includes("w") ? Math.max(x0, x1) - w : Math.min(x0, x1);
    let ny = handle.includes("n") ? Math.max(y0, y1) - h : Math.min(y0, y1);
    if (vertical) nx = cx - w / 2;
    if (horizontal) ny = cy - h / 2;
    if (opts.centre) {
      nx = cx - w / 2;
      ny = cy - h / 2;
    }
    r = { x: nx, y: ny, w, h };
  }
  return r;
}

export function drawBoxHandles(ed: EditorStore, ctx: CanvasRenderingContext2D, r: Rect, round = false) {
  const hs = boxHandles(r);
  const size = isCoarse() ? 14 : 8;
  for (const id of HANDLE_IDS) {
    const q = ed.toView(hs[id].x, hs[id].y);
    drawHandle(ctx, q.x, q.y, size, round);
  }
}

// ---------------------------------------------------------------------------
// Layers and selection

export function layersTopDown(ed: EditorStore): LayerInfo[] {
  if (!ed.summary) return [];
  return allLayers(ed.summary.layers).reverse();
}

/** Topmost visible layer with a non-transparent pixel under (x, y). */
export async function layerAt(ed: EditorStore, x: number, y: number, accept: (l: LayerInfo) => boolean = () => true): Promise<LayerInfo | null> {
  const px = Math.floor(x);
  const py = Math.floor(y);
  for (const l of layersTopDown(ed)) {
    if (!l.visible || !l.bounds || !accept(l)) continue;
    if (!inRect(l.bounds, { x: px + 0.5, y: py + 0.5 })) continue;
    try {
      const a = await ed.engine.call<Uint8Array>("layer_region", l.id, px, py, 1, 1);
      if (a[3] > 8) return l;
    } catch {
      /* layer vanished mid-walk */
    }
  }
  return null;
}

export interface SavedSelection {
  kind: "none" | "mask";
  x: number;
  y: number;
  w: number;
  h: number;
  bytes?: Uint8Array;
}

/** Remember the current selection so a tool can restore it after borrowing it. */
export async function saveSelection(ed: EditorStore): Promise<SavedSelection> {
  const s = ed.summary;
  if (!s?.selection) return { kind: "none", x: 0, y: 0, w: 0, h: 0 };
  const b = s.selection.bounds;
  if (b.w <= 0 || b.h <= 0) return { kind: "none", x: 0, y: 0, w: 0, h: 0 };
  const bytes = await ed.engine.call<Uint8Array>("selection_region", b.x, b.y, b.w, b.h);
  return { kind: "mask", x: b.x, y: b.y, w: b.w, h: b.h, bytes };
}

export async function restoreSelection(ed: EditorStore, saved: SavedSelection) {
  if (saved.kind === "none") {
    if (ed.summary?.selection) await run(ed, { op: "select.none" }, { quiet: true });
    return;
  }
  await run(ed, { op: "select.mask", x: saved.x, y: saved.y, width: saved.w, height: saved.h, mode: "replace", label: "Restore Selection" }, { bytes: saved.bytes!.slice() });
}

/** Photoshop's modifier rules for selection tools, read at pointer-down. */
export function selectionMode(p: { shift: boolean; alt: boolean }, fallback: string): "replace" | "add" | "subtract" | "intersect" {
  if (p.shift && p.alt) return "intersect";
  if (p.shift) return "add";
  if (p.alt) return "subtract";
  return fallback as "replace" | "add" | "subtract" | "intersect";
}

export function activeLayer(ed: EditorStore) {
  return ed.active;
}

/**
 * Coalesce pointer samples and flush them once per animation frame, in
 * order, never overlapping (the next flush waits for the previous command).
 */
export class FrameBatcher<T> {
  private queue: T[] = [];
  private raf = 0;
  private chain: Promise<void> = Promise.resolve();
  constructor(private flush: (items: T[]) => Promise<void> | void) {}
  push(item: T) {
    this.queue.push(item);
    if (!this.raf) this.raf = requestAnimationFrame(() => this.drain());
  }
  private drain() {
    this.raf = 0;
    if (!this.queue.length) return;
    const items = this.queue;
    this.queue = [];
    this.chain = this.chain.then(() => this.flush(items)).catch((e) => console.error(e));
  }
  /** Flush whatever is queued now and wait for every pending flush. */
  async done() {
    if (this.raf) cancelAnimationFrame(this.raf);
    this.drain();
    await this.chain;
  }
  clear() {
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.queue = [];
  }
}

/** Number keys 1..0 → 10%..100%, as Photoshop maps them. */
export function digitOpacity(e: KeyboardEvent): number | null {
  if (e.metaKey || e.ctrlKey || e.altKey) return null;
  if (!/^Digit[0-9]$/.test(e.code)) return null;
  const n = Number(e.code.slice(5));
  return n === 0 ? 1 : n / 10;
}

/** Photoshop's `[` / `]` size steps. */
export function stepSize(size: number, dir: 1 | -1) {
  const step = size < 10 ? 1 : size < 100 ? 10 : size < 200 ? 25 : size < 500 ? 50 : 100;
  return Math.max(1, size + dir * step);
}
