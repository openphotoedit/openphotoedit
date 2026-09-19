// Spot healing: click a blemish, or paint over a scratch. A click heals a
// disc at the brush radius; a painted stroke is rasterised to a coverage
// mask on release and healed in one `filter.spot-heal` (one undo step).

import type { EditorStore } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { setSizeFor, sizeFor } from "./settings.svelte";
import { dist, drawBrushCircle, hover, isUnknownOp, notReady, redraw, stepSize, trackHover, type Pt, exec, reportError, requirePixels } from "./common";
import { drawStrokeTint, strokeMask } from "./stroke-mask";

let points: Pt[] | null = null;
let working = false;

/** Heal the whole painted footprint in one command. */
async function healStroke(ed: EditorStore, pts: Pt[], radius: number) {
  const s = ed.summary;
  if (!s) return;
  const m = strokeMask(pts, radius * 2, s.width, s.height);
  if (!m) return;
  await exec(ed, { op: "filter.spot-heal", x: m.rect.x, y: m.rect.y, width: m.rect.w, height: m.rect.h, radius }, m.bytes);
}

export const spotHeal: Tool = {
  id: "spot-heal",
  label: "Spot healing brush",
  shortcut: "j",
  cursor: "none",
  down(ed, p) {
    if (working || !requirePixels(ed)) return;
    points = [{ x: p.x, y: p.y }];
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (pressed && points) {
      const last = points[points.length - 1];
      if (dist(last, p) * ed.view.zoom >= 2) points.push({ x: p.x, y: p.y });
    }
  },
  async up(ed) {
    const pts = points;
    if (!pts || working) return;
    const radius = sizeFor("spot-heal") / 2;
    // A click (or a wobble smaller than half the radius) heals one spot.
    const click = pts.every((p) => dist(p, pts[0]) < radius * 0.5);
    working = true;
    try {
      if (click) await exec(ed, { op: "filter.spot-heal", x: pts[0].x, y: pts[0].y, radius });
      else await healStroke(ed, pts, radius);
    } catch (e) {
      if (isUnknownOp(e)) notReady(ed, t("Spot healing"));
      else reportError(ed, e);
    } finally {
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
      setSizeFor("spot-heal", stepSize(sizeFor("spot-heal"), e.code === "BracketRight" ? 1 : -1));
      redraw(ed);
      return true;
    }
    return false;
  },
  overlay(ed, ctx) {
    const size = sizeFor("spot-heal");
    if (points) drawStrokeTint(ed, ctx, points, size, "rgba(0,0,0,0.35)");
    if (hover.inside) drawBrushCircle(ctx, hover.vx, hover.vy, (size / 2) * ed.view.zoom);
  },
};
