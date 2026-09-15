<script lang="ts">
  // An interactive tone curve. Click to add a point, drag to move it, drag a
  // point off the graph (or press Delete) to remove it, arrow keys nudge the
  // selected point. Coordinates are 0..255 input → output.
  import { t } from "../lib/i18n";
  import type { HistogramData } from "./histogram";
  import { curveSampler, type CurvePoint } from "./curve";

  let {
    points,
    color = "neutral",
    histogram = null,
    histogramChannel = "luminosity",
    selected = $bindable(-1),
    testid,
    onchange,
  }: {
    points: CurvePoint[];
    color?: "neutral" | "red" | "green" | "blue";
    histogram?: HistogramData | null;
    histogramChannel?: "luminosity" | "red" | "green" | "blue";
    selected?: number;
    testid?: string;
    onchange: (points: CurvePoint[], final: boolean) => void;
  } = $props();

  let host: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let size = $state(0);
  let drag: { index: number; out: boolean } | null = null;

  const INSET = 6;
  const sorted = $derived([...points].sort((a, b) => a[0] - b[0]));

  function toPx(v: number) {
    return INSET + (v / 255) * (size - INSET * 2);
  }
  function fromPx(px: number) {
    return Math.min(255, Math.max(0, ((px - INSET) / Math.max(1, size - INSET * 2)) * 255));
  }

  function lineColor(style: CSSStyleDeclaration) {
    if (color === "red") return "rgb(240 90 90)";
    if (color === "green") return "rgb(90 210 120)";
    if (color === "blue") return "rgb(100 140 250)";
    return style.getPropertyValue("--text-strong").trim() || "#fff";
  }

  function draw() {
    if (!canvas || !size) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size * dpr);
    canvas.height = Math.round(size * dpr);
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, size, size);
    const style = getComputedStyle(host);
    const hair = style.getPropertyValue("--border-hairline").trim() || "rgba(255,255,255,0.1)";
    const strong = style.getPropertyValue("--border-strong").trim() || "rgba(255,255,255,0.2)";
    const inner = size - INSET * 2;

    if (histogram) {
      const arr = histogramChannel === "red" ? histogram.r : histogramChannel === "green" ? histogram.g : histogramChannel === "blue" ? histogram.b : histogram.l;
      let max = 1;
      for (let i = 1; i < 255; i++) max = Math.max(max, arr[i]);
      g.fillStyle = style.getPropertyValue("--surface-active").trim() || "rgba(255,255,255,0.1)";
      g.beginPath();
      g.moveTo(INSET, INSET + inner);
      for (let i = 0; i < 256; i++) g.lineTo(toPx(i), INSET + inner - Math.min(1, arr[i] / max) * inner);
      g.lineTo(INSET + inner, INSET + inner);
      g.fill();
    }

    g.strokeStyle = hair;
    g.lineWidth = 1;
    for (let k = 0; k <= 4; k++) {
      const p = Math.round(INSET + (k / 4) * inner) + 0.5;
      g.beginPath();
      g.moveTo(p, INSET);
      g.lineTo(p, INSET + inner);
      g.moveTo(INSET, p);
      g.lineTo(INSET + inner, p);
      g.stroke();
    }
    g.strokeStyle = strong;
    g.beginPath();
    g.moveTo(toPx(0), toPx(255));
    g.lineTo(toPx(255), toPx(0));
    g.stroke();

    const f = curveSampler(sorted);
    g.strokeStyle = lineColor(style);
    g.lineWidth = 1.5;
    g.beginPath();
    for (let px = 0; px <= inner; px++) {
      const x = (px / inner) * 255;
      const y = size - toPx(f(x));
      if (px === 0) g.moveTo(INSET + px, y);
      else g.lineTo(INSET + px, y);
    }
    g.stroke();

    sorted.forEach(([x, y], i) => {
      const cx = toPx(x);
      const cy = size - toPx(y);
      const isSel = points.indexOf(sorted[i]) === selected;
      g.fillStyle = isSel ? lineColor(style) : style.getPropertyValue("--bg-sunken").trim() || "#000";
      g.strokeStyle = lineColor(style);
      g.lineWidth = 1.25;
      g.beginPath();
      g.rect(cx - 3.5, cy - 3.5, 7, 7);
      g.fill();
      g.stroke();
    });
  }

  $effect(() => {
    void sorted;
    void selected;
    void histogram;
    void histogramChannel;
    void size;
    draw();
  });

  function local(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    return { px: e.clientX - r.left, py: e.clientY - r.top };
  }

  function hit(px: number, py: number) {
    let best = -1;
    let bestD = 9;
    points.forEach(([x, y], i) => {
      const d = Math.hypot(toPx(x) - px, size - toPx(y) - py);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    });
    return best;
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    host.focus();
    canvas.setPointerCapture(e.pointerId);
    const { px, py } = local(e);
    let i = hit(px, py);
    if (i < 0) {
      const x = Math.round(fromPx(px));
      const y = Math.round(fromPx(size - py));
      if (points.some((p) => Math.abs(p[0] - x) < 2)) return;
      const next = [...points, [x, y] as CurvePoint];
      i = next.length - 1;
      selected = i;
      onchange(next, false);
    } else {
      selected = i;
    }
    drag = { index: i, out: false };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const { px, py } = local(e);
    const pts = points.map((p) => [...p] as CurvePoint);
    const me = pts[drag.index];
    if (!me) return;
    const order = [...pts].sort((a, b) => a[0] - b[0]);
    const k = order.indexOf(me);
    const lo = k > 0 ? order[k - 1][0] + 1 : 0;
    const hi = k < order.length - 1 ? order[k + 1][0] - 1 : 255;
    const outside = px < -16 || py < -16 || px > size + 16 || py > size + 16;
    drag.out = outside && pts.length > 2 && k > 0 && k < order.length - 1;
    me[0] = Math.round(Math.min(hi, Math.max(lo, fromPx(px))));
    me[1] = Math.round(fromPx(size - py));
    onchange(pts, false);
  }

  function up() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.out) {
      const next = points.filter((_, i) => i !== d.index);
      selected = -1;
      onchange(next, true);
    } else {
      onchange(points, true);
    }
  }

  function key(e: KeyboardEvent) {
    if (selected < 0 || !points[selected]) return;
    const order = [...points].sort((a, b) => a[0] - b[0]);
    const k = order.indexOf(points[selected]);
    if ((e.key === "Delete" || e.key === "Backspace") && points.length > 2 && k > 0 && k < order.length - 1) {
      e.preventDefault();
      e.stopPropagation();
      const next = points.filter((_, i) => i !== selected);
      selected = -1;
      onchange(next, true);
      return;
    }
    const d = e.shiftKey ? 10 : 1;
    const delta: Record<string, [number, number]> = { ArrowLeft: [-d, 0], ArrowRight: [d, 0], ArrowUp: [0, d], ArrowDown: [0, -d] };
    const m = delta[e.key];
    if (!m) return;
    e.preventDefault();
    e.stopPropagation();
    const pts = points.map((p) => [...p] as CurvePoint);
    const me = pts[selected];
    const lo = k > 0 ? order[k - 1][0] + 1 : 0;
    const hi = k < order.length - 1 ? order[k + 1][0] - 1 : 255;
    me[0] = Math.min(hi, Math.max(lo, me[0] + m[0]));
    me[1] = Math.min(255, Math.max(0, me[1] + m[1]));
    onchange(pts, true);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="ops-curve"
  bind:this={host}
  bind:clientWidth={size}
  tabindex="0"
  role="application"
  aria-label={t("Curve editor. Click to add a point; arrow keys move the selected point; Delete removes it.")}
  data-testid={testid}
  onkeydown={key}
>
  <canvas
    bind:this={canvas}
    style:width="{size}px"
    style:height="{size}px"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  ></canvas>
</div>

<style>
  .ops-curve {
    position: relative;
    width: 100%;
    aspect-ratio: 1;
    background: var(--bg-sunken);
    border-radius: var(--radius-xs);
    outline: none;
  }
  .ops-curve:focus-visible {
    box-shadow: 0 0 0 1px var(--border-focus);
  }
  canvas {
    display: block;
    cursor: crosshair;
    touch-action: none;
  }
</style>
