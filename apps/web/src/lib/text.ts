// Text layers are shaped and rasterised by the browser, which has the fonts;
// the engine stores the parameters and the pixels. This is the one place
// that turns `TextData` into RGBA, so the editor overlay and the committed
// layer agree about wrapping, spacing and the background pill.

import type { TextData } from "../engine/types";

export interface RenderedText {
  /** Raster size in document pixels. */
  width: number;
  height: number;
  /** Raster position in document pixels. */
  x: number;
  y: number;
  rgba: Uint8ClampedArray;
}

export interface TextLayout {
  lines: string[];
  /** Content box (without padding), document pixels. */
  width: number;
  height: number;
  lineHeight: number;
}

export const DEFAULT_TEXT: TextData = {
  text: "",
  font_family: "Inter, system-ui, sans-serif",
  font_size: 48,
  font_weight: 400,
  italic: false,
  color: { r: 0, g: 0, b: 0, a: 255 },
  align: "left",
  line_height: 1.2,
  letter_spacing: 0,
  box_width: null,
  x: 0,
  y: 0,
  rotation: 0,
  background: null,
  padding: 0,
  stroke: null,
  stroke_width: 0,
};

export function cssFont(d: Pick<TextData, "font_family" | "font_size" | "font_weight" | "italic">, scale = 1) {
  return `${d.italic ? "italic " : ""}${Math.round(d.font_weight)} ${Math.max(1, d.font_size * scale)}px ${d.font_family}`;
}

function rgbaCss(c: { r: number; g: number; b: number; a?: number }) {
  return `rgba(${c.r},${c.g},${c.b},${(c.a ?? 255) / 255})`;
}

let measureCtx: OffscreenCanvasRenderingContext2D | null = null;
function mctx() {
  measureCtx ??= new OffscreenCanvas(1, 1).getContext("2d")!;
  return measureCtx;
}

const hasLetterSpacing = typeof OffscreenCanvasRenderingContext2D !== "undefined" && "letterSpacing" in OffscreenCanvasRenderingContext2D.prototype;

function applyFont(ctx: OffscreenCanvasRenderingContext2D, d: TextData, scale: number) {
  ctx.font = cssFont(d, scale);
  if (hasLetterSpacing) (ctx as unknown as { letterSpacing: string }).letterSpacing = `${d.letter_spacing * scale}px`;
}

function measure(ctx: OffscreenCanvasRenderingContext2D, s: string, spacing: number) {
  const w = ctx.measureText(s).width;
  // Canvas letterSpacing adds spacing after every character, including the last.
  if (hasLetterSpacing) return Math.max(0, w - (s.length ? spacing : 0));
  return w + Math.max(0, [...s].length - 1) * spacing;
}

/** Break text into lines, wrapping at `box_width` when set. */
export function layoutText(d: TextData, scale = 1): TextLayout {
  const ctx = mctx();
  applyFont(ctx, d, scale);
  const spacing = d.letter_spacing * scale;
  const maxW = d.box_width != null ? Math.max(1, d.box_width * scale) : Infinity;
  const lines: string[] = [];
  for (const para of d.text.split("\n")) {
    if (maxW === Infinity || measure(ctx, para, spacing) <= maxW) {
      lines.push(para);
      continue;
    }
    // Word wrap, breaking over-long words by character.
    let line = "";
    const breakChars = (s: string) => {
      for (const ch of s) {
        if (line && measure(ctx, line + ch, spacing) > maxW) {
          lines.push(line);
          line = ch;
        } else line += ch;
      }
    };
    for (const word of para.split(/(\s+)/)) {
      if (!word) continue;
      if (measure(ctx, line + word, spacing) <= maxW) {
        line += word;
      } else if (!line.trim()) {
        line = line.trim();
        breakChars(word.trimStart());
      } else {
        lines.push(line.replace(/\s+$/, ""));
        line = "";
        breakChars(word.trimStart());
      }
    }
    lines.push(line);
  }
  const lineHeight = d.font_size * d.line_height * scale;
  let width = 0;
  for (const l of lines) width = Math.max(width, measure(ctx, l, spacing));
  if (d.box_width != null) width = d.box_width * scale;
  return { lines, width: Math.max(1, width), height: Math.max(lineHeight, lines.length * lineHeight), lineHeight };
}

