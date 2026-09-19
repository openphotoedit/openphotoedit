// Brush-driven tools: brush, pencil, eraser, clone stamp, healing brush,
// dodge, burn, sponge, blur, sharpen and smudge. Pointer samples are sent
// as `paint.stroke` segments once per animation frame with a shared
// stroke_id, then `paint.stroke-end`.

import type { EditorStore } from "../lib/editor.svelte";
import { paintTarget } from "../ui/paint-target.svelte";
import { strokeMask, drawStrokeTint } from "./stroke-mask";
import { t } from "../lib/i18n";
import type { Tool, ToolPointer } from "./types";
import { setSizeFor, sizeFor, toolSettings } from "./settings.svelte";
import { toolState } from "./state.svelte";
import { digitOpacity, drawBrushCircle, drawCrosshair, FrameBatcher, hover, mods, isUnknownOp, newStrokeId, notReady, redraw, rgba, setCursor, stepSize, trackHover, type Pt, exec, reportError, requirePixels } from "./common";
import { altEyedropper } from "./eyedropper-alt";
import { drawClonePreview } from "./clone-preview";

type EngineTool = "brush" | "pencil" | "eraser" | "dodge" | "burn" | "sponge" | "blur" | "sharpen" | "smudge" | "clone" | "heal";

interface StrokePoint {
  x: number;
  y: number;
  p: number;
}

interface Stroke {
  id: string;
  /** Where "Shift-click draws a line from the last stroke" remembers it. */
  key: string;
  last: StrokePoint | null;
  /** The last point handed to the batcher. */
  pushed: StrokePoint;
  /** Shift held mid-drag: the stroke runs horizontally or vertically from
   *  `anchor`; `horizontal` is decided once the pointer has moved 3 px. */
  axis: { anchor: StrokePoint; horizontal: boolean | null } | null;
  batch: FrameBatcher<StrokePoint>;
  failed: boolean;
  source: { dx: number; dy: number } | null;
}

interface PaintSpec {
  id: string;
  label: string;
  shortcut?: string;
  engine: EngineTool;
  feature: string;
}

// Clone/heal source, shared as Photoshop shares it between the two tools.
let sourcePoint: Pt | null = null;
let alignedOffset: { dx: number; dy: number } | null = null;
/** End of the last stroke per tool, layer and target (Shift-click lines). */
const lastEnd: Record<string, StrokePoint | null> = {};

function hardnessKey(tool: string): "eraserHardness" | "brushHardness" {
  return tool === "eraser" ? "eraserHardness" : "brushHardness";
}

function brushFor(ed: EditorStore, tool: string) {
  const s = toolSettings;
  const size = sizeFor(tool);
  if (tool === "eraser") {
    return {
      size,
      hardness: s.eraserMode === "pencil" ? 1 : s.eraserHardness,
      opacity: s.eraserOpacity,
      flow: s.eraserFlow,
      spacing: s.brushSpacing,
      color: rgba(ed.secondary),
      pressure_size: s.pressureSize,
      pressure_opacity: s.pressureOpacity,
    };
  }
  return {
    size,
    hardness: tool === "pencil" ? 1 : s.brushHardness,
    opacity: tool === "dodge" || tool === "burn" || tool === "sponge" || tool === "blur-brush" || tool === "sharpen-brush" || tool === "smudge" ? 1 : s.brushOpacity,
    flow: tool === "sponge" ? s.spongeFlow : s.brushFlow,
    spacing: s.brushSpacing,
    color: rgba(ed.primary),
    blend: tool === "brush" || tool === "pencil" ? s.brushBlend : "normal",
    pressure_size: s.pressureSize,
    pressure_opacity: s.pressureOpacity,
  };
}

function extras(tool: string, stroke: Stroke): Record<string, unknown> {
  const s = toolSettings;
  switch (tool) {
    case "dodge":
    case "burn":
      return { range: s.toneRange, exposure: s.toneExposure };
    case "sponge":
      return { sponge: s.spongeMode };
    case "blur-brush":
    case "sharpen-brush":
    case "smudge":
      return { strength: s.strength };
    case "clone":
    case "heal":
      return { source: { dx: stroke.source?.dx ?? 0, dy: stroke.source?.dy ?? 0, sample_all: s.cloneSampleAll } };
    default:
      return {};
  }
}

