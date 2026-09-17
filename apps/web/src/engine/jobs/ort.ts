// onnxruntime-web in the engine worker: backend choice, session options,
// session cache and the per-(model, backend) pixel-parity canary.
//
// Rules carried over from OpenPixels and OpenPhotoId, each paid for once:
//
// - The runtime's .wasm files come from this origin (`ort/`, copied there by
//   scripts/fetch-models.sh). The default resolves them against a CDN.
// - WebGPU only when an adapter actually exists (headless Chromium exposes
//   `navigator.gpu` with no adapter), and then the WebGPU build, which can
//   also run the CPU provider for fallbacks. Otherwise the smaller CPU build.
// - `enableMemPattern: false` + `freeDimensionOverrides`: WebGPU buffer
//   reuse fails on shape changes ("Shape mismatch attempting to re-use
//   buffer"), and graphs that add a resized input to their output trip it
//   even at one shape. Callers feed one constant shape per session.
// - `logSeverityLevel: 3`: the native layer logs harmless graph warnings
//   through console.error; `env.logLevel` does not reach them.
// - A session that *works* is not a session that is *right*: GFPGAN fp16 and
//   MODNet both returned wrong pixels on WebGPU with no error. So the first
//   session per (model, backend) runs a golden input (jobs/canary.ts) and a
//   failure marks that backend bad for that model, remembered on this device.

import type * as OrtModule from "onnxruntime-web";
import { MODELS, fetchModel, formatSize, modelBytes, type Backend } from "../../lib/models";
import type { JobContext } from "./types";
import { runCanary } from "./canary";

export type Ort = typeof OrtModule;
export type Session = OrtModule.InferenceSession;

export interface Made {
  id: string;
  session: Session;
  backend: Backend;
  ort: Ort;
  canary?: { ok: boolean; deviation?: number; note?: string; ms: number };
}

export const settings = {
  /** Never use WebGPU (diagnostics, or a user setting). */
  preferCpu: false,
  /** Allow WebGPU even for models listed as bad on it (diagnostics). */
  ignoreKnownBad: false,
};

interface Loaded {
  ort: Ort;
  webgpu: boolean;
  threads: number;
  note?: string;
}

let loaded: Promise<Loaded> | null = null;

async function hasAdapter(): Promise<boolean> {
  const gpu = (navigator as Navigator & { gpu?: { requestAdapter(): Promise<unknown> } }).gpu;
  if (!gpu) return false;
  try {
    return !!(await gpu.requestAdapter());
  } catch {
    return false;
  }
}

export function loadOrt(baseUrl: string): Promise<Loaded> {
  loaded ??= (async () => {
    const webgpu = !settings.preferCpu && (await hasAdapter());
    const ort: Ort = webgpu ? ((await import("onnxruntime-web/webgpu")) as unknown as Ort) : ((await import("onnxruntime-web/wasm")) as unknown as Ort);
    ort.env.wasm.wasmPaths = new URL("ort/", baseUrl).href;
    // Threads need cross-origin isolation; asking without it makes the
    // runtime probe, fail and fall back, so decide here.
    const threads = (globalThis as { crossOriginIsolated?: boolean }).crossOriginIsolated ? Math.max(1, Math.min(8, (navigator.hardwareConcurrency ?? 2) - 2)) : 1;
    ort.env.wasm.numThreads = threads;
    ort.env.logLevel = "error";
    return { ort, webgpu, threads };
  })();
  return loaded;
}

export async function runtimeInfo(baseUrl: string) {
  const l = await loadOrt(baseUrl);
  return { webgpu: l.webgpu, threads: l.threads, crossOriginIsolated: !!(globalThis as { crossOriginIsolated?: boolean }).crossOriginIsolated };
}

// ---------------------------------------------------------------------------
// Verdicts: which backend failed the canary for which model, on this device.

const VERDICTS = "openphotoshop-ai-verdicts-v1";
const VERDICT_URL = "https://verdicts.invalid/ai";
let verdicts: Record<string, { ok: boolean; deviation?: number; at: string }> | null = null;

async function loadVerdicts() {
  if (verdicts) return verdicts;
  verdicts = {};
  try {
    const c = await caches.open(VERDICTS);
    const r = await c.match(VERDICT_URL);
    if (r) verdicts = await r.json();
  } catch {
    /* no Cache Storage: verdicts last for this session */
  }
  return verdicts!;
}

async function saveVerdict(key: string, ok: boolean, deviation?: number) {
  const v = await loadVerdicts();
  v[key] = { ok, deviation, at: new Date().toISOString() };
  try {
    const c = await caches.open(VERDICTS);
    await c.put(VERDICT_URL, new Response(JSON.stringify(v), { headers: { "content-type": "application/json" } }));
  } catch {
    /* ignore */
  }
}

export async function allVerdicts() {
  return { ...(await loadVerdicts()) };
}

function verdictKey(id: string, backend: Backend) {
  return `${id}|${backend}|${navigator.userAgent}`;
}

// ---------------------------------------------------------------------------
// Sessions

const sessions = new Map<string, Promise<Made>>();

export interface SessionOptions {
  /** Named dynamic axes pinned to the one shape this session will see. */
  dims?: Record<string, number>;
  /** Force a backend (canary diagnostics). */
  backend?: Backend;
}

