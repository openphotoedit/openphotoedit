// The toolbar's view of the tools workstream. `tools/groups.ts`
// (TOOL_GROUPS, shortcutMap) and `tools/options/index.ts` (optionsFor) are
// loaded when they exist; until then the Pro shell uses Photoshop's own
// grouping below. Glob imports resolve to nothing for a missing file, so the
// build never breaks while those files are being written.

import type { Component } from "svelte";
import Move from "@lucide/svelte/icons/move";
import SquareDashed from "@lucide/svelte/icons/square-dashed";
import CircleDashed from "@lucide/svelte/icons/circle-dashed";
import Lasso from "@lucide/svelte/icons/lasso";
import LassoSelect from "@lucide/svelte/icons/lasso-select";
import SquareDashedMousePointer from "@lucide/svelte/icons/square-dashed-mouse-pointer";
import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
import Wand from "@lucide/svelte/icons/wand";
import Crop from "@lucide/svelte/icons/crop";
import Frame from "@lucide/svelte/icons/frame";
import Pipette from "@lucide/svelte/icons/pipette";
import Bandage from "@lucide/svelte/icons/bandage";
import Sparkles from "@lucide/svelte/icons/sparkles";
import SquareStack from "@lucide/svelte/icons/square-stack";
import Eye from "@lucide/svelte/icons/eye";
import Brush from "@lucide/svelte/icons/brush";
import Pencil from "@lucide/svelte/icons/pencil";
import Stamp from "@lucide/svelte/icons/stamp";
import Eraser from "@lucide/svelte/icons/eraser";
import Blend from "@lucide/svelte/icons/blend";
import PaintBucket from "@lucide/svelte/icons/paint-bucket";
import Droplet from "@lucide/svelte/icons/droplet";
import Triangle from "@lucide/svelte/icons/triangle";
import Pointer from "@lucide/svelte/icons/pointer";
import SunMedium from "@lucide/svelte/icons/sun-medium";
import Flame from "@lucide/svelte/icons/flame";
import SprayCan from "@lucide/svelte/icons/spray-can";
import ArrowUpRight from "@lucide/svelte/icons/arrow-up-right";
import PenLine from "@lucide/svelte/icons/pen-line";
import Highlighter from "@lucide/svelte/icons/highlighter";
import RectangleHorizontal from "@lucide/svelte/icons/rectangle-horizontal";
import Grid3x3 from "@lucide/svelte/icons/grid-3x3";
import Type from "@lucide/svelte/icons/type";
import Square from "@lucide/svelte/icons/square";
import Circle from "@lucide/svelte/icons/circle";
import Slash from "@lucide/svelte/icons/slash";
import Hand from "@lucide/svelte/icons/hand";
import ZoomIn from "@lucide/svelte/icons/zoom-in";
import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import { TOOLS } from "../tools/registry";

type Icon = Component<{ size?: number | string; strokeWidth?: number | string }>;

export interface ToolMeta {
  id: string;
  label: string;
  icon: Icon;
  shortcut?: string;
  hint: string;
}

export interface ToolGroup {
  id: string;
  tools: string[];
}

