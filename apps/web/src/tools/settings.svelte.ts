// Tool options, shared by every tool and both shells, remembered across
// sessions. Colours left `null` follow the editor's foreground/background
// colours, which is what Photoshop's tools do until you pick one.

import { editor } from "../lib/editor.svelte";
import type { BlendMode, GradientStop, Rgba8 } from "../engine/types";

export type SelectMode = "replace" | "add" | "subtract" | "intersect";
export type CropRatio = "free" | "original" | "1:1" | "4:5" | "3:2" | "16:9" | "9:16" | "custom";
export type CropOverlay = "thirds" | "grid" | "none";
export type GradientType = "linear" | "radial" | "angle" | "reflected" | "diamond";
export type ToneRange = "shadows" | "midtones" | "highlights";
export type Quality = "fast" | "best";

export interface ToolSettingsData {
  // Brushes
  brushSize: number;
  brushHardness: number;
  brushOpacity: number;
  brushFlow: number;
  brushSpacing: number;
  brushBlend: BlendMode;
  pressureSize: boolean;
  pressureOpacity: boolean;
  pencilSize: number;
  // Eraser
  eraserSize: number;
  eraserHardness: number;
  eraserOpacity: number;
  eraserFlow: number;
  eraserMode: "brush" | "pencil";
  // Selection
  selectMode: SelectMode;
  feather: number;
  antiAlias: boolean;
  wandTolerance: number;
  wandContiguous: boolean;
  wandSampleAll: boolean;
  quickSelectSize: number;
  // Move
  autoSelect: boolean;
  showTransformControls: boolean;
  // Crop
  cropRatio: CropRatio;
  cropCustomW: number;
  cropCustomH: number;
  /** Output size in pixels after the crop; 0 keeps the cropped size. */
  cropOutW: number;
  cropOutH: number;
  cropDeleteCropped: boolean;
  cropOverlay: CropOverlay;
  // Shapes and annotations
  shapeStroke: Rgba8 | null;
  shapeFill: Rgba8 | null;
  shapeFillOn: boolean;
  shapeStrokeOn: boolean;
  shapeWidth: number;
  shapeRadius: number;
  shapeDashed: boolean;
  arrowStart: boolean;
  arrowEnd: boolean;
  penWidth: number;
  penSmoothing: number;
  highlighterWidth: number;
  highlighterColor: Rgba8;
  redactColor: Rgba8;
  pixelateCell: number;
  // Text
  fontFamily: string;
  fontSize: number;
  fontWeight: number;
  italic: boolean;
  textColor: Rgba8 | null;
  textAlign: "left" | "center" | "right";
  lineHeight: number;
  letterSpacing: number;
  textBackgroundOn: boolean;
  textBackground: Rgba8;
  textPadding: number;
  textStrokeOn: boolean;
  textStroke: Rgba8;
  textStrokeWidth: number;
  // Fills
  gradientType: GradientType;
  /** Empty = foreground to background. */
  gradientStops: GradientStop[];
  gradientPreset: "fg-bg" | "fg-transparent" | "black-white" | "custom";
  gradientReverse: boolean;
  gradientOpacity: number;
  bucketTolerance: number;
  bucketContiguous: boolean;
  bucketSampleAll: boolean;
  bucketAntiAlias: boolean;
  bucketOpacity: number;
  // Retouching
  cloneSampleAll: boolean;
  cloneAligned: boolean;
  healSize: number;
  spotHealSize: number;
  removeSize: number;
  removeQuality: Quality;
  redEyePupil: number;
  redEyeDarken: number;
  patchBlend: number;
  // Toning
  toneRange: ToneRange;
  toneExposure: number;
  spongeMode: "saturate" | "desaturate";
  spongeFlow: number;
  strength: number;
  // Eyedropper
  sampleSize: 1 | 3 | 5;
  sampleMerged: boolean;
  // Transform
  transformKeepRatio: boolean;
}

