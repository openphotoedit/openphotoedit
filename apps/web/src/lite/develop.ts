// Lite's photographic vocabulary: master sliders, their sub-slider curves,
// and the named Looks. Pure data and arithmetic; nothing here talks to the
// engine.

import { DEVELOP_DEFAULT, type Develop } from "../engine/types";

export type DevelopKey = keyof Develop;

export interface SubSlider {
  key: DevelopKey;
  label: string;
  min: number;
  max: number;
  step: number;
}

export interface Group {
  id: "light" | "color" | "bw" | "detail";
  label: string;
  /** Range of the master slider. */
  min: number;
  max: number;
  subs: SubSlider[];
  /** Sub-slider values for a master value. */
  fromMaster(m: number): Partial<Develop>;
  /** The master value that best describes the current parameters. */
  toMaster(d: Develop): number;
}

const pos = (v: number, a: number, b: number) => (v >= 0 ? v * a : v * b);
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
const round = (v: number, step: number) => Math.round(v / step) * step;

export const GROUPS: Group[] = [
  {
    id: "light",
    label: "Light",
    min: -100,
    max: 100,
    subs: [
      { key: "exposure", label: "Exposure", min: -3, max: 3, step: 0.01 },
      { key: "contrast", label: "Contrast", min: -100, max: 100, step: 1 },
      { key: "highlights", label: "Highlights", min: -100, max: 100, step: 1 },
      { key: "shadows", label: "Shadows", min: -100, max: 100, step: 1 },
      { key: "whites", label: "Whites", min: -100, max: 100, step: 1 },
      { key: "blacks", label: "Blacks", min: -100, max: 100, step: 1 },
    ],
    // Brightening lifts exposure and opens the shadows while holding the
    // highlights back, so a sky does not clip; darkening does the reverse
    // and deepens the blacks rather than greying them.
    fromMaster(m) {
      const l = m / 100;
      return {
        exposure: round(pos(l, 0.95, 1.1), 0.01),
        shadows: Math.round(pos(l, 50, 20)),
        highlights: Math.round(pos(l, -45, 15)),
        whites: Math.round(pos(l, 10, 25)),
        blacks: Math.round(pos(l, 12, 35)),
        contrast: Math.round(pos(l, 5, 8)),
      };
    },
    toMaster(d) {
      return clamp(Math.round(d.exposure >= 0 ? (d.exposure / 0.95) * 100 : (d.exposure / 1.1) * 100), -100, 100);
    },
  },
  {
    id: "color",
    label: "Color",
    min: -100,
    max: 100,
    subs: [
      { key: "temperature", label: "Warmth", min: -100, max: 100, step: 1 },
      { key: "tint", label: "Tint", min: -100, max: 100, step: 1 },
      { key: "vibrance", label: "Vibrance", min: -100, max: 100, step: 1 },
      { key: "saturation", label: "Saturation", min: -100, max: 100, step: 1 },
    ],
    // Vibrance leads on the way up so skin does not go orange; saturation
    // leads on the way down so the far left is a clean desaturation.
    fromMaster(m) {
      const c = m / 100;
      return { vibrance: Math.round(pos(c, 70, 40)), saturation: Math.round(pos(c, 30, 75)) };
    },
    toMaster(d) {
      return clamp(Math.round(d.vibrance >= 0 ? (d.vibrance / 70) * 100 : (d.vibrance / 40) * 100), -100, 100);
    },
  },
  {
    id: "bw",
    label: "Black and white",
    min: 0,
    max: 100,
    subs: [{ key: "black_white", label: "Intensity", min: 0, max: 100, step: 1 }],
    fromMaster(m) {
      return { black_white: Math.round(m) };
    },
    toMaster(d) {
      return Math.round(d.black_white);
    },
  },
  {
    id: "detail",
    label: "Detail",
    min: 0,
    max: 100,
    subs: [
      { key: "clarity", label: "Clarity", min: -100, max: 100, step: 1 },
      { key: "dehaze", label: "Dehaze", min: -100, max: 100, step: 1 },
    ],
    fromMaster(m) {
      return { clarity: Math.round(m * 0.6), dehaze: Math.round(m * 0.3) };
    },
    toMaster(d) {
      return clamp(Math.round(d.clarity / 0.6), 0, 100);
    },
  },
];

