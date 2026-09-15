// What Lite's buttons do, as plain functions the panels, the tool search
// and the prompt bar share.

import { editor } from "../lib/editor.svelte";
import * as ai from "../lib/ai";
import { t } from "../lib/i18n";
import { DEVELOP_DEFAULT, type Develop } from "../engine/types";
import { estimateAuto, lookDevelop, lookTint, LOOKS, sanitize, type AutoStyle, type Look } from "./develop";
import { adjustments, ADJUSTMENTS, FRAME, isUnknownOp, lite, look, LOOK, LOOK_TINT } from "./lite.svelte";
import { comingSoon, runFeature } from "./run";
import { decodeThumb, docSignature } from "./previews.svelte";
import { renderText } from "../lib/text";
import type { TextData } from "../engine/types";

// ---------------------------------------------------------------------------
// Crop and size

export interface Ratio {
  id: string;
  label: string;
  /** Width / height; `null` free, `0` the photo's own. */
  value: number | null;
}

export const RATIOS: Ratio[] = [
  { id: "free", label: "Free", value: null },
  { id: "original", label: "Original", value: 0 },
  { id: "1:1", label: "Square", value: 1 },
  { id: "4:5", label: "4:5", value: 4 / 5 },
  { id: "3:2", label: "3:2", value: 3 / 2 },
  { id: "16:9", label: "16:9", value: 16 / 9 },
  { id: "9:16", label: "9:16", value: 9 / 16 },
  { id: "2:3", label: "2:3", value: 2 / 3 },
];

export interface SizePreset {
  id: string;
  label: string;
  detail: string;
  width: number;
  height: number | null;
}

export const RESIZE_PRESETS: SizePreset[] = [
  { id: "instagram", label: "Instagram post", detail: "1080 × 1350", width: 1080, height: 1350 },
  { id: "square", label: "Instagram square", detail: "1080 × 1080", width: 1080, height: 1080 },
  { id: "story", label: "Story or reel", detail: "1080 × 1920", width: 1080, height: 1920 },
  { id: "youtube", label: "YouTube thumbnail", detail: "1280 × 720", width: 1280, height: 720 },
  { id: "web", label: "Web", detail: "2000 px long edge", width: 2000, height: null },
];

function centredCrop(ratio: number) {
  const s = editor.summary!;
  let w = s.width;
  let h = Math.round(w / ratio);
  if (h > s.height) {
    h = s.height;
    w = Math.round(h * ratio);
  }
  return { x: Math.floor((s.width - w) / 2), y: Math.floor((s.height - h) / 2), width: w, height: h };
}

export async function cropToRatio(ratio: number, label?: string) {
  if (!editor.summary) return;
  lite.begin();
  const r = centredCrop(ratio);
  if (r.width === editor.summary.width && r.height === editor.summary.height) return;
  await lite.exec({ op: "image.crop", ...r });
  if (label) lite.note(t("Crop to {ratio}", { ratio: label }));
}

export async function resizeFor(p: SizePreset) {
  const s = editor.summary;
  if (!s) return;
  lite.begin();
  if (p.height) {
    const r = centredCrop(p.width / p.height);
    if (r.width !== s.width || r.height !== s.height) await lite.exec({ op: "image.crop", ...r });
    await lite.exec({ op: "image.resize", width: p.width, height: p.height, resample: "lanczos" });
  } else {
    const long = Math.max(s.width, s.height);
    if (long <= p.width) {
      editor.toast(t("This photo is already {w} × {h}, within {size} px.", { w: s.width, h: s.height, size: p.width }));
      return;
    }
    const k = p.width / long;
    await lite.exec({ op: "image.resize", width: Math.round(s.width * k), height: Math.round(s.height * k), resample: "lanczos" });
  }
  lite.note(t("Resize for {preset}", { preset: t(p.label) }));
}

