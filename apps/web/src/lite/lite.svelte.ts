// Lite's own state: which category is open, the sheets, and the engine
// plumbing Lite needs beyond `editor.exec` — a lock so previews never
// interleave with edits, previews that leave history as they found it, and
// the named develop layers the sliders and Looks drive.

import { editor } from "../lib/editor.svelte";
import { NotReady } from "../lib/ai";
import { t } from "../lib/i18n";
import { allLayers, DEVELOP_DEFAULT, type Adjustment, type Develop, type ExecResult, type LayerInfo, type Summary } from "../engine/types";
import { sanitize } from "./develop";

export type Category = "auto" | "adjust" | "crop" | "retouch" | "markup" | "effects" | "enhance";

export const ADJUSTMENTS = "Adjustments";
export const LOOK = "Look";
export const LOOK_TINT = "Look tint";
export const FRAME = "Frame";

/** Serialises work that must not interleave with preview rendering. */
class Lock {
  private tail: Promise<unknown> = Promise.resolve();
  run<T>(fn: () => Promise<T>): Promise<T> {
    const next = this.tail.then(fn, fn);
    this.tail = next.catch(() => {});
    return next;
  }
}

export function isUnknownOp(e: unknown): boolean {
  const m = e instanceof Error ? e.message : String(e);
  return /unknown operation|unknown variant|no method|no job named/i.test(m);
}

export function isNotReady(e: unknown): boolean {
  return e instanceof NotReady || (e instanceof Error && e.name === "NotReady");
}

const PHONE_QUERY = "(max-width: 899px)";

class LiteStore {
  category = $state<Category>("adjust");
  /** Phone: whether the bottom sheet shows the category's controls. */
  sheetOpen = $state(true);
  exportOpen = $state(false);
  stepsOpen = $state(false);
  searchOpen = $state(false);
  menuOpen = $state(false);
  comparing = $state(false);
  /** Degrees shown as a live CSS preview while the straighten slider moves. */
  straightenPreview = $state<number | null>(null);
  phone = $state(typeof matchMedia !== "undefined" ? matchMedia(PHONE_QUERY).matches : false);
  /** Features whose implementation said NotReady, or whose op is missing. */
  unavailable = $state<Record<string, boolean>>({});
  /** A long Lite action in progress, shown inline in its panel. */
  working = $state<string | null>(null);
  /** Redo entries left behind by previews; never offered to the user. */
  phantomRedo = $state(0);
  /** Friendly names for history entries, keyed by position. */
  private notes = new Map<number, { engine: string; text: string; group: number }>();
  private noteGroup = 0;
  notesTick = $state(0);
  readonly lock = new Lock();

  constructor() {
    if (typeof matchMedia !== "undefined") {
      matchMedia(PHONE_QUERY).addEventListener("change", (e) => (this.phone = e.matches));
    }
  }

  get summary(): Summary | null {
    return editor.summary;
  }

  get redoCount(): number {
    const n = editor.summary?.history.redo.length ?? 0;
    return Math.max(0, n - this.phantomRedo);
  }

  get undoCount(): number {
    return editor.summary?.history.undo.length ?? 0;
  }

  markUnavailable(key: string) {
    this.unavailable = { ...this.unavailable, [key]: true };
  }

  // -------------------------------------------------------------------
  // Commands

