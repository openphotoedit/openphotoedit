// Adjustment kinds in Photoshop's menu order, their icons and default
// parameters (mirroring `Adjustment::default_for` in adjust.rs, so the UI
// can show a complete editor before the engine answers).

import type { Component } from "svelte";
import Sun from "@lucide/svelte/icons/sun";
import ChartColumn from "@lucide/svelte/icons/chart-column";
import Spline from "@lucide/svelte/icons/spline";
import Aperture from "@lucide/svelte/icons/aperture";
import Droplet from "@lucide/svelte/icons/droplet";
import Palette from "@lucide/svelte/icons/palette";
import Scale from "@lucide/svelte/icons/scale";
import Contrast from "@lucide/svelte/icons/contrast";
import Camera from "@lucide/svelte/icons/camera";
import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
import FlipHorizontal from "@lucide/svelte/icons/flip-horizontal";
import Layers2 from "@lucide/svelte/icons/layers-2";
import SquareSplitHorizontal from "@lucide/svelte/icons/square-split-horizontal";
import Rainbow from "@lucide/svelte/icons/rainbow";
import Pipette from "@lucide/svelte/icons/pipette";
import Table2 from "@lucide/svelte/icons/table-2";
import Film from "@lucide/svelte/icons/film";
import type { Adjustment } from "../engine/types";
import { t } from "../lib/i18n";
import { DEFAULT_BANDS } from "./adjust/hue-bands";

export interface AdjustmentKind {
  kind: string;
  label: string;
  icon: Component<{ size?: number | string }>;
  /** Image › Adjustments shortcut (destructive), Photoshop's. */
  shortcut?: string;
}

/** Photoshop's order, with `null` separators. */
export const ADJUSTMENT_KINDS: (AdjustmentKind | null)[] = [
  { kind: "brightness-contrast", label: t("Brightness/Contrast"), icon: Sun },
  { kind: "levels", label: t("Levels"), icon: ChartColumn, shortcut: "Mod+L" },
  { kind: "curves", label: t("Curves"), icon: Spline, shortcut: "Mod+M" },
  { kind: "exposure", label: t("Exposure"), icon: Aperture },
  null,
  { kind: "vibrance", label: t("Vibrance"), icon: Droplet },
  { kind: "hue-saturation", label: t("Hue/Saturation"), icon: Palette, shortcut: "Mod+U" },
  { kind: "color-balance", label: t("Color Balance"), icon: Scale, shortcut: "Mod+B" },
  { kind: "black-white", label: t("Black & White"), icon: Contrast, shortcut: "Alt+Shift+Mod+B" },
  { kind: "photo-filter", label: t("Photo Filter"), icon: Camera },
  { kind: "channel-mixer", label: t("Channel Mixer"), icon: SlidersHorizontal },
  { kind: "color-lookup", label: t("Color Lookup"), icon: Table2 },
  null,
  { kind: "invert", label: t("Invert"), icon: FlipHorizontal, shortcut: "Mod+I" },
  { kind: "posterize", label: t("Posterize"), icon: Layers2 },
  { kind: "threshold", label: t("Threshold"), icon: SquareSplitHorizontal },
  { kind: "gradient-map", label: t("Gradient Map"), icon: Rainbow },
  { kind: "selective-color", label: t("Selective Color"), icon: Pipette },
  { kind: "grain", label: t("Grain"), icon: Film },
  null,
  { kind: "develop", label: t("Camera Raw"), icon: Grid2x2, shortcut: "Shift+Mod+A" },
];

export function kindInfo(kind: string): AdjustmentKind | null {
  return ADJUSTMENT_KINDS.find((k) => k?.kind === kind) ?? null;
}

const levelsChannel = () => ({ in_black: 0, in_white: 255, gamma: 1, out_black: 0, out_white: 255 });
const curve = () => [
  [0, 0],
  [255, 255],
];
const hsl = () => ({ hue: 0, saturation: 0, lightness: 0 });

