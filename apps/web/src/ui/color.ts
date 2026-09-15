// Colour conversions for pickers and panels. RGB is 0..255, hue 0..360,
// saturation and brightness 0..100 (Photoshop's HSB).

import type { Rgba8 } from "../engine/types";

export interface Hsb {
  h: number;
  s: number;
  b: number;
}

export function rgbToHsb({ r, g, b }: Rgba8): Hsb {
  const R = r / 255;
  const G = g / 255;
  const B = b / 255;
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === R) h = ((G - B) / d) % 6;
    else if (max === G) h = (B - R) / d + 2;
    else h = (R - G) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : (d / max) * 100, b: max * 100 };
}

export function hsbToRgb({ h, s, b }: Hsb, a = 255): Rgba8 {
  const S = s / 100;
  const V = b / 100;
  const C = V * S;
  const X = C * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = V - C;
  let rgb: [number, number, number];
  const hh = ((h % 360) + 360) % 360;
  if (hh < 60) rgb = [C, X, 0];
  else if (hh < 120) rgb = [X, C, 0];
  else if (hh < 180) rgb = [0, C, X];
  else if (hh < 240) rgb = [0, X, C];
  else if (hh < 300) rgb = [X, 0, C];
  else rgb = [C, 0, X];
  return { r: Math.round((rgb[0] + m) * 255), g: Math.round((rgb[1] + m) * 255), b: Math.round((rgb[2] + m) * 255), a };
}

export function toHex({ r, g, b }: Rgba8): string {
  return [r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
}

export function fromHex(hex: string): Rgba8 | null {
  let h = hex.trim().replace(/^#/, "");
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  if (!/^[0-9a-f]{6}$/i.test(h)) return null;
  const n = parseInt(h, 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 255 };
}

export function css(c: Rgba8 | null | undefined): string {
  if (!c) return "transparent";
  return `rgb(${c.r} ${c.g} ${c.b} / ${(c.a ?? 255) / 255})`;
}

export function sameColor(a: Rgba8 | null | undefined, b: Rgba8 | null | undefined) {
  return !!a && !!b && a.r === b.r && a.g === b.g && a.b === b.b && (a.a ?? 255) === (b.a ?? 255);
}
