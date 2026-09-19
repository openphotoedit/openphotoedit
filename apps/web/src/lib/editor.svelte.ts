// Application state shared by the Lite and Pro shells: the engine handle,
// the document summary, the viewport, the active tool and tool settings.

import { EngineClient, type JobResult } from "../engine/client";
import { findLayer, type ExecResult, type LayerInfo, type Rgba8, type Summary } from "../engine/types";

export type Profile = "lite" | "pro";

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
  action?: { label: string; run: () => void };
}

export interface Busy {
  label: string;
  fraction?: number;
  cancel?: () => void;
}

export interface ViewState {
  /** Document point at the centre of the viewport. */
  cx: number;
  cy: number;
  /** CSS pixels per document pixel. */
  zoom: number;
}

const PROFILE_KEY = "ops.profile";

function loadProfile(): Profile | null {
  try {
    const v = localStorage.getItem(PROFILE_KEY);
    return v === "lite" || v === "pro" ? v : null;
  } catch {
    return null;
  }
}

/** An open document as the tab bar shows it. */
export interface DocTab {
  id: number;
  name: string;
  dirty: boolean;
  hasDocument: boolean;
  view: ViewState;
}

export const ZOOM_STEPS = [0.01, 0.02, 0.03, 0.05, 0.0833, 0.125, 0.1667, 0.25, 0.3333, 0.5, 0.6667, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 32];

export class EditorStore {
  engine = new EngineClient();
  ready = $state(false);
  summary = $state.raw<Summary | null>(null);
  /** True once a real document is open (not the 1×1 placeholder). */
  hasDocument = $state(false);
  profile = $state<Profile | null>(loadProfile());
  tool = $state("move");
  view = $state<ViewState>({ cx: 0, cy: 0, zoom: 1 });
  /** Viewport size in CSS pixels, set by the canvas component. */
  viewport = $state({ width: 800, height: 600 });
  /** Bump to force a re-render when something outside the summary changed. */
  renderTick = $state(0);
  /** Bump to redraw tool overlays only. */
  overlayTick = $state(0);
  primary = $state<Rgba8>({ r: 0, g: 0, b: 0, a: 255 });
  secondary = $state<Rgba8>({ r: 255, g: 255, b: 255, a: 255 });
  busy = $state<Busy | null>(null);
  toasts = $state<Toast[]>([]);
  /** Unsaved changes since the last export/save. */
  dirty = $state(false);
  fileName = $state("Untitled");
  /** A tool's cursor override (over a handle, say); null uses the tool's own. */
  cursor = $state<string | null>(null);
  /** The canvas component's host element, for tools that position overlays. */
  canvasHost: HTMLElement | null = null;
  /** Layers hidden from the viewport while a tool previews them. */
  previewHidden = $state<number[]>([]);
  /** Every open document; the current one's live state is on this store. */
  tabs = $state<DocTab[]>([{ id: 1, name: "Untitled", dirty: false, hasDocument: false, view: { cx: 0, cy: 0, zoom: 1 } }]);
  currentTab = $state(1);
  /** The tab a Layers-panel drag is over (a drop copies the layers there). */
  layerDropTab = $state<number | null>(null);
  private toastId = 1;

  constructor() {
    this.engine.onSummary = (s) => {
      this.summary = s;
    };
  }

  async init() {
    const s = await this.engine.init();
    this.summary = s;
    this.ready = true;
  }

  setProfile(p: Profile) {
    this.profile = p;
    try {
      localStorage.setItem(PROFILE_KEY, p);
    } catch {
      /* private mode: the choice lasts for this session */
    }
  }

  get active(): LayerInfo | null {
    return this.summary ? findLayer(this.summary.layers, this.summary.active) : null;
  }

  /**
   * Listeners told about every successful command (Actions recording, the
   * step list). Add with `editor.onExec.add(fn)`; remove with `.delete(fn)`.
   * `bytes` is only present for commands that carried a payload.
   */
  onExec = new Set<(cmd: Record<string, unknown>, result: ExecResult, hadBytes: boolean) => void>();

