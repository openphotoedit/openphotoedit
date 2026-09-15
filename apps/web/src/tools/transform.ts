// Free transform: a box around the active layer (or the selected pixels)
// with eight handles. Corner handles scale proportionally by default for
// pixel and text layers (shift unlocks, as in Photoshop 2019+), alt scales
// from the centre, dragging outside rotates (shift snaps 15°), Cmd/Ctrl-drag
// a corner distorts. A snapshot of the layer previews the result live;
// Enter, double-click or Apply commits, Escape cancels.

import type { EditorStore } from "../lib/editor.svelte";
import type { LayerInfo, Rect, ShapeData, TextData } from "../engine/types";
import { t } from "../lib/i18n";
import { renderText, textBox } from "../lib/text";
import type { Tool, ToolPointer } from "./types";
import { toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import { boxHandles, drawHandle, drawLabel, HANDLE_CURSOR, HANDLE_IDS, handleRadius, isCoarse, isUnknownOp, notReady, redraw, resizeBox, rotatePt, run, seal, setCursor, trackHover, type HandleId, type Pt, exec, reportError } from "./common";
import { applyM, drawQuad, invert, isIdentity, mul, transformPixelsInBrowser, transformSelectionInBrowser, type Matrix } from "./raster";

interface Box {
  cx: number;
  cy: number;
  w: number;
  h: number;
  /** Radians, clockwise. */
  angle: number;
  flipX: boolean;
  flipY: boolean;
}

interface Session {
  key: string;
  mode: "layer" | "selection";
  layer: LayerInfo;
  src: Rect;
  box: Box;
  quad: Pt[] | null;
  snapshot: { canvas: OffscreenCanvas; w: number; h: number } | null;
  /** Document area the snapshot shows (the layer's current bounds). */
  snapRect: Rect;
  /** Matrix of the starting box: identity except for rotated smart objects. */
  initMatrix: Matrix;
  initCorners: Pt[];
  /** The starting box is the layer's axis-aligned bounds. */
  initAxisAligned: boolean;
  hidden: boolean;
}

interface SmartInfo {
  quad: Pt[];
}

function smartOf(l: LayerInfo): SmartInfo | null {
  const sm = (l as unknown as { smart?: SmartInfo }).smart;
  return (l.kind as string) === "smart" && sm && Array.isArray(sm.quad) && sm.quad.length === 4 ? sm : null;
}

type DragKind = "move" | "scale" | "rotate" | "corner" | "edge";

let session: Session | null = null;
let drag: { kind: DragKind; handle: string; start: Pt; box: Box; quad: Pt[] | null; moved: boolean } | null = null;
let lastDown = 0;
let busy = false;

function sessionKey(ed: EditorStore) {
  const l = ed.active;
  const sel = ed.summary?.selection?.bounds;
  return l ? `${l.id}:${l.rev}:${JSON.stringify(l.bounds)}:${JSON.stringify(sel)}` : "";
}

function matrixOf(s: Session): Matrix {
  const { src, box } = s;
  const sx = (box.w / Math.max(1e-6, src.w)) * (box.flipX ? -1 : 1);
  const sy = (box.h / Math.max(1e-6, src.h)) * (box.flipY ? -1 : 1);
  const cos = Math.cos(box.angle);
  const sin = Math.sin(box.angle);
  const a = cos * sx;
  const b = sin * sx;
  const c = -sin * sy;
  const d = cos * sy;
  const scx = src.x + src.w / 2;
  const scy = src.y + src.h / 2;
  return { a, b, c, d, e: box.cx - (a * scx + c * scy), f: box.cy - (b * scx + d * scy) };
}

function cornersOf(s: Session): Pt[] {
  if (s.quad) return s.quad;
  const m = matrixOf(s);
  const r = s.src;
  return [
    { x: r.x, y: r.y },
    { x: r.x + r.w, y: r.y },
    { x: r.x + r.w, y: r.y + r.h },
    { x: r.x, y: r.y + r.h },
  ].map((p) => applyM(m, p));
}

function localRect(b: Box) {
  return { x: b.cx - b.w / 2, y: b.cy - b.h / 2, w: b.w, h: b.h };
}

function changed(s: Session) {
  const c = cornersOf(s);
  return c.some((p, i) => Math.abs(p.x - s.initCorners[i].x) > 0.01 || Math.abs(p.y - s.initCorners[i].y) > 0.01);
}

function sync(s: Session | null) {
  toolState.transformActive = !!s && changed(s);
  toolState.transformInfo = s ? { w: Math.round(s.box.w), h: Math.round(s.box.h), angle: Math.round(((s.box.angle * 180) / Math.PI) * 10) / 10 } : { w: 0, h: 0, angle: 0 };
}

async function takeSnapshot(ed: EditorStore, s: Session) {
  const doc = ed.summary!;
  const size = Math.min(2048, Math.max(doc.width, doc.height));
  const scale = size / Math.max(doc.width, doc.height);
  try {
    const raw = await ed.engine.call<Uint8Array>("thumbnail", s.layer.id, size);
    const tw = raw[0] | (raw[1] << 8);
    const th = raw[2] | (raw[3] << 8);
    const full = new OffscreenCanvas(tw, th);
    full.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(raw.buffer as ArrayBuffer, raw.byteOffset + 4, tw * th * 4), tw, th), 0, 0);
    const k = tw / doc.width || scale;
    const R = s.snapRect;
    const w = Math.max(1, Math.round(R.w * k));
    const h = Math.max(1, Math.round(R.h * k));
    const c = new OffscreenCanvas(w, h);
    const g = c.getContext("2d")!;
    g.drawImage(full, -R.x * k, -R.y * k);
    if (s.mode === "selection") {
      const cov = await ed.engine.call<Uint8Array>("selection_view", s.src.x, s.src.y, k, w, h);
      if (cov.length === w * h) {
        const img = g.getImageData(0, 0, w, h);
        for (let i = 0; i < cov.length; i++) img.data[i * 4 + 3] = (img.data[i * 4 + 3] * cov[i]) / 255;
        g.putImageData(img, 0, 0);
      }
    }
    if (session === s) {
      s.snapshot = { canvas: c, w, h };
      redraw(ed);
    }
  } catch (e) {
    console.warn("transform snapshot failed", e);
  }
}

