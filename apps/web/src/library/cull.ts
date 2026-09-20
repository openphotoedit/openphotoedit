// Assisted culling: quality signals computed locally from a small proxy,
// near-duplicate grouping, a best-of-group pick and badges. Pure functions,
// shared by the thumbnail worker (the pixel work) and the store (grouping),
// and covered by unit tests.
//
// Nothing here deletes or rejects anything by itself; `suggestRejects`
// returns paths and the store flags them, which the photographer can undo.

import type { Badge, CullResult, QualitySignals } from "./types";

/** Long edge of the luminance proxy every signal is measured on. */
export const PROXY_EDGE = 512;

/** Rec. 709 luma of straight RGBA, 0..255. */
export function luminance(rgba: Uint8Array | Uint8ClampedArray, w: number, h: number): Uint8Array {
  const out = new Uint8Array(w * h);
  for (let i = 0, j = 0; j < out.length; i += 4, j++) {
    out[j] = (rgba[i] * 0.2126 + rgba[i + 1] * 0.7152 + rgba[i + 2] * 0.0722 + 0.5) | 0;
  }
  return out;
}

/** Area-average resample of a single-channel image. */
export function resample(src: Uint8Array | Float32Array, w: number, h: number, nw: number, nh: number): Float32Array {
  const out = new Float32Array(nw * nh);
  const sx = w / nw;
  const sy = h / nh;
  for (let y = 0; y < nh; y++) {
    const y0 = Math.floor(y * sy);
    const y1 = Math.max(y0 + 1, Math.min(h, Math.floor((y + 1) * sy)));
    for (let x = 0; x < nw; x++) {
      const x0 = Math.floor(x * sx);
      const x1 = Math.max(x0 + 1, Math.min(w, Math.floor((x + 1) * sx)));
      let s = 0;
      for (let yy = y0; yy < y1; yy++) {
        const row = yy * w;
        for (let xx = x0; xx < x1; xx++) s += src[row + xx];
      }
      out[y * nw + x] = s / ((y1 - y0) * (x1 - x0));
    }
  }
  return out;
}

/**
 * Focus measure: variance of the 4-neighbour Laplacian, taken per tile on a
 * 4×4 grid, averaging the three sharpest tiles (a portrait with a soft
 * background is judged on the subject, not on the bokeh), divided by the
 * image's luminance variance so a dark or flat frame is not mistaken for a
 * soft one. ×100 for readable numbers: sharp photos measure ~20–300, a
 * missed focus under ~5.
 */
export function sharpness(lum: Uint8Array, w: number, h: number, grid = 4): number {
  if (w < 8 || h < 8) return 0;
  const tiles = grid * grid;
  let gs = 0;
  let gq = 0;
  for (let i = 0; i < w * h; i++) {
    gs += lum[i];
    gq += lum[i] * lum[i];
  }
  const gm = gs / (w * h);
  const contrast = Math.max(25, gq / (w * h) - gm * gm);
  const sum = new Float64Array(tiles);
  const sq = new Float64Array(tiles);
  const cnt = new Uint32Array(tiles);
  const tw = (w - 2) / grid;
  const th = (h - 2) / grid;
  for (let y = 1; y < h - 1; y++) {
    const ty = Math.min(grid - 1, Math.floor((y - 1) / th));
    const row = y * w;
    for (let x = 1; x < w - 1; x++) {
      const i = row + x;
      const l = lum[i - 1] + lum[i + 1] + lum[i - w] + lum[i + w] - 4 * lum[i];
      const t = ty * grid + Math.min(grid - 1, Math.floor((x - 1) / tw));
      sum[t] += l;
      sq[t] += l * l;
      cnt[t]++;
    }
  }
  const vars: number[] = [];
  for (let t = 0; t < tiles; t++) {
    if (!cnt[t]) continue;
    const m = sum[t] / cnt[t];
    vars.push(sq[t] / cnt[t] - m * m);
  }
  vars.sort((a, b) => b - a);
  const top = vars.slice(0, Math.min(3, vars.length));
  return ((top.reduce((a, b) => a + b, 0) / Math.max(1, top.length)) / contrast) * 100;
}

