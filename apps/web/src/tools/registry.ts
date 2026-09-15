// Every tool, by id. Shells choose which to show; the canvas dispatches to
// whichever is active. Ids are the contract in docs/tools.md.
import type { Tool } from "./types";
import { hand, zoom } from "./navigate";
import { move } from "./move";
import { marqueeEllipse, marqueeRect } from "./marquee";
import { lasso, polygonLasso } from "./lasso";
import { magicWand, quickSelect } from "./wand";
import { objectSelect } from "./object-select";
import { crop } from "./crop";
import { perspectiveCrop } from "./perspective-crop";
import { eyedropper } from "./eyedropper";
import { blurBrush, brush, burn, clone, dodge, eraser, heal, pencil, sharpenBrush, smudge, sponge } from "./paint";
import { spotHeal } from "./spot-heal";
import { remove } from "./remove";
import { patch } from "./patch";
import { redEye } from "./red-eye";
import { gradient } from "./gradient";
import { bucket } from "./bucket";
import { arrow, highlighter, pen, redact, shapeEllipse, shapeLine, shapeRect } from "./annotate.svelte";
import { pixelate } from "./pixelate";
import { text } from "./text.svelte";
import { transform } from "./transform";

export const TOOLS: Record<string, Tool> = {};

export function registerTools(...tools: Tool[]) {
  for (const t of tools) TOOLS[t.id] = t;
}

registerTools(
  move,
  marqueeRect,
  marqueeEllipse,
  lasso,
  polygonLasso,
  magicWand,
  quickSelect,
  objectSelect,
  crop,
  perspectiveCrop,
  eyedropper,
  spotHeal,
  heal,
  remove,
  patch,
  redEye,
  brush,
  pencil,
  eraser,
  clone,
  gradient,
  bucket,
  dodge,
  burn,
  sponge,
  blurBrush,
  sharpenBrush,
  smudge,
  text,
  shapeRect,
  shapeEllipse,
  shapeLine,
  arrow,
  pen,
  highlighter,
  redact,
  pixelate,
  transform,
  hand,
  zoom,
);
