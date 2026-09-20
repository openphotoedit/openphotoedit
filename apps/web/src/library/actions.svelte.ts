// Actions: named lists of engine commands, recorded from what the user does
// in the editor (through `editor.onExec`) or added by hand, played back on
// the open document or on each file of a batch.
//
// Every edit is already a typed JSON command, so an action is just those
// commands with layer ids turned into references (see action-refs.ts).

import { editor } from "../lib/editor.svelte";
import { t } from "../lib/i18n";
import type { ExecResult, Summary } from "../engine/types";
import * as db from "./db";
import { createdId, isReplayable, parameterize, resolve } from "./action-refs";

export interface ActionStep {
  id: string;
  kind: "command" | "stop";
  label: string;
  enabled: boolean;
  cmd?: Record<string, unknown>;
  /** Stop steps: what to tell the user before continuing. */
  message?: string;
  /** Recorded caveats ("pixel data skipped"). */
  note?: string;
}

export interface ActionSet {
  id: string;
  name: string;
  steps: ActionStep[];
  created: number;
  updated: number;
}

/** Where commands go: the editor, or a batch's private engine. */
export interface ExecTarget {
  exec(cmd: Record<string, unknown>): Promise<ExecResult | null>;
  summary(): Summary | null;
}

export interface PlayOptions {
  /** Answer a stop step; resolve true to continue. Without it stops are skipped. */
  onStop?: (message: string) => Promise<boolean>;
  onStep?: (index: number, step: ActionStep) => void;
  signal?: AbortSignal;
}

export interface PlayReport {
  ran: number;
  notes: string[];
  stoppedAt?: number;
}

export const EXPORT_FORMAT = "openphotoedit-actions";

const uid = () => Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4);

/** Commands whose consecutive repeats on the same target collapse into one step. */
const MERGING = new Set(["layer.set-adjustment", "layer.set-fill", "layer.props", "layer.set-effects", "layer.set-text", "layer.set-shape", "layer.mask-props", "layer.offset", "select.feather"]);

export function describeCommand(cmd: Record<string, unknown>, engineLabel?: string | null): string {
  if (engineLabel) return engineLabel;
  const op = String(cmd.op ?? "");
  const [, name = op] = op.split(".");
  let label = name.replace(/-/g, " ");
  label = label.charAt(0).toUpperCase() + label.slice(1);
  const adj = (cmd.adjustment as { kind?: string } | undefined)?.kind;
  if (adj) label += `: ${adj.replace(/-/g, " ")}`;
  return label;
}

/** Hand-added steps: common, portable commands that work on any photo. */
export const STEP_TEMPLATES: { label: string; cmd: Record<string, unknown> }[] = [
  { label: "Black & White adjustment", cmd: { op: "layer.add-adjustment", adjustment: { kind: "black-white" } } },
  { label: "Develop: punchy", cmd: { op: "layer.add-adjustment", name: "Develop", adjustment: { kind: "develop", contrast: 25, vibrance: 30, clarity: 15 } } },
  { label: "Exposure +0.5", cmd: { op: "layer.add-adjustment", adjustment: { kind: "exposure", exposure: 0.5 } } },
  { label: "Vibrance +25", cmd: { op: "layer.add-adjustment", adjustment: { kind: "vibrance", vibrance: 25, saturation: 0 } } },
  { label: "Invert", cmd: { op: "layer.add-adjustment", adjustment: { kind: "invert" } } },
  { label: "Flatten image", cmd: { op: "layer.flatten" } },
  { label: "Sharpen (unsharp mask)", cmd: { op: "filter.unsharp-mask", amount: 80, radius: 1.2, threshold: 2 } },
  { label: "Vignette", cmd: { op: "filter.vignette", amount: -35, midpoint: 50, feather: 60 } },
  { label: "Gaussian blur 2 px", cmd: { op: "filter.gaussian-blur", radius: 2 } },
  { label: "Rotate 90° clockwise", cmd: { op: "image.rotate", turns: 1 } },
  { label: "Flip horizontal", cmd: { op: "image.flip", horizontal: true } },
];

function snapshot(s: Summary | null): Summary | null {
  return s ? (JSON.parse(JSON.stringify(s)) as Summary) : null;
}

export class ActionsStore {
  sets = $state<ActionSet[]>([]);
  activeId = $state<string | null>(null);
  recording = $state<string | null>(null);
  playing = $state<{ setId: string; index: number; total: number } | null>(null);
  loaded = $state(false);

  private listener: ((cmd: Record<string, unknown>, r: ExecResult, hadBytes: boolean) => void) | null = null;
  private before: Summary | null = null;
  private created = new Map<number, number | string>();

  active = $derived(this.sets.find((s) => s.id === this.activeId) ?? null);

