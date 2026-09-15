// Foreground/background colour commands (X swaps, D resets).
import { editor } from "../lib/editor.svelte";

export function swapColors() {
  const p = editor.primary;
  editor.primary = editor.secondary;
  editor.secondary = p;
}

export function resetColors() {
  editor.primary = { r: 0, g: 0, b: 0, a: 255 };
  editor.secondary = { r: 255, g: 255, b: 255, a: 255 };
}
