// Camera RAW files: a Camera Raw dialog develops the file, then it opens as
// an ordinary 8-bit document. Call `installRawIo()` once at start-up; it
// registers openers for every RAW extension with `io.ts`.
//
// The engine keeps the decoded file between calls, so after the first call
// every engine method here is passed an empty byte array ("the cached file")
// instead of copying 25 MB to the worker on each slider change.

import { mount, unmount } from "svelte";
import { editor } from "./editor.svelte";
import { baseName, openers } from "./io";
import { t } from "./i18n";
import RawDialog from "./RawDialog.svelte";

export const RAW_EXTENSIONS = [
  "3fr", "ari", "arw", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "erf", "iiq", "kdc", "mef", "mos", "mrw",
  "nef", "nrw", "orf", "ori", "pef", "raf", "raw", "rw2", "rwl", "sr2", "srf", "srw", "x3f",
];
/** File-picker `accept` for RAW files (add it to the open dialog's list). */
export const RAW_ACCEPT = RAW_EXTENSIONS.map((e) => `.${e}`).join(",");

export interface RawWhiteBalance {
  temperature: number;
  tint: number;
  multipliers: [number, number, number];
}

export interface RawInfo {
  make: string;
  model: string;
  width: number;
  height: number;
  iso: number | null;
  /** Seconds. */
  exposure: number | null;
  aperture: number | null;
  /** Millimetres. */
  focal: number | null;
  lens: string | null;
  as_shot_wb: RawWhiteBalance;
  /** EXIF orientation 1..8. */
  orientation: number;
  sensor: "bayer" | "x-trans" | "linear" | "mono";
}

export type WbPreset = "as-shot" | "auto" | "daylight" | "cloudy" | "shade" | "tungsten" | "fluorescent" | "flash" | "custom";

/** Mirrors `editor_raw::DevelopParams`. Sliders are -100..100 unless noted. */
export interface RawParams {
  /** Stops, -5..5. */
  exposure: number;
  wb: WbPreset;
  /** Kelvin; used when `wb` is "custom". */
  temperature: number;
  tint: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  vibrance: number;
  saturation: number;
  /** 0..100. */
  noise_luma: number;
  /** 0..100. */
  noise_chroma: number;
  /** 0..150. */
  sharpen: number;
}

export const DEFAULT_RAW_PARAMS: RawParams = {
  exposure: 0,
  wb: "as-shot",
  temperature: 5500,
  tint: 0,
  contrast: 0,
  highlights: 0,
  shadows: 0,
  whites: 0,
  blacks: 0,
  vibrance: 0,
  saturation: 0,
  noise_luma: 0,
  noise_chroma: 25,
  sharpen: 40,
};

export interface RawImage {
  width: number;
  height: number;
  /** Straight RGBA8, already oriented. */
  rgba: Uint8ClampedArray;
}

const EMPTY = new Uint8Array(0);

/** Engine calls the dialog uses. `bytes` go across once; later calls use the cache. */
export class RawEngine {
  private sent = false;
  constructor(private bytes: Uint8Array) {}

  private payload() {
    if (this.sent) return EMPTY;
    this.sent = true;
    return this.bytes;
  }

  /** Retry with the bytes if the engine dropped its cache (another RAW opened meanwhile). */
  private async withCache<T>(fn: (b: Uint8Array) => Promise<T>): Promise<T> {
    try {
      return await fn(this.payload());
    } catch (e) {
      if (e instanceof Error && e.message.includes("No RAW file is loaded")) return fn(this.bytes);
      throw e;
    }
  }

  /** The embedded camera JPEG (or a quick develop), drawn into a canvas-ready bitmap. */
  async preview(maxSize: number): Promise<ImageBitmap | RawImage> {
    const out = await editor.engine.call<Uint8Array>("raw_preview", this.bytes, maxSize);
    const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
    const kind = out[0];
    const orientation = out[1];
    const width = dv.getUint32(4, true);
    const height = dv.getUint32(8, true);
    const payload = out.subarray(12);
    if (kind === 2) return { width, height, rgba: new Uint8ClampedArray(payload.buffer, payload.byteOffset, payload.byteLength) };
    const bitmap = await createImageBitmap(new Blob([payload as BlobPart], { type: "image/jpeg" }), { imageOrientation: "none" });
    return orientation > 1 ? orient(bitmap, orientation) : bitmap;
  }

