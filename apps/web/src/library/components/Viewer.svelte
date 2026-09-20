<script lang="ts" module>
  export interface ViewerState {
    /** "fit" or 1 (one image pixel per device pixel). */
    zoom: "fit" | 1;
    /** Centre of the view as a fraction of the image. */
    cx: number;
    cy: number;
  }
</script>

<script lang="ts">
  // One photo, fitted or at 100%. Click toggles zoom at the pointer; drag
  // pans at 100%. The view state is bindable so Compare can sync two viewers.
  import { t } from "../../lib/i18n";
  import { library } from "../store.svelte";
  import type { LibraryItem } from "../types";

  let {
    item,
    view = $bindable({ zoom: "fit", cx: 0.5, cy: 0.5 }),
    active = false,
    label,
    onactivate,
    testid = "lib-viewer",
  }: { item: LibraryItem | null; view?: ViewerState; active?: boolean; label?: string; onactivate?: () => void; testid?: string } = $props();

  let box: HTMLDivElement;
  let bw = $state(0);
  let bh = $state(0);
  let natural = $state({ w: 0, h: 0 });
  let src = $state<string | null>(null);
  let embedded = $state(false);
  let failed = $state(false);
  let loading = $state(false);

  $effect(() => {
    const it = item;
    failed = false;
    natural = { w: 0, h: 0 };
    if (!it) {
      src = null;
      return;
    }
    // Show the thumbnail at once, then swap in the full image.
    src = library.thumbs.get(it.path) ?? null;
    library.want(it);
    loading = true;
    let cancelled = false;
    library
      .fullUrl(it)
      .then((r) => {
        if (cancelled) return;
        if (r) {
          src = r.url;
          embedded = r.embedded;
        } else failed = !src;
      })
      .catch(() => {
        if (!cancelled) failed = !src;
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const fitScale = $derived(natural.w && bw ? Math.min(bw / natural.w, bh / natural.h, 1) : 1);
  const scale = $derived(view.zoom === "fit" ? fitScale : 1 / dpr);
  const dw = $derived(natural.w * scale);
  const dh = $derived(natural.h * scale);
  const tx = $derived(view.zoom === "fit" || dw <= bw ? (bw - dw) / 2 : clamp(bw / 2 - view.cx * dw, bw - dw, 0));
  const ty = $derived(view.zoom === "fit" || dh <= bh ? (bh - dh) / 2 : clamp(bh / 2 - view.cy * dh, bh - dh, 0));

  function clamp(v: number, lo: number, hi: number) {
    return Math.max(lo, Math.min(hi, v));
  }

  let drag: { x: number; y: number; cx: number; cy: number; moved: boolean } | null = null;

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    onactivate?.();
    box.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, cx: view.cx, cy: view.cy, moved: false };
  }

  function move(e: PointerEvent) {
    if (!drag || view.zoom === "fit" || !dw) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (drag.moved) view = { zoom: 1, cx: clamp(drag.cx - dx / dw, 0, 1), cy: clamp(drag.cy - dy / dh, 0, 1) };
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    const moved = drag.moved;
    drag = null;
    if (moved || !natural.w) return;
    const r = box.getBoundingClientRect();
    if (view.zoom === "fit") {
      // Zoom to 100% around the clicked point.
      const px = (e.clientX - r.left - tx) / dw;
      const py = (e.clientY - r.top - ty) / dh;
      view = { zoom: 1, cx: clamp(px, 0, 1), cy: clamp(py, 0, 1) };
    } else view = { zoom: "fit", cx: 0.5, cy: 0.5 };
  }
</script>

<div
  class="viewer"
  class:active
  class:zoomed={view.zoom !== "fit"}
  bind:this={box}
  bind:clientWidth={bw}
  bind:clientHeight={bh}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  role="img"
  aria-label={item?.name ?? t("No photo")}
  data-testid={testid}
  data-zoom={view.zoom}
  data-path={item?.path}
>
  {#if src && !failed}
    <img
      {src}
      alt=""
      draggable="false"
      style:width="{dw}px"
      style:height="{dh}px"
      style:transform="translate({tx}px, {ty}px)"
      style:visibility={natural.w ? "visible" : "hidden"}
      onload={(e) => {
        const im = e.currentTarget as HTMLImageElement;
        natural = { w: im.naturalWidth, h: im.naturalHeight };
      }}
      onerror={() => (failed = true)}
    />
  {:else if item}
    <div class="empty">
      <strong>{item.kind === "raw" ? t("No preview in this raw file") : t("This browser cannot show this file")}</strong>
      <span>{t("Open it in the editor to work on it.")}</span>
    </div>
  {/if}
  {#if label}<span class="label">{label}</span>{/if}
  {#if embedded}<span class="note">{t("Embedded camera preview")}</span>{/if}
  {#if view.zoom !== "fit"}<span class="zoom">100%</span>{/if}
  {#if loading && !src}<span class="note">{t("Loading…")}</span>{/if}
</div>

<style>
  .viewer {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--bg-sunken);
    cursor: zoom-in;
    touch-action: none;
    user-select: none;
  }
  .viewer.zoomed {
    cursor: grab;
  }
  .viewer.zoomed:active {
    cursor: grabbing;
  }
  .viewer.active::after {
    content: "";
    position: absolute;
    inset: 0;
    box-shadow: inset 0 0 0 1px var(--border-focus);
    pointer-events: none;
  }
  img {
    position: absolute;
    left: 0;
    top: 0;
    max-width: none;
    transform-origin: 0 0;
    box-shadow: var(--shadow-md);
    image-rendering: auto;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    gap: var(--space-1);
    text-align: center;
    color: var(--text-muted);
    font: var(--type-caption);
  }
  .empty strong {
    color: var(--text-strong);
    font-weight: var(--weight-medium);
  }
  .label,
  .zoom,
  .note {
    position: absolute;
    height: 20px;
    padding: 0 6px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-xs);
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-sans);
    background: color-mix(in oklab, var(--surface-inverse) 72%, transparent);
    color: var(--text-inverse);
    pointer-events: none;
  }
  .label {
    left: 8px;
    top: 8px;
  }
  .zoom {
    right: 8px;
    top: 8px;
    font-family: var(--font-mono);
  }
  .note {
    right: 8px;
    bottom: 8px;
  }
</style>
