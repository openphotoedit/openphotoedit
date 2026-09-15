// Spot healing: click a blemish, or paint over a scratch. Each dab is a
// `filter.spot-heal` at the brush radius.

import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { setSizeFor, sizeFor } from "./settings.svelte";
import { dist, drawBrushCircle, hover, isUnknownOp, notReady, redraw, stepSize, trackHover, type Pt, exec, reportError, requirePixels } from "./common";
import { drawStrokeTint } from "./stroke-mask";

let points: Pt[] | null = null;
let working = false;

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
    // Dabs spaced at 3/4 of the radius cover the painted path.
    const dabs: Pt[] = [pts[0]];
    for (const p of pts.slice(1)) if (dist(dabs[dabs.length - 1], p) >= radius * 0.75) dabs.push(p);
    const last = pts[pts.length - 1];
    if (dabs[dabs.length - 1] !== last && dist(dabs[dabs.length - 1], last) > radius * 0.25) dabs.push(last);
    working = true;
    try {
      for (const d of dabs) {
        try {
          await exec(ed, { op: "filter.spot-heal", x: d.x, y: d.y, radius });
        } catch (e) {
          if (isUnknownOp(e)) notReady(ed, t("Spot healing"));
          else reportError(ed, e);
          break;
        }
      }
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
