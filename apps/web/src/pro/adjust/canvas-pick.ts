// Canvas sampling for adjustment editors: while an eyedropper (or the
// Hue/Saturation targeted-adjust hand) is armed, pointer presses on the
// canvas go here instead of to the active tool. Nothing else changes: the
// tool comes back as soon as the pick is disarmed.

import type { Rgba8 } from "../../engine/types";
import { editor } from "../../lib/editor.svelte";

export interface PickPoint {
  x: number;
  y: number;
  clientX: number;
  clientY: number;
  mod: boolean;
}

export interface PickHandlers {
  down: (p: PickPoint) => void;
  move?: (p: PickPoint) => void;
  up?: (p: PickPoint) => void;
}

const OVERLAY = ".ops-canvas__overlay";

function point(e: PointerEvent, overlay: Element): PickPoint {
  const r = overlay.getBoundingClientRect();
  const d = editor.toDoc(e.clientX - r.left, e.clientY - r.top);
  return { x: d.x, y: d.y, clientX: e.clientX, clientY: e.clientY, mod: e.metaKey || e.ctrlKey };
}

/**
 * The canvas overlay under a press: the press lands on it directly (panels),
 * or on the bare clear scrim of an Image › Adjustments dialog above it.
 */
function overlayAt(e: PointerEvent): Element | null {
  const t = e.target as Element | null;
  const direct = t?.closest?.(OVERLAY);
  if (direct) return direct;
  if (!t?.classList?.contains("ops-dialog-scrim") || !t.classList.contains("clear")) return null;
  const o = document.querySelector(OVERLAY);
  const r = o?.getBoundingClientRect();
  return o && r && e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom ? o : null;
}

/** Route canvas presses to `h` until the returned function is called. */
export function armCanvasPick(h: PickHandlers, cursor = "crosshair"): () => void {
  let dragging: Element | null = null;
  const prevCursor = editor.cursor;
  editor.cursor = cursor;
  const down = (e: PointerEvent) => {
    const overlay = overlayAt(e);
    if (!overlay || e.button !== 0 || !editor.hasDocument) return;
    e.stopPropagation();
    e.preventDefault();
    dragging = overlay;
    h.down(point(e, overlay));
  };
  const move = (e: PointerEvent) => {
    if (!dragging) return;
    e.stopPropagation();
    h.move?.(point(e, dragging));
  };
  const up = (e: PointerEvent) => {
    if (!dragging) return;
    e.stopPropagation();
    const o = dragging;
    dragging = null;
    h.up?.(point(e, o));
  };
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", up, true);
  return () => {
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", up, true);
    if (editor.cursor === cursor) editor.cursor = prevCursor ?? null;
  };
}

/**
 * Average colour of a size×size box of the composite at (x, y). With
 * `below`, that adjustment layer is left out, so the sample is what the
 * adjustment receives (Photoshop samples beneath an adjustment layer too).
 */
export async function sampleComposite(x: number, y: number, size = 3, below: number | null = null): Promise<Rgba8 | null> {
  const s = editor.summary;
  if (!s) return null;
  const px = Math.floor(x);
  const py = Math.floor(y);
  if (px < 0 || py < 0 || px >= s.width || py >= s.height) return null;
  const half = Math.floor(size / 2);
  const x0 = Math.max(0, px - half);
  const y0 = Math.max(0, py - half);
  const w = Math.min(s.width, px + half + 1) - x0;
  const h = Math.min(s.height, py + half + 1) - y0;
  let bytes: Uint8Array;
  if (below != null) {
    // Viewport renders honour the preview-hidden list; hide the layer for
    // this one 1:1 render and put the list back.
    const keep = editor.previewHidden;
    await editor.engine.call("set_preview_hidden", Uint32Array.from([...keep, below]));
    try {
      bytes = await editor.engine.call<Uint8Array>("render", x0, y0, 1, w, h);
    } finally {
      await editor.engine.call("set_preview_hidden", Uint32Array.from(keep));
    }
  } else {
    bytes = await editor.engine.call<Uint8Array>("region", x0, y0, w, h);
  }
  let r = 0;
  let g = 0;
  let b = 0;
  let a = 0;
  for (let i = 0; i < w * h; i++) {
    const al = bytes[i * 4 + 3];
    r += bytes[i * 4] * al;
    g += bytes[i * 4 + 1] * al;
    b += bytes[i * 4 + 2] * al;
    a += al;
  }
  if (a === 0) return null;
  return { r: r / a, g: g / a, b: b / a, a: 255 };
}
