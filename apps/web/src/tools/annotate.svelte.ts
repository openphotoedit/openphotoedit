// Live shapes and annotations: rectangle, ellipse, line, arrow, freehand
// pen, highlighter and redaction box. Drag to create (a preview draws on the
// overlay), release to add a shape layer. The new shape stays selected with
// handles: drag it to move, drag a handle to resize, change an option to
// restyle it, Delete removes it. Clicking an existing shape of the same kind
// selects it again. Every edit is an ordinary `layer.set-shape`.

import { editor, type EditorStore } from "../lib/editor.svelte";
import type { LayerInfo, Rgba8, ShapeData, ShapeKind } from "../engine/types";
import type { Tool, ToolPointer } from "./types";
import { fillColor, strokeColor, toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import {
  boxHandles,
  css,
  dist,
  distToSegment,
  drawHandle,
  HANDLE_CURSOR,
  HANDLE_IDS,
  isCoarse,
  layersTopDown,
  polylineBounds,
  rectFrom,
  redraw,
  resizeBox,
  rgba,
  run,
  seal,
  setActive,
  setCursor,
  smooth,
  snap45,
  thin,
  trackHover,
  type HandleId,
  type Pt,
} from "./common";

export type AnnotationToolId = "shape-rect" | "shape-ellipse" | "shape-line" | "arrow" | "pen" | "highlighter" | "redact";

const KIND: Record<AnnotationToolId, ShapeKind> = {
  "shape-rect": "rect",
  "shape-ellipse": "ellipse",
  "shape-line": "line",
  arrow: "arrow",
  pen: "polyline",
  highlighter: "polyline",
  redact: "redact",
};

type Drag =
  | { kind: "create"; tool: AnnotationToolId; start: Pt; startV: Pt; points: Pt[]; moved: boolean; shift: boolean; alt: boolean }
  | { kind: "move"; id: number; start: Pt; orig: ShapeData; data: ShapeData; moved: boolean }
  | { kind: "handle"; id: number; handle: string; start: Pt; orig: ShapeData; data: ShapeData; moved: boolean };

let drag: Drag | null = null;
let selected: number | null = null;
let chain: Promise<unknown> = Promise.resolve();
let raf = 0;
let pendingSend: { id: number; data: ShapeData } | null = null;

function selectedLayer(ed: EditorStore): LayerInfo | null {
  if (selected == null) return null;
  const l = layersTopDown(ed).find((x) => x.id === selected);
  if (!l || l.kind !== "shape" || !l.shape) {
    setSelected(null);
    return null;
  }
  return l;
}

function setSelected(id: number | null) {
  selected = id;
  toolState.annotationSelected = id;
}

/**
 * Markup widths (arrow, pen, highlighter) are relative to a 1000 px image so
 * they read the same on a screenshot and a 12 MP photo; Photoshop's shapes
 * keep absolute pixels.
 */
export function markupScale(): number {
  const s = editor.summary;
  return s ? Math.max(1, Math.max(s.width, s.height) / 1000) : 1;
}

/** Style from the options for a tool, applied over `base`. */
export function styleFor(tool: AnnotationToolId, base: Partial<ShapeData> = {}): ShapeData {
  const s = toolSettings;
  const kind = KIND[tool];
  const d: ShapeData = {
    kind,
    points: base.points ?? [],
    stroke: null,
    stroke_width: s.shapeWidth,
    fill: null,
    corner_radius: 0,
    arrow_start: false,
    arrow_end: false,
    dash: null,
    rotation: base.rotation ?? 0,
  };
  switch (tool) {
    case "shape-rect":
    case "shape-ellipse":
      d.stroke = s.shapeStrokeOn ? rgba(strokeColor()) : null;
      d.fill = s.shapeFillOn ? rgba(fillColor()) : null;
      d.corner_radius = tool === "shape-rect" ? s.shapeRadius : 0;
      d.dash = s.shapeDashed ? [s.shapeWidth * 2, s.shapeWidth * 1.5] : null;
      break;
    case "shape-line":
      d.stroke = rgba(strokeColor());
      d.dash = s.shapeDashed ? [s.shapeWidth * 2, s.shapeWidth * 1.5] : null;
      break;
    case "arrow":
      d.stroke = rgba(strokeColor());
      d.stroke_width = s.shapeWidth * markupScale();
      d.arrow_start = s.arrowStart;
      d.arrow_end = s.arrowEnd;
      d.dash = s.shapeDashed ? [d.stroke_width * 2, d.stroke_width * 1.5] : null;
      break;
    case "pen":
      d.stroke = rgba(strokeColor());
      d.stroke_width = s.penWidth * markupScale();
      break;
    case "highlighter":
      d.stroke = rgba(s.highlighterColor);
      d.stroke_width = s.highlighterWidth * markupScale();
      break;
    case "redact":
      d.fill = rgba(s.redactColor);
      d.stroke_width = 0;
      break;
  }
  return d;
}

/** Copy a shape's style into the options, so they show what is selected. */
function loadStyle(tool: AnnotationToolId, d: ShapeData) {
  const s = toolSettings;
  switch (tool) {
    case "shape-rect":
    case "shape-ellipse":
      s.shapeStrokeOn = !!d.stroke;
      if (d.stroke) s.shapeStroke = d.stroke;
      s.shapeFillOn = !!d.fill;
      if (d.fill) s.shapeFill = d.fill;
      s.shapeWidth = d.stroke_width;
      if (tool === "shape-rect") s.shapeRadius = d.corner_radius;
      s.shapeDashed = !!d.dash;
      break;
    case "shape-line":
    case "arrow":
      if (d.stroke) s.shapeStroke = d.stroke;
      s.shapeWidth = tool === "arrow" ? d.stroke_width / markupScale() : d.stroke_width;
      s.shapeDashed = !!d.dash;
      if (tool === "arrow") {
        s.arrowStart = d.arrow_start;
        s.arrowEnd = d.arrow_end;
      }
      break;
    case "pen":
      if (d.stroke) s.shapeStroke = d.stroke;
      s.penWidth = d.stroke_width / markupScale();
      break;
    case "highlighter":
      if (d.stroke) s.highlighterColor = d.stroke;
      s.highlighterWidth = d.stroke_width / markupScale();
      break;
    case "redact":
      if (d.fill) s.redactColor = d.fill;
      break;
  }
}

/** Deep equality with a tolerance, since the engine stores f32. */
function near(a: unknown, b: unknown): boolean {
  if (typeof a === "number" && typeof b === "number") return Math.abs(a - b) < 1e-3 * Math.max(1, Math.abs(a));
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return a === b;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  const keys = new Set([...ka, ...kb]);
  for (const k of keys) {
    const va = (a as Record<string, unknown>)[k];
    const vb = (b as Record<string, unknown>)[k];
    // A missing alpha means opaque.
    if (k === "a" && (va ?? 255) === (vb ?? 255)) continue;
    if (!near(va, vb)) return false;
  }
  return true;
}

function sameShape(a: ShapeData, b: ShapeData) {
  return near(a, b);
}

function send(ed: EditorStore, id: number, data: ShapeData) {
  pendingSend = { id, data };
  if (raf) return;
  raf = requestAnimationFrame(() => {
    raf = 0;
    const p = pendingSend;
    pendingSend = null;
    if (p) chain = chain.then(() => run(ed, { op: "layer.set-shape", id: p.id, data: p.data }));
  });
}

async function flushSend(ed: EditorStore) {
  if (raf) {
    cancelAnimationFrame(raf);
    raf = 0;
  }
  const p = pendingSend;
  pendingSend = null;
  if (p) chain = chain.then(() => run(ed, { op: "layer.set-shape", id: p.id, data: p.data }));
  await chain;
}

// Options edits restyle the selected shape.
$effect.root(() => {
  $effect(() => {
    const tool = editor.tool as AnnotationToolId;
    if (!(tool in KIND)) return;
    const next = styleFor(tool);
    const id = selected;
    if (id == null || drag) return;
    const l = selectedLayer(editor);
    if (!l?.shape || !toolKindMatches(tool, l)) return;
    const merged: ShapeData = { ...next, points: l.shape.points, rotation: l.shape.rotation };
    if (sameShape(merged, { ...l.shape })) return;
    void run(editor, { op: "layer.set-shape", id, data: merged });
  });
});

// ---------------------------------------------------------------------------
// Hit testing

function shapeHit(ed: EditorStore, d: ShapeData, p: ToolPointer): boolean {
  const tol = (isCoarse(p.pointerType) ? 14 : 6) / ed.view.zoom + d.stroke_width / 2;
  const pts = d.points;
  if (pts.length < 2) return false;
  switch (d.kind) {
    case "rect":
    case "redact":
    case "ellipse": {
      const r = rectFrom(pts[0], pts[1]);
      const inside = p.x >= r.x - tol && p.x <= r.x + r.w + tol && p.y >= r.y - tol && p.y <= r.y + r.h + tol;
      if (!inside) return false;
      if (d.fill || d.kind === "redact") return true;
      // Unfilled: only near the outline.
      if (d.kind === "ellipse") {
        const cx = r.x + r.w / 2;
        const cy = r.y + r.h / 2;
        const k = Math.hypot((p.x - cx) / Math.max(1, r.w / 2), (p.y - cy) / Math.max(1, r.h / 2));
        return Math.abs(k - 1) * Math.min(r.w, r.h) / 2 <= tol;
      }
      return p.x <= r.x + tol || p.x >= r.x + r.w - tol || p.y <= r.y + tol || p.y >= r.y + r.h - tol;
    }
    default:
      for (let i = 1; i < pts.length; i++) if (distToSegment(p, pts[i - 1], pts[i]) <= tol) return true;
      return false;
  }
}

function toolKindMatches(tool: AnnotationToolId, l: LayerInfo) {
  if (!l.shape || l.shape.kind !== KIND[tool]) return false;
  if (tool === "pen" || tool === "highlighter") {
    const translucent = (l.shape.stroke?.a ?? 255) < 255;
    return tool === "highlighter" ? translucent : !translucent;
  }
  return true;
}

function shapeHandles(d: ShapeData): Record<string, Pt> {
  if (d.kind === "line" || d.kind === "arrow") return { p0: d.points[0], p1: d.points[d.points.length - 1] };
  if (d.kind === "polyline" || d.kind === "polygon") return boxHandles(polylineBounds(d.points));
  return boxHandles(rectFrom(d.points[0], d.points[1]));
}

function hitShapeHandle(ed: EditorStore, d: ShapeData, p: ToolPointer): string | null {
  const rad = isCoarse(p.pointerType) ? 22 : 9;
  for (const [k, h] of Object.entries(shapeHandles(d))) {
    const v = ed.toView(h.x, h.y);
    if (Math.hypot(v.x - p.vx, v.y - p.vy) <= rad) return k;
  }
  return null;
}

function reshape(orig: ShapeData, handle: string, p: ToolPointer, start: Pt): ShapeData {
  const d = { ...orig, points: orig.points.map((q) => ({ ...q })) };
  if (d.kind === "line" || d.kind === "arrow") {
    const i = handle === "p0" ? 0 : d.points.length - 1;
    const other = d.points[i === 0 ? d.points.length - 1 : 0];
    d.points[i] = p.shift ? snap45(other, p) : { x: p.x, y: p.y };
    return d;
  }
  if (d.kind === "polyline" || d.kind === "polygon") {
    const b = polylineBounds(orig.points);
    const nb = resizeBox(b, handle as HandleId, p, { ratio: p.shift ? b.w / Math.max(1, b.h) : null, centre: p.alt });
    const sx = b.w ? nb.w / b.w : 1;
    const sy = b.h ? nb.h / b.h : 1;
    d.points = orig.points.map((q) => ({ x: nb.x + (q.x - b.x) * sx, y: nb.y + (q.y - b.y) * sy }));
    return d;
  }
  const r = rectFrom(orig.points[0], orig.points[1]);
  const nr = resizeBox(r, handle as HandleId, p, { ratio: p.shift ? r.w / Math.max(1, r.h) : null, centre: p.alt });
  void start;
  d.points = [
    { x: nr.x, y: nr.y },
    { x: nr.x + nr.w, y: nr.y + nr.h },
  ];
  return d;
}

// ---------------------------------------------------------------------------
// Creation geometry

function createPoints(tool: AnnotationToolId, d: Extract<Drag, { kind: "create" }>, p: Pt, shift: boolean, alt: boolean): Pt[] {
  switch (tool) {
    case "pen":
    case "highlighter":
      return d.points;
    case "shape-line":
    case "arrow":
      return [d.start, shift ? snap45(d.start, p) : p];
    default: {
      let dx = p.x - d.start.x;
      let dy = p.y - d.start.y;
      if (shift) {
        const m = Math.max(Math.abs(dx), Math.abs(dy));
        dx = Math.sign(dx || 1) * m;
        dy = Math.sign(dy || 1) * m;
      }
      if (alt) return [{ x: d.start.x - dx, y: d.start.y - dy }, { x: d.start.x + dx, y: d.start.y + dy }];
      return [d.start, { x: d.start.x + dx, y: d.start.y + dy }];
    }
  }
}

export function drawShapePreview(ed: EditorStore, ctx: CanvasRenderingContext2D, d: ShapeData) {
  const z = ed.view.zoom;
  const pts = d.points.map((q) => ed.toView(q.x, q.y));
  if (pts.length < 2) return;
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.lineWidth = Math.max(1, d.stroke_width * z);
  if (d.dash) ctx.setLineDash([d.dash[0] * z, d.dash[1] * z]);
  const stroke = d.stroke ? css(d.stroke) : null;
  const fill = d.kind === "redact" ? css(d.fill ?? { r: 0, g: 0, b: 0 }) : d.fill ? css(d.fill) : null;
  switch (d.kind) {
    case "rect":
    case "redact":
    case "ellipse": {
      const x = Math.min(pts[0].x, pts[1].x);
      const y = Math.min(pts[0].y, pts[1].y);
      const w = Math.abs(pts[1].x - pts[0].x);
      const h = Math.abs(pts[1].y - pts[0].y);
      ctx.beginPath();
      if (d.kind === "ellipse") ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
      else ctx.roundRect(x, y, w, h, Math.min(d.corner_radius * z, w / 2, h / 2));
      if (fill) {
        ctx.fillStyle = fill;
        ctx.fill();
      }
      if (stroke && d.kind !== "redact") {
        ctx.strokeStyle = stroke;
        ctx.stroke();
      }
      break;
    }
    case "line":
    case "arrow": {
      const a = pts[0];
      const e = pts[pts.length - 1];
      const head = (d.stroke_width * 3.2 + 6) * z;
      const len = Math.max(1e-6, Math.hypot(e.x - a.x, e.y - a.y));
      const ux = (e.x - a.x) / len;
      const uy = (e.y - a.y) / len;
      const arrow = d.kind === "arrow";
      const te = arrow && d.arrow_end ? head * 0.7 : 0;
      const ts = arrow && d.arrow_start ? head * 0.7 : 0;
      ctx.strokeStyle = stroke ?? "#000";
      ctx.beginPath();
      ctx.moveTo(a.x + ux * ts, a.y + uy * ts);
      ctx.lineTo(e.x - ux * te, e.y - uy * te);
      ctx.stroke();
      ctx.setLineDash([]);
      const drawHead = (tip: Pt, dx: number, dy: number) => {
        const bx = tip.x - dx * head;
        const by = tip.y - dy * head;
        const nx = -dy * head * 0.55;
        const ny = dx * head * 0.55;
        ctx.fillStyle = stroke ?? "#000";
        ctx.beginPath();
        ctx.moveTo(tip.x, tip.y);
        ctx.lineTo(bx + nx, by + ny);
        ctx.lineTo(bx - nx, by - ny);
        ctx.closePath();
        ctx.fill();
      };
      if (arrow && d.arrow_end) drawHead(e, ux, uy);
      if (arrow && d.arrow_start) drawHead(a, -ux, -uy);
      break;
    }
    default: {
      ctx.strokeStyle = stroke ?? "#000";
      ctx.beginPath();
      ctx.moveTo(pts[0].x, pts[0].y);
      for (const q of pts.slice(1)) ctx.lineTo(q.x, q.y);
      ctx.stroke();
    }
  }
  ctx.restore();
}

function drawSelection(ed: EditorStore, ctx: CanvasRenderingContext2D, d: ShapeData) {
  const hs = shapeHandles(d);
  ctx.save();
  if (d.kind !== "line" && d.kind !== "arrow") {
    const r = d.kind === "polyline" || d.kind === "polygon" ? polylineBounds(d.points) : rectFrom(d.points[0], d.points[1]);
    const a = ed.toView(r.x, r.y);
    const b = ed.toView(r.x + r.w, r.y + r.h);
    ctx.strokeStyle = "#1473e6";
    ctx.lineWidth = 1;
    ctx.strokeRect(Math.round(a.x) + 0.5, Math.round(a.y) + 0.5, Math.round(b.x - a.x), Math.round(b.y - a.y));
  }
  const size = isCoarse() ? 14 : 8;
  for (const h of Object.values(hs)) {
    const v = ed.toView(h.x, h.y);
    drawHandle(ctx, v.x, v.y, size, d.kind === "line" || d.kind === "arrow");
  }
  ctx.restore();
}

// ---------------------------------------------------------------------------

function makeAnnotationTool(id: AnnotationToolId, label: string, shortcut?: string): Tool {
  return {
    id,
    label,
    shortcut,
    cursor: "crosshair",
    activate(ed) {
      const l = selectedLayer(ed);
      if (l && !toolKindMatches(id, l)) setSelected(null);
      redraw(ed);
    },
    deactivate(ed) {
      void flushSend(ed);
      drag = null;
      setSelected(null);
      redraw(ed);
    },
    down(ed, p) {
      const pt = { x: p.x, y: p.y };
      // The selected shape's handles and body first.
      const sel = selectedLayer(ed);
      if (sel?.shape) {
        const h = hitShapeHandle(ed, sel.shape, p);
        if (h) {
          drag = { kind: "handle", id: sel.id, handle: h, start: pt, orig: sel.shape, data: sel.shape, moved: false };
          return;
        }
        if (shapeHit(ed, sel.shape, p) || (sel.shape.kind === "polyline" && inBox(polylineBounds(sel.shape.points), p))) {
          drag = { kind: "move", id: sel.id, start: pt, orig: sel.shape, data: sel.shape, moved: false };
          return;
        }
      }
      // Another shape of this kind under the pointer.
      for (const l of layersTopDown(ed)) {
        if (!l.visible || l.kind !== "shape" || !l.shape || !toolKindMatches(id, l)) continue;
        if (l.locks.all) continue;
        if (shapeHit(ed, l.shape, p)) {
          setSelected(l.id);
          loadStyle(id, l.shape);
          void setActive(ed, l.id);
          drag = { kind: "move", id: l.id, start: pt, orig: l.shape, data: l.shape, moved: false };
          return;
        }
      }
      setSelected(null);
      drag = { kind: "create", tool: id, start: pt, startV: { x: p.vx, y: p.vy }, points: [pt], moved: false, shift: p.shift, alt: p.alt };
    },
    move(ed, p, pressed) {
      trackHover(ed, p);
      if (!pressed || !drag) {
        const sel = selectedLayer(ed);
        if (sel?.shape) {
          const h = hitShapeHandle(ed, sel.shape, p);
          if (h) setCursor(h === "p0" || h === "p1" ? "move" : HANDLE_CURSOR[h as HandleId] ?? "move");
          else setCursor(shapeHit(ed, sel.shape, p) ? "move" : "crosshair");
        } else setCursor("crosshair");
        return;
      }
      const d = drag;
      if (d.kind === "create") {
        if (!d.moved && Math.hypot(p.vx - d.startV.x, p.vy - d.startV.y) < 3) return;
        d.moved = true;
        d.shift = p.shift;
        d.alt = p.alt;
        if (id === "pen" || id === "highlighter") {
          const last = d.points[d.points.length - 1];
          if (dist(last, p) * ed.view.zoom >= 1.5) d.points.push(p.shift ? snap45(d.points[0], p) : { x: p.x, y: p.y });
        } else {
          d.points = createPoints(id, d, { x: p.x, y: p.y }, p.shift, p.alt);
        }
        return;
      }
      if (d.kind === "move") {
        let dx = p.x - d.start.x;
        let dy = p.y - d.start.y;
        if (p.shift) {
          if (Math.abs(dx) > Math.abs(dy)) dy = 0;
          else dx = 0;
        }
        if (!d.moved && Math.hypot(dx, dy) * ed.view.zoom < 2) return;
        d.moved = true;
        d.data = { ...d.orig, points: d.orig.points.map((q) => ({ x: q.x + dx, y: q.y + dy })) };
        send(ed, d.id, d.data);
        return;
      }
      d.moved = true;
      d.data = reshape(d.orig, d.handle, p, d.start);
      send(ed, d.id, d.data);
    },
    async up(ed) {
      const d = drag;
      drag = null;
      if (!d) return;
      if (d.kind !== "create") {
        await flushSend(ed);
        await seal(ed);
        redraw(ed);
        return;
      }
      if (!d.moved) {
        redraw(ed);
        return;
      }
      let points = d.points;
      if (id === "pen" || id === "highlighter") {
        points = thin(points, 1 / Math.max(ed.view.zoom, 0.01));
        points = smooth(points, id === "pen" ? toolSettings.penSmoothing : 0.6);
        if (points.length < 2) return;
      } else {
        const r = rectFrom(points[0], points[1]);
        if (Math.max(r.w, r.h) * ed.view.zoom < 3) return;
      }
      const data = styleFor(id, { points: points.map((q) => ({ x: Math.round(q.x * 10) / 10, y: Math.round(q.y * 10) / 10 })) });
      const r = await run(ed, { op: "layer.add-shape", data });
      const newId = (r?.data as { id?: number } | null)?.id;
      if (newId != null) setSelected(newId);
      redraw(ed);
    },
    cancel(ed) {
      if (drag?.kind === "create") drag = null;
      else if (!drag) setSelected(null);
      redraw(ed);
    },
    key(ed, e) {
      const sel = selectedLayer(ed);
      if (!sel) return false;
      if (e.key === "Delete" || e.key === "Backspace") {
        setSelected(null);
        void run(ed, { op: "layer.delete", ids: [sel.id] });
        return true;
      }
      if (e.key === "Enter") {
        setSelected(null);
        redraw(ed);
        return true;
      }
      const step = e.shiftKey ? 10 : 1;
      const map: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
      const m = map[e.key];
      if (m && !e.metaKey && !e.ctrlKey) {
        void run(ed, { op: "layer.offset", ids: [sel.id], dx: m[0], dy: m[1] });
        return true;
      }
      return false;
    },
    overlay(ed, ctx) {
      const d = drag;
      if (d?.kind === "create" && d.moved) {
        const pts = id === "pen" || id === "highlighter" ? d.points : d.points;
        drawShapePreview(ed, ctx, styleFor(id, { points: pts }));
        return;
      }
      const sel = selectedLayer(ed);
      if (!sel?.shape) return;
      const data = d && d.kind !== "create" ? d.data : sel.shape;
      drawSelection(ed, ctx, data);
    },
  };
}

function inBox(r: { x: number; y: number; w: number; h: number }, p: Pt) {
  return p.x >= r.x && p.y >= r.y && p.x <= r.x + r.w && p.y <= r.y + r.h;
}

export const shapeRect = makeAnnotationTool("shape-rect", "Rectangle", "u");
export const shapeEllipse = makeAnnotationTool("shape-ellipse", "Ellipse");
export const shapeLine = makeAnnotationTool("shape-line", "Line");
export const arrow = makeAnnotationTool("arrow", "Arrow");
export const pen = makeAnnotationTool("pen", "Pen");
export const highlighter = makeAnnotationTool("highlighter", "Highlighter");
export const redact = makeAnnotationTool("redact", "Redact");