export async function rotate(turns: number) {
  lite.begin();
  await lite.exec({ op: "image.rotate", turns });
  lite.note(turns === 1 ? t("Rotate right") : t("Rotate left"));
}

export async function flip(horizontal = true) {
  lite.begin();
  await lite.exec({ op: "image.flip", horizontal });
  lite.note(horizontal ? t("Flip") : t("Flip vertically"));
}

/** A straighten applied by the slider: re-dragging replaces it. */
let lastStraighten: { undoLen: number; label: string; degrees: number } | null = null;

export async function straighten(degrees: number) {
  const s = editor.summary;
  if (!s) return;
  lite.begin();
  const undo = s.history.undo;
  if (lastStraighten && undo.length === lastStraighten.undoLen && undo[undo.length - 1] === lastStraighten.label) {
    await lite.exec({ op: "edit.undo" });
  }
  lastStraighten = null;
  if (Math.abs(degrees) < 0.05) return;
  const r = await lite.exec({ op: "image.rotate-arbitrary", degrees, expand: false });
  if (r && editor.summary) {
    lite.note(t("Straighten {deg}°", { deg: degrees.toFixed(1) }));
    const u = editor.summary.history.undo;
    lastStraighten = { undoLen: u.length, label: u[u.length - 1], degrees };
  }
}

/** The angle the slider last applied, while that step is still the newest. */
export function straightenedAngle(): number {
  const s = editor.summary;
  if (!s || !lastStraighten) return 0;
  const u = s.history.undo;
  return u.length === lastStraighten.undoLen && u[u.length - 1] === lastStraighten.label ? lastStraighten.degrees : 0;
}

export async function autoStraighten() {
  const r = await lite.tryExec({ op: "analyze.facts" });
  if (!r.ok) {
    if (isUnknownOp(r.error)) comingSoon("straighten", t("Automatic straightening"));
    else editor.error(r.error);
    return;
  }
  const tilt = Number((r.result.data as { tilt_degrees?: number } | null)?.tilt_degrees ?? 0);
  if (Math.abs(tilt) < 0.2) {
    editor.toast(t("The horizon already looks level."));
    return;
  }
  lastStraighten = null;
  lite.begin();
  await lite.exec({ op: "image.rotate-arbitrary", degrees: -tilt, expand: false });
  lite.note(t("Straighten"));
}

// ---------------------------------------------------------------------------
// Auto

export const AUTO_STYLES: { id: AutoStyle; label: string; detail: string }[] = [
  { id: "auto", label: "Auto", detail: "Balanced" },
  { id: "vivid", label: "Vivid", detail: "More punch" },
  { id: "natural", label: "Natural", detail: "Gentle" },
  { id: "bw", label: "Black and white", detail: "Rich mono" },
];

let autoCache: { sig: string; develops: Partial<Record<AutoStyle, Develop>> } = { sig: "", develops: {} };

/** Develop parameters for an Auto style, measured without Lite's layers. */
export async function autoDevelop(style: AutoStyle): Promise<Develop> {
  const sig = docSignature([ADJUSTMENTS, LOOK, LOOK_TINT]);
  if (autoCache.sig !== sig) autoCache = { sig, develops: {} };
  const cached = autoCache.develops[style];
  if (cached) return cached;
  const hide = lite
    .layers()
    .filter((l) => (l.name === ADJUSTMENTS || l.name === LOOK || l.name === LOOK_TINT) && l.kind === "adjustment" && l.visible)
    .map((l) => ({ op: "layer.props", id: l.id, visible: false }));
  let develop: Develop | null = null;
  if (!lite.unavailable["analyze-auto"]) {
    try {
      const out = await lite.silently(hide, async () => JSON.parse(await editor.engine.call<string>("exec", JSON.stringify({ op: "analyze.auto", style }), new Uint8Array(0))));
      const d = (out?.data as { develop?: Partial<Develop> } | null)?.develop;
      if (d) develop = sanitize({ ...DEVELOP_DEFAULT, ...d });
    } catch (e) {
      if (isUnknownOp(e) || /unknown operation/i.test(String(e))) lite.markUnavailable("analyze-auto");
      else console.warn("analyze.auto failed", e);
    }
  }
  if (!develop) {
    // Measure the photo itself: the bottom pixel layer, before any layers.
    const base = lite.basePixelLayer();
    const bytes = await editor.engine.call<Uint8Array>("thumbnail", base?.id ?? null, 192);
    develop = estimateAuto(decodeThumb(bytes).rgba, style);
  }
  autoCache.develops[style] = develop;
  return develop;
}

