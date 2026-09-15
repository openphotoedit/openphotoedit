// Every Lite action by name — the tool search lists them, the prompt bar's
// local planner maps sentences onto them.

import { editor } from "../lib/editor.svelte";
import * as ai from "../lib/ai";
import { t } from "../lib/i18n";
import { pickFiles, openFile } from "../lib/io";
import type { Develop } from "../engine/types";
import { addDevelop, LOOKS } from "./develop";
import { adjustments, isNotReady, lite, type Category } from "./lite.svelte";
import * as ops from "./ops";
import { runFeature } from "./run";
import { liteTools, MARKUP_TOOL_IDS, type RetouchTool } from "./tools.svelte";

export interface Action {
  id: string;
  label: string;
  group: string;
  keywords: string;
  run: () => unknown;
}

export const CATEGORY_LABELS: Record<Category, string> = {
  auto: "Auto",
  adjust: "Adjust",
  crop: "Crop",
  retouch: "Retouch",
  markup: "Markup",
  effects: "Effects",
  enhance: "Enhance",
};

export function openCategory(c: Category) {
  lite.category = c;
  lite.sheetOpen = true;
}

export async function openPhoto() {
  const [f] = await pickFiles();
  if (f) await openFile(f);
}

function markupTool(id: string) {
  liteTools.markup = id;
  openCategory("markup");
}

function retouchTool(id: RetouchTool) {
  liteTools.retouch = id;
  openCategory("retouch");
}

/** Nudge the Adjustments layer by deltas, as one undo step. */
export async function nudge(delta: Partial<Develop>, note?: string) {
  if (!editor.hasDocument) return;
  await adjustments.replace(addDevelop(adjustments.stored, delta));
  if (note) lite.note(note);
}

