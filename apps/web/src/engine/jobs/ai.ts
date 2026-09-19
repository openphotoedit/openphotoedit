// AI jobs. Each reads pixels from the engine, runs a model through
// jobs/ort.ts with all pixel maths in Rust (editor-ai via wasm `ai_*`), and
// ends in ordinary commands with `provenance: "ai:<model>"`, so every AI step
// is visible in history and undoable.

import * as wasm from "../../wasm-gen/editor_wasm.js";
import { MODELS, cachedModels, forgetModel, setModelBase, modelBytes } from "../../lib/models";
import type { LayerInfo, Summary } from "../types";
import { register } from "./index";
import type { JobContext, JobOutput } from "./types";
import { allVerdicts, getSession, releaseAll, run, runtimeInfo, settings, tensor, type Made } from "./ort";
import { runCanary } from "./canary";

type Engine = JobContext["engine"];

// ---------------------------------------------------------------------------
// Helpers

function summary(engine: Engine): Summary {
  return JSON.parse(engine.summary());
}

/** Warnings the engine attached to this job's commands (exec results carry `warnings`). */
let warnings: string[] = [];

function exec(engine: Engine, cmd: Record<string, unknown>, bytes?: Uint8Array) {
  const r = JSON.parse(engine.exec(JSON.stringify(cmd), bytes ?? new Uint8Array(0))) as { changed: boolean; data?: Record<string, unknown> | null; warnings?: string[] };
  if (r.warnings?.length) warnings.push(...r.warnings);
  return r;
}

/**
 * Run several commands as ONE undo step (engine `edit.begin`/`edit.end`). If
 * `fn` throws, the document is restored (`edit.cancel`) and the error rethrown.
 */
function oneStep(engine: Engine, label: string, fn: () => void) {
  exec(engine, { op: "edit.begin", label });
  try {
    fn();
  } catch (e) {
    exec(engine, { op: "edit.cancel" });
    throw e;
  }
  exec(engine, { op: "edit.end" });
}

function find(layers: LayerInfo[], id: number | null): LayerInfo | null {
  for (const l of layers) {
    if (l.id === id) return l;
    if (l.children) {
      const f = find(l.children, id);
      if (f) return f;
    }
  }
  return null;
}

function activePixelLayer(s: Summary): LayerInfo | null {
  const l = find(s.layers, s.active);
  return l && l.kind === "pixel" ? l : null;
}

/** A key that changes when any pixel could have (not when the selection does). */
function contentKey(s: Summary): string {
  const walk = (ls: LayerInfo[]): string => ls.map((l) => `${l.id}:${l.rev}:${l.visible ? 1 : 0}:${l.opacity}:${l.blend}${l.children ? `[${walk(l.children)}]` : ""}`).join(",");
  return `${s.width}x${s.height}|${walk(s.layers)}`;
}

function mode(p: Record<string, unknown>): string {
  const m = p.mode;
  return m === "add" || m === "subtract" || m === "intersect" ? m : "replace";
}

function interleavedToPlanar(rgb: Uint8Array, size: number, scale: number): Float32Array {
  const plane = size * size;
  const out = new Float32Array(plane * 3);
  for (let i = 0; i < plane; i++) {
    out[i] = rgb[i * 3] * scale;
    out[plane + i] = rgb[i * 3 + 1] * scale;
    out[2 * plane + i] = rgb[i * 3 + 2] * scale;
  }
  return out;
}

function planarToInterleaved(chw: Float32Array | Uint8Array, size: number, scale: number): Uint8Array {
  const plane = size * size;
  const out = new Uint8Array(plane * 3);
  for (let i = 0; i < plane; i++) for (let c = 0; c < 3; c++) out[i * 3 + c] = Math.max(0, Math.min(255, Math.round(chw[c * plane + i] * scale)));
  return out;
}

const yieldNow = () => new Promise((r) => setTimeout(r, 0));

function timer() {
  const t0 = performance.now();
  const marks: Record<string, number> = {};
  let last = t0;
  return {
    mark(name: string) {
      const now = performance.now();
      marks[name] = Math.round(now - last);
      last = now;
    },
    done() {
      marks.total = Math.round(performance.now() - t0);
      return marks;
    },
  };
}

async function begin(ctx: JobContext) {
  setModelBase(ctx.baseUrl);
  warnings = [];
}