function sessionOptions(ort: Ort, backend: Backend, dims?: Record<string, number>): OrtModule.InferenceSession.SessionOptions {
  void ort;
  const o: OrtModule.InferenceSession.SessionOptions = {
    executionProviders: [backend === "webgpu" ? "webgpu" : "wasm"],
    graphOptimizationLevel: "all",
    enableMemPattern: false,
    logSeverityLevel: 3,
  };
  if (dims) o.freeDimensionOverrides = dims;
  return o;
}

async function create(ort: Ort, id: string, files: Uint8Array[], backend: Backend, dims?: Record<string, number>): Promise<Session> {
  const spec = MODELS[id];
  const opts = sessionOptions(ort, backend, dims);
  if (files.length > 1) {
    opts.externalData = spec.files.slice(1).map((f, i) => ({ path: f.path.split("/").pop()!, data: files[i + 1] }));
  }
  // Step the optimiser down before giving up: iOS 17-era WebKit refused
  // graphs at "all" that load at "basic" (OpenPhotoId).
  let last: unknown;
  for (const level of ["all", "basic"] as const) {
    try {
      return await ort.InferenceSession.create(files[0], { ...opts, graphOptimizationLevel: level });
    } catch (e) {
      last = e;
    }
  }
  throw last;
}

/**
 * A ready session for `id`. Downloads (with progress), picks a backend,
 * runs the canary once per (model, backend) and falls back to the CPU when
 * WebGPU is missing, refuses the graph or fails the canary.
 */
export function getSession(ctx: JobContext, id: string, opts: SessionOptions = {}): Promise<Made> {
  const key = `${id}|${JSON.stringify(opts.dims ?? {})}|${opts.backend ?? "auto"}`;
  let p = sessions.get(key);
  if (!p) {
    p = build(ctx, id, opts);
    sessions.set(key, p);
    p.catch(() => sessions.delete(key));
  }
  return p;
}

async function build(ctx: JobContext, id: string, opts: SessionOptions): Promise<Made> {
  const spec = MODELS[id];
  if (!spec) throw new Error(`unknown model ${id}`);
  const { ort, webgpu } = await loadOrt(ctx.baseUrl);
  const label = spec.label;
  const size = formatSize(modelBytes(id));
  ctx.progress({ stage: "download", fraction: 0, message: `Loading ${label} model (${size})` });
  const files = await fetchModel(id, (p) => {
    if (!p.cached) ctx.progress({ stage: "download", fraction: p.received / p.total, message: `Downloading ${label} model (${formatSize(p.received)} of ${size})` });
  });
  ctx.checkCancelled();
  const v = await loadVerdicts();

  const candidates: Backend[] = [];
  if (opts.backend) candidates.push(opts.backend);
  else {
    const knownBad = !settings.ignoreKnownBad && spec.badBackends?.includes("webgpu");
    const failed = v[verdictKey(id, "webgpu")]?.ok === false;
    if (webgpu && !settings.preferCpu && !knownBad && !failed) candidates.push("webgpu");
    candidates.push("wasm");
  }

  let lastError: unknown;
  for (const backend of candidates) {
    let session: Session;
    try {
      ctx.progress({ stage: "session", message: `Starting ${label} (${backend === "webgpu" ? "GPU" : "CPU"})` });
      session = await create(ort, id, files, backend, opts.dims);
    } catch (e) {
      lastError = e;
      console.warn(`${id}: ${backend} session failed`, e);
      continue;
    }
    const made: Made = { id, session, backend, ort };
    const vk = verdictKey(id, backend);
    // The canary runs on the first session of this (model, backend) on this
    // device, and again whenever no verdict is stored.
    if (!v[vk] || opts.backend) {
      const t0 = performance.now();
      const res = await runCanary(ctx, made, opts.dims).catch((e) => ({ ok: false, deviation: undefined, note: String(e?.message ?? e) }));
      made.canary = { ...res, ms: Math.round(performance.now() - t0) };
      if (!opts.backend) await saveVerdict(vk, res.ok, res.deviation);
      if (!res.ok && backend === "webgpu" && !opts.backend) {
        console.warn(`${id}: WebGPU output failed the pixel-parity canary (${res.note ?? res.deviation}); using the CPU`);
        await session.release().catch(() => {});
        continue;
      }
    }
    return made;
  }
  throw lastError instanceof Error ? lastError : new Error(`Could not start the ${label} model`);
}

export async function releaseAll() {
  for (const p of sessions.values()) {
    try {
      (await p).session.release();
    } catch {
      /* ignore */
    }
  }
  sessions.clear();
}

/** Run and return named outputs as plain arrays, disposing GPU tensors. */
export async function run(made: Made, feeds: Record<string, OrtModule.Tensor>): Promise<Record<string, { data: Float32Array | Uint8Array; dims: readonly number[] }>> {
  const out = await made.session.run(feeds);
  const res: Record<string, { data: Float32Array | Uint8Array; dims: readonly number[] }> = {};
  for (const [name, t] of Object.entries(out)) {
    const data = (t.location && t.location !== "cpu" ? await t.getData(true) : t.data) as Float32Array | Uint8Array;
    res[name] = { data, dims: t.dims };
    t.dispose?.();
  }
  return res;
}

export function tensor(made: Made, type: "float32" | "uint8" | "int64", data: Float32Array | Uint8Array | BigInt64Array, dims: number[]): OrtModule.Tensor {
  return new made.ort.Tensor(type, data as Float32Array, dims);
}