  async load() {
    const rows = await db.all<ActionSet>("actions");
    this.sets = rows.map((r) => r.value).sort((a, b) => a.created - b.created);
    if (!this.sets.length) {
      const s = this.newSet(t("Web finishing"), false);
      s.steps.push(
        { id: uid(), kind: "command", label: "Develop: punchy", enabled: true, cmd: STEP_TEMPLATES[1].cmd },
        { id: uid(), kind: "command", label: "Flatten image", enabled: true, cmd: { op: "layer.flatten" } },
        { id: uid(), kind: "command", label: "Sharpen (unsharp mask)", enabled: true, cmd: STEP_TEMPLATES[6].cmd },
      );
      await this.save(s);
    }
    this.activeId ??= this.sets[0]?.id ?? null;
    this.loaded = true;
  }

  private async save(set: ActionSet) {
    set.updated = Date.now();
    await db.put("actions", set.id, $state.snapshot(set));
  }

  private find(id: string | null | undefined) {
    return this.sets.find((s) => s.id === id) ?? null;
  }

  newSet(name = t("New action"), persist = true): ActionSet {
    const now = Date.now();
    this.sets.push({ id: uid(), name, steps: [], created: now, updated: now });
    const s = this.sets[this.sets.length - 1];
    this.activeId = s.id;
    if (persist) void this.save(s);
    return s;
  }

  rename(id: string, name: string) {
    const s = this.find(id);
    if (!s || !name.trim()) return;
    s.name = name.trim();
    void this.save(s);
  }

  duplicate(id: string) {
    const s = this.find(id);
    if (!s) return;
    const copy = this.newSet(t("{name} copy", { name: s.name }), false);
    copy.steps = s.steps.map((st) => ({ ...JSON.parse(JSON.stringify(st)), id: uid() }));
    void this.save(copy);
  }

  async remove(id: string) {
    if (this.recording === id) this.stopRecording();
    this.sets = this.sets.filter((s) => s.id !== id);
    await db.del("actions", id);
    if (this.activeId === id) this.activeId = this.sets[0]?.id ?? null;
  }

  addStep(id: string, cmd: Record<string, unknown>, label?: string, note?: string) {
    const s = this.find(id);
    if (!s) return;
    s.steps.push({ id: uid(), kind: "command", label: label ?? describeCommand(cmd), enabled: true, cmd, note });
    void this.save(s);
  }

  addStop(id: string, message: string) {
    const s = this.find(id);
    if (!s) return;
    s.steps.push({ id: uid(), kind: "stop", label: t("Stop"), enabled: true, message: message || t("Check the photo before continuing.") });
    void this.save(s);
  }

  updateStep(id: string, stepId: string, patch: Partial<ActionStep>) {
    const s = this.find(id);
    const st = s?.steps.find((x) => x.id === stepId);
    if (!s || !st) return;
    Object.assign(st, patch);
    void this.save(s);
  }

  removeStep(id: string, stepId: string) {
    const s = this.find(id);
    if (!s) return;
    s.steps = s.steps.filter((x) => x.id !== stepId);
    void this.save(s);
  }