const META: Record<string, { label: string; icon: Icon; shortcut?: string; hint: string }> = {
  move: { label: t("Move tool"), icon: Move, shortcut: "V", hint: t("Drag to move the layer. Cmd-click selects the layer under the pointer.") },
  "marquee-rect": { label: t("Rectangular marquee tool"), icon: SquareDashed, shortcut: "M", hint: t("Drag to select. Shift adds, Alt subtracts.") },
  "marquee-ellipse": { label: t("Elliptical marquee tool"), icon: CircleDashed, shortcut: "M", hint: t("Drag to select an ellipse. Shift adds, Alt subtracts.") },
  lasso: { label: t("Lasso tool"), icon: Lasso, shortcut: "L", hint: t("Drag a freehand selection.") },
  "polygon-lasso": { label: t("Polygonal lasso tool"), icon: LassoSelect, shortcut: "L", hint: t("Click corners; double-click to close.") },
  "object-select": { label: t("Object selection tool"), icon: SquareDashedMousePointer, shortcut: "W", hint: t("Drag a box around an object, or click it.") },
  "quick-select": { label: t("Quick selection tool"), icon: WandSparkles, shortcut: "W", hint: t("Paint over an area to select it by its edges.") },
  "magic-wand": { label: t("Magic wand tool"), icon: Wand, shortcut: "W", hint: t("Click to select similar colours.") },
  crop: { label: t("Crop tool"), icon: Crop, shortcut: "C", hint: t("Drag the handles, then press Enter to crop.") },
  "perspective-crop": { label: t("Perspective crop tool"), icon: Frame, shortcut: "C", hint: t("Place four corners, then press Enter.") },
  eyedropper: { label: t("Eyedropper tool"), icon: Pipette, shortcut: "I", hint: t("Click to sample the foreground colour. Alt samples the background colour.") },
  "spot-heal": { label: t("Spot healing brush tool"), icon: Bandage, shortcut: "J", hint: t("Paint over a blemish to heal it.") },
  heal: { label: t("Healing brush tool"), icon: Bandage, shortcut: "J", hint: t("Alt-click a source, then paint.") },
  remove: { label: t("Remove tool"), icon: Sparkles, shortcut: "J", hint: t("Paint over something to remove it.") },
  patch: { label: t("Patch tool"), icon: SquareStack, shortcut: "J", hint: t("Draw around an area, then drag it onto clean texture.") },
  "red-eye": { label: t("Red eye tool"), icon: Eye, shortcut: "J", hint: t("Click a red pupil.") },
  brush: { label: t("Brush tool"), icon: Brush, shortcut: "B", hint: t("Paint with the foreground colour. [ and ] change the size.") },
  pencil: { label: t("Pencil tool"), icon: Pencil, shortcut: "B", hint: t("Paint hard-edged lines.") },
  clone: { label: t("Clone stamp tool"), icon: Stamp, shortcut: "S", hint: t("Alt-click a source, then paint.") },
  eraser: { label: t("Eraser tool"), icon: Eraser, shortcut: "E", hint: t("Erase to transparency.") },
  gradient: { label: t("Gradient tool"), icon: Blend, shortcut: "G", hint: t("Drag to draw a gradient.") },
  bucket: { label: t("Paint bucket tool"), icon: PaintBucket, shortcut: "G", hint: t("Click to fill similar colours.") },
  "blur-brush": { label: t("Blur tool"), icon: Droplet, hint: t("Paint to soften detail.") },
  "sharpen-brush": { label: t("Sharpen tool"), icon: Triangle, hint: t("Paint to sharpen detail.") },
  smudge: { label: t("Smudge tool"), icon: Pointer, hint: t("Drag to smear pixels.") },
  dodge: { label: t("Dodge tool"), icon: SunMedium, shortcut: "O", hint: t("Paint to lighten.") },
  burn: { label: t("Burn tool"), icon: Flame, shortcut: "O", hint: t("Paint to darken.") },
  sponge: { label: t("Sponge tool"), icon: SprayCan, shortcut: "O", hint: t("Paint to change saturation.") },
  arrow: { label: t("Arrow tool"), icon: ArrowUpRight, hint: t("Drag to draw an arrow.") },
  pen: { label: t("Pen tool"), icon: PenLine, hint: t("Draw a freehand line.") },
  highlighter: { label: t("Highlighter tool"), icon: Highlighter, hint: t("Drag to highlight.") },
  redact: { label: t("Redact tool"), icon: RectangleHorizontal, hint: t("Drag to cover an area with a solid box.") },
  pixelate: { label: t("Pixelate tool"), icon: Grid3x3, hint: t("Drag to pixelate an area.") },
  text: { label: t("Type tool"), icon: Type, shortcut: "T", hint: t("Click to type; drag to make a text box.") },
  "shape-rect": { label: t("Rectangle tool"), icon: Square, shortcut: "U", hint: t("Drag to draw a rectangle. Shift makes a square.") },
  "shape-ellipse": { label: t("Ellipse tool"), icon: Circle, shortcut: "U", hint: t("Drag to draw an ellipse. Shift makes a circle.") },
  "shape-line": { label: t("Line tool"), icon: Slash, shortcut: "U", hint: t("Drag to draw a line.") },
  hand: { label: t("Hand tool"), icon: Hand, shortcut: "H", hint: t("Drag to pan. Hold Space with any tool.") },
  zoom: { label: t("Zoom tool"), icon: ZoomIn, shortcut: "Z", hint: t("Click to zoom in. Alt-click zooms out.") },
};

const FALLBACK_GROUPS: ToolGroup[] = [
  { id: "move", tools: ["move"] },
  { id: "marquee", tools: ["marquee-rect", "marquee-ellipse"] },
  { id: "lasso", tools: ["lasso", "polygon-lasso"] },
  { id: "select", tools: ["object-select", "quick-select", "magic-wand"] },
  { id: "crop", tools: ["crop", "perspective-crop"] },
  { id: "eyedropper", tools: ["eyedropper"] },
  { id: "heal", tools: ["spot-heal", "heal", "remove", "patch", "red-eye"] },
  { id: "brush", tools: ["brush", "pencil"] },
  { id: "clone", tools: ["clone"] },
  { id: "eraser", tools: ["eraser"] },
  { id: "gradient", tools: ["gradient", "bucket"] },
  { id: "blur", tools: ["blur-brush", "sharpen-brush", "smudge"] },
  { id: "dodge", tools: ["dodge", "burn", "sponge"] },
  { id: "annotate", tools: ["arrow", "pen", "highlighter", "redact", "pixelate"] },
  { id: "text", tools: ["text"] },
  { id: "shape", tools: ["shape-rect", "shape-ellipse", "shape-line"] },
  { id: "hand", tools: ["hand"] },
  { id: "zoom", tools: ["zoom"] },
];

