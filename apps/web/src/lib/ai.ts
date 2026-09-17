// High-level AI features the shells call. Each runs as a worker job
// (apps/web/src/engine/jobs/ai.ts) and ends in ordinary engine commands with
// `provenance: "ai:<model>"`, so every AI step is visible and undoable.
//
// Everything runs on this device: models and the ONNX runtime come from this
// origin (or the native local server) and are cached after first use.
//
// Errors are thrown with a plain sentence (the shells toast them). While a
// job runs, `editor.busy` carries its label, progress and a cancel button.

import { editor } from "./editor.svelte";

export class NotReady extends Error {
  constructor(feature: string) {
    super(`${feature} is not available yet`);
    this.name = "NotReady";
  }
}

/** `auto` (remove only): the fast model for small areas, the best one for large. */
export type Quality = "fast" | "best" | "auto";

export interface AiCapabilities {
  webgpu: boolean;
  /** Models (ids from lib/models.ts) whose files are on this device already. */
  cached: string[];
  /** The native local server is connected (enables tier-3 models). */
  native: boolean;
  /** CPU threads onnxruntime uses (needs cross-origin isolation for >1). */
  threads?: number;
  /** Per model and backend: did the pixel-parity canary pass on this device. */
  verdicts?: Record<string, { ok: boolean; deviation?: number; at: string }>;
}

/** Run an AI job with the busy indicator; throws on failure or cancel. */
async function job<T = Record<string, unknown>>(label: string, name: string, params: Record<string, unknown> = {}): Promise<T> {
  if (!editor.ready) throw new Error("The editor is still starting. Try again in a moment.");
  const handle = editor.engine.job<T>(name, params, {
    onProgress: (p) => {
      if (editor.busy) editor.busy = { ...editor.busy, label: p.message ?? editor.busy.label, fraction: p.fraction };
    },
  });
  const previous = editor.busy;
  editor.busy = { label, cancel: handle.cancel };
  try {
    const r = await handle.promise;
    if (r.changed) editor.dirty = true;
    return r.result;
  } finally {
    editor.busy = previous;
  }
}

function needDocument() {
  if (!editor.hasDocument && !(editor.summary && editor.summary.width > 1)) throw new Error("Open a photo first.");
}

let nativeProbe: Promise<boolean> | null = null;

/** Whether the native local server (`openphotoshop` binary) is serving this app. */
export function nativeServer(): Promise<boolean> {
  nativeProbe ??= fetch(new URL("api/health", document.baseURI), { cache: "no-store" })
    .then(async (r) => (r.ok ? !!(await r.json()).native : false))
    .catch(() => false);
  return nativeProbe;
}

export async function capabilities(): Promise<AiCapabilities> {
  const native = await nativeServer();
  if (!editor.ready) return { webgpu: typeof navigator !== "undefined" && "gpu" in navigator, cached: [], native };
  const r = await editor.engine.job<{ webgpu: boolean; threads: number; cached: string[]; verdicts: AiCapabilities["verdicts"] }>("ai.capabilities").promise;
  return { ...r.result, native };
}

/** Select the main subject (people or the salient object) as a selection. */
export async function selectSubject(opts: { mode?: "replace" | "add" | "subtract" | "intersect"; model?: "auto" | "portrait" | "object" } = {}): Promise<void> {
  needDocument();
  await job("Selecting the subject", "ai.select-subject", { ...opts });
}

/** Select the background: the inverse of the subject. */
export async function selectBackground(opts: { mode?: "replace" | "add" | "subtract" | "intersect"; model?: "auto" | "portrait" | "object" } = {}): Promise<void> {
  needDocument();
  await job("Selecting the background", "ai.select-subject", { ...opts, background: true });
}

/**
 * Click-to-select: positive and negative points in document coordinates.
 * The first click on a photo reads it once (about a second on the CPU);
 * later clicks on the same pixels are fast.
 */
export async function selectObjectAt(points: { x: number; y: number; positive: boolean }[], mode: "replace" | "add" | "subtract" = "replace"): Promise<void> {
  needDocument();
  await job("Selecting the object", "ai.select-object", { points, mode });
}

