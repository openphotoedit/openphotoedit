<script lang="ts">
  // Find any Lite action by typing a few letters of what it does.
  import Search from "@lucide/svelte/icons/search";
  import CornerDownLeft from "@lucide/svelte/icons/corner-down-left";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { allActions, score } from "./actions";
  import { lite } from "./lite.svelte";

  let query = $state("");
  let index = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLUListElement>();

  const needsDoc = new Set(["open", "pro", "undo", "redo", "steps"]);
  const results = $derived.by(() => {
    const q = query;
    return allActions()
      .map((a) => ({ a, s: score(a, q) }))
      .filter((x) => x.s > 0)
      .sort((x, y) => y.s - x.s)
      .slice(0, q ? 12 : 10)
      .map((x) => x.a);
  });

  $effect(() => {
    if (lite.searchOpen) {
      query = "";
      index = 0;
      requestAnimationFrame(() => input?.focus());
    }
  });

  $effect(() => {
    void query;
    index = 0;
  });

  function run(i: number) {
    const a = results[i];
    if (!a) return;
    if (!editor.hasDocument && !needsDoc.has(a.id)) {
      editor.toast(t("Open a photo first."));
      return;
    }
    lite.searchOpen = false;
    void a.run();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      index = Math.min(results.length - 1, index + 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      index = Math.max(0, index - 1);
      e.preventDefault();
    } else if (e.key === "Enter") {
      run(index);
      e.preventDefault();
    } else if (e.key === "Escape") {
      lite.searchOpen = false;
      e.stopPropagation();
    }
    requestAnimationFrame(() => list?.querySelector<HTMLElement>(".active")?.scrollIntoView({ block: "nearest" }));
  }
</script>

{#if lite.searchOpen}
  <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && (lite.searchOpen = false)}>
    <div class="palette" role="dialog" aria-modal="true" aria-label={t("Search tools")} data-testid="tool-search">
      <div class="field">
        <Search size={18} />
        <input
          bind:this={input}
          bind:value={query}
          placeholder={t("Search tools and fixes")}
          aria-label={t("Search tools and fixes")}
          role="combobox"
          aria-expanded="true"
          aria-controls="lite-search-results"
          aria-activedescendant={results[index] ? `lite-search-${results[index].id}` : undefined}
          {onkeydown}
        />
        <kbd>Esc</kbd>
      </div>
      <ul class="results" id="lite-search-results" role="listbox" bind:this={list}>
        {#each results as a, i (a.id)}
          <li id="lite-search-{a.id}" role="option" aria-selected={i === index}>
            <button class:active={i === index} onpointermove={() => (index = i)} onclick={() => run(i)}>
              <span class="label">{a.label}</span>
              <span class="group">{a.group}</span>
              {#if i === index}<CornerDownLeft size={14} />{/if}
            </button>
          </li>
        {:else}
          <li class="none">{t("Nothing matches “{q}”. Try a word like brighter, crop or text.", { q: query })}</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 85;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding: 12vh var(--space-4) var(--space-4);
    background: var(--scrim);
    animation: fade var(--duration-fast) var(--ease-standard);
  }
  .palette {
    width: 100%;
    max-width: 560px;
    border-radius: var(--radius-xl);
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    box-shadow: var(--shadow-xl);
    overflow: hidden;
    animation: rise var(--duration-base) var(--ease-out);
  }
  .field {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-4);
    height: 56px;
    border-bottom: var(--border-width) solid var(--border-hairline);
    color: var(--text-muted);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    font: var(--type-body);
    font-size: var(--text-md);
    color: var(--text-strong);
    outline: none;
  }
  input:focus-visible {
    box-shadow: none;
  }
  kbd {
    font: var(--type-mono);
    font-size: var(--text-2xs);
    padding: 2px 6px;
    border-radius: var(--radius-xs);
    border: var(--border-width) solid var(--border-hairline);
    color: var(--text-faint);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: var(--space-2);
    max-height: min(420px, 60dvh);
    overflow-y: auto;
  }
  .results button {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: var(--touch-min);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
    text-align: left;
    cursor: pointer;
  }
  .results button.active {
    background: var(--bg-subtle);
    color: var(--text-strong);
  }
  .label {
    flex: 1;
    font: var(--type-ui);
    color: var(--text-strong);
  }
  .group {
    font: var(--type-caption);
    color: var(--text-faint);
  }
  .none {
    padding: var(--space-4);
    font: var(--type-caption);
    color: var(--text-muted);
  }
  @keyframes fade {
    from { opacity: 0; }
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(-6px); }
  }
  @media (max-width: 899px) {
    .scrim {
      padding-top: calc(var(--space-4) + var(--safe-top));
    }
  }
</style>
