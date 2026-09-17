// Every Pro command in one registry: the menu bar reads labels, shortcuts,
// enabled and checked states from here, and the shortcut handler runs them.

import { allLayers } from "../engine/types";
import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import { openFile, pickFiles, exportBlob, saveBlob } from "../lib/io";
import * as ai from "../lib/ai";
import { matchShortcut } from "../ui/platform";
import { SEP, tidy, type MenuBarMenu, type MenuEntry, type MenuItem } from "../ui/menu";
import { paintTarget } from "../ui/paint-target.svelte";
import { ADJUSTMENT_KINDS, defaultAdjustment } from "./adjustments";
import { copy, paste, placeEmbedded } from "./clipboard";
import { run, runAi } from "./engine.svelte";
import { FILTER_GROUPS, FILTERS, buildCommand, type FilterDef } from "./filters";
import * as ops from "./layer-ops";
import { pro, type PanelId } from "./state.svelte";
import { selectTool } from "./tools.svelte";
import { EFFECTS, activeEffects, type ProLayer } from "./types";

export interface Action {
  id: string;
  label: string | (() => string);
  shortcut?: string;
  /** Show the shortcut but let the browser deliver the event (paste). */
  passive?: boolean;
  run: () => unknown;
  enabled?: () => boolean;
  checked?: () => boolean;
  hint?: string | (() => string);
}

// ---------------------------------------------------------------------------
// PSD and project saving come from lib/psd-io.ts once it exists.

const psdModules = import.meta.glob("../lib/psd-io.ts");

export const fileBridge = $state({
  savePsd: null as null | (() => Promise<unknown>),
  saveProject: null as null | (() => Promise<unknown>),
});

export async function loadFileBridge() {
  const loader = Object.values(psdModules)[0];
  if (!loader) return;
  try {
    const mod = (await loader()) as Record<string, unknown>;
    const fns = Object.entries(mod).filter(([, v]) => typeof v === "function") as [string, (...a: unknown[]) => Promise<unknown>][];
    const psd = fns.find(([k]) => /^(save|export|download)(as)?psd$/i.test(k)) ?? fns.find(([k]) => /psd/i.test(k) && /save|export/i.test(k));
    const proj = fns.find(([k]) => /^(save|export|download)project$/i.test(k)) ?? fns.find(([k]) => /project/i.test(k) && /save|export/i.test(k));
    if (psd) fileBridge.savePsd = () => psd[1]();
    if (proj) fileBridge.saveProject = () => proj[1]();
  } catch (e) {
    console.warn("lib/psd-io.ts failed to load", e);
  }
}

// ---------------------------------------------------------------------------

const hasDoc = () => editor.hasDocument && !!editor.summary;
const hasSel = () => hasDoc() && !!editor.summary?.selection;
const active = () => editor.active as ProLayer | null;
const activeIs = (...kinds: string[]) => {
  const a = active();
  return !!a && kinds.includes(a.kind);
};
const pixelish = () => activeIs("pixel", "smart");
const canUndo = () => (editor.summary?.history.undo.length ?? 0) > 0;
const canRedo = () => (editor.summary?.history.redo.length ?? 0) > 0;

let lastToggleWasUndo = false;
let savedSelection: { x: number; y: number; w: number; h: number; bytes: Uint8Array } | null = null;

/** Open files, each in its own tab when a document is already open. */
export async function openFiles(files: File[]) {
  for (const f of files) {
    let tab: number | null = null;
    if (editor.hasDocument) tab = await editor.newTab();
    const ok = await openFile(f);
    if (!ok && tab != null) await editor.closeTab(tab);
  }
}

async function open() {
  const files = await pickFiles(undefined, true);
  await openFiles(files);
}

async function place() {
  const files = await pickFiles(undefined, true);
  for (const f of files) await placeEmbedded(f);
}

async function quickExportPng() {
  const blob = await exportBlob({ format: "png" });
  if (await saveBlob(blob, `${editor.fileName}.png`)) editor.dirty = false;
}

export async function closeDocument(id = editor.currentTab) {
  const tab = editor.tabs.find((x) => x.id === id);
  const dirty = id === editor.currentTab ? editor.dirty : !!tab?.dirty;
  const name = id === editor.currentTab ? editor.fileName : (tab?.name ?? "");
  if (dirty && !window.confirm(t("Close {name} without saving? Your changes will be lost.", { name }))) return;
  pro.selectedIds = [];
  await editor.closeTab(id);
}

async function deselect() {
  const s = editor.summary;
  if (!s?.selection) return;
  const b = s.selection.bounds;
  try {
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    const w = Math.min(s.width, b.x + b.w) - x;
    const h = Math.min(s.height, b.y + b.h) - y;
    if (w > 0 && h > 0) savedSelection = { x, y, w, h, bytes: await editor.engine.call<Uint8Array>("selection_region", x, y, w, h) };
  } catch {
    savedSelection = null;
  }
  await run({ op: "select.none" });
}

async function reselect() {
  const s = savedSelection;
  if (!s) return;
  await run({ op: "select.mask", x: s.x, y: s.y, width: s.w, height: s.h, mode: "replace", label: t("Reselect") }, s.bytes.slice());
}

async function cropToSelection() {
  const b = editor.summary?.selection?.bounds;
  if (!b) return;
  await run({ op: "image.crop", x: b.x, y: b.y, width: b.w, height: b.h, delete_cropped: true }, undefined, t("Crop"));
}

async function toggleLastState() {
  if (lastToggleWasUndo && canRedo()) {
    await editor.redo();
    lastToggleWasUndo = false;
  } else if (canUndo()) {
    await editor.undo();
    lastToggleWasUndo = true;
  }
}

