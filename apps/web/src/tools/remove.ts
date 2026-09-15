// Remove: paint over something (a red tint shows the stroke), release, and
// the AI fills it in. The stroke becomes a temporary selection; whatever
// was selected before comes back afterwards.

import * as ai from "../lib/ai";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { setSizeFor, sizeFor, toolSettings } from "./settings.svelte";
import { dist, drawBrushCircle, hover, isNotReady, notReady, redraw, restoreSelection, run, saveSelection, stepSize, trackHover, type Pt, reportError } from "./common";
import { drawStrokeTint, strokeMask } from "./stroke-mask";

let points: Pt[] | null = null;
let working = false;

export const remove: Tool = {
  id: "remove",
  label: "Remove",
  cursor: "none",
  down(_ed, p) {
    if (working) return;
    points = [{ x: p.x, y: p.y }];
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (pressed && points) {
      const last = points[points.length - 1];
      if (dist(last, p) * ed.view.zoom >= 1.5) points.push({ x: p.x, y: p.y });
    }
  },
  async up(ed) {
    const pts = points;
    const s = ed.summary;
    if (!pts || !s || working) return;
    const mask = strokeMask(pts, sizeFor("remove"), s.width, s.height);
    if (!mask) {
      points = null;
      redraw(ed);
      return;
    }
    working = true;
    let saved = null;
    try {
      saved = await saveSelection(ed);
      const sel = await run(ed, { op: "select.mask", x: mask.rect.x, y: mask.rect.y, width: mask.rect.w, height: mask.rect.h, mode: "replace", label: "Remove Area" }, { bytes: mask.bytes });
      if (!sel) return;
      try {
        await ai.removeSelected({ quality: toolSettings.removeQuality });
      } catch (e) {
        if (isNotReady(e)) notReady(ed, t("Remove"));
        else reportError(ed, e);
      }
    } finally {
      if (saved) await restoreSelection(ed, saved).catch(() => undefined);
      working = false;
      points = null;
      redraw(ed);
    }
  },
  cancel(ed) {
    if (!working) points = null;
    redraw(ed);
  },
  key(ed, e) {
    if (e.code === "BracketLeft" || e.code === "BracketRight") {
      setSizeFor("remove", stepSize(sizeFor("remove"), e.code === "BracketRight" ? 1 : -1));
      redraw(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    const size = sizeFor("remove");
    if (points) drawStrokeTint(ed, ctx, points, size, working ? "rgba(255,59,48,0.25)" : "rgba(255,59,48,0.45)");
    if (hover.inside && !working) drawBrushCircle(ctx, hover.vx, hover.vy, (size / 2) * ed.view.zoom);
  },
};