  async develop(params: RawParams, maxSize?: number): Promise<RawImage> {
    const json = JSON.stringify(maxSize ? { ...params, max_size: maxSize } : params);
    const out = await this.withCache((b) => editor.engine.call<Uint8Array>("raw_develop", b, json, true));
    const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
    const px = out.subarray(8);
    return { width: dv.getUint32(0, true), height: dv.getUint32(4, true), rgba: new Uint8ClampedArray(px.buffer, px.byteOffset, px.byteLength) };
  }

  async whiteBalance(params: RawParams): Promise<RawWhiteBalance> {
    const json = JSON.stringify(params);
    return JSON.parse(await this.withCache((b) => editor.engine.call<string>("raw_white_balance", b, json)));
  }

  /** Full-resolution develop into the current document. Returns the summary. */
  async open(params: RawParams, name: string): Promise<string> {
    const json = JSON.stringify(params);
    return this.withCache((b) => editor.engine.call<string>("raw_open", b, json, name));
  }

  close() {
    void editor.engine.call("raw_close").catch(() => {});
  }
}

/** Apply EXIF orientation to a bitmap (embedded previews are stored unrotated). */
async function orient(src: ImageBitmap, o: number): Promise<ImageBitmap> {
  const swap = o >= 5;
  const w = swap ? src.height : src.width;
  const h = swap ? src.width : src.height;
  const c = new OffscreenCanvas(w, h);
  const g = c.getContext("2d")!;
  // Matrices map source pixels to the oriented canvas (EXIF 2..8).
  const m: Record<number, [number, number, number, number, number, number]> = {
    2: [-1, 0, 0, 1, w, 0],
    3: [-1, 0, 0, -1, w, h],
    4: [1, 0, 0, -1, 0, h],
    5: [0, 1, 1, 0, 0, 0],
    6: [0, 1, -1, 0, w, 0],
    7: [0, -1, -1, 0, w, h],
    8: [0, -1, 1, 0, 0, h],
  };
  const k = m[o];
  if (!k) return src;
  g.setTransform(k[0], k[1], k[2], k[3], k[4], k[5]);
  g.drawImage(src, 0, 0);
  src.close();
  return c.transferToImageBitmap();
}

/** Show the Camera Raw dialog. Resolves with the chosen settings, or null if cancelled. */
export function showRawDialog(name: string, info: RawInfo, raw: RawEngine): Promise<RawParams | null> {
  return new Promise((resolve) => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    let app: ReturnType<typeof mount> | null = null;
    const done = (result: RawParams | null) => {
      if (app) void unmount(app);
      app = null;
      target.remove();
      resolve(result);
    };
    app = mount(RawDialog, { target, props: { name, info, raw, ondone: done } });
  });
}

/** Open a RAW file: probe, Camera Raw dialog, then develop into the current document. */
export async function openRaw(file: File): Promise<boolean> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let info: RawInfo;
  editor.busy = { label: t("Reading {name}…", { name: file.name }) };
  try {
    info = JSON.parse(await editor.engine.call<string>("raw_probe", bytes));
  } catch (e) {
    const reason = e instanceof Error ? e.message : String(e);
    editor.toast(t("Could not read {name}: {reason}. The camera may not be supported yet.", { name: file.name, reason }), "error");
    console.error(e);
    return false;
  } finally {
    editor.busy = null;
  }

  const raw = new RawEngine(bytes);
  const params = await showRawDialog(file.name, info, raw);
  if (!params) {
    raw.close();
    return false;
  }
  editor.busy = { label: t("Developing {name}…", { name: file.name }) };
  try {
    const json = await raw.open(params, baseName(file.name));
    editor.summary = JSON.parse(json);
    editor.renderTick++;
    return true;
  } catch (e) {
    const reason = e instanceof Error ? e.message : String(e);
    editor.toast(t("Could not develop {name}: {reason}.", { name: file.name, reason }), "error");
    console.error(e);
    return false;
  } finally {
    editor.busy = null;
  }
}

let installed = false;

/** Register openers for every RAW extension with `io.ts`. Safe to call more than once. */
export function installRawIo(): void {
  if (installed) return;
  installed = true;
  for (const ext of RAW_EXTENSIONS) openers[ext] = openRaw;
}
