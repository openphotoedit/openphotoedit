// Real previews, rendered by the engine on a clone of the document (see
// `Engine::preview_thumbnail`). Results are cached against the parts of the
// document the variant does not replace.

import { editor } from "../lib/editor.svelte";
import type { LayerInfo } from "../engine/types";

export interface Variant {
  key: string;
  /** Commands that turn the current document into this variant. */
  commands: () => Record<string, unknown>[];
}

export interface Thumb {
  url: string;
  width: number;
  height: number;
}

/** Decode the engine's `[w lo, w hi, h lo, h hi, ...rgba]` thumbnail. */
export function decodeThumb(bytes: Uint8Array): { width: number; height: number; rgba: Uint8ClampedArray } {
  const width = bytes[0] | (bytes[1] << 8);
  const height = bytes[2] | (bytes[3] << 8);
  return { width, height, rgba: new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 4, width * height * 4) };
}

export async function thumbToUrl(bytes: Uint8Array): Promise<Thumb> {
  const { width, height, rgba } = decodeThumb(bytes);
  const c = new OffscreenCanvas(width, height);
  c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  const blob = await c.convertToBlob({ type: "image/png" });
  return { url: URL.createObjectURL(blob), width, height };
}

/** A key for "the document apart from layers named in `ignore`". */
export function docSignature(ignore: string[]): string {
  const s = editor.summary;
  if (!s) return "";
  const walk = (ls: LayerInfo[]): string =>
    ls
      .filter((l) => !ignore.includes(l.name))
      .map((l) => `${l.id}:${l.rev}:${l.visible ? 1 : 0}${l.children ? `[${walk(l.children)}]` : ""}`)
      .join(",");
  return `${s.width}x${s.height}|${walk(s.layers)}`;
}

export class PreviewSet {
  thumbs = $state<Record<string, Thumb>>({});
  loading = $state(false);
  /** Previews were skipped because they would discard redo steps. */
  blocked = $state(false);
  private signature = "";
  private run = 0;

  constructor(private readonly size = 256) {}

  /** Render every variant not already cached for `signature`. */
  async ensure(signature: string, variants: Variant[]) {
    if (!editor.hasDocument || !editor.summary) return;
    if (signature !== this.signature) {
      for (const t of Object.values(this.thumbs)) URL.revokeObjectURL(t.url);
      this.thumbs = {};
      this.signature = signature;
    }
    const todo = variants.filter((v) => !this.thumbs[v.key]);
    if (!todo.length) return;
    const run = ++this.run;
    this.loading = true;
    try {
      // The engine renders each variant on a throwaway clone of the
      // document, so previews never touch history or pending redo steps.
      for (const v of todo) {
        if (run !== this.run) break;
        let bytes: Uint8Array;
        try {
          bytes = await editor.engine.call<Uint8Array>("preview_thumbnail", JSON.stringify(v.commands()), this.size);
        } catch (e) {
          console.warn("preview failed", v.key, e);
          continue;
        }
        const thumb = await thumbToUrl(bytes);
        if (run === this.run && this.signature === signature) this.thumbs = { ...this.thumbs, [v.key]: thumb };
        else URL.revokeObjectURL(thumb.url);
      }
      this.blocked = false;
    } finally {
      if (run === this.run) this.loading = false;
    }
  }

  clear() {
    this.run++;
    for (const t of Object.values(this.thumbs)) URL.revokeObjectURL(t.url);
    this.thumbs = {};
    this.signature = "";
    this.loading = false;
  }
}

/** The original, as-is thumbnail (no silent edits). */
export async function currentThumb(size: number): Promise<Thumb | null> {
  if (!editor.hasDocument) return null;
  const bytes = await editor.engine.call<Uint8Array>("thumbnail", null, size);
  return thumbToUrl(bytes);
}
