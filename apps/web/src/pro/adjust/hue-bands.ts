// Hue/Saturation range bands: Photoshop's four handles per colour range
// (falloff start, range start, range end, falloff end; degrees, wrapping at
// 360) and the edits the band bar and eyedroppers make to them.
//
// `weight`, `centered`, `include`, `exclude` and `setHandle` are ported from
// Compositor's HueBand (Compositor/Document/HueSaturation.swift, MIT licence,
// © the Compositor authors). `weight` mirrors `hue_band_weight` in
// crates/editor-core/src/adjust.rs, and `hueSatApply` mirrors
// `HueSaturation::apply` there (used for the before/after spectrum).

export type Band = [number, number, number, number];

export const DEFAULT_BANDS: Band[] = [
  [315, 345, 15, 45],
  [15, 45, 75, 105],
  [75, 105, 135, 165],
  [135, 165, 195, 225],
  [195, 225, 255, 285],
  [255, 285, 315, 345],
];

/** Hue of each range's swatch. */
export const RANGE_HUES = [0, 60, 120, 180, 240, 300];

const wrap = (v: number) => ((v % 360) + 360) % 360;

/** Degrees from `from` forward to `to`, in 0..360. */
export const forward = (from: number, to: number) => wrap(to - from);

/** 1 on the plateau, a straight line to 0 across each shoulder. */
export function weight(band: Band, hue: number): number {
  const [fs, rs, re, fe] = band;
  const span = forward(fs, fe);
  if (span <= 0) return 1;
  const pos = forward(fs, hue);
  if (pos > span) return 0;
  const rampIn = Math.min(forward(fs, rs), span);
  const plateauEnd = Math.max(Math.min(forward(fs, re), span), rampIn);
  if (pos < rampIn) return pos / rampIn;
  if (pos <= plateauEnd) return 1;
  const rampOut = span - plateauEnd;
  return rampOut > 0 ? (span - pos) / rampOut : 1;
}

/** Keep all four handles in 0..360 and the band under 350° wide. */
function normalize(b: Band): Band {
  const out = b.map(wrap) as Band;
  if (forward(out[0], out[3]) > 350) out[3] = wrap(out[0] + 350);
  return out;
}

/** The same band shape, centred on `hue`. */
export function centered(band: Band, hue: number): Band {
  const [fs, rs, re, fe] = band;
  const core = forward(rs, re);
  const lead = forward(fs, rs);
  const trail = forward(re, fe);
  const start = wrap(hue - core / 2);
  return [wrap(start - lead), start, wrap(start + core), wrap(start + core + trail)];
}

/** Widen the band so `hue` is at full strength, moving the nearer edge. */
export function include(band: Band, hue: number): Band {
  if (weight(band, hue) >= 1) return band;
  let [fs, rs, re, fe] = band;
  const shoulderIn = forward(fs, rs);
  const shoulderOut = forward(re, fe);
  if (forward(hue, rs) <= forward(re, hue)) {
    rs = hue;
    fs = hue - shoulderIn;
  } else {
    re = hue;
    fe = hue + shoulderOut;
  }
  return normalize([fs, rs, re, fe]);
}

/** Narrow the band so `hue` falls outside it, shoulder included. */
export function exclude(band: Band, hue: number): Band {
  if (weight(band, hue) <= 0) return band;
  let [fs, rs, re, fe] = band;
  const shoulderIn = forward(fs, rs);
  const shoulderOut = forward(re, fe);
  if (forward(fs, hue) <= forward(hue, fe)) {
    fs = hue + 1;
    rs = hue + 1 + shoulderIn;
  } else {
    fe = hue - 1;
    re = hue - 1 - shoulderOut;
  }
  return normalize([fs, rs, re, fe]);
}

/** Move one handle, keeping the four in order and the band under 350°; null if that would break the band. */
export function setHandle(band: Band, index: number, degrees: number): Band | null {
  const next = [...band] as Band;
  next[index] = wrap(Math.round(degrees));
  const span = forward(next[0], next[3]);
  const toStart = forward(next[0], next[1]);
  const toEnd = forward(next[0], next[2]);
  if (!(span > 1 && span <= 350 && toStart <= toEnd && toEnd <= span)) return null;
  return next;
}

/** Move the whole band by `delta` degrees. */
export const shift = (band: Band, delta: number): Band => band.map((v) => wrap(Math.round(v + delta))) as Band;

/**
 * The complement: full strength where the band had none. Photoshop's
 * Invert. The result is again a four-handle band (so it saves to PSD), and
 * its weight is exactly 1 − the old weight.
 */
export const invert = ([fs, rs, re, fe]: Band): Band => [re, fe, fs, rs];

/** Where on the circle the band's plateau is centred. */
export const middle = ([, rs, re]: Band) => wrap(rs + forward(rs, re) / 2);