// ---------------------------------------------------------------------------
// Faces

export interface Face {
  score: number;
  bbox: [number, number, number, number];
  landmarks: [number, number][];
}

async function detectFaces(ctx: JobContext, rgba: Uint8Array, w: number, h: number): Promise<Face[]> {
  const made = await getSession(ctx, "yunet");
  const input = wasm.ai_face_detect_input(rgba, w, h);
  const o = await run(made, { input: tensor(made, "float32", input, [1, 3, 640, 640]) });
  const g = (n: string) => o[n].data as Float32Array;
  const json = wasm.ai_face_detect_decode(wasm.ai_face_detect_scale(w, h), g("cls_8"), g("obj_8"), g("bbox_8"), g("kps_8"), g("cls_16"), g("obj_16"), g("bbox_16"), g("kps_16"), g("cls_32"), g("obj_32"), g("bbox_32"), g("kps_32"));
  return JSON.parse(json);
}

// ---------------------------------------------------------------------------
// Subject matte

const matteCache = new Map<string, { matte: Uint8Array; model: string }>();

/**
 * The subject as an 8-bit document matte.
 *
 * - A portrait (a face at least a tenth of the frame tall, at most three
 *   faces): MODNet, refined with a guided filter. Best on hair.
 * - Anything else: a rough matte (MODNet when there are people, U²-Net-p
 *   otherwise) only *locates* the subject; each main blob becomes a box
 *   prompt for EdgeTAM, which returns whole, crisp objects. Measured on a
 *   street scene: MODNet alone left holes through every walker; boxes from
 *   it gave complete people.
 */
async function subjectMatte(ctx: JobContext, which: string): Promise<{ matte: Uint8Array; model: string; w: number; h: number; faces: number; boxes?: number[] }> {
  const { engine } = ctx;
  const s = summary(engine);
  const { width: w, height: h } = s;
  const key = `${contentKey(s)}|${which}`;
  const hit = matteCache.get(key);
  if (hit) return { ...hit, w, h, faces: -1 };
  const full = engine.region(0, 0, w, h);
  let faces = -1;
  let kind = which;
  if (kind === "auto") {
    ctx.progress({ message: "Looking for people" });
    const found = (await detectFaces(ctx, full, w, h)).filter((f) => f.bbox[3] > h * 0.03);
    faces = found.length;
    const biggest = Math.max(0, ...found.map((f) => f.bbox[3]));
    kind = found.length && found.length <= 3 && biggest >= h * 0.1 ? "portrait" : found.length ? "people" : "object";
  }
  ctx.checkCancelled();
  let matte: Uint8Array;
  let model: string;
  ctx.progress({ message: "Finding the subject" });
  if (kind === "portrait" || kind === "people") {
    const made = await getSession(ctx, "modnet");
    const o = await run(made, { input: tensor(made, "float32", wasm.ai_modnet_input(full, w, h), [1, 3, 512, 512]) });
    matte = wasm.ai_matte_finish(o.output.data as Float32Array, "modnet", full, w, h, kind === "portrait");
    model = "modnet";
  } else {
    const made = await getSession(ctx, "u2netp");
    const o = await run(made, { "input.1": tensor(made, "float32", wasm.ai_u2net_input(full, w, h), [1, 3, 320, 320]) });
    matte = wasm.ai_matte_finish(o[made.session.outputNames[0]].data as Float32Array, "u2net", full, w, h, false);
    model = "u2netp";
  }
  let boxList: number[] | undefined;
  if (kind !== "portrait") {
    const boxes = wasm.ai_subject_boxes(matte, w, h, 6);
    boxList = Array.from(boxes, Math.round);
    if (boxes.length) {
      ctx.checkCancelled();
      const refined = await samFromBoxes(ctx, s, boxes);
      if (refined && wasm.ai_matte_coverage(refined) > 0.002) {
        matte = refined;
        model += "+edgetam";
      }
    }
  }
  if (matteCache.size > 4) matteCache.clear();
  matteCache.set(key, { matte, model });
  return { matte, model, w, h, faces, boxes: boxList };
}

