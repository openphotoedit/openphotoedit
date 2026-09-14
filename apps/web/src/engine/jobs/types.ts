import type { Engine } from "../../wasm-gen/editor_wasm.js";

export interface JobContext {
  engine: Engine;
  params: Record<string, unknown>;
  bytes?: Uint8Array;
  /** Where the app is served from; models and runtimes resolve against it. */
  baseUrl: string;
  progress(p: { stage?: string; fraction?: number; message?: string }): void;
  /** Throws if the job was cancelled; call between expensive steps. */
  checkCancelled(): void;
}

export interface JobOutput {
  result?: unknown;
  /** Returned to the caller as a transferable ArrayBuffer. */
  bytes?: Uint8Array;
  /** Set when the job changed the document, so the UI refreshes. */
  changed?: boolean;
}

export type JobHandler = (ctx: JobContext) => Promise<JobOutput>;