function makePaintTool(spec: PaintSpec): Tool & { keyup(ed: EditorStore, e: KeyboardEvent): boolean } {
  let stroke: Stroke | null = null;
  const needsSource = spec.engine === "clone" || spec.engine === "heal";

  function begin(ed: EditorStore, first: StrokePoint[]) {
    if (!requirePixels(ed)) return;
    let source: Stroke["source"] = null;
    if (needsSource) {
      if (!sourcePoint) {
        ed.toast(t("Alt-click (Option-click) to set where to sample from first."), "info");
        return;
      }
      const start = first[first.length - 1];
      if (toolSettings.cloneAligned && alignedOffset) source = alignedOffset;
      else source = { dx: sourcePoint.x - start.x, dy: sourcePoint.y - start.y };
      if (toolSettings.cloneAligned) alignedOffset = source;
    }
    const st: Stroke = { id: newStrokeId(spec.engine), key: endKey(ed), last: null, pushed: first[first.length - 1], axis: null, failed: false, source, batch: null as unknown as FrameBatcher<StrokePoint> };
    st.batch = new FrameBatcher<StrokePoint>(async (pts) => {
      if (st.failed) return;
      const points = st.last ? [st.last, ...pts] : pts;
      st.last = pts[pts.length - 1];
      try {
        await exec(ed, {
          op: "paint.stroke",
          target: target(ed),
          tool: spec.engine,
          brush: brushFor(ed, spec.id),
          points,
          stroke_id: st.id,
          ...extras(spec.id, st),
        });
      } catch (e) {
        st.failed = true;
        if (isUnknownOp(e)) notReady(ed, t(spec.feature));
        else reportError(ed, e);
      }
    });
    stroke = st;
    for (const p of first) st.batch.push(p);
  }

  async function end(ed: EditorStore) {
    const st = stroke;
    stroke = null;
    if (!st) return;
    await st.batch.done();
    lastEnd[st.key] = st.last;
    if (!st.failed) await exec(ed, { op: "paint.stroke-end", stroke_id: st.id }).catch(() => null);
    redraw(ed);
  }

  const sp = (p: ToolPointer): StrokePoint => ({ x: p.x, y: p.y, p: p.pointerType === "pen" ? p.pressure : 1 });
  const target = (ed: EditorStore) => (paintTarget.layerId == null || paintTarget.layerId === ed.summary?.active ? paintTarget.value : "pixels");
  const endKey = (ed: EditorStore) => `${spec.id}:${ed.summary?.active ?? ""}:${target(ed)}`;

  /**
   * Shift pressed mid-drag locks the stroke to horizontal or vertical from
   * the point where Shift went down. Returns the point to send, or null
   * while the axis is still undecided. The end of the stroke is the last
   * projected point: nothing off-axis is sent on release.
   */
  function axisLock(ed: EditorStore, st: Stroke, p: ToolPointer): StrokePoint | null {
    const q = sp(p);
    // Coalesced pointer events can carry stale modifiers; the keyboard's
    // state is the truth.
    if (!p.shift && !mods.shift) {
      st.axis = null;
      return q;
    }
    st.axis ??= { anchor: st.pushed, horizontal: null };
    const a = st.axis;
    if (a.horizontal == null) {
      const dx = q.x - a.anchor.x;
      const dy = q.y - a.anchor.y;
      if (Math.hypot(dx, dy) * ed.view.zoom < 3) return null;
      a.horizontal = Math.abs(dx) >= Math.abs(dy);
    }
    return a.horizontal ? { ...q, y: a.anchor.y } : { ...q, x: a.anchor.x };
  }

  // Alt-click samples colour instead of painting (brush and pencil), as
  // Photoshop's tools do.
  const altSamples = spec.engine === "brush" || spec.engine === "pencil";
  let sampling = false;

  // Quick Mask: brush, pencil and eraser edit the selection instead of pixels.
  // Painting white (or erasing) adds to the selection, painting dark removes
  // from it, as Photoshop's Quick Mask does.
  const quickMaskCapable = spec.engine === "brush" || spec.engine === "pencil" || spec.engine === "eraser";
  let qm: Pt[] | null = null;
  const inQuickMask = () => quickMaskCapable && paintTarget.quickMask;
  async function commitQuickMask(ed: EditorStore) {
    const pts = qm;
    qm = null;
    const s = ed.summary;
    if (!pts || !s) return;
    const size = sizeFor(spec.id);
    const m = strokeMask(pts, size, s.width, s.height);
    if (!m) return;
    const c = ed.primary;
    const light = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b >= 128;
    const mode = spec.engine === "eraser" || light ? "add" : "subtract";
    await exec(ed, { op: "select.mask", x: m.rect.x, y: m.rect.y, width: m.rect.w, height: m.rect.h, mode, label: t("Quick Mask") }, m.bytes);
    redraw(ed);
  }

  return {
    id: spec.id,
    label: spec.label,
    shortcut: spec.shortcut,
    cursor: "none",
    down(ed, p) {
      if (needsSource && p.alt) {
        sourcePoint = { x: p.x, y: p.y };
        alignedOffset = null;
        toolState.sourceSet = true;
        redraw(ed);
        return;
      }
      if (altSamples && p.alt) {
        sampling = true;
        void altEyedropper.down(ed, p);
        return;
      }
      if (inQuickMask()) {
        qm = [{ x: p.x, y: p.y }];
        redraw(ed);
        return;
      }
      const prev = lastEnd[endKey(ed)];
      if (p.shift && prev) begin(ed, [prev, sp(p)]);
      else begin(ed, [sp(p)]);
    },
    move(ed, p, pressed) {
      trackHover(ed, p);
      if (altSamples) setCursor(p.alt || sampling ? altEyedropper.cursor : null);
      if (sampling) {
        if (pressed) altEyedropper.move(ed, p);
        return;
      }
      if (pressed && qm) {
        qm.push({ x: p.x, y: p.y });
        redraw(ed);
        return;
      }
      if (pressed && stroke) {
        const q = axisLock(ed, stroke, p);
        if (q) {
          stroke.pushed = q;
          stroke.batch.push(q);
        }
      }
      if (needsSource && !stroke && sourcePoint) redraw(ed);
    },
    up(ed, p) {
      if (sampling) {
        sampling = false;
        return altEyedropper.up(ed, p);
      }
      if (qm) return commitQuickMask(ed);
      return end(ed);
    },
    cancel(ed) {
      if (sampling) altEyedropper.cancel(ed);
      sampling = false;
      if (stroke) void end(ed);
      redraw(ed);
    },
    deactivate(ed) {
      if (stroke) void end(ed);
      if (altSamples) setCursor(null);
    },
    keyup(_ed: EditorStore, e: KeyboardEvent) {
      if (altSamples && e.key === "Alt" && !sampling) setCursor(null);
      return false;
    },
    key(ed, e) {
      if (altSamples && e.key === "Alt" && hover.inside) {
        setCursor(altEyedropper.cursor);
        return false;
      }
      if (e.code === "BracketLeft" || e.code === "BracketRight") {
        const dir = e.code === "BracketRight" ? 1 : -1;
        if (e.shiftKey) {
          const k = hardnessKey(spec.id);
          toolSettings[k] = Math.max(0, Math.min(1, Math.round((toolSettings[k] + dir * 0.25) * 100) / 100));
        } else {
          setSizeFor(spec.id, stepSize(sizeFor(spec.id), dir));
        }
        redraw(ed);
        return true;
      }
      const o = digitOpacity(e);
      if (o != null) {
        switch (spec.id) {
          case "eraser":
            toolSettings.eraserOpacity = o;
            break;
          case "dodge":
          case "burn":
            toolSettings.toneExposure = o;
            break;
          case "sponge":
            toolSettings.spongeFlow = o;
            break;
          case "blur-brush":
          case "sharpen-brush":
          case "smudge":
            toolSettings.strength = o;
            break;
          default:
            toolSettings.brushOpacity = o;
        }
        return true;
      }
      return false;
    },
    overlay(ed, ctx) {
      if (qm) drawStrokeTint(ed, ctx, qm, sizeFor(spec.id), "rgba(255, 40, 40, 0.45)");
      if (sampling) {
        altEyedropper.overlay(ed, ctx);
        return;
      }
      if (!hover.inside) return;
      if (altSamples && ed.cursor === altEyedropper.cursor) return;
      const size = sizeFor(spec.id);
      if (needsSource && sourcePoint) {
        let src: Pt = sourcePoint;
        const off = stroke?.source ?? (toolSettings.cloneAligned ? alignedOffset : { dx: sourcePoint.x - hover.x, dy: sourcePoint.y - hover.y });
        if (off) src = { x: hover.x + off.dx, y: hover.y + off.dy };
        // Before painting, show what the brush would lay down.
        if (!stroke && spec.engine === "clone") drawClonePreview(ed, ctx, src, { x: hover.x, y: hover.y }, size, toolSettings.brushHardness, toolSettings.brushOpacity, toolSettings.cloneSampleAll);
        const q = ed.toView(src.x, src.y);
        drawCrosshair(ctx, q.x, q.y, 8);
      }
      drawBrushCircle(ctx, hover.vx, hover.vy, (size / 2) * ed.view.zoom, size * ed.view.zoom < 6);
    },
  };
}

