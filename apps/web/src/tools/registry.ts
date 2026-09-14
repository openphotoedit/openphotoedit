// Every tool, by id. Shells choose which to show; the canvas dispatches to
// whichever is active.
import type { Tool } from "./types";
import { hand, zoom } from "./navigate";

export const TOOLS: Record<string, Tool> = {};

export function registerTools(...tools: Tool[]) {
  for (const t of tools) TOOLS[t.id] = t;
}

registerTools(hand, zoom);