function begin(ed: EditorStore): Session | null {
  const l = ed.active;
  const doc = ed.summary;
  if (!l || !doc) return null;
  const selBounds = doc.selection?.bounds ?? null;
  let src: Rect | null = null;
  let mode: Session["mode"] = "layer";
  if (selBounds && l.kind === "pixel") {
    mode = "selection";
    src = selBounds;
  } else if (l.bounds && l.bounds.w > 0 && l.bounds.h > 0) {
    src = l.bounds;
  } else if (l.kind === "group" || l.kind === "adjustment" || l.kind === "fill") {
    src = { x: 0, y: 0, w: doc.width, h: doc.height };
  }
  if (!src) return null;
  const s: Session = {
    key: sessionKey(ed),
    mode,
    layer: l,
    src,
    box: { cx: src.x + src.w / 2, cy: src.y + src.h / 2, w: src.w, h: src.h, angle: 0, flipX: false, flipY: false },
    quad: null,
    snapshot: null,
    snapRect: src,
    initMatrix: { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 },
    initCorners: [],
    initAxisAligned: true,
    hidden: false,
  };
  const smart = mode === "layer" ? smartOf(l) : null;
  if (smart) {
    // Start from the object's own corners, which stay exact once rotated.
    const [tl, tr, br, bl] = smart.quad;
    const u = { x: tr.x - tl.x, y: tr.y - tl.y };
    const v = { x: bl.x - tl.x, y: bl.y - tl.y };
    const w = Math.hypot(u.x, u.y);
    const h = Math.hypot(v.x, v.y);
    const rectangular = Math.abs(u.x * v.x + u.y * v.y) < 1e-3 * Math.max(1, w * h) && Math.hypot(tl.x + u.x + v.x - br.x, tl.y + u.y + v.y - br.y) < 0.5;
    if (rectangular && w > 0 && h > 0) {
      const cx = (tl.x + br.x) / 2;
      const cy = (tl.y + br.y) / 2;
      s.src = { x: cx - w / 2, y: cy - h / 2, w, h };
      s.box = { cx, cy, w, h, angle: Math.atan2(u.y, u.x), flipX: false, flipY: u.x * v.y - u.y * v.x < 0 };
      s.initMatrix = matrixOf(s);
      s.initAxisAligned = isIdentity(s.initMatrix, 1e-6);
    } else {
      s.quad = smart.quad.map((q) => ({ x: q.x, y: q.y }));
      s.initAxisAligned = false;
    }
  }
  s.initCorners = cornersOf(s).map((q) => ({ ...q }));
  session = s;
  sync(s);
  void takeSnapshot(ed, s);
  return s;
}

