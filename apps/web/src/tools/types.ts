// The tool contract. A tool receives pointer events in document
// coordinates, sends commands to the engine, and may draw on the overlay.

import type { EditorStore } from "../lib/editor.svelte";

export interface ToolPointer {
  /** Document coordinates (fractional). */
  x: number;
  y: number;
  /** Viewport CSS pixels. */
  vx: number;
  vy: number;
  pressure: number;
  shift: boolean;
  alt: boolean;
  /** Cmd on macOS, Ctrl elsewhere. */
  mod: boolean;
  button: number;
  pointerType: string;
}

export interface Tool {
  id: string;
  label: string;
  /** Keyboard shortcut letter, Photoshop's where one exists. */
  shortcut?: string;
  cursor?: string;
  activate?(ed: EditorStore): void;
  deactivate?(ed: EditorStore): void;
  down?(ed: EditorStore, p: ToolPointer): void | Promise<void>;
  /** `pressed` is false for hover moves. */
  move?(ed: EditorStore, p: ToolPointer, pressed: boolean): void;
  up?(ed: EditorStore, p: ToolPointer): void | Promise<void>;
  /** Escape, or the tool being switched mid-gesture. */
  cancel?(ed: EditorStore): void;
  /** Return true if the key was handled. */
  key?(ed: EditorStore, e: KeyboardEvent): boolean;
  /** Draw in viewport CSS pixels. */
  overlay?(ed: EditorStore, ctx: CanvasRenderingContext2D): void;
}
