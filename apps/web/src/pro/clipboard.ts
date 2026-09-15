// Edit › Cut, Copy, Copy Merged, Paste and Paste in Place, through the
// system clipboard as PNG.

import { editor } from "../lib/editor.svelte";
import { baseName, decodeImage } from "../lib/io";
import { t } from "../lib/i18n";
import { run } from "./engine.svelte";

let lastCopy: { x: number; y: number; w: number; h: number } | null = null;

function copyRect() {
  const s = editor.summary!;
  if (s.selection) {
    const b = s.selection.bounds;
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    return { x, y, w: Math.min(s.width, b.x + b.w) - x, h: Math.min(s.height, b.y + b.h) - y };
  }
  const a = editor.active;
  if (!s.selection && a?.bounds && a.kind !== "group") {
    const b = a.bounds;
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    return { x, y, w: Math.min(s.width, b.x + b.w) - x, h: Math.min(s.height, b.y + b.h) - y };
  }
  return { x: 0, y: 0, w: s.width, h: s.height };
}

async function toPng(rgba: Uint8Array, w: number, h: number): Promise<Blob> {
  const bytes = await editor.engine.call<Uint8Array>("encode_png", rgba, w, h);
  return new Blob([bytes as BlobPart], { type: "image/png" });
}

export async function copy(merged: boolean, cut = false): Promise<void> {
  const s = editor.summary;
  if (!s || !editor.hasDocument) return;
  const r = copyRect();
  if (r.w <= 0 || r.h <= 0) {
    editor.toast(t("Nothing to copy: the selection is outside the canvas."), "info");
    return;
  }
  const a = editor.active;
  try {
    let px: Uint8Array;
    if (merged || !a || a.kind === "group" || a.kind === "adjustment") {
      px = await editor.engine.call<Uint8Array>("region", r.x, r.y, r.w, r.h);
    } else {
      px = await editor.engine.call<Uint8Array>("layer_region", a.id, r.x, r.y, r.w, r.h);
    }
    if (s.selection) {
      const sel = await editor.engine.call<Uint8Array>("selection_region", r.x, r.y, r.w, r.h);
      for (let i = 0; i < sel.length; i++) px[i * 4 + 3] = Math.round((px[i * 4 + 3] * sel[i]) / 255);
    }
    const blob = await toPng(px, r.w, r.h);
    lastCopy = r;
    try {
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
    } catch {
      editor.toast(t("The browser blocked the clipboard. Allow clipboard access for this site and try again."), "error");
      return;
    }
    if (cut && !merged && a) await run({ op: "layer.clear", id: a.id }, undefined, t("Cut"));
  } catch (e) {
    editor.error(e);
  }
}

/** Paste the clipboard image as a new layer; `inPlace` puts it where it was copied from. */
export async function paste(inPlace = false): Promise<void> {
  let blob: Blob | null = null;
  try {
    const items = await navigator.clipboard.read();
    for (const item of items) {
      const type = item.types.find((ty) => ty.startsWith("image/"));
      if (type) {
        blob = await item.getType(type);
        break;
      }
    }
  } catch {
    editor.toast(t("Press {key} to paste: the browser only lets a page read images from the clipboard during a paste.", { key: navigator.platform.includes("Mac") ? "⌘V" : "Ctrl+V" }), "info");
    return;
  }
  if (!blob) {
    editor.toast(t("The clipboard has no image."), "info");
    return;
  }
  await pasteBlob(blob, inPlace);
}

export async function pasteBlob(blob: Blob, inPlace = false) {
  const img = await decodeImage(blob);
  if (!editor.hasDocument) {
    const { openFile } = await import("../lib/io");
    await openFile(new File([blob], t("Pasted image.png"), { type: blob.type }));
    return;
  }
  const s = editor.summary!;
  let x = Math.round((s.width - img.width) / 2);
  let y = Math.round((s.height - img.height) / 2);
  if (inPlace && lastCopy && lastCopy.w === img.width && lastCopy.h === img.height) {
    x = lastCopy.x;
    y = lastCopy.y;
  } else if (s.selection) {
    const b = s.selection.bounds;
    x = Math.round(b.x + (b.w - img.width) / 2);
    y = Math.round(b.y + (b.h - img.height) / 2);
  }
  await run({ op: "layer.import", width: img.width, height: img.height, x, y, name: t("Layer"), above: s.active ?? undefined }, new Uint8Array(img.data.buffer), t("Paste"));
}

/** File › Place Embedded: a smart object when the engine has them, else a pixel layer. */
export async function placeEmbedded(file: File) {
  if (!editor.hasDocument) {
    const { openFile } = await import("../lib/io");
    await openFile(file);
    return;
  }
  const img = await decodeImage(file);
  const s = editor.summary!;
  // Fit inside the canvas as Photoshop does, keeping the aspect ratio.
  const x = Math.round((s.width - img.width) / 2);
  const y = Math.round((s.height - img.height) / 2);
  const name = baseName(file.name);
  try {
    const r = await editor.engine.exec({ op: "layer.place-smart", width: img.width, height: img.height, x, y, name }, new Uint8Array(img.data.buffer.slice(0)));
    if (r.changed) editor.dirty = true;
  } catch {
    await run({ op: "layer.import", width: img.width, height: img.height, x, y, name, above: s.active ?? undefined }, new Uint8Array(img.data.buffer), t("Place"));
  }
}