/**
 * Remove whatever is selected and fill it in plausibly, on the active pixel
 * layer (or a new "Remove" layer when the active layer has no pixels).
 * `fast` = MI-GAN (28 MB); `best` = LaMa (208 MB, slower, better on large areas);
 * `auto` (default) = MI-GAN when the hole is small, LaMa when it is large.
 * The fill extends a few pixels past the selection's edge on purpose.
 */
export async function removeSelected(opts: { quality?: Quality } = {}): Promise<void> {
  needDocument();
  if (!editor.summary?.selection) throw new Error("Select what to remove first.");
  await job("Removing", "ai.remove", { quality: opts.quality ?? "auto" });
}

/** Cut the subject out: adds a layer mask from the subject to the active pixel layer. */
export async function removeBackground(): Promise<void> {
  needDocument();
  await job("Removing the background", "ai.remove-background");
}

/**
 * Blur everything that is not the subject: a new "Background blur" layer,
 * masked to the background. `amount` 1..100 (12 ≈ a portrait-lens look).
 */
export async function blurBackground(amount = 12): Promise<void> {
  needDocument();
  await job("Blurring the background", "ai.blur-background", { amount });
}

/** Upscale the document; the active pixel layer gets the AI result (others are resampled). */
export async function upscale(factor: 2 | 4, opts: { quality?: Quality } = {}): Promise<void> {
  needDocument();
  await job(`Upscaling ${factor}×`, "ai.upscale", { factor, quality: opts.quality ?? "fast" });
}

/** Reduce noise on the active pixel layer (within the selection, if any). */
export async function denoise(strength: "low" | "medium" | "high" = "medium"): Promise<void> {
  needDocument();
  await job("Reducing noise", "ai.enhance", { kind: "denoise", strength });
}

/** Clean JPEG blocking and ringing on the active pixel layer (Real-ESRGAN at 1×). */
export async function removeJpegArtifacts(): Promise<void> {
  needDocument();
  await job("Cleaning up JPEG artifacts", "ai.enhance", { kind: "jpeg" });
}

/** Restore every detected face on the active pixel layer. Returns how many faces. */
export async function restoreFaces(opts: { strength?: number } = {}): Promise<number> {
  needDocument();
  const r = await job<{ faces: number }>("Restoring faces", "ai.restore-faces", { ...opts });
  if (!r.faces) throw new Error("No faces were found in this photo.");
  return r.faces;
}

/** Add colour to a black-and-white active pixel layer. */
export async function colorize(opts: { saturation?: number } = {}): Promise<void> {
  needDocument();
  await job("Adding colour", "ai.colorize", { ...opts });
}

export interface DetectedFace {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
  /** Right eye, left eye, nose, right and left mouth corner (person's right = smaller x). */
  landmarks: { x: number; y: number }[];
}

export async function detectFaces(): Promise<DetectedFace[]> {
  needDocument();
  const r = await editor.engine.job<{ score: number; bbox: number[]; landmarks: number[][] }[]>("ai.detect-faces").promise;
  return r.result.map((f) => ({ x: f.bbox[0], y: f.bbox[1], w: f.bbox[2], h: f.bbox[3], score: f.score, landmarks: f.landmarks.map(([x, y]) => ({ x, y })) }));
}

/** What the image contains, for content-aware quick actions. */
export async function analyzeScene(): Promise<{ faces: number; hasSubject: boolean; hasSky: boolean; monochrome?: boolean }> {
  needDocument();
  const r = await editor.engine.job<{ faces: number; hasSubject: boolean; hasSky: boolean; monochrome: boolean }>("ai.analyze-scene").promise;
  return r.result;
}

export interface PlannedStep {
  label: string;
  cmd?: Record<string, unknown>;
  /** A high-level AI action instead of a plain command. */
  action?: string;
  params?: Record<string, unknown>;
}

/** Turn a sentence into visible, undoable steps. Nothing is applied. */
export async function planEdit(text: string): Promise<{ steps: PlannedStep[]; unsupported?: string; ignored?: string[] }> {
  if (!editor.ready) throw new NotReady("Describe an edit");
  const r = await editor.engine.job<{ steps: PlannedStep[]; unsupported?: string; ignored?: string[] }>("ai.plan", { text }).promise;
  return r.result;
}

