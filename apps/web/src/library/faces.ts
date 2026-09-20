// Face detection for culling, on a private engine instance so the user's
// open document is never touched. The AI workstream's `ai.detect-faces` job
// runs on whatever document its engine holds; here that is a 512 px proxy.
//
// When the job is missing, the model cannot load, or it throws `NotReady`,
// detection reports itself unavailable and culling carries on with sharpness
// and exposure only. `retry()` lets a later run try again.

import { EngineClient } from "../engine/client";
import type { FaceBox } from "./cull";

export type FaceStatus = "unknown" | "loading" | "ready" | "unavailable";

let client: EngineClient | null = null;
let initPromise: Promise<void> | null = null;
let status: FaceStatus = "unknown";
let reason = "";

export function faceStatus() {
  return { status, reason };
}

export function retryFaces() {
  if (status === "unavailable") {
    status = "unknown";
    reason = "";
  }
}

async function ensure() {
  if (!client) {
    client = new EngineClient();
    initPromise = client.init().then(() => undefined);
  }
  await initPromise;
  return client;
}

let lock: Promise<unknown> = Promise.resolve();

/** Faces in an RGBA proxy, or null when detection is unavailable. Calls are serialised: one engine, one document. */
export function detectFacesIn(rgba: Uint8ClampedArray | Uint8Array, width: number, height: number): Promise<FaceBox[] | null> {
  const run = lock.then(() => detect(rgba, width, height));
  lock = run.catch(() => null);
  return run;
}

async function detect(rgba: Uint8ClampedArray | Uint8Array, width: number, height: number): Promise<FaceBox[] | null> {
  if (status === "unavailable") return null;
  try {
    if (status === "unknown") status = "loading";
    const engine = await ensure();
    await engine.exec({ op: "doc.open-pixels", width, height, name: "cull-proxy" }, rgba);
    const r = await engine.job<{ score: number; bbox: number[]; landmarks: number[][] }[]>("ai.detect-faces").promise;
    status = "ready";
    return (r.result ?? []).map((f) => ({ x: f.bbox[0], y: f.bbox[1], w: f.bbox[2], h: f.bbox[3], score: f.score, landmarks: (f.landmarks ?? []).map(([x, y]) => ({ x, y })) }));
  } catch (e) {
    status = "unavailable";
    reason = e instanceof Error ? e.message : String(e);
    console.info("Face detection unavailable for culling:", reason);
    return null;
  }
}

/** Release the private engine worker. */
export function disposeFaces() {
  const w = (client as unknown as { worker?: Worker } | null)?.worker;
  w?.terminate();
  client = null;
  initPromise = null;
}
