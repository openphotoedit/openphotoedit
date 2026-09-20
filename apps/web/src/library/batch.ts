// Batch processing: each selected file through an optional action or
// develop preset, then through one or more export recipes (crop, resize,
// watermark, format, name), into a folder or a ZIP.
//
// It runs on its own EngineClient, so the document open in the editor is
// never touched. The export steps mirror `exportBlob` in lib/io.ts, which
// is bound to the shared editor engine.

import { EngineClient } from "../engine/client";
import type { Develop, ExecResult, Summary, TextData } from "../engine/types";
import { t } from "../lib/i18n";
import { DEFAULT_TEXT, layoutText, renderText } from "../lib/text";
import { playSteps, type ActionStep, type ExecTarget } from "./actions.svelte";
import { decodeBitmap } from "./decode";
import type { ExifData } from "./exif";
import { applyTemplate, sanitizeFileName, uniqueName } from "./template";
import type { ItemKind } from "./types";
import { ZipWriter } from "./zip";

export type OutputFormat = "jpeg" | "png" | "webp";
export type ResizeMode = "none" | "long" | "width" | "height" | "fit";
export type WatermarkPosition = "top-left" | "top-right" | "bottom-left" | "bottom-right" | "center";

export interface Recipe {
  id: string;
  name: string;
  enabled: boolean;
  format: OutputFormat;
  /** 0..1 (JPEG, WebP). */
  quality: number;
  /** Largest file size in KB (JPEG, WebP); quality is searched down to fit. */
  maxKB: number | null;
  resize: { mode: ResizeMode; long: number; width: number; height: number; upscale: boolean };
  /** Centre crop to this aspect ratio, "w:h", or null for the original framing. */
  crop: string | null;
  watermark: { enabled: boolean; text: string; size: number; opacity: number; position: WatermarkPosition; color: "white" | "black" };
  /** File name template, see template.ts. */
  template: string;
  /** Subfolder inside the destination ("" = none). */
  folder: string;
}

export interface BatchInput {
  name: string;
  path: string;
  file: () => Promise<File>;
  exif?: ExifData;
  rating?: number;
  kind: ItemKind;
}

export type Destination = { kind: "zip"; zipName?: string } | { kind: "folder"; handle: FileSystemDirectoryHandle };

export interface BatchJob {
  inputs: BatchInput[];
  recipes: Recipe[];
  develop?: Partial<Develop> | null;
  steps?: ActionStep[] | null;
  destination: Destination;
  signal?: AbortSignal;
  onProgress?: (p: BatchProgress) => void;
  /** Reuse an engine (tests); by default a private one is created and released. */
  engine?: EngineClient;
}

export interface BatchProgress {
  done: number;
  total: number;
  current: string;
  stage: string;
}

export interface BatchOutput {
  source: string;
  recipe: string;
  path: string;
  width: number;
  height: number;
  bytes: number;
}

export interface BatchReport {
  outputs: BatchOutput[];
  errors: { source: string; recipe?: string; error: string }[];
  notes: string[];
  zip?: Blob;
  zipName?: string;
  cancelled: boolean;
}

const uid = () => Math.random().toString(36).slice(2, 10);

export function makeRecipe(p: Partial<Recipe> & { name: string }): Recipe {
  return {
    id: uid(),
    enabled: true,
    format: "jpeg",
    quality: 0.86,
    maxKB: null,
    resize: { mode: "none", long: 2048, width: 1080, height: 1350, upscale: false },
    crop: null,
    watermark: { enabled: false, text: "© ", size: 3, opacity: 0.7, position: "bottom-right", color: "white" },
    template: "{name}",
    folder: "",
    ...p,
  };
}