/** Auto replaces the tone and colour of "Adjustments", keeping effects. */
export function autoAdjustment(d: Develop): Develop {
  const cur = adjustments.stored;
  return { ...d, vignette: cur.vignette, grain: cur.grain };
}

export async function applyAuto(style: AutoStyle) {
  if (!editor.hasDocument) return;
  const d = await lite.lock.run(() => autoDevelop(style));
  await adjustments.replace(autoAdjustment(d));
  const label = AUTO_STYLES.find((s) => s.id === style)?.label ?? "Auto";
  lite.note(t("Auto: {style}", { style: t(label) }));
}

// ---------------------------------------------------------------------------
// Looks

export function currentTintLayer() {
  return lite.findAdjustment(LOOK_TINT, "color-balance");
}

export async function applyLook(l: Look | null, strength = 1) {
  if (!editor.hasDocument) return;
  lite.begin();
  if (!l) {
    const tint = currentTintLayer();
    const ids = [look.layer?.id, tint?.id].filter((x): x is number => x != null);
    if (ids.length) {
      await lite.exec({ op: "layer.delete", ids });
      lite.note(t("Remove look"));
    }
    return;
  }
  await look.replace(lookDevelop(l, strength));
  const tint = currentTintLayer();
  if (l.tint) {
    const adjustment = lookTint(l, strength);
    if (tint) await lite.exec({ op: "layer.set-adjustment", id: tint.id, adjustment });
    else await lite.exec({ op: "layer.add-adjustment", adjustment, name: LOOK_TINT, above: lite.anchorFor(LOOK_TINT) });
    await lite.exec({ op: "edit.seal" });
  } else if (tint) {
    await lite.exec({ op: "layer.delete", ids: [tint.id] });
  }
  lite.note(t("Look: {name}", { name: t(l.name) }));
}

/** Commands that turn the document into a Look preview. */
export function lookPreviewCommands(l: Look | null, strength = 1): Record<string, unknown>[] {
  const cmds: Record<string, unknown>[] = [];
  const cur = look.layer;
  const tint = currentTintLayer();
  if (!l) {
    for (const x of [cur, tint]) if (x) cmds.push({ op: "layer.props", id: x.id, visible: false });
    return cmds;
  }
  const adjustment = { kind: "develop", ...lookDevelop(l, strength) };
  if (cur) cmds.push({ op: "layer.set-adjustment", id: cur.id, adjustment }, { op: "edit.seal" });
  else cmds.push({ op: "layer.add-adjustment", adjustment, name: LOOK, above: lite.anchorFor(LOOK) });
  if (l.tint) {
    const ta = lookTint(l, strength);
    if (tint) cmds.push({ op: "layer.set-adjustment", id: tint.id, adjustment: ta });
    else cmds.push({ op: "layer.add-adjustment", adjustment: ta, name: LOOK_TINT, above: cur?.id ?? null });
  } else if (tint) {
    cmds.push({ op: "layer.props", id: tint.id, visible: false });
  }
  return cmds;
}

export function lookById(id: string) {
  return LOOKS.find((l) => l.id === id) ?? null;
}

// ---------------------------------------------------------------------------
// Frames

