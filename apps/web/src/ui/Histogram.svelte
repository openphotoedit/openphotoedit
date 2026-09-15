<script lang="ts">
  // A 256-bin histogram drawn on a canvas. `mode` picks the channel; "colors"
  // overlays R, G and B additively as Photoshop's Histogram panel does.
  import type { Snippet } from "svelte";
  import type { HistogramData } from "./histogram";

  let {
    data,
    mode = "rgb",
    height = 90,
    testid,
    children,
  }: {
    data: HistogramData | null;
    mode?: "rgb" | "colors" | "luminosity" | "red" | "green" | "blue";
    height?: number;
    testid?: string;
    children?: Snippet;
  } = $props();

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let width = $state(0);

  function peak(arr: number[]) {
    // Ignore the two end bins, where clipping piles up, when scaling.
    let m = 1;
    for (let i = 1; i < 255; i++) m = Math.max(m, arr[i]);
    return m;
  }

  function draw() {
    if (!canvas || !width) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    if (!data) return;
    const style = getComputedStyle(host);
    const muted = style.getPropertyValue("--text-muted").trim() || "rgba(255,255,255,0.6)";
    const plot = (arr: number[], color: string, max: number, op: GlobalCompositeOperation = "source-over") => {
      g.globalCompositeOperation = op;
      g.fillStyle = color;
      g.beginPath();
      g.moveTo(0, height);
      for (let i = 0; i < 256; i++) {
        const x = (i / 255) * width;
        const v = Math.min(1, arr[i] / max);
        g.lineTo(x, height - v * (height - 2));
      }
      g.lineTo(width, height);
      g.closePath();
      g.fill();
    };
    if (mode === "colors") {
      const max = Math.max(peak(data.r), peak(data.g), peak(data.b));
      plot(data.r, "rgb(255 60 60 / 0.85)", max, "lighter");
      plot(data.g, "rgb(60 220 90 / 0.85)", max, "lighter");
      plot(data.b, "rgb(70 120 255 / 0.85)", max, "lighter");
    } else if (mode === "red") plot(data.r, "rgb(235 80 80)", peak(data.r));
    else if (mode === "green") plot(data.g, "rgb(80 200 110)", peak(data.g));
    else if (mode === "blue") plot(data.b, "rgb(90 130 245)", peak(data.b));
    else {
      const arr = mode === "luminosity" ? data.l : data.l.map((_, i) => (data.r[i] + data.g[i] + data.b[i]) / 3);
      plot(arr, muted, peak(arr));
    }
    g.globalCompositeOperation = "source-over";
  }

  $effect(() => {
    void data;
    void mode;
    void width;
    void height;
    draw();
  });
</script>

<div class="ops-histogram" bind:this={host} bind:clientWidth={width} style:height="{height}px" data-testid={testid}>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
  {#if children}{@render children()}{/if}
</div>

<style>
  .ops-histogram {
    position: relative;
    width: 100%;
    background: var(--bg-sunken);
    border-radius: var(--radius-xs);
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    display: block;
  }
</style>