  /** `editor.exec`, serialised behind previews. */
  exec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray, opts: { quiet?: boolean } = {}): Promise<ExecResult | null> {
    return this.lock.run(() => editor.exec(cmd, bytes, opts));
  }

  /** Run a command and report failure instead of toasting it. */
  async tryExec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray): Promise<{ ok: true; result: ExecResult } | { ok: false; error: unknown }> {
    return this.lock.run(async () => {
      try {
        const result = await editor.engine.exec(cmd, bytes);
        if (result.changed) editor.dirty = true;
        return { ok: true as const, result };
      } catch (error) {
        return { ok: false as const, error };
      }
    });
  }

  undo() {
    if (!this.undoCount) return;
    return this.exec({ op: "edit.undo" });
  }

  redo() {
    if (!this.redoCount) return;
    return this.exec({ op: "edit.redo" });
  }

  goTo(index: number) {
    return this.exec({ op: "edit.history-go", index });
  }

  /** Where the history stood when the current action began. */
  private actionFrom: number | null = null;

  /** Mark the start of an action that may take several history steps. */
  begin() {
    if (this.actionFrom == null) this.actionFrom = editor.summary?.history.undo.length ?? 0;
  }

  /**
   * Name the history entries of the action that just finished (everything
   * since `begin`, or only the newest entry) for the Steps list.
   */
  note(text: string, match?: RegExp) {
    const undo = editor.summary?.history.undo;
    const from = this.actionFrom;
    this.actionFrom = null;
    if (!undo?.length) return;
    if (match && !match.test(undo[undo.length - 1])) return;
    const start = from != null && from < undo.length ? from : undo.length - 1;
    const group = ++this.noteGroup;
    for (let i = start; i < undo.length; i++) this.notes.set(i, { engine: undo[i], text, group });
    this.notesTick++;
  }

  /** Entries noted by one action share a group; others are their own. */
  stepGroup(index: number, engineLabel: string): number {
    const n = this.notes.get(index);
    return n && n.engine === engineLabel ? n.group : -(index + 1);
  }

  stepLabel(index: number, engineLabel: string): string {
    const n = this.notes.get(index);
    if (n && n.engine === engineLabel) return n.text;
    return friendlyLabel(engineLabel);
  }

  /**
   * Apply commands to the engine without telling the UI, run `probe`, then
   * jump history back to exactly where it was. The canvas never sees the
   * intermediate states. History's redo stack keeps the undone steps, which
   * are tracked as phantom entries and never offered.
   */
  async silently<T>(cmds: Record<string, unknown>[], probe: () => Promise<T>): Promise<T> {
    const call = async (cmd: Record<string, unknown>) => JSON.parse(await editor.engine.call<string>("exec", JSON.stringify(cmd), new Uint8Array(0)));
    const undoLen = async () => (JSON.parse(await editor.engine.call<string>("summary")) as Summary).history.undo.length;
    const start = JSON.parse(await editor.engine.call<string>("summary")) as Summary;
    if (cmds.length && start.history.redo.length > this.phantomRedo) throw new Error("redo steps pending");
    await call({ op: "edit.seal" });
    const before = start.history.undo.length;
    try {
      for (const c of cmds) await call(c);
      return await probe();
    } finally {
      const after = await undoLen();
      if (after !== before) {
        await call({ op: "edit.history-go", index: before });
        // Bookkeeping first, so the summary that follows is read correctly.
        this.phantomRedo = Math.max(0, after - before);
        editor.summary = JSON.parse(await editor.engine.call<string>("summary")) as Summary;
      }
    }
  }

  /** Several silent probes in a row, then one summary refresh. */
  preview<T>(fn: () => Promise<T>): Promise<T | null> {
    return this.lock.run(async () => {
      const s = editor.summary;
      // Previews would discard real redo steps; wait until there are none.
      if (!s || s.history.redo.length > this.phantomRedo) return null;
      try {
        return await fn();
      } finally {
        const fresh = JSON.parse(await editor.engine.call<string>("summary")) as Summary;
        editor.summary = fresh;
      }
    });
  }

  /** Clear phantom redo bookkeeping once the stack no longer holds them. */
  syncPhantom() {
    const u = editor.summary?.history.undo.length ?? 0;
    if (this.actionFrom != null && u < this.actionFrom) this.actionFrom = null;
    const n = editor.summary?.history.redo.length ?? 0;
    if (n < this.phantomRedo) this.phantomRedo = n;
  }

  // -------------------------------------------------------------------
  // Layers

  layers(): LayerInfo[] {
    return editor.summary ? allLayers(editor.summary.layers) : [];
  }

  findAdjustment(name: string, kind: string): LayerInfo | null {
    const found = this.layers().filter((l) => l.kind === "adjustment" && l.name === name && l.adjustment?.kind === kind);
    return found.length ? found[found.length - 1] : null;
  }

  isAnnotation(l: LayerInfo) {
    return l.kind === "shape" || l.kind === "text";
  }

  /** The topmost layer id, to keep new annotations above everything. */
  topId(): number | null {
    const ls = editor.summary?.layers ?? [];
    return ls.length ? ls[ls.length - 1].id : null;
  }

  /**
   * Where a new photo-level layer goes: above the photo and its
   * corrections, below any annotations. `order` puts Adjustments under the
   * Look and the Look under its tint.
   */
  anchorFor(name: string): number | null {
    const ls = editor.summary?.layers ?? [];
    const rank = (l: LayerInfo) => (l.name === LOOK_TINT ? 3 : l.name === LOOK ? 2 : l.name === ADJUSTMENTS ? 1 : 0);
    const mine = name === LOOK_TINT ? 3 : name === LOOK ? 2 : 1;
    let anchor: number | null = null;
    for (const l of ls) {
      if (this.isAnnotation(l)) continue;
      const r = rank(l);
      if (r === 0 || r < mine) anchor = l.id;
    }
    return anchor;
  }

  basePixelLayer(): LayerInfo | null {
    return (editor.summary?.layers ?? []).find((l) => l.kind === "pixel") ?? null;
  }
}