export function exposure(lum: Uint8Array): { mean: number; clipLow: number; clipHigh: number } {
  let s = 0;
  let lo = 0;
  let hi = 0;
  for (let i = 0; i < lum.length; i++) {
    const v = lum[i];
    s += v;
    if (v <= 4) lo++;
    else if (v >= 251) hi++;
  }
  const n = Math.max(1, lum.length);
  return { mean: s / n / 255, clipLow: lo / n, clipHigh: hi / n };
}

function toHex(bits: Uint8Array): string {
  let s = "";
  for (let i = 0; i < 64; i += 4) s += ((bits[i] << 3) | (bits[i + 1] << 2) | (bits[i + 2] << 1) | bits[i + 3]).toString(16);
  return s;
}

/** Difference hash: 9×8 area-average, one bit per horizontal gradient sign. */
export function dHash(lum: Uint8Array, w: number, h: number): string {
  const small = resample(lum, w, h, 9, 8);
  const bits = new Uint8Array(64);
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) bits[y * 8 + x] = small[y * 9 + x] > small[y * 9 + x + 1] ? 1 : 0;
  return toHex(bits);
}

const DCT_N = 32;
const DCT_COS = (() => {
  const c = new Float32Array(8 * DCT_N);
  for (let u = 0; u < 8; u++) for (let x = 0; x < DCT_N; x++) c[u * DCT_N + x] = Math.cos(((2 * x + 1) * u * Math.PI) / (2 * DCT_N));
  return c;
})();

/** Perceptual hash: low 8×8 DCT coefficients of a 32×32 proxy against their median. */
export function pHash(lum: Uint8Array, w: number, h: number): string {
  const small = resample(lum, w, h, DCT_N, DCT_N);
  // Separable DCT restricted to the 8 lowest frequencies.
  const rows = new Float32Array(DCT_N * 8);
  for (let y = 0; y < DCT_N; y++)
    for (let u = 0; u < 8; u++) {
      let s = 0;
      for (let x = 0; x < DCT_N; x++) s += small[y * DCT_N + x] * DCT_COS[u * DCT_N + x];
      rows[y * 8 + u] = s;
    }
  const coef = new Float32Array(64);
  for (let v = 0; v < 8; v++)
    for (let u = 0; u < 8; u++) {
      let s = 0;
      for (let y = 0; y < DCT_N; y++) s += rows[y * 8 + u] * DCT_COS[v * DCT_N + y];
      coef[v * 8 + u] = s;
    }
  const ac = [...coef.slice(1)].sort((a, b) => a - b);
  const median = (ac[31] + ac[32]) / 2;
  const bits = new Uint8Array(64);
  for (let i = 0; i < 64; i++) bits[i] = coef[i] > median ? 1 : 0;
  return toHex(bits);
}

const POP4 = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4];

export function hamming(a: string, b: string): number {
  if (a.length !== b.length) return 64;
  let d = 0;
  for (let i = 0; i < a.length; i++) d += POP4[parseInt(a[i], 16) ^ parseInt(b[i], 16)];
  return d;
}

/**
 * How open an eye looks, from pixels around a landmark: an open eye has a
 * dark iris against brighter sclera and skin, a closed one is a lash line in
 * skin. Returns the fraction of the patch that is clearly darker than the
 * patch's median. An estimate for 5-point landmarks; not calibrated.
 */
export function eyeOpenness(lum: Uint8Array, w: number, h: number, ex: number, ey: number, interocular: number): number {
  const r = Math.max(2, Math.round(interocular * 0.16));
  const vals: number[] = [];
  for (let y = Math.max(0, Math.round(ey - r)); y <= Math.min(h - 1, Math.round(ey + r)); y++)
    for (let x = Math.max(0, Math.round(ex - r)); x <= Math.min(w - 1, Math.round(ex + r)); x++) vals.push(lum[y * w + x]);
  if (vals.length < 9) return 1;
  const sorted = [...vals].sort((a, b) => a - b);
  const med = sorted[sorted.length >> 1];
  const dark = vals.filter((v) => v < med * 0.55).length;
  return dark / vals.length;
}