async function transformLayer(kind: "rot180" | "rot90cw" | "rot90ccw" | "fliph" | "flipv") {
  const a = active();
  const b = a?.bounds;
  if (!a || !b) return;
  const cx = b.x + b.w / 2;
  const cy = b.y + b.h / 2;
  const m = { rot180: [-1, 0, 0, -1], rot90cw: [0, 1, -1, 0], rot90ccw: [0, -1, 1, 0], fliph: [-1, 0, 0, 1], flipv: [1, 0, 0, -1] }[kind];
  const [ma, mb, mc, md] = m;
  const matrix = { a: ma, b: mb, c: mc, d: md, e: cx - ma * cx - mc * cy, f: cy - mb * cx - md * cy };
  await run({ op: "transform.layer", ids: pro.selection(), matrix }, undefined, t("Transform"));
}

function toggleView(key: "rulers" | "extras" | "pixelGrid" | "showGuides") {
  pro.layout[key] = !pro.layout[key];
  pro.save();
}

async function aiSelect(label: string, fn: () => Promise<void>) {
  await runAi(label, fn);
}

async function toggleLayerStyle() {
  const a = active();
  if (!a?.effects) return;
  await run({ op: "layer.set-effects", id: a.id, effects: { ...a.effects, enabled: !a.effects.enabled } });
  await run({ op: "edit.seal" });
}

/** Filter menu: dialogs for filters with parameters, direct for the rest. */
export async function runFilter(def: FilterDef) {
  if (!hasDoc()) return;
  const a = active();
  if (def.params.length) {
    pro.open({ kind: "filter", def, smart: a?.kind === "smart" ? { id: a.id } : undefined });
    return;
  }
  const cmd = buildCommand(def, {});
  if (a?.kind === "smart") {
    await run({ op: "layer.smart-filter-add", id: a.id, filter: cmd }, undefined, def.label);
  } else {
    await run(cmd, undefined, def.label);
  }
  pro.lastFilter = { def, params: {} };
}

async function lastFilter() {
  const lf = pro.lastFilter;
  if (!lf) return;
  const a = active();
  const cmd = buildCommand(lf.def, lf.params, lf.def.op === "filter.clouds" ? { fg: editor.primary, bg: editor.secondary } : {});
  if (a?.kind === "smart") await run({ op: "layer.smart-filter-add", id: a.id, filter: cmd }, undefined, lf.def.label);
  else await run(cmd, undefined, lf.def.label);
}

function adjustmentDialog(kind: string) {
  if (!hasDoc()) return;
  if (kind === "invert") {
    void run({ op: "filter.invert" }, undefined, t("Invert")).then((r) => {
      if (!r) void ops.applyAdjustment({ kind: "invert" });
    });
    return;
  }
  pro.open({ kind: "adjustment", adjKind: kind, initial: defaultAdjustment(kind) });
}

function layerStyle(section?: string) {
  const a = active();
  if (!a) return;
  pro.open({ kind: "layer-style", id: a.id, section });
}

export const ACTIONS: Record<string, Action> = {};

function def(a: Action) {
  ACTIONS[a.id] = a;
}

// File
def({ id: "file.new", label: t("New…"), shortcut: "Mod+N", run: () => pro.open({ kind: "new" }) });
def({ id: "file.open", label: t("Open…"), shortcut: "Mod+O", run: open });
def({ id: "file.place", label: t("Place Embedded…"), run: place, enabled: hasDoc });
def({ id: "file.close", label: t("Close"), shortcut: "Mod+W", run: () => closeDocument(), enabled: () => hasDoc() || editor.tabs.length > 1 });
def({
  id: "file.save-project",
  label: t("Save Project…"),
  shortcut: "Mod+S",
  run: () => fileBridge.saveProject?.(),
  enabled: () => hasDoc() && !!fileBridge.saveProject,
  hint: t("Project files arrive with the PSD and project workstream."),
});
def({
  id: "file.save-psd",
  label: t("Save as PSD…"),
  shortcut: "Shift+Mod+S",
  run: () => fileBridge.savePsd?.(),
  enabled: () => hasDoc() && !!fileBridge.savePsd,
  hint: t("PSD saving arrives with the PSD and project workstream."),
});
def({ id: "file.quick-export", label: t("Quick Export as PNG"), shortcut: "Shift+Alt+Mod+'", run: quickExportPng, enabled: hasDoc });
def({ id: "file.export-as", label: t("Export As…"), shortcut: "Alt+Shift+Mod+W", run: () => pro.open({ kind: "export" }), enabled: hasDoc });

