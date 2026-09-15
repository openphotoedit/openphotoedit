// Every filter in docs/commands.md, grouped the way Photoshop's Filter menu
// groups them, with the parameters each dialog shows.

import type { Summary } from "../engine/types";
import { t } from "../lib/i18n";

export type ParamDef =
  | { key: string; label: string; type: "number"; min: number; max: number; step?: number; unit?: string; default: number | ((s: Summary) => number) }
  | { key: string; label: string; type: "bool"; default: boolean }
  | { key: string; label: string; type: "choice"; options: { value: string; label: string }[]; default: string };

export interface FilterDef {
  op: string;
  label: string;
  group: string;
  params: ParamDef[];
  /** A short line under the title explaining something non-obvious. */
  note?: string;
}

const px = t("px");

export const FILTER_GROUPS: { id: string; label: string }[] = [
  { id: "blur", label: t("Blur") },
  { id: "blur-gallery", label: t("Blur Gallery") },
  { id: "distort", label: t("Distort") },
  { id: "noise", label: t("Noise") },
  { id: "pixelate", label: t("Pixelate") },
  { id: "render", label: t("Render") },
  { id: "sharpen", label: t("Sharpen") },
  { id: "stylize", label: t("Stylize") },
  { id: "other", label: t("Other") },
];

