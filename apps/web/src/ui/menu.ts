// Menu data shared by MenuBar, context menus, flyouts and Select.

import type { Component } from "svelte";

export interface MenuItem {
  type?: "item";
  id?: string;
  label: string;
  /** Shortcut spec, see ui/platform.ts. Shown, not bound: the owner binds it. */
  shortcut?: string;
  disabled?: boolean;
  /** Why it is disabled, shown as a tooltip. */
  hint?: string;
  checked?: boolean;
  /** A radio-style check (a dot rather than a tick). */
  radio?: boolean;
  icon?: Component<{ size?: number | string }>;
  testid?: string;
  run?: () => void;
  submenu?: MenuEntry[] | (() => MenuEntry[]);
}

export type MenuEntry = MenuItem | { type: "separator" } | { type: "heading"; label: string };

export type MenuCloseReason = "activate" | "escape" | "outside" | "left" | "tab";

export function isItem(e: MenuEntry): e is MenuItem {
  return e.type === undefined || e.type === "item";
}

export function subItems(e: MenuItem): MenuEntry[] | null {
  if (!e.submenu) return null;
  return typeof e.submenu === "function" ? e.submenu() : e.submenu;
}

export const SEP: MenuEntry = { type: "separator" };

/** Removes leading, trailing and doubled separators left by filtered items. */
export function tidy(entries: (MenuEntry | null | undefined | false)[]): MenuEntry[] {
  const out: MenuEntry[] = [];
  for (const e of entries) {
    if (!e) continue;
    if (e.type === "separator" && (out.length === 0 || out[out.length - 1].type === "separator")) continue;
    out.push(e);
  }
  while (out.length && out[out.length - 1].type === "separator") out.pop();
  return out;
}

export interface MenuBarMenu {
  id: string;
  label: string;
  items: () => MenuEntry[];
}
