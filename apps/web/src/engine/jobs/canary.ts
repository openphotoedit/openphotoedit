// The pixel-parity canary: a golden input per model, reduced to a coarse
// signature (editor-ai `canary.rs`) and compared with the signature the CPU
// backend produced for the same file, recorded in lib/models.ts.
//
// The golden image is a 96×96 PNG crop of testdata/photos/portrait.jpg (a
// public-domain portrait): a real face and figure, so a matte model has
// something to find and a "same output for any input" failure shows. PNG,
// so every browser decodes identical bytes.

import goldenUrl from "./canary-portrait.png?url";
import * as wasm from "../../wasm-gen/editor_wasm.js";
import { MODELS } from "../../lib/models";
import type { JobContext } from "./types";
import type { Made } from "./ort";
import { run, tensor } from "./ort";

/** Relative tolerance on the worst signature cell. */
const TOLERANCE = 0.1;

let golden: Promise<{ rgba: Uint8Array; w: number; h: number }> | null = null;

export function goldenImage(baseUrl: string) {
  golden ??= (async () => {
    const res = await fetch(new URL(goldenUrl, baseUrl));
    const bmp = await createImageBitmap(await res.blob(), { colorSpaceConversion: "none", premultiplyAlpha: "none" });
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const g = c.getContext("2d")!;
    g.drawImage(bmp, 0, 0);
    const d = g.getImageData(0, 0, bmp.width, bmp.height);
    return { rgba: new Uint8Array(d.data.buffer), w: bmp.width, h: bmp.height };
  })();
  return golden;
}

function resized(img: { rgba: Uint8Array; w: number; h: number }, size: number) {
  return wasm.ai_resize_rgba(img.rgba, img.w, img.h, size, size, "bilinear");
}

function planar(rgba: Uint8Array, size: number, map: (v: number, c: number) => number): Float32Array {
  const plane = size * size;
  const out = new Float32Array(plane * 3);
  for (let i = 0; i < plane; i++) for (let c = 0; c < 3; c++) out[c * plane + i] = map(rgba[i * 4 + c], c);
  return out;
}

function planarU8(rgba: Uint8Array, size: number): Uint8Array {
  const plane = size * size;
  const out = new Uint8Array(plane * 3);
  for (let i = 0; i < plane; i++) for (let c = 0; c < 3; c++) out[c * plane + i] = rgba[i * 4 + c];
  return out;
}

interface Probe {
  /** Too slow to run on the CPU as a check (the CPU is the reference). */
  heavy?: boolean;
  feeds(made: Made, img: { rgba: Uint8Array; w: number; h: number }, dims?: Record<string, number>): Record<string, ReturnType<typeof tensor>>;
  /** Output name and how to read it as C×H×W. */
  output: string;
  chw(dims: readonly number[]): [number, number, number];
}

const lastChw = (d: readonly number[]): [number, number, number] => [d[d.length - 3], d[d.length - 2], d[d.length - 1]];

const srProbe: Probe = {
  feeds: (m, img, dims) => {
    const s = dims?.height ?? 128;
    return { input: tensor(m, "float32", planar(resized(img, s), s, (v) => v / 255), [1, 3, s, s]) };
  },
  output: "output",
  chw: lastChw,
};

const PROBES: Record<string, Probe> = {
  migan: {
    feeds: (m, img) => {
      const s = 512;
      const mask = new Uint8Array(s * s).fill(255);
      for (let y = 200; y < 312; y++) for (let x = 200; x < 312; x++) mask[y * s + x] = 0;
      return { image: tensor(m, "uint8", planarU8(resized(img, s), s), [1, 3, s, s]), mask: tensor(m, "uint8", mask, [1, 1, s, s]) };
    },
    output: "result",
    chw: lastChw,
  },
  lama: {
    heavy: true,
    feeds: (m, img) => {
      const s = 512;
      const mask = new Float32Array(s * s);
      for (let y = 200; y < 312; y++) for (let x = 200; x < 312; x++) mask[y * s + x] = 1;
      return { image: tensor(m, "float32", planar(resized(img, s), s, (v) => v / 255), [1, 3, s, s]), mask: tensor(m, "float32", mask, [1, 1, s, s]) };
    },
    output: "output",
    chw: lastChw,
  },
  modnet: {
    feeds: (m, img) => ({ input: tensor(m, "float32", wasm.ai_modnet_input(img.rgba, img.w, img.h), [1, 3, 512, 512]) }),
    output: "output",
    chw: lastChw,
  },
  u2netp: {
    feeds: (m, img) => ({ "input.1": tensor(m, "float32", wasm.ai_u2net_input(img.rgba, img.w, img.h), [1, 3, 320, 320]) }),
    output: "1959",
    chw: lastChw,
  },
  "edgetam-encoder": {
    feeds: (m, img) => ({ pixel_values: tensor(m, "float32", wasm.ai_sam_input(img.rgba, img.w, img.h), [1, 3, 1024, 1024]) }),
    output: "image_embeddings.2",
    chw: lastChw,
  },
  yunet: {
    feeds: (m, img) => ({ input: tensor(m, "float32", wasm.ai_face_detect_input(img.rgba, img.w, img.h), [1, 3, 640, 640]) }),
    output: "cls_32",
    chw: () => [1, 20, 20],
  },
  "realesr-x4v3": srProbe,
  "realesr-x4v3-dn50": srProbe,
  "realesr-x4v3-wdn": srProbe,
  "realesrgan-x4plus": { ...srProbe, heavy: true },
  gfpgan: {
    heavy: true,
    feeds: (m, img) => ({ input: tensor(m, "float32", planar(resized(img, 512), 512, (v) => v / 127.5 - 1), [1, 3, 512, 512]) }),
    output: "output",
    chw: lastChw,
  },
  deoldify: {
    heavy: true,
    feeds: (m, img) => ({ input: tensor(m, "float32", wasm.ai_deoldify_input(img.rgba, img.w, img.h, 256), [1, 3, 256, 256]) }),
    output: "output",
    chw: lastChw,
  },
};

export interface CanaryResult {
  ok: boolean;
  deviation?: number;
  note?: string;
  signature?: number[];
}

/** Run the golden input through `made` and judge it. */
export async function runCanary(ctx: JobContext, made: Made, dims?: Record<string, number>, force = false): Promise<CanaryResult> {
  const probe = PROBES[made.id];
  if (!probe) return { ok: true, note: "no probe for this model" };
  if (probe.heavy && made.backend === "wasm" && !force) return { ok: true, note: "skipped on the CPU reference backend" };
  const img = await goldenImage(ctx.baseUrl);
  const out = await run(made, probe.feeds(made, img, dims));
  const o = out[probe.output];
  if (!o) return { ok: false, note: `no output ${probe.output}` };
  const values = o.data instanceof Float32Array ? o.data : Float32Array.from(o.data);
  if (wasm.ai_degenerate(values)) return { ok: false, note: "output is flat or not finite" };
  const [c, h, w] = probe.chw(o.dims);
  const signature = Array.from(wasm.ai_canary_signature(values, c, h, w), (v) => Math.round(v * 1e4) / 1e4);
  const reference = MODELS[made.id].canary;
  if (!reference) return { ok: true, note: "no reference signature recorded", signature };
  const deviation = wasm.ai_canary_deviation(Float32Array.from(signature), Float32Array.from(reference));
  return { ok: deviation <= TOLERANCE, deviation, signature };
}