const groupModules = import.meta.glob("../tools/groups.ts");
const optionModules = import.meta.glob("../tools/options/index.ts");

export const toolBridge = $state({
  groups: FALLBACK_GROUPS as ToolGroup[],
  /** letter (lower case) → tool ids in cycling order */
  shortcuts: {} as Record<string, string[]>,
  optionsFor: null as null | ((id: string) => unknown),
  /** Last-used tool per group, so a group button shows what you picked. */
  current: {} as Record<string, string>,
});

function normalizeGroups(raw: unknown): ToolGroup[] | null {
  if (!Array.isArray(raw)) return null;
  const out: ToolGroup[] = [];
  raw.forEach((g, i) => {
    let tools: unknown[] | null = null;
    let id = `g${i}`;
    if (Array.isArray(g)) tools = g;
    else if (g && typeof g === "object") {
      const o = g as Record<string, unknown>;
      tools = (o.tools ?? o.items ?? o.ids) as unknown[] | null;
      if (typeof o.id === "string") id = o.id;
    }
    if (!tools) return;
    const ids = tools.map((x) => (typeof x === "string" ? x : x && typeof x === "object" ? String((x as { id?: string }).id ?? "") : "")).filter(Boolean);
    if (ids.length) out.push({ id, tools: ids });
  });
  return out.length ? out : null;
}

function normalizeShortcuts(raw: unknown): Record<string, string[]> | null {
  let m: unknown = raw;
  if (m instanceof Map) m = Object.fromEntries(m);
  if (!m || typeof m !== "object") return null;
  const out: Record<string, string[]> = {};
  for (const [k, v] of Object.entries(m as Record<string, unknown>)) {
    if (k.length === 1) {
      out[k.toLowerCase()] = Array.isArray(v) ? v.map(String) : [String(v)];
    } else if (typeof v === "string" && v.length === 1) {
      (out[v.toLowerCase()] ??= []).push(k);
    }
  }
  return out;
}

function fallbackShortcuts(groups: ToolGroup[]): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const g of groups) {
    for (const id of g.tools) {
      const letter = (TOOLS[id]?.shortcut ?? META[id]?.shortcut)?.toLowerCase();
      if (letter && letter.length === 1) (out[letter] ??= []).push(id);
    }
  }
  return out;
}

export async function loadToolModules() {
  toolBridge.shortcuts = fallbackShortcuts(toolBridge.groups);
  const g = Object.values(groupModules)[0];
  if (g) {
    try {
      const mod = (await g()) as Record<string, unknown>;
      const groups = normalizeGroups(mod.TOOL_GROUPS ?? mod.PRO_TOOL_GROUPS);
      if (groups) toolBridge.groups = groups;
      const sm = typeof mod.shortcutMap === "function" ? normalizeShortcuts((mod.shortcutMap as () => unknown)()) : null;
      toolBridge.shortcuts = sm && Object.keys(sm).length ? sm : fallbackShortcuts(toolBridge.groups);
    } catch (e) {
      console.warn("tools/groups.ts failed to load; using the built-in grouping", e);
    }
  }
  const o = Object.values(optionModules)[0];
  if (o) {
    try {
      const mod = (await o()) as Record<string, unknown>;
      if (typeof mod.optionsFor === "function") toolBridge.optionsFor = mod.optionsFor as (id: string) => unknown;
    } catch (e) {
      console.warn("tools/options failed to load", e);
    }
  }
}

export function toolMeta(id: string): ToolMeta {
  const m = META[id];
  const reg = TOOLS[id];
  return {
    id,
    label: m?.label ?? reg?.label ?? id,
    icon: m?.icon ?? Square,
    shortcut: (reg?.shortcut ?? m?.shortcut)?.toUpperCase(),
    hint: m?.hint ?? "",
  };
}

export function isToolAvailable(id: string) {
  return !!TOOLS[id];
}

export function groupOf(toolId: string): ToolGroup | null {
  return toolBridge.groups.find((g) => g.tools.includes(toolId)) ?? null;
}

export function selectTool(id: string) {
  editor.tool = id;
  const g = groupOf(id);
  if (g) toolBridge.current[g.id] = id;
}

/** A shortcut letter: selects the group's current tool, or cycles with Shift. */
export function toolForLetter(letter: string, cycle: boolean): string | null {
  const ids = toolBridge.shortcuts[letter.toLowerCase()];
  if (!ids?.length) return null;
  if (cycle) {
    const i = ids.indexOf(editor.tool);
    return ids[(i + 1) % ids.length];
  }
  if (ids.includes(editor.tool)) return editor.tool;
  const g = groupOf(ids[0]);
  const remembered = g ? toolBridge.current[g.id] : undefined;
  return remembered && ids.includes(remembered) ? remembered : ids[0];
}
