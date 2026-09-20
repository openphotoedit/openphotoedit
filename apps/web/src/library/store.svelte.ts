// The library's state: the open folder, its files, marks, thumbnails,
// metadata, culling results, selection and view. One instance (`library`)
// is shared by LibraryView, BatchDialog and the shells.

import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { untrack } from "svelte";
import { t } from "../lib/i18n";
import * as db from "./db";
import { captureTime, type ExifData } from "./exif";
import { closedEyes, judge, luminance, suggestRejects, type CullInput } from "./cull";
import { sources } from "./decode";
import { detectFacesIn, faceStatus, retryFaces } from "./faces";
import { scanDirectory, scanFileList, sidecarKey } from "./scan";
import type { CullResult, Flag, ItemKind, LibraryItem, Marks, QualitySignals } from "./types";
import { NO_MARKS } from "./types";
import { buildXmp, parseXmp, sidecarName } from "./xmp";
import type { ThumbRecord, WorkerRequest, WorkerResponse } from "./thumbs.worker";

export type ViewMode = "grid" | "loupe" | "compare" | "survey";
export type SortKey = "name" | "date" | "rating" | "size";
export type FlagFilter = "all" | "picked" | "rejected" | "unflagged" | "not-rejected";

export interface Filters {
  minRating: number;
  flag: FlagFilter;
  /** Colour label indices to include; empty = any. */
  labels: number[];
  kinds: ItemKind[];
  /** Only photos with a culling badge. */
  badge: "" | "blurry" | "eyes-closed" | "exposure" | "duplicate" | "best";
}

export const DEFAULT_FILTERS: Filters = { minRating: 0, flag: "all", labels: [], kinds: [], badge: "" };

interface Settings {
  thumbSize: number;
  autoAdvance: boolean;
  writeXmp: boolean;
  sort: SortKey;
  sortDesc: boolean;
  rejectDuplicates: boolean;
}

const SETTINGS_KEY = "ops.library.settings";