// Edit
def({
  id: "edit.undo",
  label: () => (canUndo() ? t("Undo {what}", { what: editor.summary!.history.undo.at(-1)! }) : t("Undo")),
  shortcut: "Mod+Z",
  run: () => {
    lastToggleWasUndo = true;
    return editor.undo();
  },
  enabled: canUndo,
});
def({
  id: "edit.redo",
  label: () => (canRedo() ? t("Redo {what}", { what: editor.summary!.history.redo[0] }) : t("Redo")),
  shortcut: "Shift+Mod+Z",
  run: () => editor.redo(),
  enabled: canRedo,
});
def({ id: "edit.toggle-last", label: t("Toggle Last State"), shortcut: "Alt+Mod+Z", run: toggleLastState, enabled: () => canUndo() || canRedo() });
def({ id: "edit.step-forward", label: t("Step Forward"), run: () => editor.redo(), enabled: canRedo });
def({ id: "edit.step-backward", label: t("Step Backward"), run: () => editor.undo(), enabled: canUndo });
def({ id: "edit.cut", label: t("Cut"), shortcut: "Mod+X", run: () => copy(false, true), enabled: () => hasDoc() && pixelish() });
def({ id: "edit.copy", label: t("Copy"), shortcut: "Mod+C", run: () => copy(false), enabled: hasDoc });
def({ id: "edit.copy-merged", label: t("Copy Merged"), shortcut: "Shift+Mod+C", run: () => copy(true), enabled: hasDoc });
def({ id: "edit.paste", label: t("Paste"), shortcut: "Mod+V", passive: true, run: () => paste(false) });
def({ id: "edit.paste-in-place", label: t("Paste in Place"), shortcut: "Shift+Mod+V", run: () => paste(true), enabled: hasDoc });
def({ id: "edit.clear", label: t("Clear"), shortcut: "Delete", run: () => run({ op: "layer.clear" }, undefined, t("Clear")), enabled: () => hasSel() && pixelish() });
def({ id: "edit.fill", label: t("Fill…"), shortcut: "Shift+F5", run: () => pro.open({ kind: "fill" }), enabled: () => hasDoc() && activeIs("pixel") });
def({
  id: "edit.content-aware-fill",
  label: t("Content-Aware Fill"),
  run: () => run({ op: "filter.content-aware-fill", sample: "auto" }, undefined, t("Content-Aware Fill")),
  enabled: () => hasSel() && activeIs("pixel"),
  hint: t("Make a selection first."),
});
def({ id: "edit.free-transform", label: t("Free Transform"), shortcut: "Mod+T", run: () => selectTool("transform"), enabled: () => hasDoc() && !!active() });
def({ id: "edit.rot180", label: t("Rotate 180°"), run: () => transformLayer("rot180"), enabled: () => hasDoc() && !!active()?.bounds });
def({ id: "edit.rot90cw", label: t("Rotate 90° Clockwise"), run: () => transformLayer("rot90cw"), enabled: () => hasDoc() && !!active()?.bounds });
def({ id: "edit.rot90ccw", label: t("Rotate 90° Counter Clockwise"), run: () => transformLayer("rot90ccw"), enabled: () => hasDoc() && !!active()?.bounds });
def({ id: "edit.fliph", label: t("Flip Horizontal"), run: () => transformLayer("fliph"), enabled: () => hasDoc() && !!active()?.bounds });
def({ id: "edit.flipv", label: t("Flip Vertical"), run: () => transformLayer("flipv"), enabled: () => hasDoc() && !!active()?.bounds });
def({ id: "edit.shortcuts", label: t("Keyboard Shortcuts…"), shortcut: "Alt+Shift+Mod+K", run: () => pro.open({ kind: "shortcuts" }) });

// Image
def({ id: "image.auto-tone", label: t("Auto Tone"), shortcut: "Shift+Mod+L", run: () => ops.autoLevels("tone"), enabled: () => hasDoc() && pixelish() });
def({ id: "image.auto-contrast", label: t("Auto Contrast"), shortcut: "Alt+Shift+Mod+L", run: () => ops.autoLevels("contrast"), enabled: () => hasDoc() && pixelish() });
def({ id: "image.auto-color", label: t("Auto Color"), shortcut: "Shift+Mod+B", run: () => ops.autoLevels("color"), enabled: () => hasDoc() && pixelish() });
def({ id: "image.size", label: t("Image Size…"), shortcut: "Alt+Mod+I", run: () => pro.open({ kind: "image-size" }), enabled: hasDoc });
def({ id: "image.canvas-size", label: t("Canvas Size…"), shortcut: "Alt+Mod+C", run: () => pro.open({ kind: "canvas-size" }), enabled: hasDoc });
def({ id: "image.rot180", label: t("180°"), run: () => run({ op: "image.rotate", turns: 2 }, undefined, t("Rotate canvas")), enabled: hasDoc });
def({ id: "image.rot90cw", label: t("90° Clockwise"), run: () => run({ op: "image.rotate", turns: 1 }, undefined, t("Rotate canvas")), enabled: hasDoc });
def({ id: "image.rot90ccw", label: t("90° Counter Clockwise"), run: () => run({ op: "image.rotate", turns: 3 }, undefined, t("Rotate canvas")), enabled: hasDoc });
def({ id: "image.rot-arbitrary", label: t("Arbitrary…"), run: () => pro.open({ kind: "rotate" }), enabled: hasDoc });
def({ id: "image.fliph", label: t("Flip Canvas Horizontal"), run: () => run({ op: "image.flip", horizontal: true }, undefined, t("Flip canvas")), enabled: hasDoc });
def({ id: "image.flipv", label: t("Flip Canvas Vertical"), run: () => run({ op: "image.flip", horizontal: false }, undefined, t("Flip canvas")), enabled: hasDoc });
def({ id: "image.crop", label: t("Crop"), run: cropToSelection, enabled: hasSel, hint: t("Make a selection first.") });
def({ id: "image.trim", label: t("Trim"), run: () => run({ op: "image.trim" }, undefined, t("Trim")), enabled: hasDoc });
def({ id: "image.reveal-all", label: t("Reveal All"), run: () => run({ op: "image.reveal-all" }, undefined, t("Reveal All")), enabled: hasDoc });
def({ id: "image.desaturate", label: t("Desaturate"), shortcut: "Shift+Mod+U", run: () => run({ op: "filter.desaturate" }, undefined, t("Desaturate")), enabled: () => hasDoc() && pixelish() });
for (const k of ADJUSTMENT_KINDS) {
  if (!k) continue;
  def({ id: `image.adjust.${k.kind}`, label: k.kind === "develop" ? t("Camera Raw Filter…") : k.kind === "invert" ? k.label : `${k.label}…`, shortcut: k.shortcut, run: () => adjustmentDialog(k.kind), enabled: () => hasDoc() && pixelish() });
  def({ id: `layer.adjust.${k.kind}`, label: `${k.label}…`, run: () => ops.newAdjustmentLayer(k.kind), enabled: hasDoc });
}