/** Remove red eye on every detected face (needs the filters' `filter.red-eye`). */
export async function removeRedEye(): Promise<void> {
  needDocument();
  await job("Removing red eye", "ai.red-eye");
}

/** Level the horizon from `analyze.facts`. */
export async function straighten(): Promise<void> {
  needDocument();
  const facts = await editor.engine.exec({ op: "analyze.facts" });
  const tilt = Number((facts.data as { tilt_degrees?: number } | null)?.tilt_degrees ?? 0);
  if (Math.abs(tilt) < 0.1) return;
  await editor.exec({ op: "image.rotate-arbitrary", degrees: -tilt, expand: false });
}

/** Centre crop to an aspect ratio (width / height). */
export async function cropToAspect(aspect: number): Promise<void> {
  needDocument();
  const s = editor.summary!;
  let w = s.width;
  let h = Math.round(w / aspect);
  if (h > s.height) {
    h = s.height;
    w = Math.round(h * aspect);
  }
  await editor.exec({ op: "image.crop", x: Math.floor((s.width - w) / 2), y: Math.floor((s.height - h) / 2), width: w, height: h });
}

/** One-click auto tone as a Develop adjustment layer. */
export async function autoEnhance(): Promise<void> {
  needDocument();
  const r = await editor.engine.exec({ op: "analyze.auto", style: "auto" });
  const develop = (r.data as { develop?: Record<string, unknown> } | null)?.develop;
  if (!develop) throw new Error("Auto enhance did not return an adjustment.");
  await editor.exec({ op: "layer.add-adjustment", adjustment: { kind: "develop", ...develop }, name: "Auto" });
}

/** Execute a plan from `planEdit`, one undo step (or more) per step. */
export async function runPlan(steps: PlannedStep[]): Promise<void> {
  for (const step of steps) {
    if (step.cmd) {
      const r = await editor.engine.exec(step.cmd);
      if (r.changed) editor.dirty = true;
      continue;
    }
    const p = step.params ?? {};
    switch (step.action) {
      case "removeBackground": await removeBackground(); break;
      case "blurBackground": await blurBackground(Number(p.amount ?? 12)); break;
      case "selectSubject": await selectSubject(); break;
      case "selectBackground": await selectBackground(); break;
      case "removeSelected": await removeSelected(); break;
      case "upscale": await upscale(Number(p.factor) === 4 ? 4 : 2); break;
      case "restoreFaces": await restoreFaces(); break;
      case "colorize": await colorize(); break;
      case "removeJpegArtifacts": await removeJpegArtifacts(); break;
      case "denoise": await denoise((p.strength as "low" | "medium" | "high") ?? "medium"); break;
      case "removeRedEye": await removeRedEye(); break;
      case "straighten": await straighten(); break;
      case "crop": await cropToAspect(Number(p.aspect ?? 1)); break;
      case "auto": await autoEnhance(); break;
      default: throw new Error(`Unknown step “${step.label}”.`);
    }
  }
}

export interface ModelInfo {
  id: string;
  label: string;
  feature: string;
  bytes: number;
  licence: string;
  attribution: string;
  source: string;
  tier: 1 | 2;
  greyZone: string | null;
  cached: boolean;
}

/** The model catalogue with what is on this device (for a Models page). */
export async function models(): Promise<ModelInfo[]> {
  return (await editor.engine.job<ModelInfo[]>("ai.models").promise).result;
}

/** Delete one model (or all) from this device's cache. */
export async function forgetModel(id?: string): Promise<void> {
  await editor.engine.job("ai.forget", id ? { id } : {}).promise;
}

/** Use the CPU even where WebGPU exists (a setting, or for diagnostics). */
export async function preferCpu(on: boolean): Promise<void> {
  await editor.engine.job("ai.configure", { preferCpu: on }).promise;
}

/** Diagnostics: run each model's golden input on a backend and report. */
export async function runCanaries(backend: "wasm" | "webgpu" = "wasm", ids?: string[]) {
  return (await editor.engine.job<Record<string, { ok: boolean; deviation?: number; note?: string; signature?: number[]; backend?: string; loadMs?: number; runMs?: number }>>("ai.canary", { backend, ids }).promise).result;
}
