// Summary fields newer than engine/types.ts: layer styles
// (crates/editor-core/src/effects.rs) and smart objects (layer.rs, smart.rs).

import type { BlendMode, GradientStop, LayerInfo, Point, Rgba8 } from "../engine/types";

export interface ShadowFx {
  enabled: boolean;
  blend: BlendMode;
  color: Rgba8;
  opacity: number;
  angle: number;
  distance: number;
  spread: number;
  size: number;
}

export interface GlowFx {
  enabled: boolean;
  blend: BlendMode;
  color: Rgba8;
  opacity: number;
  spread: number;
  size: number;
  source: "edge" | "center";
}

export interface BevelFx {
  enabled: boolean;
  style: "inner-bevel" | "outer-bevel" | "emboss" | "pillow-emboss";
  technique: "smooth" | "chisel-hard";
  depth: number;
  down: boolean;
  size: number;
  soften: number;
  angle: number;
  altitude: number;
  highlight_blend: BlendMode;
  highlight_color: Rgba8;
  highlight_opacity: number;
  shadow_blend: BlendMode;
  shadow_color: Rgba8;
  shadow_opacity: number;
}

export interface SatinFx {
  enabled: boolean;
  blend: BlendMode;
  color: Rgba8;
  opacity: number;
  angle: number;
  distance: number;
  size: number;
  invert: boolean;
}

export interface ColorOverlayFx {
  enabled: boolean;
  blend: BlendMode;
  color: Rgba8;
  opacity: number;
}

export interface GradientOverlayFx {
  enabled: boolean;
  blend: BlendMode;
  opacity: number;
  stops: GradientStop[];
  gradient: "linear" | "radial" | "angle" | "reflected" | "diamond";
  angle: number;
  scale: number;
  reverse: boolean;
}

export interface StrokeFx {
  enabled: boolean;
  size: number;
  position: "outside" | "inside" | "center";
  blend: BlendMode;
  opacity: number;
  color: Rgba8;
}

export interface LayerEffects {
  enabled: boolean;
  scale: number;
  drop_shadow: ShadowFx | null;
  inner_shadow: ShadowFx | null;
  outer_glow: GlowFx | null;
  inner_glow: GlowFx | null;
  bevel: BevelFx | null;
  satin: SatinFx | null;
  color_overlay: ColorOverlayFx | null;
  gradient_overlay: GradientOverlayFx | null;
  stroke: StrokeFx | null;
}

export type EffectKey = Exclude<keyof LayerEffects, "enabled" | "scale">;

export interface SmartFilter {
  filter: Record<string, unknown> & { op: string };
  enabled: boolean;
  opacity: number;
  blend: BlendMode;
}

export interface SmartInfo {
  source: "pixels" | "document";
  width: number;
  height: number;
  quad: Point[];
  filters: SmartFilter[];
  layers: number;
}

export type ProLayer = Omit<LayerInfo, "effects" | "smart" | "children"> & {
  effects?: LayerEffects | null;
  smart?: SmartInfo;
  children?: ProLayer[];
};

const BLACK = { r: 0, g: 0, b: 0, a: 255 };
const WHITE = { r: 255, g: 255, b: 255, a: 255 };

/** Photoshop's order in the Layer Style dialog, with engine defaults. */
export const EFFECTS: { key: EffectKey; label: string; make: () => NonNullable<LayerEffects[EffectKey]> }[] = [
  {
    key: "bevel",
    label: "Bevel & Emboss",
    make: () => ({
      enabled: true,
      style: "inner-bevel",
      technique: "smooth",
      depth: 100,
      down: false,
      size: 5,
      soften: 0,
      angle: 120,
      altitude: 30,
      highlight_blend: "screen",
      highlight_color: WHITE,
      highlight_opacity: 0.75,
      shadow_blend: "multiply",
      shadow_color: BLACK,
      shadow_opacity: 0.75,
    }),
  },
  { key: "stroke", label: "Stroke", make: () => ({ enabled: true, size: 3, position: "outside", blend: "normal", opacity: 1, color: BLACK }) },
  { key: "inner_shadow", label: "Inner Shadow", make: () => ({ enabled: true, blend: "multiply", color: BLACK, opacity: 0.35, angle: 120, distance: 5, spread: 0, size: 5 }) },
  { key: "inner_glow", label: "Inner Glow", make: () => ({ enabled: true, blend: "screen", color: { r: 255, g: 255, b: 190, a: 255 }, opacity: 0.75, spread: 0, size: 5, source: "edge" }) },
  { key: "satin", label: "Satin", make: () => ({ enabled: true, blend: "multiply", color: BLACK, opacity: 0.5, angle: 19, distance: 11, size: 14, invert: true }) },
  { key: "color_overlay", label: "Color Overlay", make: () => ({ enabled: true, blend: "normal", color: { r: 255, g: 0, b: 0, a: 255 }, opacity: 1 }) },
  {
    key: "gradient_overlay",
    label: "Gradient Overlay",
    make: () => ({
      enabled: true,
      blend: "normal",
      opacity: 1,
      stops: [
        { pos: 0, color: BLACK },
        { pos: 1, color: WHITE },
      ],
      gradient: "linear",
      angle: 90,
      scale: 100,
      reverse: false,
    }),
  },
  { key: "outer_glow", label: "Outer Glow", make: () => ({ enabled: true, blend: "screen", color: { r: 255, g: 255, b: 190, a: 255 }, opacity: 0.75, spread: 0, size: 5, source: "edge" }) },
  { key: "drop_shadow", label: "Drop Shadow", make: () => ({ enabled: true, blend: "multiply", color: BLACK, opacity: 0.35, angle: 120, distance: 5, spread: 0, size: 5 }) },
];

export function emptyEffects(): LayerEffects {
  return {
    enabled: true,
    scale: 1,
    drop_shadow: null,
    inner_shadow: null,
    outer_glow: null,
    inner_glow: null,
    bevel: null,
    satin: null,
    color_overlay: null,
    gradient_overlay: null,
    stroke: null,
  };
}

export function activeEffects(fx: LayerEffects | null | undefined): EffectKey[] {
  if (!fx) return [];
  return EFFECTS.map((e) => e.key).filter((k) => !!fx[k]);
}