  /** Run a command; failures become an error toast and resolve to null. */
  async exec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray, opts: { quiet?: boolean } = {}): Promise<ExecResult | null> {
    try {
      const hadBytes = !!bytes && bytes.byteLength > 0;
      const r = await this.engine.exec(cmd, bytes);
      if (r.changed) this.dirty = true;
      for (const fn of this.onExec) {
        try {
          fn(cmd, r, hadBytes);
        } catch (e) {
          console.error("onExec listener failed", e);
        }
      }
      if (Array.isArray((r as { warnings?: unknown }).warnings)) {
        for (const w of (r as { warnings: string[] }).warnings) this.toast(w, "info");
      }
      return r;
    } catch (e) {
      if (!opts.quiet) this.error(e);
      return null;
    }
  }

  /**
   * Run `fn` (which may send any number of commands) as ONE undo step called
   * `label`, through the engine's `edit.begin`/`edit.end` transaction. If
   * `fn` returns false or throws, the document is put back as it was
   * (`edit.cancel`) and nothing is recorded. Transactions nest.
   */
  async oneStep(label: string, fn: () => Promise<boolean | void>): Promise<boolean> {
    await this.exec({ op: "edit.begin", label }, undefined, { quiet: true });
    let ok: boolean | void = false;
    try {
      ok = await fn();
    } finally {
      await this.exec({ op: ok === false ? "edit.cancel" : "edit.end" }, undefined, { quiet: true });
    }
    return ok !== false;
  }

  /** Run a long job with a progress indicator. */
  async job<T = unknown>(label: string, name: string, params: Record<string, unknown> = {}, bytes?: Uint8Array): Promise<JobResult<T> | null> {
    const handle = this.engine.job<T>(name, params, {
      bytes,
      onProgress: (p) => {
        if (this.busy) this.busy = { ...this.busy, label: p.message ?? this.busy.label, fraction: p.fraction };
      },
    });
    this.busy = { label, cancel: handle.cancel };
    try {
      const r = await handle.promise;
      if (r.changed) this.dirty = true;
      return r;
    } catch (e) {
      if (!(e instanceof Error && e.message === "cancelled")) this.error(e);
      return null;
    } finally {
      this.busy = null;
    }
  }

  /** Hide layers from the viewport only (no undo step, not saved). */
  async setPreviewHidden(ids: number[]) {
    this.previewHidden = ids;
    await this.engine.call("set_preview_hidden", ids);
    this.renderTick++;
  }

  undo() {
    return this.exec({ op: "edit.undo" });
  }
  redo() {
    return this.exec({ op: "edit.redo" });
  }

  toast(text: string, kind: Toast["kind"] = "info", action?: Toast["action"]) {
    const id = this.toastId++;
    this.toasts = [...this.toasts, { id, kind, text, action }];
    setTimeout(() => this.dismiss(id), kind === "error" ? 8000 : 4000);
  }

  error(e: unknown) {
    const text = e instanceof Error ? e.message : String(e);
    console.error(e);
    this.toast(text.charAt(0).toUpperCase() + text.slice(1), "error");
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  // ---------------------------------------------------------------------
  // Documents

  private saveTabState() {
    this.tabs = this.tabs.map((t) =>
      t.id === this.currentTab ? { ...t, name: this.fileName, dirty: this.dirty, hasDocument: this.hasDocument, view: { ...this.view } } : t,
    );
  }

  private async refreshSummary() {
    this.summary = JSON.parse(await this.engine.call<string>("summary"));
  }

  /** Start a new, empty document tab and make it current. */
  async newTab(): Promise<number> {
    this.saveTabState();
    const id = await this.engine.call<number>("new_document");
    this.tabs = [...this.tabs, { id, name: "Untitled", dirty: false, hasDocument: false, view: { cx: 0, cy: 0, zoom: 1 } }];
    this.currentTab = id;
    this.fileName = "Untitled";
    this.dirty = false;
    this.hasDocument = false;
    await this.refreshSummary();
    return id;
  }

  async switchTab(id: number) {
    if (id === this.currentTab) return;
    this.saveTabState();
    await this.engine.call("switch_document", id);
    const t = this.tabs.find((x) => x.id === id);
    this.currentTab = id;
    if (t) {
      this.fileName = t.name;
      this.dirty = t.dirty;
      this.hasDocument = t.hasDocument;
      this.view = { ...t.view };
    }
    await this.refreshOthers();
  }

  private async refreshOthers() {
    await this.refreshSummary();
    this.renderTick++;
  }

  /**
   * Copy layers of the current document into another open document (one
   * undo step there), then show that document. Returns how many top-level
   * layers arrived, or null on failure.
   */
  async copyLayersToTab(dst: number, ids: number[]): Promise<number | null> {
    if (dst === this.currentTab || !ids.length || !this.tabs.some((t) => t.id === dst)) return null;
    try {
      // A plain array: reactive proxies cannot cross to the worker.
      const r = JSON.parse(await this.engine.call<string>("copy_layers", this.currentTab, dst, Array.from(ids))) as { count: number };
      this.tabs = this.tabs.map((t) => (t.id === dst ? { ...t, dirty: true } : t));
      await this.switchTab(dst);
      this.dirty = true;
      return r.count;
    } catch (e) {
      this.error(e);
      return null;
    }
  }

  async closeTab(id: number) {
    this.saveTabState();
    const next = await this.engine.call<number>("close_document", id);
    let tabs = this.tabs.filter((t) => t.id !== id);
    if (!tabs.some((t) => t.id === next)) {
      tabs = [...tabs, { id: next, name: "Untitled", dirty: false, hasDocument: false, view: { cx: 0, cy: 0, zoom: 1 } }];
    }
    this.tabs = tabs;
    const t = tabs.find((x) => x.id === next)!;
    this.currentTab = next;
    this.fileName = t.name;
    this.dirty = t.dirty;
    this.hasDocument = t.hasDocument;
    this.view = { ...t.view };
    await this.refreshOthers();
  }

  // ---------------------------------------------------------------------
  // Viewport

  /** Space the shell keeps clear over the canvas edges (a prompt bar, a sheet). */
  fitInsets = { top: 0, right: 0, bottom: 0, left: 0 };

  fit(insets: Partial<{ top: number; right: number; bottom: number; left: number }> = {}) {
    const s = this.summary;
    if (!s) return;
    const i = { ...this.fitInsets, ...insets };
    const pad = 32;
    const aw = this.viewport.width - pad * 2 - i.left - i.right;
    const ah = this.viewport.height - pad * 2 - i.top - i.bottom;
    const z = Math.max(0.01, Math.min(Math.min(aw / s.width, ah / s.height), 16));
    // Centre the image in the free area, not the whole viewport.
    const offX = (i.left - i.right) / 2 / z;
    const offY = (i.top - i.bottom) / 2 / z;
    this.view = { cx: s.width / 2 - offX, cy: s.height / 2 - offY, zoom: z };
  }

  actualPixels() {
    const s = this.summary;
    if (!s) return;
    this.view = { ...this.view, zoom: 1 / (window.devicePixelRatio || 1) };
  }

  /** Zoom to `zoom`, keeping the document point under viewport pixel (vx, vy) fixed. */
  zoomAt(zoom: number, vx = this.viewport.width / 2, vy = this.viewport.height / 2) {
    const z = Math.max(0.01, Math.min(64, zoom));
    const { cx, cy, zoom: z0 } = this.view;
    const dx = cx + (vx - this.viewport.width / 2) / z0;
    const dy = cy + (vy - this.viewport.height / 2) / z0;
    this.view = { cx: dx - (vx - this.viewport.width / 2) / z, cy: dy - (vy - this.viewport.height / 2) / z, zoom: z };
  }

  zoomStep(dir: 1 | -1, vx?: number, vy?: number) {
    const z = this.view.zoom;
    const next = dir > 0 ? ZOOM_STEPS.find((s) => s > z * 1.001) ?? 64 : [...ZOOM_STEPS].reverse().find((s) => s < z / 1.001) ?? 0.01;
    this.zoomAt(next, vx, vy);
  }

  panBy(dxCss: number, dyCss: number) {
    this.view = { ...this.view, cx: this.view.cx - dxCss / this.view.zoom, cy: this.view.cy - dyCss / this.view.zoom };
  }

  /** Viewport CSS pixel → document coordinates. */
  toDoc(vx: number, vy: number) {
    const { cx, cy, zoom } = this.view;
    return { x: cx + (vx - this.viewport.width / 2) / zoom, y: cy + (vy - this.viewport.height / 2) / zoom };
  }

  /** Document coordinates → viewport CSS pixel. */
  toView(x: number, y: number) {
    const { cx, cy, zoom } = this.view;
    return { x: (x - cx) * zoom + this.viewport.width / 2, y: (y - cy) * zoom + this.viewport.height / 2 };
  }
}

export const editor = new EditorStore();