// Layer
def({ id: "layer.new", label: t("Layer…"), shortcut: "Shift+Mod+N", run: ops.newLayer, enabled: hasDoc });
def({ id: "layer.new-group", label: t("Group"), run: ops.newGroup, enabled: hasDoc });
def({
  id: "layer.via-copy",
  label: t("Layer via Copy"),
  shortcut: "Mod+J",
  run: () => (hasSel() ? run({ op: "layer.from-selection", cut: false }, undefined, t("Layer via Copy")) : ops.duplicateLayers()),
  enabled: () => hasDoc() && !!active(),
});
def({ id: "layer.via-cut", label: t("Layer via Cut"), shortcut: "Shift+Mod+J", run: () => run({ op: "layer.from-selection", cut: true }, undefined, t("Layer via Cut")), enabled: () => hasSel() && pixelish() });
def({ id: "layer.duplicate", label: t("Duplicate Layer"), run: () => ops.duplicateLayers(), enabled: () => hasDoc() && !!active() });
def({ id: "layer.delete", label: t("Delete Layer"), run: () => ops.deleteLayers(), enabled: () => hasDoc() && !!active() });
def({ id: "layer.rename", label: t("Rename Layer…"), run: () => pro.renameRequest++, enabled: () => hasDoc() && !!active() });
def({ id: "layer.style-blending", label: t("Blending Options…"), run: () => layerStyle("blending"), enabled: () => hasDoc() && !!active() });
for (const e of EFFECTS) def({ id: `layer.style.${e.key}`, label: `${t(e.label)}…`, run: () => layerStyle(e.key), enabled: () => hasDoc() && !!active() && !activeIs("adjustment") });
def({ id: "layer.style-toggle", label: () => (active()?.effects?.enabled === false ? t("Show All Effects") : t("Hide All Effects")), run: toggleLayerStyle, enabled: () => activeEffects(active()?.effects).length > 0 });
def({
  id: "layer.style-clear",
  label: t("Clear Layer Style"),
  run: async () => {
    const a = active();
    if (a) await run({ op: "layer.set-effects", id: a.id, effects: null }, undefined, t("Clear Layer Style"));
    await run({ op: "edit.seal" });
  },
  enabled: () => !!active()?.effects,
});
def({ id: "layer.fill-solid", label: t("Solid Color…"), run: () => pro.open({ kind: "fill-layer", fill: "solid" }), enabled: hasDoc });
def({ id: "layer.fill-gradient", label: t("Gradient…"), run: () => pro.open({ kind: "fill-layer", fill: "gradient" }), enabled: hasDoc });
def({ id: "layer.content-options", label: t("Layer Content Options…"), run: () => pro.showPanel("properties"), enabled: () => activeIs("adjustment", "fill", "text", "shape") });
def({ id: "layer.mask-reveal-all", label: t("Reveal All"), run: () => ops.addMask("reveal-all"), enabled: () => hasDoc() && !!active() && !active()?.mask });
def({ id: "layer.mask-hide-all", label: t("Hide All"), run: () => ops.addMask("hide-all"), enabled: () => hasDoc() && !!active() && !active()?.mask });
def({ id: "layer.mask-reveal-sel", label: t("Reveal Selection"), run: () => ops.addMask("selection"), enabled: () => hasSel() && !!active() && !active()?.mask });
def({ id: "layer.mask-hide-sel", label: t("Hide Selection"), run: () => ops.addMask("hide-selection"), enabled: () => hasSel() && !!active() && !active()?.mask });
def({ id: "layer.mask-delete", label: t("Delete"), run: () => run({ op: "layer.delete-mask", id: active()!.id, apply: false }, undefined, t("Delete mask")), enabled: () => !!active()?.mask });
def({ id: "layer.mask-apply", label: t("Apply"), run: () => run({ op: "layer.delete-mask", id: active()!.id, apply: true }, undefined, t("Apply mask")), enabled: () => !!active()?.mask && activeIs("pixel") });
def({
  id: "layer.mask-toggle",
  label: () => (active()?.mask?.enabled === false ? t("Enable") : t("Disable")),
  run: () => run({ op: "layer.mask-props", id: active()!.id, enabled: !(active()!.mask!.enabled) }, undefined, t("Mask")),
  enabled: () => !!active()?.mask,
});
def({
  id: "layer.mask-link",
  label: () => (active()?.mask?.linked === false ? t("Link") : t("Unlink")),
  run: () => run({ op: "layer.mask-props", id: active()!.id, linked: !(active()!.mask!.linked) }, undefined, t("Mask")),
  enabled: () => !!active()?.mask,
});
def({ id: "layer.clip", label: () => (active()?.clip ? t("Release Clipping Mask") : t("Create Clipping Mask")), shortcut: "Alt+Mod+G", run: () => ops.toggleClip(), enabled: () => hasDoc() && !!active() });
def({ id: "layer.smart-convert", label: t("Convert to Smart Object"), run: () => ops.convertToSmart(), enabled: () => hasDoc() && !!active() });
def({ id: "layer.smart-replace", label: t("Replace Contents…"), run: () => ops.replaceSmartContents(), enabled: () => activeIs("smart") });
def({ id: "layer.smart-unpack", label: t("Convert to Layers"), run: () => ops.unpackSmart(), enabled: () => activeIs("smart") });
def({ id: "layer.rasterize", label: t("Rasterize"), run: () => run({ op: "layer.rasterize", id: active()!.id }, undefined, t("Rasterize")), enabled: () => activeIs("text", "shape", "fill", "smart", "group") });
def({ id: "layer.group", label: t("Group Layers"), shortcut: "Mod+G", run: () => ops.groupLayers(), enabled: () => hasDoc() && !!active() });
def({ id: "layer.ungroup", label: t("Ungroup Layers"), shortcut: "Shift+Mod+G", run: () => ops.ungroup(), enabled: () => activeIs("group") });
def({
  id: "layer.hide",
  label: () => (active()?.visible === false ? t("Show Layers") : t("Hide Layers")),
  shortcut: "Mod+,",
  run: async () => {
    for (const id of pro.selection()) {
      const l = allLayers(editor.summary!.layers).find((x) => x.id === id);
      if (l) await ops.setProps(id, { visible: !l.visible });
    }
  },
  enabled: () => hasDoc() && !!active(),
});
def({ id: "layer.front", label: t("Bring to Front"), shortcut: "Shift+Mod+]", run: () => ops.arrange("top"), enabled: () => hasDoc() && !!active() });
def({ id: "layer.forward", label: t("Bring Forward"), shortcut: "Mod+]", run: () => ops.arrange("up"), enabled: () => hasDoc() && !!active() });
def({ id: "layer.backward", label: t("Send Backward"), shortcut: "Mod+[", run: () => ops.arrange("down"), enabled: () => hasDoc() && !!active() });
def({ id: "layer.back", label: t("Send to Back"), shortcut: "Shift+Mod+[", run: () => ops.arrange("bottom"), enabled: () => hasDoc() && !!active() });
def({
  id: "layer.lock-all",
  label: () => (active()?.locks.all ? t("Unlock Layer") : t("Lock All Layer Properties")),
  shortcut: "Mod+/",
  run: () => {
    const a = active()!;
    return ops.setProps(a.id, { locks: { ...a.locks, all: !a.locks.all } });
  },
  enabled: () => hasDoc() && !!active(),
});
def({ id: "layer.merge", label: () => (pro.selection().length > 1 ? t("Merge Layers") : t("Merge Down")), shortcut: "Mod+E", run: ops.mergeSelected, enabled: () => hasDoc() && !!active() });
def({ id: "layer.merge-visible", label: t("Merge Visible"), shortcut: "Shift+Mod+E", run: () => run({ op: "layer.merge-visible" }, undefined, t("Merge Visible")), enabled: hasDoc });
def({ id: "layer.flatten", label: t("Flatten Image"), run: () => run({ op: "layer.flatten" }, undefined, t("Flatten Image")), enabled: hasDoc });
def({ id: "layer.stamp", label: t("Stamp Visible"), shortcut: "Alt+Shift+Mod+E", run: () => run({ op: "layer.stamp-visible" }, undefined, t("Stamp Visible")), enabled: hasDoc });

