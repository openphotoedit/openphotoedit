// Shared shapes for the library: files, marks, analysis.

import type { ExifData } from "./exif";

export type ItemKind = "image" | "raw" | "psd" | "tiff" | "heif";

export interface LibraryItem {
  /** Path relative to the opened folder, `/`-separated. Unique within a folder. */
  path: string;
  name: string;
  /** Lower-case extension without the dot. */
  ext: string;
  kind: ItemKind;
  size: number;
  lastModified: number;
  /** Cache key: changes whenever the file does. */
  key: string;
  /** File System Access handles, when the folder was opened with the picker. */
  handle?: FileSystemFileHandle;
  dir?: FileSystemDirectoryHandle;
  /** The File itself, when the folder came from `<input webkitdirectory>`. */
  file?: File;
  /** A sidecar `.xmp` sits next to it. */
  hasSidecar?: boolean;
}

export type Flag = -1 | 0 | 1;

export interface Marks {
  /** 0..5 stars. */
  rating: number;
  /** -1 rejected, 0 unflagged, 1 picked. */
  flag: Flag;
  /** 0 none, 1 red, 2 yellow, 3 green, 4 blue, 5 purple. */
  label: number;
}

export const NO_MARKS: Marks = { rating: 0, flag: 0, label: 0 };

export interface QualitySignals {
  /** Raw focus measure (variance of the Laplacian on the sharpest regions of a 512 px proxy). */
  sharpness: number;
  /** Sharpness relative to the median of photos from the same camera in this folder. */
  sharpnessRel?: number;
  /** Mean luminance 0..1. */
  mean: number;
  /** Fraction of pixels at or near black / white. */
  clipLow: number;
  clipHigh: number;
  /** Difference hash, 16 hex digits. */
  dhash: string;
  /** DCT perceptual hash, 16 hex digits. */
  phash: string;
  /** Faces found, or null when face detection is unavailable. */
  faces: number | null;
  /** Faces whose eyes look closed (a pixel estimate), or null when unknown. */
  eyesClosed: number | null;
  capture?: number;
}

export type Badge = "blurry" | "eyes-closed" | "over" | "under" | "duplicate" | "best";

export interface CullResult extends QualitySignals {
  badges: Badge[];
  /** Duplicate group id (shared by near-identical frames). */
  group?: number;
  groupSize?: number;
  /** 0..1 overall quality used to pick the best of a group. */
  score: number;
}

export interface ItemMeta {
  exif?: ExifData;
  width?: number;
  height?: number;
}

export const LABEL_KEYS = ["", "red", "yellow", "green", "blue", "purple"] as const;
