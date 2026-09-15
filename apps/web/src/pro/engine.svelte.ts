// Engine helpers the Pro shell shares: live previews that can be taken
// back, histograms, layer thumbnails, and friendly handling for commands a
// backend workstream has not landed yet.

import type { ExecResult, LayerId } from "../engine/types";
import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import { histogramOf, type HistogramData } from "../ui/histogram";

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function isUnknownOp(e: unknown): boolean {
  return /unknown operation|no method|not available yet|no job named/i.test(errorText(e));
}

export function isNotReady(e: unknown): boolean {
  return (e instanceof Error && e.name === "NotReady") || /not available yet/i.test(errorText(e));
}

/** Run a command, turning "not built yet" into a calm message instead of a raw error. */
export async function run(cmd: Record<string, unknown>, bytes?: Uint8Array, what?: string): Promise<ExecResult | null> {
  try {
    // Panels hand over reactive state; the worker needs plain data.
    const r = await editor.engine.exec($state.snapshot(cmd) as Record<string, unknown>, bytes);
    if (r.changed) editor.dirty = true;
    return r;
  } catch (e) {
    if (isUnknownOp(e)) editor.toast(t("{what} is not available in this build yet.", { what: what ?? String(cmd.op) }), "info");
    else editor.error(e);
    return null;
  }
}

/** Run an AI feature from lib/ai.ts with a calm message when its model is not ready. */
export async function runAi(label: string, fn: () => Promise<unknown>) {
  try {
    await fn();
  } catch (e) {
    if (isNotReady(e)) editor.toast(t("{what} is not available yet. It arrives with the local AI models.", { what: label }), "info");
    else editor.error(e);
  }
}

function undoLen() {
  return editor.summary?.history.undo.length ?? 0;
}

/**
 * A preview that is applied to the document and taken back again. Requests
 * are serialised and coalesced (only the latest waits), each one first
 * reverts the previous preview, and `cancel` leaves history as it was.
 */
export class PreviewSession {
  private steps = 0;
  private running = false;
  private queued: (() => Promise<unknown>) | null = null;
  private idle: (() => void)[] = [];
  private lastKey = "";
  error = $state<string | null>(null);
  busy = $state(false);

  /** Apply one or more commands as the preview; `key` identifies the parameters. */
  request(key: string, apply: () => Promise<unknown>) {
    this.queued = async () => {
      await this.revert();
      this.lastKey = key;
      await apply();
    };
    if (!this.running) void this.pump();
  }

  /** Execute a preview command and count the undo steps it created. */
  async exec(cmd: Record<string, unknown>, bytes?: Uint8Array): Promise<ExecResult | null> {
    const before = undoLen();
    try {
      const r = await editor.engine.exec($state.snapshot(cmd) as Record<string, unknown>, bytes);
      const after = undoLen();
      if (r.changed) this.steps += Math.max(after - before, after === before && !/set-adjustment|set-effects|layer\.props/.test(String(cmd.op)) ? 1 : 0);
      this.error = null;
      return r;
    } catch (e) {
      this.error = isUnknownOp(e) ? t("This is not available in this build yet.") : errorText(e);
      throw e;
    }
  }

  private async pump() {
    this.running = true;
    this.busy = true;
    while (this.queued) {
      const job = this.queued;
      this.queued = null;
      try {
        await job();
      } catch {
        /* recorded in `error` */
      }
    }
    this.running = false;
    this.busy = false;
    for (const f of this.idle.splice(0)) f();
  }

  private waitIdle() {
    if (!this.running) return Promise.resolve();
    return new Promise<void>((r) => this.idle.push(r));
  }

  async revert() {
    if (this.steps > 0) {
      const index = Math.max(0, undoLen() - this.steps);
      this.steps = 0;
      this.lastKey = "";
      await editor.engine.exec({ op: "edit.history-go", index });
    }
  }

  /** Stop previewing and take the preview back. */
  async cancel() {
    this.queued = null;
    await this.waitIdle();
    await this.revert();
  }

  /**
   * Keep the result. If the preview already shows `key`, it stays as it is;
   * otherwise `apply` runs for real.
   */
  async commit(key: string, apply: () => Promise<unknown>): Promise<boolean> {
    this.queued = null;
    await this.waitIdle();
    if (this.steps > 0 && this.lastKey === key && !this.error) {
      this.steps = 0;
      editor.dirty = true;
      return true;
    }
    await this.revert();
    try {
      await apply();
      editor.dirty = true;
      return true;
    } catch (e) {
      if (isUnknownOp(e)) editor.toast(t("This is not available in this build yet."), "info");
      else editor.error(e);
      return false;
    }
  }
}

// ---------------------------------------------------------------------------
// Histograms

let histCache: { key: string; data: Promise<HistogramData> } | null = null;

