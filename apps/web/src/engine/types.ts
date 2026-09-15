// Types mirroring the Rust engine's JSON (crates/editor-core/src/summary.rs
// and the serde shapes in layer.rs / adjust.rs).

export type LayerId = number;

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Rgba8 {
  r: number;
  g: number;
  b: number;
  a?: number;
}

export type BlendMode =
  | "normal" | "dissolve"
  | "darken" | "multiply" | "color-burn" | "linear-burn" | "darker-color"
  | "lighten" | "screen" | "color-dodge" | "linear-dodge" | "lighter-color"
  | "overlay" | "soft-light" | "hard-light" | "vivid-light" | "linear-light" | "pin-light" | "hard-mix"
  | "difference" | "exclusion" | "subtract" | "divide"
  | "hue" | "saturation" | "color" | "luminosity";

/** Photoshop's menu order, with separators as `null`. */
export const BLEND_MODES: (BlendMode | null)[] = [
  "normal", "dissolve", null,
  "darken", "multiply", "color-burn", "linear-burn", "darker-color", null,
  "lighten", "screen", "color-dodge", "linear-dodge", "lighter-color", null,
  "overlay", "soft-light", "hard-light", "vivid-light", "linear-light", "pin-light", "hard-mix", null,
  "difference", "exclusion", "subtract", "divide", null,
  "hue", "saturation", "color", "luminosity",
];

export const BLEND_LABELS: Record<BlendMode, string> = {
  normal: "Normal", dissolve: "Dissolve",
  darken: "Darken", multiply: "Multiply", "color-burn": "Color Burn", "linear-burn": "Linear Burn", "darker-color": "Darker Color",
  lighten: "Lighten", screen: "Screen", "color-dodge": "Color Dodge", "linear-dodge": "Linear Dodge (Add)", "lighter-color": "Lighter Color",
  overlay: "Overlay", "soft-light": "Soft Light", "hard-light": "Hard Light", "vivid-light": "Vivid Light", "linear-light": "Linear Light", "pin-light": "Pin Light", "hard-mix": "Hard Mix",
  difference: "Difference", exclusion: "Exclusion", subtract: "Subtract", divide: "Divide",
  hue: "Hue", saturation: "Saturation", color: "Color", luminosity: "Luminosity",
};

export interface Locks {
  transparency: boolean;
  pixels: boolean;
  position: boolean;
  all: boolean;
}

export interface Develop {
  exposure: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  temperature: number;
  tint: number;
  vibrance: number;
  saturation: number;
  clarity: number;
  dehaze: number;
  vignette: number;
  grain: number;
  black_white: number;
}

export const DEVELOP_DEFAULT: Develop = {
  exposure: 0, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0,
  temperature: 0, tint: 0, vibrance: 0, saturation: 0, clarity: 0, dehaze: 0,
  vignette: 0, grain: 0, black_white: 0,
};

/** Adjustment parameters: `kind` plus that kind's fields (see adjust.rs). */
export type Adjustment = { kind: string; [field: string]: unknown };

export interface GradientStop {
  pos: number;
  color: Rgba8;
}

export type Fill =
  | { kind: "solid"; color: Rgba8 }
  | { kind: "gradient"; stops: GradientStop[]; gradient: "linear" | "radial" | "angle" | "reflected" | "diamond"; from: Point; to: Point; reverse?: boolean };

export interface TextData {
  text: string;
  font_family: string;
  font_size: number;
  font_weight: number;
  italic: boolean;
  color: Rgba8;
  align: "left" | "center" | "right";
  line_height: number;
  letter_spacing: number;
  box_width: number | null;
  x: number;
  y: number;
  rotation: number;
  background: Rgba8 | null;
  padding: number;
  stroke: Rgba8 | null;
  stroke_width: number;
}

export type ShapeKind = "rect" | "ellipse" | "line" | "arrow" | "polygon" | "polyline" | "redact";

export interface ShapeData {
  kind: ShapeKind;
  points: Point[];
  stroke: Rgba8 | null;
  stroke_width: number;
  fill: Rgba8 | null;
  corner_radius: number;
  arrow_start: boolean;
  arrow_end: boolean;
  dash: [number, number] | null;
  rotation: number;
}

export type LayerKindName = "pixel" | "adjustment" | "fill" | "group" | "text" | "shape" | "smart";

export interface SmartFilterInfo {
  filter: Record<string, unknown> & { op: string };
  enabled: boolean;
  opacity: number;
  blend: BlendMode;
}

export interface SmartInfo {
  source: "pixels" | "document";
  width: number;
  height: number;
  /** TL, TR, BR, BL in document pixels. */
  quad: [Point, Point, Point, Point];
  filters: SmartFilterInfo[];
  layers: number;
}

export interface LayerInfo {
  id: LayerId;
  name: string;
  kind: LayerKindName;
  visible: boolean;
  opacity: number;
  fillOpacity: number;
  blend: BlendMode;
  clip: boolean;
  locks: Locks;
  colorLabel: number;
  rev: number;
  provenance: string[];
  mask: { enabled: boolean; linked: boolean; density: number } | null;
  bounds?: Rect;
  adjustment?: Adjustment;
  fill?: Fill;
  passThrough?: boolean;
  expanded?: boolean;
  children?: LayerInfo[];
  text?: TextData;
  shape?: ShapeData;
  smart?: SmartInfo;
  effects?: Record<string, unknown> | null;
}

export interface Summary {
  width: number;
  height: number;
  resolution: number;
  active: LayerId | null;
  /** Bottom to top. */
  layers: LayerInfo[];
  selection: { bounds: Rect } | null;
  history: { undo: string[]; redo: string[] };
  revision: number;
  source: string | null;
}

export interface ExecResult {
  changed: boolean;
  warnings?: string[];
  label?: string | null;
  dirty?: Rect | null;
  data?: Record<string, unknown> | null;
  revision: number;
}

/** Find a layer anywhere in the tree. */
export function findLayer(layers: LayerInfo[], id: LayerId | null | undefined): LayerInfo | null {
  if (id == null) return null;
  for (const l of layers) {
    if (l.id === id) return l;
    if (l.children) {
      const f = findLayer(l.children, id);
      if (f) return f;
    }
  }
  return null;
}

/** Every layer, depth-first, bottom to top. */
export function allLayers(layers: LayerInfo[]): LayerInfo[] {
  const out: LayerInfo[] = [];
  const walk = (ls: LayerInfo[]) => {
    for (const l of ls) {
      out.push(l);
      if (l.children) walk(l.children);
    }
  };
  walk(layers);
  return out;
}