export const brush = makePaintTool({ id: "brush", label: "Brush", shortcut: "b", engine: "brush", feature: "Brush" });
export const pencil = makePaintTool({ id: "pencil", label: "Pencil", engine: "pencil", feature: "Pencil" });
export const eraser = makePaintTool({ id: "eraser", label: "Eraser", shortcut: "e", engine: "eraser", feature: "Eraser" });
export const clone = makePaintTool({ id: "clone", label: "Clone stamp", shortcut: "s", engine: "clone", feature: "Clone stamp" });
export const heal = makePaintTool({ id: "heal", label: "Healing brush", engine: "heal", feature: "Healing brush" });
export const dodge = makePaintTool({ id: "dodge", label: "Dodge", shortcut: "o", engine: "dodge", feature: "Dodge" });
export const burn = makePaintTool({ id: "burn", label: "Burn", engine: "burn", feature: "Burn" });
export const sponge = makePaintTool({ id: "sponge", label: "Sponge", engine: "sponge", feature: "Sponge" });
export const blurBrush = makePaintTool({ id: "blur-brush", label: "Blur", engine: "blur", feature: "Blur brush" });
export const sharpenBrush = makePaintTool({ id: "sharpen-brush", label: "Sharpen", engine: "sharpen", feature: "Sharpen brush" });
export const smudge = makePaintTool({ id: "smudge", label: "Smudge", engine: "smudge", feature: "Smudge" });
