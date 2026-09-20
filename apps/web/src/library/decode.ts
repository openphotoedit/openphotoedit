// Turning a library file into something a browser can draw: the file itself
// for formats the browser decodes, the embedded camera JPEG for raws, and
// Photoshop's stored thumbnail for PSDs. Shared by the worker and the loupe.

import { embeddedJpegCandidates, isDecodableJpeg, parseExif, psdThumbnail, RAW_EXTENSIONS, type ExifData } from "./exif";
import type { ItemKind } from "./types";

export const IMAGE_EXTENSIONS = new Set(["jpg", "jpeg", "jpe", "png", "webp", "avif", "gif", "bmp", "heic", "heif", "psd", "psb", "tif", "tiff"]);

export function kindOf(ext: string): ItemKind | null {
  if (RAW_EXTENSIONS.has(ext)) return "raw";
  if (ext === "psd" || ext === "psb") return "psd";
  if (ext === "tif" || ext === "tiff") return "tiff";
  if (ext === "heic" || ext === "heif") return "heif";
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  return null;
}

/** How many head bytes to read for metadata. */
export function headSize(kind: ItemKind) {
  return kind === "raw" || kind === "tiff" || kind === "psd" ? 1 << 20 : 256 * 1024;
}

export async function readHead(file: Blob, kind: ItemKind): Promise<Uint8Array> {
  return new Uint8Array(await file.slice(0, headSize(kind)).arrayBuffer());
}

export async function readMeta(file: Blob, ext: string, kind: ItemKind, head?: Uint8Array): Promise<ExifData> {
  return parseExif(head ?? (await readHead(file, kind)), { ext });
}

export interface Source {
  blob: Blob;
  /** An embedded preview rather than the photo itself. */
  embedded: boolean;
}

/** Candidate blobs to decode, best first. */
export async function sources(file: Blob, kind: ItemKind, head?: Uint8Array): Promise<Source[]> {
  const out: Source[] = [];
  if (kind === "image" || kind === "heif" || kind === "tiff") out.push({ blob: file, embedded: false });
  if (kind === "raw" || kind === "tiff") {
    const h = head ?? (await readHead(file, kind));
    for (const c of embeddedJpegCandidates(h, file.size).slice(0, 4)) {
      const start = new Uint8Array(await file.slice(c.offset, c.offset + 65536).arrayBuffer());
      if (isDecodableJpeg(start)) {
        out.push({ blob: file.slice(c.offset, c.offset + c.length, "image/jpeg"), embedded: true });
        break;
      }
    }
  }
  if (kind === "psd") {
    const h = head ?? (await readHead(file, kind));
    const r = psdThumbnail(h);
    if (r) out.push({ blob: file.slice(r.offset, r.offset + r.length, "image/jpeg"), embedded: true });
  }
  return out;
}

/**
 * Decode to a bitmap no larger than `maxEdge` on its long side (0 = full
 * size), honouring EXIF orientation. Tries each source in turn.
 */
export async function decodeBitmap(file: Blob, kind: ItemKind, maxEdge: number, meta?: ExifData, head?: Uint8Array): Promise<{ bitmap: ImageBitmap; embedded: boolean }> {
  let lastError: unknown = new Error("no decodable image");
  for (const s of await sources(file, kind, head)) {
    try {
      return { bitmap: await bitmapAt(s.blob, maxEdge, s.embedded ? undefined : meta), embedded: s.embedded };
    } catch (e) {
      lastError = e;
    }
  }
  throw lastError;
}

async function bitmapAt(blob: Blob, maxEdge: number, meta?: ExifData): Promise<ImageBitmap> {
  const opts: ImageBitmapOptions = { imageOrientation: "from-image", premultiplyAlpha: "none" };
  if (maxEdge > 0 && meta?.width && meta.height && Math.max(meta.width, meta.height) > maxEdge) {
    // Downscale while decoding. Browsers apply orientation before resizing,
    // so the target size is the oriented one.
    const swap = (meta.orientation ?? 1) >= 5;
    const w = swap ? meta.height : meta.width;
    const h = swap ? meta.width : meta.height;
    const s = maxEdge / Math.max(w, h);
    const bmp = await createImageBitmap(blob, { ...opts, resizeWidth: Math.max(1, Math.round(w * s)), resizeHeight: Math.max(1, Math.round(h * s)), resizeQuality: "medium" });
    return bmp;
  }
  const full = await createImageBitmap(blob, opts);
  if (maxEdge <= 0 || Math.max(full.width, full.height) <= maxEdge) return full;
  const s = maxEdge / Math.max(full.width, full.height);
  const scaled = await createImageBitmap(full, { resizeWidth: Math.max(1, Math.round(full.width * s)), resizeHeight: Math.max(1, Math.round(full.height * s)), resizeQuality: "medium" });
  full.close();
  return scaled;
}