export const lite = new LiteStore();

// ---------------------------------------------------------------------------

const LABELS: [RegExp, string][] = [
  [/^New Adjustment Layer$/, "Add adjustments"],
  [/^Edit Develop$/, "Adjust light and color"],
  [/^Edit Color Balance$/, "Adjust tint"],
  [/^Edit Shape$/, "Edit shape"],
  [/^Move$/, "Move"],
  [/^Delete Layers?$/, "Delete"],
  [/^Add Text$/, "Add text"],
  [/^Canvas Size$/, "Add border"],
  [/^Rotate 90/i, "Rotate"],
];

export function friendlyLabel(label: string): string {
  for (const [re, text] of LABELS) if (re.test(label)) return t(text);
  const s = label.trim();
  if (!s) return t("Edit");
  // Engine labels are title case; Lite speaks in sentence case.
  const words = s.split(/\s+/);
  return t(words.map((w, i) => (i === 0 ? w : /^[A-Z][a-z]+$/.test(w) ? w.toLowerCase() : w)).join(" "));
}

// ---------------------------------------------------------------------------

/**
 * One named Develop layer ("Adjustments" or "Look"), found by name and kind
 * in the summary so it survives undo, redo and reopening. Slider moves are
 * sent once per animation frame; release seals the undo step.
 */
export class DevelopLayer {
  private local = $state<Develop | null>(null);
  private queued = false;
  private flying: Promise<void> | null = null;
  private creating = false;
  onCreated: ((id: number) => void) | null = null;

  constructor(readonly name: string) {}

  get layer(): LayerInfo | null {
    void editor.summary?.revision;
    return lite.findAdjustment(this.name, "develop");
  }

  get stored(): Develop {
    const a = this.layer?.adjustment as (Adjustment & Partial<Develop>) | undefined;
    if (!a) return { ...DEVELOP_DEFAULT };
    const d = { ...DEVELOP_DEFAULT };
    for (const k of Object.keys(d) as (keyof Develop)[]) if (typeof a[k] === "number") d[k] = a[k] as number;
    return d;
  }

  get values(): Develop {
    return this.local ?? this.stored;
  }

  get active(): boolean {
    return this.local !== null;
  }

  /** Change fields; the engine hears about it on the next frame. */
  set(patch: Partial<Develop>) {
    if (!this.local) lite.begin();
    this.local = sanitize({ ...this.values, ...patch });
    if (!this.queued) {
      this.queued = true;
      requestAnimationFrame(() => {
        this.queued = false;
        void this.flush();
      });
    }
  }

  /** Replace every field at once and commit (Auto, Looks, reset). */
  async replace(d: Develop) {
    lite.begin();
    this.local = sanitize(d);
    await this.flush();
    await this.commit();
  }

  private async flush(): Promise<void> {
    if (this.flying) {
      await this.flying;
      if (!this.local) return;
    }
    const d = this.local;
    if (!d) return;
    this.flying = (async () => {
      const adjustment = { kind: "develop", ...d };
      const layer = this.layer;
      const stored = this.stored;
      const unchanged = (Object.keys(d) as (keyof Develop)[]).every((k) => Math.abs(d[k] - stored[k]) < 1e-4);
      if (layer && unchanged) return;
      if (layer) {
        await lite.exec({ op: "layer.set-adjustment", id: layer.id, adjustment });
      } else if (!this.creating) {
        this.creating = true;
        try {
          if (editor.summary?.selection) await lite.exec({ op: "select.none" });
          const r = await lite.exec({ op: "layer.add-adjustment", adjustment, name: this.name, above: lite.anchorFor(this.name) });
          const id = r?.data?.id as number | undefined;
          if (id != null) this.onCreated?.(id);
        } finally {
          this.creating = false;
        }
      }
    })();
    try {
      await this.flying;
    } finally {
      this.flying = null;
    }
    // Something changed while that was in flight: send the latest.
    const now = this.local;
    if (now && now !== d && this.layer) {
      const same = (Object.keys(now) as (keyof Develop)[]).every((k) => now[k] === d[k]);
      if (!same) await this.flush();
    }
  }

  /** End of a gesture: send what is pending, seal the undo step. */
  async commit() {
    if (this.queued) {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    }
    await this.flush();
    await lite.exec({ op: "edit.seal" });
    this.local = null;
  }

  async remove() {
    const l = this.layer;
    this.local = null;
    if (l) await lite.exec({ op: "layer.delete", ids: [l.id] });
  }
}

export const adjustments = new DevelopLayer(ADJUSTMENTS);
export const look = new DevelopLayer(LOOK);
