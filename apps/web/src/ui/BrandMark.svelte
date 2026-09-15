<script lang="ts">
  // Ported from openpdfedit's BrandMark (itself a port of the design
  // system's BrandMark.jsx): muted "Open", the product word, and the full
  // stop in the accent. Tiles carry the glyph and never the dot.
  import Layers from "@lucide/svelte/icons/layers";
  import { PRODUCT_PREFIX, PRODUCT_WORD, PRODUCT_NAME } from "../lib/brand";

  let { variant = "wordmark", size = 18 }: { variant?: "wordmark" | "monogram" | "lockup"; size?: number } = $props();
</script>

<span class="oa-brandmark" role="img" aria-label={PRODUCT_NAME}>
  {#if variant !== "wordmark"}
    <span class="tile" style="width: {size}px; height: {size}px;">
      <Layers size={Math.round(size * 0.55)} />
    </span>
  {/if}
  {#if variant !== "monogram"}
    <span class="word" style="font-size: {variant === 'lockup' ? size * 0.56 : size}px;">
      <span class="prefix">{PRODUCT_PREFIX}</span>{PRODUCT_WORD}<span class="dot">.</span>
    </span>
  {/if}
</span>

<style>
  .oa-brandmark {
    display: inline-flex;
    align-items: center;
    gap: 0.4em;
  }
  .tile {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    border-radius: var(--logo-tile-radius);
    background: var(--logo-tile-bg);
    color: var(--logo-tile-fg);
  }
  .word {
    font: var(--weight-medium) 1em/1 var(--font-display);
    letter-spacing: var(--logo-tracking);
    color: var(--logo-fg);
    white-space: nowrap;
  }
  .prefix {
    color: var(--text-muted);
  }
  .dot {
    color: var(--accent);
  }
</style>
