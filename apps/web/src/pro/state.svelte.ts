// State that belongs to the Pro shell only: panel layout, the open dialog,
// view extras (rulers, guides, pixel grid), the layer multi-selection and
// the cursor readout. The document itself lives in `editor`.

import type { Adjustment, LayerId, Rgba8 } from "../engine/types";
import { editor } from "../lib/editor.svelte";
import type { FilterDef } from "./filters";

export type PanelId = "layers" | "properties" | "adjustments" | "history" | "color" | "swatches" | "histogram" | "info" | "navigator";

export type DialogSpec =
  | { kind: "new" }
  | { kind: "export" }
  | { kind: "fill" }
  | { kind: "image-size" }
  | { kind: "canvas-size" }
  | { kind: "rotate" }
  | { kind: "filter"; def: FilterDef; smart?: { id: LayerId; index?: number; initial?: Record<string, unknown> } }
  | { kind: "smart-filter-blend"; id: LayerId; index: number }
  | { kind: "adjustment"; adjKind: string; initial?: Adjustment; title?: string }
  | { kind: "color-range" }
  | { kind: "modify"; op: "grow" | "shrink" | "border" | "smooth" | "feather" }
  | { kind: "select-mask" }
  | { kind: "shortcuts" }
  | { kind: "about" }
  | { kind: "color"; which: "primary" | "secondary" }
  | { kind: "layer-style"; id: LayerId; section?: string }
  | { kind: "fill-layer"; fill: "solid" | "gradient" };

export interface Guide {
  axis: "x" | "y";
  pos: number;
}

const LAYOUT_KEY = "ops.pro.layout";

interface Layout {
  dockWidth: number;
  dockCollapsed: boolean;
  visible: Record<PanelId, boolean>;
  groupTab: Record<string, PanelId>;
  collapsedGroups: string[];
  rulers: boolean;
  extras: boolean;
  pixelGrid: boolean;
  showGuides: boolean;
  propsHeight: number;
  swatches: string[];
}

const DEFAULT_SWATCHES = [
  "000000", "404040", "808080", "bfbfbf", "ffffff", "ff0000", "ffff00", "00ff00", "00ffff", "0000ff", "ff00ff",
  "ed1c24", "f26522", "f7941d", "fff200", "8dc63f", "39b54a", "00a651", "00a99d", "00aeef", "0072bc", "0054a6",
  "2e3192", "662d91", "92278f", "ec008c", "ed145b", "9e0b0f", "a0410d", "a36209", "aba000", "598527", "197b30",
  "007236", "00746b", "0076a3", "004b80", "003471", "1b1464", "440e62", "630460", "9e005d", "9e0039",
  "c69c6d", "a67c52", "8c6239", "754c24", "603913", "3c2415",
];

function defaults(): Layout {
  return {
    dockWidth: 300,
    dockCollapsed: false,
    visible: { layers: true, properties: true, adjustments: true, history: true, color: true, swatches: true, histogram: true, info: true, navigator: true },
    groupTab: { color: "color", props: "properties", layers: "layers", nav: "navigator", history: "history" },
    collapsedGroups: [],
    rulers: true,
    extras: true,
    pixelGrid: true,
    showGuides: true,
    propsHeight: 290,
    swatches: DEFAULT_SWATCHES,
  };
}

function load(): Layout {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    if (!raw) return defaults();
    const d = defaults();
    const v = JSON.parse(raw) as Partial<Layout>;
    return { ...d, ...v, visible: { ...d.visible, ...(v.visible ?? {}) }, groupTab: { ...d.groupTab, ...(v.groupTab ?? {}) } };
  } catch {
    return defaults();
  }
}

class ProState {
  layout = $state<Layout>(load());
  dialog = $state<DialogSpec | null>(null);
  /** Secondary panel group shown as a flyout from the icon strip. */
  flyout = $state<string | null>(null);
  /** Layers selected in the Layers panel (the active layer is always one). */
  selectedIds = $state<LayerId[]>([]);
  guides = $state<Guide[]>([]);
  /** Guide being dragged out of a ruler, in document units. */
  draftGuide = $state<Guide | null>(null);
  cursor = $state<{ x: number; y: number } | null>(null);
  cursorColor = $state<Rgba8 | null>(null);
  /** The last filter run from the Filter menu, for Filter › Last Filter. */
  lastFilter = $state<{ def: FilterDef; params: Record<string, unknown> } | null>(null);
  /** A menu is open; the canvas ignores shortcuts meanwhile. */
  menuOpen = $state(false);
  /** Bumps when the user asks to rename the active layer from a menu. */
  renameRequest = $state(0);

  save() {
    try {
      localStorage.setItem(LAYOUT_KEY, JSON.stringify($state.snapshot(this.layout)));
    } catch {
      /* private mode */
    }
  }

  open(d: DialogSpec) {
    this.dialog = d;
  }

  close() {
    this.dialog = null;
  }

  togglePanel(id: PanelId) {
    const v = !this.layout.visible[id];
    this.layout.visible[id] = v;
    if (v) {
      // Bring its group forward, as Window › <panel> does.
      for (const [g, members] of Object.entries(PANEL_GROUPS)) {
        if (members.includes(id)) {
          this.layout.groupTab[g] = id;
          this.layout.collapsedGroups = this.layout.collapsedGroups.filter((c) => c !== g);
          if (SECONDARY_GROUPS.includes(g)) this.flyout = g;
        }
      }
      this.layout.dockCollapsed = false;
    }
    this.save();
  }

  showPanel(id: PanelId) {
    if (!this.layout.visible[id]) this.togglePanel(id);
    for (const [g, members] of Object.entries(PANEL_GROUPS)) {
      if (members.includes(id)) {
        this.layout.groupTab[g] = id;
        this.layout.collapsedGroups = this.layout.collapsedGroups.filter((c) => c !== g);
        if (SECONDARY_GROUPS.includes(g)) this.flyout = g;
      }
    }
  }

  /** Selected layers that still exist, active first if none are selected. */
  selection(): LayerId[] {
    const s = editor.summary;
    if (!s) return [];
    const ids = this.selectedIds.filter((id) => existsIn(s.layers, id));
    if (ids.length) return ids;
    return s.active != null ? [s.active] : [];
  }
}

function existsIn(layers: { id: number; children?: unknown[] }[], id: number): boolean {
  for (const l of layers) {
    if (l.id === id) return true;
    if (l.children && existsIn(l.children as { id: number; children?: unknown[] }[], id)) return true;
  }
  return false;
}

/** Main dock groups, top to bottom, and the secondary groups in the icon strip. */
export const PANEL_GROUPS: Record<string, PanelId[]> = {
  color: ["color", "swatches"],
  props: ["properties", "adjustments"],
  layers: ["layers"],
  nav: ["navigator", "histogram", "info"],
  history: ["history"],
};
export const MAIN_GROUPS = ["color", "props", "layers"];
export const SECONDARY_GROUPS = ["nav", "history"];

export const pro = new ProState();
