// Messages between the UI thread and the engine worker.
//
// Every request carries an `id`; the worker answers with the same id and
// either `result` or `error`. Large pixel buffers travel as transferable
// ArrayBuffers in both directions.

import type { ExecResult, Summary } from "./types";

export type Request =
  | { id: number; type: "init"; baseUrl: string }
  | { id: number; type: "exec"; cmd: Record<string, unknown>; bytes?: ArrayBuffer }
  | { id: number; type: "render"; x: number; y: number; scale: number; width: number; height: number }
  /** Any other `Engine` method by name; array results come back as ArrayBuffers. */
  | { id: number; type: "call"; method: string; args: unknown[] }
  /** AI and other long jobs routed to modules registered in the worker. */
  | { id: number; type: "job"; name: string; params: Record<string, unknown>; bytes?: ArrayBuffer }
  | { id: number; type: "cancel"; job: number };

export type Response =
  | { id: number; ok: true; result: unknown; summary?: Summary; exec?: ExecResult }
  | { id: number; ok: false; error: string }
  | { id: number; progress: { stage?: string; fraction?: number; message?: string } };