export const RECIPE_PRESETS: Recipe[] = [
  makeRecipe({ name: "Web 2048", resize: { mode: "long", long: 2048, width: 2048, height: 2048, upscale: false }, template: "{name}_web", folder: "Web 2048" }),
  makeRecipe({ name: "Instagram 4:5", crop: "4:5", resize: { mode: "fit", long: 1350, width: 1080, height: 1350, upscale: false }, quality: 0.9, template: "{name}_ig", folder: "Instagram" }),
  makeRecipe({ name: "Email under 300 KB", resize: { mode: "long", long: 1600, width: 1600, height: 1600, upscale: false }, maxKB: 300, template: "{name}_email", folder: "Email" }),
  makeRecipe({ name: "Full size PNG", format: "png", template: "{name}", folder: "PNG" }),
];

export const EXT: Record<OutputFormat, string> = { jpeg: "jpg", png: "png", webp: "webp" };

/** Centre crop rectangle for a ratio "w:h". */
export function cropRect(w: number, h: number, ratio: string | null): { x: number; y: number; width: number; height: number } | null {
  if (!ratio) return null;
  const m = /^(\d+(?:\.\d+)?)\s*[:x/]\s*(\d+(?:\.\d+)?)$/.exec(ratio.trim());
  if (!m) return null;
  const r = Number(m[1]) / Number(m[2]);
  if (!(r > 0)) return null;
  let cw = w;
  let ch = Math.round(w / r);
  if (ch > h) {
    ch = h;
    cw = Math.round(h * r);
  }
  if (cw === w && ch === h) return null;
  return { x: Math.floor((w - cw) / 2), y: Math.floor((h - ch) / 2), width: cw, height: ch };
}

/** Output size for a resize rule, never upscaling unless asked. */
export function resizeTo(w: number, h: number, r: Recipe["resize"]): { width: number; height: number } | null {
  let s = 1;
  switch (r.mode) {
    case "none":
      return null;
    case "long":
      s = r.long / Math.max(w, h);
      break;
    case "width":
      s = r.width / w;
      break;
    case "height":
      s = r.height / h;
      break;
    case "fit":
      s = Math.min(r.width / w, r.height / h);
      break;
  }
  if (!(s > 0) || (!r.upscale && s >= 1)) return null;
  const width = Math.max(1, Math.round(w * s));
  const height = Math.max(1, Math.round(h * s));
  return width === w && height === h ? null : { width, height };
}

class Target implements ExecTarget {
  last: Summary | null = null;
  constructor(public engine: EngineClient) {
    engine.onSummary = (s) => (this.last = s);
  }
  exec(cmd: Record<string, unknown>, bytes?: Uint8Array | Uint8ClampedArray): Promise<ExecResult> {
    return this.engine.exec(cmd, bytes);
  }
  summary() {
    return this.last;
  }
}

async function encode(engine: EngineClient, rgba: Uint8Array, w: number, h: number, format: OutputFormat, quality: number, maxBytes?: number): Promise<Blob> {
  if (format === "png") {
    const bytes = await engine.call<Uint8Array>("encode_png", rgba, w, h);
    return new Blob([bytes as BlobPart], { type: "image/png" });
  }
  const src = new OffscreenCanvas(w, h);
  src.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength), w, h), 0, 0);
  let out = src;
  if (format === "jpeg") {
    out = new OffscreenCanvas(w, h);
    const g = out.getContext("2d")!;
    g.fillStyle = "#ffffff";
    g.fillRect(0, 0, w, h);
    g.drawImage(src, 0, 0);
  }
  const type = format === "jpeg" ? "image/jpeg" : "image/webp";
  let blob = await out.convertToBlob({ type, quality });
  if (maxBytes && blob.size > maxBytes) {
    let lo = 0.05;
    let hi = quality;
    let best: Blob | null = null;
    for (let i = 0; i < 7; i++) {
      const q = (lo + hi) / 2;
      const b = await out.convertToBlob({ type, quality: q });
      if (b.size <= maxBytes) {
        best = b;
        lo = q;
      } else hi = q;
    }
    blob = best ?? (await out.convertToBlob({ type, quality: 0.05 }));
  }
  return blob;
}

