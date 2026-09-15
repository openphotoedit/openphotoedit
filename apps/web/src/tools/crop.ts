// Crop: a box over the whole canvas with eight handles, darkened outside,
// a rule-of-thirds grid while dragging, ratio lock, rotation by dragging
// outside the box, and a straighten line. Enter, double-click or the Apply
// button commits; Escape resets.

import type { EditorStore } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import type { Tool, ToolPointer } from "./types";
import { toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import {
  boxHandles,
  drawHandle,
  drawLabel,
  HANDLE_CURSOR,
  HANDLE_IDS,
  handleRadius,
  hover,
  isCoarse,
  redraw,
  resizeBox,
  rotatePt,
  run,
  setCursor,
  trackHover,
  type HandleId,
  type Pt,
} from "./common";

interface CropBox {
  cx: number;
  cy: number;
  w: number;
  h: number;
  /** Clockwise degrees. */
  angle: number;
}

type DragKind = "draw" | "move" | "resize" | "rotate" | "straighten";

let box: CropBox | null = null;
let docKey = "";
let touched = false;
let drag: { kind: DragKind; handle?: HandleId; orig: CropBox; start: Pt; startV: Pt; end: Pt; moved: boolean } | null = null;
let lastDown = 0;

const RAD = Math.PI / 180;

function key(ed: EditorStore) {
  const s = ed.summary;
  return s ? `${s.width}x${s.height}` : "";
}

function fullBox(ed: EditorStore): CropBox {
  const s = ed.summary!;
  return { cx: s.width / 2, cy: s.height / 2, w: s.width, h: s.height, angle: 0 };
}

function ensure(ed: EditorStore) {
  if (!ed.summary || !ed.hasDocument) return null;
  const k = key(ed);
  if (!box || k !== docKey) {
    box = fullBox(ed);
    docKey = k;
    touched = false;
    syncState(ed);
  }
  return box;
}

function syncState(ed: EditorStore) {
  const b = box;
  const full = ed.summary && b && !touched;
  toolState.cropPending = !!b && !full;
  toolState.cropInfo = b ? { w: Math.round(b.w), h: Math.round(b.h), angle: Math.round(b.angle * 10) / 10 } : { w: 0, h: 0, angle: 0 };
}

/** Width/height ratio the box is locked to, or null. */
export function cropRatio(ed: EditorStore): number | null {
  const s = toolSettings;
  if (s.cropOutW > 0 && s.cropOutH > 0) return s.cropOutW / s.cropOutH;
  switch (s.cropRatio) {
    case "free":
      return null;
    case "original":
      return ed.summary ? ed.summary.width / ed.summary.height : null;
    case "custom":
      return s.cropCustomW > 0 && s.cropCustomH > 0 ? s.cropCustomW / s.cropCustomH : null;
    default: {
      const [a, b] = s.cropRatio.split(":").map(Number);
      return a / b;
    }
  }
}

function localRect(b: CropBox) {
  return { x: b.cx - b.w / 2, y: b.cy - b.h / 2, w: b.w, h: b.h };
}

function toLocal(b: CropBox, p: Pt): Pt {
  return rotatePt(p, { x: b.cx, y: b.cy }, -b.angle * RAD);
}

function toWorld(b: CropBox, p: Pt): Pt {
  return rotatePt(p, { x: b.cx, y: b.cy }, b.angle * RAD);
}

function corners(b: CropBox): Pt[] {
  const r = localRect(b);
  return [
    { x: r.x, y: r.y },
    { x: r.x + r.w, y: r.y },
    { x: r.x + r.w, y: r.y + r.h },
    { x: r.x, y: r.y + r.h },
  ].map((p) => toWorld(b, p));
}

function hitHandleLocal(ed: EditorStore, b: CropBox, p: ToolPointer): HandleId | null {
  const hs = boxHandles(localRect(b));
  const rad = handleRadius(p.pointerType) + 2;
  for (const id of HANDLE_IDS) {
    const q = toWorld(b, hs[id]);
    const v = ed.toView(q.x, q.y);
    if (Math.hypot(v.x - p.vx, v.y - p.vy) <= rad) return id;
  }
  return null;
}

function insideLocal(b: CropBox, p: Pt) {
  const l = toLocal(b, p);
  const r = localRect(b);
  return l.x >= r.x && l.x <= r.x + r.w && l.y >= r.y && l.y <= r.y + r.h;
}

/** Largest box of the document's aspect that fits the image rotated by `deg`. */
function straightenedBox(ed: EditorStore, deg: number): CropBox {
  const s = ed.summary!;
  const W = s.width;
  const H = s.height;
  const c = Math.abs(Math.cos(deg * RAD));
  const si = Math.abs(Math.sin(deg * RAD));
  const k = Math.min(W / (W * c + H * si), H / (W * si + H * c));
  return { cx: W / 2, cy: H / 2, w: W * k, h: H * k, angle: deg };
}

export async function commitCrop(ed: EditorStore) {
  const b = ensure(ed);
  const s = ed.summary;
  if (!b || !s) return;
  if (!touched) {
    const out = toolSettings.cropOutW > 0 && toolSettings.cropOutH > 0;
    if (!out) return;
  }
  let cx = b.cx;
  let cy = b.cy;
  if (Math.abs(b.angle) > 0.01) {
    const r = await run(ed, { op: "image.rotate-arbitrary", degrees: -b.angle, expand: false });
    if (!r) return;
    const c = rotatePt({ x: cx, y: cy }, { x: s.width / 2, y: s.height / 2 }, -b.angle * RAD);
    cx = c.x;
    cy = c.y;
  }
  const w = Math.max(1, Math.round(b.w));
  const h = Math.max(1, Math.round(b.h));
  const x = Math.round(cx - b.w / 2);
  const y = Math.round(cy - b.h / 2);
  const needsCrop = x !== 0 || y !== 0 || w !== s.width || h !== s.height;
  if (needsCrop) {
    const r = await run(ed, { op: "image.crop", x, y, width: w, height: h, delete_cropped: toolSettings.cropDeleteCropped });
    if (!r) return;
  }
  if (toolSettings.cropOutW > 0 && toolSettings.cropOutH > 0 && (toolSettings.cropOutW !== w || toolSettings.cropOutH !== h)) {
    await run(ed, { op: "image.resize", width: Math.round(toolSettings.cropOutW), height: Math.round(toolSettings.cropOutH), resample: "bicubic" });
  }
  box = null;
  ensure(ed);
  ed.fit();
  redraw(ed);
}

export function resetCrop(ed: EditorStore) {
  box = null;
  drag = null;
  toolState.cropStraighten = false;
  ensure(ed);
  redraw(ed);
}

/** Swap the box between landscape and portrait (Photoshop's X). */
export function swapCropOrientation(ed: EditorStore) {
  const b = ensure(ed);
  if (!b) return;
  box = { ...b, w: b.h, h: b.w };
  touched = true;
  if (toolSettings.cropRatio === "custom") {
    [toolSettings.cropCustomW, toolSettings.cropCustomH] = [toolSettings.cropCustomH, toolSettings.cropCustomW];
  }
  if (toolSettings.cropOutW > 0 && toolSettings.cropOutH > 0) {
    [toolSettings.cropOutW, toolSettings.cropOutH] = [toolSettings.cropOutH, toolSettings.cropOutW];
  }
  syncState(ed);
  redraw(ed);
}

/** Re-apply the ratio setting to the current box (called when it changes). */
export function applyCropRatio(ed: EditorStore) {
  const b = ensure(ed);
  const ratio = cropRatio(ed);
  if (!b || !ratio) return;
  const s = ed.summary!;
  // Largest box of that ratio inside the canvas, centred where the box is.
  let w = s.width;
  let h = w / ratio;
  if (h > s.height) {
    h = s.height;
    w = h * ratio;
  }
  box = { cx: s.width / 2, cy: s.height / 2, w, h, angle: b.angle };
  touched = true;
  syncState(ed);
  redraw(ed);
}

function cursorFor(ed: EditorStore, p: ToolPointer) {
  const b = box;
  if (!b) return "crosshair";
  if (toolState.cropStraighten || p.mod) return "crosshair";
  const h = hitHandleLocal(ed, b, p);
  if (h) {
    // Rotate the cursor direction with the box, roughly.
    const order: HandleId[] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
    const k = (order.indexOf(h) + Math.round(b.angle / 45) + 8) % 8;
    return HANDLE_CURSOR[order[k]];
  }
  if (insideLocal(b, p)) return touched ? "move" : "crosshair";
  return "alias";
}

export const crop: Tool = {
  id: "crop",
  label: "Crop",
  shortcut: "c",
  cursor: "crosshair",
  activate(ed) {
    ensure(ed);
    redraw(ed);
  },
  deactivate(ed) {
    box = null;
    drag = null;
    toolState.cropPending = false;
    toolState.cropStraighten = false;
    setCursor("crosshair");
    redraw(ed);
  },
  down(ed, p) {
    const b = ensure(ed);
    if (!b) return;
    const now = performance.now();
    const dbl = now - lastDown < 320;
    lastDown = now;
    if (dbl && insideLocal(b, p) && touched) {
      drag = null;
      void commitCrop(ed);
      return;
    }
    const start = { x: p.x, y: p.y };
    const base = { kind: "move" as DragKind, orig: { ...b }, start, startV: { x: p.vx, y: p.vy }, end: start, moved: false };
    if (toolState.cropStraighten || p.mod) {
      drag = { ...base, kind: "straighten" };
      return;
    }
    const h = hitHandleLocal(ed, b, p);
    if (h) drag = { ...base, kind: "resize", handle: h };
    else if (insideLocal(b, p)) drag = { ...base, kind: touched ? "move" : "draw" };
    else drag = { ...base, kind: "rotate" };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    const b = ensure(ed);
    if (!b) return;
    if (!pressed || !drag) {
      setCursor(cursorFor(ed, p));
      return;
    }
    const d = drag;
    d.end = { x: p.x, y: p.y };
    if (!d.moved && Math.hypot(p.vx - d.startV.x, p.vy - d.startV.y) < 3) return;
    d.moved = true;
    const ratio = cropRatio(ed) ?? (p.shift && d.kind !== "draw" ? d.orig.w / d.orig.h : null);
    const o = d.orig;
    switch (d.kind) {
      case "draw": {
        let dx = p.x - d.start.x;
        let dy = p.y - d.start.y;
        const r = ratio ?? (p.shift ? 1 : null);
        if (r) {
          const w = Math.max(Math.abs(dx), Math.abs(dy) * r);
          dx = Math.sign(dx || 1) * w;
          dy = Math.sign(dy || 1) * (w / r);
        }
        if (p.alt) box = { cx: d.start.x, cy: d.start.y, w: Math.abs(dx) * 2, h: Math.abs(dy) * 2, angle: 0 };
        else box = { cx: d.start.x + dx / 2, cy: d.start.y + dy / 2, w: Math.abs(dx), h: Math.abs(dy), angle: 0 };
        touched = true;
        break;
      }
      case "move": {
        box = { ...o, cx: o.cx + p.x - d.start.x, cy: o.cy + p.y - d.start.y };
        break;
      }
      case "resize": {
        const lp = toLocal(o, p);
        const r = resizeBox(localRect(o), d.handle!, lp, { ratio, centre: p.alt });
        const c = toWorld(o, { x: r.x + r.w / 2, y: r.y + r.h / 2 });
        box = { cx: c.x, cy: c.y, w: Math.max(1, r.w), h: Math.max(1, r.h), angle: o.angle };
        touched = true;
        break;
      }
      case "rotate": {
        const a0 = Math.atan2(d.start.y - o.cy, d.start.x - o.cx);
        const a1 = Math.atan2(p.y - o.cy, p.x - o.cx);
        let ang = o.angle + (a1 - a0) / RAD;
        ang = ((((ang + 180) % 360) + 360) % 360) - 180;
        if (p.shift) ang = Math.round(ang / 15) * 15;
        // Shrink the box so no empty corners come into the crop.
        const s = ed.summary!;
        const c = Math.abs(Math.cos(ang * RAD));
        const si = Math.abs(Math.sin(ang * RAD));
        const k = Math.min(1, s.width / (o.w * c + o.h * si), s.height / (o.w * si + o.h * c));
        box = { ...o, w: o.w * k, h: o.h * k, angle: ang };
        touched = true;
        break;
      }
      case "straighten":
        break;
    }
    syncState(ed);
  },
  up(ed, p) {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.kind === "straighten") {
      if (d.moved) {
        let ang = Math.atan2(p.y - d.start.y, p.x - d.start.x) / RAD;
        // Near-vertical lines straighten verticals.
        if (ang > 45) ang -= 90;
        if (ang < -45) ang += 90;
        if (ang > 45) ang -= 90;
        if (ang < -45) ang += 90;
        box = straightenedBox(ed, ang);
        touched = true;
      }
      toolState.cropStraighten = false;
    } else if (d.kind === "draw" && (!box || box.w < 2 || box.h < 2)) {
      box = d.orig;
    }
    syncState(ed);
    redraw(ed);
  },
  cancel(ed) {
    resetCrop(ed);
  },
  key(ed, e) {
    if (e.key === "Enter") {
      void commitCrop(ed);
      return true;
    }
    if ((e.key === "x" || e.key === "X") && !e.metaKey && !e.ctrlKey) {
      swapCropOrientation(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    const b = ensure(ed);
    if (!b) return;
    const pts = corners(b).map((q) => ed.toView(q.x, q.y));
    const W = ed.viewport.width;
    const H = ed.viewport.height;
    ctx.save();
    // Shade outside the box.
    ctx.fillStyle = drag ? "rgba(0,0,0,0.4)" : "rgba(0,0,0,0.6)";
    ctx.beginPath();
    ctx.rect(0, 0, W, H);
    ctx.moveTo(pts[0].x, pts[0].y);
    for (let i = 3; i >= 0; i--) ctx.lineTo(pts[i].x, pts[i].y);
    ctx.closePath();
    ctx.fill("evenodd");
    // Grid while adjusting.
    const overlay = toolSettings.cropOverlay;
    if (drag?.moved && overlay !== "none" && drag.kind !== "straighten") {
      const n = overlay === "grid" ? 8 : 3;
      ctx.strokeStyle = "rgba(255,255,255,0.55)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let i = 1; i < n; i++) {
        const k = i / n;
        const a = lerp(pts[0], pts[1], k);
        const bb = lerp(pts[3], pts[2], k);
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(bb.x, bb.y);
        const c = lerp(pts[0], pts[3], k);
        const dd = lerp(pts[1], pts[2], k);
        ctx.moveTo(c.x, c.y);
        ctx.lineTo(dd.x, dd.y);
      }
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(255,255,255,0.95)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(pts[0].x, pts[0].y);
    for (const q of pts.slice(1)) ctx.lineTo(q.x, q.y);
    ctx.closePath();
    ctx.stroke();
    // Handles: corner brackets and edge bars, like Photoshop.
    const hs = boxHandles(localRect(b));
    const size = isCoarse() ? 14 : 9;
    for (const id of HANDLE_IDS) {
      const q = toWorld(b, hs[id]);
      const v = ed.toView(q.x, q.y);
      drawHandle(ctx, v.x, v.y, size);
    }
    if (drag?.kind === "straighten" && drag.moved) {
      const a = ed.toView(drag.start.x, drag.start.y);
      const e = ed.toView(drag.end.x, drag.end.y);
      ctx.strokeStyle = "#ffffff";
      ctx.lineWidth = 2;
      ctx.setLineDash([6, 4]);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(e.x, e.y);
      ctx.stroke();
      ctx.setLineDash([]);
      const ang = Math.atan2(drag.end.y - drag.start.y, drag.end.x - drag.start.x) / RAD;
      drawLabel(ctx, `${ang.toFixed(1)}°`, e.x, e.y);
    } else if (drag?.moved) {
      const label = drag.kind === "rotate" ? `${b.angle.toFixed(1)}°` : `${Math.round(b.w)} × ${Math.round(b.h)} px`;
      drawLabel(ctx, label, hover.vx, hover.vy);
    }
    ctx.restore();
  },
};

function lerp(a: Pt, b: Pt, k: number): Pt {
  return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
}

export const cropHelp = () => t("Drag the handles to crop, drag outside to rotate, press Enter to apply.");
