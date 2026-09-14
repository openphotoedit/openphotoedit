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

export const ZOOM_STEPS = [0.01, 0.02, 0.03, 0.05, 0.0833, 0.125, 0.1667, 0.25, 0.3333, 0.5, 0.6667, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 32];

export class EditorStore {
  engine = new EngineClient();
  ready = $state(false);
  summary = $state<Summary | null>(null);
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

  /** Run a command; failures become an error toast and resolve to null. */
  async exec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray, opts: { quiet?: boolean } = {}): Promise<ExecResult | null> {
    try {
      const r = await this.engine.exec(cmd, bytes);
      if (r.changed) this.dirty = true;
      return r;
    } catch (e) {
      if (!opts.quiet) this.error(e);
      return null;
    }
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
  // Viewport

  fit() {
    const s = this.summary;
    if (!s) return;
    const pad = 32;
    const z = Math.min((this.viewport.width - pad * 2) / s.width, (this.viewport.height - pad * 2) / s.height);
    this.view = { cx: s.width / 2, cy: s.height / 2, zoom: Math.max(0.01, Math.min(z, 16)) };
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