// Type
def({ id: "type.rasterize", label: t("Rasterize Type Layer"), run: () => run({ op: "layer.rasterize", id: active()!.id }, undefined, t("Rasterize")), enabled: () => activeIs("text") });
def({ id: "type.properties", label: t("Character and Paragraph…"), run: () => pro.showPanel("properties"), enabled: () => activeIs("text") });
def({ id: "type.tool", label: t("Type Tool"), shortcut: "T", run: () => selectTool("text"), enabled: hasDoc });

// Select
def({ id: "select.all", label: t("All"), shortcut: "Mod+A", run: () => run({ op: "select.all" }), enabled: hasDoc });
def({ id: "select.none", label: t("Deselect"), shortcut: "Mod+D", run: deselect, enabled: hasSel });
def({ id: "select.reselect", label: t("Reselect"), shortcut: "Shift+Mod+D", run: reselect, enabled: () => hasDoc() && !editor.summary?.selection && !!savedSelection });
def({ id: "select.inverse", label: t("Inverse"), shortcut: "Shift+Mod+I", run: () => run({ op: "select.invert" }), enabled: hasSel });
def({
  id: "select.all-layers",
  label: t("All Layers"),
  shortcut: "Alt+Mod+A",
  run: () => (pro.selectedIds = (editor.summary?.layers ?? []).map((l) => l.id)),
  enabled: hasDoc,
});
def({ id: "select.deselect-layers", label: t("Deselect Layers"), run: () => (pro.selectedIds = []), enabled: hasDoc });
def({ id: "select.color-range", label: t("Color Range…"), run: () => pro.open({ kind: "color-range" }), enabled: hasDoc });
def({ id: "select.subject", label: t("Subject"), run: () => aiSelect(t("Select subject"), ai.selectSubject), enabled: hasDoc });
def({ id: "select.sky", label: t("Sky"), run: () => undefined, enabled: () => false, hint: t("Sky selection arrives with the local AI models.") });
def({ id: "select.background", label: t("Background"), run: () => aiSelect(t("Select background"), ai.selectBackground), enabled: hasDoc });
def({ id: "select.mask", label: t("Select and Mask…"), shortcut: "Alt+Mod+R", run: () => pro.open({ kind: "select-mask" }), enabled: hasSel });
def({ id: "select.border", label: t("Border…"), run: () => pro.open({ kind: "modify", op: "border" }), enabled: hasSel });
def({ id: "select.smooth", label: t("Smooth…"), run: () => pro.open({ kind: "modify", op: "smooth" }), enabled: hasSel });
def({ id: "select.expand", label: t("Expand…"), run: () => pro.open({ kind: "modify", op: "grow" }), enabled: hasSel });
def({ id: "select.contract", label: t("Contract…"), run: () => pro.open({ kind: "modify", op: "shrink" }), enabled: hasSel });
def({ id: "select.feather", label: t("Feather…"), shortcut: "Shift+F6", run: () => pro.open({ kind: "modify", op: "feather" }), enabled: hasSel });
def({ id: "select.similar", label: t("Similar"), run: () => run({ op: "select.similar", tolerance: 32 }, undefined, t("Similar")), enabled: hasSel });
def({ id: "select.layer-alpha", label: t("Load Selection from Layer"), run: () => run({ op: "select.layer-alpha", id: active()!.id, mode: "replace" }), enabled: () => hasDoc() && !!active() });
def({
  id: "select.quick-mask",
  label: t("Edit in Quick Mask Mode"),
  shortcut: "Q",
  run: () => (paintTarget.quickMask = !paintTarget.quickMask),
  checked: () => paintTarget.quickMask,
  enabled: hasDoc,
});