  moveStep(id: string, stepId: string, dir: -1 | 1) {
    const s = this.find(id);
    if (!s) return;
    const i = s.steps.findIndex((x) => x.id === stepId);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= s.steps.length) return;
    const [st] = s.steps.splice(i, 1);
    s.steps.splice(j, 0, st);
    void this.save(s);
  }

  // -------------------------------------------------------------------------
  // Recording

  /** Start appending the editor's commands to `id` (default: the active set). */
  startRecording(id = this.activeId) {
    const set = this.find(id);
    if (!set) return;
    this.stopRecording();
    this.recording = set.id;
    this.before = snapshot(editor.summary);
    this.created = new Map();
    this.listener = (cmd, result, hadBytes) => this.record(set.id, cmd, result, hadBytes);
    editor.onExec.add(this.listener);
  }

  stopRecording() {
    if (this.listener) editor.onExec.delete(this.listener);
    this.listener = null;
    this.recording = null;
  }

  /** Exposed for tests and for shells that route commands elsewhere. */
  record(id: string, cmd: Record<string, unknown>, result: ExecResult, hadBytes: boolean) {
    const set = this.find(id);
    const op = String(cmd.op ?? "");
    const before = this.before;
    this.before = snapshot(editor.summary);
    if (!set || !op || !isReplayable(op) || this.playing) return;
    if (hadBytes) {
      set.steps.push({ id: uid(), kind: "command", label: describeCommand(cmd, result.label), enabled: false, note: t("Not recorded: this step carries pixel data (a brush dab, pasted or AI pixels) that cannot be replayed on another photo.") });
      void this.save(set);
      return;
    }
    const p = parameterize(cmd, before, this.created);
    const last = set.steps[set.steps.length - 1];
    if (last?.cmd && last.cmd.op === op && MERGING.has(op) && JSON.stringify(last.cmd.id ?? last.cmd.ids) === JSON.stringify(p.id ?? p.ids)) {
      // A slider drag: keep the final value (offsets add up).
      if (op === "layer.offset") {
        last.cmd = { ...p, dx: Number(last.cmd.dx ?? 0) + Number(p.dx ?? 0), dy: Number(last.cmd.dy ?? 0) + Number(p.dy ?? 0) };
      } else last.cmd = { ...last.cmd, ...p };
      void this.save(set);
      return;
    }
    const stepId = uid();
    const newId = createdId(result);
    if (newId != null) this.created.set(newId, stepId);
    set.steps.push({ id: stepId, kind: "command", label: describeCommand(cmd, result.label), enabled: true, cmd: p });
    void this.save(set);
  }

  // -------------------------------------------------------------------------
  // Playback

  editorTarget(): ExecTarget {
    return { exec: (c) => editor.exec(c), summary: () => editor.summary };
  }

  async play(id: string | null = this.activeId, target: ExecTarget = this.editorTarget(), opts: PlayOptions = {}): Promise<PlayReport> {
    const set = this.find(id);
    if (!set) throw new Error(t("That action no longer exists."));
    if (this.recording === set.id) this.stopRecording();
    this.playing = { setId: set.id, index: 0, total: set.steps.length };
    try {
      return await playSteps($state.snapshot(set.steps) as ActionStep[], target, {
        ...opts,
        onStep: (i, st) => {
          this.playing = { setId: set.id, index: i, total: set.steps.length };
          opts.onStep?.(i, st);
        },
      });
    } finally {
      this.playing = null;
    }
  }

  // -------------------------------------------------------------------------
  // Files

  exportJson(ids: string[] = this.activeId ? [this.activeId] : []): string {
    const sets = this.sets.filter((s) => ids.includes(s.id)).map((s) => $state.snapshot(s));
    return JSON.stringify({ format: EXPORT_FORMAT, version: 1, sets }, null, 2);
  }

  /** Import sets from JSON; returns how many. Ids are renewed so imports never overwrite. */
  async importJson(text: string): Promise<number> {
    let data: unknown;
    try {
      data = JSON.parse(text);
    } catch {
      throw new Error(t("That file is not valid JSON."));
    }
    const d = data as { format?: string; sets?: ActionSet[] };
    if (d.format !== EXPORT_FORMAT || !Array.isArray(d.sets)) throw new Error(t("That file is not an OpenPhotoEdit actions file."));
    let n = 0;
    for (const raw of d.sets) {
      if (!raw || typeof raw.name !== "string" || !Array.isArray(raw.steps)) continue;
      const steps: ActionStep[] = raw.steps
        .filter((st) => st && (st.kind === "stop" || (st.kind === "command" && (!st.cmd || typeof st.cmd.op === "string"))))
        .map((st) => ({ id: uid(), kind: st.kind, label: String(st.label ?? ""), enabled: st.enabled !== false, cmd: st.cmd, message: st.message, note: st.note }));
      const now = Date.now();
      this.sets.push({ id: uid(), name: raw.name, steps, created: now, updated: now });
      const s = this.sets[this.sets.length - 1];
      await this.save(s);
      this.activeId = s.id;
      n++;
    }
    return n;
  }
}

/** Run steps against any target. Throws on the first failing command, naming it. */
export async function playSteps(steps: ActionStep[], target: ExecTarget, opts: PlayOptions = {}): Promise<PlayReport> {
  const stepIds = new Map<number | string, number>();
  const notes: string[] = [];
  let ran = 0;
  for (let i = 0; i < steps.length; i++) {
    if (opts.signal?.aborted) throw new Error("cancelled");
    const st = steps[i];
    if (!st.enabled) continue;
    opts.onStep?.(i, st);
    if (st.kind === "stop") {
      if (opts.onStop && !(await opts.onStop(st.message ?? ""))) return { ran, notes, stoppedAt: i };
      continue;
    }
    if (!st.cmd) continue;
    const r = resolve(st.cmd, target.summary(), stepIds);
    for (const n of r.notes) notes.push(`${st.label}: ${n}`);
    let res: ExecResult | null;
    try {
      res = await target.exec(r.cmd);
    } catch (e) {
      throw new Error(t("Step {n} ({label}) failed: {error}", { n: i + 1, label: st.label, error: e instanceof Error ? e.message : String(e) }));
    }
    if (!res) throw new Error(t("Step {n} ({label}) failed.", { n: i + 1, label: st.label }));
    const id = createdId(res);
    if (id != null) {
      stepIds.set(i, id);
      stepIds.set(st.id, id);
    }
    ran++;
  }
  return { ran, notes };
}

export const actions = new ActionsStore();