export function allActions(): Action[] {
  const g = (c: Category) => t(CATEGORY_LABELS[c]);
  const list: Action[] = [
    { id: "open", label: t("Open a photo"), group: t("File"), keywords: "open file import load new photo image", run: openPhoto },
    { id: "export", label: t("Export"), group: t("File"), keywords: "export save download share jpeg png webp", run: () => (lite.exportOpen = true) },
    { id: "copy", label: t("Copy to clipboard"), group: t("File"), keywords: "copy clipboard paste", run: () => (lite.exportOpen = true) },
    { id: "undo", label: t("Undo"), group: t("Steps"), keywords: "undo back revert", run: () => lite.undo() },
    { id: "redo", label: t("Redo"), group: t("Steps"), keywords: "redo forward", run: () => lite.redo() },
    { id: "steps", label: t("Show steps"), group: t("Steps"), keywords: "history steps list edits", run: () => (lite.stepsOpen = true) },
    { id: "start-over", label: t("Start over"), group: t("Steps"), keywords: "reset original revert all start over", run: () => lite.goTo(0) },
    { id: "pro", label: t("Open in Pro"), group: t("Profile"), keywords: "pro professional layers advanced photoshop", run: () => editor.setProfile("pro") },

    ...ops.AUTO_STYLES.map((s) => ({ id: `auto-${s.id}`, label: t("Auto: {style}", { style: t(s.label) }), group: g("auto"), keywords: `auto enhance fix magic wand ${s.label} ${s.detail}`, run: () => ops.applyAuto(s.id) })),

    { id: "adjust", label: t("Light, color and detail"), group: g("adjust"), keywords: "adjust light color colour brightness exposure contrast highlights shadows saturation vibrance warmth temperature tint clarity dehaze sliders", run: () => openCategory("adjust") },
    { id: "brighter", label: t("Brighter"), group: g("adjust"), keywords: "brighten lighter lift exposure", run: () => nudge({ exposure: 0.3, shadows: 15 }, t("Brighter")) },
    { id: "darker", label: t("Darker"), group: g("adjust"), keywords: "darken dim moody", run: () => nudge({ exposure: -0.3, blacks: -8 }, t("Darker")) },
    { id: "warmer", label: t("Warmer"), group: g("adjust"), keywords: "warm warmth temperature golden", run: () => nudge({ temperature: 20 }, t("Warmer")) },
    { id: "cooler", label: t("Cooler"), group: g("adjust"), keywords: "cool cold blue temperature", run: () => nudge({ temperature: -20 }, t("Cooler")) },
    { id: "pop", label: t("Make it pop"), group: g("adjust"), keywords: "pop punch vivid better enhance contrast saturation", run: () => nudge({ contrast: 15, vibrance: 30, clarity: 12 }, t("Make it pop")) },
    { id: "bw", label: t("Black and white"), group: g("adjust"), keywords: "black white mono monochrome grayscale greyscale b&w", run: () => nudge({ black_white: 100 }, t("Black and white")) },

    { id: "crop", label: t("Crop"), group: g("crop"), keywords: "crop trim cut frame", run: () => openCategory("crop") },
    ...ops.RATIOS.filter((r) => r.value).map((r) => ({ id: `crop-${r.id}`, label: t("Crop to {ratio}", { ratio: t(r.label) }), group: g("crop"), keywords: `crop ratio aspect ${r.label} ${r.id}`, run: () => ops.cropToRatio(r.value!, t(r.label)) })),
    { id: "rotate", label: t("Rotate right"), group: g("crop"), keywords: "rotate turn 90 clockwise", run: () => ops.rotate(1) },
    { id: "rotate-left", label: t("Rotate left"), group: g("crop"), keywords: "rotate turn 90 counterclockwise anticlockwise", run: () => ops.rotate(3) },
    { id: "flip", label: t("Flip"), group: g("crop"), keywords: "flip mirror horizontal", run: () => ops.flip(true) },
    { id: "straighten", label: t("Straighten"), group: g("crop"), keywords: "straighten level horizon tilt angle fix", run: () => ops.autoStraighten() },
    ...ops.RESIZE_PRESETS.map((p) => ({ id: `resize-${p.id}`, label: t("Resize for {preset}", { preset: t(p.label) }), group: g("crop"), keywords: `resize size social ${p.label} ${p.detail}`, run: () => ops.resizeFor(p) })),

    { id: "erase", label: t("Erase an object"), group: g("retouch"), keywords: "erase remove object person clean up magic eraser", run: () => retouchTool("remove") },
    { id: "spot", label: t("Spot heal"), group: g("retouch"), keywords: "spot heal blemish pimple dust", run: () => retouchTool("spot-heal") },
    { id: "redeye", label: t("Red eye"), group: g("retouch"), keywords: "red eye flash", run: () => retouchTool("red-eye") },
    { id: "subject", label: t("Select subject"), group: g("retouch"), keywords: "select subject person cutout mask", run: () => ops.selectSubject() },
    { id: "remove-bg", label: t("Remove background"), group: g("retouch"), keywords: "remove background cutout transparent png", run: () => ops.removeBackground() },
    { id: "blur-bg", label: t("Blur background"), group: g("effects"), keywords: "blur background bokeh portrait depth", run: () => ops.blurBackground() },
    { id: "brighten-subject", label: t("Brighten subject"), group: g("retouch"), keywords: "brighten subject person face light", run: () => ops.brightenSubject() },

    ...MARKUP_TOOL_IDS.map((k) => ({
      id: `markup-${k}`,
      label: t(MARKUP_LABELS[k] ?? k),
      group: g("markup"),
      keywords: `markup annotate draw ${k} ${MARKUP_LABELS[k] ?? ""} ${k === "redact" ? "hide censor blackout" : ""} ${k === "pixelate" ? "blur censor mosaic hide" : ""} ${k === "shape-rect" ? "box rectangle square" : ""} ${k === "shape-ellipse" ? "circle oval" : ""} ${k === "text" ? "type words label caption" : ""}`,
      run: () => markupTool(k),
    })),

    { id: "looks", label: t("Looks"), group: g("effects"), keywords: "looks filters presets styles", run: () => openCategory("effects") },
    ...LOOKS.map((l) => ({ id: `look-${l.id}`, label: t("Look: {name}", { name: t(l.name) }), group: g("effects"), keywords: `look filter preset ${l.name}`, run: () => ops.applyLook(l, 1) })),
    { id: "vignette", label: t("Vignette"), group: g("effects"), keywords: "vignette dark corners edges", run: () => openCategory("effects") },
    { id: "grain", label: t("Grain"), group: g("effects"), keywords: "grain film noise texture", run: () => openCategory("effects") },
    { id: "border", label: t("Add a border"), group: g("effects"), keywords: "border frame edge white polaroid", run: () => ops.addBorder(0.04, "#ffffff") },
    { id: "caption", label: t("Add a caption bar"), group: g("effects"), keywords: "caption bar text title frame", run: () => openCategory("effects") },

    { id: "upscale-2", label: t("Upscale 2×"), group: g("enhance"), keywords: "upscale enlarge bigger resolution super", run: () => ops.upscale(2, "fast") },
    { id: "upscale-4", label: t("Upscale 4×"), group: g("enhance"), keywords: "upscale enlarge bigger resolution super", run: () => ops.upscale(4, "fast") },
    { id: "denoise", label: t("Denoise"), group: g("enhance"), keywords: "denoise noise grainy low light clean", run: () => ops.denoise() },
    { id: "jpeg", label: t("Clean up JPEG"), group: g("enhance"), keywords: "jpeg artefacts artifacts compression blocky", run: () => ops.cleanJpeg() },
    { id: "faces", label: t("Restore faces"), group: g("enhance"), keywords: "faces restore blurry old portrait", run: () => ops.restoreFaces() },
    { id: "colorize", label: t("Colorize"), group: g("enhance"), keywords: "colorize colourise old black white photo", run: () => ops.colorize() },
  ];
  return list;
}

