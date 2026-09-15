// The options component for a tool id, or null when it has none. Every
// component takes `{ tool: string }` and binds to `toolSettings`, so it can
// sit in Pro's options bar or Lite's bottom sheet unchanged.

import type { Component } from "svelte";
import BrushOptions from "./BrushOptions.svelte";
import CropOptions from "./CropOptions.svelte";
import EyedropperOptions from "./EyedropperOptions.svelte";
import FillOptions from "./FillOptions.svelte";
import MoveOptions from "./MoveOptions.svelte";
import PerspectiveCropOptions from "./PerspectiveCropOptions.svelte";
import RetouchOptions from "./RetouchOptions.svelte";
import SelectionOptions from "./SelectionOptions.svelte";
import ShapeOptions from "./ShapeOptions.svelte";
import TextOptions from "./TextOptions.svelte";
import TransformOptions from "./TransformOptions.svelte";

export type ToolOptionsComponent = Component<{ tool: string }>;

const BY_TOOL: Record<string, Component<{ tool: string }> | Component<Record<string, never>>> = {
  move: MoveOptions,
  "marquee-rect": SelectionOptions,
  "marquee-ellipse": SelectionOptions,
  lasso: SelectionOptions,
  "polygon-lasso": SelectionOptions,
  "object-select": SelectionOptions,
  "quick-select": SelectionOptions,
  "magic-wand": SelectionOptions,
  crop: CropOptions,
  "perspective-crop": PerspectiveCropOptions,
  eyedropper: EyedropperOptions,
  brush: BrushOptions,
  pencil: BrushOptions,
  eraser: BrushOptions,
  clone: BrushOptions,
  heal: BrushOptions,
  dodge: BrushOptions,
  burn: BrushOptions,
  sponge: BrushOptions,
  "blur-brush": BrushOptions,
  "sharpen-brush": BrushOptions,
  smudge: BrushOptions,
  "spot-heal": RetouchOptions,
  remove: RetouchOptions,
  patch: RetouchOptions,
  "red-eye": RetouchOptions,
  gradient: FillOptions,
  bucket: FillOptions,
  "shape-rect": ShapeOptions,
  "shape-ellipse": ShapeOptions,
  "shape-line": ShapeOptions,
  arrow: ShapeOptions,
  pen: ShapeOptions,
  highlighter: ShapeOptions,
  redact: ShapeOptions,
  pixelate: ShapeOptions,
  text: TextOptions,
  transform: TransformOptions,
};

/** Mount with `<Opts tool={id} />`. */
export function optionsFor(toolId: string): ToolOptionsComponent | null {
  return (BY_TOOL[toolId] as ToolOptionsComponent | undefined) ?? null;
}
