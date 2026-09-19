<script lang="ts">
  // Document tabs: name @ zoom (mode), a dot for unsaved changes, close on
  // hover, and a button for a new document. A tab is also a drop target for
  // rows dragged from the Layers panel (the panel runs the drag and copies
  // the layers; the tab only shows that it would receive them).
  import X from "@lucide/svelte/icons/x";
  import Plus from "@lucide/svelte/icons/plus";
  import { editor } from "../lib/editor.svelte";
  import { t } from "../lib/i18n";
  import { tooltip } from "../ui/tooltip";
  import { closeDocument } from "./actions.svelte";
  import { pro } from "./state.svelte";

  const zoomPct = (z: number) => `${z >= 1 ? Math.round(z * 100) : (z * 100).toFixed(z < 0.1 ? 1 : 0).replace(/\.0$/, "")}%`;

  function label(id: number) {
    if (id === editor.currentTab) return { name: editor.fileName, dirty: editor.dirty, has: editor.hasDocument, zoom: editor.view.zoom };
    const tab = editor.tabs.find((x) => x.id === id)!;
    return { name: tab.name, dirty: tab.dirty, has: tab.hasDocument, zoom: tab.view.zoom };
  }
</script>

{#if editor.hasDocument || editor.tabs.length > 1}
<div class="tabs" role="tablist" aria-label={t("Documents")} data-testid="doc-tabs">
  {#each editor.tabs as tab (tab.id)}
    {@const l = label(tab.id)}
    {@const current = tab.id === editor.currentTab}
    <div class="tab" class:current class:drop={editor.layerDropTab === tab.id} role="presentation" data-doc-id={tab.id}>
      <button
        type="button"
        role="tab"
        class="select"
        aria-selected={current}
        data-testid="doc-tab"
        onclick={() => editor.switchTab(tab.id)}
        onauxclick={(e) => {
          if (e.button === 1) void closeDocument(tab.id);
        }}
      >
        <span class="name">{l.has ? l.name : t("Untitled")}</span>
        {#if l.has}<span class="meta">@ {zoomPct(l.zoom)} (RGB/8)</span>{/if}
      </button>
      {#if l.dirty}<span class="dirty" aria-label={t("Unsaved changes")} data-testid="dirty-dot"></span>{/if}
      <button type="button" class="close" aria-label={t("Close {name}", { name: l.name })} use:tooltip={{ text: t("Close"), shortcut: current ? "Mod+W" : undefined }} onclick={() => closeDocument(tab.id)}>
        <X size={12} />
      </button>
    </div>
  {/each}
  <button type="button" class="new" aria-label={t("New document")} use:tooltip={{ text: t("New document"), shortcut: "Mod+N" }} onclick={() => pro.open({ kind: "new" })}>
    <Plus size={13} />
  </button>
</div>
{/if}

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    height: 30px;
    flex: 0 0 auto;
    background: var(--bg-page);
    border-bottom: var(--border-width) solid var(--border-hairline);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    max-width: 280px;
    min-width: 120px;
    padding-right: 4px;
    border-right: var(--border-width) solid var(--border-hairline);
    color: var(--text-muted);
  }
  .tab.current {
    background: var(--bg-subtle);
    color: var(--text-strong);
  }
  .tab.current::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 1px;
    background: var(--text-muted);
  }
  .select {
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 6px 0 12px;
    border: 0;
    background: transparent;
    color: inherit;
    font: var(--weight-regular) var(--text-xs) / 30px var(--font-sans);
    text-align: left;
    cursor: default;
    white-space: nowrap;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .dirty {
    width: 6px;
    height: 6px;
    margin-right: 4px;
    border-radius: 50%;
    background: var(--text-muted);
    flex: 0 0 auto;
  }
  .tab.drop {
    background: var(--surface-active);
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1px var(--text-strong);
  }
  .close,
  .new {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    align-self: center;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-faint);
    cursor: default;
  }
  .close {
    opacity: 0;
  }
  .tab:hover .close,
  .tab.current .close,
  .close:focus-visible {
    opacity: 1;
  }
  .close:hover,
  .new:hover {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .new {
    margin: 0 6px;
  }
</style>