export const MARKUP_LABELS: Record<string, string> = {
  arrow: "Arrow",
  "shape-rect": "Box",
  "shape-ellipse": "Circle",
  "shape-line": "Line",
  pen: "Pen",
  highlighter: "Highlighter",
  text: "Text",
  redact: "Redact",
  pixelate: "Pixelate",
};

/** Fuzzy score: every query word must prefix-match some word, or be a subsequence. */
export function score(a: Action, query: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const hay = `${a.label} ${a.group} ${a.keywords}`.toLowerCase();
  const words = hay.split(/[^a-z0-9&:×]+/).filter(Boolean);
  const label = a.label.toLowerCase();
  let total = 0;
  for (const part of q.split(/\s+/)) {
    if (label.startsWith(part)) total += 6;
    else if (words.some((w) => w.startsWith(part))) total += label.includes(part) ? 4 : 3;
    else if (hay.includes(part)) total += 2;
    else {
      // Subsequence within the label, for typos like "blkwht".
      let i = 0;
      for (const ch of label) if (ch === part[i]) i++;
      if (i === part.length && part.length > 2) total += 1;
      else return 0;
    }
  }
  return total;
}

// ---------------------------------------------------------------------------
// Prompt planner

export interface Step {
  label: string;
  develop?: Partial<Develop>;
  /** An action id from `allActions`. */
  action?: string;
  /** A step from `ai.planEdit`, run by `ai.runPlan`. */
  planned?: ai.PlannedStep;
}

export const SUGGESTIONS: { text: string }[] = [
  { text: "Make it pop" },
  { text: "Warmer" },
  { text: "Brighter" },
  { text: "Black and white" },
  { text: "Remove background" },
  { text: "Blur background" },
  { text: "Straighten" },
  { text: "Square crop" },
];

const RULES: { re: RegExp; step: () => Step }[] = [
  { re: /\b(pop|punch|better|enhance|improve|vivid|nicer)\b/, step: () => ({ label: t("More contrast and color"), develop: { contrast: 15, vibrance: 30, clarity: 12 } }) },
  { re: /\b(warm(er)?|golden|sunny)\b/, step: () => ({ label: t("Warmer"), develop: { temperature: 22 } }) },
  { re: /\b(cool(er)?|cold(er)?|blu(e|er))\b/, step: () => ({ label: t("Cooler"), develop: { temperature: -22 } }) },
  { re: /\b(bright(er|en)?|light(er|en)?|lift)\b/, step: () => ({ label: t("Brighter"), develop: { exposure: 0.3, shadows: 18 } }) },
  { re: /\b(dark(er|en)?|dim(mer)?|moody)\b/, step: () => ({ label: t("Darker"), develop: { exposure: -0.3, blacks: -10 } }) },
  { re: /\bcontrast\b/, step: () => ({ label: t("More contrast"), develop: { contrast: 25 } }) },
  { re: /\b(black and white|black & white|b&w|b\/w|mono(chrome)?|gr[ae]yscale)\b/, step: () => ({ label: t("Black and white"), develop: { black_white: 100 } }) },
  { re: /\b(saturat\w*|colou?rful|more colou?r)\b/, step: () => ({ label: t("More color"), develop: { saturation: 22, vibrance: 15 } }) },
  { re: /\b(muted|desaturat\w*|less colou?r)\b/, step: () => ({ label: t("Less color"), develop: { saturation: -30 } }) },
  { re: /\b(sharp(er|en)?|crisp(er)?|detail)\b/, step: () => ({ label: t("More detail"), develop: { clarity: 25 } }) },
  { re: /\b(haz[ey]|fog(gy)?|dehaze)\b/, step: () => ({ label: t("Cut the haze"), develop: { dehaze: 30 } }) },
  { re: /\bvignette\b/, step: () => ({ label: t("Vignette"), develop: { vignette: -30 } }) },
  { re: /\b(film grain|grain)\b/, step: () => ({ label: t("Grain"), develop: { grain: 25 } }) },
  { re: /\bremove (the )?background\b|\bcut ?out\b/, step: () => ({ label: t("Remove background"), action: "remove-bg" }) },
  { re: /\bblur (the )?background\b|\bbokeh\b/, step: () => ({ label: t("Blur background"), action: "blur-bg" }) },
  { re: /\b(straighten|level|horizon)\b/, step: () => ({ label: t("Straighten"), action: "straighten" }) },
  { re: /\bsquare\b|\b1:1\b/, step: () => ({ label: t("Square crop"), action: "crop-1:1" }) },
  { re: /\b(4:5|instagram)\b/, step: () => ({ label: t("Crop to 4:5"), action: "crop-4:5" }) },
  { re: /\b(16:9|widescreen)\b/, step: () => ({ label: t("Crop to 16:9"), action: "crop-16:9" }) },
  { re: /\b(9:16|story|reel)\b/, step: () => ({ label: t("Crop to 9:16"), action: "crop-9:16" }) },
  { re: /\brotate\b/, step: () => ({ label: t("Rotate right"), action: "rotate" }) },
  { re: /\b(flip|mirror)\b/, step: () => ({ label: t("Flip"), action: "flip" }) },
  { re: /\b(upscale|enlarge|bigger|higher resolution)\b/, step: () => ({ label: t("Upscale 2×"), action: "upscale-2" }) },
  { re: /\b(denoise|noisy|grainy|less noise)\b/, step: () => ({ label: t("Denoise"), action: "denoise" }) },
  { re: /\b(colou?ri[sz]e)\b/, step: () => ({ label: t("Colorize"), action: "colorize" }) },
  { re: /\brestore (the )?faces?\b/, step: () => ({ label: t("Restore faces"), action: "faces" }) },
  { re: /\bborder\b/, step: () => ({ label: t("Add a border"), action: "border" }) },
];

