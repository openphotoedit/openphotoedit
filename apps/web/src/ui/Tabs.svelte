<script lang="ts">
  // A tab strip for panel groups. Arrow keys move between tabs; the
  // `trailing` snippet sits at the far end (a panel menu, a collapse chevron).
  import type { Snippet } from "svelte";

  let {
    tabs,
    active,
    label,
    testid,
    onselect,
    ondblclick,
    trailing,
  }: {
    tabs: { id: string; label: string }[];
    active: string;
    label: string;
    testid?: string;
    onselect: (id: string) => void;
    ondblclick?: () => void;
    trailing?: Snippet;
  } = $props();

  let list: HTMLDivElement;

  function key(e: KeyboardEvent, i: number) {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const j = (i + (e.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    onselect(tabs[j].id);
    list.querySelectorAll<HTMLButtonElement>("[role=tab]")[j]?.focus();
  }
</script>

<div class="ops-tabs" data-testid={testid}>
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="ops-tabs__list" role="tablist" aria-label={label} bind:this={list} ondblclick={ondblclick}>
    {#each tabs as tab, i (tab.id)}
      <button
        type="button"
        role="tab"
        class="ops-tabs__tab"
        aria-selected={tab.id === active}
        tabindex={tab.id === active ? 0 : -1}
        data-testid="tab-{tab.id}"
        onclick={() => onselect(tab.id)}
        onkeydown={(e) => key(e, i)}>{tab.label}</button
      >
    {/each}
  </div>
  {#if trailing}<div class="ops-tabs__trailing">{@render trailing()}</div>{/if}
</div>

<style>
  .ops-tabs {
    display: flex;
    align-items: stretch;
    height: 28px;
    flex: 0 0 auto;
    border-bottom: var(--border-width) solid var(--border-hairline);
    background: var(--bg-page);
  }
  .ops-tabs__list {
    flex: 1;
    display: flex;
    min-width: 0;
    overflow: hidden;
  }
  .ops-tabs__tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    padding: 0 10px;
    border: 0;
    background: transparent;
    font: var(--weight-medium) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-faint);
    white-space: nowrap;
    cursor: default;
    transition: var(--transition-control);
  }
  .ops-tabs__tab:hover {
    color: var(--text-body);
  }
  .ops-tabs__tab[aria-selected="true"] {
    color: var(--text-strong);
    background: var(--bg-subtle);
  }
  .ops-tabs__tab[aria-selected="true"]::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 1px;
    background: var(--bg-subtle);
  }
  .ops-tabs__tab:first-child[aria-selected="true"] {
    box-shadow: inset -1px 0 0 var(--border-hairline);
  }
  .ops-tabs__tab:not(:first-child)[aria-selected="true"] {
    box-shadow: inset 1px 0 0 var(--border-hairline), inset -1px 0 0 var(--border-hairline);
  }
  .ops-tabs__tab:focus-visible {
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }
  .ops-tabs__trailing {
    display: flex;
    align-items: center;
    padding: 0 4px;
  }
</style>
