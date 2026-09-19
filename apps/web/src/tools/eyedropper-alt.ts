// Alt as a temporary eyedropper for the painting tools (brush, pencil,
// gradient): Alt-click or Alt-drag samples into the foreground colour with
// the eyedropper's loupe, releasing the mouse returns to the tool, and the
// cursor shows a pipette while Alt is held.

import type { EditorStore } from "../lib/editor.svelte";
import type { ToolPointer } from "./types";
import { eyedropper } from "./eyedropper";

// Lucide "pipette", white with a dark outline so it reads on any image;
// the hotspot is the tip (bottom left).
const PIPETTE =
  "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke-linecap='round' stroke-linejoin='round'>" +
  "<g stroke='%23000' stroke-width='4'><path d='m2 22 1-1h3l9-9'/><path d='M3 21v-3l9-9'/><path d='m15 6 3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4Z'/></g>" +
  "<g stroke='%23fff' stroke-width='2'><path d='m2 22 1-1h3l9-9'/><path d='M3 21v-3l9-9'/><path d='m15 6 3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4Z'/></g></svg>";

export const altEyedropper = {
  cursor: `url("data:image/svg+xml;utf8,${PIPETTE}") 2 22, crosshair`,
  down(ed: EditorStore, p: ToolPointer) {
    // The eyedropper's own Alt means "background"; here Alt is the switch.
    return eyedropper.down?.(ed, { ...p, alt: false });
  },
  move(ed: EditorStore, p: ToolPointer) {
    eyedropper.move?.(ed, p, true);
  },
  up(ed: EditorStore, p: ToolPointer) {
    return eyedropper.up?.(ed, p);
  },
  cancel(ed: EditorStore) {
    eyedropper.cancel?.(ed);
  },
  overlay(ed: EditorStore, ctx: CanvasRenderingContext2D) {
    eyedropper.overlay?.(ed, ctx);
  },
};