export function groupKeys(g: Group): DevelopKey[] {
  return g.subs.map((s) => s.key);
}

export function isGroupNeutral(g: Group, d: Develop): boolean {
  return groupKeys(g).every((k) => d[k] === 0);
}

export function zeroed(keys: DevelopKey[]): Partial<Develop> {
  return Object.fromEntries(keys.map((k) => [k, 0]));
}

/** Clamp every field to the engine's documented range. */
export function sanitize(d: Develop): Develop {
  const out = { ...d };
  for (const k of Object.keys(out) as DevelopKey[]) {
    const v = Number(out[k]) || 0;
    if (k === "exposure") out[k] = clamp(v, -5, 5);
    else if (k === "grain" || k === "black_white") out[k] = clamp(v, 0, 100);
    else out[k] = clamp(v, -100, 100);
  }
  return out;
}

export function addDevelop(base: Develop, delta: Partial<Develop>): Develop {
  const out = { ...base };
  for (const [k, v] of Object.entries(delta) as [DevelopKey, number][]) out[k] = (out[k] ?? 0) + v;
  return sanitize(out);
}

// ---------------------------------------------------------------------------
// Looks

/** `[cyan↔red, magenta↔green, yellow↔blue]` per tonal range, -100..100. */
export interface Tint {
  shadows: [number, number, number];
  midtones: [number, number, number];
  highlights: [number, number, number];
}

export interface Look {
  id: string;
  name: string;
  develop: Partial<Develop>;
  tint?: Partial<Tint>;
}

export const LOOKS: Look[] = [
  { id: "vivid", name: "Vivid", develop: { contrast: 25, vibrance: 65, saturation: 18, clarity: 12 } },
  { id: "punch", name: "Punch", develop: { contrast: 45, vibrance: 40, saturation: 20, clarity: 35, dehaze: 15, blacks: -12 } },
  { id: "warm", name: "Warm", develop: { temperature: 55, tint: 6, vibrance: 12, exposure: 0.08 } },
  { id: "golden", name: "Golden", develop: { temperature: 60, tint: 12, vibrance: 22, highlights: -15, shadows: 12 }, tint: { highlights: [14, 0, -30] } },
  { id: "cool", name: "Cool", develop: { temperature: -55, tint: -4, vibrance: 8 } },
  { id: "airy", name: "Airy", develop: { exposure: 0.5, highlights: -30, shadows: 45, contrast: -18, saturation: -10, temperature: -10 } },
  { id: "matte", name: "Matte", develop: { contrast: -25, blacks: 55, highlights: -18, saturation: -15 } },
  { id: "fade", name: "Fade", develop: { contrast: -40, blacks: 60, whites: -25, saturation: -40 } },
  { id: "film", name: "Film", develop: { contrast: 14, blacks: 30, saturation: -18, grain: 35, temperature: 14 }, tint: { shadows: [-12, 0, 18], highlights: [12, 0, -14] } },
  { id: "vintage", name: "Vintage", develop: { contrast: -20, blacks: 38, temperature: 35, saturation: -32, grain: 35, vignette: -35 }, tint: { highlights: [18, 0, -26], shadows: [0, -8, 14] } },
  { id: "cinematic", name: "Cinematic", develop: { contrast: 22, temperature: -10, saturation: -14, vignette: -28, blacks: -8 }, tint: { shadows: [-35, 0, 35], highlights: [28, 0, -38] } },
  { id: "mono", name: "Mono", develop: { black_white: 100, contrast: 12 } },
  { id: "noir", name: "Noir", develop: { black_white: 100, contrast: 60, blacks: -30, clarity: 30, vignette: -45 } },
  { id: "sepia", name: "Sepia", develop: { black_white: 100, contrast: 8 }, tint: { midtones: [40, 0, -50], highlights: [16, 0, -20], shadows: [14, 0, -16] } },
];