/** Document-space box (including padding) the text occupies before rotation. */
export function textBox(d: TextData) {
  const lay = layoutText(d);
  const pad = d.background ? d.padding : 0;
  return { x: d.x - pad, y: d.y - pad, w: lay.width + pad * 2, h: lay.height + pad * 2 };
}

/** Make sure the font is ready before measuring or drawing. */
export async function ensureFont(d: TextData) {
  try {
    if (typeof document !== "undefined" && document.fonts) await document.fonts.load(cssFont(d), d.text || "A");
  } catch {
    /* unknown family: the fallback renders */
  }
}

/**
 * Rasterise text at document scale (`scale` > 1 for supersampled previews).
 * The result is positioned in document pixels and already rotated.
 */
export async function renderText(d: TextData, scale = 1): Promise<RenderedText> {
  await ensureFont(d);
  const lay = layoutText(d, scale);
  const pad = (d.background ? d.padding : 0) * scale;
  const strokeW = d.stroke && d.stroke_width > 0 ? d.stroke_width * scale : 0;
  const bw = lay.width + pad * 2;
  const bh = lay.height + pad * 2;
  // Room for the stroke and italic overhang.
  const margin = Math.ceil(strokeW + d.font_size * scale * 0.25 + 2);
  const rot = (d.rotation * Math.PI) / 180;
  const cos = Math.abs(Math.cos(rot));
  const sin = Math.abs(Math.sin(rot));
  const fw = bw + margin * 2;
  const fh = bh + margin * 2;
  const W = Math.max(1, Math.ceil(fw * cos + fh * sin));
  const H = Math.max(1, Math.ceil(fw * sin + fh * cos));
  const c = new OffscreenCanvas(W, H);
  const ctx = c.getContext("2d")!;
  // Box centre in document coordinates.
  const cx = d.x * scale - pad + bw / 2;
  const cy = d.y * scale - pad + bh / 2;
  const x = Math.floor(cx - W / 2);
  const y = Math.floor(cy - H / 2);
  ctx.translate(cx - x, cy - y);
  ctx.rotate(rot);
  ctx.translate(-bw / 2, -bh / 2);
  if (d.background) {
    ctx.fillStyle = rgbaCss(d.background);
    const radius = Math.min(bh / 2, Math.max(4 * scale, pad * 0.75));
    ctx.beginPath();
    ctx.roundRect(0, 0, bw, bh, radius);
    ctx.fill();
  }
  applyFont(ctx, d, scale);
  ctx.textBaseline = "alphabetic";
  const metrics = ctx.measureText("Hg");
  const ascent = metrics.actualBoundingBoxAscent || d.font_size * scale * 0.8;
  const descent = metrics.actualBoundingBoxDescent || d.font_size * scale * 0.2;
  const spacing = d.letter_spacing * scale;
  lay.lines.forEach((line, i) => {
    const lw = measure(ctx, line, spacing);
    let lx = pad;
    if (d.align === "center") lx = pad + (lay.width - lw) / 2;
    else if (d.align === "right") lx = pad + lay.width - lw;
    // Centre glyphs vertically inside the line box.
    const ly = pad + i * lay.lineHeight + (lay.lineHeight - (ascent + descent)) / 2 + ascent;
    const draw = (fn: (s: string, x: number, y: number) => void) => {
      if (hasLetterSpacing || spacing === 0) {
        fn(line, lx, ly);
        return;
      }
      let px = lx;
      for (const ch of line) {
        fn(ch, px, ly);
        px += ctx.measureText(ch).width + spacing;
      }
    };
    if (strokeW > 0 && d.stroke) {
      ctx.strokeStyle = rgbaCss(d.stroke);
      ctx.lineWidth = strokeW * 2;
      ctx.lineJoin = "round";
      draw((s, px, py) => ctx.strokeText(s, px, py));
    }
    ctx.fillStyle = rgbaCss(d.color);
    draw((s, px, py) => ctx.fillText(s, px, py));
  });
  const img = ctx.getImageData(0, 0, W, H);
  // getImageData returns straight (unpremultiplied) RGBA, which the engine expects.
  return { width: W, height: H, x, y, rgba: img.data };
}
