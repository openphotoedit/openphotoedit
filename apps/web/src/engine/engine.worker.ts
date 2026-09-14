// The engine worker: owns the WebAssembly document and every long job, so
// the UI thread never holds a 50 MB buffer or waits on a model.

import init, { Engine } from "../wasm-gen/editor_wasm.js";
import type { Request, Response } from "./protocol";
import { JOBS } from "./jobs/index";
import "./jobs/register";

let engine: Engine | null = null;
let baseUrl = "";
const cancelled = new Set<number>();

class Cancelled extends Error {
  constructor() {
    super("cancelled");
    this.name = "Cancelled";
  }
}

function post(msg: Response, transfer: Transferable[] = []) {
  (self as unknown as Worker).postMessage(msg, transfer);
}

function summary() {
  return JSON.parse(engine!.summary());
}

function asTransferable(v: unknown): { value: unknown; transfer: Transferable[] } {
  if (v instanceof Uint8Array || v instanceof Uint8ClampedArray || v instanceof Float32Array) {
    const buf = v.byteOffset === 0 && v.byteLength === v.buffer.byteLength ? v.buffer : v.slice().buffer;
    return { value: buf, transfer: [buf as ArrayBuffer] };
  }
  return { value: v, transfer: [] };
}

self.onmessage = async (event: MessageEvent<Request>) => {
  const msg = event.data;
  try {
    switch (msg.type) {
      case "init": {
        baseUrl = msg.baseUrl;
        await init();
        engine = new Engine();
        post({ id: msg.id, ok: true, result: null, summary: summary() });
        break;
      }
      case "exec": {
        const bytes = msg.bytes ? new Uint8Array(msg.bytes) : new Uint8Array(0);
        const exec = JSON.parse(engine!.exec(JSON.stringify(msg.cmd), bytes));
        post({ id: msg.id, ok: true, result: exec, exec, summary: summary() });
        break;
      }
      case "render": {
        const px = engine!.render(msg.x, msg.y, msg.scale, msg.width, msg.height);
        const { value, transfer } = asTransferable(px);
        post({ id: msg.id, ok: true, result: value }, transfer);
        break;
      }
      case "call": {
        const fn = (engine as unknown as Record<string, (...a: unknown[]) => unknown>)[msg.method];
        if (typeof fn !== "function") throw new Error(`engine has no method ${msg.method}`);
        const args = msg.args.map((a) => (a instanceof ArrayBuffer ? new Uint8Array(a) : a));
        const out = fn.apply(engine, args);
        const { value, transfer } = asTransferable(out);
        post({ id: msg.id, ok: true, result: value }, transfer);
        break;
      }
      case "job": {
        const handler = JOBS[msg.name];
        if (!handler) throw new Error(`no job named ${msg.name}`);
        const out = await handler({
          engine: engine!,
          params: msg.params,
          bytes: msg.bytes ? new Uint8Array(msg.bytes) : undefined,
          baseUrl,
          progress: (p) => post({ id: msg.id, progress: p }),
          checkCancelled: () => {
            if (cancelled.has(msg.id)) throw new Cancelled();
          },
        });
        cancelled.delete(msg.id);
        const { value, transfer } = asTransferable(out.bytes);
        post(
          { id: msg.id, ok: true, result: { result: out.result, bytes: value, changed: !!out.changed }, summary: out.changed ? summary() : undefined },
          transfer,
        );
        break;
      }
      case "cancel":
        cancelled.add(msg.job);
        break;
    }
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    if ("id" in msg) post({ id: msg.id, ok: false, error });
  }
};