export function planLocally(text: string): { steps: Step[]; unsupported?: string } {
  const q = text.toLowerCase();
  const steps: Step[] = [];
  for (const look of LOOKS) {
    if (new RegExp(`\\b${look.name.toLowerCase()}\\b`).test(q) && !/\b(vivid|warm|cool|mono)\b/.test(look.id)) steps.push({ label: t("Look: {name}", { name: t(look.name) }), action: `look-${look.id}` });
  }
  for (const r of RULES) {
    // "grain" inside "grainy" belongs to denoise, not to adding grain.
    if (r.re.source.includes("grain") && /grainy/.test(q) && !/film grain/.test(q)) continue;
    if (r.re.test(q)) steps.push(r.step());
  }
  if (!steps.length) return { steps, unsupported: t("I could not turn that into an edit yet. Try one of the suggestions, or search the tools.") };
  return { steps };
}

/** Develop steps become slider changes on "Adjustments", so they stay editable in Adjust. */
function fromPlanned(p: ai.PlannedStep): Step {
  const adj = p.cmd?.op === "layer.add-adjustment" ? (p.cmd.adjustment as Record<string, unknown> | undefined) : undefined;
  if (adj?.kind === "develop") {
    const develop: Partial<Develop> = {};
    for (const [k, v] of Object.entries(adj)) if (k !== "kind" && typeof v === "number") develop[k as keyof Develop] = v;
    return { label: p.label, develop };
  }
  return { label: p.label, planned: p };
}

export async function plan(text: string): Promise<{ steps: Step[]; unsupported?: string }> {
  try {
    const r = await ai.planEdit(text);
    if (!r.steps.length) {
      const local = planLocally(text);
      return local.steps.length ? local : { steps: [], unsupported: r.unsupported ?? local.unsupported };
    }
    return { steps: r.steps.map(fromPlanned), unsupported: r.unsupported };
  } catch (e) {
    if (!isNotReady(e)) console.warn("planEdit failed; using the local rules", e);
    return planLocally(text);
  }
}

export async function applyPlan(steps: Step[]) {
  const actions = allActions();
  // Slider changes land together as one visible, editable step.
  const delta: Partial<Develop> = {};
  for (const s of steps) for (const [k, v] of Object.entries(s.develop ?? {}) as [keyof Develop, number][]) delta[k] = (delta[k] ?? 0) + v;
  if (Object.keys(delta).length) await nudge(delta, steps.filter((s) => s.develop).map((s) => s.label).join(", "));
  // Plain data: reactive proxies cannot cross postMessage to the worker.
  const planned = JSON.parse(JSON.stringify(steps.filter((s) => s.planned).map((s) => s.planned!))) as ai.PlannedStep[];
  if (planned.length) {
    lite.begin();
    const ok = await runFeature("plan", t("Describe an edit"), () => lite.lock.run(() => ai.runPlan(planned)));
    if (ok) lite.note(planned.map((p) => p.label).join(", "));
  }
  for (const s of steps) {
    if (s.action) await actions.find((a) => a.id === s.action)?.run();
  }
}
