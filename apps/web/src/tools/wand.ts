// Magic wand (click a colour) and quick selection (paint an edge-aware
// selection). Both are backed by the select domain.

import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { setSizeFor, sizeFor, toolSettings } from "./settings.svelte";
import { drawBrushCircle, FrameBatcher, hover, newStrokeId, notReady, redraw, run, selectionMode, stepSize, trackHover, type Pt, exec, reportError } from "./common";

export const magicWand: Tool = {
  id: "magic-wand",
  label: "Magic wand",
  shortcut: "w",
  cursor: "crosshair",
  async down(ed, p) {
    const s = ed.summary;
    if (!s) return;
    if (p.x < 0 || p.y < 0 || p.x >= s.width || p.y >= s.height) return;
    const mode = s.selection && (p.shift || p.alt) ? selectionMode(p, "replace") : toolSettings.selectMode;
    await run(
      ed,
      {
        op: "select.magic-wand",
        x: Math.floor(p.x),
        y: Math.floor(p.y),
        tolerance: toolSettings.wandTolerance,
        contiguous: toolSettings.wandContiguous,
        sample_all: toolSettings.wandSampleAll,
        anti_alias: toolSettings.antiAlias,
        mode,
      },
      { feature: t("Magic wand") },
    );
    redraw(ed);
  },
};

let qs: { id: string; last: Pt | null; batch: FrameBatcher<Pt>; mode: "add" | "subtract"; failed: boolean } | null = null;

export const quickSelect: Tool = {
  id: "quick-select",
  label: "Quick selection",
  cursor: "none",
  down(ed, p) {
    const s = { id: newStrokeId("qs"), last: null as Pt | null, mode: (p.alt !== (toolSettings.selectMode === "subtract") ? "subtract" : "add") as "add" | "subtract", failed: false, batch: null as unknown as FrameBatcher<Pt> };
    s.batch = new FrameBatcher<Pt>(async (pts) => {
      if (s.failed) return;
      const points = s.last ? [s.last, ...pts] : pts;
      s.last = pts[pts.length - 1];
      try {
        await exec(ed, { op: "select.quick", points, radius: sizeFor("quick-select") / 2, mode: s.mode, stroke_id: s.id });
      } catch (e) {
        s.failed = true;
        if (/unknown operation/i.test(String(e))) notReady(ed, t("Quick selection"));
        else reportError(ed, e);
      }
    });
    qs = s;
    s.batch.push({ x: p.x, y: p.y });
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (pressed && qs) qs.batch.push({ x: p.x, y: p.y });
  },
  async up(ed) {
    const s = qs;
    qs = null;
    if (!s) return;
    await s.batch.done();
    await ed.engine.exec({ op: "edit.seal" }).catch(() => null);
    redraw(ed);
  },
  cancel(ed) {
    qs?.batch.clear();
    qs = null;
    redraw(ed);
  },
  key(ed, e) {
    if (e.code === "BracketLeft" || e.code === "BracketRight") {
      setSizeFor("quick-select", stepSize(sizeFor("quick-select"), e.code === "BracketRight" ? 1 : -1));
      redraw(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    if (!hover.inside) return;
    const r = (sizeFor("quick-select") / 2) * ed.view.zoom;
    drawBrushCircle(ctx, hover.vx, hover.vy, r);
    ctx.save();
    ctx.strokeStyle = "rgba(0,0,0,0.9)";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(hover.vx - 4, hover.vy);
    ctx.lineTo(hover.vx + 4, hover.vy);
    if (!qs || qs.mode === "add") {
      ctx.moveTo(hover.vx, hover.vy - 4);
      ctx.lineTo(hover.vx, hover.vy + 4);
    }
    ctx.stroke();
    ctx.restore();
  },
};
