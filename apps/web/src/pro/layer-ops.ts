// Layer operations shared by menus, shortcuts and the Layers panel.

import type { Adjustment, LayerId, LayerInfo } from "../engine/types";
import { allLayers, findLayer } from "../engine/types";
import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import { pickFiles, decodeImage } from "../lib/io";
import { defaultAdjustment, kindInfo } from "./adjustments";
import { isUnknownOp, run } from "./engine.svelte";
import { pro } from "./state.svelte";
import type { ProLayer } from "./types";

export function activeLayer(): ProLayer | null {
  return editor.active as ProLayer | null;
}

export function parentOf(id: LayerId): LayerInfo | null {
  const s = editor.summary;
  if (!s) return null;
  for (const l of allLayers(s.layers)) if (l.children?.some((c) => c.id === id)) return l;
  return null;
}

export function siblingsOf(id: LayerId): LayerInfo[] {
  const p = parentOf(id);
  return p ? (p.children ?? []) : (editor.summary?.layers ?? []);
}

export async function setActive(id: LayerId) {
  if (editor.summary?.active === id) return;
  try {
    await editor.engine.exec({ op: "layer.set-active", id });
  } catch (e) {
    editor.error(e);
  }
}

export async function newLayer() {
  const r = await run({ op: "layer.add-pixel", above: editor.summary?.active ?? undefined }, undefined, t("New layer"));
  if (r?.data?.id != null) pro.selectedIds = [r.data.id as number];
}

export async function newAdjustmentLayer(kind: string, adjustment?: Adjustment) {
  const adj = adjustment ?? defaultAdjustment(kind);
  const r = await run({ op: "layer.add-adjustment", adjustment: adj, name: kindInfo(kind)?.label, above: editor.summary?.active ?? undefined }, undefined, t("Adjustment layer"));
  if (r?.data?.id != null) {
    pro.selectedIds = [r.data.id as number];
    pro.showPanel("properties");
  }
  return r;
}

export async function deleteLayers(ids = pro.selection()) {
  if (!ids.length) return;
  await run({ op: "layer.delete", ids }, undefined, t("Delete layer"));
  pro.selectedIds = [];
}

export async function duplicateLayers(ids = pro.selection()) {
  if (!ids.length) return;
  await run({ op: "layer.duplicate", ids }, undefined, t("Duplicate layer"));
}

export async function groupLayers(ids = pro.selection()) {
  const s = editor.summary;
  if (!s || !ids.length) return;
  // Layers from different parents cannot share a group; keep the active one's siblings.
  const parent = parentOf(ids[0])?.id ?? null;
  const same = ids.filter((id) => (parentOf(id)?.id ?? null) === parent);
  const r = await run({ op: "layer.group", ids: same }, undefined, t("Group layers"));
  if (r?.data?.id != null) pro.selectedIds = [r.data.id as number];
}

export async function newGroup() {
  const s = editor.summary;
  if (!s) return;
  try {
    const r = await editor.engine.exec({ op: "layer.group", ids: [] });
    if (r.changed) editor.dirty = true;
    return;
  } catch {
    /* the engine wants members; make one and take it out again */
  }
  const add = await run({ op: "layer.add-pixel", above: s.active ?? undefined });
  const id = add?.data?.id as number | undefined;
  if (id == null) return;
  const g = await run({ op: "layer.group", ids: [id] });
  if (g?.data?.id != null) {
    await run({ op: "layer.delete", ids: [id] });
    pro.selectedIds = [g.data.id as number];
  }
}

export async function ungroup(id = editor.summary?.active) {
  const l = findLayer(editor.summary?.layers ?? [], id);
  if (!l || l.kind !== "group") return;
  await run({ op: "layer.ungroup", id: l.id }, undefined, t("Ungroup layers"));
}

export async function mergeSelected() {
  const ids = pro.selection();
  const a = activeLayer();
  if (!a) return;
  if (ids.length <= 1) {
    await run({ op: "layer.merge-down", id: a.id }, undefined, t("Merge down"));
    return;
  }
  // Merge Layers: group the selection, then rasterise the group into one layer.
  const g = await run({ op: "layer.group", ids }, undefined, t("Merge layers"));
  const gid = g?.data?.id as number | undefined;
  if (gid != null) await run({ op: "layer.rasterize", id: gid }, undefined, t("Merge layers"));
}

export async function setProps(id: LayerId, props: Record<string, unknown>, label?: string) {
  return run({ op: "layer.props", id, ...props }, undefined, label);
}

export async function toggleClip(id = editor.summary?.active) {
  const l = findLayer(editor.summary?.layers ?? [], id);
  if (!l) return;
  await setProps(l.id, { clip: !l.clip });
}

