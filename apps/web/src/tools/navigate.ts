import type { Tool } from "./types";

let last: { vx: number; vy: number } | null = null;

export const hand: Tool = {
  id: "hand",
  label: "Hand",
  shortcut: "h",
  cursor: "grab",
  down(_ed, p) {
    last = { vx: p.vx, vy: p.vy };
  },
  move(ed, p, pressed) {
    if (!pressed || !last) return;
    ed.panBy(p.vx - last.vx, p.vy - last.vy);
    last = { vx: p.vx, vy: p.vy };
  },
  up() {
    last = null;
  },
};

export const zoom: Tool = {
  id: "zoom",
  label: "Zoom",
  shortcut: "z",
  cursor: "zoom-in",
  down(ed, p) {
    ed.zoomStep(p.alt ? -1 : 1, p.vx, p.vy);
  },
};