export interface FaceBox {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
  landmarks: { x: number; y: number }[];
}

/** Faces whose eyes both look closed. */
export function closedEyes(lum: Uint8Array, w: number, h: number, faces: FaceBox[]): number {
  let closed = 0;
  for (const f of faces) {
    if (f.landmarks.length < 2 || f.h < 24) continue;
    const [a, b] = f.landmarks;
    const iod = Math.hypot(a.x - b.x, a.y - b.y);
    if (iod < 8) continue;
    const oa = eyeOpenness(lum, w, h, a.x, a.y, iod);
    const ob = eyeOpenness(lum, w, h, b.x, b.y, iod);
    if (oa < 0.03 && ob < 0.03) closed++;
  }
  return closed;
}

/** Every pixel signal for one proxy. Faces are filled in later when a detector exists. */
export function measure(rgba: Uint8Array | Uint8ClampedArray, w: number, h: number): Omit<QualitySignals, "faces" | "eyesClosed" | "capture"> {
  const lum = luminance(rgba, w, h);
  return { sharpness: sharpness(lum, w, h), ...exposure(lum), dhash: dHash(lum, w, h), phash: pHash(lum, w, h) };
}

// ---------------------------------------------------------------------------
// Folder-level judgement

export interface CullInput extends QualitySignals {
  path: string;
  camera?: string;
}

export interface CullOptions {
  /** Max dHash distance for near-duplicates. */
  dhashMax?: number;
  /** Max pHash distance for near-duplicates. */
  phashMax?: number;
  /** Frames further apart than this (ms) must be closer in hash to group. */
  timeWindow?: number;
}

/** Below this focus measure a 512 px proxy is soft whatever the scene. */
export const BLUR_ABS = 6;
/** Below this fraction of the camera's median a frame is soft for this shoot. */
export const BLUR_REL = 0.3;
/** Within a duplicate group, a frame this much softer than the sharpest is a missed focus. */
export const BLUR_GROUP = 0.4;

function median(v: number[]) {
  if (!v.length) return 0;
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[s.length >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
}

export function exposureBadge(s: Pick<QualitySignals, "mean" | "clipLow" | "clipHigh">): "over" | "under" | null {
  if (s.mean > 0.86 || (s.clipHigh > 0.12 && s.mean > 0.5)) return "over";
  if (s.mean < 0.09 || (s.clipLow > 0.25 && s.mean < 0.25)) return "under";
  return null;
}

/** Group near-duplicates (union-find over hash distance and capture time). Returns a group index per input, -1 for singletons. */
export function groupDuplicates(items: Pick<CullInput, "dhash" | "phash" | "capture">[], opts: CullOptions = {}): number[] {
  const dMax = opts.dhashMax ?? 12;
  const pMax = opts.phashMax ?? 14;
  const win = opts.timeWindow ?? 5 * 60_000;
  const n = items.length;
  const parent = Int32Array.from({ length: n }, (_, i) => i);
  const find = (i: number): number => {
    while (parent[i] !== i) {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    return i;
  };
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const a = items[i];
      const b = items[j];
      const dh = hamming(a.dhash, b.dhash);
      if (dh > dMax) continue;
      const ph = hamming(a.phash, b.phash);
      if (ph > pMax) continue;
      const far = a.capture != null && b.capture != null && Math.abs(a.capture - b.capture) > win;
      // Far apart in time: only near-identical frames count (re-exports, copies).
      if (far && (dh > dMax / 2 || ph > pMax / 2)) continue;
      parent[find(i)] = find(j);
    }
  }
  const roots = new Map<number, number[]>();
  for (let i = 0; i < n; i++) {
    const r = find(i);
    if (!roots.has(r)) roots.set(r, []);
    roots.get(r)!.push(i);
  }
  const out = new Array<number>(n).fill(-1);
  let g = 0;
  for (const members of roots.values()) {
    if (members.length < 2) continue;
    for (const m of members) out[m] = g;
    g++;
  }
  return out;
}

