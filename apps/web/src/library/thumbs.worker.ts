// The library's own worker: thumbnails, metadata and culling signals, off the
// UI thread and away from the engine worker (which owns the open document).
// Results are cached in IndexedDB by file key, so a folder opens instantly
// the second time.

import { put, get } from "./db";
import { measure, PROXY_EDGE } from "./cull";
import { decodeBitmap, readHead, readMeta } from "./decode";
import type { ExifData } from "./exif";
import type { ItemKind } from "./types";

export const THUMB_EDGE = 384;

export type WorkerRequest =
  | { type: "thumb"; id: number; key: string; file: Blob; ext: string; kind: ItemKind }
  | { type: "meta"; id: number; key: string; file: Blob; ext: string; kind: ItemKind }
  | { type: "analyze"; id: number; key: string; file: Blob; ext: string; kind: ItemKind; wantPixels: boolean }
  | { type: "cancel"; ids: number[] };

export interface ThumbRecord {
  blob: Blob | null;
  width?: number;
  height?: number;
  exif: ExifData;
  embedded: boolean;
  /** Why no thumbnail could be made. */
  error?: string;
}

export type WorkerResponse =
  | { id: number; ok: true; thumb?: ThumbRecord; exif?: ExifData; signals?: ReturnType<typeof measure>; pixels?: { rgba: ArrayBuffer; width: number; height: number }; cached?: boolean }
  | { id: number; ok: false; error: string; cancelled?: boolean };

const cancelled = new Set<number>();

function post(msg: WorkerResponse, transfer: Transferable[] = []) {
  (self as unknown as Worker).postMessage(msg, transfer);
}

function check(id: number) {
  if (cancelled.has(id)) {
    cancelled.delete(id);
    throw Object.assign(new Error("cancelled"), { cancelled: true });
  }
}

async function metaFor(key: string, file: Blob, ext: string, kind: ItemKind, head?: Uint8Array): Promise<ExifData> {
  const cached = await get<ThumbRecord>("thumbs", key);
  if (cached) return cached.exif;
  return readMeta(file, ext, kind, head);
}

async function thumb(msg: Extract<WorkerRequest, { type: "thumb" }>): Promise<WorkerResponse> {
  const cached = await get<ThumbRecord>("thumbs", msg.key);
  if (cached) return { id: msg.id, ok: true, thumb: cached, cached: true };
  check(msg.id);
  const head = await readHead(msg.file, msg.kind);
  const exif = await readMeta(msg.file, msg.ext, msg.kind, head);
  check(msg.id);
  let record: ThumbRecord;
  try {
    const { bitmap, embedded } = await decodeBitmap(msg.file, msg.kind, THUMB_EDGE, exif, head);
    check(msg.id);
    const c = new OffscreenCanvas(bitmap.width, bitmap.height);
    const g = c.getContext("2d")!;
    g.drawImage(bitmap, 0, 0);
    bitmap.close();
    const blob = await c.convertToBlob({ type: "image/jpeg", quality: 0.82 });
    // Browser-decoded formats report their real size in EXIF/SOF; embedded previews do not.
    record = { blob, width: exif.width, height: exif.height, exif, embedded };
  } catch (e) {
    if ((e as { cancelled?: boolean }).cancelled) throw e;
    record = { blob: null, width: exif.width, height: exif.height, exif, embedded: false, error: e instanceof Error ? e.message : String(e) };
  }
  await put("thumbs", msg.key, record);
  return { id: msg.id, ok: true, thumb: record };
}

async function analyze(msg: Extract<WorkerRequest, { type: "analyze" }>): Promise<{ res: WorkerResponse; transfer: Transferable[] }> {
  const head = await readHead(msg.file, msg.kind);
  const exif = await metaFor(msg.key, msg.file, msg.ext, msg.kind, head);
  check(msg.id);
  const { bitmap } = await decodeBitmap(msg.file, msg.kind, PROXY_EDGE, exif, head);
  check(msg.id);
  const c = new OffscreenCanvas(bitmap.width, bitmap.height);
  const g = c.getContext("2d", { willReadFrequently: true })!;
  g.drawImage(bitmap, 0, 0);
  const { width, height } = bitmap;
  bitmap.close();
  const img = g.getImageData(0, 0, width, height);
  const signals = measure(img.data, width, height);
  if (msg.wantPixels) {
    const buf = img.data.buffer as ArrayBuffer;
    return { res: { id: msg.id, ok: true, signals, exif, pixels: { rgba: buf, width, height } }, transfer: [buf] };
  }
  return { res: { id: msg.id, ok: true, signals, exif }, transfer: [] };
}

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  const msg = e.data;
  if (msg.type === "cancel") {
    for (const id of msg.ids) cancelled.add(id);
    return;
  }
  try {
    if (msg.type === "thumb") post(await thumb(msg));
    else if (msg.type === "meta") post({ id: msg.id, ok: true, exif: await metaFor(msg.key, msg.file, msg.ext, msg.kind) });
    else if (msg.type === "analyze") {
      const { res, transfer } = await analyze(msg);
      post(res, transfer);
    }
  } catch (err) {
    cancelled.delete(msg.id);
    post({ id: msg.id, ok: false, error: err instanceof Error ? err.message : String(err), cancelled: !!(err as { cancelled?: boolean }).cancelled });
  }
};
