// The UI thread's handle on the engine worker.

import type { Request, Response } from "./protocol";
import type { ExecResult, Summary } from "./types";

type Pending = {
  resolve: (r: Extract<Response, { ok: true }>) => void;
  reject: (e: Error) => void;
  onProgress?: (p: { stage?: string; fraction?: number; message?: string }) => void;
};

export interface JobResult<T = unknown> {
  result: T;
  bytes?: Uint8Array;
  changed: boolean;
}

// Distributes Omit over a union, so each variant keeps its own fields.
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

export class EngineClient {
  private worker: Worker;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  /** Latest document summary; updated by every exec and changing job. */
  onSummary: (s: Summary) => void = () => {};

  constructor() {
    this.worker = new Worker(new URL("./engine.worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e: MessageEvent<Response>) => this.receive(e.data);
    this.worker.onerror = (e) => console.error("engine worker error", e);
  }

  private receive(msg: Response) {
    const p = this.pending.get(msg.id);
    if (!p) return;
    if ("progress" in msg) {
      p.onProgress?.(msg.progress);
      return;
    }
    this.pending.delete(msg.id);
    if (msg.ok) {
      if (msg.summary) this.onSummary(msg.summary);
      p.resolve(msg);
    } else {
      p.reject(new Error(msg.error));
    }
  }

  private send(
    req: DistributiveOmit<Request, "id">,
    transfer: Transferable[] = [],
    onProgress?: Pending["onProgress"],
  ): { id: number; promise: Promise<Extract<Response, { ok: true }>> } {
    const id = this.nextId++;
    const promise = new Promise<Extract<Response, { ok: true }>>((resolve, reject) => {
      this.pending.set(id, { resolve, reject, onProgress });
    });
    this.worker.postMessage({ ...req, id } as Request, transfer);
    return { id, promise };
  }

  async init(): Promise<Summary> {
    const baseUrl = new URL("./", document.baseURI).href;
    const r = await this.send({ type: "init", baseUrl }).promise;
    return r.summary!;
  }

  /** Run a document command. Pixel payloads are transferred, not copied. */
  async exec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray): Promise<ExecResult> {
    let buf: ArrayBuffer | undefined;
    if (bytes) buf = bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength ? (bytes.buffer as ArrayBuffer) : (bytes.slice().buffer as ArrayBuffer);
    const r = await this.send({ type: "exec", cmd, bytes: buf }, buf ? [buf] : []).promise;
    return r.exec!;
  }

  async render(x: number, y: number, scale: number, width: number, height: number): Promise<Uint8ClampedArray> {
    const r = await this.send({ type: "render", x, y, scale, width, height }).promise;
    return new Uint8ClampedArray(r.result as ArrayBuffer);
  }

  /** Call any engine method; byte results arrive as Uint8Array. */
  async call<T = unknown>(method: string, ...args: unknown[]): Promise<T> {
    const r = await this.send({ type: "call", method, args }).promise;
    return (r.result instanceof ArrayBuffer ? new Uint8Array(r.result) : r.result) as T;
  }

  /** Start a long job. Returns a cancel function alongside the promise. */
  job<T = unknown>(
    name: string,
    params: Record<string, unknown> = {},
    opts: { bytes?: Uint8Array; onProgress?: Pending["onProgress"] } = {},
  ): { promise: Promise<JobResult<T>>; cancel: () => void } {
    let buf: ArrayBuffer | undefined;
    if (opts.bytes) buf = opts.bytes.slice().buffer as ArrayBuffer;
    const { id, promise } = this.send({ type: "job", name, params, bytes: buf }, buf ? [buf] : [], opts.onProgress);
    return {
      promise: promise.then((r) => {
        const out = r.result as { result: T; bytes?: ArrayBuffer; changed: boolean };
        return { result: out.result, bytes: out.bytes ? new Uint8Array(out.bytes) : undefined, changed: out.changed };
      }),
      cancel: () => this.worker.postMessage({ type: "cancel", id: this.nextId++, job: id } satisfies Request),
    };
  }
}
