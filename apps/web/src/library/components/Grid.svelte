<script lang="ts">
  // The virtualised thumbnail grid. Only the rows on screen (plus a margin)
  // exist in the DOM, so a 5,000-photo folder scrolls like a 50-photo one.
  import ImageOff from "@lucide/svelte/icons/image-off";
  import { t } from "../../lib/i18n";
  import { library } from "../store.svelte";
  import type { LibraryItem } from "../types";
  import Marks from "./Marks.svelte";
  import Badges from "./Badges.svelte";

  let { columns = $bindable(1), onopen }: { columns?: number; onopen: (item: LibraryItem) => void } = $props();

  const GAP = 6;
  const INFO_H = 26;
  const PAD = 12;

  let scroller: HTMLDivElement;
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  const cols = $derived(Math.max(1, Math.floor((width - PAD * 2 + GAP) / (library.settings.thumbSize + GAP))));
  const cellW = $derived(Math.floor((width - PAD * 2 - GAP * (cols - 1)) / cols));
  const rowH = $derived(cellW + INFO_H + GAP);
  const rows = $derived(Math.ceil(library.view.length / cols));
  const overscan = 2;
  const first = $derived(Math.max(0, Math.floor((scrollTop - PAD) / rowH) - overscan));
  const last = $derived(Math.min(rows, Math.ceil((scrollTop + height) / rowH) + overscan));
  const visible = $derived(library.view.slice(first * cols, last * cols));

  $effect(() => {
    columns = cols;
  });

  // Thumbnails for what is on screen, plus a screen ahead.
  $effect(() => {
    const ahead = Math.ceil(height / Math.max(1, rowH)) + 1;
    library.setVisible(library.view.slice(first * cols, Math.min(rows, last + ahead) * cols));
  });

  // Keep the focused photo in view as the keyboard moves it.
  $effect(() => {
    const i = library.focusIndex;
    if (i < 0 || !scroller) return;
    const top = PAD + Math.floor(i / cols) * rowH;
    const st = scroller.scrollTop;
    if (top < st) scroller.scrollTop = top - PAD;
    else if (top + rowH > st + height) scroller.scrollTop = top + rowH - height + PAD;
  });

  function click(e: MouseEvent, it: LibraryItem) {
    if (e.shiftKey) library.select(it.path, "range");
    else if (e.metaKey || e.ctrlKey) library.select(it.path, "toggle");
    else library.select(it.path);
  }
</script>

<div
  class="grid lib-scroll"
  bind:this={scroller}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = scroller.scrollTop)}
  data-testid="lib-grid"
  role="listbox"
  aria-label={t("Photos")}
  aria-multiselectable="true"
  tabindex="-1"
>
  <div class="spacer" style:height="{rows * rowH + PAD * 2}px">
    {#each visible as it, k (it.path)}
      {@const i = first * cols + k}
      {@const m = library.marksOf(it.path)}
      {@const thumb = library.thumbs.get(it.path)}
      {@const selected = library.selected.has(it.path)}
      <div
        class="cell"
        class:selected
        class:focused={library.focus === it.path}
        class:rejected={m.flag === -1}
        role="option"
        aria-selected={selected}
        tabindex="-1"
        data-testid="lib-cell"
        data-path={it.path}
        data-rating={m.rating}
        data-flag={m.flag}
        data-label={m.label}
        style:left="{PAD + (i % cols) * (cellW + GAP)}px"
        style:top="{PAD + Math.floor(i / cols) * rowH}px"
        style:width="{cellW}px"
        style:height="{cellW + INFO_H}px"
        onclick={(e) => click(e, it)}
        ondblclick={() => onopen(it)}
        onkeydown={() => {}}
      >
        <div class="frame" style:height="{cellW}px">
          {#if thumb}
            <img src={thumb} alt="" draggable="false" decoding="async" data-testid="lib-thumb" />
          {:else if library.thumbErrors.has(it.path)}
            <span class="placeholder"><ImageOff size={18} /><span>{it.kind === "raw" ? t("RAW") : it.ext.toUpperCase()}</span></span>
          {:else}
            <span class="placeholder loading"></span>
          {/if}
          {#if it.kind === "raw" || it.kind === "psd"}
            <span class="kind">{it.kind === "raw" ? t("RAW") : "PSD"}</span>
          {/if}
          <span class="badges"><Badges result={library.cull.get(it.path)} compact={cellW < 150} /></span>
        </div>
        <div class="info">
          <span class="name" title={it.path}>{it.name}</span>
          <Marks marks={m} size={10} />
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .grid {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
  }
  .spacer {
    position: relative;
  }
  .cell {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-sm);
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    overflow: hidden;
    cursor: default;
    contain: strict;
    transition: border-color var(--duration-fast) var(--ease-standard);
  }
  .cell:hover {
    border-color: var(--border-strong);
  }
  .cell.selected {
    background: var(--surface-selected);
    border-color: var(--border-strong);
  }
  .cell.focused {
    border-color: var(--border-focus);
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }
  .frame {
    position: relative;
    display: grid;
    place-items: center;
    padding: 6px;
    background: var(--bg-sunken);
  }
  .cell.selected .frame {
    background: transparent;
  }
  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
    box-shadow: var(--shadow-sm);
    user-select: none;
  }
  .rejected img,
  .rejected .placeholder {
    opacity: 0.35;
  }
  .placeholder {
    display: grid;
    place-items: center;
    gap: 4px;
    color: var(--text-faint);
    font: var(--type-caption);
    font-size: var(--text-2xs);
  }
  .placeholder.loading {
    width: 60%;
    height: 60%;
    border-radius: var(--radius-xs);
    background: var(--surface-hover);
  }
  .kind {
    position: absolute;
    right: 6px;
    top: 6px;
    height: 16px;
    padding: 0 4px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-xs);
    font: var(--weight-medium) var(--text-2xs) / 1 var(--font-mono);
    background: color-mix(in oklab, var(--surface-inverse) 72%, transparent);
    color: var(--text-inverse);
  }
  .badges {
    position: absolute;
    left: 6px;
    top: 6px;
    right: 40px;
    pointer-events: none;
  }
  .info {
    flex: 0 0 26px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    min-width: 0;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--type-caption);
    font-size: var(--text-2xs);
    color: var(--text-muted);
  }
</style>
