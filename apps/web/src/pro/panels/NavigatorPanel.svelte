<script lang="ts">
  // Navigator: a thumbnail of the composite with the visible area outlined;
  // drag it to pan. The slider zooms on a log scale.
  import ZoomIn from "@lucide/svelte/icons/zoom-in";
  import ZoomOut from "@lucide/svelte/icons/zoom-out";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import IconButton from "../../ui/IconButton.svelte";
  import NumberField from "../../ui/NumberField.svelte";
  import { thumbnail } from "../engine.svelte";

  let box: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();
  let bw = $state(240);
  const BH = 150;
  let timer = 0;

  const s = $derived(editor.summary);
  const fit = $derived.by(() => {
    if (!s) return { k: 1, w: 0, h: 0, x: 0, y: 0 };
    const k = Math.min(bw / s.width, BH / s.height);
    const w = s.width * k;
    const h = s.height * k;
    return { k, w, h, x: (bw - w) / 2, y: (BH - h) / 2 };
  });

  $effect(() => {
    const rev = editor.summary?.revision ?? 0;
    const has = editor.hasDocument;
    void bw;
    clearTimeout(timer);
    if (!has) return;
    timer = window.setTimeout(async () => {
      const bmp = await thumbnail(null, rev, 256);
      if (!bmp || !canvas) return;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(fit.w * dpr);
      canvas.height = Math.round(fit.h * dpr);
      const g = canvas.getContext("2d")!;
      g.imageSmoothingQuality = "high";
      g.drawImage(bmp, 0, 0, canvas.width, canvas.height);
    }, 250);
    return () => clearTimeout(timer);
  });

  const rect = $derived.by(() => {
    const tl = editor.toDoc(0, 0);
    const br = editor.toDoc(editor.viewport.width, editor.viewport.height);
    const x0 = Math.max(0, tl.x) * fit.k + fit.x;
    const y0 = Math.max(0, tl.y) * fit.k + fit.y;
    const x1 = Math.min(s?.width ?? 0, br.x) * fit.k + fit.x;
    const y1 = Math.min(s?.height ?? 0, br.y) * fit.k + fit.y;
    return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
  });

  function pan(e: PointerEvent) {
    const r = box.getBoundingClientRect();
    const cx = (e.clientX - r.left - fit.x) / fit.k;
    const cy = (e.clientY - r.top - fit.y) / fit.k;
    editor.view = { ...editor.view, cx, cy };
  }

  const MIN = Math.log(0.01);
  const MAX = Math.log(32);
</script>

<div class="panel" data-testid="navigator-panel">
  <div
    class="box"
    bind:this={box}
    bind:clientWidth={bw}
    style:height="{BH}px"
    role="slider"
    tabindex="0"
    aria-label={t("Visible area. Drag to pan.")}
    aria-valuenow={Math.round(editor.view.cx)}
    onpointerdown={(e) => {
      if (!editor.hasDocument) return;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      pan(e);
    }}
    onpointermove={(e) => {
      if (e.buttons === 1 && editor.hasDocument) pan(e);
    }}
  >
    {#if editor.hasDocument}
      <canvas bind:this={canvas} style:left="{fit.x}px" style:top="{fit.y}px" style:width="{fit.w}px" style:height="{fit.h}px"></canvas>
      <span class="view" style:left="{rect.x}px" style:top="{rect.y}px" style:width="{rect.w}px" style:height="{rect.h}px"></span>
    {/if}
  </div>
  <div class="ops-row">
    <NumberField value={Math.round(editor.view.zoom * 1000) / 10} min={1} max={3200} unit="%" width={60} precision={editor.view.zoom < 1 ? 1 : 0} ariaLabel={t("Zoom")} disabled={!editor.hasDocument} onchange={(v) => editor.zoomAt(v / 100)} />
    <IconButton size="xs" label={t("Zoom Out")} disabled={!editor.hasDocument} onclick={() => editor.zoomStep(-1)}><ZoomOut size={12} /></IconButton>
    <input
      class="zoom"
      type="range"
      min="0"
      max="1000"
      aria-label={t("Zoom")}
      disabled={!editor.hasDocument}
      value={Math.round(((Math.log(editor.view.zoom) - MIN) / (MAX - MIN)) * 1000)}
      oninput={(e) => editor.zoomAt(Math.exp(MIN + (Number(e.currentTarget.value) / 1000) * (MAX - MIN)))}
    />
    <IconButton size="xs" label={t("Zoom In")} disabled={!editor.hasDocument} onclick={() => editor.zoomStep(1)}><ZoomIn size={12} /></IconButton>
  </div>
</div>

<style>
  .panel {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .box {
    position: relative;
    background: var(--bg-sunken);
    border-radius: var(--radius-xs);
    overflow: hidden;
    cursor: move;
    touch-action: none;
  }
  canvas {
    position: absolute;
  }
  .view {
    position: absolute;
    border: 1px solid rgb(255 60 60);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.4);
    pointer-events: none;
  }
  .zoom {
    flex: 1;
    min-width: 0;
    accent-color: var(--text-strong);
    height: 14px;
  }
</style>
