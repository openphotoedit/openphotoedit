// Photoshop files and native projects: opening them into the engine and
// saving the current document back out. Call `installPsdIo()` once at
// start-up; it registers openers for .psd, .psb and .opproj with `io.ts`.

import { editor } from "./editor.svelte";
import { baseName, openers, saveBlob } from "./io";
import type { Summary } from "../engine/types";

export const PROJECT_EXTENSION = "opproj";
export const PROJECT_MIME = "application/vnd.openphotoedit.project+zip";
export const PSD_MIME = "image/vnd.adobe.photoshop";
/** File-picker `accept` for everything this module opens. */
export const PSD_ACCEPT = `.psd,.psb,.${PROJECT_EXTENSION}`;

export interface PsdExportOptions {
  /** Force PSB (large document format). Documents wider or taller than 30,000 px are always PSB. */
  psb?: boolean;
  /** Include the flattened composite that other apps show (default true). */
  maxCompat?: boolean;
}

type SummaryWithWarnings = Summary & { data?: { warnings?: string[] } };

/** Apply a summary returned by `import_psd` / `load_project` and surface its warnings. */
function adopt(json: string, label: string): void {
  const s = JSON.parse(json) as SummaryWithWarnings;
  const warnings = s.data?.warnings ?? [];
  delete s.data;
  editor.summary = s;
  editor.renderTick++;
  if (warnings.length === 1) {
    editor.toast(warnings[0], "info");
  } else if (warnings.length > 1) {
    const [first, ...rest] = warnings;
    editor.toast(`${label} opened with ${warnings.length} notes. ${first}`, "info", {
      label: "Show all",
      run: () => rest.forEach((w) => editor.toast(w, "info")),
    });
    console.info(`${label}:`, warnings);
  }
}

async function openWith(method: "import_psd" | "load_project", file: File): Promise<boolean> {
  editor.busy = { label: `Opening ${file.name}…` };
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const json = await editor.engine.call<string>(method, bytes);
    adopt(json, file.name);
    return true;
  } catch (e) {
    const reason = e instanceof Error ? e.message : String(e);
    editor.toast(`Could not open ${file.name}: ${reason}.`, "error");
    console.error(e);
    return false;
  } finally {
    editor.busy = null;
  }
}

/** Open a PSD or PSB as the current document. */
export function openPsd(file: File): Promise<boolean> {
  return openWith("import_psd", file);
}

/** Open an OpenPhotoEdit project as the current document. */
export function openProject(file: File): Promise<boolean> {
  return openWith("load_project", file);
}

/** The current document as a PSD (or PSB) file. */
export async function exportPsd(opts: PsdExportOptions = {}): Promise<Blob> {
  const options = JSON.stringify({ psb: opts.psb ?? null, max_compat: opts.maxCompat ?? true });
  const bytes = await editor.engine.call<Uint8Array>("export_psd", options);
  return new Blob([bytes as BlobPart], { type: PSD_MIME });
}

/** The current document as a lossless project file. */
export async function saveProject(): Promise<Blob> {
  const bytes = await editor.engine.call<Uint8Array>("save_project");
  return new Blob([bytes as BlobPart], { type: PROJECT_MIME });
}

/** Export and hand the PSD to the save dialog. Returns false if cancelled or failed. */
export async function downloadPsd(opts: PsdExportOptions = {}): Promise<boolean> {
  const ext = opts.psb ? "psb" : "psd";
  editor.busy = { label: `Saving ${ext.toUpperCase()}…` };
  try {
    const blob = await exportPsd(opts);
    const ok = await saveBlob(blob, `${baseName(editor.fileName) || "Untitled"}.${ext}`);
    if (ok) editor.dirty = false;
    return ok;
  } catch (e) {
    editor.error(e);
    return false;
  } finally {
    editor.busy = null;
  }
}

/** Save the project through the save dialog. Returns false if cancelled or failed. */
export async function downloadProject(): Promise<boolean> {
  editor.busy = { label: "Saving project…" };
  try {
    const blob = await saveProject();
    const ok = await saveBlob(blob, `${baseName(editor.fileName) || "Untitled"}.${PROJECT_EXTENSION}`);
    if (ok) editor.dirty = false;
    return ok;
  } catch (e) {
    editor.error(e);
    return false;
  } finally {
    editor.busy = null;
  }
}

let installed = false;

/** Register the .psd, .psb and .opproj openers with `io.ts`. Safe to call more than once. */
export function installPsdIo(): void {
  if (installed) return;
  installed = true;
  openers.psd = openPsd;
  openers.psb = openPsd;
  openers[PROJECT_EXTENSION] = openProject;
}
