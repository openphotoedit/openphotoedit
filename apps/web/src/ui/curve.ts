// The engine's curve interpolation (Fritsch–Carlson monotone cubic, see
// crates/editor-core/src/adjust.rs `CurvePoints::lut`), so what the editor
// draws is exactly what the renderer applies.

export type CurvePoint = [number, number];

export function curveSampler(points: CurvePoint[]): (x: number) => number {
  const pts = [...points].map(([x, y]) => [x / 255, y / 255] as [number, number]).sort((a, b) => a[0] - b[0]);
  const dedup: [number, number][] = [];
  for (const p of pts) if (!dedup.length || Math.abs(dedup[dedup.length - 1][0] - p[0]) >= 1e-6) dedup.push(p);
  const n = dedup.length;
  if (n < 2) return (x) => x;
  const d = new Array<number>(n - 1);
  for (let i = 0; i < n - 1; i++) d[i] = (dedup[i + 1][1] - dedup[i][1]) / (dedup[i + 1][0] - dedup[i][0]);
  const m = new Array<number>(n).fill(0);
  m[0] = d[0];
  m[n - 1] = d[n - 2];
  for (let i = 1; i < n - 1; i++) m[i] = d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2;
  for (let i = 0; i < n - 1; i++) {
    if (Math.abs(d[i]) < 1e-9) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / d[i];
    const b = m[i + 1] / d[i];
    const s = a * a + b * b;
    if (s > 9) {
      const t = 3 / Math.sqrt(s);
      m[i] = t * a * d[i];
      m[i + 1] = t * b * d[i];
    }
  }
  return (xx: number) => {
    const x = xx / 255;
    if (x <= dedup[0][0]) return clamp01(dedup[0][1]) * 255;
    if (x >= dedup[n - 1][0]) return clamp01(dedup[n - 1][1]) * 255;
    let i = 0;
    while (i + 1 < n - 1 && x > dedup[i + 1][0]) i++;
    const h = dedup[i + 1][0] - dedup[i][0];
    const t = (x - dedup[i][0]) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    const y = (2 * t3 - 3 * t2 + 1) * dedup[i][1] + (t3 - 2 * t2 + t) * h * m[i] + (-2 * t3 + 3 * t2) * dedup[i + 1][1] + (t3 - t2) * h * m[i + 1];
    return clamp01(y) * 255;
  };
}

function clamp01(v: number) {
  return Math.min(1, Math.max(0, v));
}