const DEFAULTS: ToolSettingsData = {
  brushSize: 30,
  brushHardness: 0.8,
  brushOpacity: 1,
  brushFlow: 1,
  brushSpacing: 0.1,
  brushBlend: "normal",
  pressureSize: true,
  pressureOpacity: false,
  pencilSize: 1,
  eraserSize: 40,
  eraserHardness: 0.8,
  eraserOpacity: 1,
  eraserFlow: 1,
  eraserMode: "brush",
  selectMode: "replace",
  feather: 0,
  antiAlias: true,
  wandTolerance: 32,
  wandContiguous: true,
  wandSampleAll: false,
  quickSelectSize: 30,
  autoSelect: false,
  showTransformControls: false,
  cropRatio: "free",
  cropCustomW: 4,
  cropCustomH: 3,
  cropOutW: 0,
  cropOutH: 0,
  cropDeleteCropped: true,
  cropOverlay: "thirds",
  shapeStroke: null,
  shapeFill: null,
  shapeFillOn: false,
  shapeStrokeOn: true,
  shapeWidth: 6,
  shapeRadius: 0,
  shapeDashed: false,
  arrowStart: false,
  arrowEnd: true,
  penWidth: 6,
  penSmoothing: 0.5,
  highlighterWidth: 28,
  highlighterColor: { r: 255, g: 214, b: 10, a: 102 },
  redactColor: { r: 0, g: 0, b: 0, a: 255 },
  pixelateCell: 16,
  fontFamily: "Inter, system-ui, sans-serif",
  fontSize: 48,
  fontWeight: 600,
  italic: false,
  textColor: null,
  textAlign: "left",
  lineHeight: 1.2,
  letterSpacing: 0,
  textBackgroundOn: false,
  textBackground: { r: 255, g: 255, b: 255, a: 255 },
  textPadding: 12,
  textStrokeOn: false,
  textStroke: { r: 255, g: 255, b: 255, a: 255 },
  textStrokeWidth: 2,
  gradientType: "linear",
  gradientStops: [],
  gradientPreset: "fg-bg",
  gradientReverse: false,
  gradientOpacity: 1,
  bucketTolerance: 32,
  bucketContiguous: true,
  bucketSampleAll: false,
  bucketAntiAlias: true,
  bucketOpacity: 1,
  cloneSampleAll: false,
  cloneAligned: true,
  healSize: 30,
  spotHealSize: 30,
  removeSize: 40,
  removeQuality: "fast",
  redEyePupil: 0.5,
  redEyeDarken: 0.5,
  patchBlend: 0.5,
  toneRange: "midtones",
  toneExposure: 0.5,
  spongeMode: "desaturate",
  spongeFlow: 0.5,
  strength: 0.5,
  sampleSize: 1,
  sampleMerged: true,
  transformKeepRatio: true,
};

const KEY = "ops.toolSettings";
const NULLABLE = new Set<keyof ToolSettingsData>(["shapeStroke", "shapeFill", "textColor"]);

function load(): Partial<ToolSettingsData> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const v = JSON.parse(raw);
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

export const TOOL_SETTING_DEFAULTS: Readonly<ToolSettingsData> = DEFAULTS;

function initial(): ToolSettingsData {
  const out: ToolSettingsData = structuredClone(DEFAULTS);
  const saved = load();
  for (const k of Object.keys(DEFAULTS) as (keyof ToolSettingsData)[]) {
    if (!(k in saved)) continue;
    const v = saved[k];
    const nullable = NULLABLE.has(k);
    const ok = v === null ? nullable : nullable ? typeof v === "object" : typeof v === typeof DEFAULTS[k] && Array.isArray(v) === Array.isArray(DEFAULTS[k]);
    if (ok) (out as unknown as Record<string, unknown>)[k] = v;
  }
  return out;
}

/** Every tool option. Read and bind its fields directly. */
export const toolSettings: ToolSettingsData = $state(initial());

$effect.root(() => {
  $effect(() => {
    const json = JSON.stringify(toolSettings);
    try {
      localStorage.setItem(KEY, json);
    } catch {
      /* private mode or full storage: settings last for this session */
    }
  });
});

export function resetToolSettings() {
  Object.assign(toolSettings, structuredClone(DEFAULTS));
}

const SIZE_KEY: Record<string, keyof ToolSettingsData> = {
  pencil: "pencilSize",
  eraser: "eraserSize",
  heal: "healSize",
  "spot-heal": "spotHealSize",
  remove: "removeSize",
  "quick-select": "quickSelectSize",
};

/** Brush diameter in document pixels for a painting tool id. */
export function sizeFor(tool: string): number {
  return toolSettings[SIZE_KEY[tool] ?? "brushSize"] as number;
}

export function setSizeFor(tool: string, v: number) {
  const size = Math.max(1, Math.min(5000, Math.round(v)));
  (toolSettings as unknown as Record<string, number>)[SIZE_KEY[tool] ?? "brushSize"] = size;
}

export function strokeColor(): Rgba8 {
  return toolSettings.shapeStroke ?? editor.primary;
}
export function fillColor(): Rgba8 {
  return toolSettings.shapeFill ?? editor.secondary;
}
export function textColor(): Rgba8 {
  return toolSettings.textColor ?? editor.primary;
}

export function gradientStops(): GradientStop[] {
  const fg = { ...editor.primary, a: editor.primary.a ?? 255 };
  const bg = { ...editor.secondary, a: editor.secondary.a ?? 255 };
  switch (toolSettings.gradientPreset) {
    case "fg-transparent":
      return [{ pos: 0, color: fg }, { pos: 1, color: { ...fg, a: 0 } }];
    case "black-white":
      return [{ pos: 0, color: { r: 0, g: 0, b: 0, a: 255 } }, { pos: 1, color: { r: 255, g: 255, b: 255, a: 255 } }];
    case "custom":
      if (toolSettings.gradientStops.length >= 2) return toolSettings.gradientStops;
      return [{ pos: 0, color: fg }, { pos: 1, color: bg }];
    default:
      return [{ pos: 0, color: fg }, { pos: 1, color: bg }];
  }
}
