<script lang="ts">
  // History: the document's opening state, every undoable step, and the
  // steps undone (dimmed). Click one to go there.
  import Image from "@lucide/svelte/icons/image";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import Redo2 from "@lucide/svelte/icons/redo-2";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import IconButton from "../../ui/IconButton.svelte";
  import { run } from "../engine.svelte";

  const undo = $derived(editor.summary?.history.undo ?? []);
  const redo = $derived(editor.summary?.history.redo ?? []);
  let list: HTMLDivElement;

  function go(index: number) {
    void run({ op: "edit.history-go", index }, undefined, t("History"));
  }

  $effect(() => {
    void undo.length;
    queueMicrotask(() => list?.querySelector(".current")?.scrollIntoView({ block: "nearest" }));
  });
</script>

<div class="panel" data-testid="history-panel">
  <div class="list" bind:this={list} role="listbox" aria-label={t("History")} tabindex="0"
    onkeydown={(e) => {
      if (e.key === "ArrowUp" && undo.length) {
        e.preventDefault();
        go(undo.length - 1);
      } else if (e.key === "ArrowDown" && redo.length) {
        e.preventDefault();
        go(undo.length + 1);
      }
    }}>
    {#if editor.hasDocument}
      <div class="row snapshot" class:current={undo.length === 0} role="option" tabindex="-1" aria-selected={undo.length === 0} data-testid="history-row" onclick={() => go(0)} onkeydown={() => undefined}>
        <span class="icon"><Image size={13} /></span>
        <span class="label">{editor.fileName}</span>
      </div>
      {#each undo as label, i (i)}
        <div class="row" class:current={i === undo.length - 1} role="option" tabindex="-1" aria-selected={i === undo.length - 1} data-testid="history-row" onclick={() => go(i + 1)} onkeydown={() => undefined}>
          <span class="icon dot"></span>
          <span class="label">{label}</span>
        </div>
      {/each}
      {#each redo as label, j (j)}
        <div class="row undone" role="option" tabindex="-1" aria-selected="false" data-testid="history-row" onclick={() => go(undo.length + j + 1)} onkeydown={() => undefined}>
          <span class="icon dot"></span>
          <span class="label">{label}</span>
        </div>
      {/each}
    {:else}
      <p class="ops-note empty">{t("Steps appear here as you edit.")}</p>
    {/if}
  </div>
  <div class="bar">
    <span class="count">{t("{n} steps", { n: undo.length })}</span>
    <IconButton size="sm" label={t("Step Backward")} shortcut="Mod+Z" disabled={!undo.length} onclick={() => editor.undo()}><Undo2 size={14} /></IconButton>
    <IconButton size="sm" label={t("Step Forward")} shortcut="Shift+Mod+Z" disabled={!redo.length} onclick={() => editor.redo()}><Redo2 size={14} /></IconButton>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    outline: none;
    padding: 4px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 10px;
    font: var(--weight-regular) var(--text-xs) / 1 var(--font-sans);
    color: var(--text-body);
    cursor: default;
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row.current {
    background: var(--surface-active);
    color: var(--text-strong);
  }
  .row.undone {
    color: var(--text-faint);
  }
  .snapshot {
    height: 32px;
    border-bottom: var(--border-width) solid var(--border-hairline);
    margin-bottom: 2px;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 16px;
    color: var(--text-muted);
  }
  .dot::before {
    content: "";
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: currentColor;
    opacity: 0.6;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    padding: var(--space-4);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 28px;
    padding: 0 6px 0 10px;
    border-top: var(--border-width) solid var(--border-hairline);
  }
  .count {
    flex: 1;
    font: var(--type-caption);
    color: var(--text-faint);
  }
</style>
