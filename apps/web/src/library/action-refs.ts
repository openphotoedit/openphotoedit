// Actions store commands, but commands name layers by id, and ids are only
// meaningful in the document they were recorded in. At record time each id
// becomes a reference: "the layer step 3 created", or "the layer named X,
// the Nth from the top". At playback the reference is resolved against the
// document the action is playing on. Pure functions; unit-tested.

import { allLayers, type LayerInfo, type Summary } from "../engine/types";

export interface LayerRef {
  $layer: {
    /** The step (its id, or index) whose result created this layer. Ids survive reordering. */
    step?: number | string;
    name?: string;
    kind?: string;
    /** Position from the top of the flattened layer list (0 = topmost). */
    fromTop?: number;
    /** The layer was the active one when recorded. */
    active?: boolean;
  };
}

/** Parameters that hold one layer id, and those that hold a list of them. */
const ID_KEYS = new Set(["id", "parent", "above", "target_id", "source_id"]);
const IDS_KEYS = new Set(["ids"]);

/** Ops that never belong in an action: history, view state, analysis. */
export function isReplayable(op: string): boolean {
  if (op.startsWith("edit.") || op.startsWith("analyze.") || op.startsWith("view.")) return false;
  if (op === "layer.set-active" || op === "doc.open-pixels" || op === "doc.new") return false;
  return true;
}

export function isLayerRef(v: unknown): v is LayerRef {
  return !!v && typeof v === "object" && "$layer" in (v as object);
}

function flat(summary: Summary | null): LayerInfo[] {
  return summary ? allLayers(summary.layers) : [];
}

/**
 * Replace layer ids in `cmd` with references.
 * `before` is the document as it was when the command ran; `created` maps
 * ids made by earlier steps of this recording to their step index.
 */
export function parameterize(cmd: Record<string, unknown>, before: Summary | null, created: Map<number, number | string>): Record<string, unknown> {
  const layers = flat(before);
  const ref = (id: unknown): unknown => {
    if (typeof id !== "number") return id;
    if (created.has(id)) return { $layer: { step: created.get(id)! } } satisfies LayerRef;
    const i = layers.findIndex((l) => l.id === id);
    if (i < 0) return id;
    const l = layers[i];
    return { $layer: { name: l.name, kind: l.kind, fromTop: layers.length - 1 - i, active: before?.active === id || undefined } } satisfies LayerRef;
  };
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(cmd)) {
    if (ID_KEYS.has(k)) out[k] = ref(v);
    else if (IDS_KEYS.has(k) && Array.isArray(v)) out[k] = v.map(ref);
    else out[k] = v;
  }
  return out;
}

export interface Resolution {
  cmd: Record<string, unknown>;
  /** Why a reference could not be resolved exactly (shown as a note). */
  notes: string[];
}

/**
 * Turn references back into ids for `summary` (the document now).
 * `stepIds` holds the id each earlier step created during this playback.
 * Order: created-by-step, then a unique name (and kind) match, then the same
 * position from the top, then the active layer.
 */
export function resolve(cmd: Record<string, unknown>, summary: Summary | null, stepIds: Map<number | string, number>): Resolution {
  const layers = flat(summary);
  const notes: string[] = [];
  const one = (v: unknown): unknown => {
    if (!isLayerRef(v)) return v;
    const r = v.$layer;
    if (r.step != null) {
      const id = stepIds.get(r.step);
      if (id != null) return id;
      notes.push(typeof r.step === "number" ? `step ${r.step + 1} did not create a layer` : "the step that created this layer did not run");
    }
    if (r.name != null) {
      const byName = layers.filter((l) => l.name === r.name && (!r.kind || l.kind === r.kind));
      if (byName.length === 1) return byName[0].id;
      if (byName.length > 1 && r.fromTop != null) {
        // Several with that name: the one nearest the recorded position.
        const pos = (l: LayerInfo) => layers.length - 1 - layers.indexOf(l);
        return byName.reduce((a, b) => (Math.abs(pos(a) - r.fromTop!) <= Math.abs(pos(b) - r.fromTop!) ? a : b)).id;
      }
    }
    if (r.active && summary?.active != null) {
      notes.push(`no layer named "${r.name}"; used the active layer`);
      return summary.active;
    }
    if (r.fromTop != null && r.fromTop < layers.length) {
      notes.push(`no layer named "${r.name}"; used the layer in the same position`);
      return layers[layers.length - 1 - r.fromTop].id;
    }
    if (summary?.active != null) {
      notes.push(`no layer matched "${r.name ?? "a created layer"}"; used the active layer`);
      return summary.active;
    }
    throw new Error(`no layer matches "${r.name ?? "the recorded layer"}"`);
  };
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(cmd)) {
    if (Array.isArray(v)) out[k] = v.map(one);
    else out[k] = one(v);
  }
  return { cmd: out, notes };
}

/** The id a command's result created, if any. */
export function createdId(result: { data?: Record<string, unknown> | null } | null | undefined): number | undefined {
  const id = result?.data?.id;
  return typeof id === "number" ? id : undefined;
}
