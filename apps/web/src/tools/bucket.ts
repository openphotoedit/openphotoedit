// Paint bucket: fill similar colours around the click with the foreground.

import { t } from "../lib/i18n";
import type { Tool } from "./types";
import { toolSettings } from "./settings.svelte";
import { redraw, requirePixels, rgba, run } from "./common";

export const bucket: Tool = {
  id: "bucket",
  label: "Paint bucket",
  cursor: "crosshair",
  async down(ed, p) {
    const s = ed.summary;
    if (!s || p.x < 0 || p.y < 0 || p.x >= s.width || p.y >= s.height || !requirePixels(ed)) return;
    await run(
      ed,
      {
        op: "paint.fill",
        x: Math.floor(p.x),
        y: Math.floor(p.y),
        tolerance: toolSettings.bucketTolerance,
        contiguous: toolSettings.bucketContiguous,
        sample_all: toolSettings.bucketSampleAll,
        anti_alias: toolSettings.bucketAntiAlias,
        color: rgba(p.alt ? ed.secondary : ed.primary),
        opacity: toolSettings.bucketOpacity,
      },
      { feature: t("Paint bucket") },
    );
    redraw(ed);
  },
};