function ensure(ed: EditorStore): Session | null {
  if (busy) return session;
  if (session && (drag || session.hidden || changed(session))) return session;
  const key = sessionKey(ed);
  if (!session || session.key !== key) {
    session = null;
    if (key) begin(ed);
    else sync(null);
  }
  return session;
}

/** Hide the original while the preview stands in for it (viewport only). */
async function hideOriginal(ed: EditorStore, s: Session) {
  if (s.hidden || s.mode !== "layer" || !s.layer.visible) return;
  s.hidden = true;
  await ed.setPreviewHidden([...ed.previewHidden, s.layer.id]);
}

async function unhide(ed: EditorStore, s: Session) {
  if (!s.hidden) return;
  s.hidden = false;
  await ed.setPreviewHidden(ed.previewHidden.filter((id) => id !== s.layer.id));
}

function bilinear(q: Pt[], src: Rect, p: Pt): Pt {
  const u = (p.x - src.x) / Math.max(1e-6, src.w);
  const v = (p.y - src.y) / Math.max(1e-6, src.h);
  const top = { x: q[0].x + (q[1].x - q[0].x) * u, y: q[0].y + (q[1].y - q[0].y) * u };
  const bot = { x: q[3].x + (q[2].x - q[3].x) * u, y: q[3].y + (q[2].y - q[3].y) * u };
  return { x: top.x + (bot.x - top.x) * v, y: top.y + (bot.y - top.y) * v };
}

async function commitShape(ed: EditorStore, s: Session, m: Matrix): Promise<boolean> {
  const d = s.layer.shape!;
  const map = (p: Pt) => (s.quad ? bilinear(s.quad, s.src, p) : applyM(m, p));
  const pointwise = d.kind === "line" || d.kind === "arrow" || d.kind === "polyline" || d.kind === "polygon";
  const axisAligned = !s.quad && Math.abs(m.b) < 1e-6 && Math.abs(m.c) < 1e-6;
  if (pointwise || axisAligned) {
    const data: ShapeData = { ...d, points: d.points.map((p) => map(p)) };
    return !!(await run(ed, { op: "layer.set-shape", id: s.layer.id, data }));
  }
  return commitEngine(ed, s, m);
}

async function commitText(ed: EditorStore, s: Session, m: Matrix): Promise<boolean> {
  const d = s.layer.text!;
  if (s.quad) return commitEngine(ed, s, m);
  const sx = Math.hypot(m.a, m.b);
  const sy = Math.hypot(m.c, m.d);
  const k = sy;
  const next: TextData = {
    ...d,
    font_size: Math.max(1, d.font_size * k),
    box_width: d.box_width != null ? d.box_width * sx : null,
    padding: d.padding * k,
    stroke_width: d.stroke_width * k,
    letter_spacing: d.letter_spacing * sx,
    rotation: d.rotation + (s.box.angle * 180) / Math.PI,
  };
  // Keep the box centred where the transform put it.
  const b = textBox({ ...next, x: 0, y: 0 });
  const pad = next.background ? next.padding : 0;
  next.x = s.box.cx - b.w / 2 + pad;
  next.y = s.box.cy - b.h / 2 + pad;
  const r = await renderText(next);
  return !!(await run(ed, { op: "layer.set-text", id: s.layer.id, data: next, width: r.width, height: r.height, x: r.x, y: r.y }, { bytes: r.rgba }));
}

async function commitEngine(ed: EditorStore, s: Session, m: Matrix): Promise<boolean> {
  const target = s.quad ? { quad: s.quad } : { matrix: m };
  const op = s.mode === "selection" ? { op: "transform.selection-pixels", id: s.layer.id, ...target } : { op: "transform.layer", ids: [s.layer.id], ...target, resample: "bicubic" };
  try {
    await exec(ed, op);
    return true;
  } catch (e) {
    if (!isUnknownOp(e)) {
      reportError(ed, e);
      return false;
    }
  }
  // The transform backend has not landed: pixel layers can be done here.
  if (s.layer.kind !== "pixel") {
    notReady(ed, t("Transforming this kind of layer"));
    return false;
  }
  if (s.mode === "selection") {
    const ok = await transformPixelsInBrowser(ed, s.layer.id, s.src, target, { selectionOnly: true });
    if (ok) await transformSelectionInBrowser(ed, target);
    return ok;
  }
  return transformPixelsInBrowser(ed, s.layer.id, s.src, target);
}

