<script lang="ts">
  // The wrapper every tool options component renders in. It lays controls
  // out in one wrapping row, which suits both Pro's options bar and Lite's
  // bottom sheet; controls take their height from --control-h-*.
  import type { Snippet } from "svelte";
  let { children, testid }: { children: Snippet; testid?: string } = $props();
</script>

<div class="ops-topt" data-testid={testid ?? "tool-options"}>
  {@render children()}
</div>

<style>
  .ops-topt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
    min-width: 0;
    font: var(--weight-regular) var(--text-sm) / 1.2 var(--font-sans);
    color: var(--text-body);
  }
  .ops-topt :global(.ops-topt__field) {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-h-sm);
    white-space: nowrap;
  }
  .ops-topt :global(.ops-topt__label) {
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .ops-topt :global(.ops-topt__sep) {
    width: 1px;
    align-self: stretch;
    background: var(--border-hairline);
  }
  .ops-topt :global(.ops-topt__num) {
    width: 4.2em;
    text-align: right;
    padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }
  .ops-topt :global(input[type="range"]) {
    width: 96px;
    accent-color: var(--text-strong);
  }
  .ops-topt :global(select.oa-input) {
    width: auto;
    padding-right: 4px;
  }
  .ops-topt :global(.ops-topt__seg) {
    display: inline-flex;
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .ops-topt :global(.ops-topt__seg button) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-width: var(--control-h-sm);
    height: var(--control-h-sm);
    padding: 0 8px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .ops-topt :global(.ops-topt__seg button + button) {
    border-left: var(--border-width) solid var(--border-hairline);
  }
  .ops-topt :global(.ops-topt__seg button[aria-pressed="true"]) {
    background: var(--surface-selected);
    color: var(--text-strong);
  }
  .ops-topt :global(.ops-topt__color) {
    width: var(--control-h-sm);
    height: var(--control-h-sm);
    padding: 2px;
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface-card);
    cursor: pointer;
  }
  @media (pointer: coarse) {
    .ops-topt :global(input[type="range"]) {
      width: 140px;
      height: var(--control-h-sm);
    }
  }
</style>