export async function addBorder(fraction: number, hex: string) {
  const s = editor.summary;
  if (!s) return;
  lite.begin();
  const b = Math.max(4, Math.round(Math.min(s.width, s.height) * fraction));
  const color = hexRgb(hex);
  await lite.exec({ op: "image.canvas-size", width: s.width + b * 2, height: s.height + b * 2, anchor: "center" });
  const r = await lite.exec({ op: "layer.add-fill", fill: { kind: "solid", color }, name: FRAME });
  const id = r?.data?.id as number | undefined;
  if (id != null) await lite.exec({ op: "layer.move", id, index: 0 });
  lite.note(t("Add border"));
}

export async function addCaption(text: string, hex: string) {
  const s = editor.summary;
  if (!s) return;
  lite.begin();
  const bar = Math.max(40, Math.round(Math.min(s.width, s.height) * 0.14));
  const color = hexRgb(hex);
  await lite.exec({ op: "image.canvas-size", width: s.width, height: s.height + bar, anchor: "top" });
  const r = await lite.exec({ op: "layer.add-fill", fill: { kind: "solid", color }, name: FRAME });
  const id = r?.data?.id as number | undefined;
  if (id != null) await lite.exec({ op: "layer.move", id, index: 0 });
  const trimmed = text.trim();
  if (trimmed) {
    const light = color.r * 0.299 + color.g * 0.587 + color.b * 0.114 > 150;
    const data: TextData = {
      text: trimmed,
      font_family: "Geist, 'Helvetica Neue', Arial, sans-serif",
      font_size: Math.round(bar * 0.34),
      font_weight: 400,
      italic: false,
      color: light ? { r: 17, g: 17, b: 17, a: 255 } : { r: 255, g: 255, b: 255, a: 255 },
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
    const probe = await renderText(data);
    const tw = probe.width - (probe.x - data.x) * 2;
    const th = probe.height - (probe.y - data.y) * 2;
    data.x = Math.round((s.width - tw) / 2);
    data.y = Math.round(s.height + (bar - th) / 2);
    const img = await renderText(data);
    await lite.exec({ op: "layer.add-text", data, width: img.width, height: img.height, x: img.x, y: img.y, above: lite.topId() }, img.rgba);
  }
  lite.note(t("Add caption"));
}

function hexRgb(hex: string) {
  const n = parseInt(hex.replace("#", ""), 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 255 };
}

// ---------------------------------------------------------------------------
// AI

export const selectSubject = () => runFeature("subject", t("Select subject"), () => ai.selectSubject());
export const removeBackground = () => runFeature("remove-bg", t("Remove background"), () => ai.removeBackground());
export const blurBackground = (amount = 12) => runFeature("blur-bg", t("Blur background"), () => ai.blurBackground(amount));
export const upscale = (factor: 2 | 4, quality: ai.Quality) => runFeature(`upscale-${factor}`, t("Upscale"), () => ai.upscale(factor, { quality }));
export const denoise = (strength: "low" | "medium" | "high" = "medium") => runFeature("denoise", t("Denoise"), () => ai.denoise(strength));
export const cleanJpeg = () => runFeature("jpeg", t("Clean up JPEG"), () => ai.removeJpegArtifacts());
export const restoreFaces = () => runFeature("faces", t("Restore faces"), () => ai.restoreFaces());
export const colorize = () => runFeature("colorize", t("Colorize"), () => ai.colorize());

/** Brighten whatever is selected (the subject) with its own masked layer. */
export async function brightenSubject() {
  if (!editor.summary?.selection) {
    const ok = await selectSubject();
    if (!ok || !editor.summary?.selection) return;
  }
  lite.begin();
  const adjustment = { kind: "develop", ...DEVELOP_DEFAULT, exposure: 0.35, shadows: 25, vibrance: 10 };
  await lite.exec({ op: "layer.add-adjustment", adjustment, name: t("Brighten subject") });
  await lite.exec({ op: "select.none" });
  lite.note(t("Brighten subject"));
}