/** Union of EdgeTAM masks, one box prompt per box. */
async function samFromBoxes(ctx: JobContext, s: Summary, boxes: Float32Array): Promise<Uint8Array | null> {
  const cache = await samEmbeddings(ctx, s);
  const dec = await getSession(ctx, "edgetam-decoder");
  const union = new Uint8Array(s.width * s.height);
  for (let b = 0; b < boxes.length / 4; b++) {
    const corners = wasm.ai_sam_points(boxes.subarray(b * 4, b * 4 + 4), s.width, s.height);
    const o = await run(dec, { ...decoderEmbeddings(dec, cache), input_points: tensor(dec, "float32", new Float32Array(0), [1, 1, 0, 2]), input_labels: tensor(dec, "int64", new BigInt64Array(0), [1, 1, 0]), input_boxes: tensor(dec, "float32", corners, [1, 1, 4]) });
    const iou = o.iou_scores.data as Float32Array;
    const index = wasm.ai_sam_choose(o.pred_masks.data as Float32Array, iou, false);
    const m = wasm.ai_sam_mask(o.pred_masks.data as Float32Array, index, s.width, s.height, cache.guide, cache.gw, cache.gh);
    for (let i = 0; i < m.length; i++) if (m[i] > union[i]) union[i] = m[i];
  }
  return union;
}

function decoderEmbeddings(dec: Made, cache: NonNullable<typeof samCache>) {
  const e = cache.emb;
  const t = (n: string) => tensor(dec, "float32", e[n].data, [...e[n].dims]);
  return { "image_embeddings.0": t("image_embeddings.0"), "image_embeddings.1": t("image_embeddings.1"), "image_embeddings.2": t("image_embeddings.2") };
}

function invert(m: Uint8Array): Uint8Array {
  const out = new Uint8Array(m.length);
  for (let i = 0; i < m.length; i++) out[i] = 255 - m[i];
  return out;
}

// ---------------------------------------------------------------------------
// SAM encoder cache (per document content)

let samCache: { key: string; emb: Record<string, { data: Float32Array; dims: readonly number[] }>; guide: Uint8Array; gw: number; gh: number } | null = null;

async function samEmbeddings(ctx: JobContext, s: Summary) {
  const key = contentKey(s);
  if (samCache?.key === key) return samCache;
  const { width: w, height: h } = s;
  const full = ctx.engine.region(0, 0, w, h);
  const enc = await getSession(ctx, "edgetam-encoder");
  ctx.progress({ message: "Reading the photo" });
  const out = await run(enc, { pixel_values: tensor(enc, "float32", wasm.ai_sam_input(full, w, h), [1, 3, 1024, 1024]) });
  const scale = Math.min(1, 1600 / Math.max(w, h));
  const gw = Math.max(1, Math.round(w * scale));
  const gh = Math.max(1, Math.round(h * scale));
  const guide = scale < 1 ? wasm.ai_resize_rgba(full, w, h, gw, gh, "bilinear") : full;
  samCache = { key, emb: out as Record<string, { data: Float32Array; dims: readonly number[] }>, guide, gw, gh };
  return samCache;
}

// ---------------------------------------------------------------------------
// Tiled super-resolution (upscale, denoise, JPEG clean-up)

async function superResolve(ctx: JobContext, model: string, rgba: Uint8Array, w: number, h: number, outScale: number): Promise<{ rgba: Uint8Array; w: number; h: number; backend: string }> {
  // Decide the tile before the session: the pad square is pinned into it.
  const info = await runtimeInfo(ctx.baseUrl);
  const gpu = info.webgpu && !settings.preferCpu;
  const tile = model === "realesrgan-x4plus" ? (gpu ? 192 : 128) : gpu ? 256 : 192;
  const overlap = 16;
  const pad = tile + 2 * overlap;
  const made = await getSession(ctx, model, { dims: { batch: 1, height: pad, width: pad } });
  const up = new wasm.AiUpscaler(rgba, w, h, tile, overlap, 4, outScale);
  try {
    const n = up.count();
    for (let i = 0; i < n; i++) {
      ctx.checkCancelled();
      const o = await run(made, { input: tensor(made, "float32", up.input(i), [1, 3, pad, pad]) });
      up.add(i, o.output.data as Float32Array);
      ctx.progress({ stage: "tiles", fraction: (i + 1) / n, message: `Enhancing (${i + 1} of ${n})` });
      await yieldNow();
    }
    return { rgba: up.finish(), w: up.out_width(), h: up.out_height(), backend: made.backend };
  } finally {
    up.free();
  }
}

