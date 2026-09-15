<script lang="ts">
  // A layer or mask thumbnail, fetched once per layer revision.
  import { onDestroy } from "svelte";
  import { maskThumbnail, thumbnail } from "../engine.svelte";

  let { id, rev, mask = false, width = 34, height = 26, docRatio = 1 }: { id: number; rev: number; mask?: boolean; width?: number; height?: number; docRatio?: number } = $props();

  let canvas: HTMLCanvasElement;
  let alive = true;
  onDestroy(() => (alive = false));

  $effect(() => {
    const key = `${id}:${rev}:${mask}`;
    const p = mask ? maskThumbnail(id, rev, 64) : thumbnail(id, rev, 64);
    p.then((bmp) => {
      if (!alive || !canvas || key !== `${id}:${rev}:${mask}`) return;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      const g = canvas.getContext("2d")!;
      g.clearRect(0, 0, canvas.width, canvas.height);
      if (!bmp) return;
      const k = Math.min(canvas.width / bmp.width, canvas.height / bmp.height);
      const w = bmp.width * k;
      const h = bmp.height * k;
      g.imageSmoothingQuality = "high";
      g.drawImage(bmp, (canvas.width - w) / 2, (canvas.height - h) / 2, w, h);
    });
  });

  const box = $derived.by(() => {
    // The image area inside the thumbnail, letterboxed by the document's ratio.
    const r = docRatio || 1;
    const w = r >= width / height ? width : Math.round(height * r);
    const h = r >= width / height ? Math.round(width / r) : height;
    return { w, h };
  });
</script>

<span class="thumb" class:mask style:width="{width}px" style:height="{height}px">
  <span class="paper" style:width="{box.w}px" style:height="{box.h}px"></span>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
</span>

<style>
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    overflow: hidden;
  }
  .paper {
    position: absolute;
    background-color: #fff;
    background-image: conic-gradient(#cfcfcf 25%, transparent 0 50%, #cfcfcf 0 75%, transparent 0);
    background-size: 6px 6px;
  }
  .mask .paper {
    background: #000;
  }
  canvas {
    position: absolute;
    inset: 0;
  }
</style>