// Filter
def({ id: "filter.last", label: () => (pro.lastFilter ? t("Last Filter: {name}", { name: pro.lastFilter.def.label }) : t("Last Filter")), shortcut: "Alt+Mod+F", run: lastFilter, enabled: () => hasDoc() && !!pro.lastFilter });
def({ id: "filter.convert-smart", label: t("Convert for Smart Filters"), run: () => ops.convertToSmart([active()!.id]), enabled: () => hasDoc() && activeIs("pixel", "text", "shape") });
for (const f of FILTERS) def({ id: `filter.${f.op}`, label: f.params.length ? `${f.label}…` : f.label, run: () => runFilter(f), enabled: () => hasDoc() && (pixelish() || activeIs("text", "shape")) });
def({ id: "filter.lens-correct", label: t("Lens Correction…"), run: () => runFilter(LENS_CORRECTION), enabled: () => hasDoc() && pixelish() });
def({ id: "ai.remove-bg", label: t("Remove Background"), run: () => runAi(t("Remove background"), ai.removeBackground), enabled: hasDoc });
def({ id: "ai.upscale2", label: t("Upscale 2×"), run: () => runAi(t("Upscale"), () => ai.upscale(2)), enabled: hasDoc });
def({ id: "ai.upscale4", label: t("Upscale 4×"), run: () => runAi(t("Upscale"), () => ai.upscale(4)), enabled: hasDoc });
def({ id: "ai.denoise", label: t("Denoise"), run: () => runAi(t("Denoise"), () => ai.denoise("medium")), enabled: hasDoc });
def({ id: "ai.jpeg", label: t("Remove JPEG Artifacts"), run: () => runAi(t("JPEG clean-up"), ai.removeJpegArtifacts), enabled: hasDoc });
def({ id: "ai.faces", label: t("Restore Faces"), run: () => runAi(t("Face restoration"), ai.restoreFaces), enabled: hasDoc });
def({ id: "ai.colorize", label: t("Colorize"), run: () => runAi(t("Colorize"), ai.colorize), enabled: hasDoc });
def({ id: "ai.blur-bg", label: t("Blur Background"), run: () => runAi(t("Blur background"), () => ai.blurBackground()), enabled: hasDoc });

export const LENS_CORRECTION: FilterDef = {
  op: "transform.lens-correct",
  label: t("Lens Correction"),
  group: "top",
  params: [
    { key: "distortion", label: t("Remove distortion"), type: "number", min: -100, max: 100, step: 1, default: 0 },
    { key: "chromatic_rc", label: t("Fix red/cyan fringe"), type: "number", min: -100, max: 100, step: 1, default: 0 },
    { key: "chromatic_by", label: t("Fix blue/yellow fringe"), type: "number", min: -100, max: 100, step: 1, default: 0 },
    { key: "vignette", label: t("Vignette amount"), type: "number", min: -100, max: 100, step: 1, default: 0 },
    { key: "vignette_midpoint", label: t("Vignette midpoint"), type: "number", min: 0, max: 100, step: 1, default: 50 },
    { key: "scale", label: t("Scale"), type: "number", min: 50, max: 150, step: 1, unit: "%", default: 100 },
  ],
};

// View
def({ id: "view.zoom-in", label: t("Zoom In"), shortcut: "Mod+=", run: () => editor.zoomStep(1), enabled: hasDoc });
def({ id: "view.zoom-out", label: t("Zoom Out"), shortcut: "Mod+-", run: () => editor.zoomStep(-1), enabled: hasDoc });
def({ id: "view.fit", label: t("Fit on Screen"), shortcut: "Mod+0", run: () => editor.fit(), enabled: hasDoc });
def({ id: "view.100", label: t("100%"), shortcut: "Mod+1", run: () => editor.zoomAt(1), enabled: hasDoc });
def({ id: "view.200", label: t("200%"), run: () => editor.zoomAt(2), enabled: hasDoc });
def({ id: "view.extras", label: t("Extras"), shortcut: "Mod+H", run: () => toggleView("extras"), checked: () => pro.layout.extras });
def({ id: "view.pixel-grid", label: t("Pixel Grid"), run: () => toggleView("pixelGrid"), checked: () => pro.layout.pixelGrid });
def({ id: "view.guides", label: t("Guides"), shortcut: "Mod+;", run: () => toggleView("showGuides"), checked: () => pro.layout.showGuides });
def({ id: "view.rulers", label: t("Rulers"), shortcut: "Mod+R", run: () => toggleView("rulers"), checked: () => pro.layout.rulers });
def({
  id: "view.new-guide",
  label: t("New Guide at Centre"),
  run: () => {
    const s = editor.summary!;
    pro.guides = [...pro.guides, { axis: "x", pos: Math.round(s.width / 2) }];
  },
  enabled: hasDoc,
});
def({ id: "view.clear-guides", label: t("Clear Guides"), run: () => (pro.guides = []), enabled: () => pro.guides.length > 0 });

