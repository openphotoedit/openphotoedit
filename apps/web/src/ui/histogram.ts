// Histogram data as `analyze.histogram` returns it, and a local fallback
// that bins straight RGBA pixels (used on a downscaled composite until the
// analysis command is available, and for anything already in memory).

export interface HistogramData {
  r: number[];
  g: number[];
  b: number[];
  l: number[];
}

export function histogramOf(rgba: ArrayLike<number>, stride = 1): HistogramData {
  const r = new Array<number>(256).fill(0);
  const g = new Array<number>(256).fill(0);
  const b = new Array<number>(256).fill(0);
  const l = new Array<number>(256).fill(0);
  const step = 4 * Math.max(1, stride);
  for (let i = 0; i + 3 < rgba.length; i += step) {
    if (rgba[i + 3] === 0) continue;
    const R = rgba[i];
    const G = rgba[i + 1];
    const B = rgba[i + 2];
    r[R]++;
    g[G]++;
    b[B]++;
    l[Math.min(255, Math.round(0.299 * R + 0.587 * G + 0.114 * B))]++;
  }
  return { r, g, b, l };
}