/** The merged document's histogram, or one layer's. Approximated from a downscaled render until `analyze.histogram` lands. */
export function histogram(opts: { id?: LayerId | null; merged?: boolean } = {}): Promise<HistogramData> {
  const s = editor.summary;
  const key = `${s?.revision}:${opts.id ?? "merged"}`;
  if (histCache?.key === key) return histCache.data;
  const data = (async () => {
    try {
      const r = await editor.engine.exec({ op: "analyze.histogram", merged: opts.merged ?? !opts.id, ...(opts.id ? { id: opts.id } : {}) });
      const d = r.data as unknown as HistogramData | undefined;
      if (d?.r?.length === 256) return d;
    } catch {
      /* fall back below */
    }
    const bytes = await editor.engine.call<Uint8Array>("thumbnail", opts.id ?? null, 384);
    return histogramOf(bytes.subarray(4));
  })();
  histCache = { key, data };
  return data;
}

// ---------------------------------------------------------------------------
// Thumbnails

const thumbs = new Map<string, Promise<ImageBitmap | null>>();
let chain: Promise<unknown> = Promise.resolve();

/** A layer (or, with `id` null, the composite) thumbnail, cached by revision and queued one at a time. */
export function thumbnail(id: LayerId | null, rev: number, size: number): Promise<ImageBitmap | null> {
  const key = `${id}:${rev}:${size}`;
  const hit = thumbs.get(key);
  if (hit) return hit;
  const p = (chain = chain.then(async () => {
    try {
      const bytes = await editor.engine.call<Uint8Array>("thumbnail", id, size);
      const w = bytes[0] | (bytes[1] << 8);
      const h = bytes[2] | (bytes[3] << 8);
      if (!w || !h) return null;
      const px = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 4, w * h * 4);
      return await createImageBitmap(new ImageData(px.slice(), w, h));
    } catch {
      return null;
    }
  })) as Promise<ImageBitmap | null>;
  thumbs.set(key, p);
  // Keep the cache bounded: drop the oldest entries.
  if (thumbs.size > 400) {
    const first = thumbs.keys().next().value;
    if (first) thumbs.delete(first);
  }
  return p;
}

const maskThumbs = new Map<string, Promise<ImageBitmap | null>>();

/** A layer mask thumbnail. Uses `mask_thumbnail` when the engine has it, else samples the mask. */
export function maskThumbnail(id: LayerId, rev: number, size: number): Promise<ImageBitmap | null> {
  const key = `${id}:${rev}:${size}`;
  const hit = maskThumbs.get(key);
  if (hit) return hit;
  const p = (chain = chain.then(async () => {
    const s = editor.summary;
    if (!s) return null;
    const k = Math.min(1, size / Math.max(s.width, s.height));
    const w = Math.max(1, Math.round(s.width * k));
    const h = Math.max(1, Math.round(s.height * k));
    const out = new Uint8ClampedArray(w * h * 4);
    try {
      let gray: Uint8Array;
      try {
        const b = await editor.engine.call<Uint8Array>("mask_thumbnail", id, size);
        gray = b.subarray(4);
      } catch {
        // Whole-mask read: fine for ordinary photos, skipped for huge documents.
        if (s.width * s.height > 40_000_000) return null;
        const full = await editor.engine.call<Uint8Array>("mask_region", id, 0, 0, s.width, s.height);
        gray = new Uint8Array(w * h);
        for (let y = 0; y < h; y++) {
          const sy = Math.min(s.height - 1, Math.floor((y + 0.5) / k));
          for (let x = 0; x < w; x++) gray[y * w + x] = full[sy * s.width + Math.min(s.width - 1, Math.floor((x + 0.5) / k))];
        }
      }
      for (let i = 0; i < w * h; i++) {
        const v = gray[i] ?? 255;
        out[i * 4] = v;
        out[i * 4 + 1] = v;
        out[i * 4 + 2] = v;
        out[i * 4 + 3] = 255;
      }
      return await createImageBitmap(new ImageData(out, w, h));
    } catch {
      return null;
    }
  })) as Promise<ImageBitmap | null>;
  maskThumbs.set(key, p);
  if (maskThumbs.size > 200) {
    const first = maskThumbs.keys().next().value;
    if (first) maskThumbs.delete(first);
  }
  return p;
}

/** Throttle to one call per animation frame, always delivering the latest arguments. */
export function rafThrottle<A extends unknown[]>(fn: (...a: A) => void | Promise<void>) {
  let pending: A | null = null;
  let frame = 0;
  let inFlight = false;
  const flush = async () => {
    frame = 0;
    if (inFlight || !pending) return;
    const args = pending;
    pending = null;
    inFlight = true;
    try {
      await fn(...args);
    } finally {
      inFlight = false;
      if (pending && !frame) frame = requestAnimationFrame(flush);
    }
  };
  const call = (...a: A) => {
    pending = a;
    if (!frame && !inFlight) frame = requestAnimationFrame(flush);
  };
  call.flush = async () => {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    while (inFlight) await new Promise((r) => setTimeout(r, 4));
    if (pending) await flush();
  };
  return call;
}