// Window
const PANEL_LABELS: [PanelId, string, string?][] = [
  ["adjustments", t("Adjustments")],
  ["color", t("Color"), "F6"],
  ["histogram", t("Histogram")],
  ["history", t("History")],
  ["info", t("Info"), "F8"],
  ["layers", t("Layers"), "F7"],
  ["navigator", t("Navigator")],
  ["properties", t("Properties")],
  ["swatches", t("Swatches")],
];
for (const [id, label, sc] of PANEL_LABELS) def({ id: `window.${id}`, label, shortcut: sc, run: () => pro.togglePanel(id), checked: () => pro.layout.visible[id] });
def({
  id: "window.reset",
  label: t("Reset Essentials"),
  run: () => {
    try {
      localStorage.removeItem("ops.pro.layout");
    } catch {
      /* ignore */
    }
    location.reload();
  },
});
def({ id: "window.dock", label: t("Show Panels"), shortcut: "Tab", run: () => ((pro.layout.dockCollapsed = !pro.layout.dockCollapsed), pro.save()), checked: () => !pro.layout.dockCollapsed });

// Help
def({ id: "help.shortcuts", label: t("Keyboard Shortcuts"), run: () => pro.open({ kind: "shortcuts" }) });
def({ id: "help.about", label: t("About OpenPhotoEdit"), run: () => pro.open({ kind: "about" }) });
def({ id: "help.simple", label: t("Switch to Simple Mode"), run: () => editor.setProfile("lite") });

// ---------------------------------------------------------------------------
// Menus

function item(id: string, extra: Partial<MenuItem> = {}): MenuItem {
  const a = ACTIONS[id];
  const enabled = a.enabled ? a.enabled() : true;
  const hint = typeof a.hint === "function" ? a.hint() : a.hint;
  return {
    id,
    label: typeof a.label === "function" ? a.label() : a.label,
    shortcut: a.shortcut,
    disabled: !enabled,
    hint: !enabled ? hint : undefined,
    checked: a.checked ? a.checked() : undefined,
    testid: `action-${id}`,
    run: () => void a.run(),
    ...extra,
  };
}

const sub = (label: string, entries: () => MenuEntry[], extra: Partial<MenuItem> = {}): MenuItem => ({ label, submenu: entries, ...extra });

