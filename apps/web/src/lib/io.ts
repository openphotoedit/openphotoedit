// Opening files into the engine and getting pixels back out as files.

import { editor } from "./editor.svelte";

export const IMAGE_ACCEPT = "image/*,.psd,.psb,.opproj,.heic,.heif,.avif,.jxl,.tif,.tiff,.cr2,.cr3,.nef,.arw,.raf,.orf,.rw2,.dng,.pef,.srw";

export interface Decoded {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

/**
 * Decode an image with the browser's own codecs. `createImageBitmap`
 * applies EXIF orientation and handles JPEG, PNG, WebP, GIF, BMP, AVIF, and
 * HEIC where the browser has a decoder (Safari).
 */
export async function decodeImage(blob: Blob): Promise<Decoded> {
  const bitmap = await createImageBitmap(blob, { imageOrientation: "from-image", premultiplyAlpha: "none", colorSpaceConversion: "default" });
  try {
    const c = new OffscreenCanvas(bitmap.width, bitmap.height);
    const g = c.getContext("2d", { willReadFrequently: true })!;
    g.drawImage(bitmap, 0, 0);
    const img = g.getImageData(0, 0, bitmap.width, bitmap.height);
    return { width: img.width, height: img.height, data: img.data };
  } finally {
    bitmap.close();
  }
}

function extension(name: string) {
  const i = name.lastIndexOf(".");
  return i >= 0 ? name.slice(i + 1).toLowerCase() : "";
}

export function baseName(name: string) {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(0, i) : name;
}

/** Hooks other modules install for formats the browser cannot decode. */
export const openers: Record<string, (file: File) => Promise<boolean>> = {};

/** Open a file as a new document. */
export async function openFile(file: File): Promise<boolean> {
  const ext = extension(file.name);
  try {
    const custom = openers[ext];
    if (custom) {
      const ok = await custom(file);
      if (ok) finishOpen(file.name);
      return ok;
    }
    editor.busy = { label: `Opening ${file.name}…` };
    const img = await decodeImage(file);
    const r = await editor.exec({ op: "doc.open-pixels", width: img.width, height: img.height, name: file.name }, img.data);
    if (!r) return false;
    finishOpen(file.name);
    return true;
  } catch (e) {
    const hint = ext === "heic" || ext === "heif" ? " This browser cannot decode HEIC; Safari can, or export the photo as JPEG first." : "";
    editor.toast(`Could not open ${file.name}.${hint}`, "error");
    console.error(e);
    return false;
  } finally {
    editor.busy = null;
  }
}

function finishOpen(name: string) {
  editor.fileName = baseName(name);
  editor.hasDocument = true;
  editor.dirty = false;
  editor.fit();
}

/** Place an image as a new layer, centred on the canvas. */
export async function placeFile(file: File): Promise<void> {
  if (!editor.hasDocument) {
    await openFile(file);
    return;
  }
  const img = await decodeImage(file);
  const s = editor.summary!;
  const x = Math.round((s.width - img.width) / 2);
  const y = Math.round((s.height - img.height) / 2);
  await editor.exec({ op: "layer.import", width: img.width, height: img.height, x, y, name: baseName(file.name) }, img.data);
}

export async function newDocument(width: number, height: number, background: { r: number; g: number; b: number; a?: number } | null) {
  const r = await editor.exec({ op: "doc.new", width, height, background });
  if (r) {
    editor.fileName = "Untitled";
    editor.hasDocument = true;
    editor.dirty = false;
    editor.fit();
  }
}

export type ExportFormat = "png" | "jpeg" | "webp";

export interface ExportOptions {
  format: ExportFormat;
  /** 0..1, JPEG and WebP. */
  quality?: number;
  /** Resize to this width (height follows) before encoding. */
  width?: number;
  /** Largest acceptable file size in bytes (JPEG/WebP): quality is searched down to fit. */
  maxBytes?: number;
  /** Fill transparency with this colour (JPEG has no alpha). */
  matte?: string;
}

/** Flatten and encode the document. */
export async function exportBlob(opts: ExportOptions): Promise<Blob> {
  const s = editor.summary!;
  let rgba = await editor.engine.call<Uint8Array>("flatten");
  let w = s.width;
  let h = s.height;
  let canvas: OffscreenCanvas | null = null;
  const toCanvas = () => {
    const c = new OffscreenCanvas(w, h);
    const g = c.getContext("2d")!;
    g.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), w, h), 0, 0);
    return c;
  };
  if (opts.width && opts.width !== w) {
    const src = toCanvas();
    const nw = Math.max(1, Math.round(opts.width));
    const nh = Math.max(1, Math.round((h * nw) / w));
    const c = new OffscreenCanvas(nw, nh);
    const g = c.getContext("2d")!;
    g.imageSmoothingQuality = "high";
    g.drawImage(src, 0, 0, nw, nh);
    w = nw;
    h = nh;
    const d = g.getImageData(0, 0, nw, nh).data;
    rgba = new Uint8Array(d.buffer, d.byteOffset, d.byteLength);
    canvas = c;
  }
  if (opts.format === "png") {
    const bytes = await editor.engine.call<Uint8Array>("encode_png", rgba, w, h);
    return new Blob([bytes as BlobPart], { type: "image/png" });
  }
  canvas ??= toCanvas();
  let out = canvas;
  if (opts.format === "jpeg") {
    out = new OffscreenCanvas(w, h);
    const g = out.getContext("2d")!;
    g.fillStyle = opts.matte ?? "#ffffff";
    g.fillRect(0, 0, w, h);
    g.drawImage(canvas, 0, 0);
  }
  const type = opts.format === "jpeg" ? "image/jpeg" : "image/webp";
  let quality = opts.quality ?? 0.9;
  let blob = await out.convertToBlob({ type, quality });
  if (opts.maxBytes && blob.size > opts.maxBytes) {
    // Binary search on quality for the best result under the limit.
    let lo = 0.05;
    let hi = quality;
    let best: Blob | null = null;
    for (let i = 0; i < 7; i++) {
      const q = (lo + hi) / 2;
      const b = await out.convertToBlob({ type, quality: q });
      if (b.size <= opts.maxBytes) {
        best = b;
        lo = q;
      } else {
        hi = q;
      }
    }
    blob = best ?? (await out.convertToBlob({ type, quality: 0.05 }));
    quality = lo;
  }
  return blob;
}

/** Save a blob: the File System Access picker where available, else a download. */
export async function saveBlob(blob: Blob, suggestedName: string): Promise<boolean> {
  const w = window as unknown as { showSaveFilePicker?: (o: unknown) => Promise<FileSystemFileHandle> };
  if (w.showSaveFilePicker) {
    try {
      const ext = suggestedName.split(".").pop() ?? "";
      const handle = await w.showSaveFilePicker({
        suggestedName,
        types: [{ description: ext.toUpperCase(), accept: { [blob.type || "application/octet-stream"]: ["." + ext] } }],
      });
      const stream = await handle.createWritable();
      await stream.write(blob);
      await stream.close();
      return true;
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return false;
      // Fall through to a download on any other failure.
    }
  }
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = suggestedName;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  return true;
}

export async function copyToClipboard(): Promise<void> {
  const blob = await exportBlob({ format: "png" });
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
  editor.toast("Copied to the clipboard", "success");
}

/** Pick files with a hidden input (works everywhere). */
export function pickFiles(accept = IMAGE_ACCEPT, multiple = false): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.multiple = multiple;
    input.onchange = () => resolve(input.files ? [...input.files] : []);
    input.oncancel = () => resolve([]);
    input.click();
  });
}
