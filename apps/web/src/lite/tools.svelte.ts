// Lite drives the shared canvas tools (apps/web/src/tools) with its own,
// simpler vocabulary: a colour row, S/M/L widths relative to the photo, a
// brush size that feels the same on any photo. This file maps that
// vocabulary onto `toolSettings`.

import { editor } from "../lib/editor.svelte";
import { LITE_MARKUP_TOOLS } from "../tools/groups";
import { sizeFor, setSizeFor, toolSettings } from "../tools/settings.svelte";
import { toolState } from "../tools/state.svelte";
import type { Rgba8 } from "../engine/types";
import { lite, type Category } from "./lite.svelte";

export type Size = "s" | "m" | "l";
export type RetouchTool = "remove" | "spot-heal" | "red-eye";

export const MARKUP_TOOL_IDS = [...LITE_MARKUP_TOOLS.map((t) => t.id), ...(LITE_MARKUP_TOOLS.some((t) => t.id === "pixelate") ? [] : ["pixelate"])];

export const SWATCHES: string[] = ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#0a84ff", "#af52de", "#ffffff", "#111111"];

export function hexToRgba(hex: string, a = 255): Rgba8 {
  const h = hex.replace("#", "");
  const n = parseInt(h.length === 3 ? h.split("").map((c) => c + c).join("") : h, 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a };
}

export function rgbaToHex(c: Rgba8): string {
  return "#" + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
}

class LiteTools {
  markup = $state("arrow");
  retouch = $state<RetouchTool>("remove");
  color = $state("#ff3b30");
  width = $state<Size>("m");
  textSize = $state<Size>("m");
  textBackground = $state(false);

  /** The canvas tool a category uses. */
  toolFor(c: Category): string {
    switch (c) {
      case "crop":
        return "crop";
      case "markup":
        return this.markup;
      case "retouch":
        return this.retouch;
      default:
        return "hand";
    }
  }

  /** The photo's short side, which sizes are relative to. */
  private get base() {
    const s = editor.summary;
    return s ? Math.min(s.width, s.height) : 1000;
  }

  applyColor(hex: string) {
    this.color = hex;
    const c = hexToRgba(hex);
    toolSettings.shapeStroke = c;
    toolSettings.shapeFill = c;
    toolSettings.textStroke = { r: 0, g: 0, b: 0, a: 150 };
    if (this.textBackground) {
      toolSettings.textBackground = c;
      const light = c.r * 0.299 + c.g * 0.587 + c.b * 0.114 > 150;
      toolSettings.textColor = light ? { r: 17, g: 17, b: 17, a: 255 } : { r: 255, g: 255, b: 255, a: 255 };
    } else {
      toolSettings.textColor = c;
    }
    toolSettings.highlighterColor = { ...c, a: 102 };
  }

  applyWidth(size: Size) {
    this.width = size;
    // Markup widths are scaled by the tools per 1000 px of long edge.
    const w = size === "s" ? 3 : size === "m" ? 6 : 12;
    toolSettings.shapeWidth = w;
    toolSettings.penWidth = w;
    toolSettings.highlighterWidth = w * 4.5;
  }

  applyTextSize(size: Size) {
    this.textSize = size;
    const f = size === "s" ? 0.035 : size === "m" ? 0.055 : 0.085;
    toolSettings.fontSize = Math.max(12, Math.round(this.base * f));
  }

  applyTextBackground(on: boolean) {
    this.textBackground = on;
    toolSettings.textBackgroundOn = on;
    toolSettings.textPadding = Math.round(toolSettings.fontSize * 0.3);
    this.applyColor(this.color);
  }

  /** Put Lite's choices into the shared settings (entering a category). */
  sync() {
    toolSettings.shapeStrokeOn = true;
    toolSettings.shapeFillOn = false;
    toolSettings.shapeDashed = false;
    toolSettings.shapeRadius = 0;
    toolSettings.arrowStart = false;
    toolSettings.arrowEnd = true;
    toolSettings.fontFamily = "Geist, 'Helvetica Neue', Arial, sans-serif";
    toolSettings.fontWeight = 500;
    toolSettings.textStrokeOn = false;
    toolSettings.cropOutW = 0;
    toolSettings.cropOutH = 0;
    toolSettings.cropDeleteCropped = true;
    toolSettings.pixelateCell = Math.max(8, Math.round(this.base / 60));
    this.applyWidth(this.width);
    this.applyTextSize(this.textSize);
    this.applyTextBackground(this.textBackground);
    this.applyColor(this.color);
  }

  /** Brush size as a share of the photo's short side, 1..100. */
  get brush(): number {
    return Math.max(1, Math.min(100, Math.round((sizeFor(this.retouch === "red-eye" ? "remove" : this.retouch) / this.base) * 400)));
  }

  set brush(v: number) {
    const px = Math.max(4, (v / 400) * this.base);
    setSizeFor("remove", px);
    setSizeFor("spot-heal", px);
  }

  get selectedAnnotation(): number | null {
    return toolState.annotationSelected;
  }
}

export const liteTools = new LiteTools();

/** Retouching edits pixels: make the topmost pixel layer active. */
export async function activatePixels() {
  const s = editor.summary;
  if (!s) return;
  if (editor.active?.kind === "pixel") return;
  const px = [...lite.layers()].reverse().find((l) => l.kind === "pixel" && l.visible);
  if (px) await editor.exec({ op: "layer.set-active", id: px.id }, undefined, { quiet: true });
}

/** Keep new markup above everything: make the topmost layer active. */
export async function activateTop() {
  const id = lite.topId();
  if (id != null && editor.summary?.active !== id) await editor.exec({ op: "layer.set-active", id }, undefined, { quiet: true });
}