export const MENUS: MenuBarMenu[] = [
  {
    id: "file",
    label: t("File"),
    items: () =>
      tidy([
        item("file.new"),
        item("file.open"),
        SEP,
        item("file.close"),
        item("file.save-project"),
        item("file.save-psd"),
        SEP,
        sub(t("Export"), () => [item("file.quick-export"), item("file.export-as")]),
        SEP,
        item("file.place"),
        SEP,
        { label: t("File Info…"), disabled: true, hint: t("Metadata editing is not built yet.") },
      ]),
  },
  {
    id: "edit",
    label: t("Edit"),
    items: () =>
      tidy([
        item("edit.undo"),
        item("edit.redo"),
        item("edit.toggle-last"),
        item("edit.step-forward"),
        item("edit.step-backward"),
        SEP,
        item("edit.cut"),
        item("edit.copy"),
        item("edit.copy-merged"),
        item("edit.paste"),
        item("edit.paste-in-place"),
        item("edit.clear"),
        SEP,
        item("edit.fill"),
        { label: t("Stroke…"), disabled: true, hint: t("Stroke a selection with a layer style Stroke for now.") },
        item("edit.content-aware-fill"),
        SEP,
        item("edit.free-transform"),
        sub(t("Transform"), () => [item("edit.rot180"), item("edit.rot90cw"), item("edit.rot90ccw"), SEP, item("edit.fliph"), item("edit.flipv")], { disabled: !hasDoc() }),
        SEP,
        item("edit.shortcuts"),
      ]),
  },
  {
    id: "image",
    label: t("Image"),
    items: () =>
      tidy([
        sub(t("Mode"), () => [
          { label: t("Bitmap"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
          { label: t("Grayscale"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
          { label: t("RGB Color"), checked: true },
          { label: t("CMYK Color"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
          { label: t("Lab Color"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
          SEP,
          { label: t("8 Bits/Channel"), checked: true },
          { label: t("16 Bits/Channel"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
          { label: t("32 Bits/Channel"), disabled: true, hint: t("Only 8-bit RGB is supported for now.") },
        ]),
        SEP,
        sub(t("Adjustments"), () => [
          ...ADJUSTMENT_KINDS.filter((k) => k?.kind !== "develop").map((k) => (k ? item(`image.adjust.${k.kind}`) : SEP)),
          SEP,
          item("image.desaturate"),
        ], { disabled: !hasDoc() }),
        SEP,
        item("image.auto-tone"),
        item("image.auto-contrast"),
        item("image.auto-color"),
        SEP,
        item("image.size"),
        item("image.canvas-size"),
        sub(t("Image Rotation"), () => [item("image.rot180"), item("image.rot90cw"), item("image.rot90ccw"), item("image.rot-arbitrary"), SEP, item("image.fliph"), item("image.flipv")], { disabled: !hasDoc() }),
        item("image.crop"),
        item("image.trim"),
        item("image.reveal-all"),
      ]),
  },
  {
    id: "layer",
    label: t("Layer"),
    items: () =>
      tidy([
        sub(t("New"), () => [item("layer.new"), item("layer.new-group"), SEP, item("layer.via-copy"), item("layer.via-cut")], { disabled: !hasDoc() }),
        item("layer.duplicate"),
        item("layer.delete"),
        item("layer.rename"),
        SEP,
        sub(
          t("Layer Style"),
          () => tidy([item("layer.style-blending"), SEP, ...EFFECTS.map((e) => item(`layer.style.${e.key}`)), SEP, item("layer.style-toggle"), item("layer.style-clear")]),
          { disabled: !hasDoc() || !active() },
        ),
        SEP,
        sub(t("New Fill Layer"), () => [item("layer.fill-solid"), item("layer.fill-gradient"), { label: t("Pattern…"), disabled: true, hint: t("Pattern fills are not built yet.") }], { disabled: !hasDoc() }),
        sub(t("New Adjustment Layer"), () => ADJUSTMENT_KINDS.map((k) => (k ? item(`layer.adjust.${k.kind}`) : SEP)), { disabled: !hasDoc(), testid: "menu-new-adjustment" }),
        item("layer.content-options"),
        SEP,
        sub(
          t("Layer Mask"),
          () => [item("layer.mask-reveal-all"), item("layer.mask-hide-all"), item("layer.mask-reveal-sel"), item("layer.mask-hide-sel"), SEP, item("layer.mask-delete"), item("layer.mask-apply"), SEP, item("layer.mask-toggle"), item("layer.mask-link")],
          { disabled: !hasDoc() || !active() },
        ),
        item("layer.clip"),
        SEP,
        sub(t("Smart Objects"), () => [item("layer.smart-convert"), SEP, item("layer.smart-replace"), item("layer.smart-unpack"), SEP, item("layer.rasterize", { label: t("Rasterize") })], { disabled: !hasDoc() }),
        item("layer.rasterize", { label: t("Rasterize Layer") }),
        SEP,
        item("layer.group"),
        item("layer.ungroup"),
        item("layer.hide"),
        SEP,
        sub(t("Arrange"), () => [item("layer.front"), item("layer.forward"), item("layer.backward"), item("layer.back")], { disabled: !hasDoc() || !active() }),
        SEP,
        item("layer.lock-all"),
        SEP,
        item("layer.merge"),
        item("layer.merge-visible"),
        item("layer.flatten"),
        item("layer.stamp"),
      ]),
  },
  {
    id: "type",
    label: t("Type"),
    items: () =>
      tidy([
        item("type.tool"),
        item("type.properties"),
        SEP,
        { label: t("Anti-Alias"), submenu: [{ label: t("Smooth"), checked: true, radio: true }, { label: t("None"), disabled: true, hint: t("Text renders smooth for now.") }] },
        SEP,
        item("type.rasterize"),
      ]),
  },
  {
    id: "select",
    label: t("Select"),
    items: () =>
      tidy([
        item("select.all"),
        item("select.none"),
        item("select.reselect"),
        item("select.inverse"),
        SEP,
        item("select.all-layers"),
        item("select.deselect-layers"),
        SEP,
        item("select.color-range"),
        { label: t("Focus Area…"), disabled: true, hint: t("Focus Area is not built yet.") },
        item("select.subject"),
        item("select.sky"),
        item("select.background"),
        SEP,
        item("select.mask"),
        sub(t("Modify"), () => [item("select.border"), item("select.smooth"), item("select.expand"), item("select.contract"), item("select.feather")], { disabled: !hasSel() }),
        item("select.similar"),
        item("select.layer-alpha"),
        SEP,
        item("select.quick-mask"),
      ]),
  },
  {
    id: "filter",
    label: t("Filter"),
    items: () =>
      tidy([
        item("filter.last"),
        SEP,
        item("filter.convert-smart"),
        SEP,
        sub(t("Neural Filters"), () => [item("ai.remove-bg"), item("ai.blur-bg"), SEP, item("ai.upscale2"), item("ai.upscale4"), item("ai.denoise"), item("ai.jpeg"), SEP, item("ai.faces"), item("ai.colorize")], {
          disabled: !hasDoc(),
        }),
        ACTIONS["image.adjust.develop"] ? item("image.adjust.develop") : null,
        item("filter.lens-correct"),
        { label: t("Liquify…"), disabled: true, hint: t("Liquify arrives with the transform workstream's brush.") },
        SEP,
        ...FILTER_GROUPS.map((g) => sub(g.label, () => FILTERS.filter((f) => f.group === g.id).map((f) => item(`filter.${f.op}`)), { disabled: !hasDoc(), testid: `menu-filter-${g.id}` })),
      ]),
  },
  {
    id: "view",
    label: t("View"),
    items: () =>
      tidy([
        item("view.zoom-in"),
        item("view.zoom-out"),
        item("view.fit"),
        item("view.100"),
        item("view.200"),
        SEP,
        item("view.extras"),
        sub(t("Show"), () => [item("view.pixel-grid"), item("view.guides")]),
        SEP,
        item("view.rulers"),
        SEP,
        item("view.new-guide"),
        item("view.clear-guides"),
      ]),
  },
  {
    id: "window",
    label: t("Window"),
    items: () => tidy([...PANEL_LABELS.map(([id]) => item(`window.${id}`)), SEP, item("window.dock"), item("window.reset")]),
  },
  {
    id: "help",
    label: t("Help"),
    items: () => tidy([item("help.shortcuts"), SEP, item("help.simple"), SEP, item("help.about")]),
  },
];

// ---------------------------------------------------------------------------
// Shortcuts

const BOUND = () => Object.values(ACTIONS).filter((a) => a.shortcut && !a.passive && a.shortcut.length > 1 && a.id !== "window.dock" && a.id !== "select.quick-mask" && a.id !== "type.tool");

let bound: Action[] | null = null;

/** Run the action bound to this key press. Returns true when one ran. */
export function handleShortcut(e: KeyboardEvent): boolean {
  bound ??= BOUND();
  for (const a of bound) {
    if (!matchShortcut(e, a.shortcut!)) continue;
    // Browser-reserved keys: Mod+N/W/T cannot be taken on every platform; try anyway.
    e.preventDefault();
    if (a.enabled && !a.enabled()) return true;
    void a.run();
    return true;
  }
  return false;
}