export async function commitTransform(ed: EditorStore) {
  const s = session;
  if (!s || busy) return;
  busy = true;
  drag = null;
  try {
    await unhide(ed, s);
    if (!changed(s)) return;
    const m = matrixOf(s);
    if (smartOf(s.layer) && s.mode === "layer") {
      await run(ed, { op: "layer.smart-transform", id: s.layer.id, quad: cornersOf(s).map((q) => ({ x: q.x, y: q.y })) });
      await seal(ed);
    } else if (s.layer.kind === "shape" && s.layer.shape && s.mode === "layer") await commitShape(ed, s, m);
    else if (s.layer.kind === "text" && s.layer.text && s.mode === "layer") await commitText(ed, s, m);
    else await commitEngine(ed, s, m);
  } finally {
    busy = false;
    session = null;
    ensure(ed);
    redraw(ed);
  }
}

export async function cancelTransform(ed: EditorStore) {
  const s = session;
  drag = null;
  if (!s) return;
  busy = true;
  try {
    await unhide(ed, s);
  } finally {
    busy = false;
    session = null;
    ensure(ed);
    redraw(ed);
  }
}

/** Flip or rotate by fixed amounts (for option buttons and menus). */
export function transformBy(ed: EditorStore, op: "flip-h" | "flip-v" | "rotate-cw" | "rotate-ccw") {
  const s = ensure(ed);
  if (!s || s.quad) return;
  if (op === "flip-h") s.box.flipX = !s.box.flipX;
  if (op === "flip-v") s.box.flipY = !s.box.flipY;
  if (op === "rotate-cw") s.box.angle += Math.PI / 2;
  if (op === "rotate-ccw") s.box.angle -= Math.PI / 2;
  void hideOriginal(ed, s);
  sync(s);
  redraw(ed);
}

function hitHandle(ed: EditorStore, s: Session, p: ToolPointer): string | null {
  const rad = handleRadius(p.pointerType) + 2;
  const test = (q: Pt) => {
    const v = ed.toView(q.x, q.y);
    return Math.hypot(v.x - p.vx, v.y - p.vy) <= rad;
  };
  if (s.quad) {
    const names = ["nw", "ne", "se", "sw"];
    for (let i = 0; i < 4; i++) if (test(s.quad[i])) return names[i];
    const mids = ["n", "e", "s", "w"];
    for (let i = 0; i < 4; i++) {
      const a = s.quad[i];
      const b = s.quad[(i + 1) % 4];
      if (test({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 })) return mids[i];
    }
    return null;
  }
  const hs = boxHandles(localRect(s.box));
  for (const id of HANDLE_IDS) if (test(rotatePt(hs[id], { x: s.box.cx, y: s.box.cy }, s.box.angle))) return id;
  return null;
}

function insideQuad(q: Pt[], p: Pt) {
  let c = false;
  for (let i = 0, j = 3; i < 4; j = i++) {
    if (q[i].y > p.y !== q[j].y > p.y && p.x < ((q[j].x - q[i].x) * (p.y - q[i].y)) / (q[j].y - q[i].y) + q[i].x) c = !c;
  }
  return c;
}

const CORNER_INDEX: Record<string, number> = { nw: 0, ne: 1, se: 2, sw: 3 };
const EDGE_INDEX: Record<string, [number, number]> = { n: [0, 1], e: [1, 2], s: [2, 3], w: [3, 0] };

function keepRatioByDefault(s: Session) {
  return toolSettings.transformKeepRatio && s.layer.kind !== "shape";
}

