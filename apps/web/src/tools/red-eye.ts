// Red eye: click a pupil (a box around the click is searched) or drag a
// box around an eye.

import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { antsStroke, docRectPath, isUnknownOp, notReady, rectFrom, redraw, trackHover, type Pt, exec, reportError } from "./common";

let drag: { start: Pt; end: Pt; startV: Pt; moved: boolean } | null = null;

export const redEye: Tool = {
  id: "red-eye",
  label: "Red eye",
  cursor: "crosshair",
  down(_ed, p) {
    drag = { start: { x: p.x, y: p.y }, end: { x: p.x, y: p.y }, startV: { x: p.vx, y: p.vy }, moved: false };
  },
  move(ed, p, pressed) {
    trackHover(ed, p);
    if (!pressed || !drag) return;
    drag.end = { x: p.x, y: p.y };
    if (Math.hypot(p.vx - drag.startV.x, p.vy - drag.startV.y) > 4) drag.moved = true;
  },
  async up(ed) {
    const d = drag;
    drag = null;
    redraw(ed);
    if (!d) return;
    let r = rectFrom(d.start, d.end);
    if (!d.moved) {
      // A click searches a box about 60 screen pixels across.
      const half = Math.max(12, 30 / ed.view.zoom);
      r = { x: d.start.x - half, y: d.start.y - half, w: half * 2, h: half * 2 };
    }
    try {
      await exec(ed, {
        op: "filter.red-eye",
        x: Math.round(r.x),
        y: Math.round(r.y),
        width: Math.max(1, Math.round(r.w)),
        height: Math.max(1, Math.round(r.h)),
        pupil_size: toolSettings.redEyePupil,
        darken: toolSettings.redEyeDarken,
      });
    } catch (e) {
      if (isUnknownOp(e)) notReady(ed, t("Red eye"));
      else reportError(ed, e);
    }
  },
  cancel(ed) {
    drag = null;
    redraw(ed);
  },
  overlay(ed, ctx) {
    if (!drag?.moved) return;
    const r = rectFrom(drag.start, drag.end);
    antsStroke(ctx, () => docRectPath(ed, ctx, r));
  },
};
