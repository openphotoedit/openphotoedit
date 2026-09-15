<script lang="ts">
  // A colour chip over a checkerboard, so transparency reads.
  import type { Rgba8 } from "../engine/types";
  import { css, toHex } from "./color";
  import { tooltip } from "./tooltip";

  let {
    color,
    size = 20,
    label,
    selected = false,
    testid,
    onclick,
  }: { color: Rgba8 | null; size?: number; label?: string; selected?: boolean; testid?: string; onclick?: (e: MouseEvent) => void } = $props();

  const name = $derived(label ?? (color ? `#${toHex(color)}` : "None"));
</script>

{#if onclick}
  <button
    type="button"
    class="ops-swatch"
    class:selected
    style:width="{size}px"
    style:height="{size}px"
    aria-label={name}
    data-testid={testid}
    use:tooltip={name}
    {onclick}
  >
    <span class="chip" style:background={css(color)}></span>
    {#if !color}<span class="none"></span>{/if}
  </button>
{:else}
  <span class="ops-swatch" style:width="{size}px" style:height="{size}px" role="img" aria-label={name}>
    <span class="chip" style:background={css(color)}></span>
    {#if !color}<span class="none"></span>{/if}
  </span>
{/if}

<style>
  .ops-swatch {
    position: relative;
    display: inline-block;
    flex: 0 0 auto;
    padding: 0;
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-xs);
    overflow: hidden;
    background-color: #fff;
    background-image: conic-gradient(#d6d6d6 25%, transparent 0 50%, #d6d6d6 0 75%, transparent 0);
    background-size: 8px 8px;
    cursor: default;
  }
  button.ops-swatch {
    cursor: pointer;
  }
  button.ops-swatch:hover {
    border-color: var(--text-muted);
  }
  .ops-swatch.selected {
    box-shadow: 0 0 0 1px var(--bg-page), 0 0 0 2px var(--text-strong);
  }
  .chip {
    position: absolute;
    inset: 0;
  }
  .none {
    position: absolute;
    inset: 0;
    background: linear-gradient(to top right, transparent calc(50% - 1px), var(--danger-fg) 50%, transparent calc(50% + 1px));
  }
</style>