export async function arrange(direction: "up" | "down" | "top" | "bottom") {
  const a = activeLayer();
  if (!a) return;
  await run({ op: "layer.reorder", id: a.id, direction }, undefined, t("Arrange"));
}

export async function addMask(from: "reveal-all" | "hide-all" | "selection" | "hide-selection", id = editor.summary?.active) {
  if (id == null) return;
  await run({ op: "layer.add-mask", id, from }, undefined, t("Layer mask"));
}

// ---------------------------------------------------------------------------
// Smart objects

export async function convertToSmart(ids = pro.selection()) {
  if (!ids.length) return;
  const r = await run({ op: "layer.convert-to-smart", ids }, undefined, t("Convert to Smart Object"));
  if (r?.data?.id != null) pro.selectedIds = [r.data.id as number];
  return r;
}

export async function replaceSmartContents(id = editor.summary?.active) {
  if (id == null) return;
  const [file] = await pickFiles();
  if (!file) return;
  const img = await decodeImage(file);
  await run({ op: "layer.smart-replace", id, width: img.width, height: img.height }, new Uint8Array(img.data.buffer), t("Replace Contents"));
}

export async function unpackSmart(id = editor.summary?.active) {
  if (id == null) return;
  await run({ op: "layer.smart-unpack", id }, undefined, t("Convert to Layers"));
}

export async function setSmartFilter(id: LayerId, index: number, patch: Record<string, unknown>) {
  return run({ op: "layer.smart-filter-set", id, index, ...patch }, undefined, t("Smart filter"));
}

export async function removeSmartFilter(id: LayerId, index: number) {
  return run({ op: "layer.smart-filter-remove", id, index }, undefined, t("Smart filter"));
}

// ---------------------------------------------------------------------------
// Destructive adjustments (Image › Adjustments)

/** Apply an adjustment to the active layer's pixels. Falls back to an adjustment layer merged down. */
export async function applyAdjustment(adjustment: Adjustment): Promise<boolean> {
  const a = activeLayer();
  try {
    const r = await editor.engine.exec({ op: "filter.apply-adjustment", adjustment, ...(a ? { id: a.id } : {}) });
    if (r.changed) editor.dirty = true;
    return true;
  } catch (e) {
    if (!isUnknownOp(e)) {
      editor.error(e);
      return false;
    }
  }
  if (!a || a.kind !== "pixel") {
    editor.toast(t("Select a pixel layer to apply an adjustment to."), "info");
    return false;
  }
  const add = await run({ op: "layer.add-adjustment", adjustment, above: a.id });
  const id = add?.data?.id as number | undefined;
  if (id == null) return false;
  const m = await run({ op: "layer.merge-down", id });
  return !!m;
}

/** Photoshop's Auto Tone / Contrast / Color as Levels. */
export async function autoLevels(mode: "tone" | "contrast" | "color") {
  let levels: Record<string, unknown> | null = null;
  try {
    const r = await editor.engine.exec({ op: "analyze.auto-levels", mode, id: editor.summary?.active ?? undefined });
    levels = (r.data as { levels?: Record<string, unknown> } | null)?.levels ?? null;
  } catch {
    const { histogram } = await import("./engine.svelte");
    const h = await histogram({ id: activeLayer()?.kind === "pixel" ? activeLayer()!.id : null });
    levels = localAutoLevels(h, mode);
  }
  if (!levels) return;
  await applyAdjustment({ kind: "levels", ...levels });
}

function clipPoints(arr: number[], frac = 0.001): [number, number] {
  const total = arr.reduce((a, b) => a + b, 0) || 1;
  let acc = 0;
  let lo = 0;
  for (let i = 0; i < 256; i++) {
    acc += arr[i];
    if (acc / total > frac) {
      lo = i;
      break;
    }
  }
  acc = 0;
  let hi = 255;
  for (let i = 255; i >= 0; i--) {
    acc += arr[i];
    if (acc / total > frac) {
      hi = i;
      break;
    }
  }
  return hi - lo < 8 ? [0, 255] : [lo, hi];
}

function localAutoLevels(h: { r: number[]; g: number[]; b: number[]; l: number[] }, mode: "tone" | "contrast" | "color") {
  const id = { in_black: 0, in_white: 255, gamma: 1, out_black: 0, out_white: 255 };
  const ch = (arr: number[]) => {
    const [lo, hi] = clipPoints(arr);
    return { ...id, in_black: lo, in_white: hi };
  };
  if (mode === "contrast") return { master: ch(h.l), red: id, green: id, blue: id };
  return { master: id, red: ch(h.r), green: ch(h.g), blue: ch(h.b) };
}