/** Hue (0..360) and HSL saturation of a colour, or null for greys. */
export function hueOf(r: number, g: number, b: number): { hue: number; sat: number } | null {
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const mx = Math.max(R, G, B);
  const mn = Math.min(R, G, B);
  const d = mx - mn;
  const l = (mx + mn) / 2;
  const sat = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  if (sat <= 0.02) return null;
  let h = mx === R ? ((G - B) / d) % 6 : mx === G ? (B - R) / d + 2 : (R - G) / d + 4;
  h *= 60;
  return { hue: wrap(h), sat };
}

/** The range that claims `hue` most strongly. */
export function bestRange(bands: Band[], hue: number): number {
  let best = 0;
  let bestW = -1;
  bands.forEach((b, i) => {
    const w = weight(b, hue);
    if (w > bestW) {
      bestW = w;
      best = i;
    }
  });
  return best;
}

// ---- The Hue/Saturation model, for previews (same maths as adjust.rs)

type Rgb = [number, number, number];
interface Shift {
  hue: number;
  saturation: number;
  lightness: number;
}
export interface HueSatParams {
  master: Shift;
  ranges: Shift[];
  bands: Band[];
  colorize: boolean;
  colorize_hue: number;
  colorize_saturation: number;
  colorize_lightness: number;
}

function toHsl([r, g, b]: Rgb): Rgb {
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const l = (mx + mn) / 2;
  const d = mx - mn;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  let h = mx === r ? ((g - b) / d) % 6 : mx === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [wrap(h * 60), s, l];
}

function fromHsl([h, s, l]: Rgb): Rgb {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] = hp < 1 ? [c, x, 0] : hp < 2 ? [x, c, 0] : hp < 3 ? [0, c, x] : hp < 4 ? [0, x, c] : hp < 5 ? [x, 0, c] : [c, 0, x];
  return [r + m, g + m, b + m];
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const satAlpha = (inc: number) => (inc > 0 ? (inc >= 1 ? 1e6 : inc / (1 - inc)) : inc);

function saturate(c: Rgb, alpha: number): Rgb {
  if (!alpha) return c;
  const mx = Math.max(...c);
  const mn = Math.min(...c);
  const l = (mx + mn) / 2;
  const d = mx - mn;
  if (d <= 0) return c;
  const s = d / Math.max(1e-6, 1 - Math.abs(2 * l - 1));
  const a = Math.min(Math.max(alpha, -1), Math.max(0, 1 / Math.max(s, 1e-6) - 1));
  return c.map((v) => clamp01(v + (v - l) * a)) as Rgb;
}

const rotate = (c: Rgb, dh: number): Rgb => {
  if (!dh) return c;
  const [h, s, l] = toHsl(c);
  return fromHsl([wrap(h + dh), s, l]);
};

/** One colour (0..1 RGB) through Hue/Saturation. */
export function hueSatApply(p: HueSatParams, input: Rgb): Rgb {
  if (p.colorize) {
    const l0 = (Math.max(...input) + Math.min(...input)) / 2;
    const dl = Math.max(-1, Math.min(1, p.colorize_lightness / 100));
    const l = dl > 0 ? l0 + (1 - l0) * dl : l0 * (1 + dl);
    return fromHsl([wrap(p.colorize_hue), clamp01(p.colorize_saturation / 100), clamp01(l)]).map(clamp01) as Rgb;
  }
  let c = input;
  const [h, s] = toHsl(c);
  if (s > 0) {
    let dh = 0;
    let dl = 0;
    let alpha = 0;
    p.ranges.forEach((r, i) => {
      if (!r.hue && !r.saturation && !r.lightness) return;
      const w = weight(p.bands[i] ?? DEFAULT_BANDS[i], h);
      dh += r.hue * w;
      dl += (r.lightness / 100) * w;
      alpha += satAlpha(r.saturation / 100) * w;
    });
    c = rotate(c, dh);
    if (dl) {
      const L = Math.max(-1, Math.min(1, dl));
      const mx = Math.max(...c);
      const mn = Math.min(...c);
      c = c.map((v) => clamp01(L > 0 ? v + (mx - v) * L : v + (v - mn) * L)) as Rgb;
    }
    c = saturate(c, alpha);
  }
  c = rotate(c, p.master.hue);
  const ml = Math.max(-1, Math.min(1, p.master.lightness / 100));
  if (ml) c = c.map((v) => (ml > 0 ? v + (1 - v) * ml : v * (1 + ml))) as Rgb;
  return saturate(c, satAlpha(p.master.saturation / 100)).map(clamp01) as Rgb;
}

/** A CSS gradient of fully saturated hues from `start` over 360°, optionally through the adjustment. */
export function spectrum(start: number, p?: HueSatParams, steps = 72): string {
  const stops: string[] = [];
  for (let i = 0; i <= steps; i++) {
    const hue = wrap(start + (360 * i) / steps);
    let c = fromHsl([hue, 1, 0.5]);
    if (p) c = hueSatApply(p, c);
    stops.push(`rgb(${Math.round(c[0] * 255)} ${Math.round(c[1] * 255)} ${Math.round(c[2] * 255)}) ${((100 * i) / steps).toFixed(2)}%`);
  }
  return `linear-gradient(to right, ${stops.join(", ")})`;
}
