// High-level AI features the shells call. Each runs as a worker job and
// ends in ordinary engine commands, so every AI step is one undo step.
//
// Owned by the AI workstream; until a function is implemented it throws
// `NotReady`, and the shells show the feature as unavailable.

import { editor } from "./editor.svelte";

export class NotReady extends Error {
  constructor(feature: string) {
    super(`${feature} is not available yet`);
    this.name = "NotReady";
  }
}

export type Quality = "fast" | "best";

export interface AiCapabilities {
  webgpu: boolean;
  /** Features whose models are on this device already. */
  cached: string[];
  /** The native local server is connected (enables tier-3 models). */
  native: boolean;
}

export async function capabilities(): Promise<AiCapabilities> {
  return { webgpu: typeof navigator !== "undefined" && "gpu" in navigator, cached: [], native: false };
}

/** Select the main subject (people or the salient object) as a selection. */
export async function selectSubject(): Promise<void> {
  throw new NotReady("Select subject");
}

/** Select the background: the inverse of the subject. */
export async function selectBackground(): Promise<void> {
  throw new NotReady("Select background");
}

/** Click-to-select: positive and negative points in document coordinates. */
export async function selectObjectAt(_points: { x: number; y: number; positive: boolean }[], _mode: "replace" | "add" | "subtract" = "replace"): Promise<void> {
  throw new NotReady("Object selection");
}

/** Remove whatever is selected (or masked by `mask`) and fill it in plausibly. */
export async function removeSelected(_opts: { quality?: Quality } = {}): Promise<void> {
  void editor;
  throw new NotReady("Remove");
}

/** Cut the subject out: new layer with the background made transparent. */
export async function removeBackground(): Promise<void> {
  throw new NotReady("Remove background");
}

/** Blur everything that is not the subject. */
export async function blurBackground(_amount = 12): Promise<void> {
  throw new NotReady("Blur background");
}

export async function upscale(_factor: 2 | 4, _opts: { quality?: Quality } = {}): Promise<void> {
  throw new NotReady("Upscale");
}

export async function denoise(_strength: "low" | "medium" | "high" = "medium"): Promise<void> {
  throw new NotReady("Denoise");
}

export async function removeJpegArtifacts(): Promise<void> {
  throw new NotReady("JPEG clean-up");
}

export async function restoreFaces(): Promise<void> {
  throw new NotReady("Face restoration");
}

export async function colorize(): Promise<void> {
  throw new NotReady("Colorize");
}

export interface DetectedFace {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
  landmarks: { x: number; y: number }[];
}

export async function detectFaces(): Promise<DetectedFace[]> {
  throw new NotReady("Face detection");
}

/** What the image contains, for content-aware quick actions. */
export async function analyzeScene(): Promise<{ faces: number; hasSubject: boolean; hasSky: boolean }> {
  throw new NotReady("Scene analysis");
}

export interface PlannedStep {
  label: string;
  cmd?: Record<string, unknown>;
  /** A high-level AI action instead of a plain command. */
  action?: string;
  params?: Record<string, unknown>;
}

/** Turn a sentence into visible, undoable steps. */
export async function planEdit(_text: string): Promise<{ steps: PlannedStep[]; unsupported?: string }> {
  throw new NotReady("Describe an edit");
}