/** Score, badges, groups and best picks for a folder. */
export function judge(items: CullInput[], opts: CullOptions = {}): Map<string, CullResult> {
  const byCamera = new Map<string, number[]>();
  for (const it of items) {
    const k = it.camera || "";
    if (!byCamera.has(k)) byCamera.set(k, []);
    byCamera.get(k)!.push(it.sharpness);
  }
  const folderMedian = median(items.map((i) => i.sharpness));
  const groups = groupDuplicates(items, opts);
  const results: CullResult[] = items.map((it) => {
    const cam = byCamera.get(it.camera || "")!;
    const ref = cam.length >= 3 ? median(cam) : folderMedian;
    const rel = ref > 0 ? it.sharpness / ref : 1;
    const badges: Badge[] = [];
    const blurry = it.sharpness < BLUR_ABS || (rel < BLUR_REL && it.sharpness < BLUR_ABS * 4);
    if (blurry) badges.push("blurry");
    if (it.eyesClosed) badges.push("eyes-closed");
    const exp = exposureBadge(it);
    if (exp) badges.push(exp);
    const sharpTerm = Math.max(0, Math.min(1, Math.log1p(it.sharpness) / Math.log1p(Math.max(ref, BLUR_ABS) * 2)));
    const expPenalty = Math.min(0.5, it.clipHigh * 2 + it.clipLow * 1.5 + Math.max(0, Math.abs(it.mean - 0.46) - 0.2));
    const eyePenalty = it.eyesClosed && it.faces ? 0.35 * (it.eyesClosed / it.faces) : 0;
    const score = Math.max(0, Math.min(1, 0.75 * sharpTerm + 0.25 - expPenalty - eyePenalty - (blurry ? 0.2 : 0)));
    return { ...it, sharpnessRel: rel, badges, score };
  });
  const members = new Map<number, number[]>();
  groups.forEach((g, i) => {
    if (g < 0) return;
    if (!members.has(g)) members.set(g, []);
    members.get(g)!.push(i);
  });
  for (const [g, idx] of members) {
    // Badly exposed frames measure falsely sharp (clipping adds edges); compare against well-exposed ones.
    const ref = idx.filter((i) => !results[i].badges.includes("over") && !results[i].badges.includes("under"));
    const maxSharp = Math.max(0, ...(ref.length ? ref : idx).map((i) => results[i].sharpness));
    let best = idx[0];
    for (const i of idx) {
      const r = results[i];
      r.group = g;
      r.groupSize = idx.length;
      r.badges.push("duplicate");
      if (r.sharpness < maxSharp * BLUR_GROUP && r.sharpness < BLUR_ABS * 4 && !r.badges.includes("blurry")) r.badges.unshift("blurry");
      if (r.score > results[best].score) best = i;
    }
    results[best].badges.push("best");
  }
  const out = new Map<string, CullResult>();
  items.forEach((it, i) => {
    const { path: _p, camera: _c, ...rest } = results[i] as CullResult & { path?: string; camera?: string };
    out.set(it.path, rest);
  });
  return out;
}

/** Paths worth rejecting: soft, eyes closed, badly exposed; optionally the non-best frames of each group. */
export function suggestRejects(results: Map<string, CullResult>, opts: { duplicates?: boolean } = {}): string[] {
  const out: string[] = [];
  for (const [path, r] of results) {
    const bad = r.badges.includes("blurry") || r.badges.includes("eyes-closed") || r.badges.includes("over") || r.badges.includes("under");
    const extra = opts.duplicates && r.badges.includes("duplicate") && !r.badges.includes("best");
    if (bad || extra) out.push(path);
  }
  return out;
}
