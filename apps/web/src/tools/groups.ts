// Toolbar structure for the shells: Photoshop's toolbar order with nested
// groups, the Lite markup set, and the shortcut letters. Icons are Lucide
// names (`@lucide/svelte/icons/<icon>`); labels go through t() when shown.

export interface ToolEntry {
  id: string;
  label: string;
  /** Lucide icon name. */
  icon: string;
  /** Photoshop's shortcut letter, lower case. */
  shortcut?: string;
}

export interface ToolGroupDef {
  id: string;
  label: string;
  tools: ToolEntry[];
}

const e = (id: string, label: string, icon: string, shortcut?: string): ToolEntry => ({ id, label, icon, shortcut });

export const TOOL_GROUPS: ToolGroupDef[] = [
  { id: "move", label: "Move", tools: [e("move", "Move tool", "move", "v")] },
  { id: "marquee", label: "Marquee", tools: [e("marquee-rect", "Rectangular marquee tool", "square-dashed", "m"), e("marquee-ellipse", "Elliptical marquee tool", "circle-dashed", "m")] },
  { id: "lasso", label: "Lasso", tools: [e("lasso", "Lasso tool", "lasso", "l"), e("polygon-lasso", "Polygonal lasso tool", "lasso-select", "l")] },
  {
    id: "select",
    label: "Selection",
    tools: [e("object-select", "Object selection tool", "square-dashed-mouse-pointer", "w"), e("quick-select", "Quick selection tool", "wand-sparkles", "w"), e("magic-wand", "Magic wand tool", "wand", "w")],
  },
  { id: "crop", label: "Crop", tools: [e("crop", "Crop tool", "crop", "c"), e("perspective-crop", "Perspective crop tool", "frame", "c")] },
  { id: "eyedropper", label: "Eyedropper", tools: [e("eyedropper", "Eyedropper tool", "pipette", "i")] },
  {
    id: "heal",
    label: "Healing",
    tools: [
      e("spot-heal", "Spot healing brush tool", "bandage", "j"),
      e("heal", "Healing brush tool", "bandage", "j"),
      e("remove", "Remove tool", "sparkles", "j"),
      e("patch", "Patch tool", "square-stack", "j"),
      e("red-eye", "Red eye tool", "eye", "j"),
    ],
  },
  { id: "brush", label: "Brush", tools: [e("brush", "Brush tool", "brush", "b"), e("pencil", "Pencil tool", "pencil", "b")] },
  { id: "clone", label: "Clone stamp", tools: [e("clone", "Clone stamp tool", "stamp", "s")] },
  { id: "eraser", label: "Eraser", tools: [e("eraser", "Eraser tool", "eraser", "e")] },
  { id: "gradient", label: "Gradient", tools: [e("gradient", "Gradient tool", "blend", "g"), e("bucket", "Paint bucket tool", "paint-bucket", "g")] },
  { id: "blur", label: "Blur", tools: [e("blur-brush", "Blur tool", "droplet"), e("sharpen-brush", "Sharpen tool", "triangle"), e("smudge", "Smudge tool", "pointer")] },
  { id: "dodge", label: "Dodge", tools: [e("dodge", "Dodge tool", "sun-medium", "o"), e("burn", "Burn tool", "flame", "o"), e("sponge", "Sponge tool", "spray-can", "o")] },
  { id: "text", label: "Type", tools: [e("text", "Type tool", "type", "t")] },
  {
    id: "annotate",
    label: "Markup",
    tools: [e("arrow", "Arrow tool", "arrow-up-right"), e("pen", "Pen tool", "pen-line"), e("highlighter", "Highlighter tool", "highlighter"), e("redact", "Redact tool", "rectangle-horizontal"), e("pixelate", "Pixelate tool", "grid-3x3")],
  },
  {
    id: "shape",
    label: "Shapes",
    tools: [e("shape-rect", "Rectangle tool", "square", "u"), e("shape-ellipse", "Ellipse tool", "circle", "u"), e("shape-line", "Line tool", "slash", "u")],
  },
  { id: "hand", label: "Hand", tools: [e("hand", "Hand tool", "hand", "h")] },
  { id: "zoom", label: "Zoom", tools: [e("zoom", "Zoom tool", "zoom-in", "z")] },
];

/** Lite's markup tools, in its sheet order. */
export const LITE_MARKUP_TOOLS: ToolEntry[] = [
  e("arrow", "Arrow", "arrow-up-right"),
  e("shape-rect", "Box", "square"),
  e("shape-ellipse", "Circle", "circle"),
  e("pen", "Pen", "pen-line"),
  e("highlighter", "Highlighter", "highlighter"),
  e("text", "Text", "type"),
  e("redact", "Redact", "rectangle-horizontal"),
  e("pixelate", "Pixelate", "grid-3x3"),
];

/** The entry for a tool id, wherever it sits. */
export function toolEntry(id: string): ToolEntry | null {
  for (const g of TOOL_GROUPS) for (const t of g.tools) if (t.id === id) return t;
  return null;
}

/**
 * Shortcut letter → tool ids in cycling order. Press the letter to pick the
 * group's current tool; shift+letter steps to the next id in the list.
 * `transform` is Cmd/Ctrl+T and is not listed.
 */
export function shortcutMap(): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const g of TOOL_GROUPS) {
    for (const t of g.tools) {
      if (t.shortcut) (out[t.shortcut] ??= []).push(t.id);
    }
  }
  return out;
}

/** The tool a shortcut press selects, given the tool now active. */
export function toolForShortcut(letter: string, current: string, shift: boolean): string | null {
  const ids = shortcutMap()[letter.toLowerCase()];
  if (!ids?.length) return null;
  const i = ids.indexOf(current);
  if (i < 0) return ids[0];
  return shift ? ids[(i + 1) % ids.length] : current;
}