async function watermarkLayer(wm: Recipe["watermark"], w: number, h: number) {
  const size = Math.max(8, Math.round((Math.min(w, h) * wm.size) / 100));
  const alpha = Math.round(Math.max(0, Math.min(1, wm.opacity)) * 255);
  const c = wm.color === "black" ? { r: 0, g: 0, b: 0, a: alpha } : { r: 255, g: 255, b: 255, a: alpha };
  const data: TextData = { ...DEFAULT_TEXT, text: wm.text, font_family: "Geist, Inter, system-ui, sans-serif", font_size: size, font_weight: 500, color: c, x: 0, y: 0 };
  const lay = layoutText(data);
  const margin = Math.round(Math.min(w, h) * 0.03);
  const right = w - margin - lay.width;
  const bottom = h - margin - lay.height;
  const pos = { "top-left": [margin, margin], "top-right": [right, margin], "bottom-left": [margin, bottom], "bottom-right": [right, bottom], center: [(w - lay.width) / 2, (h - lay.height) / 2] }[wm.position];
  data.x = Math.round(pos[0]);
  data.y = Math.round(pos[1]);
  const r = await renderText(data);
  return { data, raster: r };
}

async function uniqueIn(dir: FileSystemDirectoryHandle, cache: Map<FileSystemDirectoryHandle, Set<string>>) {
  let taken = cache.get(dir);
  if (!taken) {
    taken = new Set();
    for await (const [name] of (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) taken.add(name.toLowerCase());
    cache.set(dir, taken);
  }
  return taken;
}

async function subdir(root: FileSystemDirectoryHandle, path: string) {
  let d = root;
  for (const part of path.split("/").map((p) => sanitizeFileName(p)).filter(Boolean)) d = await d.getDirectoryHandle(part, { create: true });
  return d;
}

export async function runBatch(job: BatchJob): Promise<BatchReport> {
  const report: BatchReport = { outputs: [], errors: [], notes: [], cancelled: false };
  const recipes = job.recipes.filter((r) => r.enabled);
  const total = job.inputs.length;
  const progress = (done: number, current: string, stage: string) => job.onProgress?.({ done, total, current, stage });
  if (!recipes.length) throw new Error(t("Turn on at least one export recipe."));
  const ownEngine = !job.engine;
  const engine = job.engine ?? new EngineClient();
  const target = new Target(engine);
  const zip = job.destination.kind === "zip" ? new ZipWriter() : null;
  const zipTaken = new Set<string>();
  const dirTaken = new Map<FileSystemDirectoryHandle, Set<string>>();
  try {
    progress(0, "", t("Starting the batch engine"));
    target.last = ownEngine ? await engine.init() : target.last;
    for (let i = 0; i < total; i++) {
      const input = job.inputs[i];
      if (job.signal?.aborted) {
        report.cancelled = true;
        break;
      }
      progress(i, input.name, t("Opening"));
      try {
        const file = await input.file();
        const { bitmap, embedded } = await decodeBitmap(file, input.kind, 0, input.exif);
        if (embedded && input.kind === "psd") {
          bitmap.close();
          throw new Error(t("PSD files need the editor's PSD import; open it in the editor instead."));
        }
        if (embedded) report.notes.push(t("{name}: used the camera's embedded JPEG, not a raw development.", { name: input.name }));
        const c = new OffscreenCanvas(bitmap.width, bitmap.height);
        const g = c.getContext("2d", { willReadFrequently: true })!;
        g.drawImage(bitmap, 0, 0);
        const img = g.getImageData(0, 0, bitmap.width, bitmap.height);
        bitmap.close();
        await target.exec({ op: "doc.open-pixels", width: img.width, height: img.height, name: input.name }, img.data);
        if (job.develop && Object.values(job.develop).some((v) => v)) {
          progress(i, input.name, t("Developing"));
          await target.exec({ op: "layer.add-adjustment", name: "Develop", adjustment: { kind: "develop", ...job.develop } });
        }
        if (job.steps?.length) {
          progress(i, input.name, t("Playing the action"));
          const r = await playSteps(job.steps, target, { signal: job.signal });
          for (const n of r.notes) report.notes.push(`${input.name}: ${n}`);
        }
        const base = target.summary()!.history.undo.length;
        for (const recipe of recipes) {
          if (job.signal?.aborted) break;
          progress(i, input.name, t("Exporting {recipe}", { recipe: recipe.name }));
          try {
            let s = target.summary()!;
            const crop = cropRect(s.width, s.height, recipe.crop);
            if (crop) s = (await target.exec({ op: "image.crop", ...crop }), target.summary()!);
            const size = resizeTo(s.width, s.height, recipe.resize);
            if (size) s = (await target.exec({ op: "image.resize", ...size, resample: "lanczos" }), target.summary()!);
            if (recipe.watermark.enabled && recipe.watermark.text.trim()) {
              const { data, raster } = await watermarkLayer(recipe.watermark, s.width, s.height);
              await target.exec({ op: "layer.add-text", data, width: raster.width, height: raster.height, x: raster.x, y: raster.y }, raster.rgba);
            }
            const rgba = await engine.call<Uint8Array>("flatten");
            const blob = await encode(engine, rgba, s.width, s.height, recipe.format, recipe.quality, recipe.maxKB ? recipe.maxKB * 1024 : undefined);
            const ext = EXT[recipe.format];
            const stem = applyTemplate(recipe.template, {
              name: input.name.replace(/\.[^.]+$/, ""),
              n: i + 1,
              date: input.exif?.dateTaken ?? new Date(file.lastModified),
              w: s.width,
              h: s.height,
              recipe: recipe.name,
              rating: input.rating,
              camera: input.exif?.model,
              ext,
            });
            const folder = recipe.folder.split("/").map((p) => (p ? sanitizeFileName(p) : "")).filter(Boolean).join("/");
            let outPath: string;
            if (zip) {
              const scoped = new Set([...zipTaken].filter((n) => n.startsWith(folder.toLowerCase() + "/")).map((n) => n.slice(folder.length + 1)));
              const name = uniqueName(stem, ext, folder ? scoped : zipTaken);
              outPath = folder ? `${folder}/${name}` : name;
              zipTaken.add(outPath.toLowerCase());
              await zip.add(outPath, blob, new Date());
            } else {
              const dest = job.destination as Extract<Destination, { kind: "folder" }>;
              const dir = folder ? await subdir(dest.handle, folder) : dest.handle;
              const name = uniqueName(stem, ext, await uniqueIn(dir, dirTaken));
              const fh = await dir.getFileHandle(name, { create: true });
              const w = await fh.createWritable();
              await w.write(blob);
              await w.close();
              outPath = folder ? `${folder}/${name}` : name;
            }
            report.outputs.push({ source: input.path, recipe: recipe.name, path: outPath, width: s.width, height: s.height, bytes: blob.size });
            if (recipe.maxKB && blob.size > recipe.maxKB * 1024) report.notes.push(t("{name}: could not get under {kb} KB at the lowest quality.", { name: outPath, kb: recipe.maxKB }));
          } catch (e) {
            report.errors.push({ source: input.path, recipe: recipe.name, error: e instanceof Error ? e.message : String(e) });
          } finally {
            const undo = target.summary()?.history.undo.length ?? base;
            if (undo > base) await target.exec({ op: "edit.history-go", index: base });
          }
        }
      } catch (e) {
        if (e instanceof Error && e.message === "cancelled") {
          report.cancelled = true;
          break;
        }
        report.errors.push({ source: input.path, error: e instanceof Error ? e.message : String(e) });
      }
      progress(i + 1, input.name, t("Done"));
    }
    if (job.signal?.aborted) report.cancelled = true;
    if (zip && zip.count) {
      report.zip = zip.finish();
      report.zipName = (job.destination as { zipName?: string }).zipName ?? "export.zip";
    }
    return report;
  } finally {
    if (ownEngine) (engine as unknown as { worker?: Worker }).worker?.terminate();
  }
}