export const transform: Tool = {
  id: "transform",
  label: "Free transform",
  cursor: "default",
  activate(ed) {
    session = null;
    ensure(ed);
    redraw(ed);
  },
  deactivate(ed) {
    if (session && changed(session)) void commitTransform(ed);
    else void cancelTransform(ed);
    setCursor("default");
  },
  down(ed, p) {
    const s = ensure(ed);
    if (!s || busy) return;
    const now = performance.now();
    const corners = cornersOf(s);
    const inside = insideQuad(corners, p);
    if (now - lastDown < 320 && inside) {
      lastDown = 0;
      void commitTransform(ed);
      return;
    }
    lastDown = now;
    const h = hitHandle(ed, s, p);
    const base = { start: { x: p.x, y: p.y }, box: { ...s.box }, quad: s.quad ? s.quad.map((q) => ({ ...q })) : null, moved: false };
    if (h && (p.mod || s.quad)) {
      // Distort: switch to free corners.
      if (!s.quad) s.quad = corners.map((q) => ({ ...q }));
      base.quad = s.quad.map((q) => ({ ...q }));
      drag = { ...base, kind: h.length === 2 ? "corner" : "edge", handle: h };
    } else if (h) {
      drag = { ...base, kind: "scale", handle: h };
    } else if (inside) {
      drag = { ...base, kind: "move", handle: "" };
    } else {
      drag = { ...base, kind: "rotate", handle: "" };
    }
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    const s = ensure(ed);
    if (!s) return;
    if (!pressed || !drag) {
      const h = hitHandle(ed, s, p);
      if (h) setCursor(p.mod || s.quad ? "crosshair" : HANDLE_CURSOR[h as HandleId] ?? "move");
      else setCursor(insideQuad(cornersOf(s), p) ? "move" : "alias");
      return;
    }
    const d = drag;
    if (!d.moved) {
      d.moved = true;
      void hideOriginal(ed, s);
    }
    const dx = p.x - d.start.x;
    const dy = p.y - d.start.y;
    switch (d.kind) {
      case "move":
        if (s.quad && d.quad) s.quad = d.quad.map((q) => ({ x: q.x + dx, y: q.y + dy }));
        else s.box = { ...d.box, cx: d.box.cx + (p.shift ? (Math.abs(dx) > Math.abs(dy) ? dx : 0) : dx), cy: d.box.cy + (p.shift ? (Math.abs(dx) > Math.abs(dy) ? 0 : dy) : dy) };
        break;
      case "scale": {
        const b = d.box;
        const c = { x: b.cx, y: b.cy };
        const lp = rotatePt({ x: p.x, y: p.y }, c, -b.angle);
        const corner = d.handle.length === 2;
        const keep = corner && keepRatioByDefault(s) !== p.shift;
        const r = resizeBox(localRect(b), d.handle as HandleId, lp, { ratio: keep ? b.w / Math.max(1e-6, b.h) : null, centre: p.alt });
        const nc = rotatePt({ x: r.x + r.w / 2, y: r.y + r.h / 2 }, c, b.angle);
        // Dragging a handle past the opposite edge flips, as in Photoshop.
        const rawL = { x: b.cx - b.w / 2, y: b.cy - b.h / 2 };
        let flipX = b.flipX;
        let flipY = b.flipY;
        if (d.handle.includes("w") && lp.x > rawL.x + b.w) flipX = !b.flipX;
        if (d.handle.includes("e") && lp.x < rawL.x) flipX = !b.flipX;
        if (d.handle.includes("n") && lp.y > rawL.y + b.h) flipY = !b.flipY;
        if (d.handle.includes("s") && lp.y < rawL.y) flipY = !b.flipY;
        s.box = { ...b, cx: nc.x, cy: nc.y, w: Math.max(1, r.w), h: Math.max(1, r.h), flipX, flipY };
        break;
      }
      case "rotate": {
        if (s.quad && d.quad) {
          const c = { x: d.quad.reduce((a, q) => a + q.x, 0) / 4, y: d.quad.reduce((a, q) => a + q.y, 0) / 4 };
          let ang = Math.atan2(p.y - c.y, p.x - c.x) - Math.atan2(d.start.y - c.y, d.start.x - c.x);
          if (p.shift) ang = Math.round(ang / (Math.PI / 12)) * (Math.PI / 12);
          s.quad = d.quad.map((q) => rotatePt(q, c, ang));
        } else {
          const b = d.box;
          let ang = b.angle + Math.atan2(p.y - b.cy, p.x - b.cx) - Math.atan2(d.start.y - b.cy, d.start.x - b.cx);
          if (p.shift) ang = Math.round(ang / (Math.PI / 12)) * (Math.PI / 12);
          s.box = { ...b, angle: ang };
        }
        break;
      }
      case "corner":
        if (s.quad && d.quad) {
          const i = CORNER_INDEX[d.handle];
          s.quad = d.quad.map((q, k) => (k === i ? { x: q.x + dx, y: q.y + dy } : q));
        }
        break;
      case "edge":
        if (s.quad && d.quad) {
          const [i, j] = EDGE_INDEX[d.handle];
          s.quad = d.quad.map((q, k) => (k === i || k === j ? { x: q.x + dx, y: q.y + dy } : q));
        }
        break;
    }
    sync(s);
  },
  up(ed) {
    drag = null;
    redraw(ed);
  },
  cancel(ed) {
    void cancelTransform(ed);
  },
  key(ed, e) {
    if (!session) return false;
    if (e.key === "Enter") {
      void commitTransform(ed);
      return true;
    }
    const step = e.shiftKey ? 10 : 1;
    const map: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    const m = map[e.key];
    if (m && !e.metaKey && !e.ctrlKey) {
      const s = session;
      if (s.quad) s.quad = s.quad.map((q) => ({ x: q.x + m[0], y: q.y + m[1] }));
      else s.box = { ...s.box, cx: s.box.cx + m[0], cy: s.box.cy + m[1] };
      void hideOriginal(ed, s);
      sync(s);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    const s = ensure(ed);
    if (!s) return;
    const corners = cornersOf(s);
    const v = corners.map((q) => ed.toView(q.x, q.y));
    const moved = changed(s) || s.hidden;
    if (s.snapshot && moved && (!s.quad || s.initAxisAligned)) {
      ctx.save();
      ctx.imageSmoothingEnabled = true;
      ctx.globalAlpha = s.layer.opacity ?? 1;
      if (s.quad) {
        drawQuad(ctx, s.snapshot.canvas, s.snapshot.w, s.snapshot.h, v, 6);
      } else {
        const inv = invert(s.initMatrix);
        const m = inv ? mul(matrixOf(s), inv) : matrixOf(s);
        const z = ed.view.zoom;
        const o = ed.toView(0, 0);
        // view ∘ (M ∘ M₀⁻¹) ∘ (snapshot px → document px)
        const R = s.snapRect;
        ctx.transform(z, 0, 0, z, o.x, o.y);
        ctx.transform(m.a, m.b, m.c, m.d, m.e, m.f);
        ctx.transform(R.w / s.snapshot.w, 0, 0, R.h / s.snapshot.h, R.x, R.y);
        ctx.drawImage(s.snapshot.canvas, 0, 0);
      }
      ctx.restore();
    }
    ctx.save();
    ctx.strokeStyle = "#1473e6";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(v[0].x, v[0].y);
    for (const q of v.slice(1)) ctx.lineTo(q.x, q.y);
    ctx.closePath();
    ctx.stroke();
    const size = isCoarse() ? 14 : 8;
    const mids = [0, 1, 2, 3].map((i) => ({ x: (v[i].x + v[(i + 1) % 4].x) / 2, y: (v[i].y + v[(i + 1) % 4].y) / 2 }));
    for (const q of [...v, ...mids]) drawHandle(ctx, q.x, q.y, size);
    // Reference point.
    const c = { x: (v[0].x + v[2].x) / 2, y: (v[0].y + v[2].y) / 2 };
    ctx.strokeStyle = "rgba(0,0,0,0.7)";
    ctx.beginPath();
    ctx.arc(c.x, c.y, 4, 0, Math.PI * 2);
    ctx.stroke();
    ctx.restore();
    if (drag?.moved) {
      const label =
        drag.kind === "rotate"
          ? `${((s.box.angle * 180) / Math.PI).toFixed(1)}°`
          : drag.kind === "move"
            ? `Δx ${Math.round(s.box.cx - drag.box.cx)}  Δy ${Math.round(s.box.cy - drag.box.cy)}`
            : `W ${Math.round((s.box.w / s.src.w) * 100)}%  H ${Math.round((s.box.h / s.src.h) * 100)}%`;
      drawLabel(ctx, label, v[2].x, v[2].y);
    }
  },
};