export function lookDevelop(look: Look, strength: number): Develop {
  const d = { ...DEVELOP_DEFAULT };
  for (const [k, v] of Object.entries(look.develop) as [DevelopKey, number][]) d[k] = v * strength;
  return sanitize(d);
}

export function lookTint(look: Look, strength: number) {
  const z: [number, number, number] = [0, 0, 0];
  const scale = (v?: [number, number, number]) => (v ?? z).map((x) => Math.round(x * strength)) as [number, number, number];
  return {
    kind: "color-balance",
    shadows: scale(look.tint?.shadows),
    midtones: scale(look.tint?.midtones),
    highlights: scale(look.tint?.highlights),
    preserve_luminosity: true,
  };
}

/** Which Look a develop layer's parameters came from, and at what strength. */
export function identifyLook(d: Develop): { look: Look; strength: number } | null {
  for (const look of LOOKS) {
    const entries = Object.entries(look.develop) as [DevelopKey, number][];
    const [k0, v0] = entries.reduce((a, b) => (Math.abs(b[1]) > Math.abs(a[1]) ? b : a));
    const s = d[k0] / v0;
    if (!(s > 0.001 && s <= 1.0001)) continue;
    const fits = (Object.keys(d) as DevelopKey[]).every((k) => Math.abs(d[k] - (look.develop[k] ?? 0) * s) < 0.02 + Math.abs((look.develop[k] ?? 0) * 0.002));
    if (fits) return { look, strength: Math.round(s * 100) / 100 };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Auto, computed from pixels when the engine's analyze.auto is not available.

export type AutoStyle = "auto" | "vivid" | "natural" | "bw";

/**
 * A tone and colour estimate from a small RGBA thumbnail: exposure from the
 * median luminance, contrast from the spread between the 2nd and 98th
 * percentiles, white balance from the grey-world average.
 */
export function estimateAuto(rgba: Uint8Array | Uint8ClampedArray, style: AutoStyle): Develop {
  const hist = new Uint32Array(256);
  let r = 0, g = 0, b = 0, n = 0;
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i + 3] < 8) continue;
    const L = Math.round(0.2126 * rgba[i] + 0.7152 * rgba[i + 1] + 0.0722 * rgba[i + 2]);
    hist[L]++;
    r += rgba[i];
    g += rgba[i + 1];
    b += rgba[i + 2];
    n++;
  }
  const d = { ...DEVELOP_DEFAULT };
  if (!n) return d;
  const pct = (p: number) => {
    let acc = 0;
    for (let i = 0; i < 256; i++) {
      acc += hist[i];
      if (acc >= n * p) return i;
    }
    return 255;
  };
  const p02 = pct(0.02), p50 = pct(0.5), p98 = pct(0.98);
  const target = 118;
  d.exposure = clamp(Math.log2(target / Math.max(12, p50)) * 0.7, -1.2, 1.2);
  const spread = p98 - p02;
  d.contrast = clamp((200 - spread) * 0.2, -10, 30);
  d.shadows = clamp((70 - p02 * 1.2) * 0.5, 0, 35);
  d.highlights = clamp((p98 - 235) * -1.2, -45, 0);
  const avg = (r + g + b) / 3 / n;
  d.temperature = clamp(((b / n - r / n) / Math.max(1, avg)) * 60, -25, 25);
  d.tint = clamp(((g / n - (r / n + b / n) / 2) / Math.max(1, avg)) * -50, -20, 20);
  d.vibrance = 18;
  d.clarity = 6;
  if (style === "vivid") {
    d.contrast += 14;
    d.vibrance = 45;
    d.saturation = 12;
    d.clarity = 16;
  } else if (style === "natural") {
    d.exposure *= 0.7;
    d.contrast *= 0.5;
    d.vibrance = 8;
    d.clarity = 0;
  } else if (style === "bw") {
    d.black_white = 100;
    d.contrast += 12;
    d.vibrance = 0;
    d.clarity = 10;
  }
  const out = sanitize(d);
  out.exposure = round(out.exposure, 0.01);
  for (const k of Object.keys(out) as DevelopKey[]) if (k !== "exposure") out[k] = Math.round(out[k]);
  return out;
}
