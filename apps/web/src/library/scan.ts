// Listing a folder: recursive, images only, without reading pixels.

import { kindOf } from "./decode";
import type { LibraryItem } from "./types";

const SKIP_DIRS = /^(\.|@eaDir$|\$RECYCLE\.BIN$|System Volume Information$|node_modules$)/;

export function extOf(name: string) {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

export function itemKey(path: string, size: number, lastModified: number) {
  return `${path}|${size}|${lastModified}`;
}

type DirEntries = AsyncIterable<[string, FileSystemHandle]>;

export interface ScanResult {
  items: LibraryItem[];
  /** Sidecar `.xmp` files by path (without extension, lower case) → handle or File. */
  sidecars: Map<string, FileSystemFileHandle | File>;
}

function stem(path: string) {
  const i = path.lastIndexOf(".");
  return (i > path.lastIndexOf("/") ? path.slice(0, i) : path).toLowerCase();
}

/** Walk a directory picked with `showDirectoryPicker`. `onProgress` gets the running count. */
export async function scanDirectory(root: FileSystemDirectoryHandle, onProgress?: (n: number) => void, signal?: AbortSignal): Promise<ScanResult> {
  const items: LibraryItem[] = [];
  const sidecars = new Map<string, FileSystemFileHandle | File>();
  const walk = async (dir: FileSystemDirectoryHandle, prefix: string, depth: number) => {
    if (depth > 24) return;
    const subdirs: [string, FileSystemDirectoryHandle][] = [];
    const files: [string, FileSystemFileHandle][] = [];
    for await (const [name, h] of (dir as unknown as { entries(): DirEntries }).entries()) {
      if (signal?.aborted) return;
      if (h.kind === "directory") {
        if (!SKIP_DIRS.test(name)) subdirs.push([name, h as FileSystemDirectoryHandle]);
      } else if (!name.startsWith("._") && !name.startsWith(".")) files.push([name, h as FileSystemFileHandle]);
    }
    // getFile() per entry is cheap; run a few at once.
    const batch = 16;
    for (let i = 0; i < files.length; i += batch) {
      await Promise.all(
        files.slice(i, i + batch).map(async ([name, h]) => {
          const ext = extOf(name);
          const path = prefix + name;
          if (ext === "xmp") {
            sidecars.set(stem(path), h);
            return;
          }
          const kind = kindOf(ext);
          if (!kind) return;
          try {
            const f = await h.getFile();
            items.push({ path, name, ext, kind, size: f.size, lastModified: f.lastModified, key: itemKey(path, f.size, f.lastModified), handle: h, dir });
          } catch {
            /* unreadable (permissions, vanished): skip */
          }
        }),
      );
      onProgress?.(items.length);
    }
    for (const [name, d] of subdirs.sort((a, b) => a[0].localeCompare(b[0]))) await walk(d, prefix + name + "/", depth + 1);
  };
  await walk(root, "", 0);
  for (const it of items) it.hasSidecar = sidecars.has(stem(it.path));
  return { items, sidecars };
}

/** The `<input webkitdirectory>` fallback: a flat FileList with relative paths. */
export function scanFileList(files: FileList | File[]): ScanResult & { rootName: string } {
  const items: LibraryItem[] = [];
  const sidecars = new Map<string, FileSystemFileHandle | File>();
  let rootName = "";
  for (const f of Array.from(files)) {
    const rel = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
    const parts = rel.split("/");
    if (parts.length > 1) {
      rootName ||= parts[0];
      parts.shift();
    }
    if (parts.some((p, i) => p.startsWith(".") || (i < parts.length - 1 && SKIP_DIRS.test(p)))) continue;
    const path = parts.join("/");
    const name = parts[parts.length - 1];
    const ext = extOf(name);
    if (ext === "xmp") {
      sidecars.set(stem(path), f);
      continue;
    }
    const kind = kindOf(ext);
    if (!kind) continue;
    items.push({ path, name, ext, kind, size: f.size, lastModified: f.lastModified, key: itemKey(path, f.size, f.lastModified), file: f });
  }
  for (const it of items) it.hasSidecar = sidecars.has(stem(it.path));
  return { items, sidecars, rootName: rootName || "Folder" };
}

export function sidecarKey(path: string) {
  return stem(path);
}
