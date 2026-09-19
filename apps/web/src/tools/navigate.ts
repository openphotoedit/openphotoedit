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

// Zoom: click zooms in a step at the pointer, Alt-click zooms out (both on
// release, as in Photoshop), and dragging a rectangle zooms to fit it.
let zdrag: { vx: number; vy: number; ex: number; ey: number; alt: boolean; moved: boolean } | null = null;

export const zoom: Tool = {
  id: "zoom",
  label: "Zoom",
  shortcut: "z",
  cursor: "zoom-in",
  down(_ed, p) {
    zdrag = { vx: p.vx, vy: p.vy, ex: p.vx, ey: p.vy, alt: p.alt, moved: false };
  },
  move(ed, p, pressed) {
    if (!pressed || !zdrag) return;
    zdrag.ex = p.vx;
    zdrag.ey = p.vy;
    if (Math.hypot(p.vx - zdrag.vx, p.vy - zdrag.vy) >= 4) zdrag.moved = true;
    ed.overlayTick++;
  },
  up(ed, p) {
    const d = zdrag;
    zdrag = null;
    if (!d) return;
    ed.overlayTick++;
    const w = Math.abs(p.vx - d.vx);
    const h = Math.abs(p.vy - d.vy);
    if (!d.moved || w < 4 || h < 4) {
      ed.zoomStep(p.alt || d.alt ? -1 : 1, d.vx, d.vy);
      return;
    }
    // Fit the dragged rectangle to the view.
    const a = ed.toDoc(Math.min(d.vx, p.vx), Math.min(d.vy, p.vy));
    const b = ed.toDoc(Math.max(d.vx, p.vx), Math.max(d.vy, p.vy));
    const z = Math.max(0.01, Math.min(64, Math.min(ed.viewport.width / (b.x - a.x), ed.viewport.height / (b.y - a.y))));
    ed.view = { cx: (a.x + b.x) / 2, cy: (a.y + b.y) / 2, zoom: z };
  },
  cancel(ed) {
    zdrag = null;
    ed.overlayTick++;
  },
  overlay(_ed, ctx) {
    const d = zdrag;
    if (!d?.moved) return;
    const x = Math.min(d.vx, d.ex);
    const y = Math.min(d.vy, d.ey);
    const w = Math.abs(d.ex - d.vx);
    const h = Math.abs(d.ey - d.vy);
    ctx.save();
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(255,255,255,0.95)";
    ctx.strokeRect(Math.round(x) + 0.5, Math.round(y) + 0.5, Math.round(w), Math.round(h));
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = "rgba(0,0,0,0.95)";
    ctx.strokeRect(Math.round(x) + 0.5, Math.round(y) + 0.5, Math.round(w), Math.round(h));
    ctx.restore();
  },
};