export function defaultAdjustment(kind: string): Adjustment {
  switch (kind) {
    case "brightness-contrast":
      return { kind, brightness: 0, contrast: 0, legacy: false };
    case "levels":
      return { kind, master: levelsChannel(), red: levelsChannel(), green: levelsChannel(), blue: levelsChannel() };
    case "curves":
      return { kind, master: curve(), red: curve(), green: curve(), blue: curve() };
    case "exposure":
      return { kind, exposure: 0, offset: 0, gamma: 1 };
    case "vibrance":
      return { kind, vibrance: 0, saturation: 0 };
    case "hue-saturation":
      return {
        kind,
        master: hsl(),
        ranges: Array.from({ length: 6 }, hsl),
        colorize: false,
        colorize_hue: 0,
        colorize_saturation: 25,
        colorize_lightness: 0,
        bands: DEFAULT_BANDS.map((b) => [...b]),
      };
    case "color-balance":
      return { kind, shadows: [0, 0, 0], midtones: [0, 0, 0], highlights: [0, 0, 0], preserve_luminosity: true };
    case "black-white":
      return { kind, weights: [40, 60, 40, 60, 20, 80], tint: false, tint_color: { r: 225, g: 211, b: 179, a: 255 } };
    case "photo-filter":
      return { kind, color: { r: 236, g: 138, b: 0, a: 255 }, density: 25, preserve_luminosity: true };
    case "channel-mixer":
      return { kind, red: [100, 0, 0, 0], green: [0, 100, 0, 0], blue: [0, 0, 100, 0], monochrome: false };
    case "invert":
      return { kind };
    case "posterize":
      return { kind, levels: 4 };
    case "threshold":
      return { kind, level: 128 };
    case "gradient-map":
      return {
        kind,
        stops: [
          { pos: 0, color: { r: 0, g: 0, b: 0, a: 255 } },
          { pos: 1, color: { r: 255, g: 255, b: 255, a: 255 } },
        ],
        reverse: false,
      };
    case "selective-color":
      return { kind, colors: Array.from({ length: 9 }, () => [0, 0, 0, 0]), absolute: false };
    case "develop":
      return {
        kind,
        exposure: 0,
        contrast: 0,
        highlights: 0,
        shadows: 0,
        whites: 0,
        blacks: 0,
        temperature: 0,
        tint: 0,
        vibrance: 0,
        saturation: 0,
        clarity: 0,
        dehaze: 0,
        vignette: 0,
        grain: 0,
        black_white: 0,
      };
    case "grain":
      // Each new Grain layer gets its own pattern.
      return { kind, amount: 25, size: 1.5, roughness: 50, seed: newSeed() };
    case "color-lookup":
      return { kind, name: t("Identity"), size: 2, table: identityLut(2), strength: 1 };
    default:
      return { kind };
  }
}

/** A random 32-bit seed (Grain, Add Noise). */
export function newSeed(): number {
  return crypto.getRandomValues(new Uint32Array(1))[0];
}

/** Fill in any fields missing from an engine value, so editors never read `undefined`. */
export function withDefaults(a: Adjustment): Adjustment {
  const d = defaultAdjustment(a.kind);
  return { ...d, ...a };
}

export function identityLut(size: number): number[] {
  const out: number[] = [];
  for (let b = 0; b < size; b++) for (let g = 0; g < size; g++) for (let r = 0; r < size; r++) out.push(r / (size - 1), g / (size - 1), b / (size - 1));
  return out;
}

/** Parse an Adobe/Resolve `.cube` 3D LUT the way adjust.rs does. */
export function parseCube(name: string, text: string): Adjustment {
  let size = 0;
  const table: number[] = [];
  let dmin = [0, 0, 0];
  let dmax = [1, 1, 1];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const parts = line.split(/\s+/);
    const head = parts[0];
    if (head === "LUT_3D_SIZE") size = parseInt(parts[1], 10);
    else if (head === "LUT_1D_SIZE") throw new Error(t("1D .cube files are not supported. Choose a 3D LUT."));
    else if (head === "DOMAIN_MIN") dmin = parts.slice(1, 4).map(Number);
    else if (head === "DOMAIN_MAX") dmax = parts.slice(1, 4).map(Number);
    else if (head === "TITLE") continue;
    else {
      const v = parts.slice(0, 3).map(Number);
      if (v.length === 3 && v.every(Number.isFinite)) table.push((v[0] - dmin[0]) / (dmax[0] - dmin[0]), (v[1] - dmin[1]) / (dmax[1] - dmin[1]), (v[2] - dmin[2]) / (dmax[2] - dmin[2]));
    }
  }
  if (size < 2 || table.length !== size * size * size * 3) {
    throw new Error(t("This .cube file is incomplete: expected {n} entries.", { n: size ** 3 }));
  }
  return { kind: "color-lookup", name, size, table, strength: 1 };
}