export const FILTERS: FilterDef[] = [
  // Blur
  { op: "filter.box-blur", label: t("Box Blur"), group: "blur", params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 500, step: 1, unit: px, default: 5 }] },
  { op: "filter.gaussian-blur", label: t("Gaussian Blur"), group: "blur", params: [{ key: "radius", label: t("Radius"), type: "number", min: 0.1, max: 250, step: 0.1, unit: px, default: 4 }] },
  {
    op: "filter.lens-blur",
    label: t("Lens Blur"),
    group: "blur",
    note: t("Blurs evenly, with a round aperture. Depth maps come with Neural Filters."),
    params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 15 }],
  },
  {
    op: "filter.motion-blur",
    label: t("Motion Blur"),
    group: "blur",
    params: [
      { key: "angle", label: t("Angle"), type: "number", min: -180, max: 180, step: 1, unit: "°", default: 0 },
      { key: "distance", label: t("Distance"), type: "number", min: 1, max: 2000, step: 1, unit: px, default: 10 },
    ],
  },
  {
    op: "filter.radial-blur",
    label: t("Radial Blur"),
    group: "blur",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: 1, max: 100, step: 1, default: 10 },
      { key: "mode", label: t("Blur method"), type: "choice", options: [{ value: "spin", label: t("Spin") }, { value: "zoom", label: t("Zoom") }], default: "spin" },
      { key: "cx", label: t("Centre X"), type: "number", min: 0, max: 100000, step: 1, unit: px, default: (s) => Math.round(s.width / 2) },
      { key: "cy", label: t("Centre Y"), type: "number", min: 0, max: 100000, step: 1, unit: px, default: (s) => Math.round(s.height / 2) },
    ],
  },
  {
    op: "filter.surface-blur",
    label: t("Surface Blur"),
    group: "blur",
    params: [
      { key: "radius", label: t("Radius"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 5 },
      { key: "threshold", label: t("Threshold"), type: "number", min: 2, max: 255, step: 1, unit: t("levels"), default: 15 },
    ],
  },
  // Blur Gallery
  {
    op: "filter.tilt-shift",
    label: t("Tilt-Shift"),
    group: "blur-gallery",
    params: [
      { key: "center_y", label: t("Focus line"), type: "number", min: 0, max: 100000, step: 1, unit: px, default: (s) => Math.round(s.height / 2) },
      { key: "band", label: t("Sharp band"), type: "number", min: 0, max: 100000, step: 1, unit: px, default: (s) => Math.round(s.height * 0.15) },
      { key: "feather", label: t("Transition"), type: "number", min: 0, max: 100000, step: 1, unit: px, default: (s) => Math.round(s.height * 0.2) },
      { key: "radius", label: t("Blur"), type: "number", min: 0, max: 500, step: 1, unit: px, default: 15 },
    ],
  },
  // Distort
  { op: "filter.pinch", label: t("Pinch"), group: "distort", params: [{ key: "amount", label: t("Amount"), type: "number", min: -100, max: 100, step: 1, unit: "%", default: 50 }] },
  {
    op: "filter.polar-coordinates",
    label: t("Polar Coordinates"),
    group: "distort",
    params: [
      {
        key: "to_polar",
        label: t("Direction"),
        type: "choice",
        options: [
          { value: "true", label: t("Rectangular to polar") },
          { value: "false", label: t("Polar to rectangular") },
        ],
        default: "true",
      },
    ],
  },
  {
    op: "filter.ripple",
    label: t("Ripple"),
    group: "distort",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: -999, max: 999, step: 1, unit: "%", default: 100 },
      {
        key: "size",
        label: t("Size"),
        type: "choice",
        options: [
          { value: "small", label: t("Small") },
          { value: "medium", label: t("Medium") },
          { value: "large", label: t("Large") },
        ],
        default: "medium",
      },
    ],
  },
  { op: "filter.spherize", label: t("Spherize"), group: "distort", params: [{ key: "amount", label: t("Amount"), type: "number", min: -100, max: 100, step: 1, unit: "%", default: 100 }] },
  { op: "filter.twirl", label: t("Twirl"), group: "distort", params: [{ key: "angle", label: t("Angle"), type: "number", min: -999, max: 999, step: 1, unit: "°", default: 50 }] },
  // Noise
  {
    op: "filter.add-noise",
    label: t("Add Noise"),
    group: "noise",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: 0.1, max: 400, step: 0.1, unit: "%", default: 12.5 },
      { key: "gaussian", label: t("Gaussian distribution"), type: "bool", default: true },
      { key: "monochromatic", label: t("Monochromatic"), type: "bool", default: false },
    ],
  },
  {
    op: "filter.dust-and-scratches",
    label: t("Dust & Scratches"),
    group: "noise",
    params: [
      { key: "radius", label: t("Radius"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 1 },
      { key: "threshold", label: t("Threshold"), type: "number", min: 0, max: 255, step: 1, unit: t("levels"), default: 0 },
    ],
  },
  { op: "filter.median", label: t("Median"), group: "noise", params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 1 }] },
  {
    op: "filter.reduce-noise",
    label: t("Reduce Noise"),
    group: "noise",
    params: [
      { key: "strength", label: t("Strength"), type: "number", min: 0, max: 10, step: 1, default: 6 },
      { key: "preserve_details", label: t("Preserve details"), type: "number", min: 0, max: 100, step: 1, unit: "%", default: 60 },
      { key: "reduce_color_noise", label: t("Reduce colour noise"), type: "number", min: 0, max: 100, step: 1, unit: "%", default: 45 },
    ],
  },
  // Pixelate
  {
    op: "filter.pixelate",
    label: t("Mosaic"),
    group: "pixelate",
    note: t("Mosaic cannot be undone by blurring: the original detail is gone."),
    params: [{ key: "cell", label: t("Cell size"), type: "number", min: 2, max: 200, step: 1, unit: t("square"), default: 8 }],
  },
  // Render
  { op: "filter.clouds", label: t("Clouds"), group: "render", note: t("Uses the foreground and background colours."), params: [{ key: "seed", label: t("Seed"), type: "number", min: 0, max: 99999, step: 1, default: 1 }] },
  {
    op: "filter.vignette",
    label: t("Vignette"),
    group: "render",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: -100, max: 100, step: 1, default: -40 },
      { key: "midpoint", label: t("Midpoint"), type: "number", min: 0, max: 100, step: 1, default: 50 },
      { key: "feather", label: t("Feather"), type: "number", min: 0, max: 100, step: 1, default: 50 },
    ],
  },
  // Sharpen
  {
    op: "filter.smart-sharpen",
    label: t("Smart Sharpen"),
    group: "sharpen",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: 1, max: 500, step: 1, unit: "%", default: 150 },
      { key: "radius", label: t("Radius"), type: "number", min: 0.1, max: 64, step: 0.1, unit: px, default: 1 },
      { key: "reduce_noise", label: t("Reduce noise"), type: "number", min: 0, max: 100, step: 1, unit: "%", default: 10 },
    ],
  },
  {
    op: "filter.unsharp-mask",
    label: t("Unsharp Mask"),
    group: "sharpen",
    params: [
      { key: "amount", label: t("Amount"), type: "number", min: 1, max: 500, step: 1, unit: "%", default: 100 },
      { key: "radius", label: t("Radius"), type: "number", min: 0.1, max: 250, step: 0.1, unit: px, default: 1.5 },
      { key: "threshold", label: t("Threshold"), type: "number", min: 0, max: 255, step: 1, unit: t("levels"), default: 0 },
    ],
  },
  // Stylize
  {
    op: "filter.emboss",
    label: t("Emboss"),
    group: "stylize",
    params: [
      { key: "angle", label: t("Angle"), type: "number", min: -180, max: 180, step: 1, unit: "°", default: 135 },
      { key: "height", label: t("Height"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 3 },
      { key: "amount", label: t("Amount"), type: "number", min: 1, max: 500, step: 1, unit: "%", default: 100 },
    ],
  },
  { op: "filter.find-edges", label: t("Find Edges"), group: "stylize", params: [] },
  { op: "filter.solarize", label: t("Solarize"), group: "stylize", params: [] },
  // Other
  {
    op: "filter.custom",
    label: t("Custom"),
    group: "other",
    note: t("A 3×3 convolution kernel. The default sharpens."),
    params: [
      { key: "k0", label: "−1, −1", type: "number", min: -999, max: 999, default: 0 },
      { key: "k1", label: "0, −1", type: "number", min: -999, max: 999, default: -1 },
      { key: "k2", label: "+1, −1", type: "number", min: -999, max: 999, default: 0 },
      { key: "k3", label: "−1, 0", type: "number", min: -999, max: 999, default: -1 },
      { key: "k4", label: "0, 0", type: "number", min: -999, max: 999, default: 5 },
      { key: "k5", label: "+1, 0", type: "number", min: -999, max: 999, default: -1 },
      { key: "k6", label: "−1, +1", type: "number", min: -999, max: 999, default: 0 },
      { key: "k7", label: "0, +1", type: "number", min: -999, max: 999, default: -1 },
      { key: "k8", label: "+1, +1", type: "number", min: -999, max: 999, default: 0 },
      { key: "scale", label: t("Scale"), type: "number", min: 1, max: 9999, default: 1 },
      { key: "offset", label: t("Offset"), type: "number", min: -9999, max: 9999, default: 0 },
    ],
  },
  { op: "filter.high-pass", label: t("High Pass"), group: "other", params: [{ key: "radius", label: t("Radius"), type: "number", min: 0.1, max: 1000, step: 0.1, unit: px, default: 10 }] },
  { op: "filter.maximum", label: t("Maximum"), group: "other", params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 500, step: 1, unit: px, default: 1 }] },
  { op: "filter.minimum", label: t("Minimum"), group: "other", params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 500, step: 1, unit: px, default: 1 }] },
  {
    op: "filter.offset",
    label: t("Offset"),
    group: "other",
    params: [
      { key: "dx", label: t("Horizontal"), type: "number", min: -30000, max: 30000, step: 1, unit: px, default: (s) => Math.round(s.width / 2) },
      { key: "dy", label: t("Vertical"), type: "number", min: -30000, max: 30000, step: 1, unit: px, default: (s) => Math.round(s.height / 2) },
      { key: "wrap", label: t("Wrap around"), type: "bool", default: true },
    ],
  },
  {
    op: "filter.frequency-separation",
    label: t("Frequency Separation"),
    group: "other",
    note: t("Adds a low-frequency layer and a high-frequency layer above the active layer."),
    params: [{ key: "radius", label: t("Radius"), type: "number", min: 1, max: 100, step: 1, unit: px, default: 8 }],
  },
];

export function defaultParams(def: FilterDef, s: Summary): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const p of def.params) out[p.key] = typeof p.default === "function" ? p.default(s) : p.default;
  return out;
}

/** Dialog values → the command the engine takes. */
export function buildCommand(def: FilterDef, values: Record<string, unknown>, extra: Record<string, unknown> = {}): Record<string, unknown> {
  const cmd: Record<string, unknown> = { op: def.op, ...extra };
  if (def.op === "filter.custom") {
    cmd.kernel = Array.from({ length: 9 }, (_, i) => Number(values[`k${i}`] ?? 0));
    cmd.scale = values.scale;
    cmd.offset = values.offset;
    return cmd;
  }
  for (const p of def.params) {
    const v = values[p.key];
    if (def.op === "filter.polar-coordinates" && p.key === "to_polar") cmd[p.key] = v === true || v === "true";
    else cmd[p.key] = v;
  }
  return cmd;
}

export function filterByOp(op: string) {
  return FILTERS.find((f) => f.op === op) ?? null;
}
