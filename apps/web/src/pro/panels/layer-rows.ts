// The Layers panel's flattened, top-to-bottom row model.

import type { LayerId } from "../../engine/types";
import type { EffectKey, ProLayer } from "../types";
import { EFFECTS } from "../types";

export type Row =
  | { type: "layer"; key: string; layer: ProLayer; depth: number; parent: LayerId | null; index: number; clipped: boolean; clipBase: boolean }
  | { type: "fx-header"; key: string; layer: ProLayer; depth: number }
  | { type: "fx"; key: string; layer: ProLayer; depth: number; effect: EffectKey; label: string; enabled: boolean }
  | { type: "sf-header"; key: string; layer: ProLayer; depth: number }
  | { type: "sf"; key: string; layer: ProLayer; depth: number; index: number; label: string; enabled: boolean };

export function buildRows(layers: ProLayer[], collapsedFx: Set<LayerId>, filterLabel: (op: string) => string): Row[] {
  const out: Row[] = [];
  const walk = (list: ProLayer[], depth: number, parent: LayerId | null) => {
    for (let i = list.length - 1; i >= 0; i--) {
      const l = list[i];
      const below = list[i - 1];
      const above = list[i + 1];
      out.push({ type: "layer", key: `l${l.id}`, layer: l, depth, parent, index: i, clipped: l.clip && !!below, clipBase: !l.clip && !!above?.clip });
      if (l.smart && l.smart.filters.length) {
        out.push({ type: "sf-header", key: `sfh${l.id}`, layer: l, depth });
        l.smart.filters.forEach((f, fi) => out.push({ type: "sf", key: `sf${l.id}:${fi}`, layer: l, depth, index: fi, label: filterLabel(f.filter.op), enabled: f.enabled }));
      }
      const fx = l.effects;
      if (fx && EFFECTS.some((e) => fx[e.key]) && !collapsedFx.has(l.id)) {
        out.push({ type: "fx-header", key: `fxh${l.id}`, layer: l, depth });
        // Photoshop lists effects in the dialog's order.
        for (const e of EFFECTS) {
          const v = fx[e.key];
          if (v) out.push({ type: "fx", key: `fx${l.id}:${e.key}`, layer: l, depth, effect: e.key, label: e.label, enabled: v.enabled });
        }
      }
      if (l.kind === "group" && l.expanded !== false && l.children) walk(l.children, depth + 1, l.id);
    }
  };
  walk(layers, 0, null);
  return out;
}

export function isDescendant(layer: ProLayer, id: LayerId): boolean {
  if (!layer.children) return false;
  return layer.children.some((c) => c.id === id || isDescendant(c, id));
}
