// An instant tooltip shared by every control, drawn with `.oa-tooltip`.
// Native `title` tooltips take a second to appear and cannot show a
// shortcut in a second tone, so controls use this action instead.

import "./primitives.css";
import { formatShortcut } from "./platform";

export interface TooltipOptions {
  text: string;
  shortcut?: string;
  placement?: "bottom" | "right" | "top" | "left";
}

let el: HTMLDivElement | null = null;
let showTimer = 0;
let warm = false;
let coolTimer = 0;

function ensure() {
  if (el) return el;
  el = document.createElement("div");
  el.className = "oa-tooltip ops-tooltip";
  el.setAttribute("role", "tooltip");
  document.body.appendChild(el);
  return el;
}

function place(node: HTMLElement, opts: TooltipOptions) {
  const tip = ensure();
  tip.textContent = "";
  tip.append(document.createTextNode(opts.text));
  if (opts.shortcut) {
    const k = document.createElement("span");
    k.className = "ops-tooltip__key";
    k.textContent = formatShortcut(opts.shortcut);
    tip.append(k);
  }
  const r = node.getBoundingClientRect();
  tip.style.left = "0px";
  tip.style.top = "0px";
  const tr = tip.getBoundingClientRect();
  const gap = 6;
  let x = r.left + r.width / 2 - tr.width / 2;
  let y = r.bottom + gap;
  switch (opts.placement) {
    case "right":
      x = r.right + gap;
      y = r.top + r.height / 2 - tr.height / 2;
      break;
    case "left":
      x = r.left - tr.width - gap;
      y = r.top + r.height / 2 - tr.height / 2;
      break;
    case "top":
      y = r.top - tr.height - gap;
      break;
  }
  x = Math.max(4, Math.min(window.innerWidth - tr.width - 4, x));
  y = Math.max(4, Math.min(window.innerHeight - tr.height - 4, y));
  tip.style.left = `${Math.round(x)}px`;
  tip.style.top = `${Math.round(y)}px`;
  tip.classList.add("oa-tooltip--visible");
}

export function hideTooltip() {
  clearTimeout(showTimer);
  el?.classList.remove("oa-tooltip--visible");
  clearTimeout(coolTimer);
  coolTimer = window.setTimeout(() => (warm = false), 400);
}

export function tooltip(node: HTMLElement, initial: TooltipOptions | string | null | undefined) {
  let opts: TooltipOptions | null = typeof initial === "string" ? { text: initial } : (initial ?? null);
  const enter = () => {
    if (!opts?.text) return;
    clearTimeout(showTimer);
    clearTimeout(coolTimer);
    showTimer = window.setTimeout(
      () => {
        warm = true;
        if (opts && node.isConnected) place(node, opts);
      },
      warm ? 0 : 450,
    );
  };
  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hideTooltip);
  node.addEventListener("pointerdown", hideTooltip);
  node.addEventListener("focusin", (e) => {
    if ((e.target as HTMLElement).matches?.(":focus-visible")) enter();
  });
  node.addEventListener("focusout", hideTooltip);
  return {
    update(next: TooltipOptions | string | null | undefined) {
      opts = typeof next === "string" ? { text: next } : (next ?? null);
    },
    destroy() {
      hideTooltip();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hideTooltip);
      node.removeEventListener("pointerdown", hideTooltip);
    },
  };
}