function loadSettings(): Settings {
  const d: Settings = { thumbSize: 168, autoAdvance: false, writeXmp: false, sort: "name", sortDesc: false, rejectDuplicates: false };
  try {
    return { ...d, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") };
  } catch {
    return d;
  }
}

interface FolderInfo {
  id: string;
  name: string;
  source: "picker" | "input";
  handle?: FileSystemDirectoryHandle;
}

interface MarkChange {
  path: string;
  before: Marks;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export class LibraryStore {
  folder = $state<FolderInfo | null>(null);
  items = $state.raw<LibraryItem[]>([]);
  scanning = $state<{ count: number } | null>(null);
  /** Rating/flag/label per path. */
  marks = new SvelteMap<string, Marks>();
  thumbs = new SvelteMap<string, string>();
  thumbErrors = new SvelteMap<string, string>();
  embedded = new SvelteSet<string>();
  meta = new SvelteMap<string, ExifData>();
  cull = new SvelteMap<string, CullResult>();
  cullRun = $state<{ done: number; total: number; stage: string } | null>(null);
  faces = $state(faceStatus());

  mode = $state<ViewMode>("grid");
  focus = $state<string | null>(null);
  selected = new SvelteSet<string>();
  filters = $state<Filters>({ ...DEFAULT_FILTERS });
  settings = $state<Settings>(loadSettings());
  /** Writable folder handle (the XMP setting needs it). */
  writable = $state(false);
  message = $state<{ text: string; kind: "info" | "error" | "success"; undo?: () => void } | null>(null);

  /** Bumped (throttled) when metadata arrives, so date sort re-runs without thrashing. */
  private metaVersion = $state(0);
  private metaTimer = 0;
  private sidecars = new Map<string, FileSystemFileHandle | File>();
  private undoStack: MarkChange[][] = [];
  private saveTimer = 0;
  private xmpQueue = new Set<string>();
  private xmpTimer = 0;
  private worker: Worker | null = null;
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: Extract<WorkerResponse, { ok: true }>) => void; reject: (e: Error) => void }>();
  private inflight = 0;
  private readonly maxInflight = 6;
  private thumbQueue: LibraryItem[] = [];
  private queued = new Set<string>();
  private wanted = new Set<string>();
  private metaQueue: LibraryItem[] = [];
  private cullAbort: AbortController | null = null;
  private cullIds = new Set<number>();
  private generation = 0;
  private fullUrls = new Map<string, string>();

  /** Files after filters and sort, as the grid shows them. */
  view = $derived.by(() => {
    const items = this.items;
    const f = this.filters;
    const { sort, sortDesc } = this.settings;
    void this.metaVersion;
    void this.marksVersion;
    void this.cull.size;
    return untrack(() => {
      const marks = this.marks;
      let out = items.filter((it) => {
        const m = marks.get(it.path) ?? NO_MARKS;
        if (m.rating < f.minRating) return false;
        if (f.flag === "picked" && m.flag !== 1) return false;
        if (f.flag === "rejected" && m.flag !== -1) return false;
        if (f.flag === "unflagged" && m.flag !== 0) return false;
        if (f.flag === "not-rejected" && m.flag === -1) return false;
        if (f.labels.length && !f.labels.includes(m.label)) return false;
        if (f.kinds.length && !f.kinds.includes(it.kind)) return false;
        if (f.badge) {
          const b = this.cull.get(it.path)?.badges ?? [];
          if (f.badge === "exposure" ? !(b.includes("over") || b.includes("under")) : !b.includes(f.badge)) return false;
        }
        return true;
      });
      const time = (it: LibraryItem) => captureTime(this.meta.get(it.path)) ?? it.lastModified;
      const cmp: Record<SortKey, (a: LibraryItem, b: LibraryItem) => number> = {
        name: (a, b) => collator.compare(a.path, b.path),
        date: (a, b) => time(a) - time(b) || collator.compare(a.path, b.path),
        rating: (a, b) => (marks.get(a.path)?.rating ?? 0) - (marks.get(b.path)?.rating ?? 0) || collator.compare(a.path, b.path),
        size: (a, b) => a.size - b.size || collator.compare(a.path, b.path),
      };
      out = out.sort(cmp[sort]);
      if (sortDesc) out.reverse();
      return out;
    });
  });

  /** Bumped when a mark changes, for components that show marks of many items. */
  marksVersion = $state(0);

  focusIndex = $derived(this.focus ? this.view.findIndex((i) => i.path === this.focus) : -1);
  focused = $derived(this.focusIndex >= 0 ? this.view[this.focusIndex] : null);

  constructor() {
    $effect.root(() => {
      $effect(() => {
        const s = $state.snapshot(this.settings);
        try {
          localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
        } catch {
          /* private mode */
        }
      });
    });
  }

  get supportsPicker() {
    return typeof window !== "undefined" && "showDirectoryPicker" in window;
  }

  // -------------------------------------------------------------------------
  // Opening

  /** Open a folder with the File System Access picker. Returns false if cancelled. */
  async openWithPicker(): Promise<boolean> {
    const w = window as unknown as { showDirectoryPicker?: (o?: unknown) => Promise<FileSystemDirectoryHandle> };
    if (!w.showDirectoryPicker) return false;
    let handle: FileSystemDirectoryHandle;
    try {
      handle = await w.showDirectoryPicker({ id: "ops-library", mode: "read" });
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return false;
      throw e;
    }
    await this.openHandle(handle);
    return true;
  }

  async openHandle(handle: FileSystemDirectoryHandle) {
    const gen = this.reset();
    const known = await db.all<FolderInfo>("folders");
    let id: string | null = null;
    for (const k of known) {
      try {
        if (k.value.handle && (await k.value.handle.isSameEntry(handle))) id = k.value.id;
      } catch {
        /* stale handle */
      }
    }
    id ??= `picker:${handle.name}:${Date.now().toString(36)}`;
    const info: FolderInfo = { id, name: handle.name, source: "picker", handle };
    await db.put("folders", id, { ...info, opened: Date.now() });
    this.folder = info;
    this.writable = (await (handle as unknown as { queryPermission?: (o: unknown) => Promise<string> }).queryPermission?.({ mode: "readwrite" })) === "granted";
    this.scanning = { count: 0 };
    const res = await scanDirectory(handle, (n) => {
      if (gen === this.generation) this.scanning = { count: n };
    });
    if (gen !== this.generation) return;
    await this.loaded(res.items, res.sidecars, gen);
  }

  /** The fallback: files from `<input webkitdirectory>`. Changes cannot be written back. */
  async openFileList(files: FileList | File[]) {
    const gen = this.reset();
    const res = scanFileList(files);
    const id = `input:${res.rootName}`;
    this.folder = { id, name: res.rootName, source: "input" };
    this.writable = false;
    await this.loaded(res.items, res.sidecars, gen);
  }

  /** Reopen the most recent picker folder (asks the browser for permission again). */
  async reopenRecent(): Promise<boolean> {
    const known = (await db.all<FolderInfo & { opened: number }>("folders")).filter((k) => k.value.handle).sort((a, b) => b.value.opened - a.value.opened);
    const h = known[0]?.value.handle as (FileSystemDirectoryHandle & { requestPermission?: (o: unknown) => Promise<string> }) | undefined;
    if (!h) return false;
    if ((await h.requestPermission?.({ mode: "read" })) !== "granted") return false;
    await this.openHandle(h);
    return true;
  }

  async recentFolderName(): Promise<string | null> {
    const known = (await db.all<FolderInfo & { opened: number }>("folders")).filter((k) => k.value.handle).sort((a, b) => b.value.opened - a.value.opened);
    return known[0]?.value.name ?? null;
  }

  private reset() {
    this.cancelCull();
    this.generation++;
    for (const u of this.thumbs.values()) URL.revokeObjectURL(u);
    for (const u of this.fullUrls.values()) URL.revokeObjectURL(u);
    this.fullUrls.clear();
    this.thumbs.clear();
    this.thumbErrors.clear();
    this.embedded.clear();
    this.meta.clear();
    this.marks.clear();
    this.cull.clear();
    this.selected.clear();
    this.thumbQueue = [];
    this.queued.clear();
    this.metaQueue = [];
    this.undoStack = [];
    this.items = [];
    this.focus = null;
    this.mode = "grid";
    return this.generation;
  }

  private async loaded(items: LibraryItem[], sidecars: Map<string, FileSystemFileHandle | File>, gen: number) {
    this.sidecars = sidecars;
    const stored = await db.get<{ marks: Record<string, Marks> }>("marks", this.folder!.id);
    if (gen !== this.generation) return;
    for (const it of items) {
      const m = stored?.marks[it.path];
      if (m) this.marks.set(it.path, m);
    }
    // Sidecars written by other tools fill in photos we have no marks for.
    const withSidecar = items.filter((it) => it.hasSidecar && !this.marks.has(it.path));
    await Promise.all(
      withSidecar.slice(0, 5000).map(async (it) => {
        const h = sidecars.get(sidecarKey(it.path));
        try {
          const file = h instanceof File ? h : await h!.getFile();
          const m = parseXmp(await file.text());
          if (m) this.marks.set(it.path, m);
        } catch {
          /* unreadable sidecar */
        }
      }),
    );
    // Cached analysis for unchanged files.
    const cached = await db.getMany<CullResult>("analysis", items.map((i) => i.key));
    cached.forEach((c, i) => {
      if (c && "badges" in c) this.cull.set(items[i].path, c);
    });
    if (gen !== this.generation) return;
    this.items = items;
    this.scanning = null;
    this.focus = this.view[0]?.path ?? null;
    this.metaQueue = [...items];
    this.pump();
  }

  // -------------------------------------------------------------------------
  // Worker

  private ensureWorker() {
    if (this.worker) return this.worker;
    this.worker = new Worker(new URL("./thumbs.worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const p = this.pending.get(e.data.id);
      if (!p) return;
      this.pending.delete(e.data.id);
      if (e.data.ok) p.resolve(e.data);
      else p.reject(Object.assign(new Error(e.data.error), { cancelled: e.data.cancelled }));
    };
    return this.worker;
  }

  private call(req: Omit<Extract<WorkerRequest, { type: "thumb" | "meta" | "analyze" }>, "id">): { id: number; promise: Promise<Extract<WorkerResponse, { ok: true }>> } {
    const id = this.nextId++;
    const promise = new Promise<Extract<WorkerResponse, { ok: true }>>((resolve, reject) => this.pending.set(id, { resolve, reject }));
    this.ensureWorker().postMessage({ ...req, id });
    return { id, promise };
  }

  async fileOf(item: LibraryItem): Promise<File> {
    if (item.file) return item.file;
    return item.handle!.getFile();
  }

  /** The grid reports which items are on screen (plus a margin); only those get thumbnails first. */
  setVisible(items: LibraryItem[]) {
    this.wanted = new Set(items.map((i) => i.path));
    for (const it of items) {
      if (this.thumbs.has(it.path) || this.thumbErrors.has(it.path) || this.queued.has(it.path)) continue;
      this.queued.add(it.path);
      this.thumbQueue.push(it);
    }
    this.pump();
  }

  /** Ask for one item's thumbnail now (loupe filmstrip, compare). */
  want(item: LibraryItem | null | undefined) {
    if (!item || this.thumbs.has(item.path) || this.queued.has(item.path)) return;
    this.queued.add(item.path);
    this.wanted.add(item.path);
    this.thumbQueue.push(item);
    this.pump();
  }

  private pump() {
    const gen = this.generation;
    while (this.inflight < this.maxInflight) {
      // Visible thumbnails first (most recently requested first), then metadata in the background.
      let idx = -1;
      for (let i = this.thumbQueue.length - 1; i >= 0; i--) {
        if (this.wanted.has(this.thumbQueue[i].path)) {
          idx = i;
          break;
        }
      }
      if (idx >= 0) {
        const it = this.thumbQueue.splice(idx, 1)[0];
        this.inflight++;
        this.runThumb(it, gen).finally(() => {
          this.inflight--;
          if (gen === this.generation) this.pump();
        });
        continue;
      }
      // Drop requests for items that scrolled away; they re-queue when seen again.
      if (this.thumbQueue.length) {
        for (const it of this.thumbQueue) this.queued.delete(it.path);
        this.thumbQueue = [];
      }
      const m = this.metaQueue.shift();
      if (!m) break;
      if (this.meta.has(m.path)) continue;
      this.inflight++;
      this.runMeta(m, gen).finally(() => {
        this.inflight--;
        if (gen === this.generation) this.pump();
      });
    }
  }

  private async runThumb(it: LibraryItem, gen: number) {
    try {
      const file = await this.fileOf(it);
      const r = await this.call({ type: "thumb", key: it.key, file, ext: it.ext, kind: it.kind }).promise;
      if (gen !== this.generation) return;
      const rec = r.thumb as ThumbRecord;
      this.setMeta(it.path, rec.exif);
      if (rec.blob) {
        this.thumbs.set(it.path, URL.createObjectURL(rec.blob));
        if (rec.embedded) this.embedded.add(it.path);
      } else this.thumbErrors.set(it.path, rec.error ?? "no preview");
    } catch (e) {
      if (gen === this.generation) this.thumbErrors.set(it.path, e instanceof Error ? e.message : String(e));
    } finally {
      this.queued.delete(it.path);
    }
  }

  private async runMeta(it: LibraryItem, gen: number) {
    try {
      const file = await this.fileOf(it);
      const r = await this.call({ type: "meta", key: it.key, file, ext: it.ext, kind: it.kind }).promise;
      if (gen === this.generation && r.exif) this.setMeta(it.path, r.exif);
    } catch {
      /* unreadable: the sidebar shows file facts only */
    }
  }

  private setMeta(path: string, exif: ExifData) {
    if (this.meta.has(path)) return;
    this.meta.set(path, exif);
    if (this.settings.sort === "date" && !this.metaTimer) {
      this.metaTimer = window.setTimeout(() => {
        this.metaTimer = 0;
        this.metaVersion++;
      }, 600);
    }
  }

  /** A full-size, browser-drawable URL for the loupe (embedded preview for raws). */
  async fullUrl(item: LibraryItem): Promise<{ url: string; embedded: boolean } | null> {
    const cached = this.fullUrls.get(item.path);
    const file = await this.fileOf(item);
    const src = (await sources(file, item.kind))[0];
    if (!src) return null;
    if (cached) return { url: cached, embedded: src.embedded };
    const url = URL.createObjectURL(src.blob);
    this.fullUrls.set(item.path, url);
    // Keep a handful decoded; the rest are cheap to recreate.
    while (this.fullUrls.size > 8) {
      const [k, u] = this.fullUrls.entries().next().value as [string, string];
      URL.revokeObjectURL(u);
      this.fullUrls.delete(k);
    }
    return { url, embedded: src.embedded };
  }

  // -------------------------------------------------------------------------
  // Selection and navigation

  select(path: string, how: "replace" | "toggle" | "range" = "replace") {
    if (how === "toggle") {
      if (this.selected.has(path)) this.selected.delete(path);
      else this.selected.add(path);
      if (this.focus && !this.selected.size) this.selected.add(this.focus);
    } else if (how === "range" && this.focus) {
      const a = this.view.findIndex((i) => i.path === this.focus);
      const b = this.view.findIndex((i) => i.path === path);
      if (a >= 0 && b >= 0) {
        this.selected.clear();
        for (let i = Math.min(a, b); i <= Math.max(a, b); i++) this.selected.add(this.view[i].path);
      }
      this.focus = path;
      return;
    } else {
      this.selected.clear();
      this.selected.add(path);
    }
    this.focus = path;
  }

  selectAll() {
    this.selected.clear();
    for (const it of this.view) this.selected.add(it.path);
  }

  move(delta: number, extend = false) {
    const v = this.view;
    if (!v.length) return;
    const i = this.focusIndex < 0 ? 0 : Math.max(0, Math.min(v.length - 1, this.focusIndex + delta));
    this.select(v[i].path, extend ? "range" : "replace");
  }

  /** The items a command applies to: the selection when it includes the focus, else the focus. */
  targets(): LibraryItem[] {
    const f = this.focused;
    if (f && this.selected.size > 1 && this.selected.has(f.path)) return this.view.filter((i) => this.selected.has(i.path));
    return f ? [f] : [];
  }

  selectedItems(): LibraryItem[] {
    const s = this.view.filter((i) => this.selected.has(i.path));
    return s.length ? s : this.focused ? [this.focused] : [];
  }

  // -------------------------------------------------------------------------
  // Marks

  marksOf(path: string): Marks {
    return this.marks.get(path) ?? NO_MARKS;
  }

  private change(items: LibraryItem[], fn: (m: Marks) => Marks, opts: { advance?: boolean; label?: string } = {}) {
    if (!items.length) return;
    const changes: MarkChange[] = [];
    for (const it of items) {
      const before = this.marksOf(it.path);
      const after = fn(before);
      if (after.rating === before.rating && after.flag === before.flag && after.label === before.label) continue;
      changes.push({ path: it.path, before });
      this.marks.set(it.path, after);
      this.queueXmp(it.path);
    }
    if (!changes.length) return;
    this.undoStack.push(changes);
    if (this.undoStack.length > 200) this.undoStack.shift();
    this.marksVersion++;
    this.persist();
    if (opts.advance && items.length === 1) this.move(1);
  }

  setRating(r: number, advance = this.settings.autoAdvance) {
    this.change(this.targets(), (m) => ({ ...m, rating: r }), { advance });
  }

  setFlag(f: Flag, advance = this.settings.autoAdvance) {
    this.change(this.targets(), (m) => ({ ...m, flag: f }), { advance });
  }

  /** Toggle a colour label (setting the same label again clears it). */
  toggleLabel(label: number, advance = this.settings.autoAdvance) {
    const items = this.targets();
    const all = items.every((i) => this.marksOf(i.path).label === label);
    this.change(items, (m) => ({ ...m, label: all ? 0 : label }), { advance });
  }

  setMarksFor(path: string, m: Partial<Marks>) {
    const it = this.items.find((i) => i.path === path);
    if (it) this.change([it], (old) => ({ ...old, ...m }));
  }

  undo() {
    const last = this.undoStack.pop();
    if (!last) return false;
    for (const c of last) {
      if (c.before.rating || c.before.flag || c.before.label) this.marks.set(c.path, c.before);
      else this.marks.delete(c.path);
      this.queueXmp(c.path);
    }
    this.marksVersion++;
    this.persist();
    return true;
  }

  private persist() {
    clearTimeout(this.saveTimer);
    const folder = this.folder;
    this.saveTimer = window.setTimeout(() => {
      if (!folder) return;
      const marks: Record<string, Marks> = {};
      for (const [k, v] of this.marks) if (v.rating || v.flag || v.label) marks[k] = { rating: v.rating, flag: v.flag, label: v.label };
      void db.put("marks", folder.id, { marks, saved: Date.now() });
    }, 300);
  }

  /** Flush pending saves (tests and page unload). */
  async flush() {
    clearTimeout(this.saveTimer);
    if (this.folder) {
      const marks: Record<string, Marks> = {};
      for (const [k, v] of this.marks) if (v.rating || v.flag || v.label) marks[k] = { ...v };
      await db.put("marks", this.folder.id, { marks, saved: Date.now() });
    }
    await this.writeXmpNow();
  }

  // -------------------------------------------------------------------------
  // XMP sidecars

  /** Turn sidecar writing on; asks for write access to the folder. */
  async enableXmp(on: boolean): Promise<boolean> {
    if (!on) {
      this.settings.writeXmp = false;
      return true;
    }
    const h = this.folder?.handle as (FileSystemDirectoryHandle & { requestPermission?: (o: unknown) => Promise<string> }) | undefined;
    if (!h) {
      this.notify(t("This folder was opened without write access, so sidecars cannot be saved. Open it with Open folder in Chrome or Edge."), "error");
      return false;
    }
    const granted = (await h.requestPermission?.({ mode: "readwrite" })) === "granted";
    this.writable = granted;
    if (!granted) {
      this.notify(t("Write access was not granted, so sidecars will not be saved."), "error");
      return false;
    }
    this.settings.writeXmp = true;
    for (const [path] of this.marks) this.xmpQueue.add(path);
    await this.writeXmpNow();
    return true;
  }

  private queueXmp(path: string) {
    if (!this.settings.writeXmp || !this.writable) return;
    this.xmpQueue.add(path);
    clearTimeout(this.xmpTimer);
    this.xmpTimer = window.setTimeout(() => void this.writeXmpNow(), 800);
  }

  private async writeXmpNow() {
    if (!this.settings.writeXmp || !this.writable) return;
    const paths = [...this.xmpQueue];
    this.xmpQueue.clear();
    let failed = 0;
    for (const p of paths) {
      const it = this.items.find((i) => i.path === p);
      if (!it?.dir) continue;
      try {
        const fh = await it.dir.getFileHandle(sidecarName(it.name), { create: true });
        const w = await fh.createWritable();
        await w.write(buildXmp(this.marksOf(p)));
        await w.close();
        it.hasSidecar = true;
      } catch {
        failed++;
      }
    }
    if (failed) this.notify(t("Could not write {n} sidecar files. Check the folder is not read-only.", { n: failed }), "error");
  }

  // -------------------------------------------------------------------------
  // Assisted culling

  async runCull(scope: "view" | "selection" = "view") {
    this.cancelCull();
    const gen = this.generation;
    const abort = new AbortController();
    this.cullAbort = abort;
    const items = scope === "selection" ? this.selectedItems() : [...this.view];
    retryFaces();
    this.cullRun = { done: 0, total: items.length, stage: t("Measuring focus and exposure") };
    const signals = new Map<string, QualitySignals>();
    // Cached signals for unchanged files skip the decode.
    const cached = await db.getMany<CullResult & { faceChecked?: boolean }>("analysis", items.map((i) => i.key));
    let next = 0;
    let done = 0;
    const worker = async () => {
      while (!abort.signal.aborted) {
        const i = next++;
        if (i >= items.length) return;
        const it = items[i];
        const c = cached[i];
        const facesOk = faceStatus().status !== "unavailable";
        if (c && (c.faces != null || !facesOk)) {
          signals.set(it.path, c);
        } else {
          try {
            const file = await this.fileOf(it);
            const call = this.call({ type: "analyze", key: it.key, file, ext: it.ext, kind: it.kind, wantPixels: facesOk });
            this.cullIds.add(call.id);
            const r = await call.promise.finally(() => this.cullIds.delete(call.id));
            if (abort.signal.aborted) return;
            if (r.exif) this.setMeta(it.path, r.exif);
            let faces: number | null = null;
            let eyesClosed: number | null = null;
            if (r.pixels) {
              const px = new Uint8ClampedArray(r.pixels.rgba);
              const found = await detectFacesIn(px, r.pixels.width, r.pixels.height);
              this.faces = faceStatus();
              if (found) {
                const strong = found.filter((f) => f.score >= 0.6);
                faces = strong.length;
                eyesClosed = strong.length ? closedEyes(luminance(px, r.pixels.width, r.pixels.height), r.pixels.width, r.pixels.height, strong) : 0;
              }
            }
            signals.set(it.path, { ...r.signals!, faces, eyesClosed, capture: captureTime(r.exif) });
          } catch (e) {
            if ((e as { cancelled?: boolean }).cancelled || abort.signal.aborted) return;
            this.thumbErrors.set(it.path, e instanceof Error ? e.message : String(e));
          }
        }
        done++;
        if (gen === this.generation && this.cullRun) this.cullRun = { ...this.cullRun, done };
      }
    };
    // Face detection is one engine: keep analysis concurrency modest.
    await Promise.all([worker(), worker(), worker()]);
    if (abort.signal.aborted || gen !== this.generation) return;
    this.cullRun = { done, total: items.length, stage: t("Grouping similar shots") };
    const inputs: CullInput[] = [];
    for (const it of items) {
      const s = signals.get(it.path);
      if (s) inputs.push({ ...s, path: it.path, camera: this.meta.get(it.path)?.model, capture: s.capture ?? captureTime(this.meta.get(it.path)) });
    }
    const results = judge(inputs);
    const byPath = new Map(items.map((i) => [i.path, i]));
    for (const [path, r] of results) {
      this.cull.set(path, r);
      const it = byPath.get(path);
      if (it) void db.put("analysis", it.key, $state.snapshot(r));
    }
    this.faces = faceStatus();
    this.cullRun = null;
    this.cullAbort = null;
    this.marksVersion++;
    const blurry = [...results.values()].filter((r) => r.badges.includes("blurry")).length;
    const groups = new Set([...results.values()].map((r) => r.group).filter((g) => g != null)).size;
    this.notify(t("Checked {n} photos: {b} look soft, {g} groups of similar shots.", { n: results.size, b: blurry, g: groups }), "success");
  }

  cancelCull() {
    if (!this.cullAbort) return;
    this.cullAbort.abort();
    this.cullAbort = null;
    if (this.worker && this.cullIds.size) this.worker.postMessage({ type: "cancel", ids: [...this.cullIds] } satisfies WorkerRequest);
    this.cullIds.clear();
    this.cullRun = null;
  }

  /** Flag likely rejects. Never deletes, never overrides a pick; one undo step. */
  suggestRejects() {
    const paths = suggestRejects(new Map([...this.cull].filter(([p]) => this.view.some((v) => v.path === p))), { duplicates: this.settings.rejectDuplicates });
    const items = this.items.filter((i) => paths.includes(i.path) && this.marksOf(i.path).flag === 0);
    if (!items.length) {
      this.notify(t("No likely rejects among unflagged photos."), "info");
      return 0;
    }
    this.change(items, (m) => ({ ...m, flag: -1 }));
    this.notify(t("Flagged {n} photos as rejected. Nothing was deleted.", { n: items.length }), "success", () => this.undo());
    return items.length;
  }

  notify(text: string, kind: "info" | "error" | "success" = "info", undo?: () => void) {
    this.message = { text, kind, undo };
  }

  /** Everything BatchDialog needs about the chosen files. */
  batchInputs(items = this.selectedItems()) {
    return items.map((it) => ({
      name: it.name,
      path: it.path,
      file: () => this.fileOf(it),
      exif: this.meta.get(it.path),
      rating: this.marksOf(it.path).rating,
      kind: it.kind,
    }));
  }
}

export const library = new LibraryStore();
