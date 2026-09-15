// Gradient: drag a line; a translucent preview of the gradient shows over
// the selection (or the canvas) until release sends `paint.gradient`.

import type { EditorStore } from "../lib/editor.svelte";
import { paintTarget } from "../ui/paint-target.svelte";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { gradientStops, toolSettings } from "./settings.svelte";
import { css, drawHandle, drawLabel, redraw, requirePixels, run, snap45, trackHover, type Pt } from "./common";

let drag: { from: Pt; to: Pt; moved: boolean } | null = null;

function previewFill(ed: EditorStore, ctx: CanvasRenderingContext2D, from: Pt, to: Pt): CanvasFillStrokeStyles["fillStyle"] | null {
  const a = ed.toView(from.x, from.y);
  const b = ed.toView(to.x, to.y);
  let stops = gradientStops();
  if (toolSettings.gradientReverse) stops = stops.map((s) => ({ pos: 1 - s.pos, color: s.color })).reverse();
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  let g: CanvasGradient;
  switch (toolSettings.gradientType) {
    case "radial":
      g = ctx.createRadialGradient(a.x, a.y, 0, a.x, a.y, Math.max(1, len));
      break;
    case "angle":
      g = ctx.createConicGradient(Math.atan2(b.y - a.y, b.x - a.x), a.x, a.y);
      break;
    case "reflected": {
      g = ctx.createLinearGradient(a.x - (b.x - a.x), a.y - (b.y - a.y), b.x, b.y);
      const mirrored = [...stops.map((s) => ({ pos: 0.5 - s.pos / 2, color: s.color })).reverse(), ...stops.map((s) => ({ pos: 0.5 + s.pos / 2, color: s.color }))];
      for (const s of mirrored) g.addColorStop(Math.max(0, Math.min(1, s.pos)), css(s.color));
      return g;
    }
    case "diamond":
      // The canvas has no diamond gradient; radial reads close enough for a preview.
      g = ctx.createRadialGradient(a.x, a.y, 0, a.x, a.y, Math.max(1, len));
      break;
    default:
      g = ctx.createLinearGradient(a.x, a.y, b.x, b.y);
  }
  for (const s of stops) g.addColorStop(Math.max(0, Math.min(1, s.pos)), css(s.color));
  return g;
}

export const gradient: Tool = {
  id: "gradient",
  label: "Gradient",
  shortcut: "g",
  cursor: "crosshair",
  down(ed, p) {
    if (!requirePixels(ed)) return;
    drag = { from: { x: p.x, y: p.y }, to: { x: p.x, y: p.y }, moved: false };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) return;
    drag.to = p.shift ? snap45(drag.from, p) : { x: p.x, y: p.y };
    const a = ed.toView(drag.from.x, drag.from.y);
    if (Math.hypot(p.vx - a.x, p.vy - a.y) > 3) drag.moved = true;
  },
  async up(ed, p) {
    const d = drag;
    drag = null;
    redraw(ed);
    if (!d || !d.moved) return;
    const to = p.shift ? snap45(d.from, p) : { x: p.x, y: p.y };
    await run(
      ed,
      {
        op: "paint.gradient",
        target: paintTarget.layerId == null || paintTarget.layerId === ed.summary?.active ? paintTarget.value : "pixels",
        from: d.from,
        to,
        gradient: toolSettings.gradientType,
        stops: gradientStops(),
        opacity: toolSettings.gradientOpacity,
        blend: toolSettings.brushBlend,
        reverse: toolSettings.gradientReverse,
      },
      { feature: t("Gradient") },
    );
    redraw(ed);
  },
  cancel(ed) {
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    const d = drag;
    if (!d?.moved || !ed.summary) return;
    const s = ed.summary;
    const area = s.selection?.bounds ?? { x: 0, y: 0, w: s.width, h: s.height };
    const tl = ed.toView(area.x, area.y);
    const br = ed.toView(area.x + area.w, area.y + area.h);
    ctx.save();
    ctx.globalAlpha = 0.6 * toolSettings.gradientOpacity;
    const fill = previewFill(ed, ctx, d.from, d.to);
    if (fill) {
      ctx.fillStyle = fill;
      ctx.fillRect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
    }
    ctx.restore();
    const a = ed.toView(d.from.x, d.from.y);
    const b = ed.toView(d.to.x, d.to.y);
    ctx.save();
    for (const [col, w] of [["rgba(0,0,0,0.8)", 3], ["#ffffff", 1]] as const) {
      ctx.strokeStyle = col;
      ctx.lineWidth = w;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    }
    ctx.restore();
    drawHandle(ctx, a.x, a.y, 8, true);
    drawHandle(ctx, b.x, b.y, 8, true);
    const ang = (Math.atan2(d.to.y - d.from.y, d.to.x - d.from.x) * 180) / Math.PI;
    drawLabel(ctx, `${Math.round(Math.hypot(d.to.x - d.from.x, d.to.y - d.from.y))} px  ${ang.toFixed(0)}°`, b.x, b.y);
  },
};
