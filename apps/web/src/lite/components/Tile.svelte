<script lang="ts">
  // A preview tile: the engine's rendering of a variant, its name, and a
  // selected state. Shimmers while the preview renders.
  import Check from "@lucide/svelte/icons/check";
  import type { Thumb } from "../previews.svelte";

  let {
    label,
    thumb,
    fallback = null,
    selected = false,
    loading = false,
    testid,
    onclick,
  }: {
    label: string;
    thumb: Thumb | undefined;
    fallback?: string | null;
    selected?: boolean;
    loading?: boolean;
    testid?: string;
    onclick: () => void;
  } = $props();

  const src = $derived(thumb?.url ?? fallback);
</script>

<button class="lt-tile" class:selected aria-pressed={selected} data-testid={testid} {onclick}>
  <span class="frame" class:lt-shimmer={!thumb && loading}>
    {#if src}
      <img {src} alt="" class:dim={!thumb && loading} draggable="false" />
    {/if}
    {#if selected}
      <span class="tick"><Check size={12} strokeWidth={3} /></span>
    {/if}
  </span>
  <span class="label">{label}</span>
</button>

<style>
  .lt-tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
    min-width: 0;
    text-align: center;
  }
  .frame {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 1;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--border-hairline);
    transition: box-shadow var(--duration-fast) var(--ease-standard);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: opacity var(--duration-base) var(--ease-standard);
  }
  img.dim {
    opacity: 0.35;
  }
  .lt-tile:hover .frame {
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  .selected .frame {
    box-shadow: 0 0 0 2px var(--bg-page), 0 0 0 4px var(--text-strong);
  }
  .tick {
    position: absolute;
    right: 6px;
    top: 6px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--text-strong);
    color: var(--bg-page);
  }
  .label {
    font: var(--type-caption);
    font-weight: var(--weight-medium);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .selected .label {
    color: var(--text-strong);
  }
</style>