/** Cap on upscale output, in pixels: a browser tab cannot hold much more. */
export const MAX_OUTPUT_PIXELS = 40_000_000;

// ---------------------------------------------------------------------------
// Jobs

const handlers: Record<string, (ctx: JobContext) => Promise<JobOutput>> = {
  "ai.capabilities": async (ctx) => {
    await begin(ctx);
    const info = await runtimeInfo(ctx.baseUrl);
    return { result: { ...info, cached: await cachedModels(), verdicts: await allVerdicts() } };
  },

  "ai.models": async (ctx) => {
    await begin(ctx);
    const cached = new Set(await cachedModels());
    return { result: Object.values(MODELS).map((m) => ({ id: m.id, label: m.label, feature: m.feature, bytes: modelBytes(m.id), licence: m.licence, attribution: m.attribution, source: m.source, tier: m.tier, greyZone: m.greyZone ?? null, cached: cached.has(m.id) })) };
  },

  "ai.forget": async (ctx) => {
    await begin(ctx);
    await releaseAll();
    await forgetModel(typeof ctx.params.id === "string" ? ctx.params.id : undefined);
    return { result: await cachedModels() };
  },

  "ai.configure": async (ctx) => {
    if (typeof ctx.params.preferCpu === "boolean") settings.preferCpu = ctx.params.preferCpu;
    if (typeof ctx.params.ignoreKnownBad === "boolean") settings.ignoreKnownBad = ctx.params.ignoreKnownBad;
    await releaseAll();
    return { result: { ...settings } };
  },

  /** Diagnostics: run every model's canary on a forced backend. */
  "ai.canary": async (ctx) => {
    await begin(ctx);
    const ids = Array.isArray(ctx.params.ids) ? (ctx.params.ids as string[]) : Object.keys(MODELS);
    const backend = ctx.params.backend === "webgpu" ? "webgpu" : "wasm";
    const out: Record<string, unknown> = {};
    for (const id of ids) {
      ctx.checkCancelled();
      const dims = id.startsWith("realesr") ? { batch: 1, height: 224, width: 224 } : undefined;
      try {
        const t0 = performance.now();
        const made = await getSession(ctx, id, { backend, dims });
        const loadMs = Math.round(performance.now() - t0);
        const t1 = performance.now();
        const res = await runCanary(ctx, made, dims, true);
        out[id] = { ...res, backend: made.backend, loadMs, runMs: Math.round(performance.now() - t1) };
      } catch (e) {
        out[id] = { ok: false, note: String((e as Error)?.message ?? e) };
      }
      ctx.progress({ message: `Checked ${id}` });
    }
    return { result: out };
  },

  "ai.select-subject": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const background = !!ctx.params.background;
    const { matte, model, w, h, faces, boxes } = await subjectMatte(ctx, String(ctx.params.model ?? "auto"));
    t.mark("matte");
    const bytes = background ? invert(matte) : matte;
    exec(ctx.engine, { op: "select.mask", x: 0, y: 0, width: w, height: h, mode: mode(ctx.params), label: background ? "Select Background" : "Select Subject" }, bytes);
    return { changed: true, result: { model, faces, boxes, coverage: wasm.ai_matte_coverage(matte), timings: t.done() } };
  },

  "ai.remove-background": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const s = summary(ctx.engine);
    const layer = activePixelLayer(s);
    if (!layer) throw new Error("Select a pixel layer to remove its background.");
    const { matte, model, w, h } = await subjectMatte(ctx, String(ctx.params.model ?? "auto"));
    t.mark("matte");
    // Combine with a mask the layer already has (it only ever hides more),
    // and limit the removal to the selection when there is one.
    const existing = layer.mask ? ctx.engine.mask_region(layer.id, 0, 0, w, h) : new Uint8Array(0);
    const selection = s.selection ? ctx.engine.selection_region(0, 0, w, h) : new Uint8Array(0);
    const combined = wasm.ai_combine_masks(matte, existing, selection);
    oneStep(ctx.engine, "Remove Background", () => {
      if (!layer.mask) exec(ctx.engine, { op: "layer.add-mask", id: layer.id, from: "reveal-all" });
      exec(ctx.engine, { op: "layer.set-mask-pixels", id: layer.id, x: 0, y: 0, width: w, height: h }, combined);
    });
    return { changed: true, result: { model, layer: layer.id, combined: !!layer.mask, inSelection: !!s.selection, timings: t.done() } };
  },

  "ai.blur-background": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const s = summary(ctx.engine);
    const { width: w, height: h } = s;
    const amount = Math.max(1, Math.min(100, Number(ctx.params.amount ?? 12)));
    const { matte, model } = await subjectMatte(ctx, String(ctx.params.model ?? "auto"));
    t.mark("matte");
    const layer = activePixelLayer(s);
    const src = layer ? ctx.engine.layer_region(layer.id, 0, 0, w, h) : ctx.engine.region(0, 0, w, h);
    const bg = invert(matte);
    const sigma = (Math.max(w, h) * amount) / 1000;
    ctx.progress({ message: "Blurring the background" });
    const blurred = wasm.ai_background_blur(src, w, h, bg, sigma);
    t.mark("blur");
    let id = 0;
    oneStep(ctx.engine, "Blur Background", () => {
      const r = exec(ctx.engine, { op: "layer.import", width: w, height: h, x: 0, y: 0, name: "Background blur", above: layer?.id, provenance: `ai:${model}` }, blurred);
      id = r.data?.id as number;
      exec(ctx.engine, { op: "layer.add-mask", id, from: "reveal-all" });
      exec(ctx.engine, { op: "layer.set-mask-pixels", id, x: 0, y: 0, width: w, height: h }, bg);
    });
    return { changed: true, result: { model, layer: id, sigma, timings: t.done() } };
  },

  "ai.select-object": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const s = summary(ctx.engine);
    const pts = (ctx.params.points as { x: number; y: number; positive: boolean }[]) ?? [];
    if (!pts.length) throw new Error("Click on the object to select it.");
    const cache = await samEmbeddings(ctx, s);
    t.mark("encoder");
    const dec = await getSession(ctx, "edgetam-decoder");
    const flat = new Float32Array(pts.flatMap((p) => [p.x, p.y]));
    const coords = wasm.ai_sam_points(flat, s.width, s.height);
    const labels = BigInt64Array.from(pts.map((p) => (p.positive ? 1n : 0n)));
    const o = await run(dec, {
      ...decoderEmbeddings(dec, cache),
      input_points: tensor(dec, "float32", coords, [1, 1, pts.length, 2]),
      input_labels: tensor(dec, "int64", labels, [1, 1, pts.length]),
      input_boxes: tensor(dec, "float32", new Float32Array(0), [1, 0, 4]),
    });
    t.mark("decoder");
    const iou = o.iou_scores.data as Float32Array;
    // One click means the whole object; more points are a refinement, where
    // the model's own IoU ranking is the better judge.
    const index = wasm.ai_sam_choose(o.pred_masks.data as Float32Array, iou, pts.length === 1);
    const mask = wasm.ai_sam_mask(o.pred_masks.data as Float32Array, index, s.width, s.height, cache.guide, cache.gw, cache.gh);
    t.mark("mask");
    exec(ctx.engine, { op: "select.mask", x: 0, y: 0, width: s.width, height: s.height, mode: mode(ctx.params), label: "Object Selection" }, mask);
    return { changed: true, result: { iou: Array.from(iou), index, coverage: wasm.ai_matte_coverage(mask), timings: t.done() } };
  },

  "ai.remove": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const { engine } = ctx;
    const s = summary(engine);
    const sel = s.selection?.bounds;
    if (!sel || sel.w <= 0 || sel.h <= 0) throw new Error("Select what to remove first.");
    const quality = ctx.params.quality === "best" || ctx.params.quality === "fast" ? ctx.params.quality : "auto";
    const region = JSON.parse(wasm.ai_inpaint_region(s.width, s.height, sel.x, sel.y, sel.w, sel.h)) as { x: number; y: number; w: number; h: number };
    let layer = activePixelLayer(s);
    const pixels = layer ? engine.layer_region(layer.id, region.x, region.y, region.w, region.h) : engine.region(region.x, region.y, region.w, region.h);
    const coverage = engine.selection_region(region.x, region.y, region.w, region.h);
    const inp = new wasm.AiInpainter(pixels, coverage, region.w, region.h, JSON.stringify({ max_runs: quality === "best" ? 5 : 12, refine: ctx.params.refine === true, grain: ctx.params.grain !== false }));
    // "auto": MI-GAN for strokes and small objects, LaMa once the hole is a
    // real share of what the model sees (MI-GAN invented debris in a
    // removed street walker at 13%; LaMa continued the road).
    const holeFraction = inp.hole_fraction();
    const model = quality === "best" || (quality === "auto" && holeFraction > 0.06) ? "lama" : "migan";
    const made = await getSession(ctx, model);
    t.mark("load");
    const M = 512;
    const crops = JSON.parse(inp.crops());
    let debugBytes: Uint8Array | undefined;
    try {
      const n = inp.count();
      for (let i = 0; i < n; i++) {
        ctx.checkCancelled();
        ctx.progress({ stage: "inpaint", fraction: i / n, message: n > 1 ? `Filling (${i + 1} of ${n})` : "Filling" });
        const buf = inp.input(i);
        const rgb = buf.subarray(0, M * M * 3);
        const hole = buf.subarray(M * M * 3);
        let outRgb: Uint8Array;
        if (model === "migan") {
          const plane = new Uint8Array(M * M * 3);
          for (let k = 0; k < M * M; k++) for (let c = 0; c < 3; c++) plane[c * M * M + k] = rgb[k * 3 + c];
          const known = new Uint8Array(M * M);
          for (let k = 0; k < M * M; k++) known[k] = 255 - hole[k];
          const o = await run(made, { image: tensor(made, "uint8", plane, [1, 3, M, M]), mask: tensor(made, "uint8", known, [1, 1, M, M]) });
          outRgb = planarToInterleaved(o.result.data, M, 1);
        } else {
          const mask = new Float32Array(M * M);
          for (let k = 0; k < M * M; k++) mask[k] = hole[k] ? 1 : 0;
          const o = await run(made, { image: tensor(made, "float32", interleavedToPlanar(rgb, M, 1 / 255), [1, 3, M, M]), mask: tensor(made, "float32", mask, [1, 1, M, M]) });
          outRgb = planarToInterleaved(o.output.data, M, 1);
        }
        inp.apply(i, outRgb);
        if (ctx.params.debug && i === 0) debugBytes = new Uint8Array([...buf.subarray(0, M * M * 3), ...outRgb, ...hole]);
        await yieldNow();
      }
      t.mark("model");
      const out = inp.finish();
      const provenance = `ai:${model}`;
      oneStep(engine, "Remove", () => {
        if (!layer) {
          const r = exec(engine, { op: "layer.add-pixel", name: "Remove" });
          layer = { id: r.data?.id as number } as LayerInfo;
        }
        // Not `respect_selection`: the fill is deliberately a few pixels wider
        // than the selection (editor-ai dilates the hole and feathers the
        // blend). Clipping to the selection's soft edge left the removed
        // object's own outline standing as a coloured contour.
        exec(engine, { op: "layer.set-pixels", id: layer.id, x: region.x, y: region.y, width: region.w, height: region.h, label: "Remove", provenance }, out);
      });
      return { changed: true, bytes: debugBytes, result: { model, holeFraction, backend: made.backend, runs: n, region, crops, timings: t.done() } };
    } finally {
      inp.free();
    }
  },

  "ai.upscale": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const { engine } = ctx;
    const s = summary(engine);
    const factor = Number(ctx.params.factor) === 4 ? 4 : 2;
    const layer = activePixelLayer(s);
    if (!layer) throw new Error("Select a pixel layer to upscale.");
    const { width: w, height: h } = s;
    if (w * h * factor * factor > MAX_OUTPUT_PIXELS) {
      throw new Error(`That would make a ${Math.round((w * h * factor * factor) / 1e6)} MP image; the limit in the browser is ${MAX_OUTPUT_PIXELS / 1e6} MP. Try 2× or crop first.`);
    }
    const model = ctx.params.quality === "best" ? "realesrgan-x4plus" : "realesr-x4v3";
    const src = engine.layer_region(layer.id, 0, 0, w, h);
    const r = await superResolve(ctx, model, src, w, h, factor);
    t.mark("model");
    oneStep(engine, `Upscale ${factor}×`, () => {
      exec(engine, { op: "image.resize", width: r.w, height: r.h, resample: "bicubic" });
      exec(engine, { op: "layer.set-pixels", id: layer.id, x: 0, y: 0, width: r.w, height: r.h, label: `Upscale ${factor}×`, provenance: `ai:${model}` }, r.rgba);
    });
    return { changed: true, result: { model, backend: r.backend, width: r.w, height: r.h, timings: t.done() } };
  },

  "ai.enhance": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const { engine } = ctx;
    const s = summary(engine);
    const layer = activePixelLayer(s);
    if (!layer) throw new Error("Select a pixel layer first.");
    const kind = ctx.params.kind === "jpeg" ? "jpeg" : "denoise";
    const strength = String(ctx.params.strength ?? "medium");
    const model = kind === "jpeg" || strength === "low" ? "realesr-x4v3" : strength === "high" ? "realesr-x4v3-wdn" : "realesr-x4v3-dn50";
    const { width: w, height: h } = s;
    const src = engine.layer_region(layer.id, 0, 0, w, h);
    const r = await superResolve(ctx, model, src, w, h, 1);
    t.mark("model");
    exec(engine, { op: "layer.set-pixels", id: layer.id, x: 0, y: 0, width: w, height: h, respect_selection: true, label: kind === "jpeg" ? "Remove JPEG Artifacts" : "Reduce Noise", provenance: `ai:${model}` }, r.rgba);
    return { changed: true, result: { model, backend: r.backend, timings: t.done() } };
  },

  "ai.detect-faces": async (ctx) => {
    await begin(ctx);
    const s = summary(ctx.engine);
    const full = ctx.engine.region(0, 0, s.width, s.height);
    return { result: await detectFaces(ctx, full, s.width, s.height) };
  },

  "ai.restore-faces": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const { engine } = ctx;
    const s = summary(engine);
    const layer = activePixelLayer(s);
    if (!layer) throw new Error("Select a pixel layer first.");
    const { width: w, height: h } = s;
    const full = engine.layer_region(layer.id, 0, 0, w, h);
    const faces = (await detectFaces(ctx, full, w, h)).filter((f) => f.bbox[2] >= 24).slice(0, 12);
    t.mark("detect");
    if (!faces.length) return { result: { faces: 0, timings: t.done() } };
    // One region around every face, read and written once: one undo step.
    let x0 = w, y0 = h, x1 = 0, y1 = 0;
    for (const f of faces) {
      const [bx, by, bw, bh] = f.bbox;
      const m = Math.max(bw, bh) * 1.3;
      x0 = Math.min(x0, Math.floor(bx - m));
      y0 = Math.min(y0, Math.floor(by - m));
      x1 = Math.max(x1, Math.ceil(bx + bw + m));
      y1 = Math.max(y1, Math.ceil(by + bh + m));
    }
    x0 = Math.max(0, x0); y0 = Math.max(0, y0); x1 = Math.min(w, x1); y1 = Math.min(h, y1);
    const rw = x1 - x0, rh = y1 - y0;
    let region = engine.layer_region(layer.id, x0, y0, rw, rh);
    let made: Made = await getSession(ctx, "gfpgan");
    const strength = Math.max(0, Math.min(1, Number(ctx.params.strength ?? 0.8)));
    let checked = false;
    for (let i = 0; i < faces.length; i++) {
      ctx.checkCancelled();
      ctx.progress({ stage: "faces", fraction: i / faces.length, message: `Restoring face ${i + 1} of ${faces.length}` });
      const lm = new Float32Array(faces[i].landmarks.flatMap(([x, y]) => [x - x0, y - y0]));
      const crop = wasm.ai_face_crop(region, rw, rh, lm);
      let o = await run(made, { input: tensor(made, "float32", crop, [1, 3, 512, 512]) });
      // GFPGAN's known silent WebGPU failure returns one wash for any face.
      if (!checked) {
        checked = true;
        if (made.backend === "webgpu" && wasm.ai_degenerate(o.output.data as Float32Array)) {
          made = await getSession(ctx, "gfpgan", { backend: "wasm" });
          o = await run(made, { input: tensor(made, "float32", crop, [1, 3, 512, 512]) });
        }
      }
      region = wasm.ai_face_paste(region, rw, rh, lm, o.output.data as Float32Array, 24, strength);
      await yieldNow();
    }
    t.mark("restore");
    exec(engine, { op: "layer.set-pixels", id: layer.id, x: x0, y: y0, width: rw, height: rh, respect_selection: true, label: "Restore Faces", provenance: "ai:gfpgan" }, region);
    return { changed: true, result: { faces: faces.length, backend: made.backend, timings: t.done() } };
  },

  "ai.colorize": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const { engine } = ctx;
    const s = summary(engine);
    const layer = activePixelLayer(s);
    if (!layer) throw new Error("Select a pixel layer first.");
    const { width: w, height: h } = s;
    const src = engine.layer_region(layer.id, 0, 0, w, h);
    const made = await getSession(ctx, "deoldify");
    ctx.progress({ message: "Adding colour" });
    const o = await run(made, { input: tensor(made, "float32", wasm.ai_deoldify_input(src, w, h, 256), [1, 3, 256, 256]) });
    t.mark("model");
    const out = wasm.ai_colorize_merge(src, w, h, o.output.data as Float32Array, 256, Number(ctx.params.saturation ?? 1));
    exec(engine, { op: "layer.set-pixels", id: layer.id, x: 0, y: 0, width: w, height: h, respect_selection: true, label: "Colorize", provenance: "ai:deoldify" }, out);
    return { changed: true, result: { backend: made.backend, timings: t.done() } };
  },

  "ai.analyze-scene": async (ctx) => {
    await begin(ctx);
    const t = timer();
    const s = summary(ctx.engine);
    const { width: w, height: h } = s;
    const full = ctx.engine.region(0, 0, w, h);
    const faces = (await detectFaces(ctx, full, w, h)).filter((f) => f.bbox[2] * f.bbox[3] > w * h * 0.001);
    const scale = Math.min(1, 512 / Math.max(w, h));
    const sw = Math.max(1, Math.round(w * scale)), sh = Math.max(1, Math.round(h * scale));
    const small = wasm.ai_resize_rgba(full, w, h, sw, sh, "bilinear");
    const made = await getSession(ctx, "u2netp");
    const o = await run(made, { "input.1": tensor(made, "float32", wasm.ai_u2net_input(small, sw, sh), [1, 3, 320, 320]) });
    const matte = wasm.ai_matte_finish(o[made.session.outputNames[0]].data as Float32Array, "u2net", small, sw, sh, false);
    const sky = wasm.ai_sky_fraction(small, sw, sh);
    const monochrome = wasm.ai_mean_chroma(small, sw, sh) < 4;
    return { result: { faces: faces.length, hasSubject: faces.length > 0 || wasm.ai_looks_like_subject(matte), hasSky: sky > 0.25, sky, subjectCoverage: wasm.ai_matte_coverage(matte), monochrome, timings: t.done() } };
  },

  "ai.red-eye": async (ctx) => {
    await begin(ctx);
    const s = summary(ctx.engine);
    const full = ctx.engine.region(0, 0, s.width, s.height);
    const faces = await detectFaces(ctx, full, s.width, s.height);
    let eyes = 0;
    for (const f of faces) {
      const [[rx, ry], [lx, ly]] = f.landmarks;
      const iod = Math.hypot(lx - rx, ly - ry);
      const r = Math.max(4, Math.round(iod * 0.22));
      for (const [x, y] of [[rx, ry], [lx, ly]]) {
        exec(ctx.engine, { op: "filter.red-eye", x: Math.round(x - r), y: Math.round(y - r), width: 2 * r, height: 2 * r });
        eyes++;
      }
    }
    return { changed: eyes > 0, result: { faces: faces.length, eyes } };
  },

  "ai.plan": async (ctx) => {
    return { result: JSON.parse(wasm.ai_plan(String(ctx.params.text ?? ""))) } as JobOutput;
  },
};

// Every job result carries the engine's warnings, if any command raised one.
register(
  Object.fromEntries(
    Object.entries(handlers).map(([name, run]) => [
      name,
      async (ctx: JobContext) => {
        const out = await run(ctx);
        if (warnings.length && out.result && typeof out.result === "object" && !Array.isArray(out.result)) {
          out.result = { ...(out.result as Record<string, unknown>), warnings: [...warnings] };
        }
        return out;
      },
    ]),
  ),
);

